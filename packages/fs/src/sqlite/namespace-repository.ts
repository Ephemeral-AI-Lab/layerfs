import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  MAINTENANCE_TOTAL_EMERGENCY_BYTES,
  type FilesystemLimits,
  type StorageLimits,
} from "../resources/limits.js";
import { canonicalizePath, type CanonicalPath } from "../namespace/paths.js";
import { fsError } from "../filesystem/errors.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { bytesToHex, intrinsicByteLength } from "../cas/bytes.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";
import { validateDurableIdentifier } from "./identifiers.js";

export interface InodeRow extends SqliteRow {
  id: string;
  type: number;
  mode: number;
  birthtime_ms: number;
  mtime_ms: number;
  ctime_ms: number;
  nlink: number;
  size: number | null;
  manifest_hash: Uint8Array | null;
  symlink_target: string | null;
  token: number;
}
export interface EntryRow extends SqliteRow {
  parent_inode: string;
  name_sort: Uint8Array;
  name: string | null;
  inode_id: string | null;
  token: number;
}
export interface ChildRow extends SqliteRow {
  name: string;
  name_sort: Uint8Array;
  inode_id: string;
  token: number;
  type: number;
}
interface MetaRow extends SqliteRow {
  root_inode: string;
  main_revision: number;
  root_mutation_generation: number;
}
export interface ResolvedPath {
  readonly path: CanonicalPath;
  readonly inode: InodeRow;
  readonly parentInode: string | null;
  readonly name: string;
  readonly nameSort: Uint8Array | null;
  readonly entryToken: number | null;
}

export class NamespaceRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: FilesystemLimits;
  readonly #storage: StorageLimits;
  readonly #syscall: string;
  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: FilesystemLimits,
    storage: StorageLimits,
    syscall: string,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#storage = storage;
    this.#syscall = syscall;
  }
  meta(): MetaRow {
    const row = this.#tx.all<MetaRow>(
      "SELECT root_inode,main_revision,root_mutation_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) throw new Error("ECORRUPT: missing filesystem metadata");
    return row;
  }
  inode(id: string): InodeRow | undefined {
    return this.#tx.all<InodeRow>(
      "SELECT id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token FROM efs_inodes WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: this.#limits.maxSymlinkTargetBytes * 2 + 2048 },
    )[0];
  }
  entry(parentInode: string, nameSort: Uint8Array): EntryRow | undefined {
    return this.#tx.all<EntryRow>(
      "SELECT parent_inode,name_sort,name,inode_id,token FROM efs_entries WHERE parent_inode=? AND name_sort=?",
      [parentInode, nameSort],
      { maxRows: 1, maxBytes: 8192 },
    )[0];
  }
  resolve(input: string | CanonicalPath, followFinal = true): ResolvedPath {
    let path =
      typeof input === "string"
        ? canonicalizePath(input, this.#limits, this.#syscall)
        : input;
    let traversals = 0;
    restart: while (true) {
      const meta = this.meta();
      const root = this.inode(meta.root_inode);
      if (!root || root.type !== 1)
        throw new Error("ECORRUPT: root inode is missing or not a directory");
      if (!path.segments.length)
        return Object.freeze({
          path,
          inode: root,
          parentInode: null,
          name: "",
          nameSort: null,
          entryToken: null,
        });
      let parent = root;
      for (let index = 0; index < path.segments.length; index += 1) {
        const name = path.segments[index]!;
        const nameSort = path.encodedSegments[index]!;
        const entry = this.entry(parent.id, nameSort);
        if (!entry?.inode_id || entry.name !== name)
          throw fsError("ENOENT", this.#syscall, path.value, "path does not exist");
        const inode = this.inode(entry.inode_id);
        if (!inode)
          throw new Error("ECORRUPT: live directory entry references missing inode");
        const final = index === path.segments.length - 1;
        if (inode.type === 2 && (!final || followFinal)) {
          traversals += 1;
          if (traversals > this.#limits.maxSymlinkTraversals)
            throw fsError(
              "ELOOP",
              this.#syscall,
              path.value,
              "too many symbolic link traversals",
            );
          const target = inode.symlink_target;
          if (!target) throw new Error("ECORRUPT: symbolic link target is missing");
          const remaining = path.segments.slice(index + 1).join("/");
          const base = path.segments.slice(0, index).join("/");
          const expanded = target.startsWith("/")
            ? `${target}${remaining ? `/${remaining}` : ""}`
            : `/${base}${base ? "/" : ""}${target}${remaining ? `/${remaining}` : ""}`;
          path = canonicalizePath(expanded, this.#limits, this.#syscall);
          continue restart;
        }
        if (!final && inode.type !== 1)
          throw fsError(
            "ENOTDIR",
            this.#syscall,
            path.value,
            "intermediate path component is not a directory",
          );
        if (final)
          return Object.freeze({
            path,
            inode,
            parentInode: parent.id,
            name,
            nameSort,
            entryToken: entry.token,
          });
        parent = inode;
      }
    }
  }
  resolveOptional(
    input: string | CanonicalPath,
    followFinal = true,
  ): ResolvedPath | undefined {
    try {
      return this.resolve(input, followFinal);
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "ENOENT")
        return undefined;
      throw error;
    }
  }
  resolveParent(path: CanonicalPath): {
    readonly parent: ResolvedPath;
    readonly name: string;
    readonly nameSort: Uint8Array;
  } {
    if (!path.segments.length)
      throw fsError(
        "EPERM",
        this.#syscall,
        path.value,
        "operation is not permitted on root",
      );
    const name = path.segments.at(-1)!;
    const parentPath =
      path.segments.length === 1 ? "/" : `/${path.segments.slice(0, -1).join("/")}`;
    const parent = this.resolve(parentPath, true);
    if (parent.inode.type !== 1)
      throw fsError("ENOTDIR", this.#syscall, parentPath, "parent is not a directory");
    return Object.freeze({ parent, name, nameSort: encodeUtf8(name) });
  }
  nextRevision(now: number, changeCount: number, writer = "filesystem"): number {
    validateDurableIdentifier(writer, "revision writer identifier");
    const meta = this.meta();
    const revision = meta.main_revision + 1;
    const generation = meta.root_mutation_generation + 1;
    if (!Number.isSafeInteger(revision) || !Number.isSafeInteger(generation))
      throw new Error("ENOSPC: revision or generation space exhausted");
    const rootId = encodeUtf8(String(revision));
    const writerBytes = intrinsicByteLength(encodeUtf8(writer));
    new UsageRepository(this.#tx, this.#storage).apply(
      {
        maintenance_bytes: CHARGED_ROW_BYTES + intrinsicByteLength(rootId),
        charged_metadata_bytes: CHARGED_ROW_BYTES + writerBytes,
      },
      "namespace root journal",
      { preserveMaintenanceBytes: MAINTENANCE_TOTAL_EMERGENCY_BYTES },
    );
    this.#tx.run(
      "INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(?,?,?,?,?)",
      [revision, meta.main_revision, now, writer, changeCount],
    );
    this.#tx.run(
      "UPDATE efs_meta SET main_revision=?,root_mutation_generation=? WHERE singleton=1 AND main_revision=? AND root_mutation_generation=?",
      [revision, generation, meta.main_revision, meta.root_mutation_generation],
    );
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,0,?)",
      [generation, rootId],
    );
    return revision;
  }
  recordInode(revision: number, inodeId: string, tombstone = false): void {
    validateDurableIdentifier(inodeId, "inode identifier");
    const inode = tombstone ? undefined : this.inode(inodeId);
    const encoded = inode
      ? encodeUtf8(
          JSON.stringify({
            ...inode,
            manifest_hash: inode.manifest_hash ? bytesToHex(inode.manifest_hash) : null,
          }),
        )
      : null;
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT coalesce(length(encoded),0) bytes FROM efs_inode_revisions WHERE revision=? AND inode_id=?",
      [revision, inodeId],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    const priorRoots = this.#tx.all<{ count: number } & SqliteRow>(
      "SELECT count(*) count FROM efs_revision_manifest_roots WHERE revision=? AND inode_id=?",
      [revision, inodeId],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.count;
    const encodedBytes = encoded ? intrinsicByteLength(encoded) : 0;
    this.#changeMetadata(
      (prior ? 0 : CHARGED_ROW_BYTES) +
        encodedBytes -
        (prior?.bytes ?? 0) +
        (inode?.manifest_hash ? CHARGED_ROW_BYTES : 0) -
        (priorRoots ?? 0) * CHARGED_ROW_BYTES,
      "inode revision metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(?,?,?,?) ON CONFLICT(revision,inode_id) DO UPDATE SET tombstone=excluded.tombstone,encoded=excluded.encoded",
      [revision, inodeId, tombstone ? 1 : 0, encoded],
    );
    this.#tx.run(
      "DELETE FROM efs_revision_manifest_roots WHERE revision=? AND inode_id=?",
      [revision, inodeId],
    );
    if (inode?.manifest_hash)
      this.#tx.run(
        "INSERT INTO efs_revision_manifest_roots(revision,inode_id,manifest_hash) VALUES(?,?,?)",
        [revision, inodeId, inode.manifest_hash],
      );
  }
  recordEntry(
    revision: number,
    parentInode: string,
    nameSort: Uint8Array,
    tombstone = false,
  ): void {
    validateDurableIdentifier(parentInode, "parent inode identifier");
    const entry = tombstone ? undefined : this.entry(parentInode, nameSort);
    const encoded = entry
      ? encodeUtf8(JSON.stringify({ ...entry, name_sort: bytesToHex(entry.name_sort) }))
      : null;
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(name_sort)+coalesce(length(encoded),0) bytes FROM efs_entry_revisions WHERE revision=? AND parent_inode=? AND name_sort=?",
      [revision, parentInode, nameSort],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    const variableBytes =
      intrinsicByteLength(nameSort) + (encoded ? intrinsicByteLength(encoded) : 0);
    this.#changeMetadata(
      (prior ? 0 : CHARGED_ROW_BYTES) + variableBytes - (prior?.bytes ?? 0),
      "entry revision metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_entry_revisions(revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,?,?) ON CONFLICT(revision,parent_inode,name_sort) DO UPDATE SET tombstone=excluded.tombstone,encoded=excluded.encoded",
      [revision, parentInode, nameSort, tombstone ? 1 : 0, encoded],
    );
  }
  putEntry(
    parentInode: string,
    nameSort: Uint8Array,
    name: string | null,
    inodeId: string | null,
    token: number,
  ): void {
    validateDurableIdentifier(parentInode, "parent inode identifier");
    if (inodeId !== null) validateDurableIdentifier(inodeId, "inode identifier");
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(name_sort)+coalesce(length(CAST(name AS BLOB)),0) bytes FROM efs_entries WHERE parent_inode=? AND name_sort=?",
      [parentInode, nameSort],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    const variableBytes =
      intrinsicByteLength(nameSort) +
      (name === null ? 0 : intrinsicByteLength(encodeUtf8(name)));
    this.#changeMetadata(
      (prior ? 0 : CHARGED_ROW_BYTES) + variableBytes - (prior?.bytes ?? 0),
      "namespace entry metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_entries(parent_inode,name_sort,name,inode_id,token) VALUES(?,?,?,?,?) ON CONFLICT(parent_inode,name_sort) DO UPDATE SET name=excluded.name,inode_id=excluded.inode_id,token=excluded.token",
      [parentInode, nameSort, name, inodeId, token],
    );
  }
  children(
    parentInode: string,
    limit: number,
    maxBytes: number,
    startAfter?: Uint8Array,
  ): readonly ChildRow[] {
    return startAfter
      ? this.#tx.all<ChildRow>(
          "SELECT e.name,e.name_sort,e.inode_id,e.token,i.type FROM efs_entries e JOIN efs_inodes i ON i.id=e.inode_id WHERE e.parent_inode=? AND e.inode_id IS NOT NULL AND e.name_sort>? ORDER BY e.name_sort LIMIT ?",
          [parentInode, startAfter, limit],
          { maxRows: limit, maxBytes },
        )
      : this.#tx.all<ChildRow>(
          "SELECT e.name,e.name_sort,e.inode_id,e.token,i.type FROM efs_entries e JOIN efs_inodes i ON i.id=e.inode_id WHERE e.parent_inode=? AND e.inode_id IS NOT NULL ORDER BY e.name_sort LIMIT ?",
          [parentInode, limit],
          { maxRows: limit, maxBytes },
        );
  }
  childCount(parentInode: string): number {
    return (
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_entries WHERE parent_inode=? AND inode_id IS NOT NULL",
        [parentInode],
        { maxRows: 1, maxBytes: 1024 },
      )[0]?.count ?? 0
    );
  }
  linkCount(inodeId: string): number {
    return (
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_entries WHERE inode_id=?",
        [inodeId],
        { maxRows: 1, maxBytes: 1024 },
      )[0]?.count ?? 0
    );
  }
  createInode(value: {
    readonly id: string;
    readonly type: number;
    readonly mode: number;
    readonly now: number;
    readonly revision: number;
    readonly size?: number | null;
    readonly manifestHash?: Uint8Array | null;
    readonly symlinkTarget?: string | null;
  }): void {
    validateDurableIdentifier(value.id, "inode identifier");
    this.#changeMetadata(
      CHARGED_ROW_BYTES +
        (value.symlinkTarget
          ? intrinsicByteLength(encodeUtf8(value.symlinkTarget))
          : 0),
      "inode metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,?,?,?,?,?,1,?,?,?,?)",
      [
        value.id,
        value.type,
        value.mode,
        value.now,
        value.now,
        value.now,
        value.size ?? null,
        value.manifestHash ?? null,
        value.symlinkTarget ?? null,
        value.revision,
      ],
    );
  }
  upsertInode(value: {
    readonly id: string;
    readonly type: number;
    readonly mode: number;
    readonly birthtimeMs: number;
    readonly mtimeMs: number;
    readonly ctimeMs: number;
    readonly nlink: number;
    readonly size: number | null;
    readonly manifestHash: Uint8Array | null;
    readonly symlinkTarget: string | null;
    readonly token: number;
  }): void {
    validateDurableIdentifier(value.id, "inode identifier");
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT coalesce(length(CAST(symlink_target AS BLOB)),0) bytes FROM efs_inodes WHERE id=?",
      [value.id],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    const variableBytes = value.symlinkTarget
      ? intrinsicByteLength(encodeUtf8(value.symlinkTarget))
      : 0;
    this.#changeMetadata(
      (prior ? 0 : CHARGED_ROW_BYTES) + variableBytes - (prior?.bytes ?? 0),
      "inode metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET type=excluded.type,mode=excluded.mode,mtime_ms=excluded.mtime_ms,ctime_ms=excluded.ctime_ms,size=excluded.size,manifest_hash=excluded.manifest_hash,symlink_target=excluded.symlink_target,token=excluded.token",
      [
        value.id,
        value.type,
        value.mode,
        value.birthtimeMs,
        value.mtimeMs,
        value.ctimeMs,
        value.nlink,
        value.size,
        value.manifestHash,
        value.symlinkTarget,
        value.token,
      ],
    );
  }
  setFileContent(
    id: string,
    size: number,
    manifestHash: Uint8Array,
    mtime: number,
    ctime: number,
    token: number,
    expectedToken?: number,
  ): number {
    const result =
      expectedToken === undefined
        ? this.#tx.run(
            "UPDATE efs_inodes SET size=?,manifest_hash=?,mtime_ms=max(mtime_ms,?),ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
            [size, manifestHash, mtime, ctime, token, id],
          )
        : this.#tx.run(
            "UPDATE efs_inodes SET size=?,manifest_hash=?,mtime_ms=?,ctime_ms=?,token=? WHERE id=? AND token=?",
            [size, manifestHash, mtime, ctime, token, id, expectedToken],
          );
    return result.changes;
  }
  setMode(id: string, mode: number, ctime: number, token: number): void {
    this.#tx.run(
      "UPDATE efs_inodes SET mode=?,ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
      [mode, ctime, token, id],
    );
  }
  incrementLinks(id: string, ctime: number, token: number): void {
    this.#tx.run(
      "UPDATE efs_inodes SET nlink=nlink+1,ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
      [ctime, token, id],
    );
  }
  decrementLinks(id: string, ctime: number, token: number): void {
    this.#tx.run(
      "UPDATE efs_inodes SET nlink=nlink-1,ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
      [ctime, token, id],
    );
  }
  setLinks(id: string, count: number, ctime: number, token: number): void {
    this.#tx.run(
      "UPDATE efs_inodes SET nlink=?,ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
      [count, ctime, token, id],
    );
  }
  touch(id: string, mtime: number, ctime: number, token: number): void {
    this.#tx.run(
      "UPDATE efs_inodes SET mtime_ms=max(mtime_ms,?),ctime_ms=max(ctime_ms,?),token=? WHERE id=?",
      [mtime, ctime, token, id],
    );
  }
  deleteEntriesUnder(parentInode: string, tombstonesOnly = false): void {
    const where = tombstonesOnly
      ? "parent_inode=? AND inode_id IS NULL"
      : "parent_inode=?";
    const prior = this.#tx.all<{ count: number; bytes: number } & SqliteRow>(
      `SELECT count(*) count,coalesce(sum(length(name_sort)+coalesce(length(CAST(name AS BLOB)),0)),0) bytes FROM efs_entries WHERE ${where}`,
      [parentInode],
      { maxRows: 1, maxBytes: 128 },
    )[0]!;
    this.#changeMetadata(
      -(prior.count * CHARGED_ROW_BYTES + prior.bytes),
      "namespace entry cleanup",
    );
    this.#tx.run(
      tombstonesOnly
        ? "DELETE FROM efs_entries WHERE parent_inode=? AND inode_id IS NULL"
        : "DELETE FROM efs_entries WHERE parent_inode=?",
      [parentInode],
    );
  }
  deleteInode(id: string): void {
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT coalesce(length(CAST(symlink_target AS BLOB)),0) bytes FROM efs_inodes WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    if (prior)
      this.#changeMetadata(-(CHARGED_ROW_BYTES + prior.bytes), "inode cleanup");
    this.#tx.run("DELETE FROM efs_inodes WHERE id=?", [id]);
  }
  bumpRoot(kind: number, id: string): void {
    validateDurableIdentifier(id, "root journal identifier");
    const generation = this.meta().root_mutation_generation + 1;
    const rootId = encodeUtf8(id);
    new UsageRepository(this.#tx, this.#storage).apply(
      { maintenance_bytes: CHARGED_ROW_BYTES + intrinsicByteLength(rootId) },
      "namespace root journal",
      { preserveMaintenanceBytes: MAINTENANCE_TOTAL_EMERGENCY_BYTES },
    );
    this.#tx.run("UPDATE efs_meta SET root_mutation_generation=? WHERE singleton=1", [
      generation,
    ]);
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
      [generation, kind, rootId],
    );
  }
  #changeMetadata(bytes: number, reason: string): void {
    if (!bytes) return;
    new UsageRepository(this.#tx, this.#storage).apply(
      { charged_metadata_bytes: bytes },
      reason,
    );
  }
}

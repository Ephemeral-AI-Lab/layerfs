import type { FilesystemSQLiteTransaction, SqliteRow } from "../sqlite-driver.js";
import type { FilesystemLimits } from "../resources/limits.js";
import { canonicalizePath, type CanonicalPath } from "./paths.js";
import { fsError } from "../filesystem/errors.js";
import { utf8 } from "../utils/bytes.js";

export interface InodeRow extends SqliteRow { id: string; type: number; mode: number; birthtime_ms: number; mtime_ms: number; ctime_ms: number; nlink: number; size: number | null; manifest_hash: Uint8Array | null; symlink_target: string | null; token: number }
export interface EntryRow extends SqliteRow { parent_inode: string; name_sort: Uint8Array; name: string | null; inode_id: string | null; token: number }
interface MetaRow extends SqliteRow { root_inode: string; main_revision: number; root_mutation_generation: number }
export interface ResolvedPath { readonly path: CanonicalPath; readonly inode: InodeRow; readonly parentInode: string | null; readonly name: string; readonly nameSort: Uint8Array | null; readonly entryToken: number | null }

export class NamespaceRepository {
  readonly #tx: FilesystemSQLiteTransaction; readonly #limits: FilesystemLimits; readonly #syscall: string;
  constructor(tx: FilesystemSQLiteTransaction, limits: FilesystemLimits, syscall: string) { this.#tx = tx; this.#limits = limits; this.#syscall = syscall; }
  meta(): MetaRow { const row = this.#tx.all<MetaRow>("SELECT root_inode,main_revision,root_mutation_generation FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 1024 })[0]; if (!row) throw new Error("ECORRUPT: missing filesystem metadata"); return row; }
  inode(id: string): InodeRow | undefined { return this.#tx.all<InodeRow>("SELECT id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token FROM efs_inodes WHERE id=?", [id], { maxRows: 1, maxBytes: 8192 })[0]; }
  entry(parentInode: string, nameSort: Uint8Array): EntryRow | undefined { return this.#tx.all<EntryRow>("SELECT parent_inode,name_sort,name,inode_id,token FROM efs_entries WHERE parent_inode=? AND name_sort=?", [parentInode, nameSort], { maxRows: 1, maxBytes: 8192 })[0]; }
  resolve(input: string | CanonicalPath, followFinal = true): ResolvedPath {
    let path = typeof input === "string" ? canonicalizePath(input, this.#limits, this.#syscall) : input; let traversals = 0;
    restart: while (true) {
      const meta = this.meta(); const root = this.inode(meta.root_inode); if (!root || root.type !== 1) throw new Error("ECORRUPT: root inode is missing or not a directory");
      if (!path.segments.length) return Object.freeze({ path, inode: root, parentInode: null, name: "", nameSort: null, entryToken: null });
      let parent = root;
      for (let index = 0; index < path.segments.length; index += 1) {
        const name = path.segments[index]!; const nameSort = path.encodedSegments[index]!;
        const entry = this.entry(parent.id, nameSort); if (!entry?.inode_id || entry.name !== name) throw fsError("ENOENT", this.#syscall, path.value, "path does not exist");
        const inode = this.inode(entry.inode_id); if (!inode) throw new Error("ECORRUPT: live directory entry references missing inode");
        const final = index === path.segments.length - 1;
        if (inode.type === 2 && (!final || followFinal)) {
          traversals += 1; if (traversals > this.#limits.maxSymlinkTraversals) throw fsError("ELOOP", this.#syscall, path.value, "too many symbolic link traversals");
          const target = inode.symlink_target; if (!target) throw new Error("ECORRUPT: symbolic link target is missing");
          const remaining = path.segments.slice(index + 1).join("/"); const base = path.segments.slice(0, index).join("/");
          const expanded = target.startsWith("/") ? `${target}${remaining ? `/${remaining}` : ""}` : `/${base}${base ? "/" : ""}${target}${remaining ? `/${remaining}` : ""}`;
          path = canonicalizePath(expanded, this.#limits, this.#syscall); continue restart;
        }
        if (!final && inode.type !== 1) throw fsError("ENOTDIR", this.#syscall, path.value, "intermediate path component is not a directory");
        if (final) return Object.freeze({ path, inode, parentInode: parent.id, name, nameSort, entryToken: entry.token });
        parent = inode;
      }
    }
  }
  resolveOptional(input: string | CanonicalPath, followFinal = true): ResolvedPath | undefined { try { return this.resolve(input, followFinal); } catch (error) { if (error instanceof Error && "code" in error && error.code === "ENOENT") return undefined; throw error; } }
  resolveParent(path: CanonicalPath): { readonly parent: ResolvedPath; readonly name: string; readonly nameSort: Uint8Array } {
    if (!path.segments.length) throw fsError("EPERM", this.#syscall, path.value, "operation is not permitted on root");
    const name = path.segments.at(-1)!; const parentPath = path.segments.length === 1 ? "/" : `/${path.segments.slice(0, -1).join("/")}`;
    const parent = this.resolve(parentPath, true); if (parent.inode.type !== 1) throw fsError("ENOTDIR", this.#syscall, parentPath, "parent is not a directory");
    return Object.freeze({ parent, name, nameSort: utf8(name) });
  }
  nextRevision(now: number, changeCount: number, writer = "filesystem"): number {
    const meta = this.meta(); const revision = meta.main_revision + 1; const generation = meta.root_mutation_generation + 1;
    if (!Number.isSafeInteger(revision) || !Number.isSafeInteger(generation)) throw new Error("ENOSPC: revision or generation space exhausted");
    this.#tx.run("INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(?,?,?,?,?)", [revision, meta.main_revision, now, writer, changeCount]);
    this.#tx.run("UPDATE efs_meta SET main_revision=?,root_mutation_generation=? WHERE singleton=1 AND main_revision=? AND root_mutation_generation=?", [revision, generation, meta.main_revision, meta.root_mutation_generation]);
    this.#tx.run("INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,0,?)", [generation, utf8(String(revision))]);
    return revision;
  }
  recordInode(revision: number, inodeId: string, tombstone = false): void { this.#tx.run("INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(?,?,?,NULL) ON CONFLICT(revision,inode_id) DO UPDATE SET tombstone=excluded.tombstone,encoded=NULL", [revision, inodeId, tombstone ? 1 : 0]); }
  recordEntry(revision: number, parentInode: string, nameSort: Uint8Array, tombstone = false): void { this.#tx.run("INSERT INTO efs_entry_revisions(revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,?,NULL) ON CONFLICT(revision,parent_inode,name_sort) DO UPDATE SET tombstone=excluded.tombstone,encoded=NULL", [revision, parentInode, nameSort, tombstone ? 1 : 0]); }
  putEntry(parentInode: string, nameSort: Uint8Array, name: string | null, inodeId: string | null, token: number): void { this.#tx.run("INSERT INTO efs_entries(parent_inode,name_sort,name,inode_id,token) VALUES(?,?,?,?,?) ON CONFLICT(parent_inode,name_sort) DO UPDATE SET name=excluded.name,inode_id=excluded.inode_id,token=excluded.token", [parentInode, nameSort, name, inodeId, token]); }
}

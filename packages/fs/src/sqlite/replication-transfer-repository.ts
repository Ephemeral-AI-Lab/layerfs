import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import { type StorageLimits } from "../resources/limits.js";
import { UsageRepository } from "./usage-repository.js";
import { bytesToHex, copyBytes, equalBytes } from "../cas/bytes.js";
import { decodeManifestNode, decodeManifestRoot } from "../manifests/codec.js";
import type { ReplicationFlow } from "../filesystem/types.js";
import { BranchRepository } from "./branch-repository.js";
import { ContentRepository } from "./content-repository.js";
import { StagingRepository } from "./staging-repository.js";
import {
  branchPatchInsertDigest,
  computeBranchGenerationDigest,
  type BranchGenerationExpectation,
  type BranchGenerationNode,
} from "../operations/generation-digest.js";
import {
  encodeRevisionFragment,
  encodeCheckpointFragment,
  encodeBranchGenerationFragment,
  encodeGenesisFragment,
  type TransferBranchRow,
  type TransferGenesisFragment,
  type TransferNamespaceRow,
} from "./transfer-codec.js";
import type {
  OperationsStorage,
  ReplicationAuthorityResult,
  ReplicationExportMeta,
  ReplicationTransferRecord,
  ReplicationTransferStore,
} from "../operations/storage-ports.js";
import { CHARGED_ROW_BYTES } from "./usage-repository.js";
import type { ContentCache } from "../cache/content-cache.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const ZERO_DIGEST = new Uint8Array(32);
const MANIFEST_FORMAT = "efs-merkle-manifest-v1";
const CHUNKER_FORMAT = "fastcdc-v1";
const DEFAULT_FASTCDC_MINIMUM = 32_768;
const DEFAULT_FASTCDC_AVERAGE = 131_072;
const DEFAULT_FASTCDC_MAXIMUM = 524_288;

interface ExportRow extends SqliteRow {
  readonly session_id: string;
  readonly kind: number;
  readonly selected_identity: string;
  readonly selected_generation: number;
  readonly base_revision: number;
  readonly target_revision: number;
  readonly root_mutation_generation: number;
  readonly next_allocation_sequence: number;
  readonly root_inode: string;
  readonly meta_json: Uint8Array;
  readonly revision_cursor: number;
  readonly mark_kind: number;
  readonly mark_hash: Uint8Array | null;
  readonly mark_edge: number;
  readonly root_count: number;
  readonly node_count: number;
  readonly object_count: number;
  readonly object_bytes: number;
  readonly offered_roots: number;
  readonly offered_nodes: number;
  readonly offered_objects: number;
  readonly state_rows: number;
  readonly done: number;
}

interface BranchCaptureCursor {
  readonly kind: 1 | 2 | 3 | 4 | 5 | 6;
  readonly pathHex: string | null;
  readonly inodeId: string | null;
  readonly pageIndex: number | null;
  readonly generation: number | null;
  readonly sequence: number | null;
}

interface RevisionStateCursor {
  readonly revision: number;
  readonly kind: 1 | 2 | 3;
  readonly inodeId: string | null;
  readonly parentInode: string | null;
  readonly nameSortHex: string | null;
  readonly fragmentIndex: number;
}

interface ImportRow extends SqliteRow {
  readonly session_id: string;
  readonly lease_id: string;
  readonly owner_nonce: Uint8Array;
  readonly kind: number;
  readonly phase: number;
  readonly branch_id: string | null;
  readonly base_revision: number | null;
  readonly generation: number | null;
  readonly expected_generation_digest: Uint8Array | null;
  readonly closure_object_count: number;
  readonly closure_object_bytes: number;
  readonly closure_root_count: number;
  readonly closure_node_count: number;
  readonly transferred_object_count: number;
  readonly transferred_object_bytes: number;
  readonly transferred_root_count: number;
  readonly transferred_node_count: number;
  readonly state_row_count: number;
  readonly state_byte_count: number;
  readonly revision_count: number;
  readonly installed_revision_count: number;
  readonly sealed: number;
}

interface StagedRow extends SqliteRow {
  readonly key: Uint8Array;
  readonly value: Uint8Array | null;
}

interface MetaRow extends SqliteRow {
  readonly schema_version: number;
  readonly filesystem_id: string;
  readonly main_revision: number;
  readonly root_inode: string;
  readonly root_mutation_generation: number;
  readonly last_root_removal_generation: number;
  readonly next_allocation_sequence: number;
  readonly cow_page_bytes: number;
  readonly max_manifest_entries: number;
  readonly max_manifest_depth: number;
  readonly max_file_bytes: number;
  readonly writer_profile: string;
  readonly created_at_ms: number;
}

interface RevisionHeaderRow extends SqliteRow {
  readonly revision: number;
  readonly parent_revision: number | null;
  readonly created_at_ms: number;
  readonly writer_id: string;
  readonly change_count: number;
}

interface BranchRowSql extends SqliteRow {
  readonly base_revision: number;
  readonly state: number;
  readonly generation: number;
  readonly created_at_ms: number;
  readonly terminal_at_ms: number | null;
  readonly merged_revision: number | null;
}

interface BranchResultSql extends SqliteRow {
  readonly operation_id: string;
  readonly branch_id: string;
  readonly generation: number;
  readonly reservation_nonce: Uint8Array;
  readonly outcome: number;
  readonly encoded: Uint8Array | null;
  readonly expires_at_ms: number | null;
}

interface InodeProjectionRow extends SqliteRow {
  readonly id: string;
  readonly type: number;
  readonly mode: number;
  readonly birthtime_ms: number;
  readonly mtime_ms: number;
  readonly ctime_ms: number;
  readonly nlink: number;
  readonly size: number | null;
  readonly manifest_hash: Uint8Array | null;
  readonly symlink_target: string | null;
  readonly token: number;
}

interface EntryProjectionRow extends SqliteRow {
  readonly parent_inode: string;
  readonly name_sort: Uint8Array;
  readonly name: string | null;
  readonly inode_id: string | null;
  readonly token: number;
}

function transferError(code: string, message: string): Error {
  return new Error(`${code}: ${message}`);
}

function decodeJson<T>(bytes: Uint8Array | null): T | undefined {
  if (!bytes) return undefined;
  return JSON.parse(decoder.decode(bytes)) as T;
}

function encodeJson(value: unknown): Uint8Array {
  return encoder.encode(JSON.stringify(value));
}

function safeNonnegative(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`${name} is invalid`);
  return value;
}

function u64be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError("u64be value is invalid");
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), false);
  return out;
}

function u32be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff)
    throw new RangeError("uint32 value is outside the canonical envelope");
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, false);
  return out;
}

function readU64(bytes: Uint8Array, offset: number, name: string): number {
  if (offset + 8 > bytes.byteLength) throw new RangeError(`truncated ${name}`);
  const value = new DataView(bytes.buffer, bytes.byteOffset + offset, 8).getBigUint64(
    0,
    false,
  );
  if (value > BigInt(Number.MAX_SAFE_INTEGER))
    throw new RangeError(`${name} exceeds the safe integer envelope`);
  return Number(value);
}

function readU32(bytes: Uint8Array, offset: number, name: string): number {
  if (offset + 4 > bytes.byteLength) throw transferError("IntegrityFailure", `truncated ${name}`);
  return new DataView(bytes.buffer, bytes.byteOffset + offset, 4).getUint32(0, false);
}

function snapshotTextKey(value: string): Uint8Array {
  const encoded = encoder.encode(value);
  return keyBytes([u32be(encoded.byteLength), encoded]);
}

function snapshotOptionalU64(value: number | null): Uint8Array {
  return value === null ? Uint8Array.of(0) : Uint8Array.of(1, ...u64be(value));
}

function snapshotOptionalBytes(value: Uint8Array | null): Uint8Array {
  return value === null
    ? Uint8Array.of(0)
    : new Uint8Array([1, ...u32be(value.byteLength), ...value]);
}

function encodeBranchSnapshotRow(row: TransferBranchRow): Readonly<{
  readonly kind: number;
  readonly key: Uint8Array;
  readonly value: Uint8Array;
}> {
  if (row.kind === 1)
    return Object.freeze({
      kind: 1,
      key: copyBytes(row.path),
      value: new Uint8Array([
        row.disposition,
        ...snapshotOptionalU64(row.expectedToken),
        ...snapshotOptionalBytes(row.encoded),
      ]),
    });
  if (row.kind === 2)
    return Object.freeze({
      kind: 2,
      key: snapshotTextKey(row.inodeId),
      value: new Uint8Array([
        ...snapshotOptionalU64(row.expectedToken),
        ...u32be(row.encoded.byteLength),
        ...row.encoded,
      ]),
    });
  if (row.kind === 3)
    return Object.freeze({
      kind: 3,
      key: new Uint8Array([
        ...snapshotTextKey(row.inodeId),
        ...u64be(row.pageIndex),
        ...u64be(row.generation),
      ]),
      value: new Uint8Array([
        ...u64be(row.created_at_ms),
        row.head ? 1 : 0,
        ...u32be(row.bytes.byteLength),
        ...row.bytes,
      ]),
    });
  if (row.kind === 4)
    return Object.freeze({
      kind: 4,
      key: new Uint8Array([...snapshotTextKey(row.inodeId), ...u64be(row.sequence)]),
      value: new Uint8Array([
        ...u64be(row.generation),
        ...u64be(row.offset),
        ...u64be(row.deleteLength),
        ...u64be(row.insertLength),
        ...u32be(row.segments.length),
        ...row.segments.flatMap((segment) => [...u32be(segment.byteLength), ...segment]),
      ]),
    });
  if (row.kind === 5)
    return Object.freeze({
      kind: 5,
      key: snapshotTextKey(row.inodeId),
      value: snapshotOptionalU64(row.expectedToken),
    });
  return Object.freeze({ kind: 6, key: copyBytes(row.path), value: copyBytes(row.manifestHash) });
}

function decodeSnapshotTextKey(bytes: Uint8Array, offset: number, name: string): Readonly<{
  readonly value: string;
  readonly next: number;
}> {
  const length = readU32(bytes, offset, `${name}.length`);
  const start = offset + 4;
  if (start + length > bytes.byteLength) throw transferError("IntegrityFailure", `truncated ${name}`);
  let value: string;
  try {
    value = decoder.decode(bytes.subarray(start, start + length));
  } catch {
    throw transferError("IntegrityFailure", `${name} is not UTF-8`);
  }
  return Object.freeze({ value, next: start + length });
}

function decodeSnapshotOptionalU64(bytes: Uint8Array, offset: number, name: string): Readonly<{
  readonly value: number | null;
  readonly next: number;
}> {
  const tag = bytes[offset];
  if (tag === 0) return Object.freeze({ value: null, next: offset + 1 });
  if (tag !== 1) throw transferError("IntegrityFailure", `${name} optional tag is invalid`);
  return Object.freeze({ value: readU64(bytes, offset + 1, name), next: offset + 9 });
}

function decodeSnapshotOptionalBytes(bytes: Uint8Array, offset: number, name: string): Readonly<{
  readonly value: Uint8Array | null;
  readonly next: number;
}> {
  const tag = bytes[offset];
  if (tag === 0) return Object.freeze({ value: null, next: offset + 1 });
  if (tag !== 1) throw transferError("IntegrityFailure", `${name} optional tag is invalid`);
  const length = readU32(bytes, offset + 1, `${name}.length`);
  const start = offset + 5;
  if (start + length > bytes.byteLength) throw transferError("IntegrityFailure", `truncated ${name}`);
  return Object.freeze({ value: copyBytes(bytes.subarray(start, start + length)), next: start + length });
}

function decodeBranchSnapshotRow(kind: number, key: Uint8Array, value: Uint8Array): TransferBranchRow {
  if (kind === 1) {
    const expected = decodeSnapshotOptionalU64(value, 1, "change expected token");
    const encoded = decodeSnapshotOptionalBytes(value, expected.next, "change encoded");
    if (encoded.next !== value.byteLength || value[0]! > 1)
      throw transferError("IntegrityFailure", "change snapshot row is invalid");
    return { kind: 1, path: copyBytes(key), disposition: value[0]!, expectedToken: expected.value, encoded: encoded.value };
  }
  if (kind === 2) {
    const inode = decodeSnapshotTextKey(key, 0, "overlay inode");
    const expected = decodeSnapshotOptionalU64(value, 0, "overlay expected token");
    const length = readU32(value, expected.next, "overlay encoded.length");
    const start = expected.next + 4;
    if (start + length !== value.byteLength) throw transferError("IntegrityFailure", "overlay snapshot row is invalid");
    return { kind: 2, inodeId: inode.value, expectedToken: expected.value, encoded: copyBytes(value.subarray(start)) };
  }
  if (kind === 3) {
    const inode = decodeSnapshotTextKey(key, 0, "page inode");
    const pageIndex = readU64(key, inode.next, "page index");
    const generation = readU64(key, inode.next + 8, "page generation");
    const created = readU64(value, 0, "page creation time");
    const head = value[8];
    const length = readU32(value, 9, "page bytes.length");
    if ((head !== 0 && head !== 1) || 13 + length !== value.byteLength)
      throw transferError("IntegrityFailure", "page snapshot row is invalid");
    return { kind: 3, inodeId: inode.value, pageIndex, generation, bytes: copyBytes(value.subarray(13)), created_at_ms: created, head: head === 1 };
  }
  if (kind === 4) {
    const inode = decodeSnapshotTextKey(key, 0, "patch inode");
    const sequence = readU64(key, inode.next, "patch sequence");
    let offset = 0;
    const generation = readU64(value, offset, "patch generation"); offset += 8;
    const patchOffset = readU64(value, offset, "patch offset"); offset += 8;
    const deleteLength = readU64(value, offset, "patch delete length"); offset += 8;
    const insertLength = readU64(value, offset, "patch insert length"); offset += 8;
    const count = readU32(value, offset, "patch segment count"); offset += 4;
    if (count > 64) throw transferError("IntegrityFailure", "patch snapshot segment count exceeds limit");
    const segments: Uint8Array[] = [];
    for (let index = 0; index < count; index += 1) {
      const length = readU32(value, offset, "patch segment length"); offset += 4;
      if (offset + length > value.byteLength) throw transferError("IntegrityFailure", "truncated patch segment");
      segments.push(copyBytes(value.subarray(offset, offset + length))); offset += length;
    }
    if (offset !== value.byteLength) throw transferError("IntegrityFailure", "patch snapshot row has trailing bytes");
    return { kind: 4, inodeId: inode.value, sequence, generation, offset: patchOffset, deleteLength, insertLength, segments };
  }
  if (kind === 5) {
    const inode = decodeSnapshotTextKey(key, 0, "expectation inode");
    const expected = decodeSnapshotOptionalU64(value, 0, "expectation token");
    if (expected.next !== value.byteLength) throw transferError("IntegrityFailure", "expectation snapshot row is invalid");
    return { kind: 5, inodeId: inode.value, expectedToken: expected.value };
  }
  if (kind === 6 && value.byteLength === 32)
    return { kind: 6, path: copyBytes(key), manifestHash: copyBytes(value) };
  throw transferError("IntegrityFailure", "unknown branch snapshot row");
}

function u8(value: number): Uint8Array {
  return Uint8Array.of(value);
}

function keyBytes(parts: readonly (Uint8Array | string)[]): Uint8Array {
  let length = 0;
  for (const part of parts)
    length += typeof part === "string" ? encoder.encode(part).byteLength : part.byteLength;
  const out = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    const bytes = typeof part === "string" ? encoder.encode(part) : part;
    out.set(bytes, offset);
    offset += bytes.byteLength;
  }
  return out;
}

function parseIntegerRevision(text: string, name: string): number {
  if (!/^[0-9]+$/u.test(text)) throw new RangeError(`${name} is invalid`);
  const value = Number(text);
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`${name} is invalid`);
  return value;
}

function intrinsicByteLength(value: Uint8Array): number {
  return value.byteLength;
}

function deserializeInode(encoded: Uint8Array): InodeProjectionRow {
  const value = decodeJson<Record<string, unknown>>(encoded);
  if (!value || typeof value !== "object")
    throw transferError("IntegrityFailure", "inode revision is not canonical JSON");
  const id = value.id;
  if (typeof id !== "string" || !/^[0-9a-f-]{36}$/u.test(id))
    throw transferError("IntegrityFailure", "inode identifier is invalid");
  const row: InodeProjectionRow = {
    id,
    type: value.type as number,
    mode: value.mode as number,
    birthtime_ms: value.birthtime_ms as number,
    mtime_ms: value.mtime_ms as number,
    ctime_ms: value.ctime_ms as number,
    nlink: value.nlink as number,
    size: (value.size as number | null) ?? null,
    manifest_hash:
      typeof value.manifest_hash === "string"
        ? hexBytes(value.manifest_hash)
        : null,
    symlink_target: (value.symlink_target as string | null) ?? null,
    token: value.token as number,
  };
  for (const name of [
    "type",
    "mode",
    "birthtime_ms",
    "mtime_ms",
    "ctime_ms",
    "nlink",
    "token",
  ] as const)
    if (!Number.isSafeInteger(row[name]) || row[name] < 0)
      throw transferError("IntegrityFailure", `inode field ${name} is invalid`);
  if (row.size !== null && (!Number.isSafeInteger(row.size) || row.size < 0))
    throw transferError("IntegrityFailure", "inode size is invalid");
  if (row.type !== 0 && row.type !== 1 && row.type !== 2)
    throw transferError("IntegrityFailure", "inode type is invalid");
  return row;
}

function deserializeEntry(encoded: Uint8Array): EntryProjectionRow {
  const value = decodeJson<Record<string, unknown>>(encoded);
  if (!value || typeof value !== "object")
    throw transferError("IntegrityFailure", "entry revision is not canonical JSON");
  const parentInode = value.parent_inode;
  const nameSort = value.name_sort;
  const inodeId = value.inode_id;
  const token = value.token as number;
  if (
    typeof parentInode !== "string" ||
    typeof nameSort !== "string" ||
    !Number.isSafeInteger(token) ||
    token < 0 ||
    (inodeId !== null && typeof inodeId !== "string")
  )
    throw transferError("IntegrityFailure", "entry revision is invalid");
  return Object.freeze({
    parent_inode: parentInode,
    name_sort: hexBytes(nameSort),
    name: (value.name as string | null) ?? null,
    inode_id: (inodeId as string | null) ?? null,
    token,
  });
}

function hexBytes(value: string): Uint8Array {
  if (value.length % 2 !== 0 || !/^[0-9a-f]*$/u.test(value))
    throw transferError("IntegrityFailure", "hex byte value is invalid");
  const out = new Uint8Array(value.length / 2);
  for (let index = 0; index < out.length; index += 1)
    out[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return out;
}

export class ReplicationTransferRepository implements ReplicationTransferStore {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  readonly #hashBytes: (bytes: Uint8Array) => Uint8Array;
  readonly #maxBindings: number;
  readonly #branchDigest: ((branchId: string, generation: number) => string) | null;
  readonly #cache: ContentCache | undefined;
  #resultRetentionMs = 30 * 24 * 60 * 60 * 1000;

  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: StorageLimits,
    hashBytes: (bytes: Uint8Array) => Uint8Array,
    maxBindings: number,
    branchDigest?: (branchId: string, generation: number) => string,
    cache?: ContentCache,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#hashBytes = hashBytes;
    this.#maxBindings = maxBindings;
    this.#branchDigest = branchDigest ?? null;
    this.#cache = cache;
  }

  #content(): ContentRepository {
    return new ContentRepository(this.#tx, this.#limits, this.#cache, this.#hashBytes);
  }

  #staging(): StagingRepository {
    return new StagingRepository(
      this.#tx,
      this.#limits,
       this.#cache,
      this.#hashBytes,
      this.#maxBindings,
    );
  }

  #branches(): BranchRepository {
    return new BranchRepository(this.#tx, this.#limits);
  }

  #meta(): MetaRow {
    const rows = this.#tx.all<MetaRow>(
      "SELECT schema_version,filesystem_id,main_revision,root_inode,root_mutation_generation,last_root_removal_generation,next_allocation_sequence,cow_page_bytes,max_manifest_entries,max_manifest_depth,max_file_bytes,writer_profile,created_at_ms FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 4096 },
    );
    const meta = rows[0];
    if (!meta || meta.schema_version !== 13)
      throw transferError("ECORRUPT", "invalid filesystem metadata");
    return meta;
  }

  #exportRow(sessionId: string): ExportRow {
    const rows = this.#tx.all<ExportRow>(
      "SELECT session_id,kind,selected_identity,selected_generation,base_revision,target_revision,root_mutation_generation,next_allocation_sequence,root_inode,meta_json,revision_cursor,mark_kind,mark_hash,mark_edge,root_count,node_count,object_count,object_bytes,offered_roots,offered_nodes,offered_objects,state_rows,done FROM efs_replication_exports WHERE session_id=?",
      [sessionId],
      { maxRows: 1, maxBytes: 16384 },
    )[0];
    if (!rows) throw transferError("CursorMismatch", "export state is missing");
    return rows;
  }

  #importRow(sessionId: string): ImportRow {
    const rows = this.#tx.all<ImportRow>(
      "SELECT session_id,lease_id,owner_nonce,kind,phase,branch_id,base_revision,generation,expected_generation_digest,closure_object_count,closure_object_bytes,closure_root_count,closure_node_count,transferred_object_count,transferred_object_bytes,transferred_root_count,transferred_node_count,state_row_count,state_byte_count,revision_count,installed_revision_count,sealed FROM efs_replication_imports WHERE session_id=?",
      [sessionId],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!rows) throw transferError("CursorMismatch", "import state is missing");
    return rows;
  }

  #pendingMarks(sessionId: string, limit: number): readonly {
    readonly kind: number;
    readonly hash: Uint8Array;
    readonly edge: number;
  }[] {
    return this.#tx.all<{ kind: number; hash: Uint8Array; edge: number } & SqliteRow>(
      "SELECT kind,hash,edge FROM efs_replication_export_marks WHERE session_id=? ORDER BY kind,hash LIMIT ?",
      [sessionId, limit],
      { maxRows: limit, maxBytes: 256 * 1024 },
    );
  }

  captureExport(options: {
    readonly sessionId: string;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
    readonly destinationHead: number;
    readonly now: number;
    readonly expiresAt: number;
  }): {
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly destinationHead: number;
    readonly rootMutationGeneration: number;
    readonly nextAllocationSequence: number;
    readonly rootInode: string;
    readonly complete: boolean;
  } {
    const meta = this.#meta();
    safeNonnegative(options.destinationHead, "destination head");
    const prior = this.#tx.all<{
      kind: number;
      selected_identity: string;
      selected_generation: number;
      base_revision: number;
      target_revision: number;
      root_mutation_generation: number;
      next_allocation_sequence: number;
      root_inode: string;
      revision_cursor: number;
      done: number;
    } & SqliteRow>(
      "SELECT kind,selected_identity,selected_generation,base_revision,target_revision,root_mutation_generation,next_allocation_sequence,root_inode,revision_cursor,done FROM efs_replication_exports WHERE session_id=?",
      [options.sessionId],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (prior) {
      const expectedKind = options.flow === "authority-main-to-replica" ? 0 : 1;
      const expectedIdentity = expectedKind === 0 ? String(prior.target_revision) : (options.branchId ?? "");
      if (
        prior.kind !== expectedKind ||
        prior.selected_identity !== expectedIdentity ||
        (expectedKind === 0 && prior.revision_cursor !== options.destinationHead)
      )
        throw transferError("OperationMismatch", "replication export binding changed during resume");
      return Object.freeze({
        selectedRevision: expectedKind === 0 ? prior.target_revision : prior.base_revision,
        selectedGeneration: expectedKind === 0 ? null : prior.selected_generation,
        destinationHead: expectedKind === 0 ? prior.revision_cursor : options.destinationHead,
        rootMutationGeneration: prior.root_mutation_generation,
        nextAllocationSequence: prior.next_allocation_sequence,
        rootInode: prior.root_inode,
        complete: prior.done === 1,
      });
    }
    let selectedRevision: number;
    let selectedGeneration: number | null = null;
    let selectedBranchBaseRevision: number | null = null;
    let selectedBranchDigest: string | null = null;
    let selectedBranchPreviousGeneration: number | null = null;
    let selectedBranchPreviousDigest: string | null = null;
    let rootHashes: readonly { readonly hash: Uint8Array }[];
    let state: 0 | 1 | 2 = 0;
    if (options.flow === "authority-main-to-replica") {
      selectedRevision = meta.main_revision;
      if (options.destinationHead > selectedRevision)
        throw transferError(
          "MainDiverged",
          "destination head is ahead of the selected source head",
        );
      rootHashes = this.#tx.all<{ hash: Uint8Array } & SqliteRow>(
        "SELECT DISTINCT manifest_hash hash FROM efs_revision_manifest_roots WHERE revision>? AND revision<=? ORDER BY manifest_hash",
        [options.destinationHead, selectedRevision],
        { maxRows: 8192, maxBytes: 512 * 1024 },
      );
    } else {
      const branchId = options.branchId;
      if (!branchId) throw new RangeError("branch flow requires a branchId");
      const rows = this.#tx.all<BranchRowSql & SqliteRow>(
        "SELECT base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision FROM efs_branches WHERE id=?",
        [branchId],
        { maxRows: 1, maxBytes: 2048 },
      );
      const branch = rows[0];
      if (!branch)
        throw transferError("BranchIdentityMismatch", "branch does not exist");
      if (
        options.flow === "replica-branch-to-authority" ||
        options.flow === "replica-branch-to-replica"
      ) {
        if (branch.state !== 0)
          throw transferError(
            "UnauthorizedScope",
            "a replica may export only an active branch generation",
          );
      }
      state = branch.state as 0 | 1 | 2;
      selectedGeneration = branch.generation;
      const base = branch.base_revision;
      selectedBranchBaseRevision = base;
      selectedBranchDigest = this.#storedBranchDigest(options.sessionId, branchId, branch.generation).reduce(
        (output, byte) => output + byte.toString(16).padStart(2, "0"),
        "",
      );
      const prior = this.#branches().storedGenerationDigest(branchId);
      if (prior && prior.generation < branch.generation) {
        selectedBranchPreviousGeneration = prior.generation;
        selectedBranchPreviousDigest = prior.digest;
      }
      // Retain the exact source snapshot digest so a later export can carry
      // the predecessor required to advance an already-installed replica
      // branch.  The row is durable and replaced atomically with the capture;
      // it is also used for terminal generations, so this does not introduce
      // a second generation-digest format or an in-memory history.
      this.#branches().putTerminalGenerationDigest(
        branchId,
        branch.generation,
        selectedBranchDigest,
      );
      if (base < 0 || base > options.destinationHead)
        throw transferError(
          "BaseRevisionMissing",
          "destination does not contain the branch base revision",
        );
      selectedRevision = base;
      rootHashes = this.#tx.all<{ hash: Uint8Array } & SqliteRow>(
        "SELECT manifest_hash hash FROM efs_branch_manifest_roots WHERE branch_id=? ORDER BY manifest_hash",
        [branchId],
        { maxRows: 8192, maxBytes: 512 * 1024 },
      );
    }
    const root = this.#tx.all<InodeProjectionRow & SqliteRow>(
      "SELECT id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token FROM efs_inodes WHERE id=?",
      [meta.root_inode],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!root)
      throw transferError("ECORRUPT", "root inode is missing");
    const fastCdc = this.#tx.all<
      { chunk_min: number; chunk_avg: number; chunk_max: number } & SqliteRow
    >(
      "SELECT chunk_min,chunk_avg,chunk_max FROM efs_manifest_roots ORDER BY allocation_sequence LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    const metaJson = encodeJson({
      filesystemId: meta.filesystem_id,
      rootInode: meta.root_inode,
      mainRevision: meta.main_revision,
      rootMutationGeneration: meta.root_mutation_generation,
      nextAllocationSequence: meta.next_allocation_sequence,
      cowPageBytes: meta.cow_page_bytes,
      createdAtMs: meta.created_at_ms,
      maxManifestEntries: meta.max_manifest_entries,
      maxManifestDepth: meta.max_manifest_depth,
      maxFileBytes: meta.max_file_bytes,
      writerProfile: meta.writer_profile,
      manifestFormat: MANIFEST_FORMAT,
      chunkerFormat: CHUNKER_FORMAT,
      fastCdcMinimum: fastCdc?.chunk_min ?? 0,
      fastCdcAverage: fastCdc?.chunk_avg ?? 0,
      fastCdcMaximum: fastCdc?.chunk_max ?? 0,
      rootInodeType: root.type,
      rootMode: root.mode,
      rootBirthtimeMs: root.birthtime_ms,
      rootMtimeMs: root.mtime_ms,
      rootCtimeMs: root.ctime_ms,
      rootToken: root.token,
      branchState: state,
      branchBaseRevision: selectedBranchBaseRevision,
      branchGenerationDigest: selectedBranchDigest,
      branchPreviousGeneration: selectedBranchPreviousGeneration,
      branchPreviousGenerationDigest: selectedBranchPreviousDigest,
      branchCapture: options.flow === "authority-main-to-replica"
        ? null
        : { kind: 1, pathHex: null, inodeId: null, pageIndex: null, generation: null, sequence: null },
      branchCaptureComplete: options.flow === "authority-main-to-replica",
    });
    this.#tx.run(
      "INSERT INTO efs_replication_exports(session_id,kind,selected_identity,selected_generation,base_revision,target_revision,root_mutation_generation,next_allocation_sequence,root_inode,meta_json,revision_cursor,mark_kind,mark_hash,mark_edge,root_count,node_count,object_count,object_bytes,offered_roots,offered_nodes,offered_objects,state_rows,done) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,0,0,0,0,0,0,0,0)",
      [
        options.sessionId,
        options.flow === "authority-main-to-replica" ? 0 : 1,
        options.flow === "authority-main-to-replica"
          ? String(selectedRevision)
          : (options.branchId ?? ""),
        selectedGeneration ?? 0,
        selectedBranchBaseRevision ?? options.destinationHead,
        meta.main_revision,
        meta.root_mutation_generation,
        meta.next_allocation_sequence,
        meta.root_inode,
        metaJson,
        options.flow === "authority-main-to-replica" ? options.destinationHead : -1,
        0,
        null,
        0,
      ],
    );
    if (options.flow !== "authority-main-to-replica")
      this.#snapshotBranchRows(options.sessionId, options.branchId!, selectedGeneration!);
    for (const row of rootHashes)
      this.#tx.run(
        "INSERT OR IGNORE INTO efs_replication_export_marks(session_id,kind,hash,edge) VALUES(?,0,?,0)",
        [options.sessionId, row.hash],
      );
    return Object.freeze({
      selectedRevision,
      selectedGeneration,
      destinationHead: options.destinationHead,
      rootMutationGeneration: meta.root_mutation_generation,
      nextAllocationSequence: meta.next_allocation_sequence,
      rootInode: meta.root_inode,
      complete: false,
    });
  }

  #snapshotBranchRows(sessionId: string, branchId: string, generation: number): boolean {
    const exportRow = this.#exportRow(sessionId);
    const metadata = decodeJson<Record<string, unknown>>(exportRow.meta_json) ?? {};
    if (metadata.branchCaptureComplete === true) return true;
    const liveBranch = this.#tx.all<BranchRowSql & SqliteRow>(
      "SELECT base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision FROM efs_branches WHERE id=?",
      [branchId],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    if (!liveBranch || liveBranch.generation !== generation)
      throw transferError("BranchDiverged", "branch changed during export capture");
    const raw = metadata.branchCapture as Partial<BranchCaptureCursor> | undefined;
    let cursor: BranchCaptureCursor = {
      kind: raw?.kind === 2 || raw?.kind === 3 || raw?.kind === 4 || raw?.kind === 5 || raw?.kind === 6 ? raw.kind : 1,
      pathHex: typeof raw?.pathHex === "string" ? raw.pathHex : null,
      inodeId: typeof raw?.inodeId === "string" ? raw.inodeId : null,
      pageIndex: raw && Number.isSafeInteger(raw.pageIndex) ? raw.pageIndex ?? null : null,
      generation: raw && Number.isSafeInteger(raw.generation) ? raw.generation ?? null : null,
      sequence: raw && Number.isSafeInteger(raw.sequence) ? raw.sequence ?? null : null,
    };
    const pageSize = Math.max(1, Math.min(this.#limits.maxQueryBatchSize, 64));
    const reset = (kind: 1 | 2 | 3 | 4 | 5 | 6): BranchCaptureCursor => ({
      kind, pathHex: null, inodeId: null, pageIndex: null, generation: null, sequence: null,
    });
    const insert = (row: TransferBranchRow): void => {
      const encoded = encodeBranchSnapshotRow(row);
      const nextIndex = this.#tx.all<{ next_index: number } & SqliteRow>(
        "SELECT coalesce(max(row_index),-1)+1 next_index FROM efs_replication_export_rows WHERE session_id=?",
        [sessionId], { maxRows: 1, maxBytes: 256 },
      )[0]!.next_index;
      this.#tx.run(
        "INSERT INTO efs_replication_export_rows(session_id,row_index,kind,row_key,value) VALUES(?,?,?,?,?)",
        [sessionId, nextIndex, encoded.kind, encoded.key, encoded.value],
      );
    };

    // One durable invocation captures at most one bounded keyset page.  The
    // cursor and page rows commit together, so a statement fault retries the
    // same page without skipping or duplicating source rows.
    while (cursor.kind <= 6) {
      let rowCount = 0;
      if (cursor.kind === 1) {
        const rows = this.#tx.all<{ path: Uint8Array; expected_token: number | null; kind: number; encoded: Uint8Array | null } & SqliteRow>(
          cursor.pathHex === null
            ? "SELECT path,expected_token,kind,encoded FROM efs_branch_changes WHERE branch_id=? ORDER BY path LIMIT ?"
            : "SELECT path,expected_token,kind,encoded FROM efs_branch_changes WHERE branch_id=? AND path>? ORDER BY path LIMIT ?",
          cursor.pathHex === null ? [branchId, pageSize] : [branchId, hexBytes(cursor.pathHex), pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          insert({ kind: 1, path: copyBytes(row.path), disposition: row.kind, expectedToken: row.expected_token, encoded: row.encoded ? copyBytes(row.encoded) : null });
          cursor = { ...cursor, pathHex: bytesToHex(row.path) };
        }
        rowCount = rows.length;
      } else if (cursor.kind === 2) {
        const rows = this.#tx.all<{ inode_id: string; expected_token: number | null; encoded: Uint8Array } & SqliteRow>(
          cursor.inodeId === null
            ? "SELECT inode_id,expected_token,encoded FROM efs_branch_inode_overlays WHERE branch_id=? ORDER BY inode_id LIMIT ?"
            : "SELECT inode_id,expected_token,encoded FROM efs_branch_inode_overlays WHERE branch_id=? AND inode_id>? ORDER BY inode_id LIMIT ?",
          cursor.inodeId === null ? [branchId, pageSize] : [branchId, cursor.inodeId, pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          insert({ kind: 2, inodeId: row.inode_id, expectedToken: row.expected_token, encoded: copyBytes(row.encoded) });
          cursor = { ...cursor, inodeId: row.inode_id };
        }
        rowCount = rows.length;
      } else if (cursor.kind === 3) {
        const rows = this.#tx.all<{ inode_id: string; page_index: number; generation: number; bytes: Uint8Array; created_at_ms: number; head: number } & SqliteRow>(
          cursor.inodeId === null
            ? "SELECT v.inode_id,v.page_index,v.generation,v.bytes,v.created_at_ms,CASE WHEN v.generation=(SELECT max(v2.generation) FROM efs_cow_page_versions v2 WHERE v2.branch_id=v.branch_id AND v2.inode_id=v.inode_id AND v2.page_index=v.page_index AND v2.generation<=?) THEN 1 ELSE 0 END head FROM efs_cow_page_versions v WHERE v.branch_id=? AND v.generation<=? ORDER BY v.inode_id,v.page_index,v.generation LIMIT ?"
            : "SELECT v.inode_id,v.page_index,v.generation,v.bytes,v.created_at_ms,CASE WHEN v.generation=(SELECT max(v2.generation) FROM efs_cow_page_versions v2 WHERE v2.branch_id=v.branch_id AND v2.inode_id=v.inode_id AND v2.page_index=v.page_index AND v2.generation<=?) THEN 1 ELSE 0 END head FROM efs_cow_page_versions v WHERE v.branch_id=? AND v.generation<=? AND (v.inode_id>? OR (v.inode_id=? AND (v.page_index>? OR (v.page_index=? AND v.generation>?)))) ORDER BY v.inode_id,v.page_index,v.generation LIMIT ?",
          cursor.inodeId === null
            ? [generation, branchId, generation, pageSize]
            : [generation, branchId, generation, cursor.inodeId, cursor.inodeId, cursor.pageIndex, cursor.pageIndex, cursor.generation, pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          insert({ kind: 3, inodeId: row.inode_id, pageIndex: row.page_index, generation: row.generation, bytes: copyBytes(row.bytes), created_at_ms: row.created_at_ms, head: row.head === 1 });
          cursor = { ...cursor, inodeId: row.inode_id, pageIndex: row.page_index, generation: row.generation };
        }
        rowCount = rows.length;
      } else if (cursor.kind === 4) {
        const rows = this.#tx.all<{ inode_id: string; sequence: number; generation: number; offset: number; delete_length: number; insert_length: number } & SqliteRow>(
          cursor.inodeId === null
            ? "SELECT inode_id,sequence,generation,offset,delete_length,insert_length FROM efs_patches WHERE branch_id=? AND generation<=? ORDER BY inode_id,sequence LIMIT ?"
            : "SELECT inode_id,sequence,generation,offset,delete_length,insert_length FROM efs_patches WHERE branch_id=? AND generation<=? AND (inode_id>? OR (inode_id=? AND sequence>?)) ORDER BY inode_id,sequence LIMIT ?",
          cursor.inodeId === null ? [branchId, generation, pageSize] : [branchId, generation, cursor.inodeId, cursor.inodeId, cursor.sequence, pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          const segments = this.#tx.all<{ segment_index: number; bytes: Uint8Array } & SqliteRow>(
            "SELECT segment_index,bytes FROM efs_patch_segments WHERE branch_id=? AND inode_id=? AND sequence=? ORDER BY segment_index",
            [branchId, row.inode_id, row.sequence], { maxRows: 64, maxBytes: this.#limits.maxFinalTransactionBytes },
          );
          insert({ kind: 4, inodeId: row.inode_id, sequence: row.sequence, generation: row.generation, offset: row.offset, deleteLength: row.delete_length, insertLength: row.insert_length, segments: segments.map((segment) => copyBytes(segment.bytes)) });
          cursor = { ...cursor, inodeId: row.inode_id, sequence: row.sequence };
        }
        rowCount = rows.length;
      } else if (cursor.kind === 5) {
        const rows = this.#tx.all<{ inode_id: string; expected_token: number | null } & SqliteRow>(
          cursor.inodeId === null
            ? "SELECT inode_id,expected_token FROM efs_branch_inode_expectations WHERE branch_id=? ORDER BY inode_id LIMIT ?"
            : "SELECT inode_id,expected_token FROM efs_branch_inode_expectations WHERE branch_id=? AND inode_id>? ORDER BY inode_id LIMIT ?",
          cursor.inodeId === null ? [branchId, pageSize] : [branchId, cursor.inodeId, pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          insert({ kind: 5, inodeId: row.inode_id, expectedToken: row.expected_token });
          cursor = { ...cursor, inodeId: row.inode_id };
        }
        rowCount = rows.length;
      } else {
        const rows = this.#tx.all<{ path: Uint8Array; manifest_hash: Uint8Array } & SqliteRow>(
          cursor.pathHex === null
            ? "SELECT path,manifest_hash FROM efs_branch_manifest_roots WHERE branch_id=? ORDER BY path LIMIT ?"
            : "SELECT path,manifest_hash FROM efs_branch_manifest_roots WHERE branch_id=? AND path>? ORDER BY path LIMIT ?",
          cursor.pathHex === null ? [branchId, pageSize] : [branchId, hexBytes(cursor.pathHex), pageSize],
          { maxRows: pageSize, maxBytes: this.#limits.maxFinalTransactionBytes },
        );
        for (const row of rows) {
          insert({ kind: 6, path: copyBytes(row.path), manifestHash: copyBytes(row.manifest_hash) });
          cursor = { ...cursor, pathHex: bytesToHex(row.path) };
        }
        rowCount = rows.length;
      }
      if (rowCount === pageSize) break;
      if (cursor.kind === 6) {
        cursor = reset(6);
        break;
      }
      cursor = reset((cursor.kind + 1) as 1 | 2 | 3 | 4 | 5 | 6);
    }
    const complete = cursor.kind === 6 && cursor.pathHex === null;
    this.#tx.run(
      "UPDATE efs_replication_exports SET meta_json=? WHERE session_id=?",
      [encodeJson({ ...metadata, branchCapture: cursor, branchCaptureComplete: complete }), sessionId],
    );
    return complete;
  }

  #offerNodeChildren(
    sessionId: string,
    hash: Uint8Array,
    edge: number,
    budget: number,
  ): { readonly done: boolean; readonly nextEdge: number } {
    const rows = this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
      "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
      [hash],
      { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 4096 },
    );
    if (rows.length !== 1)
      throw transferError("ECORRUPT", "export manifest node is missing");
    const decoded = decodeManifestNode(rows[0]!.encoded, hash);
    let nextEdge = edge;
    let queued = 0;
    if (decoded.kind === "internal") {
      while (nextEdge < decoded.children.length && queued < budget) {
        const child = decoded.children[nextEdge]!;
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_export_marks(session_id,kind,hash,edge) VALUES(?,1,?,0)",
          [sessionId, child.hash],
        );
        nextEdge += 1;
        queued += 1;
      }
    } else {
      while (nextEdge < decoded.entries.length && queued < budget) {
        const entry = decoded.entries[nextEdge]!;
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_export_marks(session_id,kind,hash,edge) VALUES(?,2,?,0)",
          [sessionId, entry.hash],
        );
        nextEdge += 1;
        queued += 1;
      }
    }
    return {
      done:
        nextEdge >=
        (decoded.kind === "internal" ? decoded.children.length : decoded.entries.length),
      nextEdge,
    };
  }

  readExportBatch(options: {
    readonly sessionId: string;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
  }): Readonly<{
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
    readonly offered: number;
    readonly reused: number;
  }> {
    const exportRow = this.#exportRow(options.sessionId);
    const records: ReplicationTransferRecord[] = [];
    let offered = 0;
    let bytesUsed = 0;
    let budget = Math.max(1, Math.min(options.maxEntries, 8192));
    const byteBudget = Math.max(1024, options.maxBytes);
    let pending = this.#pendingMarks(options.sessionId, budget + 1);
    while (pending.length > 0 && records.length < budget && bytesUsed < byteBudget) {
      const mark = pending[0]!;
      if (mark.kind === 0) {
        const rows = this.#tx.all<
          {
            file_size: number;
            entry_count: number;
            root_node_hash: Uint8Array;
            encoded: Uint8Array;
          } & SqliteRow
        >(
          "SELECT file_size,entry_count,root_node_hash,encoded FROM efs_manifest_roots WHERE hash=?",
          [mark.hash],
          { maxRows: 1, maxBytes: 8192 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "export manifest root is missing");
        const row = rows[0]!;
        records.push(
          Object.freeze({
            kind: "manifest-root-descriptor",
            format: MANIFEST_FORMAT,
            digest: copyBytes(mark.hash),
            encodedLength: row.encoded.byteLength,
            logicalFileLength: row.file_size,
            entryCount: row.entry_count,
            rootNodeDigest: copyBytes(row.root_node_hash),
          }),
        );
        offered += 1;
        bytesUsed += 160;
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_export_marks(session_id,kind,hash,edge) VALUES(?,1,?,0)",
          [options.sessionId, row.root_node_hash],
        );
        this.#tx.run(
          "DELETE FROM efs_replication_export_marks WHERE session_id=? AND kind=0 AND hash=?",
          [options.sessionId, mark.hash],
        );
        this.#tx.run(
          "UPDATE efs_replication_exports SET root_count=root_count+1,offered_roots=offered_roots+1 WHERE session_id=?",
          [options.sessionId],
        );
      } else if (mark.kind === 1) {
        const rows = this.#tx.all<
          {
            kind: number;
            logical_bytes: number;
            entry_count: number;
            encoded: Uint8Array;
          } & SqliteRow
        >(
          "SELECT kind,logical_bytes,entry_count,encoded FROM efs_manifest_nodes WHERE hash=?",
          [mark.hash],
          { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 4096 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "export manifest node is missing");
        const row = rows[0]!;
        const children = this.#offerNodeChildren(
          options.sessionId,
          mark.hash,
          mark.edge,
          Math.max(1, budget - records.length),
        );
        if (!children.done) {
          this.#tx.run(
            "UPDATE efs_replication_export_marks SET edge=? WHERE session_id=? AND kind=1 AND hash=?",
            [children.nextEdge, options.sessionId, mark.hash],
          );
          pending = this.#pendingMarks(options.sessionId, budget + 1);
          continue;
        }
        const decoded = decodeManifestNode(row.encoded, mark.hash);
        records.push(
          Object.freeze({
            kind: "manifest-node-descriptor",
            digest: copyBytes(mark.hash),
            nodeKind: decoded.kind,
            encodedLength: row.encoded.byteLength,
            logicalSpan: row.logical_bytes,
            entryCount: row.entry_count,
          }),
        );
        offered += 1;
        bytesUsed += 128;
        this.#tx.run(
          "DELETE FROM efs_replication_export_marks WHERE session_id=? AND kind=1 AND hash=?",
          [options.sessionId, mark.hash],
        );
        this.#tx.run(
          "UPDATE efs_replication_exports SET node_count=node_count+1,offered_nodes=offered_nodes+1 WHERE session_id=?",
          [options.sessionId],
        );
      } else {
        const rows = this.#tx.all<{ size: number } & SqliteRow>(
          "SELECT size FROM efs_cas_objects WHERE hash=?",
          [mark.hash],
          { maxRows: 1, maxBytes: 256 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "export object is missing");
        records.push(
          Object.freeze({
            kind: "object-descriptor",
            digest: copyBytes(mark.hash),
            byteLength: rows[0]!.size,
          }),
        );
        offered += 1;
        bytesUsed += 64;
        this.#tx.run(
          "DELETE FROM efs_replication_export_marks WHERE session_id=? AND kind=2 AND hash=?",
          [options.sessionId, mark.hash],
        );
        this.#tx.run(
          "UPDATE efs_replication_exports SET object_count=object_count+1,object_bytes=object_bytes+? WHERE session_id=?",
          [rows[0]!.size, options.sessionId],
        );
      }
      pending = this.#pendingMarks(options.sessionId, budget + 1);
    }
    const state = this.#exportRow(options.sessionId);
    const complete =
      state.mark_hash === null &&
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_replication_export_marks WHERE session_id=?",
        [options.sessionId],
        { maxRows: 1, maxBytes: 256 },
      )[0]!.count === 0;
    return Object.freeze({ records, complete, offered, reused: 0 });
  }

  readExportPayloads(options: {
    readonly sessionId: string;
    readonly requested: readonly {
      readonly contentKind: "object" | "manifest-root" | "manifest-node";
      readonly digest: Uint8Array;
    }[];
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
  }): Readonly<{
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
  }> {
    const records: ReplicationTransferRecord[] = [];
    let bytesUsed = 0;
    for (const request of options.requested.slice(0, options.maxEntries)) {
      if (bytesUsed >= options.maxBytes) break;
      if (request.contentKind === "object") {
        const rows = this.#tx.all<{ bytes: Uint8Array; size: number } & SqliteRow>(
          "SELECT bytes,size FROM efs_cas_objects WHERE hash=?",
          [request.digest],
          { maxRows: 1, maxBytes: this.#limits.maxFinalTransactionBytes + 4096 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "requested export object is missing");
        const row = rows[0]!;
        if (row.bytes.byteLength !== row.size)
          throw transferError("ECORRUPT", "export object size mismatch");
        records.push(
          Object.freeze({
            kind: "object-payload",
            digest: copyBytes(request.digest),
            byteLength: row.bytes.byteLength,
            bytes: copyBytes(row.bytes),
          }),
        );
        bytesUsed += row.bytes.byteLength + 64;
      } else if (request.contentKind === "manifest-root") {
        const rows = this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
          "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
          [request.digest],
          { maxRows: 1, maxBytes: 8192 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "requested export manifest root is missing");
        records.push(
          Object.freeze({
            kind: "object-payload",
            digest: copyBytes(request.digest),
            byteLength: rows[0]!.encoded.byteLength,
            bytes: copyBytes(rows[0]!.encoded),
          }),
        );
        bytesUsed += rows[0]!.encoded.byteLength + 64;
      } else {
        const rows = this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
          "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
          [request.digest],
          { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes + 4096 },
        );
        if (rows.length !== 1)
          throw transferError("ECORRUPT", "requested export manifest node is missing");
        records.push(
          Object.freeze({
            kind: "object-payload",
            digest: copyBytes(request.digest),
            byteLength: rows[0]!.encoded.byteLength,
            bytes: copyBytes(rows[0]!.encoded),
          }),
        );
        bytesUsed += rows[0]!.encoded.byteLength + 64;
      }
    }
    return Object.freeze({ records, complete: true });
  }

  readExportStateBatch(options: {
    readonly sessionId: string;
    readonly flow: ReplicationFlow;
    readonly branchId: string | null;
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
    readonly checkpoint: boolean;
    readonly allowTerminal: boolean;
  }): Readonly<{
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
    readonly terminalResult: Readonly<{
      readonly operationId: string;
      readonly branchId: string | null;
      readonly generation: number;
      readonly generationDigest: Uint8Array;
      readonly resultBytes: Uint8Array;
    }> | null;
  }> {
    let exportRow = this.#exportRow(options.sessionId);
    if (exportRow.kind === 1) {
      const branchId = options.branchId!;
      const captureMetadata = decodeJson<Record<string, unknown>>(exportRow.meta_json) ?? {};
      if (captureMetadata.branchCaptureComplete !== true) {
        this.#snapshotBranchRows(options.sessionId, branchId, exportRow.selected_generation);
        exportRow = this.#exportRow(options.sessionId);
      }
      const liveRows = this.#tx.all<BranchRowSql & SqliteRow>(
        "SELECT base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision FROM efs_branches WHERE id=?",
        [branchId],
        { maxRows: 1, maxBytes: 2048 },
      );
      if (!liveRows[0]) throw transferError("ECORRUPT", "export branch is missing");
      const selected = decodeJson<{
        readonly branchState?: number;
        readonly branchBaseRevision?: number;
        readonly branchGenerationDigest?: string;
        readonly branchPreviousGeneration?: number | null;
        readonly branchPreviousGenerationDigest?: string | null;
      }>(exportRow.meta_json);
      const selectedState = selected?.branchState;
      const selectedBase = selected?.branchBaseRevision;
      const selectedDigest = selected?.branchGenerationDigest;
      const selectedPreviousGeneration = selected?.branchPreviousGeneration ?? null;
      const selectedPreviousDigest = selected?.branchPreviousGenerationDigest ?? null;
      if (
        (selectedState !== 0 && selectedState !== 1 && selectedState !== 2) ||
        !Number.isSafeInteger(selectedBase) ||
        typeof selectedDigest !== "string" ||
        !/^[0-9a-f]{64}$/u.test(selectedDigest) ||
        (selectedPreviousGeneration !== null &&
          (!Number.isSafeInteger(selectedPreviousGeneration) || selectedPreviousGeneration < 0)) ||
        (selectedPreviousDigest !== null && !/^[0-9a-f]{64}$/u.test(selectedPreviousDigest)) ||
        (selectedPreviousGeneration === null) !== (selectedPreviousDigest === null)
      )
        throw transferError("IntegrityFailure", "branch export snapshot metadata is invalid");
      if (selectedState !== 0 && !options.allowTerminal)
        throw transferError("UnauthorizedScope", "terminal branch export is not allowed here");
      const snapshotRows = this.#tx.all<{ row_index: number; kind: number; row_key: Uint8Array; value: Uint8Array } & SqliteRow>(
        "SELECT row_index,kind,row_key,value FROM efs_replication_export_rows WHERE session_id=? AND row_index>? ORDER BY row_index LIMIT ?",
        [options.sessionId, exportRow.revision_cursor, Math.min(options.maxEntries, 256)],
        { maxRows: Math.min(options.maxEntries, 256), maxBytes: options.maxBytes + 8192 },
      );
      const branchRows: TransferBranchRow[] = [];
      let nextCursor = exportRow.revision_cursor;
      for (const row of snapshotRows) {
        const decoded = decodeBranchSnapshotRow(row.kind, row.row_key, row.value);
        const candidate = encodeBranchGenerationFragment({
          branchId,
          baseRevision: String(selectedBase),
          generation: exportRow.selected_generation,
          generationDigest: hexBytes(selectedDigest),
          previousGeneration: selectedPreviousGeneration,
          previousGenerationDigest:
            selectedPreviousDigest === null ? null : hexBytes(selectedPreviousDigest),
          state: selectedState as 0 | 1 | 2,
          rows: [...branchRows, decoded],
        });
        if (candidate.byteLength > options.maxBytes && branchRows.length > 0) break;
        if (candidate.byteLength > options.maxBytes)
          throw transferError("ResourceLimit", "one branch snapshot row exceeds the negotiated batch limit");
        branchRows.push(decoded);
        nextCursor = row.row_index;
      }
      this.#tx.run(
        "UPDATE efs_replication_exports SET revision_cursor=?,state_rows=state_rows+? WHERE session_id=?",
        [nextCursor, branchRows.length, options.sessionId],
      );
      const captureComplete = (decodeJson<Record<string, unknown>>(exportRow.meta_json) ?? {}).branchCaptureComplete === true;
      const complete = captureComplete && this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_replication_export_rows WHERE session_id=? AND row_index>?",
        [options.sessionId, nextCursor],
        { maxRows: 1, maxBytes: 256 },
      )[0]!.count === 0;
      const digest = hexBytes(selectedDigest);
      const fragment = encodeBranchGenerationFragment({
        branchId,
        baseRevision: String(selectedBase),
        generation: exportRow.selected_generation,
        generationDigest: digest,
        previousGeneration: selectedPreviousGeneration,
        previousGenerationDigest:
          selectedPreviousDigest === null ? null : hexBytes(selectedPreviousDigest),
        state: selectedState as 0 | 1 | 2,
        rows: branchRows,
      });
      let terminalResult: Readonly<{
        readonly operationId: string;
        readonly branchId: string | null;
        readonly generation: number;
        readonly generationDigest: Uint8Array;
        readonly resultBytes: Uint8Array;
      }> | null = null;
      if (complete && selectedState !== 0) {
        const result = this.#tx.all<BranchResultSql & SqliteRow>(
          "SELECT i.id operation_id,i.branch_id,i.generation,i.reservation_nonce,coalesce(r.outcome,-1) outcome,r.encoded,r.expires_at_ms FROM efs_operation_ids i LEFT JOIN efs_operation_results r ON r.operation_id=i.id WHERE i.branch_id=? AND i.generation<=? ORDER BY i.generation DESC,i.id LIMIT 1",
          [branchId, exportRow.selected_generation],
          { maxRows: 1, maxBytes: 65536 },
        )[0];
        if (result && result.encoded) {
          const digestBytes = this.#hashBytes(result.encoded);
          terminalResult = {
            operationId: result.operation_id,
            branchId,
            generation: result.generation,
            generationDigest: copyBytes(digest),
            resultBytes: copyBytes(result.encoded),
          };
        }
      }
      return Object.freeze({
        records: [
          Object.freeze({
            kind: "branch-generation-fragment",
            branchId,
            baseRevision: String(selectedBase),
            generation: exportRow.selected_generation,
            generationDigest: digest,
            fragmentIndex: 0,
            fragmentCount: 1,
            fragmentBytes: fragment,
          }),
        ],
        complete,
        terminalResult: complete ? terminalResult : null,
      });
    }
    if (exportRow.kind === 2) {
      return Object.freeze({
        records: this.#readGenesisState(options.sessionId, options.maxEntries, options.maxBytes),
        complete: (decodeJson<Record<string, unknown>>(this.#exportRow(options.sessionId).meta_json) ?? {}).genesisComplete === true,
        terminalResult: null,
      });
    }
    const records = this.#readRevisionState(
      options.sessionId,
      options.flow,
      options.maxEntries,
      options.maxBytes,
      options.checkpoint,
    );
    const state = this.#exportRow(options.sessionId);
    const stateMetadata = decodeJson<Record<string, unknown>>(state.meta_json) ?? {};
    return Object.freeze({
      records,
      complete:
        state.revision_cursor >= state.target_revision &&
        stateMetadata.stateCursor === undefined,
      terminalResult: null,
    });
  }

  #readGenesisState(
    sessionId: string,
    maxEntries: number,
    maxBytes: number,
  ): ReplicationTransferRecord[] {
    const exportRow = this.#exportRow(sessionId);
    const metadata = decodeJson<Record<string, unknown>>(exportRow.meta_json) ?? {};
    const raw = metadata.genesisCursor as Partial<RevisionStateCursor> | undefined;
    const cursor = {
      inodeId: typeof raw?.inodeId === "string" ? raw.inodeId : null,
      fragmentIndex: raw && Number.isSafeInteger(raw.fragmentIndex) ? raw.fragmentIndex ?? 0 : 0,
    };
    const limit = Math.max(1, Math.min(maxEntries, 256));
    const rows = this.#tx.all<{ inode_id: string; tombstone: number; encoded: Uint8Array | null } & SqliteRow>(
      cursor.inodeId === null
        ? "SELECT inode_id,tombstone,encoded FROM efs_inode_revisions WHERE revision=0 ORDER BY inode_id LIMIT ?"
        : "SELECT inode_id,tombstone,encoded FROM efs_inode_revisions WHERE revision=0 AND inode_id>? ORDER BY inode_id LIMIT ?",
      cursor.inodeId === null ? [limit + 1] : [cursor.inodeId, limit + 1],
      { maxRows: limit + 1, maxBytes: maxBytes + 8192 },
    );
    const namespaceRows: TransferNamespaceRow[] = rows.slice(0, limit).map((row) => ({
      kind: 1,
      inodeId: row.inode_id,
      tombstone: row.tombstone === 1,
      encoded: row.encoded ? copyBytes(row.encoded) : null,
    }));
    if (namespaceRows.length === 0 && rows.length > 0)
      throw transferError("ResourceLimit", "one genesis inode exceeds the negotiated batch limit");
    const header = this.#tx.all<RevisionHeaderRow & SqliteRow>(
      "SELECT revision,parent_revision,created_at_ms,writer_id,change_count FROM efs_revisions WHERE revision=0",
      [], { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!header) throw transferError("ECORRUPT", "genesis revision is missing");
    const fragmentBytes = encodeRevisionFragment({
      revisionId: "0",
      parentRevisionId: null,
      created_at_ms: header.created_at_ms,
      writerId: header.writer_id,
      changeCount: header.change_count,
      rows: namespaceRows,
    });
    if (fragmentBytes.byteLength > maxBytes)
      throw transferError("ResourceLimit", "genesis fragment exceeds the negotiated batch limit");
    const complete = rows.length <= limit;
    const lastRow = namespaceRows.at(-1);
    const nextCursor = complete ? undefined : {
      inodeId: lastRow?.kind === 1 ? lastRow.inodeId : null,
      fragmentIndex: cursor.fragmentIndex + 1,
    };
    this.#tx.run(
      "UPDATE efs_replication_exports SET state_rows=state_rows+?,meta_json=? WHERE session_id=?",
      [namespaceRows.length, encodeJson({ ...metadata, ...(complete ? { genesisComplete: true, genesisCursor: undefined } : { genesisCursor: nextCursor }) }), sessionId],
    );
    return [Object.freeze({
      kind: "revision-fragment" as const,
      checkpointId: "0",
      revisionId: "0",
      parentRevisionId: null,
      fragmentIndex: cursor.fragmentIndex,
      fragmentCount: cursor.fragmentIndex + 1,
      fragmentBytes,
    })];
  }

  #storedBranchDigest(sessionId: string, branchId: string, generation: number): Uint8Array {
    void sessionId;
    if (this.#branchDigest) return hexBytes(this.#branchDigest(branchId, generation));
    const digestRows = this.#branches().terminalGenerationDigest(branchId, generation);
    return digestRows ? hexBytes(digestRows) : ZERO_DIGEST;
  }

  #readNamespaceRows(
    sessionId: string,
    revision: number,
    maxEntries: number,
    maxBytes: number,
    checkpoint: boolean,
  ): readonly TransferNamespaceRow[] {
    const rows: TransferNamespaceRow[] = [];
    let bytesUsed = 0;
    let entries = maxEntries;
    const inodeTable = checkpoint ? "efs_checkpoint_inodes" : "efs_inode_revisions";
    const entryTable = checkpoint ? "efs_checkpoint_entries" : "efs_entry_revisions";
    const refTable = checkpoint
      ? "efs_checkpoint_manifest_roots"
      : "efs_revision_manifest_roots";
    const inodes = this.#tx.all<{ inode_id: string; tombstone: number; encoded: Uint8Array | null } & SqliteRow>(
      `SELECT inode_id,tombstone,encoded FROM ${inodeTable} WHERE ${checkpoint ? "target_revision" : "revision"}=? ORDER BY inode_id LIMIT ?`,
      [revision, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of inodes) {
      rows.push({
        kind: 1,
        inodeId: row.inode_id,
        tombstone: row.tombstone === 1,
        encoded: row.encoded ? copyBytes(row.encoded) : null,
      });
      bytesUsed += 32 + encoder.encode(row.inode_id).byteLength + (row.encoded?.byteLength ?? 0);
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const entryRows = this.#tx.all<{ parent_inode: string; name_sort: Uint8Array; tombstone: number; encoded: Uint8Array | null } & SqliteRow>(
      `SELECT parent_inode,name_sort,tombstone,encoded FROM ${entryTable} WHERE ${checkpoint ? "target_revision" : "revision"}=? ORDER BY parent_inode,name_sort LIMIT ?`,
      [revision, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of entryRows) {
      rows.push({
        kind: 2,
        parentInode: row.parent_inode,
        nameSort: copyBytes(row.name_sort),
        tombstone: row.tombstone === 1,
        encoded: row.encoded ? copyBytes(row.encoded) : null,
      });
      bytesUsed += 32 + row.name_sort.byteLength + (row.encoded?.byteLength ?? 0);
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const refs = this.#tx.all<{ inode_id: string; manifest_hash: Uint8Array } & SqliteRow>(
      `SELECT inode_id,manifest_hash FROM ${refTable} WHERE ${checkpoint ? "target_revision" : "revision"}=? ORDER BY inode_id LIMIT ?`,
      [revision, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of refs) {
      rows.push({ kind: 3, inodeId: row.inode_id, manifestHash: copyBytes(row.manifest_hash) });
      bytesUsed += 64;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    return rows;
  }

  #readBranchRows(
    sessionId: string,
    branchId: string,
    generation: number,
    maxEntries: number,
    maxBytes: number,
  ): readonly TransferBranchRow[] {
    const rows: TransferBranchRow[] = [];
    let bytesUsed = 0;
    let entries = maxEntries;
    const changes = this.#tx.all<{ path: Uint8Array; expected_token: number | null; kind: number; encoded: Uint8Array | null } & SqliteRow>(
      "SELECT path,expected_token,kind,encoded FROM efs_branch_changes WHERE branch_id=? ORDER BY path LIMIT ?",
      [branchId, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of changes) {
      rows.push({
        kind: 1,
        path: copyBytes(row.path),
        disposition: row.kind,
        expectedToken: row.expected_token,
        encoded: row.encoded ? copyBytes(row.encoded) : null,
      });
      bytesUsed += 64 + row.path.byteLength + (row.encoded?.byteLength ?? 0);
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const overlays = this.#tx.all<{ inode_id: string; expected_token: number | null; encoded: Uint8Array } & SqliteRow>(
      "SELECT inode_id,expected_token,encoded FROM efs_branch_inode_overlays WHERE branch_id=? ORDER BY inode_id LIMIT ?",
      [branchId, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of overlays) {
      rows.push({
        kind: 2,
        inodeId: row.inode_id,
        expectedToken: row.expected_token,
        encoded: copyBytes(row.encoded),
      });
      bytesUsed += 64 + row.encoded.byteLength;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const pages = this.#tx.all<{ inode_id: string; page_index: number; generation: number; bytes: Uint8Array; created_at_ms: number; head: number } & SqliteRow>(
      "SELECT v.inode_id,v.page_index,v.generation,v.bytes,v.created_at_ms,EXISTS(SELECT 1 FROM efs_cow_page_heads h WHERE h.branch_id=v.branch_id AND h.inode_id=v.inode_id AND h.page_index=v.page_index AND h.generation=v.generation) head FROM efs_cow_page_versions v WHERE v.branch_id=? AND v.generation<=? ORDER BY v.inode_id,v.page_index,v.generation LIMIT ?",
      [branchId, generation, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of pages) {
      rows.push({
        kind: 3,
        inodeId: row.inode_id,
        pageIndex: row.page_index,
        generation: row.generation,
        bytes: copyBytes(row.bytes),
        created_at_ms: row.created_at_ms,
        head: row.head === 1,
      });
      bytesUsed += 96 + row.bytes.byteLength;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const patches = this.#tx.all<{ inode_id: string; sequence: number; generation: number; offset: number; delete_length: number; insert_length: number } & SqliteRow>(
      "SELECT inode_id,sequence,generation,offset,delete_length,insert_length FROM efs_patches WHERE branch_id=? AND generation<=? ORDER BY inode_id,sequence LIMIT ?",
      [branchId, generation, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of patches) {
      const segments = this.#tx.all<{ segment_index: number; bytes: Uint8Array } & SqliteRow>(
        "SELECT segment_index,bytes FROM efs_patch_segments WHERE branch_id=? AND inode_id=? AND sequence=? ORDER BY segment_index",
        [branchId, row.inode_id, row.sequence],
        { maxRows: 256, maxBytes: maxBytes + 8192 },
      );
      rows.push({
        kind: 4,
        inodeId: row.inode_id,
        sequence: row.sequence,
        generation: row.generation,
        offset: row.offset,
        deleteLength: row.delete_length,
        insertLength: row.insert_length,
        segments: segments.map((segment) => copyBytes(segment.bytes)),
      });
      bytesUsed += 96;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const expectations = this.#tx.all<{ inode_id: string; expected_token: number | null } & SqliteRow>(
      "SELECT inode_id,expected_token FROM efs_branch_inode_expectations WHERE branch_id=? ORDER BY inode_id LIMIT ?",
      [branchId, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of expectations) {
      rows.push({ kind: 5, inodeId: row.inode_id, expectedToken: row.expected_token });
      bytesUsed += 32;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    const refs = this.#tx.all<{ path: Uint8Array; manifest_hash: Uint8Array } & SqliteRow>(
      "SELECT path,manifest_hash FROM efs_branch_manifest_roots WHERE branch_id=? ORDER BY path LIMIT ?",
      [branchId, entries],
      { maxRows: entries, maxBytes: maxBytes + 8192 },
    );
    for (const row of refs) {
      rows.push({ kind: 6, path: copyBytes(row.path), manifestHash: copyBytes(row.manifest_hash) });
      bytesUsed += 64;
      entries -= 1;
      if (bytesUsed >= maxBytes || entries <= 0) return rows;
    }
    return rows;
  }

  #readNamespaceRowsPage(
    cursor: RevisionStateCursor,
    maxEntries: number,
    maxBytes: number,
    checkpoint: boolean,
  ): Readonly<{
    readonly rows: readonly TransferNamespaceRow[];
    readonly nextCursor: RevisionStateCursor;
    readonly revisionComplete: boolean;
  }> {
    const rows: TransferNamespaceRow[] = [];
    const limit = Math.max(1, Math.min(maxEntries, 256));
    const keyColumn = checkpoint ? "target_revision" : "revision";
    const inodeTable = checkpoint ? "efs_checkpoint_inodes" : "efs_inode_revisions";
    const entryTable = checkpoint ? "efs_checkpoint_entries" : "efs_entry_revisions";
    const refTable = checkpoint ? "efs_checkpoint_manifest_roots" : "efs_revision_manifest_roots";
    let fetched: readonly SqliteRow[] = [];
    if (cursor.kind === 1) {
      fetched = this.#tx.all<{ inode_id: string; tombstone: number; encoded: Uint8Array | null } & SqliteRow>(
        cursor.inodeId === null
          ? `SELECT inode_id,tombstone,encoded FROM ${inodeTable} WHERE ${keyColumn}=? ORDER BY inode_id LIMIT ?`
          : `SELECT inode_id,tombstone,encoded FROM ${inodeTable} WHERE ${keyColumn}=? AND inode_id>? ORDER BY inode_id LIMIT ?`,
        cursor.inodeId === null ? [cursor.revision, limit + 1] : [cursor.revision, cursor.inodeId, limit + 1],
        { maxRows: limit + 1, maxBytes: maxBytes + 8192 },
      );
      for (const row of fetched as readonly { inode_id: string; tombstone: number; encoded: Uint8Array | null }[]) {
        const size = 32 + encoder.encode(row.inode_id).byteLength + (row.encoded?.byteLength ?? 0);
        if (rows.length >= limit || (rows.length > 0 && size + rows.reduce((total, item) => total + 32 + (item.kind === 1 ? encoder.encode(item.inodeId).byteLength : 0), 0) > maxBytes)) break;
        if (size > maxBytes && rows.length === 0) throw transferError("ResourceLimit", "one inode row exceeds the negotiated batch limit");
        rows.push({ kind: 1, inodeId: row.inode_id, tombstone: row.tombstone === 1, encoded: row.encoded ? copyBytes(row.encoded) : null });
      }
      const hasMore = fetched.length > rows.length;
      const last = rows.at(-1);
      return Object.freeze({
        rows,
        nextCursor: hasMore && last !== undefined && last.kind === 1
          ? { ...cursor, inodeId: last.inodeId }
          : { revision: cursor.revision, kind: 2 as const, inodeId: null, parentInode: null, nameSortHex: null, fragmentIndex: cursor.fragmentIndex },
        revisionComplete: false,
      });
    }
    if (cursor.kind === 2) {
      fetched = this.#tx.all<{ parent_inode: string; name_sort: Uint8Array; tombstone: number; encoded: Uint8Array | null } & SqliteRow>(
        cursor.parentInode === null
          ? `SELECT parent_inode,name_sort,tombstone,encoded FROM ${entryTable} WHERE ${keyColumn}=? ORDER BY parent_inode,name_sort LIMIT ?`
          : `SELECT parent_inode,name_sort,tombstone,encoded FROM ${entryTable} WHERE ${keyColumn}=? AND (parent_inode>? OR (parent_inode=? AND name_sort>?)) ORDER BY parent_inode,name_sort LIMIT ?`,
        cursor.parentInode === null
          ? [cursor.revision, limit + 1]
          : [cursor.revision, cursor.parentInode, cursor.parentInode, hexBytes(cursor.nameSortHex ?? ""), limit + 1],
        { maxRows: limit + 1, maxBytes: maxBytes + 8192 },
      );
      for (const row of fetched as readonly { parent_inode: string; name_sort: Uint8Array; tombstone: number; encoded: Uint8Array | null }[]) {
        const size = 32 + row.name_sort.byteLength + (row.encoded?.byteLength ?? 0);
        if (rows.length >= limit || (rows.length > 0 && size > maxBytes)) break;
        if (size > maxBytes && rows.length === 0) throw transferError("ResourceLimit", "one entry row exceeds the negotiated batch limit");
        rows.push({ kind: 2, parentInode: row.parent_inode, nameSort: copyBytes(row.name_sort), tombstone: row.tombstone === 1, encoded: row.encoded ? copyBytes(row.encoded) : null });
      }
      const hasMore = fetched.length > rows.length;
      const last = rows.at(-1);
      return Object.freeze({
        rows,
        nextCursor: hasMore && last !== undefined && last.kind === 2
          ? { ...cursor, parentInode: last.parentInode, nameSortHex: bytesToHex(last.nameSort) }
          : { revision: cursor.revision, kind: 3 as const, inodeId: null, parentInode: null, nameSortHex: null, fragmentIndex: cursor.fragmentIndex },
        revisionComplete: false,
      });
    }
    fetched = this.#tx.all<{ inode_id: string; manifest_hash: Uint8Array } & SqliteRow>(
      cursor.inodeId === null
        ? `SELECT inode_id,manifest_hash FROM ${refTable} WHERE ${keyColumn}=? ORDER BY inode_id LIMIT ?`
        : `SELECT inode_id,manifest_hash FROM ${refTable} WHERE ${keyColumn}=? AND inode_id>? ORDER BY inode_id LIMIT ?`,
      cursor.inodeId === null ? [cursor.revision, limit + 1] : [cursor.revision, cursor.inodeId, limit + 1],
      { maxRows: limit + 1, maxBytes: maxBytes + 8192 },
    );
    for (const row of fetched as readonly { inode_id: string; manifest_hash: Uint8Array }[]) {
      if (rows.length >= limit) break;
      if (rows.length > 0 && (rows.length + 1) * 64 > maxBytes) break;
      if (64 > maxBytes && rows.length === 0) throw transferError("ResourceLimit", "one manifest reference exceeds the negotiated batch limit");
      rows.push({ kind: 3, inodeId: row.inode_id, manifestHash: copyBytes(row.manifest_hash) });
    }
    const hasMore = fetched.length > rows.length;
    const last = rows.at(-1);
    return Object.freeze({
      rows,
      nextCursor: hasMore && last !== undefined && last.kind === 3
        ? { ...cursor, inodeId: last.inodeId }
        : { revision: cursor.revision + 1, kind: 1 as const, inodeId: null, parentInode: null, nameSortHex: null, fragmentIndex: 0 },
      revisionComplete: !hasMore && rows.length === fetched.length,
    });
  }

  #readRevisionState(
    sessionId: string,
    flow: ReplicationFlow,
    maxEntries: number,
    maxBytes: number,
    checkpoint: boolean,
  ): ReplicationTransferRecord[] {
    const exportRow = this.#exportRow(sessionId);
    const metadata = decodeJson<Record<string, unknown>>(exportRow.meta_json) ?? {};
    const raw = metadata.stateCursor as Partial<RevisionStateCursor> | undefined;
    let cursor: RevisionStateCursor = {
      revision: raw && Number.isSafeInteger(raw.revision)
        ? raw.revision ?? Math.max(exportRow.revision_cursor + 1, exportRow.base_revision + 1)
        : Math.max(exportRow.revision_cursor + 1, exportRow.base_revision + 1),
      kind: raw?.kind === 2 ? 2 : raw?.kind === 3 ? 3 : 1,
      inodeId: typeof raw?.inodeId === "string" ? raw.inodeId : null,
      parentInode: typeof raw?.parentInode === "string" ? raw.parentInode : null,
      nameSortHex: typeof raw?.nameSortHex === "string" ? raw.nameSortHex : null,
      fragmentIndex: raw && Number.isSafeInteger(raw.fragmentIndex) ? raw.fragmentIndex ?? 0 : 0,
    };
    const records: ReplicationTransferRecord[] = [];
    let bytesUsed = 0;
    let emitted = 0;
    while (cursor.revision <= exportRow.target_revision && emitted < maxEntries && bytesUsed < maxBytes) {
      const headers = this.#tx.all<RevisionHeaderRow & SqliteRow>(
        "SELECT revision,parent_revision,created_at_ms,writer_id,change_count FROM efs_revisions WHERE revision=?",
        [cursor.revision], { maxRows: 1, maxBytes: 4096 },
      );
      if (headers.length !== 1) throw transferError("ECORRUPT", "export revision is missing");
      const header = headers[0]!;
      const page = this.#readNamespaceRowsPage(
        cursor, Math.max(1, Math.min(maxEntries - emitted, 256)), maxBytes - bytesUsed, checkpoint,
      );
      if (page.rows.length === 0) {
        cursor = page.nextCursor;
        const durableCursor = cursor.revision > exportRow.target_revision ? undefined : cursor;
        this.#tx.run(
          "UPDATE efs_replication_exports SET revision_cursor=?,meta_json=? WHERE session_id=?",
          [page.revisionComplete ? cursor.revision - 1 : exportRow.revision_cursor, encodeJson({ ...metadata, ...(durableCursor === undefined ? { stateCursor: undefined } : { stateCursor: durableCursor }) }), sessionId],
        );
        continue;
      }
      const fragmentBytes = checkpoint
        ? encodeCheckpointFragment({ revisionId: String(cursor.revision), rows: page.rows })
        : encodeRevisionFragment({
            revisionId: String(cursor.revision),
            parentRevisionId: header.parent_revision === null ? null : String(header.parent_revision),
            created_at_ms: header.created_at_ms,
            writerId: header.writer_id,
            changeCount: header.change_count,
            rows: page.rows,
          });
      if (fragmentBytes.byteLength > maxBytes - bytesUsed)
        throw transferError("ResourceLimit", "one revision fragment exceeds the negotiated batch limit");
      records.push(Object.freeze({
        kind: checkpoint ? ("checkpoint-fragment" as const) : ("revision-fragment" as const),
        checkpointId: String(cursor.revision),
        revisionId: String(cursor.revision),
        parentRevisionId: header.parent_revision === null ? null : String(header.parent_revision),
        fragmentIndex: cursor.fragmentIndex,
        fragmentCount: cursor.fragmentIndex + 1,
        fragmentBytes,
      }));
      emitted += 1;
      bytesUsed += fragmentBytes.byteLength;
      const next = page.revisionComplete
        ? {
            revision: cursor.revision + 1,
            kind: 1 as const,
            inodeId: null,
            parentInode: null,
            nameSortHex: null,
            fragmentIndex: 0,
          }
        : { ...page.nextCursor, fragmentIndex: cursor.fragmentIndex + 1 };
      cursor = next;
      const durableCursor = cursor.revision > exportRow.target_revision ? undefined : cursor;
      this.#tx.run(
        "UPDATE efs_replication_exports SET revision_cursor=?,state_rows=state_rows+?,meta_json=? WHERE session_id=?",
        [page.revisionComplete ? cursor.revision - 1 : exportRow.revision_cursor, page.rows.length, encodeJson({ ...metadata, ...(durableCursor === undefined ? { stateCursor: undefined } : { stateCursor: durableCursor }) }), sessionId],
      );
      if (page.revisionComplete && cursor.revision > exportRow.target_revision) break;
    }
    void flow;
    return records;
  }

  exportSummary(options: {
    readonly sessionId: string;
    readonly flow: ReplicationFlow;
  }): Readonly<{
    readonly selectedRevision: number;
    readonly selectedGeneration: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly baseRevision: number;
    readonly rootCount: number;
    readonly nodeCount: number;
    readonly objectCount: number;
    readonly objectBytes: number;
    readonly stateRows: number;
    readonly complete: boolean;
  }> {
    const exportRow = this.#exportRow(options.sessionId);
    return Object.freeze({
      selectedRevision: exportRow.target_revision,
      selectedGeneration: exportRow.kind === 1 ? exportRow.selected_generation : null,
      generationDigest:
        exportRow.kind === 1
          ? (() => {
              const meta = decodeJson<{ readonly branchGenerationDigest?: string }>(exportRow.meta_json);
              return meta?.branchGenerationDigest && /^[0-9a-f]{64}$/u.test(meta.branchGenerationDigest)
                ? hexBytes(meta.branchGenerationDigest)
                : null;
            })()
          : null,
      baseRevision: exportRow.base_revision,
      rootCount: exportRow.root_count,
      nodeCount: exportRow.node_count,
      objectCount: exportRow.object_count,
      objectBytes: exportRow.object_bytes,
      stateRows: exportRow.state_rows,
      complete: exportRow.done === 1,
    });
  }

  beginImport(options: {
    readonly sessionId: string;
    readonly kind: 0 | 1 | 2;
    readonly leaseId: string;
    readonly ownerNonce: Uint8Array;
    readonly branchId: string | null;
    readonly baseRevision: number | null;
    readonly generation: number | null;
    readonly expectedGenerationDigest: Uint8Array | null;
    readonly now: number;
    readonly expiresAt: number;
    readonly ingestReservationBytes: number;
    readonly metadataReservationBytes: number;
    readonly resultRetentionMs?: number;
  }): void {
    if (options.resultRetentionMs !== undefined) {
      if (!Number.isSafeInteger(options.resultRetentionMs) || options.resultRetentionMs <= 0)
        throw new RangeError("resultRetentionMs is invalid");
      this.#resultRetentionMs = options.resultRetentionMs;
    }
    if (options.ownerNonce.byteLength !== 16)
      throw new RangeError("import owner nonce must contain 16 bytes");
    const existing = this.#tx.all<{ lease_id: string } & SqliteRow>(
      "SELECT lease_id FROM efs_replication_imports WHERE session_id=?",
      [options.sessionId],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (existing && existing.lease_id !== options.leaseId)
      throw transferError("CursorMismatch", "import lease identity changed");
    if (existing) {
      const lease = this.#tx.all<{
        owner_nonce: Uint8Array;
        state: number;
        expires_at_ms: number;
      } & SqliteRow>(
        "SELECT owner_nonce,state,expires_at_ms FROM efs_leases WHERE id=?",
        [options.leaseId],
        { maxRows: 1, maxBytes: 1024 },
      )[0];
      if (!lease)
        throw transferError("IntegrityFailure", "replication import lease is missing");
      if (!equalBytes(lease.owner_nonce, options.ownerNonce))
        throw transferError("CursorMismatch", "import owner nonce mismatch");
      if (lease.state !== 0 || lease.expires_at_ms <= options.now)
        throw transferError("StagingExpired", "replication import lease is not active");
      this.#tx.run(
        "UPDATE efs_leases SET last_renewal_at_ms=?,expires_at_ms=? WHERE id=? AND owner_nonce=? AND state=0",
        [options.now, Math.max(lease.expires_at_ms, options.expiresAt), options.leaseId, options.ownerNonce],
      );
      return;
    }
    this.#staging()
      .begin({
        leaseId: options.leaseId,
        ownerId: `replication:${options.sessionId}`,
        ownerNonce: options.ownerNonce,
        now: options.now,
        expiresAt: options.expiresAt,
        kind: 2,
        ...(options.branchId === null ? {} : { branchId: options.branchId }),
        ...(options.generation === null ? {} : { generation: options.generation }),
        ingestReservationBytes: options.ingestReservationBytes,
        metadataReservationBytes: options.metadataReservationBytes,
      });
    this.#tx.run(
      "INSERT INTO efs_replication_imports(session_id,lease_id,owner_nonce,kind,phase,branch_id,base_revision,generation,expected_generation_digest,closure_object_count,closure_object_bytes,closure_root_count,closure_node_count,transferred_object_count,transferred_object_bytes,transferred_root_count,transferred_node_count,state_row_count,state_byte_count,revision_count,installed_revision_count,sealed) VALUES(?,?,?,?,0,?,?,?,?,0,0,0,0,0,0,0,0,0,0,0,0,0) ON CONFLICT DO NOTHING",
      [
        options.sessionId,
        options.leaseId,
        options.ownerNonce,
        options.kind,
        options.branchId,
        options.baseRevision,
        options.generation,
        options.expectedGenerationDigest ?? null,
      ],
    );
  }

  readMissingContent(options: {
    readonly sessionId: string;
    readonly maxEntries: number;
    readonly maxBytes: number;
  }): Readonly<{
    readonly records: readonly ReplicationTransferRecord[];
    readonly complete: boolean;
  }> {
    const rows = this.#tx.all<StagedRow>(
      "SELECT key,value FROM efs_replication_import_rows WHERE session_id=? AND kind=0 ORDER BY key LIMIT ?",
      [options.sessionId, options.maxEntries],
      { maxRows: options.maxEntries, maxBytes: options.maxBytes + 8192 },
    );
    const records: ReplicationTransferRecord[] = [];
    for (const row of rows) {
      const kindByte = row.value?.[0] ?? 0;
      const contentKind =
        kindByte === 1
          ? ("manifest-root" as const)
          : kindByte === 2
            ? ("manifest-node" as const)
            : ("object" as const);
      records.push(
        Object.freeze({
          kind: "missing-content",
          contentKind,
          digest: copyBytes(row.key),
        }),
      );
    }
    return Object.freeze({
      records,
      complete: rows.length < options.maxEntries,
    });
  }

  applyImportRecords(options: {
    readonly sessionId: string;
    readonly records: readonly ReplicationTransferRecord[];
    readonly now: number;
  }): Readonly<{
    readonly stagedBytesDelta: number;
    readonly insertedObjects: number;
    readonly reusedObjects: number;
    readonly insertedNodes: number;
    readonly reusedNodes: number;
    readonly insertedRoots: number;
    readonly reusedRoots: number;
    readonly missingCount: number;
    readonly transferredCount: number;
  }> {
    const importRow = this.#importRow(options.sessionId);
    const leaseId = importRow.lease_id;
    const ownerNonce = copyBytes(importRow.owner_nonce);
    let stagedBytesDelta = 0;
    let insertedObjects = 0;
    let reusedObjects = 0;
    let insertedNodes = 0;
    let reusedNodes = 0;
    let insertedRoots = 0;
    let reusedRoots = 0;
    let missingCount = 0;
    let transferredCount = 0;
    const members: {
      readonly kind: "object" | "manifest-root" | "manifest-node";
      readonly hash: Uint8Array;
      readonly size: number;
    }[] = [];
    const content = this.#content();
    for (const record of options.records) {
      if (record.kind === "object-descriptor") {
        const present = this.#tx.all<{ size: number } & SqliteRow>(
          "SELECT size FROM efs_cas_objects WHERE hash=?",
          [record.digest],
          { maxRows: 1, maxBytes: 256 },
        )[0];
        if (present && present.size === record.byteLength) {
          reusedObjects += 1;
          members.push({
            kind: "object",
            hash: copyBytes(record.digest),
            size: record.byteLength,
          });
          continue;
        }
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,0,?,?)",
          [
            options.sessionId,
            record.digest,
            new Uint8Array([0, ...u64be(record.byteLength)]),
          ],
        );
        missingCount += 1;
      } else if (record.kind === "manifest-root-descriptor") {
        const present = this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
          "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
          [record.digest],
          { maxRows: 1, maxBytes: 8192 },
        )[0];
        let valid = false;
        if (present) {
          try {
            const root = decodeManifestRoot(present.encoded, record.digest);
            valid =
              root.fileSize === record.logicalFileLength &&
              root.entryCount === record.entryCount &&
              equalBytes(root.rootNodeHash, record.rootNodeDigest);
          } catch {
            valid = false;
          }
        }
        if (valid) {
          reusedRoots += 1;
          members.push({
            kind: "manifest-root",
            hash: copyBytes(record.digest),
            size: record.encodedLength,
          });
          continue;
        }
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,0,?,?)",
          [
            options.sessionId,
            record.digest,
            new Uint8Array([1, ...u64be(record.encodedLength)]),
          ],
        );
        missingCount += 1;
      } else if (record.kind === "manifest-node-descriptor") {
        const present = this.#tx.all<{ kind: number; encoded: Uint8Array } & SqliteRow>(
          "SELECT kind,encoded FROM efs_manifest_nodes WHERE hash=?",
          [record.digest],
          { maxRows: 1, maxBytes: 8192 },
        )[0];
        let valid = false;
        if (present) {
          try {
            const node = decodeManifestNode(present.encoded, record.digest);
            valid =
              node.span === record.logicalSpan &&
              node.entryCount === record.entryCount &&
              node.kind === record.nodeKind;
          } catch {
            valid = false;
          }
        }
        if (valid) {
          reusedNodes += 1;
          members.push({
            kind: "manifest-node",
            hash: copyBytes(record.digest),
            size: record.encodedLength,
          });
          continue;
        }
        this.#tx.run(
          "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,0,?,?)",
          [
            options.sessionId,
            record.digest,
            new Uint8Array([2, ...u64be(record.encodedLength)]),
          ],
        );
        missingCount += 1;
      } else if (record.kind === "object-payload") {
        const missing = this.#tx.all<StagedRow>(
          "SELECT value FROM efs_replication_import_rows WHERE session_id=? AND kind=0 AND key=?",
          [options.sessionId, record.digest],
          { maxRows: 1, maxBytes: 256 },
        )[0];
        if (!missing)
          throw transferError(
            "IntegrityFailure",
            "payload was not requested by the receiver",
          );
        if (record.byteLength !== record.bytes.byteLength)
          throw transferError("IntegrityFailure", "payload length mismatch");
        if (record.byteLength > this.#limits.maxFinalTransactionBytes)
          throw transferError("ResourceLimit", "payload exceeds the blob envelope");
        const kindByte = missing.value?.[0] ?? 0;
        if (kindByte === 0) {
          const declared = readU64(missing.value!, 1, "missing object size");
          if (declared !== record.byteLength)
            throw transferError("IntegrityFailure", "payload size does not match the offer");
        } else if (kindByte === 1) {
          if (record.byteLength < 68)
            throw transferError("IntegrityFailure", "manifest root envelope is invalid");
        }
        const actual = this.#hashBytes(record.bytes);
        if (!equalBytes(actual, record.digest))
          throw transferError("IntegrityFailure", "payload digest mismatch");
        if (kindByte === 0) {
          content.putObjectsBatch([{ hash: record.digest, bytes: record.bytes }], true);
          insertedObjects += 1;
          members.push({
            kind: "object",
            hash: copyBytes(record.digest),
            size: record.byteLength,
          });
        } else if (kindByte === 1) {
          decodeManifestRoot(record.bytes, record.digest);
          content.putManifestRoot(record.digest, record.bytes);
          insertedRoots += 1;
          members.push({
            kind: "manifest-root",
            hash: copyBytes(record.digest),
            size: record.byteLength,
          });
        } else {
          decodeManifestNode(record.bytes, record.digest);
          content.putManifestNodesBatch([{ hash: record.digest, encoded: record.bytes }]);
          insertedNodes += 1;
          members.push({
            kind: "manifest-node",
            hash: copyBytes(record.digest),
            size: record.byteLength,
          });
        }
        this.#tx.run(
          "DELETE FROM efs_replication_import_rows WHERE session_id=? AND kind=0 AND key=?",
          [options.sessionId, record.digest],
        );
        transferredCount += 1;
      } else if (record.kind === "revision-fragment") {
        const decoded = decodeRevisionFragment(record.fragmentBytes);
        const revision = parseIntegerRevision(decoded.revisionId, "revisionId");
        if (decoded.rows.length === 0 && revision !== 0)
          throw transferError("IntegrityFailure", "revision fragment is empty");
        this.#storeRevisionFragment(options.sessionId, revision, decoded, false);
      } else if (record.kind === "checkpoint-fragment") {
        const decoded = decodeRevisionFragment(record.fragmentBytes);
        const revision = parseIntegerRevision(decoded.revisionId, "checkpoint revision");
        this.#storeRevisionFragment(options.sessionId, revision, decoded, true);
      } else if (record.kind === "branch-generation-fragment") {
        this.#storeBranchFragment(options.sessionId, record);
      } else if (record.kind === "terminal-result") {
        this.#tx.run(
          "INSERT INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,12,?,?) ON CONFLICT DO NOTHING",
          [
            options.sessionId,
            encoder.encode(record.operationId),
            new Uint8Array([...u64be(record.resultBytes.byteLength), ...record.resultBytes]),
          ],
        );
      }
    }
    if (members.length > 0) {
      // Imported membership is already covered by the durable import/session
      // journal.  Do not create a second root-journal generation here: the
      // finalizer records the authoritative root transition atomically.
      const certificate = this.#staging().appendBatch(leaseId, ownerNonce, members, false);
      void certificate;
      this.#tx.run(
        "UPDATE efs_replication_imports SET closure_object_count=closure_object_count+?,closure_object_bytes=closure_object_bytes+?,closure_root_count=closure_root_count+?,closure_node_count=closure_node_count+?,transferred_object_count=transferred_object_count+?,transferred_object_bytes=transferred_object_bytes+?,transferred_root_count=transferred_root_count+?,transferred_node_count=transferred_node_count+? WHERE session_id=? AND lease_id=?",
        [
          members.filter((m) => m.kind === "object").length,
          members
            .filter((m) => m.kind === "object")
            .reduce((sum, member) => sum + member.size, 0),
          members.filter((m) => m.kind === "manifest-root").length,
          members.filter((m) => m.kind === "manifest-node").length,
          members.filter((m) => m.kind === "object").length,
          members
            .filter((m) => m.kind === "object")
            .reduce((sum, member) => sum + member.size, 0),
          members.filter((m) => m.kind === "manifest-root").length,
          members.filter((m) => m.kind === "manifest-node").length,
          options.sessionId,
          leaseId,
        ],
      );
      stagedBytesDelta = members.reduce((sum, member) => sum + member.size, 0);
    }
    return Object.freeze({
      stagedBytesDelta,
      insertedObjects,
      reusedObjects,
      insertedNodes,
      reusedNodes,
      insertedRoots,
      reusedRoots,
      missingCount,
      transferredCount,
    });
  }

  #storeRevisionFragment(
    sessionId: string,
    revision: number,
    decoded: TransferRevisionFragmentDecoded,
    checkpoint: boolean,
  ): void {
    void checkpoint;
    const revisionKey = u64be(revision);
    const headerKey = keyBytes([u8(1), revisionKey]);
    const headerValue = new Uint8Array([
      ...u64be(decoded.parentRevisionId === null ? 0 : Number(decoded.parentRevisionId)),
      ...u64be(decoded.created_at_ms),
      ...u64be(decoded.changeCount),
      ...encoder.encode(decoded.writerId),
    ]);
    if (decoded.parentRevisionId !== null && revision !== 0) {
      const parent = parseIntegerRevision(decoded.parentRevisionId, "parent revision");
      if (parent !== revision - 1)
        throw transferError(
          "IntegrityFailure",
          "revision parent is not the contiguous predecessor",
        );
    }
    const hadHeader = this.#tx.all<{ count: number } & SqliteRow>(
      "SELECT count(*) count FROM efs_replication_import_rows WHERE session_id=? AND kind=1 AND key=?",
      [sessionId, headerKey],
      { maxRows: 1, maxBytes: 256 },
    )[0]!.count;
    if (!hadHeader) {
      this.#tx.run(
        "INSERT INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,1,?,?)",
        [sessionId, headerKey, headerValue],
      );
      this.#tx.run(
        "UPDATE efs_replication_imports SET revision_count=revision_count+1 WHERE session_id=?",
        [sessionId],
      );
    }
    for (const row of decoded.rows) {
      let kind: number;
      let key: Uint8Array;
      let value: Uint8Array;
      if (row.kind === 1) {
        kind = 2;
        key = keyBytes([u8(2), revisionKey, row.inodeId]);
        value = new Uint8Array([
          row.tombstone ? 1 : 0,
          ...(row.encoded ? row.encoded : new Uint8Array(0)),
        ]);
      } else if (row.kind === 2) {
        kind = 3;
        const parentBytes = encoder.encode(row.parentInode);
        key = keyBytes([u8(3), revisionKey, u32be(parentBytes.byteLength), parentBytes, row.nameSort]);
        value = new Uint8Array([
          row.tombstone ? 1 : 0,
          ...(row.encoded ? row.encoded : new Uint8Array(0)),
        ]);
      } else {
        kind = 4;
        key = keyBytes([u8(4), revisionKey, row.inodeId]);
        value = copyBytes(row.manifestHash);
      }
      const existed = this.#tx.run(
        "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,?,?,?)",
        [sessionId, kind, key, value],
      ).changes;
      if (existed) {
        this.#tx.run(
          "UPDATE efs_replication_imports SET state_row_count=state_row_count+1,state_byte_count=state_byte_count+? WHERE session_id=?",
          [value.byteLength + key.byteLength, sessionId],
        );
      }
    }
  }

  #storeBranchFragment(
    sessionId: string,
    record: Extract<ReplicationTransferRecord, { kind: "branch-generation-fragment" }>,
  ): void {
    const decoded = decodeBranchGenerationFragment(record.fragmentBytes);
    const importRow = this.#importRow(sessionId);
    if (importRow.branch_id !== null && importRow.branch_id !== decoded.branchId)
      throw transferError("BranchIdentityMismatch", "branch identity changed");
    if (
      importRow.base_revision !== null &&
      String(importRow.base_revision) !== decoded.baseRevision
    )
      throw transferError("BranchIdentityMismatch", "branch base revision changed");
    if (importRow.generation !== null && importRow.generation !== decoded.generation)
      throw transferError("BranchIdentityMismatch", "branch generation changed");
    const key = keyBytes([u8(5), record.branchId]);
    const value = new Uint8Array([
      ...u64be(Number(decoded.baseRevision)),
      ...u64be(decoded.generation),
      ...copyBytes(decoded.generationDigest),
      decoded.previousGeneration === null ? 0 : 1,
      ...(decoded.previousGeneration === null ? [] : u64be(decoded.previousGeneration)),
      decoded.previousGenerationDigest === null ? 0 : 1,
      ...(decoded.previousGenerationDigest === null ? [] : copyBytes(decoded.previousGenerationDigest)),
      decoded.state,
    ]);
    const existed = this.#tx.run(
      "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,5,?,?)",
      [sessionId, key, value],
    ).changes;
    if (existed) {
      this.#tx.run(
        "UPDATE efs_replication_imports SET state_row_count=state_row_count+1,state_byte_count=state_byte_count+? WHERE session_id=?",
        [value.byteLength + key.byteLength, sessionId],
      );
    }
    for (const row of decoded.rows) this.#storeBranchRow(sessionId, row);
  }

  #storeBranchRow(sessionId: string, row: TransferBranchRow): void {
    let kind: number;
    let key: Uint8Array;
    let value: Uint8Array;
    if (row.kind === 1) {
      kind = 6;
      key = keyBytes([u8(6), row.path]);
      value = new Uint8Array([
        row.disposition,
        row.expectedToken === null ? 0 : 1,
        ...(row.expectedToken === null ? [] : u64be(row.expectedToken)),
        row.encoded === null ? 0 : 1,
        ...(row.encoded === null ? [] : row.encoded),
      ]);
    } else if (row.kind === 2) {
      kind = 7;
      key = keyBytes([u8(7), row.inodeId]);
      value = new Uint8Array([
        row.expectedToken === null ? 0 : 1,
        ...(row.expectedToken === null ? [] : u64be(row.expectedToken)),
        ...row.encoded,
      ]);
    } else if (row.kind === 3) {
      kind = 8;
      key = keyBytes([u8(8), row.inodeId, u64be(row.pageIndex), u64be(row.generation)]);
      value = new Uint8Array([...row.bytes, ...u64be(row.created_at_ms), row.head ? 1 : 0]);
    } else if (row.kind === 4) {
      kind = 9;
      let length = 40;
      for (const segment of row.segments) length += 4 + segment.byteLength;
      value = new Uint8Array(length);
      const view = new DataView(value.buffer);
      view.setBigUint64(0, BigInt(row.generation), false);
      view.setBigUint64(8, BigInt(row.offset), false);
      view.setBigUint64(16, BigInt(row.deleteLength), false);
      view.setBigUint64(24, BigInt(row.insertLength), false);
      view.setUint32(32, row.segments.length, false);
      let offset = 36;
      for (const segment of row.segments) {
        view.setUint32(offset, segment.byteLength, false);
        value.set(segment, offset + 4);
        offset += 4 + segment.byteLength;
      }
      key = keyBytes([u8(9), row.inodeId, u64be(row.sequence)]);
    } else if (row.kind === 5) {
      kind = 10;
      key = keyBytes([u8(10), row.inodeId]);
      value = new Uint8Array([
        row.expectedToken === null ? 0 : 1,
        ...(row.expectedToken === null ? [] : u64be(row.expectedToken)),
      ]);
    } else {
      kind = 11;
      key = keyBytes([u8(11), row.path]);
      value = copyBytes(row.manifestHash);
    }
    const existed = this.#tx.run(
      "INSERT OR IGNORE INTO efs_replication_import_rows(session_id,kind,key,value) VALUES(?,?,?,?)",
      [sessionId, kind, key, value],
    ).changes;
    if (existed) {
      this.#tx.run(
        "UPDATE efs_replication_imports SET state_row_count=state_row_count+1,state_byte_count=state_byte_count+? WHERE session_id=?",
        [value.byteLength + key.byteLength, sessionId],
      );
    }
  }

  renewLease(options: {
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
    readonly expiresAt: number;
  }): boolean {
    const importRow = this.#importRow(options.sessionId);
    if (!equalBytes(importRow.owner_nonce, options.ownerNonce)) return false;
    if (!Number.isSafeInteger(options.expiresAt) || options.expiresAt <= options.now)
      return false;
    const result = this.#tx.run(
      "UPDATE efs_leases SET last_renewal_at_ms=?,expires_at_ms=? WHERE id=? AND owner_nonce=? AND state=0 AND expires_at_ms>?",
      [options.now, options.expiresAt, importRow.lease_id, options.ownerNonce, options.now],
    );
    return result.changes === 1;
  }

  abortImport(options: {
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
  }): void {
    if (!this.abortImportIfPresent(options))
      throw transferError("CursorMismatch", "import state is missing");
  }

  abortImportIfPresent(options: {
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
  }): boolean {
    const rows = this.#tx.all<ImportRow>(
      "SELECT session_id,lease_id,owner_nonce,kind,phase,branch_id,base_revision,generation,expected_generation_digest,closure_object_count,closure_object_bytes,closure_root_count,closure_node_count,transferred_object_count,transferred_object_bytes,transferred_root_count,transferred_node_count,state_row_count,state_byte_count,revision_count,installed_revision_count,sealed FROM efs_replication_imports WHERE session_id=?",
      [options.sessionId],
      { maxRows: 1, maxBytes: 4096 },
    );
    const importRow = rows[0];
    if (!importRow) return false;
    if (!equalBytes(importRow.owner_nonce, options.ownerNonce))
      throw transferError("CursorMismatch", "import owner nonce mismatch");
    if (importRow.sealed !== 2)
      this.#staging().release(importRow.lease_id, options.ownerNonce, false);
    this.#tx.run("UPDATE efs_replication_imports SET sealed=2 WHERE session_id=?", [
      options.sessionId,
    ]);
    return true;
  }

  maintenance(options: { readonly now: number; readonly limit: number }): Readonly<{ readonly expiredLeases: number; readonly cleanupPasses: number }> {
    if (!Number.isSafeInteger(options.now) || options.now < 0)
      throw transferError("ResourceLimit", "maintenance time is invalid");
    if (!Number.isSafeInteger(options.limit) || options.limit <= 0)
      throw transferError("ResourceLimit", "maintenance limit is invalid");
    const imports = this.#tx.all<{ session_id: string; lease_id: string; owner_nonce: Uint8Array } & SqliteRow>(
      "SELECT i.session_id,i.lease_id,i.owner_nonce FROM efs_replication_imports i JOIN efs_replication_sessions s ON s.id=i.session_id LEFT JOIN efs_leases l ON l.id=i.lease_id WHERE s.expires_at_ms<=? OR l.expires_at_ms<=? ORDER BY i.session_id LIMIT ?",
      [options.now, options.now, options.limit],
      { maxRows: options.limit, maxBytes: Math.max(1024, options.limit * 512) },
    );
    const staging = this.#staging();
    for (const row of imports) {
      staging.release(row.lease_id, row.owner_nonce, false);
      this.#tx.run("UPDATE efs_replication_imports SET sealed=2 WHERE session_id=?", [row.session_id]);
    }
    const expiredLeases = staging.expireBatch(options.now, options.limit);
    let cleanupPasses = 0;
    for (let pass = 0; pass < options.limit; pass += 1) {
      const progress = staging.cleanupBatch(Math.min(options.limit, this.#limits.maxGcBatchSize));
      if (!progress.worked) break;
      cleanupPasses += 1;
    }
    return Object.freeze({ expiredLeases, cleanupPasses });
  }

  finalizeImport(options: {
    readonly sessionId: string;
    readonly kind: 0 | 1 | 2;
    readonly expectedRevision: number;
    readonly expectedRootMutationGeneration: number;
    readonly expectedNextAllocationSequence: number;
    readonly expectedRootInode: string;
    readonly expectedRevisionCount: number;
    readonly expectedStateRows: number;
    readonly expectedClosureRoots: number;
    readonly expectedClosureNodes: number;
    readonly expectedClosureObjects: number;
    readonly expectedClosureObjectBytes: number;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly checkpoint: boolean;
    readonly terminalState: 0 | 1 | 2;
    readonly terminalResultOperationId: string | null;
    readonly terminalResultBytes: Uint8Array | null;
    readonly genesisMeta: ReplicationExportMeta | null;
    readonly genesisRows: readonly {
      readonly inodeId: string;
      readonly tombstone: boolean;
      readonly encoded: Uint8Array | null;
    }[];
    readonly now: number;
  }): Readonly<{
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
  }> {
    const importRow = this.#importRow(options.sessionId);
    if (importRow.sealed === 2)
      throw transferError("Aborted", "import was aborted");
    if (importRow.kind !== options.kind)
      throw transferError("OperationMismatch", "import kind changed");
    const staging = this.#staging();
    const certificate = staging.snapshot(importRow.lease_id, importRow.owner_nonce);
    if (
      certificate.objectCount !== options.expectedClosureObjects ||
      certificate.nodeCount !==
        options.expectedClosureRoots + options.expectedClosureNodes ||
      certificate.membershipCount !==
        options.expectedClosureRoots +
          options.expectedClosureNodes +
          options.expectedClosureObjects ||
      certificate.objectBytes !== options.expectedClosureObjectBytes
    ) {
      throw transferError(
        "IntegrityFailure",
        "staged closure certificate does not match the negotiated summary",
      );
    }
    if (options.kind === 0) {
      const result = this.#finalizeMain(options, importRow);
      staging.release(importRow.lease_id, importRow.owner_nonce, false, undefined, false);
      return result;
    }
    if (options.kind === 1) {
      const result = this.#finalizeBranch(options, importRow);
      staging.release(importRow.lease_id, importRow.owner_nonce, false, undefined, false);
      return result;
    }
    const result = this.#finalizeGenesis(options, importRow);
    staging.release(importRow.lease_id, importRow.owner_nonce, false, undefined, false);
    return result;
  }

  #stagedRows(sessionId: string, kind: number): readonly StagedRow[] {
    return this.#tx.all<StagedRow>(
      "SELECT key,value FROM efs_replication_import_rows WHERE session_id=? AND kind=? ORDER BY key",
      [sessionId, kind],
      { maxRows: 65536, maxBytes: 128 * 1024 * 1024 },
    );
  }

  #validateImportedManifest(sessionId: string, importRow: ImportRow): void {
    const roots = new Map<string, Uint8Array>();
    for (const row of [
      ...this.#stagedRows(sessionId, 4),
      ...this.#stagedRows(sessionId, 11),
    ]) {
      if (row.value?.byteLength !== 32)
        throw transferError("IntegrityFailure", "staged manifest reference is invalid");
      const key = bytesToHex(row.value);
      if (!roots.has(key)) roots.set(key, copyBytes(row.value));
    }
    if (roots.size === 0) return;
    const staging = this.#staging();
    for (const manifestHash of roots.values()) {
      staging.beginReconciliation(importRow.lease_id, importRow.owner_nonce, manifestHash);
      let progress = staging.reconcileBatch(
        importRow.lease_id,
        importRow.owner_nonce,
        Math.max(1, Math.min(this.#limits.maxQueryBatchSize, 1024)),
        { validationOnly: true },
      );
      while (!progress.complete) {
        progress = staging.reconcileBatch(
          importRow.lease_id,
          importRow.owner_nonce,
          Math.max(1, Math.min(this.#limits.maxQueryBatchSize, 1024)),
          { validationOnly: true },
        );
      }
      staging.clearReconciliation(importRow.lease_id, importRow.owner_nonce);
    }
  }

  #finalizeMain(
    options: {
      readonly sessionId: string;
      readonly expectedRevision: number;
      readonly expectedRootMutationGeneration: number;
      readonly expectedNextAllocationSequence: number;
      readonly expectedRootInode: string;
      readonly expectedRevisionCount: number;
      readonly expectedStateRows: number;
      readonly checkpoint: boolean;
      readonly now: number;
    },
    importRow: ImportRow,
  ): Readonly<{
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
  }> {
    const meta = this.#meta();
    if (meta.main_revision > options.expectedRevision)
      throw transferError("MainDiverged", "destination head is ahead of the transfer");
    if (meta.root_inode !== options.expectedRootInode)
      throw transferError(
        "FilesystemMismatch",
        "destination root inode does not match the authority",
      );
    if (
      importRow.state_row_count !== options.expectedStateRows ||
      importRow.revision_count !== options.expectedRevisionCount
    ) {
      throw transferError("IntegrityFailure", "staged state summary does not match");
    }
    // A fresh export against an already caught-up destination is a valid
    // idempotent replay.  The staged rows are still authenticated and the
    // closure certificate has already been checked by finalizeImport, but
    // they must not be installed a second time as revisions 1..N.
    if (meta.main_revision === options.expectedRevision) {
      if (
        meta.root_mutation_generation !== options.expectedRootMutationGeneration ||
        meta.next_allocation_sequence < options.expectedNextAllocationSequence
      ) {
        throw transferError("MainDiverged", "destination metadata differs at the selected revision");
      }
      if (this.#stagedRows(options.sessionId, 1).length !== options.expectedRevisionCount)
        throw transferError("IntegrityFailure", "staged revision count does not match");
      this.#tx.run(
        "UPDATE efs_replication_imports SET installed_revision_count=1 WHERE session_id=?",
        [options.sessionId],
      );
      return Object.freeze({
        revision: String(options.expectedRevision),
        branchId: null,
        baseRevision: null,
        generation: 0,
        generationDigest: null,
        state: 0,
        authorityResult: null,
        reusedBytes: 0,
      });
    }
    this.#validateImportedManifest(options.sessionId, importRow);
    const headers = this.#stagedRows(options.sessionId, 1);
    if (headers.length !== options.expectedRevisionCount)
      throw transferError("IntegrityFailure", "staged revision count does not match");
    const first = meta.main_revision + 1;
    const revisions: number[] = [];
    for (const header of headers) {
      const revision = readU64(header.key, 1, "staged revision");
      if (revision > options.expectedRevision)
        throw transferError("IntegrityFailure", "staged revision is out of range");
      revisions.push(revision);
    }
    revisions.sort((left, right) => left - right);
    for (let index = 0; index < revisions.length; index += 1)
      if (revisions[index] !== index + 1)
        throw transferError("IntegrityFailure", "staged revision range is not contiguous");
    const newHeaders = headers.filter(
      (header) => readU64(header.key, 1, "staged revision") >= first,
    );
    const isNewRevisionRow = (row: StagedRow): boolean =>
      readU64(row.key, 1, "staged state revision") >= first;
    const inodeRows = this.#stagedRows(options.sessionId, 2).filter(isNewRevisionRow);
    const entryRows = this.#stagedRows(options.sessionId, 3).filter(isNewRevisionRow);
    const refRows = this.#stagedRows(options.sessionId, 4).filter(isNewRevisionRow);
    const usage = new UsageRepository(this.#tx, this.#limits);
    let chargedMetadata = 0;
    let maintenanceBytes = 0;
    for (const header of newHeaders) {
      const revision = readU64(header.key, 1, "staged revision");
      const parentValue = readU64(header.value!, 0, "staged parent revision");
      const parent = revision === 0 || parentValue === 0xffff_ffff_ffff_ffff ? null : parentValue;
      const createdAtMs = readU64(header.value!, 8, "staged creation time");
      const changeCount = readU64(header.value!, 16, "staged change count");
      const writerBytes = header.value!.subarray(24);
      let writerId: string;
      try {
        writerId = decoder.decode(writerBytes);
      } catch {
        throw transferError("IntegrityFailure", "staged writer id is not UTF-8");
      }
      const inserted = this.#tx.run(
        "INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(?,?,?,?,?)",
        [revision, parent, createdAtMs, writerId, changeCount],
      ).changes;
      void inserted;
      chargedMetadata += CHARGED_ROW_BYTES + writerBytes.byteLength;
      maintenanceBytes += CHARGED_ROW_BYTES + encoder.encode(String(revision)).byteLength;
      this.#tx.run(
        "INSERT OR IGNORE INTO efs_root_journal(generation,kind,root_id) VALUES(?,0,?)",
        [revision, String(revision)],
      );
    }
    let installedRows = 0;
    for (const row of inodeRows) {
      const revision = readU64(row.key, 1, "staged inode revision");
      const inodeIdBytes = row.key.subarray(9);
      let inodeId: string;
      try {
        inodeId = decoder.decode(inodeIdBytes);
      } catch {
        throw transferError("IntegrityFailure", "staged inode id is not UTF-8");
      }
      const tombstone = (row.value![0] ?? 0) === 1;
      const encoded = row.value!.subarray(1);
      const existed = this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_inodes WHERE id=?",
        [inodeId],
        { maxRows: 1, maxBytes: 256 },
      )[0]!.count;
      if (tombstone) {
        this.#tx.run("DELETE FROM efs_inodes WHERE id=?", [inodeId]);
        this.#tx.run(
          "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(?,?,1,NULL) ON CONFLICT DO NOTHING",
          [revision, inodeId],
        );
        chargedMetadata += CHARGED_ROW_BYTES;
        installedRows += 1;
        continue;
      }
      const inode = deserializeInode(encoded);
      if (inode.type === 0 && (inode.manifest_hash === null || inode.size === null))
        throw transferError("IntegrityFailure", "regular file inode lacks content");
      this.#tx.run(
        "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET type=excluded.type,mode=excluded.mode,birthtime_ms=excluded.birthtime_ms,mtime_ms=excluded.mtime_ms,ctime_ms=excluded.ctime_ms,nlink=excluded.nlink,size=excluded.size,manifest_hash=excluded.manifest_hash,symlink_target=excluded.symlink_target,token=excluded.token",
        [
          inode.id,
          inode.type,
          inode.mode,
          inode.birthtime_ms,
          inode.mtime_ms,
          inode.ctime_ms,
          inode.nlink,
          inode.size,
          inode.manifest_hash,
          inode.symlink_target,
          inode.token,
        ],
      );
      this.#tx.run(
        "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(?,?,0,?) ON CONFLICT DO NOTHING",
        [revision, inode.id, encoded],
      );
      chargedMetadata +=
        CHARGED_ROW_BYTES + encoded.byteLength + (existed ? 0 : CHARGED_ROW_BYTES);
      installedRows += 1;
    }
    for (const row of entryRows) {
      const revision = readU64(row.key, 1, "staged entry revision");
      const rest = row.key.subarray(9);
      const parentLength = readU32Length(rest, 0, "staged entry parent");
      let parentInode: string;
      try {
        parentInode = decoder.decode(rest.subarray(4, 4 + parentLength));
      } catch {
        throw transferError("IntegrityFailure", "staged entry parent is not UTF-8");
      }
      const nameSort = copyBytes(rest.subarray(4 + parentLength));
      const tombstone = (row.value![0] ?? 0) === 1;
      const encoded = row.value!.subarray(1);
      const existed = this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_entries WHERE parent_inode=? AND name_sort=?",
        [parentInode, nameSort],
        { maxRows: 1, maxBytes: 256 },
      )[0]!.count;
      if (tombstone) {
        this.#tx.run(
          "DELETE FROM efs_entries WHERE parent_inode=? AND name_sort=?",
          [parentInode, nameSort],
        );
        this.#tx.run(
          "INSERT INTO efs_entry_revisions(revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,1,NULL) ON CONFLICT DO NOTHING",
          [revision, parentInode, nameSort],
        );
        chargedMetadata += CHARGED_ROW_BYTES + nameSort.byteLength;
        installedRows += 1;
        continue;
      }
      const entry = deserializeEntry(encoded);
      this.#tx.run(
        "INSERT INTO efs_entries(parent_inode,name_sort,name,inode_id,token) VALUES(?,?,?,?,?) ON CONFLICT(parent_inode,name_sort) DO UPDATE SET name=excluded.name,inode_id=excluded.inode_id,token=excluded.token",
        [parentInode, nameSort, entry.name, entry.inode_id, entry.token],
      );
      this.#tx.run(
        "INSERT INTO efs_entry_revisions(revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,0,?) ON CONFLICT DO NOTHING",
        [revision, parentInode, nameSort, encoded],
      );
      chargedMetadata +=
        CHARGED_ROW_BYTES +
        nameSort.byteLength +
        encoded.byteLength +
        (existed ? 0 : CHARGED_ROW_BYTES);
      installedRows += 1;
    }
    for (const row of refRows) {
      const revision = readU64(row.key, 1, "staged manifest ref revision");
      const inodeId = decoder.decode(row.key.subarray(9));
      const manifestHash = copyBytes(row.value!);
      this.#tx.run(
        "INSERT INTO efs_revision_manifest_roots(revision,inode_id,manifest_hash) VALUES(?,?,?)",
        [revision, inodeId, manifestHash],
      );
      chargedMetadata += CHARGED_ROW_BYTES;
    }
    usage.apply(
      {
        charged_metadata_bytes: chargedMetadata,
        maintenance_bytes: maintenanceBytes,
      },
      "replicated main revision install",
    );
    if (options.checkpoint) {
      for (const row of inodeRows) {
        const revision = readU64(row.key, 1, "checkpoint inode revision");
        const inodeId = decoder.decode(row.key.subarray(9));
        this.#tx.run(
          "INSERT INTO efs_checkpoint_inodes(target_revision,inode_id,tombstone,encoded) VALUES(?,?,?,?)",
          [revision, inodeId, (row.value![0] ?? 0), row.value!.subarray(1)],
        );
      }
      for (const row of entryRows) {
        const revision = readU64(row.key, 1, "checkpoint entry revision");
        const rest = row.key.subarray(9);
        const parentLength = readU32Length(rest, 0, "checkpoint entry parent");
        const parentInode = decoder.decode(rest.subarray(4, 4 + parentLength));
        const nameSort = copyBytes(rest.subarray(4 + parentLength));
        this.#tx.run(
          "INSERT INTO efs_checkpoint_entries(target_revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,?,?)",
          [revision, parentInode, nameSort, (row.value![0] ?? 0), row.value!.subarray(1)],
        );
      }
      for (const row of refRows) {
        const revision = readU64(row.key, 1, "checkpoint ref revision");
        const inodeId = decoder.decode(row.key.subarray(9));
        this.#tx.run(
          "INSERT INTO efs_checkpoint_manifest_roots(target_revision,inode_id,manifest_hash) VALUES(?,?,?)",
          [revision, inodeId, copyBytes(row.value!)],
        );
      }
      const target = options.expectedRevision;
      this.#tx.run(
        "INSERT INTO efs_revision_checkpoints(target_revision,state,phase,inode_cursor,entry_parent,entry_name_sort,inode_count,entry_count,created_at_ms) VALUES(?,1,7,NULL,NULL,NULL,?,?,?) ON CONFLICT DO NOTHING",
        [target, inodeRows.length, entryRows.length, options.now],
      );
      usage.apply(
        {
          charged_metadata_bytes:
            (inodeRows.length + entryRows.length + refRows.length) * CHARGED_ROW_BYTES +
            inodeRows.reduce((sum, row) => sum + (row.value!.subarray(1).byteLength), 0) +
            entryRows.reduce(
              (sum, row) =>
                sum + readU32Length(row.key.subarray(9), 0, "entry parent") + row.value!.subarray(1).byteLength,
              0,
            ),
        },
        "replicated checkpoint install",
      );
    }
    const updated = this.#tx.run(
      "UPDATE efs_meta SET main_revision=?,root_mutation_generation=?,last_root_removal_generation=?,next_allocation_sequence=MAX(next_allocation_sequence,?) WHERE singleton=1",
      [
        options.expectedRevision,
        options.expectedRootMutationGeneration,
        options.expectedRootMutationGeneration,
        options.expectedNextAllocationSequence,
      ],
    );
    if (updated.changes !== 1)
      throw transferError("ECORRUPT", "filesystem metadata could not be advanced");
    return Object.freeze({
      revision: String(options.expectedRevision),
      branchId: null,
      baseRevision: null,
      generation: 0,
      generationDigest: null,
      state: 0,
      authorityResult: null,
      reusedBytes: 0,
    });
  }

  #finalizeBranch(
    options: {
      readonly sessionId: string;
      readonly branchId: string | null;
      readonly baseRevision: string | null;
      readonly generation: number | null;
      readonly generationDigest: Uint8Array | null;
      readonly terminalState: 0 | 1 | 2;
      readonly terminalResultOperationId: string | null;
      readonly terminalResultBytes: Uint8Array | null;
      readonly now: number;
    },
    importRow: ImportRow,
  ): Readonly<{
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
  }> {
    this.#validateImportedManifest(options.sessionId, importRow);
    const branchId = options.branchId ?? importRow.branch_id;
    if (!branchId) throw transferError("BranchIdentityMismatch", "branch identity is missing");
    const branchRows = this.#stagedRows(options.sessionId, 5);
    if (branchRows.length !== 1)
      throw transferError("IntegrityFailure", "branch state is not staged exactly once");
    const branchValue = branchRows[0]!.value!;
    const baseRevision = readU64(branchValue, 0, "staged branch base revision");
    const generation = readU64(branchValue, 8, "staged branch generation");
    const expectedDigest = copyBytes(branchValue.subarray(16, 48));
    const priorGenerationTag = branchValue[48];
    if (priorGenerationTag !== 0 && priorGenerationTag !== 1)
      throw transferError("IntegrityFailure", "staged branch predecessor generation tag is invalid");
    const priorGeneration =
      priorGenerationTag === 0 ? null : readU64(branchValue, 49, "staged branch predecessor generation");
    const priorDigestOffset = priorGeneration === null ? 49 : 57;
    const priorDigestTag = branchValue[priorDigestOffset];
    if (priorDigestTag !== 0 && priorDigestTag !== 1)
      throw transferError("IntegrityFailure", "staged branch predecessor digest tag is invalid");
    const priorDigest =
      priorDigestTag === 0
        ? null
        : copyBytes(branchValue.subarray(priorDigestOffset + 1, priorDigestOffset + 33));
    const fragmentStateOffset = priorDigest === null ? priorDigestOffset + 1 : priorDigestOffset + 33;
    const fragmentState = (branchValue[fragmentStateOffset] ?? 0) as 0 | 1 | 2;
    if (fragmentState > 2 || branchValue.byteLength !== fragmentStateOffset + 1)
      throw transferError("IntegrityFailure", "staged branch state envelope is invalid");
    if ((priorGeneration === null) !== (priorDigest === null))
      throw transferError("IntegrityFailure", "staged branch predecessor is incomplete");
    if (
      options.generationDigest !== null &&
      !equalBytes(options.generationDigest, expectedDigest)
    )
      throw transferError(
        "BranchIdentityMismatch",
        "activation generation digest differs from the selected branch snapshot",
      );
    const existing = this.#tx.all<BranchRowSql & SqliteRow>(
      "SELECT base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision FROM efs_branches WHERE id=?",
      [branchId],
      { maxRows: 1, maxBytes: 2048 },
    )[0];
    const requestedBase = parseIntegerRevision(options.baseRevision ?? "", "base revision");
    if (requestedBase !== baseRevision)
      throw transferError("BranchIdentityMismatch", "branch base revision changed");
    if (options.generation !== null && options.generation !== generation)
      throw transferError("BranchIdentityMismatch", "branch generation changed");
    const terminalDetails =
      fragmentState === 0
        ? null
        : (() => {
            if (options.terminalResultOperationId === null || options.terminalResultBytes === null)
              throw transferError("IntegrityFailure", "terminal branch result is missing from activation");
            const operationId = options.terminalResultOperationId;
            const resultBytes = options.terminalResultBytes;
            const resultDigest = copyBytes(this.#hashBytes(resultBytes));
            const decoded = decodeJson<Record<string, unknown>>(resultBytes);
            const result =
              decoded && decoded.kind === "efs-publication-result-v2" && decoded.result &&
              typeof decoded.result === "object"
                ? decoded.result as Record<string, unknown>
                : decoded;
            const merged =
              result?.outcome === "merged" ||
              result?.outcome === 0 ||
              (typeof result?.outcome === "number" && result.outcome === 0);
            const revisionValue = result?.revision;
            const mergedRevision =
              fragmentState === 1 &&
              ((typeof revisionValue === "string" && /^\d+$/u.test(revisionValue)) ||
                (typeof revisionValue === "number" && Number.isSafeInteger(revisionValue)))
                ? Number(revisionValue)
                : null;
            if (fragmentState === 1 && mergedRevision === null)
              throw transferError("IntegrityFailure", "merged terminal result has no revision");
            return {
              operationId,
              resultBytes,
              resultDigest,
              merged,
              mergedRevision,
              authorityResult:
                fragmentState === 1
                  ? {
                      kind: "publication" as const,
                      operationId,
                      outcome: merged ? ("merged" as const) : ("conflict" as const),
                      resultDigest,
                    }
                  : { kind: "discard" as const, operationId: null, resultDigest },
            };
          })();
    const installTerminalResult = (details: NonNullable<typeof terminalDetails>): void => {
      const prior = this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
        "SELECT encoded FROM efs_operation_results WHERE operation_id=?",
        [details.operationId],
        { maxRows: 1, maxBytes: this.#limits.maxFinalTransactionBytes },
      )[0];
      if (prior) {
        if (!equalBytes(prior.encoded, details.resultBytes))
          throw transferError("IntegrityFailure", "terminal result bytes changed for the operation");
        return;
      }
      this.#tx.run(
        "INSERT OR IGNORE INTO efs_operation_ids(id,branch_id,generation,created_at_ms) VALUES(?,?,?,?)",
        [details.operationId, branchId, generation, options.now],
      );
      const expiresAt = options.now + this.#resultRetentionMs;
      this.#tx.run(
        "INSERT INTO efs_operation_results(operation_id,outcome,encoded,expires_at_ms,revision) VALUES(?,?,?,?,?)",
        [details.operationId, details.merged ? 1 : 0, details.resultBytes, expiresAt, details.mergedRevision],
      );
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: 2 * CHARGED_ROW_BYTES + details.resultBytes.byteLength,
          permanent_identifiers: 1,
          result_bytes: details.resultBytes.byteLength,
        },
        "replicated terminal result install",
      );
    };
    let replacingExisting = false;
    if (existing) {
      if (existing.base_revision !== baseRevision)
        throw transferError(
          "BranchIdentityMismatch",
          "branch identifier is bound to another base revision",
        );
      if (existing.generation > generation)
        throw transferError(
          "BranchIdentityMismatch",
          "stale branch generation import is rejected",
        );
      if (existing.generation === generation) {
        if (existing.state !== 0)
          throw transferError(
            "BranchIdentityMismatch",
            "terminal branch state cannot be reimported as active",
          );
        const recomputed =
          fragmentState !== 0 && this.#branchDigest
            ? hexBytes(this.#branchDigest(branchId, generation))
            : this.#recomputeBranchDigest(
                options.sessionId,
                branchId,
                baseRevision,
                generation,
              );
        if (!equalBytes(recomputed, expectedDigest))
          throw transferError(
            "IntegrityFailure",
            `staged branch generation digest does not match the installed generation (expected=${bytesToHex(expectedDigest)}, actual=${bytesToHex(recomputed)}, changes=${this.#stagedRows(options.sessionId, 6).length}, overlays=${this.#stagedRows(options.sessionId, 7).length}, pages=${this.#stagedRows(options.sessionId, 8).length}, patches=${this.#stagedRows(options.sessionId, 9).length}, expectations=${this.#stagedRows(options.sessionId, 10).length}, refs=${this.#stagedRows(options.sessionId, 11).length})`,
          );
        if (fragmentState !== 0) {
          this.#branches().putTerminalGenerationDigest(
            branchId,
            generation,
            bytesToHex(expectedDigest),
          );
          this.#branches().finish(
            branchId,
            fragmentState,
            options.now,
            terminalDetails!.mergedRevision,
          );
          installTerminalResult(terminalDetails!);
        }
        return Object.freeze({
          revision: String(baseRevision),
          branchId,
          baseRevision: String(baseRevision),
          generation,
          generationDigest: copyBytes(expectedDigest),
          state: fragmentState,
          authorityResult: terminalDetails?.authorityResult ?? null,
          reusedBytes: 0,
        });
      }
      if (existing.state !== 0)
        throw transferError(
          "BranchIdentityMismatch",
          "terminal branch state cannot be advanced by an active generation",
        );
      if (priorGeneration === null || priorDigest === null || priorGeneration !== existing.generation)
        throw transferError(
          "BranchDiverged",
          "a lower branch generation requires the exact installed predecessor digest",
        );
      const installedDigest = this.#branchDigest
        ? hexBytes(this.#branchDigest(branchId, existing.generation))
        : (() => {
            const stored = this.#branches().terminalGenerationDigest(branchId, existing.generation);
            return stored ? hexBytes(stored) : null;
          })();
      if (installedDigest === null || !equalBytes(installedDigest, priorDigest))
        throw transferError(
          "BranchDiverged",
          "the installed branch generation does not match the advertised predecessor digest",
        );
      this.#branches().replaceReplicatedPayload(branchId);
      this.#branches().setReplicatedGeneration(branchId, generation);
      replacingExisting = true;
    }
    const baseExists = this.#tx.all<{ count: number } & SqliteRow>(
      "SELECT count(*) count FROM efs_revisions WHERE revision=?",
      [baseRevision],
      { maxRows: 1, maxBytes: 256 },
    )[0]!.count;
    if (baseExists !== 1)
      throw transferError("BaseRevisionMissing", "destination lacks the branch base revision");
    const usage = new UsageRepository(this.#tx, this.#limits);
    const createdNow = options.now;
    if (!replacingExisting) {
      this.#tx.run(
        "INSERT OR IGNORE INTO efs_branch_ids(id,created_at_ms) VALUES(?,?)",
        [branchId, createdNow],
      );
      this.#tx.run(
        "INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision) VALUES(?,?,?,?,?,?,?)",
        [
          branchId,
          baseRevision,
          fragmentState,
          generation,
          createdNow,
          fragmentState === 0 ? null : options.now,
          null,
        ],
      );
      usage.apply(
        {
          charged_metadata_bytes: 2 * CHARGED_ROW_BYTES,
          permanent_identifiers: 1,
        },
        "replicated branch install",
      );
    }
    const changes = this.#stagedRows(options.sessionId, 6);
    for (const row of changes) {
      const value = row.value!;
      const kind = value[0] ?? 0;
      const hasToken = (value[1] ?? 0) === 1;
      const token = hasToken ? readU64(value, 2, "staged change token") : null;
      const encodedTag = 2 + (hasToken ? 8 : 0);
      const hasEncoded = (value[encodedTag] ?? 0) === 1;
      const encodedStart = encodedTag + 1;
      const encoded = hasEncoded ? value.subarray(encodedStart) : null;
      this.#tx.run(
        "INSERT INTO efs_branch_changes(branch_id,path,expected_token,kind,encoded) VALUES(?,?,?,?,?)",
        [branchId, copyBytes(row.key.subarray(1)), token, kind, encoded],
      );
      usage.apply(
        {
          charged_metadata_bytes:
            CHARGED_ROW_BYTES +
            row.key.subarray(1).byteLength +
            (encoded?.byteLength ?? 0),
        },
        "replicated branch change install",
      );
    }
    const overlays = this.#stagedRows(options.sessionId, 7);
    for (const row of overlays) {
      const value = row.value!;
      const hasToken = (value[0] ?? 0) === 1;
      const token = hasToken ? readU64(value, 1, "staged overlay token") : null;
      const encoded = value.subarray(hasToken ? 9 : 1);
      this.#tx.run(
        "INSERT INTO efs_branch_inode_overlays(branch_id,inode_id,expected_token,encoded) VALUES(?,?,?,?)",
        [branchId, decoder.decode(row.key.subarray(1)), token, encoded],
      );
      usage.apply(
        {
          charged_metadata_bytes: CHARGED_ROW_BYTES + encoded.byteLength,
        },
        "replicated branch overlay install",
      );
    }
    const pages = this.#stagedRows(options.sessionId, 8);
    let pageBytes = 0;
    for (const row of pages) {
      const rest = row.key.subarray(1);
      const inodeLength = readU32Length(rest, 0, "staged page inode");
      const inodeId = decoder.decode(rest.subarray(4, 4 + inodeLength));
      const pageIndex = readU64(rest, 4 + inodeLength, "staged page index");
      const pageGeneration = readU64(
        rest,
        12 + inodeLength,
        "staged page generation",
      );
      const value = row.value!;
      const pageBytesValue = value.byteLength - 9;
      const createdAtMs = readU64(value, pageBytesValue, "staged page creation time");
      const head = (value[value.byteLength - 1] ?? 0) === 1;
      this.#tx.run(
        "INSERT INTO efs_cow_page_versions(branch_id,inode_id,page_index,generation,bytes,created_at_ms) VALUES(?,?,?,?,?,?)",
        [branchId, inodeId, pageIndex, pageGeneration, value.subarray(0, pageBytesValue), createdAtMs],
      );
      if (head)
        this.#tx.run(
          "INSERT INTO efs_cow_page_heads(branch_id,inode_id,page_index,generation) VALUES(?,?,?,?)",
          [branchId, inodeId, pageIndex, pageGeneration],
        );
      pageBytes += pageBytesValue;
    }
    usage.apply(
      {
        charged_metadata_bytes: pages.length * 2 * CHARGED_ROW_BYTES,
        page_count: pages.length,
        page_bytes: pageBytes,
      },
      "replicated branch pages install",
    );
    const patches = this.#stagedRows(options.sessionId, 9);
    let patchBytes = 0;
    for (const row of patches) {
      const rest = row.key.subarray(1);
      const inodeLength = readU32Length(rest, 0, "staged patch inode");
      const inodeId = decoder.decode(rest.subarray(4, 4 + inodeLength));
      const sequence = readU64(rest, 4 + inodeLength, "staged patch sequence");
      const value = row.value!;
      const view = new DataView(value.buffer, value.byteOffset, value.byteLength);
      const patchGeneration = Number(view.getBigUint64(0, false));
      const offset = Number(view.getBigUint64(8, false));
      const deleteLength = Number(view.getBigUint64(16, false));
      const insertLength = Number(view.getBigUint64(24, false));
      const segmentCount = view.getUint32(32, false);
      let cursor = 36;
      const segments: Uint8Array[] = [];
      for (let index = 0; index < segmentCount; index += 1) {
        const length = view.getUint32(cursor, false);
        segments.push(copyBytes(value.subarray(cursor + 4, cursor + 4 + length)));
        cursor += 4 + length;
      }
      this.#tx.run(
        "INSERT INTO efs_patches(branch_id,inode_id,sequence,generation,offset,delete_length,insert_length) VALUES(?,?,?,?,?,?,?)",
        [branchId, inodeId, sequence, patchGeneration, offset, deleteLength, insertLength],
      );
      for (let index = 0; index < segments.length; index += 1)
        this.#tx.run(
          "INSERT INTO efs_patch_segments(branch_id,inode_id,sequence,segment_index,bytes) VALUES(?,?,?,?,?)",
          [branchId, inodeId, sequence, index, segments[index]!],
        );
      patchBytes += segments.reduce((sum, segment) => sum + segment.byteLength, 0);
    }
    usage.apply(
      {
        charged_metadata_bytes: patches.length * (CHARGED_ROW_BYTES + CHARGED_ROW_BYTES),
        patch_count: patches.length,
        patch_bytes: patchBytes,
      },
      "replicated branch patches install",
    );
    const expectations = this.#stagedRows(options.sessionId, 10);
    for (const row of expectations) {
      const value = row.value!;
      const hasToken = (value[0] ?? 0) === 1;
      const token = hasToken ? readU64(value, 1, "staged expectation token") : null;
      this.#tx.run(
        "INSERT INTO efs_branch_inode_expectations(branch_id,inode_id,expected_token) VALUES(?,?,?)",
        [branchId, decoder.decode(row.key.subarray(1)), token],
      );
      usage.apply(
        { charged_metadata_bytes: CHARGED_ROW_BYTES },
        "replicated branch expectation install",
      );
    }
    const refs = this.#stagedRows(options.sessionId, 11);
    for (const row of refs) {
      this.#tx.run(
        "INSERT INTO efs_branch_manifest_roots(branch_id,path,manifest_hash) VALUES(?,?,?)",
        [branchId, copyBytes(row.key.subarray(1)), copyBytes(row.value!)],
      );
      usage.apply(
        {
          charged_metadata_bytes: CHARGED_ROW_BYTES + row.key.subarray(1).byteLength,
        },
        "replicated branch manifest ref install",
      );
    }
    const recomputed = this.#recomputeBranchDigest(
      options.sessionId,
      branchId,
      baseRevision,
      generation,
    );
    if (!equalBytes(recomputed, expectedDigest))
      throw transferError(
        "IntegrityFailure",
        `recomputed branch generation digest does not match the authority digest (expected=${bytesToHex(expectedDigest)}, actual=${bytesToHex(recomputed)}, changes=${changes.length}, overlays=${overlays.length}, pages=${pages.length}, patches=${patches.length}, expectations=${expectations.length}, refs=${refs.length})`,
      );
    this.#branches().putTerminalGenerationDigest(branchId, generation, bytesToHex(recomputed));
    let authorityResult: ReplicationAuthorityResult | null = null;
    if (terminalDetails !== null) {
      authorityResult = terminalDetails.authorityResult;
      this.#branches().putTerminalGenerationDigest(
        branchId,
        generation,
        bytesToHex(expectedDigest),
      );
      installTerminalResult(terminalDetails);
      if (fragmentState === 1)
        this.#tx.run(
          "UPDATE efs_branches SET merged_revision=? WHERE id=? AND state=1",
          [terminalDetails.mergedRevision, branchId],
        );
      if (replacingExisting)
        this.#branches().finish(
          branchId,
          fragmentState as 1 | 2,
          options.now,
          terminalDetails.mergedRevision,
        );
    }
    this.#tx.run(
      "UPDATE efs_replication_imports SET installed_revision_count=1 WHERE session_id=?",
      [options.sessionId],
    );
    return Object.freeze({
      revision: String(baseRevision),
      branchId,
      baseRevision: String(baseRevision),
      generation,
      generationDigest: copyBytes(expectedDigest),
      state: fragmentState,
      authorityResult,
      reusedBytes: 0,
    });
  }

  #recomputeBranchDigest(
    sessionId: string,
    branchId: string,
    baseRevision: number,
    generation: number,
  ): Uint8Array {
    const meta = this.#meta();
    const changes = this.#stagedRows(sessionId, 6);
    const overlays = this.#stagedRows(sessionId, 7);
    const pages = this.#stagedRows(sessionId, 8);
    const patches = this.#stagedRows(sessionId, 9);
    const expectations = this.#stagedRows(sessionId, 10);
    const refs = this.#stagedRows(sessionId, 11);
    const nodes = new Map<string, BranchGenerationNode>();
    const digestExpectations: BranchGenerationExpectation[] = [];
    const references = new Map<string, Uint8Array>();
    const overlayDesiredByInode = new Map<string, Record<string, unknown>>();
    const baseInodes = new Map<string, InodeProjectionRow>();
    const baseRows = this.#tx.all<InodeProjectionRow & SqliteRow>(
      "SELECT id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token FROM efs_inodes",
      [],
      { maxRows: 65536, maxBytes: 32 * 1024 * 1024 },
    );
    for (const row of baseRows) baseInodes.set(row.id, row);
    for (const row of overlays) {
      const value = row.value!;
      const hasToken = (value[0] ?? 0) === 1;
      const encoded = value.subarray(hasToken ? 9 : 1);
      const inodeId = decoder.decode(row.key.subarray(1));
      const desired = decodeJson<Record<string, unknown>>(encoded);
      if (!desired) throw transferError("IntegrityFailure", "staged overlay is not JSON");
      overlayDesiredByInode.set(inodeId, desired);
    }
    for (const row of changes) {
      const value = row.value!;
      const kind = value[0] ?? 0;
      const hasToken = (value[1] ?? 0) === 1;
      const token = hasToken ? readU64(value, 2, "staged change token") : null;
      const encodedTag = 2 + (hasToken ? 8 : 0);
      const hasEncoded = (value[encodedTag] ?? 0) === 1;
      const encodedStart = encodedTag + 1;
      const encoded = hasEncoded ? value.subarray(encodedStart) : null;
      let path: string;
      try {
        path = decoder.decode(row.key.subarray(1));
      } catch {
        throw transferError("IntegrityFailure", "staged change path is not UTF-8");
      }
      const rawDesired = encoded ? decodeJson<Record<string, unknown>>(encoded) : undefined;
      const desired =
        rawDesired && typeof rawDesired.inodeId === "string"
          ? { ...rawDesired, ...overlayDesiredByInode.get(rawDesired.inodeId) }
          : rawDesired;
      digestExpectations.push({
        reason:
          desired?.conflictRole === "source"
            ? ("source-changed" as const)
            : desired?.conflictRole === "destination"
              ? ("destination-changed" as const)
              : ("entry-changed" as const),
        path,
        expectedRevision: null,
        expectedToken: token === null ? null : String(token),
      });
      if (desired && typeof desired === "object") {
        if (desired.expectedInodeToken !== null && desired.expectedInodeToken !== undefined)
          digestExpectations.push({
            reason:
              desired.conflictRole === "source"
                ? ("source-changed" as const)
                : desired.conflictRole === "destination"
                  ? ("destination-changed" as const)
                  : ("node-changed" as const),
            path,
            expectedRevision: null,
            expectedToken: String(desired.expectedInodeToken),
          });
        if (typeof desired.sourcePath === "string")
          digestExpectations.push({
            reason: "source-changed" as const,
            path: desired.sourcePath,
            expectedRevision: null,
            expectedToken:
              desired.sourceInodeToken === null ||
              desired.sourceInodeToken === undefined
                ? null
                : String(desired.sourceInodeToken),
          });
        if (desired.subtreeGuard === true)
          digestExpectations.push({
            reason: "subtree-changed" as const,
            path,
            expectedRevision: String(baseRevision),
            expectedToken: null,
          });
        for (const ancestor of (desired.ancestorTokens as
          | readonly { path: string; inodeId: string | null; entryToken: number | null }[]
          | undefined) ?? [])
          digestExpectations.push({
            reason: "ancestor-changed" as const,
            path: ancestor.path,
            expectedRevision: null,
            expectedToken:
              ancestor.entryToken === null ? null : String(ancestor.entryToken),
          });
      }
      if (kind !== 0 || typeof desired?.inodeId !== "string") continue;
      const inodeId = desired.inodeId;
      const base = baseInodes.get(inodeId);
      const manifestHash =
        typeof desired.manifestHash === "string"
          ? hexBytes(desired.manifestHash)
          : base?.manifest_hash
            ? copyBytes(base.manifest_hash)
            : null;
      if (manifestHash) references.set(bytesToHex(manifestHash), manifestHash);
      nodes.set(inodeId, {
        inodeId,
        kind:
          desired.type === 0
            ? "file"
            : desired.type === 1
              ? "directory"
              : "symlink",
        mode: (desired.mode as number) ?? base?.mode ?? 0o755,
        birthtimeMs: (desired.birthtimeMs as number) ?? base?.birthtime_ms ?? 0,
        mtimeMs: (desired.mtimeMs as number) ?? base?.mtime_ms ?? 0,
        ctimeMs: (desired.ctimeMs as number) ?? base?.ctime_ms ?? 0,
        logicalSize: (desired.size as number | null) ?? base?.size ?? 0,
        manifestHash,
        pages: [],
        patches: [],
        symlinkTarget:
          (desired.symlinkTarget as string | null) ?? base?.symlink_target ?? null,
      });
    }
    for (const row of overlays) {
      const value = row.value!;
      const hasToken = (value[0] ?? 0) === 1;
      const token = hasToken ? readU64(value, 1, "staged overlay token") : null;
      const encoded = value.subarray(hasToken ? 9 : 1);
      let inodeId: string;
      try {
        inodeId = decoder.decode(row.key.subarray(1));
      } catch {
        throw transferError("IntegrityFailure", "staged overlay inode is not UTF-8");
      }
      const desired = decodeJson<Record<string, unknown>>(encoded);
      if (!desired) throw transferError("IntegrityFailure", "staged overlay is not JSON");
      const base = baseInodes.get(inodeId);
      const type = (desired.type as number) ?? base?.type ?? 0;
      const logicalSize = (desired.size as number | null) ?? base?.size ?? 0;
      const manifestHash =
        typeof desired.manifestHash === "string"
          ? hexBytes(desired.manifestHash)
          : base?.manifest_hash
            ? copyBytes(base.manifest_hash)
            : null;
      if (manifestHash) references.set(bytesToHex(manifestHash), manifestHash);
      const generationPages: { index: number; bytes: Uint8Array }[] = [];
      for (const pageRow of pages) {
        const rest = pageRow.key.subarray(1);
        const inodeLength = readU32Length(rest, 0, "staged page inode");
        const pageInode = decoder.decode(rest.subarray(4, 4 + inodeLength));
        if (pageInode !== inodeId) continue;
        const pageIndex = readU64(rest, 4 + inodeLength, "staged page index");
        const pageValue = pageRow.value!;
        const bytesLength = pageValue.byteLength - 9;
        const head = (pageValue[pageValue.byteLength - 1] ?? 0) === 1;
        if (!head) continue;
        generationPages.push({
          index: pageIndex,
          bytes: copyBytes(pageValue.subarray(0, bytesLength)),
        });
      }
      generationPages.sort((left, right) => left.index - right.index);
      const generationPatches: {
        order: number;
        offset: number;
        deleteLength: number;
        insertManifestDigest: Uint8Array | null;
      }[] = [];
      for (const patchRow of patches) {
        const rest = patchRow.key.subarray(1);
        const inodeLength = readU32Length(rest, 0, "staged patch inode");
        const patchInode = decoder.decode(rest.subarray(4, 4 + inodeLength));
        if (patchInode !== inodeId) continue;
        const sequence = readU64(rest, 4 + inodeLength, "staged patch sequence");
        const patchValue = patchRow.value!;
        const view = new DataView(patchValue.buffer, patchValue.byteOffset, patchValue.byteLength);
        const patchOffset = Number(view.getBigUint64(8, false));
        const deleteLength = Number(view.getBigUint64(16, false));
        const segmentCount = view.getUint32(32, false);
        let cursor = 36;
        const segments: Uint8Array[] = [];
        for (let index = 0; index < segmentCount; index += 1) {
          const length = view.getUint32(cursor, false);
          segments.push(copyBytes(patchValue.subarray(cursor + 4, cursor + 4 + length)));
          cursor += 4 + length;
        }
        const insertDigest = branchPatchInsertDigest(segments);
        if (insertDigest) references.set(bytesToHex(insertDigest), insertDigest);
        generationPatches.push({
          order: sequence,
          offset: patchOffset,
          deleteLength,
          insertManifestDigest: insertDigest,
        });
      }
      nodes.set(inodeId, {
        inodeId,
        kind: type === 0 ? "file" : type === 1 ? "directory" : "symlink",
        mode: (desired.mode as number) ?? base?.mode ?? 0o755,
        birthtimeMs: (desired.birthtimeMs as number) ?? base?.birthtime_ms ?? 0,
        mtimeMs: (desired.mtimeMs as number) ?? base?.mtime_ms ?? 0,
        ctimeMs: (desired.ctimeMs as number) ?? base?.ctime_ms ?? 0,
        logicalSize,
        manifestHash,
        pages: generationPages,
        patches: generationPatches,
        symlinkTarget:
          (desired.symlinkTarget as string | null) ?? base?.symlink_target ?? null,
      });
      void token;
    }
    for (const row of expectations) {
      const value = row.value!;
      const hasToken = (value[0] ?? 0) === 1;
      const token = hasToken ? readU64(value, 1, "staged expectation token") : null;
      let inodeId: string;
      try {
        inodeId = decoder.decode(row.key.subarray(1));
      } catch {
        throw transferError("IntegrityFailure", "staged expectation inode is not UTF-8");
      }
      const change = changes.find((changeRow) => {
        const changeValue = changeRow.value!;
        const changeHasEncoded =
          changeValue[2 + (changeValue[1] === 1 ? 8 : 0)] === 1;
        if (!changeHasEncoded) return false;
        const start = 3 + (changeValue[1] === 1 ? 8 : 0);
        const desired = decodeJson<Record<string, unknown>>(changeValue.subarray(start));
        return desired?.inodeId === inodeId;
      });
      void change;
      digestExpectations.push({
        reason: "node-changed" as const,
        path: inodeId,
        expectedRevision: null,
        expectedToken: token === null ? null : String(token),
      });
    }
    for (const row of refs) {
      references.set(bytesToHex(row.value!), copyBytes(row.value!));
    }
    const namespace = changes.map((row) => {
      const value = row.value!;
      const kind = value[0] ?? 0;
      const hasToken = (value[1] ?? 0) === 1;
      const encodedTag = 2 + (hasToken ? 8 : 0);
      const hasEncoded = (value[encodedTag] ?? 0) === 1;
      const encodedStart = encodedTag + 1;
      const encoded = hasEncoded ? value.subarray(encodedStart) : null;
      const desired = encoded ? decodeJson<Record<string, unknown>>(encoded) : undefined;
      let path: string;
      try {
        path = decoder.decode(row.key.subarray(1));
      } catch {
        throw transferError("IntegrityFailure", "staged change path is not UTF-8");
      }
      return {
        path,
        disposition: kind === 0 ? ("present" as const) : ("tombstone" as const),
        inodeId:
          kind === 0 && desired && typeof desired.inodeId === "string"
            ? desired.inodeId
            : null,
      };
    });
    const digest = computeBranchGenerationDigest({
      filesystemId: meta.filesystem_id,
      branchId,
      baseRevision: String(baseRevision),
      generation,
      namespace,
      nodes: [...nodes.values()],
      expectations: digestExpectations,
      immutableReferences: [...references.values()].map((digest) => ({
        kind: "manifest" as const,
        digest,
      })),
    });
    return hexBytes(digest);
  }

  #finalizeGenesis(
    options: {
      readonly sessionId: string;
      readonly expectedRevision: number;
      readonly expectedRootMutationGeneration: number;
      readonly expectedNextAllocationSequence: number;
      readonly expectedRootInode: string;
      readonly genesisMeta: ReplicationExportMeta | null;
      readonly genesisRows: readonly {
        readonly inodeId: string;
        readonly tombstone: boolean;
        readonly encoded: Uint8Array | null;
      }[];
      readonly now: number;
    },
    importRow: ImportRow,
  ): Readonly<{
    readonly revision: string;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number;
    readonly generationDigest: Uint8Array | null;
    readonly state: 0 | 1 | 2;
    readonly authorityResult: ReplicationAuthorityResult | null;
    readonly reusedBytes: number;
  }> {
    const metaRows = this.#tx.all<{ count: number } & SqliteRow>(
      "SELECT count(*) count FROM efs_meta",
      [],
      { maxRows: 1, maxBytes: 256 },
    );
    if (metaRows[0]!.count !== 0)
      throw transferError("ProvisioningRejected", "database is already bound");
    const genesis = options.genesisMeta;
    if (!genesis)
      throw transferError("ProvisioningRejected", "genesis metadata is missing");
    if (genesis.mainRevision !== 0)
      throw transferError("ProvisioningRejected", "genesis is not revision zero");
    if (options.expectedRevision !== 0)
      throw transferError("ProvisioningRejected", "provisioning adopts revision zero only");
    if (options.expectedRootInode !== genesis.rootInode)
      throw transferError("ProvisioningRejected", "genesis root inode mismatch");
    const stagedGenesisRows = options.genesisRows.length > 0
      ? options.genesisRows
      : this.#stagedRows(options.sessionId, 2).map((row) => {
          if (row.key.byteLength < 10 || row.key[0] !== 2 || readU64(row.key, 1, "genesis staged revision") !== 0)
            throw transferError("IntegrityFailure", "staged genesis inode key is invalid");
          let inodeId: string;
          try {
            inodeId = decoder.decode(row.key.subarray(9));
          } catch {
            throw transferError("IntegrityFailure", "staged genesis inode id is not UTF-8");
          }
          return {
            inodeId,
            tombstone: (row.value?.[0] ?? 0) === 1,
            encoded: row.value && row.value.byteLength > 1 ? copyBytes(row.value.subarray(1)) : null,
          };
        });
    this.#tx.run(
      "INSERT INTO efs_meta(singleton,schema_version,filesystem_id,main_revision,root_inode,root_mutation_generation,next_allocation_sequence,cow_page_bytes,created_at_ms,last_root_removal_generation,max_manifest_entries,max_manifest_depth,max_file_bytes,writer_profile) VALUES(1,13,?,?,?,?,?,?,?,?,?,?,?,?)",
      [
        genesis.filesystemId,
        0,
        genesis.rootInode,
        genesis.rootMutationGeneration,
        genesis.nextAllocationSequence,
        genesis.cowPageBytes,
        genesis.createdAtMs,
        genesis.rootMutationGeneration,
        genesis.maxManifestEntries,
        genesis.maxManifestDepth,
        genesis.maxFileBytes,
        genesis.writerProfile,
      ],
    );
    this.#tx.run(
      "INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(0,NULL,?,'bootstrap',1)",
      [genesis.createdAtMs],
    );
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(0,0,'0')",
    );
    this.#tx.run(
      "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,?,?,?,?,?,?,NULL,NULL,NULL,?)",
      [
        genesis.rootInode,
        genesis.rootInodeType,
        genesis.rootMode,
        genesis.rootBirthtimeMs,
        genesis.rootMtimeMs,
        genesis.rootCtimeMs,
        1,
        genesis.rootToken,
      ],
    );
    for (const row of stagedGenesisRows) {
      if (row.tombstone) {
        this.#tx.run(
          "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(0,?,1,NULL)",
          [row.inodeId],
        );
        continue;
      }
      this.#tx.run(
        "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(0,?,0,?)",
        [row.inodeId, row.encoded],
      );
    }
    this.#tx.run(
      "DELETE FROM efs_replication_sessions WHERE id=? AND state=-1",
      ["efs-unbound-replica-v1"],
    );
    return Object.freeze({
      revision: "0",
      branchId: null,
      baseRevision: null,
      generation: 0,
      generationDigest: null,
      state: 0,
      authorityResult: null,
      reusedBytes: 0,
    });
  }

  captureGenesis(options: {
    readonly sessionId: string;
    readonly now: number;
    readonly expiresAt: number;
  }): Readonly<{
    readonly meta: ReplicationExportMeta;
    readonly rows: readonly {
      readonly inodeId: string;
      readonly tombstone: boolean;
      readonly encoded: Uint8Array | null;
    }[];
  }> {
    const meta = this.#meta();
    const root = this.#tx.all<InodeProjectionRow & SqliteRow>(
      "SELECT id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token FROM efs_inodes WHERE id=?",
      [meta.root_inode],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!root) throw transferError("ECORRUPT", "root inode is missing");
    const anyRoot = this.#tx.all<
      { chunk_min: number; chunk_avg: number; chunk_max: number } & SqliteRow
    >(
      "SELECT chunk_min,chunk_avg,chunk_max FROM efs_manifest_roots ORDER BY allocation_sequence LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    const exported: ReplicationExportMeta = {
      filesystemId: meta.filesystem_id,
      rootInode: meta.root_inode,
      mainRevision: 0,
      rootMutationGeneration: meta.root_mutation_generation,
      nextAllocationSequence: meta.next_allocation_sequence,
      cowPageBytes: meta.cow_page_bytes,
      createdAtMs: meta.created_at_ms,
      maxManifestEntries: meta.max_manifest_entries,
      maxManifestDepth: meta.max_manifest_depth,
      maxFileBytes: meta.max_file_bytes,
      writerProfile: meta.writer_profile,
      manifestFormat: MANIFEST_FORMAT,
      chunkerFormat: CHUNKER_FORMAT,
      fastCdcMinimum: anyRoot?.chunk_min ?? DEFAULT_FASTCDC_MINIMUM,
      fastCdcAverage: anyRoot?.chunk_avg ?? DEFAULT_FASTCDC_AVERAGE,
      fastCdcMaximum: anyRoot?.chunk_max ?? DEFAULT_FASTCDC_MAXIMUM,
      rootInodeType: root.type,
      rootMode: root.mode,
      rootBirthtimeMs: root.birthtime_ms,
      rootMtimeMs: root.mtime_ms,
      rootCtimeMs: root.ctime_ms,
      rootToken: root.token,
    };
    const exportMetadata = {
      ...exported,
      genesisCursor: {
        revision: 0,
        kind: 1,
        inodeId: null,
        parentInode: null,
        nameSortHex: null,
        fragmentIndex: 0,
      },
      genesisComplete: false,
    };
    this.#tx.run(
      "INSERT INTO efs_replication_exports(session_id,kind,selected_identity,selected_generation,base_revision,target_revision,root_mutation_generation,next_allocation_sequence,root_inode,meta_json,revision_cursor,mark_kind,mark_hash,mark_edge,root_count,node_count,object_count,object_bytes,offered_roots,offered_nodes,offered_objects,state_rows,done) VALUES(?,2,?,0,0,0,0,1,?,?,0,0,NULL,0,0,0,0,0,0,0,0,0,0)",
      [
        options.sessionId,
        meta.filesystem_id,
        meta.root_inode,
        encodeJson(exportMetadata),
      ],
    );
    return Object.freeze({
      meta: exported,
      rows: [],
    });
  }
}

function readU32Length(bytes: Uint8Array, offset: number, name: string): number {
  if (offset + 4 > bytes.byteLength) throw new RangeError(`truncated ${name}`);
  return new DataView(bytes.buffer, bytes.byteOffset + offset, 4).getUint32(0, false);
}

interface TransferRevisionFragmentDecoded {
  readonly revisionId: string;
  readonly parentRevisionId: string | null;
  readonly created_at_ms: number;
  readonly writerId: string;
  readonly changeCount: number;
  readonly rows: readonly TransferNamespaceRow[];
}

function decodeRevisionFragment(bytes: Uint8Array): TransferRevisionFragmentDecoded {
  const view = new FragmentDecoder(bytes);
  const version = view.uint8("revision fragment version");
  if (version !== 1) throw new RangeError("revision fragment version is not canonical");
  const revisionId = view.text("revision id");
  const parentRevisionId = view.optional(() => view.text("parent revision id"));
  const created_at_ms = view.uint64("revision creation time");
  const writerId = view.text("writer id");
  const changeCount = view.uint64("revision change count");
  const rowCount = view.uint32("revision row count");
  if (rowCount > 256) throw new RangeError("revision row count exceeds the envelope");
  const rows: TransferNamespaceRow[] = [];
  for (let index = 0; index < rowCount; index += 1) {
    const kind = view.uint8("namespace row kind");
    if (kind === 1) {
      rows.push({
        kind: 1,
        inodeId: view.text("inode id"),
        tombstone: view.boolean("inode tombstone"),
        encoded: view.bytesOrNull("inode encoded"),
      });
    } else if (kind === 2) {
      rows.push({
        kind: 2,
        parentInode: view.text("parent inode"),
        nameSort: view.bytes("name sort"),
        tombstone: view.boolean("entry tombstone"),
        encoded: view.bytesOrNull("entry encoded"),
      });
    } else if (kind === 3) {
      rows.push({
        kind: 3,
        inodeId: view.text("inode id"),
        manifestHash: view.digest("manifest ref"),
      });
    } else throw new RangeError("namespace row kind is not canonical");
  }
  if (view.remaining() !== 0)
    throw new RangeError("revision fragment has trailing bytes");
  return { revisionId, parentRevisionId, created_at_ms, writerId, changeCount, rows };
}

function decodeBranchGenerationFragment(bytes: Uint8Array): {
  readonly branchId: string;
  readonly baseRevision: string;
  readonly generation: number;
  readonly generationDigest: Uint8Array;
  readonly previousGeneration: number | null;
  readonly previousGenerationDigest: Uint8Array | null;
  readonly state: number;
  readonly rows: readonly TransferBranchRow[];
} {
  const view = new FragmentDecoder(bytes);
  const version = view.uint8("branch fragment version");
  if (version !== 1) throw new RangeError("branch fragment version is not canonical");
  const branchId = view.text("branch id");
  const baseRevision = view.text("base revision");
  const generation = view.uint64("branch generation");
  const generationDigest = view.digest("branch generation digest");
  const previousGeneration = view.optional(() => view.uint64("branch predecessor generation"));
  const previousGenerationDigest = view.optional(() => view.digest("branch predecessor digest"));
  if ((previousGeneration === null) !== (previousGenerationDigest === null))
    throw new RangeError("branch predecessor generation and digest must be present together");
  const state = view.uint8("branch state");
  if (state > 2) throw new RangeError("branch state is not canonical");
  const rowCount = view.uint32("branch row count");
  if (rowCount > 256) throw new RangeError("branch row count exceeds the envelope");
  const rows: TransferBranchRow[] = [];
  for (let index = 0; index < rowCount; index += 1) {
    const kind = view.uint8("branch row kind");
    if (kind === 1) {
      const disposition = view.uint8("change disposition");
      rows.push({
        kind: 1,
        path: view.bytes("change path"),
        disposition,
        expectedToken: view.optional(() => view.uint64("change expected token")),
        encoded: view.optional(() => view.bytes("change encoded")),
      });
    } else if (kind === 2)
      rows.push({
        kind: 2,
        inodeId: view.text("overlay inode"),
        expectedToken: view.optional(() => view.uint64("overlay expected token")),
        encoded: view.bytes("overlay encoded"),
      });
    else if (kind === 3)
      rows.push({
        kind: 3,
        inodeId: view.text("page inode"),
        pageIndex: view.uint64("page index"),
        generation: view.uint64("page generation"),
        bytes: view.bytes("page bytes"),
        created_at_ms: view.uint64("page creation time"),
        head: view.boolean("page head"),
      });
    else if (kind === 4) {
      const inodeId = view.text("patch inode");
      const sequence = view.uint64("patch sequence");
      const patchGeneration = view.uint64("patch generation");
      const offset = view.uint64("patch offset");
      const deleteLength = view.uint64("patch delete length");
      const insertLength = view.uint64("patch insert length");
      const segmentCount = view.uint32("patch segment count");
      if (segmentCount > 64)
        throw new RangeError("patch segment count exceeds the envelope");
      const segments: Uint8Array[] = [];
      for (let segment = 0; segment < segmentCount; segment += 1)
        segments.push(view.bytes("patch segment"));
      rows.push({
        kind: 4,
        inodeId,
        sequence,
        generation: patchGeneration,
        offset,
        deleteLength,
        insertLength,
        segments,
      });
    } else if (kind === 5)
      rows.push({
        kind: 5,
        inodeId: view.text("expectation inode"),
        expectedToken: view.optional(() => view.uint64("expectation token")),
      });
    else if (kind === 6)
      rows.push({
        kind: 6,
        path: view.bytes("ref path"),
        manifestHash: view.digest("branch manifest ref"),
      });
    else throw new RangeError("branch row kind is not canonical");
  }
  if (view.remaining() !== 0)
    throw new RangeError("branch fragment has trailing bytes");
  return {
    branchId,
    baseRevision,
    generation,
    generationDigest,
    previousGeneration,
    previousGenerationDigest,
    state,
    rows,
  };
}

class FragmentDecoder {
  readonly #value: Uint8Array;
  #offset = 0;
  constructor(value: Uint8Array) {
    this.#value = value;
  }
  remaining(): number {
    return this.#value.byteLength - this.#offset;
  }
  #take(length: number, name: string): Uint8Array {
    if (length < 0 || this.#offset + length > this.#value.byteLength)
      throw new RangeError(`truncated ${name}`);
    const out = this.#value.subarray(this.#offset, this.#offset + length);
    this.#offset += length;
    return out;
  }
  uint8(name: string): number {
    return this.#take(1, name)[0]!;
  }
  boolean(name: string): boolean {
    const value = this.uint8(name);
    if (value !== 0 && value !== 1)
      throw new RangeError(`${name} is not a canonical boolean`);
    return value === 1;
  }
  uint32(name: string): number {
    const bytes = this.#take(4, name);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, false);
  }
  uint64(name: string): number {
    const bytes = this.#take(8, name);
    const value = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0, false);
    if (value > BigInt(Number.MAX_SAFE_INTEGER))
      throw new RangeError(`${name} exceeds the safe integer envelope`);
    return Number(value);
  }
  digest(name: string): Uint8Array {
    return copyBytes(this.#take(32, name));
  }
  bytes(name: string): Uint8Array {
    const length = this.uint32(name);
    return copyBytes(this.#take(length, `${name} bytes`));
  }
  bytesOrNull(name: string): Uint8Array | null {
    const length = this.uint32(name);
    if (length === 0) return null;
    return copyBytes(this.#take(length, `${name} bytes`));
  }
  text(name: string): string {
    const bytes = this.bytes(name);
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new RangeError(`${name} is not well-formed UTF-8`);
    }
  }
  optional<T>(read: () => T): T | null {
    const tag = this.uint8("optional tag");
    if (tag === 0) return null;
    if (tag !== 1) throw new RangeError("optional tag is not canonical");
    return read();
  }
}

export function createReplicationTransferRepository(
  tx: FilesystemSQLiteTransaction,
  limits: StorageLimits,
  hashBytes: (bytes: Uint8Array) => Uint8Array,
  maxBindings: number,
  branchDigest?: (branchId: string, generation: number) => string,
  cache?: ContentCache,
): ReplicationTransferStore {
  return new ReplicationTransferRepository(
    tx,
    limits,
    hashBytes,
    maxBindings,
    branchDigest,
    cache,
  );
}

import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  MAINTENANCE_TOTAL_EMERGENCY_BYTES,
  type StorageLimits,
} from "../resources/limits.js";
import {
  beginUsageMutationBatch,
  CHARGED_ROW_BYTES,
  flushUsageMutationBatch,
  UsageRepository,
} from "./usage-repository.js";
import {
  bytesToHex,
  equalBytes,
  hexToBytes,
  intrinsicByteLength,
  intrinsicByteRange,
} from "../cas/bytes.js";
import { sha256 } from "../cas/sha256.js";
import {
  validateBranchIdentifier,
  validateDurableIdentifier,
  validateOperationIdentifier,
} from "./identifiers.js";
import { encodeUtf8 } from "../namespace/utf8.js";
import { advanceRootMutationGeneration } from "./namespace-repository.js";

const checkpointDecoder = new TextDecoder();
const TERMINAL_BRANCH_METADATA_STATE = -2;
const TERMINAL_BRANCH_METADATA_PREFIX = "efs-system-branch-terminal-v1:";
const TERMINAL_BRANCH_METADATA_MAGIC = Uint8Array.of(
  0x45,
  0x46,
  0x53,
  0x42,
  0x54,
  0x44,
  0x31,
  0x00,
);

function terminalBranchMetadataId(branchId: string): string {
  validateBranchIdentifier(branchId);
  return `${TERMINAL_BRANCH_METADATA_PREFIX}${bytesToHex(sha256(encodeUtf8(branchId)))}`;
}

function encodeTerminalBranchMetadata(
  branchId: string,
  generation: number,
  digest: string,
): Uint8Array {
  validateBranchIdentifier(branchId);
  if (!Number.isSafeInteger(generation) || generation < 0)
    throw new RangeError("invalid terminal branch generation");
  if (!/^[0-9a-f]{64}$/.test(digest))
    throw new RangeError("invalid terminal branch generation digest");
  const branchBytes = encodeUtf8(branchId);
  const output = new Uint8Array(8 + 4 + branchBytes.byteLength + 8 + 32);
  output.set(TERMINAL_BRANCH_METADATA_MAGIC);
  const view = new DataView(output.buffer);
  view.setUint32(8, branchBytes.byteLength, false);
  output.set(branchBytes, 12);
  view.setBigUint64(12 + branchBytes.byteLength, BigInt(generation), false);
  output.set(hexToBytes(digest, 32), 20 + branchBytes.byteLength);
  return output;
}

function decodeTerminalBranchMetadata(value: Uint8Array): Readonly<{
  branchId: string;
  generation: number;
  digest: string;
}> {
  if (
    !(value instanceof Uint8Array) ||
    value.byteLength < 52 ||
    !equalBytes(value.subarray(0, 8), TERMINAL_BRANCH_METADATA_MAGIC)
  )
    throw new Error("ECORRUPT: invalid terminal branch metadata");
  const view = new DataView(value.buffer, value.byteOffset, value.byteLength);
  const branchLength = view.getUint32(8, false);
  const expectedLength = 8 + 4 + branchLength + 8 + 32;
  if (branchLength === 0 || expectedLength !== value.byteLength)
    throw new Error("ECORRUPT: invalid terminal branch metadata length");
  let branchId: string;
  try {
    branchId = new TextDecoder("utf-8", { fatal: true }).decode(
      value.subarray(12, 12 + branchLength),
    );
  } catch {
    throw new Error("ECORRUPT: invalid terminal branch metadata identifier");
  }
  validateBranchIdentifier(branchId);
  const generationValue = view.getBigUint64(12 + branchLength, false);
  if (generationValue > BigInt(Number.MAX_SAFE_INTEGER))
    throw new Error("ECORRUPT: invalid terminal branch metadata generation");
  return Object.freeze({
    branchId,
    generation: Number(generationValue),
    digest: bytesToHex(value.subarray(20 + branchLength)),
  });
}

export interface BranchRow extends SqliteRow {
  id: string;
  base_revision: number;
  state: number;
  generation: number;
  created_at_ms: number;
  terminal_at_ms: number | null;
  merged_revision: number | null;
}
export interface BranchHistoryRow extends SqliteRow {
  tombstone: number;
  encoded: Uint8Array | null;
}
export interface BranchHistoryEntryRow extends SqliteRow {
  name_sort: Uint8Array;
  tombstone: number;
  encoded: Uint8Array | null;
}
export interface BranchChangeRow extends SqliteRow {
  path: Uint8Array;
  expected_token: number | null;
  kind: number;
  encoded: Uint8Array | null;
}
export interface BranchResultRow extends SqliteRow {
  branch_id: string;
  generation: number;
  reservation_nonce: Uint8Array;
  outcome: number;
  encoded: Uint8Array | null;
  expires_at_ms: number | null;
}
interface CheckpointRow extends SqliteRow {
  target_revision: number;
  state: number;
  phase: number;
  inode_cursor: string | null;
  entry_parent: string | null;
  entry_name_sort: Uint8Array | null;
  inode_count: number;
  entry_count: number;
}
interface CheckpointInodeRow extends SqliteRow {
  inode_id: string;
  tombstone: number;
  encoded: Uint8Array | null;
  revision: number;
}
interface CheckpointEntryRow extends SqliteRow {
  parent_inode: string;
  name_sort: Uint8Array;
  tombstone: number;
  encoded: Uint8Array | null;
  revision: number;
}

export class BranchRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) {
    this.#tx = tx;
    this.#limits = limits;
  }
  filesystemId(): string {
    const value = this.#tx.all<{ filesystem_id: string } & SqliteRow>(
      "SELECT filesystem_id FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.filesystem_id;
    if (typeof value !== "string" || value.length === 0)
      throw new Error("ECORRUPT: filesystem identifier is missing");
    return value;
  }
  rootInodeId(): string {
    const value = this.#tx.all<{ root_inode: string } & SqliteRow>(
      "SELECT root_inode FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.root_inode;
    if (!value) throw new Error("ECORRUPT: missing metadata");
    return value;
  }
  historyEntries(
    parentInode: string,
    revision: number,
  ): readonly BranchHistoryEntryRow[] {
    const checkpoint = this.#checkpointAt(revision);
    const rows = this.#tx.all<BranchHistoryEntryRow & { revision: number }>(
      checkpoint === null
        ? "SELECT r.name_sort,r.tombstone,r.encoded,r.revision FROM efs_entry_revisions r WHERE r.parent_inode=? AND r.revision=(SELECT max(x.revision) FROM efs_entry_revisions x WHERE x.parent_inode=r.parent_inode AND x.name_sort=r.name_sort AND x.revision<=?)"
        : "SELECT name_sort,tombstone,encoded,target_revision revision FROM efs_checkpoint_entries WHERE target_revision=? AND parent_inode=? UNION ALL SELECT r.name_sort,r.tombstone,r.encoded,r.revision FROM efs_entry_revisions r WHERE r.parent_inode=? AND r.revision>? AND r.revision<=?",
      checkpoint === null
        ? [parentInode, revision]
        : [checkpoint, parentInode, parentInode, checkpoint, revision],
      { maxRows: 100_001, maxBytes: 16 * 1024 * 1024 },
    );
    const latest = new Map<string, BranchHistoryEntryRow & { revision: number }>();
    for (const row of rows) {
      const key = `${row.name_sort.byteLength}:${Array.from(row.name_sort).join(",")}`;
      const prior = latest.get(key);
      if (!prior || row.revision > prior.revision) latest.set(key, row);
    }
    return [...latest.values()];
  }
  historicEntry(
    parentInode: string,
    nameSort: Uint8Array,
    revision: number,
  ): BranchHistoryRow | undefined {
    const checkpoint = this.#checkpointAt(revision);
    return this.#tx.all<BranchHistoryRow>(
      checkpoint === null
        ? "SELECT tombstone,encoded FROM efs_entry_revisions WHERE parent_inode=? AND name_sort=? AND revision<=? ORDER BY revision DESC LIMIT 1"
        : "SELECT tombstone,encoded FROM (SELECT tombstone,encoded,target_revision revision FROM efs_checkpoint_entries WHERE target_revision=? AND parent_inode=? AND name_sort=? UNION ALL SELECT tombstone,encoded,revision FROM efs_entry_revisions WHERE parent_inode=? AND name_sort=? AND revision>? AND revision<=?) ORDER BY revision DESC LIMIT 1",
      checkpoint === null
        ? [parentInode, nameSort, revision]
        : [
            checkpoint,
            parentInode,
            nameSort,
            parentInode,
            nameSort,
            checkpoint,
            revision,
          ],
      { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes * 2 + 1024 },
    )[0];
  }
  historicInode(inodeId: string, revision: number): BranchHistoryRow | undefined {
    const checkpoint = this.#checkpointAt(revision);
    return this.#tx.all<BranchHistoryRow>(
      checkpoint === null
        ? "SELECT tombstone,encoded FROM efs_inode_revisions WHERE inode_id=? AND revision<=? ORDER BY revision DESC LIMIT 1"
        : "SELECT tombstone,encoded FROM (SELECT tombstone,encoded,target_revision revision FROM efs_checkpoint_inodes WHERE target_revision=? AND inode_id=? UNION ALL SELECT tombstone,encoded,revision FROM efs_inode_revisions WHERE inode_id=? AND revision>? AND revision<=?) ORDER BY revision DESC LIMIT 1",
      checkpoint === null
        ? [inodeId, revision]
        : [checkpoint, inodeId, inodeId, checkpoint, revision],
      { maxRows: 1, maxBytes: this.#limits.maxManifestNodeBytes * 2 + 1024 },
    )[0];
  }
  inodeOverlay(
    branchId: string,
    inodeId: string,
    maxBytes: number,
  ): Uint8Array | undefined {
    return this.#tx.all<{ encoded: Uint8Array } & SqliteRow>(
      "SELECT encoded FROM efs_branch_inode_overlays WHERE branch_id=? AND inode_id=?",
      [branchId, inodeId],
      { maxRows: 1, maxBytes },
    )[0]?.encoded;
  }
  change(branchId: string, path: Uint8Array): BranchChangeRow | undefined {
    return this.#tx.all<BranchChangeRow>(
      "SELECT path,expected_token,kind,encoded FROM efs_branch_changes WHERE branch_id=? AND path=?",
      [branchId, path],
      { maxRows: 1, maxBytes: 16 * 1024 },
    )[0];
  }
  changes(branchId: string): readonly BranchChangeRow[] {
    return this.#tx.all<BranchChangeRow>(
      "SELECT path,expected_token,kind,encoded FROM efs_branch_changes WHERE branch_id=? ORDER BY path",
      [branchId],
      { maxRows: 100_001, maxBytes: 16 * 1024 * 1024 },
    );
  }
  activeCount(): number {
    return (
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_branches WHERE state=0",
        [],
        { maxRows: 1, maxBytes: 1024 },
      )[0]?.count ?? 0
    );
  }
  headRevision(): number {
    const value = this.#tx.all<{ revision: number } & SqliteRow>(
      "SELECT main_revision revision FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0]?.revision;
    if (!Number.isSafeInteger(value))
      throw new Error("ECORRUPT: invalid head revision");
    return value!;
  }
  revisionExists(revision: number): boolean {
    return (
      this.#tx.all(
        "SELECT revision FROM efs_revisions WHERE revision=? AND (NOT EXISTS(SELECT 1 FROM efs_revision_checkpoints WHERE state=1) OR revision>=(SELECT max(target_revision) FROM efs_revision_checkpoints WHERE state=1 AND target_revision<=?))",
        [revision, revision],
        {
          maxRows: 1,
          maxBytes: 1024,
        },
      ).length === 1
    );
  }
  create(id: string, baseRevision: number, now: number): BranchRow {
    validateBranchIdentifier(id);
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        permanent_identifiers: 1,
        charged_metadata_bytes: CHARGED_ROW_BYTES * 2,
      },
      "branch identifier",
    );
    this.#tx.run("INSERT INTO efs_branch_ids(id,created_at_ms) VALUES(?,?)", [id, now]);
    this.#tx.run(
      "INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision) VALUES(?,?,0,0,?,NULL,NULL)",
      [id, baseRevision, now],
    );
    this.#bumpRoot(1, id, false);
    return this.row(id)!;
  }
  row(id: string): BranchRow | undefined {
    return this.#tx.all<BranchRow>(
      "SELECT id,base_revision,state,generation,created_at_ms,terminal_at_ms,merged_revision FROM efs_branches WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  terminalGenerationDigest(branchId: string, generation: number): string | undefined {
    const id = terminalBranchMetadataId(branchId);
    const row = this.#tx.all<
      {
        state: number;
        nonce: Uint8Array;
        cursor: Uint8Array;
        expires_at_ms: number;
        staged_bytes: number;
      } & SqliteRow
    >(
      "SELECT state,nonce,cursor,expires_at_ms,staged_bytes FROM efs_replication_sessions WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) return undefined;
    if (
      row.state !== TERMINAL_BRANCH_METADATA_STATE ||
      !(row.nonce instanceof Uint8Array) ||
      row.nonce.byteLength !== 16 ||
      !(row.cursor instanceof Uint8Array) ||
      row.expires_at_ms !== Number.MAX_SAFE_INTEGER ||
      row.staged_bytes !== 0 ||
      !equalBytes(row.nonce, sha256(row.cursor).subarray(0, 16))
    )
      throw new Error("ECORRUPT: invalid terminal branch metadata row");
    const decoded = decodeTerminalBranchMetadata(row.cursor);
    if (decoded.branchId !== branchId || decoded.generation !== generation)
      throw new Error("ECORRUPT: terminal branch metadata binding changed");
    return decoded.digest;
  }
  storedGenerationDigest(branchId: string): Readonly<{
    readonly generation: number;
    readonly digest: string;
    readonly cursorBytes: number;
  }> | undefined {
    const id = terminalBranchMetadataId(branchId);
    const row = this.#tx.all<
      { state: number; nonce: Uint8Array; cursor: Uint8Array; expires_at_ms: number; staged_bytes: number } & SqliteRow
    >(
      "SELECT state,nonce,cursor,expires_at_ms,staged_bytes FROM efs_replication_sessions WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) return undefined;
    if (
      row.state !== TERMINAL_BRANCH_METADATA_STATE ||
      !(row.nonce instanceof Uint8Array) ||
      row.nonce.byteLength !== 16 ||
      !(row.cursor instanceof Uint8Array) ||
      row.expires_at_ms !== Number.MAX_SAFE_INTEGER ||
      row.staged_bytes !== 0 ||
      !equalBytes(row.nonce, sha256(row.cursor).subarray(0, 16))
    )
      throw new Error("ECORRUPT: invalid terminal branch metadata row");
    const decoded = decodeTerminalBranchMetadata(row.cursor);
    if (decoded.branchId !== branchId)
      throw new Error("ECORRUPT: terminal branch metadata identifier changed");
    return Object.freeze({
      generation: decoded.generation,
      digest: decoded.digest,
      cursorBytes: row.cursor.byteLength,
    });
  }
  putTerminalGenerationDigest(
    branchId: string,
    generation: number,
    digest: string,
  ): void {
    const id = terminalBranchMetadataId(branchId);
    const cursor = encodeTerminalBranchMetadata(branchId, generation, digest);
    const priorRow = this.storedGenerationDigest(branchId);
    if (priorRow && priorRow.generation === generation) {
      if (priorRow.digest !== digest)
        throw new Error("ECORRUPT: terminal branch generation digest changed");
      return;
    }
    if (priorRow) {
      beginUsageMutationBatch(this.#tx, this.#limits);
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: cursor.byteLength - priorRow.cursorBytes },
        "terminal branch generation metadata replacement",
      );
      this.#tx.run(
        "UPDATE efs_replication_sessions SET nonce=?,cursor=? WHERE id=? AND state=?",
        [sha256(cursor).subarray(0, 16), cursor, id, TERMINAL_BRANCH_METADATA_STATE],
      );
      flushUsageMutationBatch(this.#tx, this.#limits);
      return;
    }
    beginUsageMutationBatch(this.#tx, this.#limits);
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        permanent_identifiers: 1,
        charged_metadata_bytes: CHARGED_ROW_BYTES + cursor.byteLength,
      },
      "terminal branch generation metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_replication_sessions(id,state,nonce,cursor,expires_at_ms,staged_bytes) VALUES(?,?,?,?,?,0)",
      [
        id,
        TERMINAL_BRANCH_METADATA_STATE,
        sha256(cursor).subarray(0, 16),
        cursor,
        Number.MAX_SAFE_INTEGER,
      ],
    );
  }
  #deleteTerminalGenerationDigest(branchId: string): void {
    const id = terminalBranchMetadataId(branchId);
    const row = this.#tx.all<{ cursor: Uint8Array } & SqliteRow>(
      "SELECT cursor FROM efs_replication_sessions WHERE id=? AND state=?",
      [id, TERMINAL_BRANCH_METADATA_STATE],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) return;
    if (!(row.cursor instanceof Uint8Array))
      throw new Error("ECORRUPT: terminal branch metadata row is invalid");
    const decoded = decodeTerminalBranchMetadata(row.cursor);
    if (decoded.branchId !== branchId)
      throw new Error("ECORRUPT: terminal branch metadata identifier changed");
    const deleted = this.#tx.run(
      "DELETE FROM efs_replication_sessions WHERE id=? AND state=?",
      [id, TERMINAL_BRANCH_METADATA_STATE],
    );
    if (deleted.changes !== 1)
      throw new Error("ECORRUPT: terminal branch metadata deletion raced");
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        permanent_identifiers: -1,
        charged_metadata_bytes: -(CHARGED_ROW_BYTES + row.cursor.byteLength),
      },
      "terminal branch generation metadata pruning",
    );
  }
  operationResult(operationId: string, maxBytes: number): BranchResultRow | undefined {
    validateOperationIdentifier(operationId);
    return this.#tx.all<BranchResultRow>(
      "SELECT i.branch_id,i.generation,i.reservation_nonce,coalesce(r.outcome,-1) outcome,CASE WHEN length(r.encoded)=0 THEN NULL ELSE r.encoded END encoded,r.expires_at_ms FROM efs_operation_ids i LEFT JOIN efs_operation_results r ON r.operation_id=i.id WHERE i.id=?",
      [operationId],
      { maxRows: 1, maxBytes },
    )[0];
  }
  reserveOperation(
    operationId: string,
    branchId: string,
    generation: number,
    now: number,
    reservationExpiresAt: number,
    reservationNonce: Uint8Array,
    requestBinding: Uint8Array,
  ): void {
    validateOperationIdentifier(operationId);
    validateBranchIdentifier(branchId);
    if (reservationNonce.byteLength !== 16)
      throw new RangeError("invalid operation reservation nonce");
    requestBinding = intrinsicByteRange(requestBinding);
    if (requestBinding.byteLength === 0 || requestBinding.byteLength > 1024)
      throw new RangeError("invalid operation request binding");
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        permanent_identifiers: 1,
        result_bytes: requestBinding.byteLength,
        charged_metadata_bytes: 2 * CHARGED_ROW_BYTES,
      },
      "operation identifier",
    );
    this.#tx.run(
      "INSERT INTO efs_operation_ids(id,branch_id,generation,created_at_ms,reservation_nonce) VALUES(?,?,?,?,?)",
      [operationId, branchId, generation, now, reservationNonce],
    );
    this.#tx.run(
      "INSERT INTO efs_operation_results(operation_id,outcome,encoded,expires_at_ms,revision) VALUES(?,?,?,?,NULL)",
      [operationId, -1, requestBinding, reservationExpiresAt],
    );
  }
  reclaimOperation(
    operationId: string,
    branchId: string,
    generation: number,
    now: number,
    reservationExpiresAt: number,
    reservationNonce: Uint8Array,
  ): boolean {
    validateOperationIdentifier(operationId);
    validateBranchIdentifier(branchId);
    if (reservationNonce.byteLength !== 16)
      throw new RangeError("invalid operation reservation nonce");
    const updated = this.#tx.run(
      "UPDATE efs_operation_ids SET reservation_nonce=? WHERE id=? AND branch_id=? AND generation=? AND EXISTS(SELECT 1 FROM efs_operation_results WHERE operation_id=? AND outcome=-1 AND expires_at_ms<=?)",
      [reservationNonce, operationId, branchId, generation, operationId, now],
    );
    if (updated.changes !== 1) return false;
    this.#tx.run(
      "UPDATE efs_operation_results SET outcome=-1,expires_at_ms=?,revision=NULL WHERE operation_id=? AND outcome=-1",
      [reservationExpiresAt, operationId],
    );
    return true;
  }
  expireOperation(
    operationId: string,
    reservationNonce: Uint8Array,
    now: number,
  ): void {
    validateOperationIdentifier(operationId);
    if (reservationNonce.byteLength !== 16)
      throw new RangeError("invalid operation reservation nonce");
    const row = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(encoded) bytes FROM efs_operation_results WHERE operation_id=? AND outcome=-1 AND EXISTS(SELECT 1 FROM efs_operation_ids i WHERE i.id=? AND i.reservation_nonce=?)",
      [operationId, operationId, reservationNonce],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    if (!row) return;
    const updated = this.#tx.run(
      "UPDATE efs_operation_results SET outcome=2,encoded=X'',expires_at_ms=?,revision=NULL WHERE operation_id=? AND outcome=-1 AND EXISTS(SELECT 1 FROM efs_operation_ids i WHERE i.id=? AND i.reservation_nonce=?)",
      [now, operationId, operationId, reservationNonce],
    );
    if (updated.changes === 1 && row.bytes !== 0)
      new UsageRepository(this.#tx, this.#limits).apply(
        { result_bytes: -row.bytes },
        "expired operation reservation cleanup",
      );
  }
  putChange(
    branchId: string,
    path: Uint8Array,
    expectedToken: number | null,
    kind: number,
    encoded: Uint8Array | null,
  ): void {
    validateDurableIdentifier(branchId, "branch identifier");
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(path)+coalesce(length(encoded),0) bytes FROM efs_branch_changes WHERE branch_id=? AND path=?",
      [branchId, path],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    const variableBytes =
      intrinsicByteLength(path) + (encoded ? intrinsicByteLength(encoded) : 0);
    this.#changeMetadata(
      (prior ? 0 : CHARGED_ROW_BYTES) + variableBytes - (prior?.bytes ?? 0),
      "branch change metadata",
    );
    this.#tx.run(
      "INSERT INTO efs_branch_changes(branch_id,path,expected_token,kind,encoded) VALUES(?,?,?,?,?) ON CONFLICT(branch_id,path) DO UPDATE SET kind=excluded.kind,encoded=excluded.encoded",
      [branchId, path, expectedToken, kind, encoded],
    );
  }
  putInodeExpectation(
    branchId: string,
    inodeId: string,
    expectedToken: number | null,
  ): void {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    const exists =
      this.#tx.all(
        "SELECT 1 present FROM efs_branch_inode_expectations WHERE branch_id=? AND inode_id=?",
        [branchId, inodeId],
        { maxRows: 1, maxBytes: 128 },
      ).length !== 0;
    if (!exists) this.#changeMetadata(CHARGED_ROW_BYTES, "branch inode expectation");
    this.#tx.run(
      "INSERT INTO efs_branch_inode_expectations(branch_id,inode_id,expected_token) VALUES(?,?,?) ON CONFLICT(branch_id,inode_id) DO NOTHING",
      [branchId, inodeId, expectedToken],
    );
  }
  setManifestRoot(branchId: string, path: Uint8Array, manifestHash?: Uint8Array): void {
    validateDurableIdentifier(branchId, "branch identifier");
    path = intrinsicByteRange(path);
    if (manifestHash) {
      manifestHash = intrinsicByteRange(manifestHash);
      if (intrinsicByteLength(manifestHash) !== 32)
        throw new RangeError("branch manifest hash must contain 32 bytes");
    }
    const prior = this.#tx.all<
      { manifest_hash: Uint8Array; bytes: number } & SqliteRow
    >(
      "SELECT manifest_hash,length(path) bytes FROM efs_branch_manifest_roots WHERE branch_id=? AND path=?",
      [branchId, path],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (prior && manifestHash && equalBytes(prior.manifest_hash, manifestHash)) return;
    if (!prior && !manifestHash) return;
    const nextCharge = manifestHash ? CHARGED_ROW_BYTES + intrinsicByteLength(path) : 0;
    this.#changeMetadata(
      nextCharge - (prior ? CHARGED_ROW_BYTES + prior.bytes : 0),
      "branch manifest root metadata",
    );
    this.#bumpRoot(1, branchId, prior !== undefined);
    this.#tx.run("DELETE FROM efs_branch_manifest_roots WHERE branch_id=? AND path=?", [
      branchId,
      path,
    ]);
    if (manifestHash)
      this.#tx.run(
        "INSERT INTO efs_branch_manifest_roots(branch_id,path,manifest_hash) VALUES(?,?,?)",
        [branchId, path, manifestHash],
      );
  }
  changeCount(branchId: string): number {
    return (
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_branch_changes WHERE branch_id=?",
        [branchId],
        { maxRows: 1, maxBytes: 1024 },
      )[0]?.count ?? 0
    );
  }
  changeBytes(branchId: string): number {
    return (
      this.#tx.all<{ bytes: number } & SqliteRow>(
        "SELECT coalesce(sum(length(path)+coalesce(length(encoded),0)),0) bytes FROM efs_branch_changes WHERE branch_id=?",
        [branchId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.bytes ?? 0
    );
  }
  changePathBytes(branchId: string): number {
    return (
      this.#tx.all<{ bytes: number } & SqliteRow>(
        "SELECT coalesce(sum(length(path)),0) bytes FROM efs_branch_changes WHERE branch_id=?",
        [branchId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.bytes ?? 0
    );
  }
  subtreeChanged(inodeId: string, baseRevision: number): boolean {
    validateDurableIdentifier(inodeId, "inode identifier");
    const token =
      this.#tx.all<{ token: number } & SqliteRow>(
        "SELECT token FROM efs_subtree_tokens WHERE inode_id=?",
        [inodeId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.token ??
      this.#tx.all<{ token: number } & SqliteRow>(
        "SELECT coalesce((SELECT t.token FROM efs_subtree_tokens t JOIN efs_meta m ON m.root_inode=t.inode_id WHERE m.singleton=1),(SELECT main_revision FROM efs_meta WHERE singleton=1)) token",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.token;
    if (!Number.isSafeInteger(token))
      throw new Error("ECORRUPT: invalid durable subtree token");
    return token! > baseRevision;
  }
  putInodeOverlay(
    branchId: string,
    inodeId: string,
    expectedToken: number | null,
    encoded: Uint8Array,
  ): void {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    const prior = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(encoded) bytes FROM efs_branch_inode_overlays WHERE branch_id=? AND inode_id=?",
      [branchId, inodeId],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    this.#changeMetadata(
      CHARGED_ROW_BYTES +
        intrinsicByteLength(encoded) -
        (prior ? CHARGED_ROW_BYTES + prior.bytes : 0),
      "branch inode overlay",
    );
    this.#tx.run(
      "INSERT INTO efs_branch_inode_overlays(branch_id,inode_id,expected_token,encoded) VALUES(?,?,?,?) ON CONFLICT(branch_id,inode_id) DO UPDATE SET expected_token=excluded.expected_token,encoded=excluded.encoded",
      [branchId, inodeId, expectedToken, encoded],
    );
  }
  incrementGeneration(branchId: string): void {
    const updated = this.#tx.run(
      "UPDATE efs_branches SET generation=generation+1 WHERE id=? AND state=0",
      [branchId],
    );
    if (updated.changes) this.#bumpRoot(1, branchId, true);
  }
  finish(
    branchId: string,
    state: 1 | 2,
    now: number,
    mergedRevision: number | null = null,
  ): void {
    if (state === 1 && (!Number.isSafeInteger(mergedRevision) || mergedRevision! < 0))
      throw new RangeError("merged branches require a durable revision");
    if (state === 2 && mergedRevision !== null)
      throw new RangeError("discarded branches cannot have a merged revision");
    this.#tx.run(
      "UPDATE efs_branches SET state=?,terminal_at_ms=?,merged_revision=? WHERE id=? AND state=0",
      [state, now, state === 1 ? mergedRevision : null, branchId],
    );
    this.#bumpRoot(1, branchId, true);
    // Terminal namespace rows are no longer mutable.  COW versions and
    // structural patches remain only when a durable stream lease needs them;
    // the deletes below are single bounded statements and therefore do not
    // scale the final publication transaction with the branch write set.
    this.clearChanges(branchId);
    this.clearOverlayPayload(branchId);
    flushUsageMutationBatch(this.#tx, this.#limits);
  }
  terminalCleanupRows(branchId: string): number {
    const count = this.#tx.all<{ rows: number } & SqliteRow>(
      "SELECT (SELECT count(*) FROM efs_branch_changes WHERE branch_id=?)+(SELECT count(*) FROM efs_branch_inode_expectations WHERE branch_id=?)+(SELECT count(*) FROM efs_branch_manifest_roots WHERE branch_id=?)+(SELECT count(*) FROM efs_branch_inode_overlays WHERE branch_id=?)+(SELECT count(*) FROM efs_cow_page_heads WHERE branch_id=?)+(SELECT count(*) FROM efs_cow_page_versions WHERE branch_id=?)+(SELECT count(*) FROM efs_patches WHERE branch_id=?)+(SELECT count(*) FROM efs_patch_segments WHERE branch_id=?) rows",
      [branchId, branchId, branchId, branchId, branchId, branchId, branchId, branchId],
      { maxRows: 1, maxBytes: 4096 },
    )[0]?.rows;
    if (count === undefined || !Number.isSafeInteger(count) || count < 0)
      throw new Error("ECORRUPT: invalid terminal branch cleanup row count");
    return count as number;
  }
  clearChanges(branchId: string): void {
    const rows = this.#tx.all<
      {
        changes: number;
        change_bytes: number;
        expectations: number;
        roots: number;
        root_bytes: number;
      } & SqliteRow
    >(
      "SELECT (SELECT count(*) FROM efs_branch_changes WHERE branch_id=?) changes,(SELECT coalesce(sum(length(path)+coalesce(length(encoded),0)),0) FROM efs_branch_changes WHERE branch_id=?) change_bytes,(SELECT count(*) FROM efs_branch_inode_expectations WHERE branch_id=?) expectations,(SELECT count(*) FROM efs_branch_manifest_roots WHERE branch_id=?) roots,(SELECT coalesce(sum(length(path)),0) FROM efs_branch_manifest_roots WHERE branch_id=?) root_bytes",
      [branchId, branchId, branchId, branchId, branchId],
      { maxRows: 1, maxBytes: 256 },
    )[0]!;
    this.#changeMetadata(
      -(
        (rows.changes + rows.expectations + rows.roots) * CHARGED_ROW_BYTES +
        rows.change_bytes +
        rows.root_bytes
      ),
      "terminal branch metadata cleanup",
    );
    if (rows.roots) this.#bumpRoot(1, branchId);
    this.#tx.run("DELETE FROM efs_branch_changes WHERE branch_id=?", [branchId]);
    this.#tx.run("DELETE FROM efs_branch_inode_expectations WHERE branch_id=?", [
      branchId,
    ]);
  }
  replaceReplicatedPayload(branchId: string): void {
    this.clearChanges(branchId);
    this.clearOverlayPayload(branchId);
    flushUsageMutationBatch(this.#tx, this.#limits);
  }
  setReplicatedGeneration(branchId: string, generation: number): void {
    if (!Number.isSafeInteger(generation) || generation < 0)
      throw new RangeError("invalid replicated branch generation");
    const updated = this.#tx.run(
      "UPDATE efs_branches SET generation=?,state=0,terminal_at_ms=NULL,merged_revision=NULL WHERE id=? AND state=0",
      [generation, branchId],
    );
    if (updated.changes !== 1)
      throw new Error("ECORRUPT: replicated branch generation update missed the active branch");
    this.#bumpRoot(1, branchId, true);
  }
  private clearOverlayPayload(branchId: string): void {
    const overlayCounts = (): {
      pages: number;
      page_bytes: number;
      heads: number;
      patches: number;
      patch_bytes: number;
      segments: number;
      inode_overlays: number;
      inode_overlay_bytes: number;
    } => {
      const rows = this.#tx.all<
        {
          pages: number;
          page_bytes: number;
          heads: number;
          patches: number;
          patch_bytes: number;
          segments: number;
          inode_overlays: number;
          inode_overlay_bytes: number;
        } & SqliteRow
      >(
        "SELECT (SELECT count(*) FROM efs_cow_page_versions WHERE branch_id=?) pages,(SELECT coalesce(sum(length(bytes)),0) FROM efs_cow_page_versions WHERE branch_id=?) page_bytes,(SELECT count(*) FROM efs_cow_page_heads WHERE branch_id=?) heads,(SELECT count(*) FROM efs_patches WHERE branch_id=?) patches,(SELECT coalesce(sum(length(bytes)),0) FROM efs_patch_segments WHERE branch_id=?) patch_bytes,(SELECT count(*) FROM efs_patch_segments WHERE branch_id=?) segments,(SELECT count(*) FROM efs_branch_inode_overlays WHERE branch_id=?) inode_overlays,(SELECT coalesce(sum(length(encoded)),0) FROM efs_branch_inode_overlays WHERE branch_id=?) inode_overlay_bytes",
        [
          branchId,
          branchId,
          branchId,
          branchId,
          branchId,
          branchId,
          branchId,
          branchId,
        ],
        { maxRows: 1, maxBytes: 256 },
      )[0]!;
      for (const [name, value] of Object.entries(rows))
        if (!Number.isSafeInteger(value))
          throw new Error(`ECORRUPT: invalid branch overlay count ${name}`);
      return rows;
    };
    const before = overlayCounts();
    this.#tx.run("DELETE FROM efs_branch_inode_overlays WHERE branch_id=?", [branchId]);
    // Heads detach first so unpinned versions can be reclaimed. Versions and
    // patches pinned by an open snapshot stream survive until lease cleanup.
    this.#tx.run("DELETE FROM efs_cow_page_heads WHERE branch_id=?", [branchId]);
    this.#tx.run(
      "DELETE FROM efs_cow_page_versions WHERE branch_id=? AND NOT EXISTS(SELECT 1 FROM efs_lease_cow_pages p JOIN efs_leases l ON l.id=p.lease_id AND l.state IN (1,2) WHERE p.branch_id=efs_cow_page_versions.branch_id AND p.inode_id=efs_cow_page_versions.inode_id AND p.page_index=efs_cow_page_versions.page_index AND p.generation=efs_cow_page_versions.generation) AND NOT EXISTS(SELECT 1 FROM efs_leases l WHERE l.kind=0 AND l.branch_id=efs_cow_page_versions.branch_id AND l.generation>=efs_cow_page_versions.generation AND l.state IN (1,2))",
      [branchId],
    );
    this.#tx.run(
      "DELETE FROM efs_patches WHERE branch_id=? AND NOT EXISTS(SELECT 1 FROM efs_lease_patches p JOIN efs_leases l ON l.id=p.lease_id AND l.state IN (1,2) WHERE p.branch_id=efs_patches.branch_id AND p.inode_id=efs_patches.inode_id AND p.sequence=efs_patches.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases l WHERE l.kind=0 AND l.branch_id=efs_patches.branch_id AND l.generation>=efs_patches.generation AND l.state IN (1,2))",
      [branchId],
    );
    const after = overlayCounts();
    const pageDelta = after.pages - before.pages;
    const pageByteDelta = after.page_bytes - before.page_bytes;
    const patchDelta = after.patches - before.patches;
    const patchByteDelta = after.patch_bytes - before.patch_bytes;
    const chargedDelta =
      (after.pages -
        before.pages +
        after.heads -
        before.heads +
        after.patches -
        before.patches +
        after.segments -
        before.segments +
        after.inode_overlays -
        before.inode_overlays) *
        CHARGED_ROW_BYTES +
      (after.inode_overlay_bytes - before.inode_overlay_bytes);
    if (
      pageDelta !== 0 ||
      pageByteDelta !== 0 ||
      patchDelta !== 0 ||
      patchByteDelta !== 0 ||
      chargedDelta !== 0
    )
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          page_count: pageDelta,
          page_bytes: pageByteDelta,
          patch_count: patchDelta,
          patch_bytes: patchByteDelta,
          charged_metadata_bytes: chargedDelta,
        },
        "terminal branch payload cleanup",
      );
  }
  storeResult(
    operationId: string,
    outcome: number,
    encoded: Uint8Array,
    expiresAt: number,
    revision: number | null,
  ): void {
    validateDurableIdentifier(operationId, "operation identifier");
    const prior = this.#tx.all<{ bytes: number; outcome: number } & SqliteRow>(
      "SELECT length(encoded) bytes,outcome FROM efs_operation_results WHERE operation_id=?",
      [operationId],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (prior && prior.outcome !== -1)
      throw new Error("ECORRUPT: completed operation tombstone is immutable");
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        result_bytes: intrinsicByteLength(encoded) - (prior?.bytes ?? 0),
        charged_metadata_bytes: prior ? 0 : CHARGED_ROW_BYTES,
      },
      "operation result",
    );
    if (prior)
      this.#tx.run(
        "UPDATE efs_operation_results SET outcome=?,encoded=?,expires_at_ms=?,revision=? WHERE operation_id=? AND outcome=-1",
        [outcome, encoded, expiresAt, revision, operationId],
      );
    else
      this.#tx.run(
        "INSERT INTO efs_operation_results(operation_id,outcome,encoded,expires_at_ms,revision) VALUES(?,?,?,?,?)",
        [operationId, outcome, encoded, expiresAt, revision],
      );
  }
  releaseOperation(operationId: string, reservationNonce?: Uint8Array): void {
    validateDurableIdentifier(operationId, "operation identifier");
    if (reservationNonce && reservationNonce.byteLength !== 16)
      throw new RangeError("invalid operation reservation nonce");
    const row = this.#tx.all<{ bytes: number } & SqliteRow>(
      "SELECT length(encoded) bytes FROM efs_operation_results WHERE operation_id=?",
      [operationId],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!row) return;
    const deletedResult = this.#tx.run(
      `DELETE FROM efs_operation_results WHERE operation_id=? AND outcome=-1${reservationNonce ? " AND EXISTS(SELECT 1 FROM efs_operation_ids i WHERE i.id=? AND i.reservation_nonce=?)" : ""}`,
      reservationNonce ? [operationId, operationId, reservationNonce] : [operationId],
    );
    if (!deletedResult.changes) return;
    const deleted = this.#tx.run(
      "DELETE FROM efs_operation_ids WHERE id=? AND NOT EXISTS(SELECT 1 FROM efs_operation_results WHERE operation_id=?)",
      [operationId, operationId],
    );
    if (deleted.changes)
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          permanent_identifiers: -1,
          result_bytes: -row.bytes,
          charged_metadata_bytes: -2 * CHARGED_ROW_BYTES,
        },
        "operation reservation release",
      );
  }
  pruneExpiredResults(now: number, limit: number): number {
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid result expiration time");
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid expired-result batch limit");
    const rows = this.#tx.all<{ operation_id: string; bytes: number } & SqliteRow>(
      "SELECT operation_id,length(encoded) bytes FROM efs_operation_results WHERE expires_at_ms<=? AND length(encoded)>0 ORDER BY operation_id LIMIT ?",
      [now, limit],
      { maxRows: limit, maxBytes: Math.min(16 * 1024 * 1024, limit * 512) },
    );
    if (!rows.length) return 0;
    let resultBytes = 0;
    for (const row of rows) {
      if (!Number.isSafeInteger(row.bytes) || row.bytes < 0)
        throw new Error("ECORRUPT: invalid operation result size");
      resultBytes += row.bytes;
    }
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        result_bytes: -resultBytes,
      },
      "expired operation result cleanup",
    );
    for (const row of rows)
      this.#tx.run(
        "UPDATE efs_operation_results SET encoded=X'' WHERE operation_id=? AND length(encoded)>0",
        [row.operation_id],
      );
    this.#bumpRoot(3, `expired-results:${now}`, true);
    return rows.length;
  }
  pruneTerminalBranches(now: number, retentionMs: number, limit: number): number {
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid terminal branch cleanup time");
    if (!Number.isSafeInteger(retentionMs) || retentionMs <= 0)
      throw new RangeError("invalid terminal branch retention");
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid terminal branch cleanup batch limit");
    const branchLimit = Math.max(
      1,
      Math.min(
        limit,
        Math.floor(Math.max(1, this.#limits.maxFinalTransactionRows - 8) / 4),
      ),
    );
    const cutoff = now - retentionMs;
    const rows = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT b.id FROM efs_branches b WHERE b.state<>0 AND b.terminal_at_ms IS NOT NULL AND b.terminal_at_ms<=? AND NOT EXISTS(SELECT 1 FROM efs_operation_ids i JOIN efs_operation_results r ON r.operation_id=i.id WHERE i.branch_id=b.id AND length(r.encoded)>0 AND r.expires_at_ms>?) AND NOT EXISTS(SELECT 1 FROM efs_leases l WHERE l.branch_id=b.id OR (l.branch_id IS NULL AND l.owner_id LIKE 'branch-stream:%')) ORDER BY b.terminal_at_ms,b.id LIMIT ?",
      [cutoff, now, branchLimit],
      { maxRows: branchLimit, maxBytes: Math.max(256, branchLimit * 128) },
    );
    for (const row of rows) {
      const cleaned = this.#cleanupTerminalBranch(row.id, limit);
      if (cleaned) return 1;
      beginUsageMutationBatch(this.#tx, this.#limits);
      this.#deleteTerminalGenerationDigest(row.id);
      this.#tx.run("DELETE FROM efs_branches WHERE id=? AND state<>0", [row.id]);
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: -CHARGED_ROW_BYTES },
        "terminal branch metadata pruning",
      );
      flushUsageMutationBatch(this.#tx, this.#limits);
    }
    return rows.length;
  }

  #cleanupTerminalBranch(branchId: string, limit: number): number {
    const batch = Math.max(
      1,
      Math.min(
        limit,
        Math.floor(Math.max(1, this.#limits.maxFinalTransactionRows - 24) / 6),
      ),
    );
    const roots = this.#tx.all<{ path: Uint8Array } & SqliteRow>(
      "SELECT path FROM efs_branch_manifest_roots WHERE branch_id=? ORDER BY path LIMIT ?",
      [branchId, batch],
      { maxRows: batch, maxBytes: Math.max(256, batch * 128) },
    );
    if (roots.length) {
      for (const row of roots)
        this.#tx.run(
          "DELETE FROM efs_branch_manifest_roots WHERE branch_id=? AND path=?",
          [branchId, row.path],
        );
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: -roots.reduce(
            (total, row) => total + CHARGED_ROW_BYTES + intrinsicByteLength(row.path),
            0,
          ),
        },
        "terminal branch root cleanup",
      );
      this.#bumpRoot(1, branchId);
      return roots.length;
    }
    const overlays = this.#tx.all<{ inode_id: string; bytes: number } & SqliteRow>(
      "SELECT inode_id,coalesce(length(encoded),0) bytes FROM efs_branch_inode_overlays WHERE branch_id=? ORDER BY inode_id LIMIT ?",
      [branchId, batch],
      { maxRows: batch, maxBytes: Math.max(256, batch * 256) },
    );
    if (overlays.length) {
      for (const row of overlays)
        this.#tx.run(
          "DELETE FROM efs_branch_inode_overlays WHERE branch_id=? AND inode_id=?",
          [branchId, row.inode_id],
        );
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: -overlays.reduce(
            (total, row) => total + CHARGED_ROW_BYTES + row.bytes,
            0,
          ),
        },
        "terminal branch inode overlay cleanup",
      );
      return overlays.length;
    }
    const changes = this.#tx.all<{ path: Uint8Array; bytes: number } & SqliteRow>(
      "SELECT path,coalesce(length(encoded),0) bytes FROM efs_branch_changes WHERE branch_id=? ORDER BY path LIMIT ?",
      [branchId, batch],
      { maxRows: batch, maxBytes: Math.max(256, batch * 512) },
    );
    if (changes.length) {
      for (const row of changes)
        this.#tx.run("DELETE FROM efs_branch_changes WHERE branch_id=? AND path=?", [
          branchId,
          row.path,
        ]);
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: -changes.reduce(
            (total, row) =>
              total + CHARGED_ROW_BYTES + intrinsicByteLength(row.path) + row.bytes,
            0,
          ),
        },
        "terminal branch change cleanup",
      );
      return changes.length;
    }
    const expectations = this.#tx.all<{ inode_id: string } & SqliteRow>(
      "SELECT inode_id FROM efs_branch_inode_expectations WHERE branch_id=? ORDER BY inode_id LIMIT ?",
      [branchId, batch],
      { maxRows: batch, maxBytes: Math.max(256, batch * 128) },
    );
    for (const row of expectations)
      this.#tx.run(
        "DELETE FROM efs_branch_inode_expectations WHERE branch_id=? AND inode_id=?",
        [branchId, row.inode_id],
      );
    if (expectations.length)
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: -expectations.length * CHARGED_ROW_BYTES },
        "terminal branch expectation cleanup",
      );
    return expectations.length;
  }
  maintainRevisionRetention(
    maxRetainedRevisions: number,
    now: number,
    limit: number,
  ): number {
    if (!Number.isSafeInteger(maxRetainedRevisions) || maxRetainedRevisions <= 0)
      throw new RangeError("invalid retained revision limit");
    if (!Number.isSafeInteger(now) || now < 0)
      throw new RangeError("invalid revision retention time");
    if (
      !Number.isSafeInteger(limit) ||
      limit <= 0 ||
      limit > this.#limits.maxQueryBatchSize
    )
      throw new RangeError("invalid revision retention batch limit");

    const head = this.headRevision();
    let target = Math.max(0, head - maxRetainedRevisions + 1);
    const protectedBase = this.#tx.all<{ revision: number } & SqliteRow>(
      "SELECT min(base_revision) revision FROM efs_branches WHERE state=0",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.revision;
    const protectedResult = this.#tx.all<{ revision: number } & SqliteRow>(
      "SELECT min(revision) revision FROM efs_operation_results WHERE revision IS NOT NULL AND expires_at_ms>?",
      [now],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.revision;
    if (Number.isSafeInteger(protectedBase)) target = Math.min(target, protectedBase!);
    if (Number.isSafeInteger(protectedResult))
      target = Math.min(target, protectedResult!);
    if (target <= 0) return 0;

    let checkpoint = this.#tx.all<CheckpointRow>(
      "SELECT target_revision,state,phase,inode_cursor,entry_parent,entry_name_sort,inode_count,entry_count FROM efs_revision_checkpoints WHERE state=0 ORDER BY target_revision DESC LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
    if (!checkpoint) {
      const completed = this.#tx.all<CheckpointRow>(
        "SELECT target_revision,state,phase,inode_cursor,entry_parent,entry_name_sort,inode_count,entry_count FROM efs_revision_checkpoints WHERE state=1 ORDER BY target_revision DESC LIMIT 1",
        [],
        { maxRows: 1, maxBytes: 4096 },
      )[0];
      if (completed && completed.target_revision >= target && completed.phase >= 7)
        return 0;
      if (completed && completed.target_revision >= target) checkpoint = completed;
      else {
        new UsageRepository(this.#tx, this.#limits).apply(
          { charged_metadata_bytes: CHARGED_ROW_BYTES },
          "revision checkpoint metadata",
        );
        this.#tx.run(
          "INSERT INTO efs_revision_checkpoints(target_revision,state,phase,created_at_ms) VALUES(?,0,0,?)",
          [target, now],
        );
        return 1;
      }
    }

    const checkpointTarget = checkpoint.target_revision;
    const sourceCheckpoint = this.#checkpointAt(checkpointTarget - 1);
    if (checkpoint.phase === 0) {
      const cursor = checkpoint.inode_cursor ?? "";
      const rows = this.#checkpointInodes(
        checkpointTarget,
        sourceCheckpoint,
        cursor,
        limit,
      );
      if (!rows.length) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET phase=1,inode_cursor=NULL WHERE target_revision=? AND state=0",
          [checkpointTarget],
        );
        return 1;
      }
      let metadata = 0;
      let rootsAdded = 0;
      for (const row of rows) {
        const encodedBytes = row.encoded ? intrinsicByteLength(row.encoded) : 0;
        const inserted = this.#tx.run(
          "INSERT OR IGNORE INTO efs_checkpoint_inodes(target_revision,inode_id,tombstone,encoded) VALUES(?,?,?,?)",
          [checkpointTarget, row.inode_id, row.tombstone, row.encoded],
        );
        if (inserted.changes !== 1) continue;
        metadata += CHARGED_ROW_BYTES + encodedBytes;
        const manifestHash = this.#checkpointManifestHash(row.encoded);
        if (manifestHash) {
          const root = this.#tx.run(
            "INSERT OR IGNORE INTO efs_checkpoint_manifest_roots(target_revision,inode_id,manifest_hash) VALUES(?,?,?)",
            [checkpointTarget, row.inode_id, manifestHash],
          );
          if (root.changes === 1) {
            metadata += CHARGED_ROW_BYTES;
            rootsAdded += 1;
          }
        }
      }
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: metadata },
        "revision checkpoint inode rows",
      );
      if (rootsAdded) this.#bumpRoot(2, `checkpoint-${checkpointTarget}`, false);
      const last = rows[rows.length - 1]!;
      this.#tx.run(
        "UPDATE efs_revision_checkpoints SET inode_cursor=?,inode_count=inode_count+? WHERE target_revision=? AND state=0",
        [last.inode_id, rows.length, checkpointTarget],
      );
      return rows.length;
    }
    if (checkpoint.phase === 1) {
      const cursorParent = checkpoint.entry_parent ?? "";
      const cursorName = checkpoint.entry_name_sort ?? new Uint8Array();
      const rows = this.#checkpointEntries(
        checkpointTarget,
        sourceCheckpoint,
        cursorParent,
        cursorName,
        limit,
      );
      if (!rows.length) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET state=1,phase=2 WHERE target_revision=? AND state=0",
          [checkpointTarget],
        );
        return 1;
      }
      let metadata = 0;
      for (const row of rows) {
        const inserted = this.#tx.run(
          "INSERT OR IGNORE INTO efs_checkpoint_entries(target_revision,parent_inode,name_sort,tombstone,encoded) VALUES(?,?,?,?,?)",
          [
            checkpointTarget,
            row.parent_inode,
            row.name_sort,
            row.tombstone,
            row.encoded,
          ],
        );
        if (inserted.changes === 1)
          metadata +=
            CHARGED_ROW_BYTES +
            intrinsicByteLength(row.name_sort) +
            (row.encoded ? intrinsicByteLength(row.encoded) : 0);
      }
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: metadata },
        "revision checkpoint entry rows",
      );
      const last = rows[rows.length - 1]!;
      this.#tx.run(
        "UPDATE efs_revision_checkpoints SET entry_parent=?,entry_name_sort=?,entry_count=entry_count+? WHERE target_revision=? AND state=0",
        [last.parent_inode, last.name_sort, rows.length, checkpointTarget],
      );
      return rows.length;
    }
    if (checkpoint.phase === 2) {
      this.#tx.run(
        "UPDATE efs_revision_checkpoints SET phase=3 WHERE target_revision=? AND state=1",
        [checkpointTarget],
      );
      return 1;
    }
    if (checkpoint.phase === 3) {
      const rows = this.#tx.all<
        { revision: number; inode_id: string; bytes: number } & SqliteRow
      >(
        "SELECT revision,inode_id,coalesce(length(encoded),0) bytes FROM efs_inode_revisions WHERE revision<=? ORDER BY revision,inode_id LIMIT ?",
        [checkpointTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 256) },
      );
      if (!rows.length) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET phase=4 WHERE target_revision=? AND state=1",
          [checkpointTarget],
        );
        return 1;
      }
      this.#tx.run(
        "DELETE FROM efs_inode_revisions WHERE (revision,inode_id) IN (SELECT revision,inode_id FROM efs_inode_revisions WHERE revision<=? ORDER BY revision,inode_id LIMIT ?)",
        [checkpointTarget, limit],
      );
      const bytes = rows.reduce((sum, row) => sum + row.bytes, 0);
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: -(rows.length * CHARGED_ROW_BYTES + bytes),
        },
        "retained inode revision cleanup",
      );
      return rows.length;
    }
    if (checkpoint.phase === 4) {
      const rows = this.#tx.all<
        {
          revision: number;
          parent_inode: string;
          name_sort: Uint8Array;
          bytes: number;
        } & SqliteRow
      >(
        "SELECT revision,parent_inode,name_sort,coalesce(length(encoded),0) bytes FROM efs_entry_revisions WHERE revision<=? ORDER BY revision,parent_inode,name_sort LIMIT ?",
        [checkpointTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 512) },
      );
      if (!rows.length) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET phase=5 WHERE target_revision=? AND state=1",
          [checkpointTarget],
        );
        return 1;
      }
      this.#tx.run(
        "DELETE FROM efs_entry_revisions WHERE (revision,parent_inode,name_sort) IN (SELECT revision,parent_inode,name_sort FROM efs_entry_revisions WHERE revision<=? ORDER BY revision,parent_inode,name_sort LIMIT ?)",
        [checkpointTarget, limit],
      );
      const bytes = rows.reduce(
        (sum, row) => sum + intrinsicByteLength(row.name_sort) + row.bytes,
        0,
      );
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          charged_metadata_bytes: -(rows.length * CHARGED_ROW_BYTES + bytes),
        },
        "retained entry revision cleanup",
      );
      return rows.length;
    }
    if (checkpoint.phase === 5) {
      const rows = this.#tx.all<
        {
          revision: number;
          inode_id: string;
          manifest_hash: Uint8Array;
        } & SqliteRow
      >(
        "SELECT revision,inode_id,manifest_hash FROM efs_revision_manifest_roots WHERE revision<=? ORDER BY revision,inode_id,manifest_hash LIMIT ?",
        [checkpointTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 128) },
      );
      if (!rows.length) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET phase=6 WHERE target_revision=? AND state=1",
          [checkpointTarget],
        );
        return 1;
      }
      this.#tx.run(
        "DELETE FROM efs_revision_manifest_roots WHERE (revision,inode_id,manifest_hash) IN (SELECT revision,inode_id,manifest_hash FROM efs_revision_manifest_roots WHERE revision<=? ORDER BY revision,inode_id,manifest_hash LIMIT ?)",
        [checkpointTarget, limit],
      );
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: -rows.length * CHARGED_ROW_BYTES },
        "retained manifest revision cleanup",
      );
      return rows.length;
    }
    if (checkpoint.phase === 6) {
      const old = this.#tx.all<{ target_revision: number } & SqliteRow>(
        "SELECT target_revision FROM efs_revision_checkpoints WHERE state=1 AND target_revision<? ORDER BY target_revision LIMIT 1",
        [checkpointTarget],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.target_revision;
      if (!Number.isSafeInteger(old)) {
        this.#tx.run(
          "UPDATE efs_revision_checkpoints SET phase=7 WHERE target_revision=? AND state=1",
          [checkpointTarget],
        );
        return 1;
      }
      const oldTarget = old!;
      const rootRows = this.#tx.all<
        { inode_id: string; manifest_hash: Uint8Array } & SqliteRow
      >(
        "SELECT inode_id,manifest_hash FROM efs_checkpoint_manifest_roots WHERE target_revision=? ORDER BY inode_id,manifest_hash LIMIT ?",
        [oldTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 64) },
      );
      if (rootRows.length) {
        for (const row of rootRows)
          this.#tx.run(
            "DELETE FROM efs_checkpoint_manifest_roots WHERE target_revision=? AND inode_id=? AND manifest_hash=?",
            [oldTarget, row.inode_id, row.manifest_hash],
          );
        new UsageRepository(this.#tx, this.#limits).apply(
          { charged_metadata_bytes: -rootRows.length * CHARGED_ROW_BYTES },
          "old revision checkpoint root cleanup",
        );
        return rootRows.length;
      }
      const inodeRows = this.#tx.all<{ inode_id: string; bytes: number } & SqliteRow>(
        "SELECT inode_id,coalesce(length(encoded),0) bytes FROM efs_checkpoint_inodes WHERE target_revision=? ORDER BY inode_id LIMIT ?",
        [oldTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 256) },
      );
      if (inodeRows.length) {
        this.#tx.run(
          "DELETE FROM efs_checkpoint_inodes WHERE target_revision=? AND inode_id IN (SELECT inode_id FROM efs_checkpoint_inodes WHERE target_revision=? ORDER BY inode_id LIMIT ?)",
          [oldTarget, oldTarget, limit],
        );
        new UsageRepository(this.#tx, this.#limits).apply(
          {
            charged_metadata_bytes: -(
              inodeRows.length * CHARGED_ROW_BYTES +
              inodeRows.reduce((sum, row) => sum + row.bytes, 0)
            ),
          },
          "old revision checkpoint inode cleanup",
        );
        return inodeRows.length;
      }
      const entryRows = this.#tx.all<
        {
          parent_inode: string;
          name_sort: Uint8Array;
          bytes: number;
        } & SqliteRow
      >(
        "SELECT parent_inode,name_sort,coalesce(length(encoded),0) bytes FROM efs_checkpoint_entries WHERE target_revision=? ORDER BY parent_inode,name_sort LIMIT ?",
        [oldTarget, limit],
        { maxRows: limit, maxBytes: Math.max(1024, limit * 512) },
      );
      if (entryRows.length) {
        this.#tx.run(
          "DELETE FROM efs_checkpoint_entries WHERE target_revision=? AND (parent_inode,name_sort) IN (SELECT parent_inode,name_sort FROM efs_checkpoint_entries WHERE target_revision=? ORDER BY parent_inode,name_sort LIMIT ?)",
          [oldTarget, oldTarget, limit],
        );
        new UsageRepository(this.#tx, this.#limits).apply(
          {
            charged_metadata_bytes: -(
              entryRows.length * CHARGED_ROW_BYTES +
              entryRows.reduce(
                (sum, row) => sum + intrinsicByteLength(row.name_sort) + row.bytes,
                0,
              )
            ),
          },
          "old revision checkpoint entry cleanup",
        );
        return entryRows.length;
      }
      this.#bumpRoot(2, `checkpoint-${oldTarget}`);
      this.#tx.run("DELETE FROM efs_revision_checkpoints WHERE target_revision=?", [
        oldTarget,
      ]);
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: -CHARGED_ROW_BYTES },
        "old revision checkpoint cleanup",
      );
      return 1;
    }
    return 0;
  }
  #checkpointAt(revision: number): number | null {
    if (!Number.isSafeInteger(revision) || revision < 0) return null;
    return (
      this.#tx.all<{ target_revision: number } & SqliteRow>(
        "SELECT target_revision FROM efs_revision_checkpoints WHERE state=1 AND target_revision<=? ORDER BY target_revision DESC LIMIT 1",
        [revision],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.target_revision ?? null
    );
  }
  #checkpointInodes(
    target: number,
    source: number | null,
    cursor: string,
    limit: number,
  ): readonly CheckpointInodeRow[] {
    const sourceSql =
      source === null
        ? "SELECT inode_id,tombstone,encoded,revision FROM efs_inode_revisions WHERE revision<=?"
        : "SELECT inode_id,tombstone,encoded,target_revision revision FROM efs_checkpoint_inodes WHERE target_revision=? UNION ALL SELECT inode_id,tombstone,encoded,revision FROM efs_inode_revisions WHERE revision>? AND revision<=?";
    const sourceBindings = source === null ? [target] : [source, source, target];
    const sql = `SELECT c.inode_id,c.tombstone,c.encoded,c.revision FROM (${sourceSql}) c JOIN (SELECT inode_id,max(revision) revision FROM (${sourceSql}) WHERE inode_id>? GROUP BY inode_id) latest ON latest.inode_id=c.inode_id AND latest.revision=c.revision WHERE c.inode_id>? ORDER BY c.inode_id LIMIT ?`;
    return this.#tx.all<CheckpointInodeRow>(
      sql,
      [...sourceBindings, ...sourceBindings, cursor, cursor, limit],
      {
        maxRows: limit,
        maxBytes: Math.min(16 * 1024 * 1024, Math.max(1024, limit * 4096)),
      },
    );
  }
  #checkpointEntries(
    target: number,
    source: number | null,
    cursorParent: string,
    cursorName: Uint8Array,
    limit: number,
  ): readonly CheckpointEntryRow[] {
    const sourceSql =
      source === null
        ? "SELECT parent_inode,name_sort,tombstone,encoded,revision FROM efs_entry_revisions WHERE revision<=?"
        : "SELECT parent_inode,name_sort,tombstone,encoded,target_revision revision FROM efs_checkpoint_entries WHERE target_revision=? UNION ALL SELECT parent_inode,name_sort,tombstone,encoded,revision FROM efs_entry_revisions WHERE revision>? AND revision<=?";
    const sourceBindings = source === null ? [target] : [source, source, target];
    const cursor = "(parent_inode>? OR (parent_inode=? AND name_sort>?))";
    const outerCursor = "(c.parent_inode>? OR (c.parent_inode=? AND c.name_sort>?))";
    const sql = `SELECT c.parent_inode,c.name_sort,c.tombstone,c.encoded,c.revision FROM (${sourceSql}) c JOIN (SELECT parent_inode,name_sort,max(revision) revision FROM (${sourceSql}) WHERE ${cursor} GROUP BY parent_inode,name_sort) latest ON latest.parent_inode=c.parent_inode AND latest.name_sort=c.name_sort AND latest.revision=c.revision WHERE ${outerCursor} ORDER BY c.parent_inode,c.name_sort LIMIT ?`;
    return this.#tx.all<CheckpointEntryRow>(
      sql,
      [
        ...sourceBindings,
        ...sourceBindings,
        cursorParent,
        cursorParent,
        cursorName,
        cursorParent,
        cursorParent,
        cursorName,
        limit,
      ],
      {
        maxRows: limit,
        maxBytes: Math.min(16 * 1024 * 1024, Math.max(1024, limit * 4096)),
      },
    );
  }
  #checkpointManifestHash(encoded: Uint8Array | null): Uint8Array | null {
    if (!encoded) return null;
    try {
      const value = JSON.parse(checkpointDecoder.decode(encoded)) as {
        manifest_hash?: unknown;
      };
      if (
        typeof value.manifest_hash !== "string" ||
        !/^[0-9a-f]{64}$/u.test(value.manifest_hash)
      )
        return null;
      return hexToBytes(value.manifest_hash);
    } catch {
      return null;
    }
  }
  #changeMetadata(bytes: number, reason: string): void {
    if (!bytes) return;
    new UsageRepository(this.#tx, this.#limits).apply(
      { charged_metadata_bytes: bytes },
      reason,
    );
  }
  #bumpRoot(kind: number, id: string, mayRemoveRoots = true): void {
    validateDurableIdentifier(id, "root journal identifier");
    const rootId = encodeUtf8(id);
    const prior = this.#tx.all<{ generation: number } & SqliteRow>(
      "SELECT generation FROM efs_root_journal WHERE kind=? AND root_id=? ORDER BY generation DESC LIMIT 1",
      [kind, rootId],
      { maxRows: 1, maxBytes: intrinsicByteLength(rootId) + 128 },
    )[0];
    if (!prior)
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: CHARGED_ROW_BYTES + intrinsicByteLength(rootId) },
        "branch root journal",
        { preserveMaintenanceBytes: MAINTENANCE_TOTAL_EMERGENCY_BYTES },
      );
    const generation = advanceRootMutationGeneration(this.#tx, mayRemoveRoots);
    if (prior)
      this.#tx.run("DELETE FROM efs_root_journal WHERE generation=?", [
        prior.generation,
      ]);
    this.#tx.run(
      "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
      [generation, kind, rootId],
    );
  }
}

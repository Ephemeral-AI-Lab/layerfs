import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import {
  MAX_MAINTENANCE_MARK_ROW_BYTES,
  type StorageLimits,
} from "../resources/limits.js";
import {
  CHARGED_ROW_BYTES,
  GC_MARK_RESERVATION_BYTES,
  USAGE_COUNTER_COLUMNS,
  USAGE_RECOUNT_PHASE_COUNT,
  UsageRepository,
} from "./usage-repository.js";
import { utf8ByteLength } from "../namespace/utf8.js";

const MAX_GC_RUN_ID_BYTES = 256;
const GC_RUN_BASE_BYTES = 512;
const GC_MARK_BASE_BYTES = 192;

function runIdBytes(runId: string): number {
  if (typeof runId !== "string" || runId.length === 0 || runId.includes("\0"))
    throw new RangeError("invalid garbage-collection run id");
  const bytes = utf8ByteLength(runId);
  if (bytes > MAX_GC_RUN_ID_BYTES)
    throw new RangeError("garbage-collection run id exceeds byte limit");
  return bytes;
}
function runCharge(runId: string): number {
  return GC_RUN_BASE_BYTES + runIdBytes(runId) * 2;
}

export interface GcRunRow extends SqliteRow {
  id: string;
  state: number;
  high_water: number;
  root_generation: number;
  examined_roots: number;
  deleted_roots: number;
  examined_nodes: number;
  deleted_nodes: number;
  examined_objects: number;
  deleted_objects: number;
  reclaimed_object_bytes: number;
  reclaimed_manifest_bytes: number;
}
export interface GcMarkRow extends SqliteRow {
  kind: number;
  hash: Uint8Array;
  edge_cursor: number;
}
export interface PayloadRow extends SqliteRow {
  hash: Uint8Array;
  size: number;
  allocation_sequence: number;
  metadata_rows?: number;
}
export interface SnapshotRow extends SqliteRow {
  object_count: number;
  object_bytes: number;
  manifest_root_count: number;
  manifest_root_bytes: number;
  manifest_node_count: number;
  manifest_node_bytes: number;
  page_bytes: number;
  patch_bytes: number;
  charged_metadata_bytes: number;
  generation: number;
  logical_bytes: number;
  revisions: number;
}
export interface HashRow extends SqliteRow {
  hash: Uint8Array;
  encoded: Uint8Array;
}
export interface InodeVerifyRow extends SqliteRow {
  id: string;
  type: number;
  size: number | null;
  manifest_hash: Uint8Array | null;
  nlink: number;
  actual_links: number;
}

export class MaintenanceRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  constructor(tx: FilesystemSQLiteTransaction, limits: StorageLimits) {
    this.#tx = tx;
    this.#limits = limits;
  }
  beginRun(runId: string, now: number): void {
    runIdBytes(runId);
    if (this.run(runId)) return;
    const active = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT id FROM efs_gc_runs WHERE state<>7 LIMIT 1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (active) throw new Error("EBUSY: another garbage-collection run is nonterminal");
    const meta = this.#tx.all<
      { next_allocation_sequence: number; root_mutation_generation: number } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: runCharge(runId) },
      "garbage-collection run",
      { preserveMaintenanceBytes: MAX_MAINTENANCE_MARK_ROW_BYTES },
    );
    this.#tx.run(
      "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES(?,0,?,?,0,NULL,?)",
      [runId, meta.next_allocation_sequence - 1, meta.root_mutation_generation, now],
    );
  }
  abandonRun(runId: string, completeState: number, abandonedState: number): void {
    this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=? AND state<>?", [
      abandonedState,
      runId,
      completeState,
    ]);
  }
  resumeAbandonedRun(
    runId: string,
    abandonedState: number,
    cleanupMarksState: number,
  ): void {
    const changed = this.#tx.run(
      "UPDATE efs_gc_runs SET state=? WHERE id=? AND state=?",
      [cleanupMarksState, runId, abandonedState],
    );
    if (changed.changes !== 1)
      throw new Error("ECORRUPT: abandoned garbage-collection run changed");
  }
  run(id: string): GcRunRow | undefined {
    runIdBytes(id);
    return this.#tx.all<GcRunRow>(
      "SELECT id,state,high_water,root_generation,examined_roots,deleted_roots,examined_nodes,deleted_nodes,examined_objects,deleted_objects,reclaimed_object_bytes,reclaimed_manifest_bytes FROM efs_gc_runs WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  snapshot(): SnapshotRow | undefined {
    return this.#tx.all<SnapshotRow>(
      "SELECT u.object_count,u.object_bytes,u.manifest_root_count,u.manifest_root_bytes,u.manifest_node_count,u.manifest_node_bytes,u.page_bytes,u.patch_bytes,u.charged_metadata_bytes,m.root_mutation_generation generation,(SELECT coalesce(sum(size),0) FROM efs_inodes WHERE type=0) logical_bytes,(SELECT count(*) FROM efs_revisions) revisions FROM efs_usage u JOIN efs_meta m ON m.singleton=u.singleton",
      [],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  physical(): {
    readonly pageCount: number;
    readonly pageSize: number;
    readonly freePages: number;
  } {
    return {
      pageCount: this.#scalar("SELECT page_count value FROM pragma_page_count"),
      pageSize: this.#scalar("SELECT page_size value FROM pragma_page_size"),
      freePages: this.#scalar("SELECT freelist_count value FROM pragma_freelist_count"),
    };
  }
  generation(): number {
    return this.#scalar(
      "SELECT root_mutation_generation value FROM efs_meta WHERE singleton=1",
    );
  }
  usageVerificationState(): {
    readonly mutationSequence: number;
    readonly counters: readonly number[];
  } {
    const snapshot = new UsageRepository(this.#tx, this.#limits).snapshot();
    return Object.freeze({
      mutationSequence: snapshot.mutation_sequence,
      counters: Object.freeze(USAGE_COUNTER_COLUMNS.map((column) => snapshot[column])),
    });
  }
  usageVerificationPhaseCount(): number {
    return USAGE_RECOUNT_PHASE_COUNT;
  }
  usageVerificationBatch(
    phase: number,
    afterKey: string | null,
    limit: number,
    maxBytes: number,
  ) {
    return new UsageRepository(this.#tx, this.#limits).recountBatch(
      phase,
      afterKey,
      limit,
      maxBytes,
    );
  }
  hashes(
    kind: "roots" | "nodes",
    after: Uint8Array,
    limit: number,
    maxBytes: number,
  ): readonly HashRow[] {
    const table = kind === "roots" ? "efs_manifest_roots" : "efs_manifest_nodes";
    return this.#tx.all<HashRow>(
      `SELECT hash,encoded FROM ${table} WHERE hash>? ORDER BY hash LIMIT ?`,
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  objects(after: Uint8Array, limit: number, maxBytes: number): readonly PayloadRow[] {
    return this.#tx.all<PayloadRow>(
      "SELECT hash,size,allocation_sequence FROM efs_cas_objects WHERE hash>? ORDER BY hash LIMIT ?",
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  inodes(after: string, limit: number, maxBytes: number): readonly InodeVerifyRow[] {
    return this.#tx.all<InodeVerifyRow>(
      "SELECT i.id,i.type,i.size,i.manifest_hash,i.nlink,(SELECT count(*) FROM efs_entries e WHERE e.inode_id=i.id) actual_links FROM efs_inodes i WHERE i.id>? ORDER BY i.id LIMIT ?",
      [after, limit],
      { maxRows: limit, maxBytes },
    );
  }
  pendingMarks(runId: string, limit: number, maxBytes: number): readonly GcMarkRow[] {
    return this.#tx.all<GcMarkRow>(
      "SELECT kind,hash,edge_cursor FROM efs_gc_marks WHERE run_id=? AND processed=0 ORDER BY kind,hash LIMIT ?",
      [runId, limit],
      { maxRows: limit, maxBytes },
    );
  }
  addMark(runId: string, kind: number, hash: Uint8Array): void {
    const result = this.#tx.run(
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) VALUES(?,?,?,0)",
      [runId, kind, hash],
    );
    void result;
  }
  advanceMark(
    runId: string,
    kind: number,
    hash: Uint8Array,
    edgeCursor: number,
    processed: boolean,
  ): void {
    this.#tx.run(
      "UPDATE efs_gc_marks SET edge_cursor=?,processed=? WHERE run_id=? AND kind=? AND hash=?",
      [edgeCursor, processed ? 1 : 0, runId, kind, hash],
    );
  }
  addExamined(runId: string, roots: number, nodes: number, objects: number): void {
    this.#tx.run(
      "UPDATE efs_gc_runs SET examined_roots=examined_roots+?,examined_nodes=examined_nodes+?,examined_objects=examined_objects+? WHERE id=?",
      [roots, nodes, objects, runId],
    );
  }
  seedRootsBatch(runId: string, limit: number, maxBytes: number): boolean {
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid GC root batch limit");
    const run = this.#tx.all<
      {
        cursor_kind: number;
        cursor_value: Uint8Array | null;
        root_generation: number;
      } & SqliteRow
    >(
      "SELECT cursor_kind,cursor_value,root_generation FROM efs_gc_runs WHERE id=? AND state=0",
      [runId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!run) throw new Error("ECORRUPT: missing active garbage-collection run");
    if (!Number.isSafeInteger(run.cursor_kind) || run.cursor_kind < 0)
      throw new Error("ECORRUPT: invalid garbage-collection root cursor");
    if (run.cursor_kind >= 5) return this.#finishRootPass(runId, run.root_generation);
    // A one-byte zero BLOB sorts before every 32-byte digest, while avoiding
    // runtimes that normalize reconstructed empty typed-array views to NULL.
    const after = run.cursor_value ?? Uint8Array.of(0);
    const queries = [
      "SELECT DISTINCT manifest_hash hash FROM efs_inodes WHERE manifest_hash IS NOT NULL AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT manifest_hash hash FROM efs_revision_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT manifest_hash hash FROM efs_branch_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT manifest_hash hash FROM efs_lease_manifests WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
      "SELECT DISTINCT manifest_hash hash FROM efs_lease_staged_manifests WHERE kind=0 AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
    ] as const;
    const rows = this.#tx.all<{ hash: Uint8Array } & SqliteRow>(
      queries[run.cursor_kind]!,
      [after, limit],
      { maxRows: limit, maxBytes },
    );
    for (const row of rows) this.addMark(runId, 0, row.hash);
    if (rows.length === limit) {
      this.#tx.run("UPDATE efs_gc_runs SET cursor_value=? WHERE id=?", [
        rows.at(-1)!.hash,
        runId,
      ]);
      return false;
    }
    const nextKind = run.cursor_kind + 1;
    this.#tx.run("UPDATE efs_gc_runs SET cursor_kind=?,cursor_value=NULL WHERE id=?", [
      nextKind,
      runId,
    ]);
    return nextKind >= 5 ? this.#finishRootPass(runId, run.root_generation) : false;
  }
  sweepCandidates(
    runId: string,
    state: number,
    highWater: number,
    limit: number,
    maxBytes: number,
  ): readonly PayloadRow[] {
    const table =
      state === 1
        ? "efs_manifest_roots"
        : state === 2
          ? "efs_manifest_nodes"
          : "efs_cas_objects";
    const kind = state - 1;
    const size = state === 3 ? "size" : "length(encoded)";
    const metadataRows =
      state === 1
        ? "1+CASE WHEN EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=efs_manifest_roots.hash) THEN 1 ELSE 0 END"
        : "1";
    const unreferenced =
      state === 1
        ? "NOT EXISTS(SELECT 1 FROM efs_inodes i WHERE i.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_revision_manifest_roots r WHERE r.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_branch_manifest_roots b WHERE b.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_lease_manifests l WHERE l.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_lease_staged_manifests m WHERE m.kind=0 AND m.manifest_hash=efs_manifest_roots.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_reused_subtrees s WHERE s.source_manifest_hash=efs_manifest_roots.hash)"
        : state === 2
          ? "NOT EXISTS(SELECT 1 FROM efs_lease_staged_manifests m WHERE m.kind=1 AND m.manifest_hash=efs_manifest_nodes.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_level_records l WHERE l.node_hash=efs_manifest_nodes.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_reused_subtrees s WHERE s.node_hash=efs_manifest_nodes.hash)"
          : "NOT EXISTS(SELECT 1 FROM efs_lease_objects l WHERE l.object_hash=efs_cas_objects.hash) AND NOT EXISTS(SELECT 1 FROM efs_staging_entries s WHERE s.object_hash=efs_cas_objects.hash)";
    return this.#tx.all<PayloadRow>(
      `SELECT hash,${size} size,allocation_sequence,${metadataRows} metadata_rows FROM ${table} WHERE allocation_sequence<=? AND NOT EXISTS(SELECT 1 FROM efs_gc_marks m WHERE m.run_id=? AND m.kind=? AND m.hash=${table}.hash) AND ${unreferenced} ORDER BY allocation_sequence LIMIT ?`,
      [highWater, runId, kind, limit],
      { maxRows: limit, maxBytes },
    );
  }
  reconcileSweepGeneration(runId: string, state: number): boolean {
    const row = this.#tx.all<
      { state: number; root_generation: number; generation: number } & SqliteRow
    >(
      "SELECT r.state,r.root_generation,m.root_mutation_generation generation FROM efs_gc_runs r JOIN efs_meta m ON m.singleton=1 WHERE r.id=?",
      [runId],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    if (!row || row.state !== state)
      throw new Error("ECORRUPT: garbage-collection sweep state changed");
    if (row.root_generation === row.generation) return true;
    this.#tx.run(
      "UPDATE efs_gc_runs SET state=0,root_generation=?,cursor_kind=0,cursor_value=NULL WHERE id=? AND state=?",
      [row.generation, runId, state],
    );
    return false;
  }
  applySweep(
    runId: string,
    state: number,
    rows: readonly PayloadRow[],
    completeState: number,
  ): void {
    const table =
      state === 1
        ? "efs_manifest_roots"
        : state === 2
          ? "efs_manifest_nodes"
          : "efs_cas_objects";
    let bytes = 0;
    let metadataRows = 0;
    for (const row of rows) {
      if (state === 1 && (row.metadata_rows ?? 1) === 2) {
        const validation = this.#tx.run(
          "DELETE FROM efs_manifest_validations WHERE manifest_hash=?",
          [row.hash],
        );
        if (validation.changes !== 1)
          throw new Error("ECORRUPT: manifest validation certificate changed");
      }
      this.#tx.run(`DELETE FROM ${table} WHERE hash=?`, [row.hash]);
      bytes += row.size;
      metadataRows += row.metadata_rows ?? 1;
    }
    if (rows.length) {
      const countColumn =
        state === 1
          ? "manifest_root_count"
          : state === 2
            ? "manifest_node_count"
            : "object_count";
      const byteColumn =
        state === 1
          ? "manifest_root_bytes"
          : state === 2
            ? "manifest_node_bytes"
            : "object_bytes";
      const deletedColumn =
        state === 1
          ? "deleted_roots"
          : state === 2
            ? "deleted_nodes"
            : "deleted_objects";
      const reclaimedColumn =
        state === 3 ? "reclaimed_object_bytes" : "reclaimed_manifest_bytes";
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          [countColumn]: -rows.length,
          [byteColumn]: -bytes,
          charged_metadata_bytes: -metadataRows * CHARGED_ROW_BYTES,
          maintenance_bytes: -rows.length * GC_MARK_RESERVATION_BYTES,
        },
        "garbage-collection sweep",
      );
      this.#tx.run(
        `UPDATE efs_gc_runs SET ${deletedColumn}=${deletedColumn}+?,${reclaimedColumn}=${reclaimedColumn}+? WHERE id=?`,
        [rows.length, bytes, runId],
      );
    } else {
      const next = state === 3 ? completeState : state + 1;
      this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [next, runId]);
    }
  }
  cleanupMarks(runId: string, limit: number, nextState: number): boolean {
    const rows = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? ORDER BY kind,hash LIMIT ?",
      [runId, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 128) },
    );
    for (const row of rows)
      this.#tx.run("DELETE FROM efs_gc_marks WHERE run_id=? AND kind=? AND hash=?", [
        runId,
        row.kind,
        row.hash,
      ]);
    if (!rows.length)
      this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
    return rows.length > 0;
  }
  cleanupRootJournal(runId: string, limit: number, nextState: number): boolean {
    const safeGeneration = this.#tx.all<{ root_generation: number } & SqliteRow>(
      "SELECT root_generation FROM efs_gc_runs WHERE id=?",
      [runId],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.root_generation;
    if (!Number.isSafeInteger(safeGeneration) || safeGeneration! < 0)
      throw new Error("ECORRUPT: invalid garbage-collection root generation");
    const rows = this.#tx.all<{ generation: number; bytes: number } & SqliteRow>(
      "SELECT generation,length(root_id) bytes FROM efs_root_journal WHERE generation<=? ORDER BY generation LIMIT ?",
      [safeGeneration!, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 64) },
    );
    let charged = 0;
    for (const row of rows) {
      this.#tx.run("DELETE FROM efs_root_journal WHERE generation=?", [row.generation]);
      charged += CHARGED_ROW_BYTES + row.bytes;
    }
    if (rows.length)
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: -charged },
        "root journal cleanup",
      );
    else this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
    return rows.length > 0;
  }
  cleanupTerminalRuns(
    runId: string,
    limit: number,
    completeState: number,
    abandonedState: number,
    nextState: number,
  ): boolean {
    const prior = this.#tx.all<{ id: string } & SqliteRow>(
      "SELECT id FROM efs_gc_runs WHERE id<>? AND state IN (?,?) ORDER BY created_at_ms,id LIMIT 1",
      [runId, completeState, abandonedState],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!prior) {
      this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=?", [nextState, runId]);
      return false;
    }
    const marks = this.#tx.all<{ kind: number; hash: Uint8Array } & SqliteRow>(
      "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? ORDER BY kind,hash LIMIT ?",
      [prior.id, limit],
      { maxRows: limit, maxBytes: Math.max(256, limit * 128) },
    );
    for (const mark of marks)
      this.#tx.run("DELETE FROM efs_gc_marks WHERE run_id=? AND kind=? AND hash=?", [
        prior.id,
        mark.kind,
        mark.hash,
      ]);
    if (marks.length) return true;
    const deleted = this.#tx.run("DELETE FROM efs_gc_runs WHERE id=?", [prior.id]);
    if (deleted.changes !== 1)
      throw new Error("ECORRUPT: terminal garbage-collection run changed");
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: -runCharge(prior.id) },
      "terminal garbage-collection run cleanup",
    );
    return true;
  }
  #finishRootPass(runId: string, expectedGeneration: number): boolean {
    const generation = this.generation();
    if (generation !== expectedGeneration) {
      this.#tx.run(
        "UPDATE efs_gc_runs SET root_generation=?,cursor_kind=0,cursor_value=NULL WHERE id=?",
        [generation, runId],
      );
      return false;
    }
    this.#tx.run(
      "UPDATE efs_gc_runs SET state=1,cursor_kind=5,cursor_value=NULL WHERE id=? AND state=0",
      [runId],
    );
    return true;
  }
  #scalar(sql: string): number {
    const value = this.#tx.all<{ value: number } & SqliteRow>(sql, [], {
      maxRows: 1,
      maxBytes: 1024,
    })[0]?.value;
    if (typeof value !== "number") throw new Error("ECORRUPT: invalid scalar");
    return value;
  }
}

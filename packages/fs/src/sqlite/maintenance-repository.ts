import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import type { StorageLimits } from "../resources/limits.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";

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
}
export interface PayloadRow extends SqliteRow {
  hash: Uint8Array;
  size: number;
  allocation_sequence: number;
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
    if (this.run(runId)) return;
    const meta = this.#tx.all<
      { next_allocation_sequence: number; root_mutation_generation: number } & SqliteRow
    >(
      "SELECT next_allocation_sequence,root_mutation_generation FROM efs_meta WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 1024 },
    )[0];
    if (!meta) throw new Error("ECORRUPT: missing metadata");
    new UsageRepository(this.#tx, this.#limits).apply(
      { maintenance_bytes: CHARGED_ROW_BYTES },
      "garbage-collection run",
    );
    this.#tx.run(
      "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES(?,0,?,?,0,NULL,?)",
      [runId, meta.next_allocation_sequence - 1, meta.root_mutation_generation, now],
    );
    this.addRoots(runId);
  }
  abandonRun(runId: string, completeState: number, abandonedState: number): void {
    this.#tx.run("UPDATE efs_gc_runs SET state=? WHERE id=? AND state<>?", [
      abandonedState,
      runId,
      completeState,
    ]);
  }
  run(id: string): GcRunRow | undefined {
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
      "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? AND processed=0 ORDER BY kind,hash LIMIT ?",
      [runId, limit],
      { maxRows: limit, maxBytes },
    );
  }
  addMark(runId: string, kind: number, hash: Uint8Array): void {
    const result = this.#tx.run(
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) VALUES(?,?,?,0)",
      [runId, kind, hash],
    );
    if (result.changes)
      new UsageRepository(this.#tx, this.#limits).apply(
        { maintenance_bytes: CHARGED_ROW_BYTES },
        "garbage-collection mark",
      );
  }
  markProcessed(runId: string, kind: number, hash: Uint8Array): void {
    this.#tx.run(
      "UPDATE efs_gc_marks SET processed=1 WHERE run_id=? AND kind=? AND hash=?",
      [runId, kind, hash],
    );
  }
  addExamined(runId: string, roots: number, nodes: number, objects: number): void {
    this.#tx.run(
      "UPDATE efs_gc_runs SET examined_roots=examined_roots+?,examined_nodes=examined_nodes+?,examined_objects=examined_objects+? WHERE id=?",
      [roots, nodes, objects, runId],
    );
  }
  reconcileRoots(runId: string): void {
    const added = this.addRoots(runId);
    this.#tx.run("UPDATE efs_gc_runs SET root_generation=? WHERE id=?", [
      this.generation(),
      runId,
    ]);
    if (!added) this.#tx.run("UPDATE efs_gc_runs SET state=1 WHERE id=?", [runId]);
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
    return this.#tx.all<PayloadRow>(
      `SELECT hash,${size} size,allocation_sequence FROM ${table} WHERE allocation_sequence<=? AND NOT EXISTS(SELECT 1 FROM efs_gc_marks m WHERE m.run_id=? AND m.kind=? AND m.hash=${table}.hash) ORDER BY allocation_sequence LIMIT ?`,
      [highWater, runId, kind, limit],
      { maxRows: limit, maxBytes },
    );
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
    for (const row of rows) {
      this.#tx.run(`DELETE FROM ${table} WHERE hash=?`, [row.hash]);
      bytes += row.size;
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
          charged_metadata_bytes: -rows.length * CHARGED_ROW_BYTES,
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
      if (next === completeState) {
        const before = Math.max(0, this.generation() - 10_000);
        const journal = this.#tx.all<{ count: number; bytes: number } & SqliteRow>(
          "SELECT count(*) count,coalesce(sum(length(root_id)),0) bytes FROM efs_root_journal WHERE generation<?",
          [before],
          { maxRows: 1, maxBytes: 256 },
        )[0];
        const deleted = this.#tx.run(
          "DELETE FROM efs_root_journal WHERE generation<?",
          [before],
        ).changes;
        if (deleted) {
          if (!journal || journal.count !== deleted)
            throw new Error("ECORRUPT: root journal cleanup count changed");
          new UsageRepository(this.#tx, this.#limits).apply(
            {
              maintenance_bytes: -(deleted * CHARGED_ROW_BYTES + journal.bytes),
            },
            "root journal cleanup",
          );
        }
      }
    }
  }
  addRoots(runId: string): number {
    let changes = 0;
    for (const sql of [
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) SELECT ?,0,manifest_hash,0 FROM efs_inodes WHERE manifest_hash IS NOT NULL",
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) SELECT ?,0,manifest_hash,0 FROM efs_revision_manifest_roots",
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) SELECT ?,0,manifest_hash,0 FROM efs_branch_manifest_roots",
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) SELECT ?,0,lm.manifest_hash,0 FROM efs_lease_manifests lm JOIN efs_leases l ON l.id=lm.lease_id WHERE l.state IN (0,1)",
      "INSERT OR IGNORE INTO efs_gc_marks(run_id,kind,hash,processed) SELECT ?,0,c.manifest_hash,0 FROM efs_staging_certificates c JOIN efs_leases l ON l.id=c.lease_id WHERE c.sealed=1 AND l.state=1",
    ]) {
      const inserted = this.#tx.run(sql, [runId]).changes;
      changes += inserted;
      if (inserted)
        new UsageRepository(this.#tx, this.#limits).apply(
          { maintenance_bytes: inserted * CHARGED_ROW_BYTES },
          "garbage-collection roots",
        );
    }
    return changes;
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

import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";

export interface BranchRow extends SqliteRow {
  id: string;
  base_revision: number;
  state: number;
  generation: number;
  created_at_ms: number;
  terminal_at_ms: number | null;
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
  encoded: Uint8Array | null;
  expires_at_ms: number | null;
}

export class BranchRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  constructor(tx: FilesystemSQLiteTransaction) {
    this.#tx = tx;
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
    return this.#tx.all<BranchHistoryEntryRow>(
      "SELECT r.name_sort,r.tombstone,r.encoded FROM efs_entry_revisions r WHERE r.parent_inode=? AND r.revision=(SELECT max(x.revision) FROM efs_entry_revisions x WHERE x.parent_inode=r.parent_inode AND x.name_sort=r.name_sort AND x.revision<=?)",
      [parentInode, revision],
      { maxRows: 100_001, maxBytes: 16 * 1024 * 1024 },
    );
  }
  historicEntry(
    parentInode: string,
    nameSort: Uint8Array,
    revision: number,
  ): BranchHistoryRow | undefined {
    return this.#tx.all<BranchHistoryRow>(
      "SELECT tombstone,encoded FROM efs_entry_revisions WHERE parent_inode=? AND name_sort=? AND revision<=? ORDER BY revision DESC LIMIT 1",
      [parentInode, nameSort, revision],
      { maxRows: 1, maxBytes: 8192 },
    )[0];
  }
  historicInode(inodeId: string, revision: number): BranchHistoryRow | undefined {
    return this.#tx.all<BranchHistoryRow>(
      "SELECT tombstone,encoded FROM efs_inode_revisions WHERE inode_id=? AND revision<=? ORDER BY revision DESC LIMIT 1",
      [inodeId, revision],
      { maxRows: 1, maxBytes: 8192 },
    )[0];
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
      this.#tx.all("SELECT revision FROM efs_revisions WHERE revision=?", [revision], {
        maxRows: 1,
        maxBytes: 1024,
      }).length === 1
    );
  }
  create(id: string, baseRevision: number, now: number): BranchRow {
    this.#tx.run("INSERT INTO efs_branch_ids(id,created_at_ms) VALUES(?,?)", [id, now]);
    this.#tx.run(
      "INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms) VALUES(?,?,0,0,?,NULL)",
      [id, baseRevision, now],
    );
    return this.row(id)!;
  }
  row(id: string): BranchRow | undefined {
    return this.#tx.all<BranchRow>(
      "SELECT id,base_revision,state,generation,created_at_ms,terminal_at_ms FROM efs_branches WHERE id=?",
      [id],
      { maxRows: 1, maxBytes: 4096 },
    )[0];
  }
  operationResult(operationId: string, maxBytes: number): BranchResultRow | undefined {
    return this.#tx.all<BranchResultRow>(
      "SELECT i.branch_id,i.generation,r.encoded,r.expires_at_ms FROM efs_operation_ids i LEFT JOIN efs_operation_results r ON r.operation_id=i.id WHERE i.id=?",
      [operationId],
      { maxRows: 1, maxBytes },
    )[0];
  }
  reserveOperation(
    operationId: string,
    branchId: string,
    generation: number,
    now: number,
  ): void {
    this.#tx.run(
      "INSERT INTO efs_operation_ids(id,branch_id,generation,created_at_ms) VALUES(?,?,?,?)",
      [operationId, branchId, generation, now],
    );
  }
  putChange(
    branchId: string,
    path: Uint8Array,
    expectedToken: number | null,
    kind: number,
    encoded: Uint8Array | null,
  ): void {
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
    this.#tx.run(
      "INSERT INTO efs_branch_inode_expectations(branch_id,inode_id,expected_token) VALUES(?,?,?) ON CONFLICT(branch_id,inode_id) DO NOTHING",
      [branchId, inodeId, expectedToken],
    );
  }
  setManifestRoot(branchId: string, path: Uint8Array, manifestHash?: Uint8Array): void {
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
  incrementGeneration(branchId: string): void {
    this.#tx.run(
      "UPDATE efs_branches SET generation=generation+1 WHERE id=? AND state=0",
      [branchId],
    );
  }
  finish(branchId: string, state: 1 | 2, now: number): void {
    this.#tx.run("UPDATE efs_branches SET state=?,terminal_at_ms=? WHERE id=?", [
      state,
      now,
      branchId,
    ]);
    this.clearChanges(branchId);
  }
  clearChanges(branchId: string): void {
    this.#tx.run("DELETE FROM efs_branch_changes WHERE branch_id=?", [branchId]);
    this.#tx.run("DELETE FROM efs_branch_inode_expectations WHERE branch_id=?", [
      branchId,
    ]);
  }
  storeResult(
    operationId: string,
    outcome: number,
    encoded: Uint8Array,
    expiresAt: number,
  ): void {
    this.#tx.run(
      "INSERT INTO efs_operation_results(operation_id,outcome,encoded,expires_at_ms) VALUES(?,?,?,?)",
      [operationId, outcome, encoded, expiresAt],
    );
  }
}

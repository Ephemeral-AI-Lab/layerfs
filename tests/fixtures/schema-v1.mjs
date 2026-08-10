import { EFS_APPLICATION_ID } from "../../packages/fs/dist/sqlite/schema.js";

const TABLES = [
  `CREATE TABLE efs_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), schema_version INTEGER NOT NULL, filesystem_id TEXT NOT NULL UNIQUE, main_revision INTEGER NOT NULL, root_inode TEXT NOT NULL, root_mutation_generation INTEGER NOT NULL, next_allocation_sequence INTEGER NOT NULL, cow_page_bytes INTEGER NOT NULL CHECK(cow_page_bytes IN (4096,8192,16384)), created_at_ms INTEGER NOT NULL)`,
  `CREATE TABLE efs_usage (singleton INTEGER PRIMARY KEY CHECK(singleton=1), object_count INTEGER NOT NULL, object_bytes INTEGER NOT NULL, manifest_root_count INTEGER NOT NULL, manifest_root_bytes INTEGER NOT NULL, manifest_node_count INTEGER NOT NULL, manifest_node_bytes INTEGER NOT NULL, page_count INTEGER NOT NULL, page_bytes INTEGER NOT NULL, patch_count INTEGER NOT NULL, patch_bytes INTEGER NOT NULL, staging_bytes INTEGER NOT NULL, result_bytes INTEGER NOT NULL, maintenance_bytes INTEGER NOT NULL, permanent_identifiers INTEGER NOT NULL, charged_metadata_bytes INTEGER NOT NULL)`,
  `CREATE TABLE efs_cas_objects (hash BLOB PRIMARY KEY CHECK(length(hash)=32), size INTEGER NOT NULL CHECK(size>=0 AND size=length(bytes)), bytes BLOB NOT NULL, allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID`,
  `CREATE TABLE efs_manifest_roots (hash BLOB PRIMARY KEY CHECK(length(hash)=32), root_node_hash BLOB NOT NULL CHECK(length(root_node_hash)=32), file_size INTEGER NOT NULL CHECK(file_size>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), chunk_min INTEGER NOT NULL, chunk_avg INTEGER NOT NULL, chunk_max INTEGER NOT NULL, encoded BLOB NOT NULL CHECK(length(encoded)=68), allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID`,
  `CREATE TABLE efs_manifest_nodes (hash BLOB PRIMARY KEY CHECK(length(hash)=32), kind INTEGER NOT NULL CHECK(kind IN (0,1)), logical_bytes INTEGER NOT NULL CHECK(logical_bytes>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), encoded BLOB NOT NULL, allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID`,
  `CREATE TABLE efs_revisions (revision INTEGER PRIMARY KEY, parent_revision INTEGER REFERENCES efs_revisions(revision), created_at_ms INTEGER NOT NULL, writer_id TEXT NOT NULL, change_count INTEGER NOT NULL CHECK(change_count>=0))`,
  `CREATE TABLE efs_inodes (id TEXT PRIMARY KEY, type INTEGER NOT NULL CHECK(type IN (0,1,2)), mode INTEGER NOT NULL, birthtime_ms INTEGER NOT NULL, mtime_ms INTEGER NOT NULL, ctime_ms INTEGER NOT NULL, nlink INTEGER NOT NULL CHECK(nlink>0), size INTEGER, manifest_hash BLOB REFERENCES efs_manifest_roots(hash), symlink_target TEXT, token INTEGER NOT NULL, CHECK((type=0 AND size IS NOT NULL AND manifest_hash IS NOT NULL AND symlink_target IS NULL) OR (type=1 AND size IS NULL AND manifest_hash IS NULL AND symlink_target IS NULL) OR (type=2 AND size IS NULL AND manifest_hash IS NULL AND symlink_target IS NOT NULL)))`,
  `CREATE TABLE efs_entries (parent_inode TEXT NOT NULL REFERENCES efs_inodes(id), name_sort BLOB NOT NULL, name TEXT, inode_id TEXT REFERENCES efs_inodes(id), token INTEGER NOT NULL, PRIMARY KEY(parent_inode,name_sort)) WITHOUT ROWID`,
  `CREATE INDEX efs_entries_inode ON efs_entries(inode_id)`,
  `CREATE TABLE efs_inode_revisions (revision INTEGER NOT NULL REFERENCES efs_revisions(revision), inode_id TEXT NOT NULL, tombstone INTEGER NOT NULL CHECK(tombstone IN (0,1)), encoded BLOB, PRIMARY KEY(revision,inode_id)) WITHOUT ROWID`,
  `CREATE TABLE efs_revision_manifest_roots (revision INTEGER NOT NULL REFERENCES efs_revisions(revision) ON DELETE CASCADE, inode_id TEXT NOT NULL, manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash), PRIMARY KEY(revision,inode_id,manifest_hash)) WITHOUT ROWID`,
  `CREATE TABLE efs_entry_revisions (revision INTEGER NOT NULL REFERENCES efs_revisions(revision), parent_inode TEXT NOT NULL, name_sort BLOB NOT NULL, tombstone INTEGER NOT NULL CHECK(tombstone IN (0,1)), encoded BLOB, PRIMARY KEY(revision,parent_inode,name_sort)) WITHOUT ROWID`,
  `CREATE TABLE efs_branches (id TEXT PRIMARY KEY, base_revision INTEGER NOT NULL REFERENCES efs_revisions(revision), state INTEGER NOT NULL CHECK(state IN (0,1,2)), generation INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, terminal_at_ms INTEGER)`,
  `CREATE TABLE efs_branch_ids (id TEXT PRIMARY KEY, created_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_branch_changes (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, path BLOB NOT NULL, expected_token INTEGER, kind INTEGER NOT NULL, encoded BLOB, PRIMARY KEY(branch_id,path)) WITHOUT ROWID`,
  `CREATE TABLE efs_branch_inode_expectations (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, expected_token INTEGER, PRIMARY KEY(branch_id,inode_id)) WITHOUT ROWID`,
  `CREATE TABLE efs_branch_manifest_roots (branch_id TEXT NOT NULL, path BLOB NOT NULL, manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash), PRIMARY KEY(branch_id,path,manifest_hash), FOREIGN KEY(branch_id,path) REFERENCES efs_branch_changes(branch_id,path) ON DELETE CASCADE) WITHOUT ROWID`,
  `CREATE TABLE efs_cow_pages (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index)) WITHOUT ROWID`,
  `CREATE TABLE efs_patches (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, offset INTEGER NOT NULL, delete_length INTEGER NOT NULL, insert_bytes BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id,sequence)) WITHOUT ROWID`,
  `CREATE TABLE efs_leases (id TEXT PRIMARY KEY, kind INTEGER NOT NULL, owner_id TEXT NOT NULL, expires_at_ms INTEGER NOT NULL, state INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_manifests (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash), PRIMARY KEY(lease_id,manifest_hash)) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_certificates (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, manifest_hash BLOB NOT NULL, chain_digest BLOB NOT NULL, object_count INTEGER NOT NULL, object_bytes INTEGER NOT NULL, node_count INTEGER NOT NULL, node_bytes INTEGER NOT NULL, sealed INTEGER NOT NULL CHECK(sealed IN (0,1))) WITHOUT ROWID`,
  `CREATE TABLE efs_operation_ids (id TEXT PRIMARY KEY, branch_id TEXT NOT NULL, generation INTEGER NOT NULL, created_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_operation_results (operation_id TEXT PRIMARY KEY REFERENCES efs_operation_ids(id), outcome INTEGER NOT NULL, encoded BLOB NOT NULL, expires_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_root_journal (generation INTEGER PRIMARY KEY, kind INTEGER NOT NULL, root_id BLOB NOT NULL)`,
  `CREATE TABLE efs_gc_runs (id TEXT PRIMARY KEY, state INTEGER NOT NULL, high_water INTEGER NOT NULL, root_generation INTEGER NOT NULL, cursor_kind INTEGER NOT NULL, cursor_value BLOB, created_at_ms INTEGER NOT NULL, examined_roots INTEGER NOT NULL DEFAULT 0, deleted_roots INTEGER NOT NULL DEFAULT 0, examined_nodes INTEGER NOT NULL DEFAULT 0, deleted_nodes INTEGER NOT NULL DEFAULT 0, examined_objects INTEGER NOT NULL DEFAULT 0, deleted_objects INTEGER NOT NULL DEFAULT 0, reclaimed_object_bytes INTEGER NOT NULL DEFAULT 0, reclaimed_manifest_bytes INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID`,
  `CREATE TABLE efs_gc_marks (run_id TEXT NOT NULL REFERENCES efs_gc_runs(id) ON DELETE CASCADE, kind INTEGER NOT NULL, hash BLOB NOT NULL, processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(run_id,kind,hash)) WITHOUT ROWID`,
  `CREATE TABLE efs_replication_sessions (id TEXT PRIMARY KEY, state INTEGER NOT NULL, nonce BLOB NOT NULL, cursor BLOB, expires_at_ms INTEGER NOT NULL, staged_bytes INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_replication_receipts (session_id TEXT NOT NULL REFERENCES efs_replication_sessions(id) ON DELETE CASCADE, batch_index INTEGER NOT NULL, digest BLOB NOT NULL, encoded BLOB NOT NULL, PRIMARY KEY(session_id,batch_index)) WITHOUT ROWID`,
];

export function createV1Schema(driver) {
  driver.transaction("exclusive", (tx) => {
    tx.run(`PRAGMA application_id=${EFS_APPLICATION_ID}`);
    for (const statement of TABLES) tx.run(statement);
    tx.run("INSERT INTO efs_revisions VALUES(0,NULL,1,'bootstrap',1)");
    tx.run("INSERT INTO efs_meta VALUES(1,1,'v1-fixture',0,'root',0,1,4096,1)");
    tx.run("INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,256)");
    tx.run("INSERT INTO efs_inodes VALUES('root',1,493,1,1,1,1,NULL,NULL,NULL,0)");
    tx.run("INSERT INTO efs_inode_revisions VALUES(0,'root',0,?)", [
      new TextEncoder().encode("{}"),
    ]);
    tx.run("PRAGMA user_version=1");
  });
}

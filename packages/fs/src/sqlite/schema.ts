import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import type { CowPageBytes } from "../cow/pages.js";

export const EFS_APPLICATION_ID = 0x45414653;
export const EFS_SCHEMA_VERSION = 3;

const CREATE_STATEMENTS = [
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
  `CREATE TABLE efs_cow_page_versions (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL CHECK(page_index>=0), generation INTEGER NOT NULL CHECK(generation>=0), bytes BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index,generation)) WITHOUT ROWID`,
  `CREATE TABLE efs_cow_page_heads (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`,
  `CREATE TABLE efs_patches (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), generation INTEGER NOT NULL CHECK(generation>=0), offset INTEGER NOT NULL CHECK(offset>=0), delete_length INTEGER NOT NULL CHECK(delete_length>=0), insert_length INTEGER NOT NULL CHECK(insert_length>=0), PRIMARY KEY(branch_id,inode_id,sequence)) WITHOUT ROWID`,
  `CREATE TABLE efs_patch_segments (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, segment_index INTEGER NOT NULL CHECK(segment_index>=0), bytes BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id,sequence,segment_index), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE CASCADE) WITHOUT ROWID`,
  `CREATE TABLE efs_leases (id TEXT PRIMARY KEY, kind INTEGER NOT NULL, owner_id TEXT NOT NULL, owner_nonce BLOB NOT NULL DEFAULT X'', branch_id TEXT, generation INTEGER, created_at_ms INTEGER NOT NULL DEFAULT 0, last_renewal_at_ms INTEGER NOT NULL DEFAULT 0, expires_at_ms INTEGER NOT NULL, state INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_manifests (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash), PRIMARY KEY(lease_id,manifest_hash)) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_objects (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,object_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_staged_manifests (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1)), manifest_hash BLOB NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,kind,manifest_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_entries (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, entry_index INTEGER NOT NULL CHECK(entry_index>=0), object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, length INTEGER NOT NULL CHECK(length>0), PRIMARY KEY(lease_id,entry_index)) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_level_records (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, level INTEGER NOT NULL CHECK(level>=0), record_index INTEGER NOT NULL CHECK(record_index>=0), node_hash BLOB NOT NULL REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, span INTEGER NOT NULL CHECK(span>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), PRIMARY KEY(lease_id,level,record_index)) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_cow_pages (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,page_index,generation), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`,
  `CREATE TABLE efs_lease_patches (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,sequence), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE RESTRICT) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_certificates (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB, chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), sealed INTEGER NOT NULL CHECK(sealed IN (0,1)), verified INTEGER NOT NULL CHECK(verified IN (0,1))) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_reconciliations (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), complete INTEGER NOT NULL CHECK(complete IN (0,1))) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_reconciliation_queue (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1,2)), hash BLOB NOT NULL CHECK(length(hash)=32), sequence INTEGER NOT NULL CHECK(sequence>=0), declared_size INTEGER NOT NULL CHECK(declared_size>=0), declared_span INTEGER, declared_entry_count INTEGER, edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0), processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(lease_id,kind,hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  `CREATE TABLE efs_operation_ids (id TEXT PRIMARY KEY, branch_id TEXT NOT NULL, generation INTEGER NOT NULL, created_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_operation_results (operation_id TEXT PRIMARY KEY REFERENCES efs_operation_ids(id), outcome INTEGER NOT NULL, encoded BLOB NOT NULL, expires_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_root_journal (generation INTEGER PRIMARY KEY, kind INTEGER NOT NULL, root_id BLOB NOT NULL)`,
  `CREATE TABLE efs_gc_runs (id TEXT PRIMARY KEY, state INTEGER NOT NULL, high_water INTEGER NOT NULL, root_generation INTEGER NOT NULL, cursor_kind INTEGER NOT NULL, cursor_value BLOB, created_at_ms INTEGER NOT NULL, examined_roots INTEGER NOT NULL DEFAULT 0, deleted_roots INTEGER NOT NULL DEFAULT 0, examined_nodes INTEGER NOT NULL DEFAULT 0, deleted_nodes INTEGER NOT NULL DEFAULT 0, examined_objects INTEGER NOT NULL DEFAULT 0, deleted_objects INTEGER NOT NULL DEFAULT 0, reclaimed_object_bytes INTEGER NOT NULL DEFAULT 0, reclaimed_manifest_bytes INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID`,
  `CREATE TABLE efs_gc_marks (run_id TEXT NOT NULL REFERENCES efs_gc_runs(id) ON DELETE CASCADE, kind INTEGER NOT NULL, hash BLOB NOT NULL, processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(run_id,kind,hash)) WITHOUT ROWID`,
  `CREATE TABLE efs_replication_sessions (id TEXT PRIMARY KEY, state INTEGER NOT NULL, nonce BLOB NOT NULL, cursor BLOB, expires_at_ms INTEGER NOT NULL, staged_bytes INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_replication_receipts (session_id TEXT NOT NULL REFERENCES efs_replication_sessions(id) ON DELETE CASCADE, batch_index INTEGER NOT NULL, digest BLOB NOT NULL, encoded BLOB NOT NULL, PRIMARY KEY(session_id,batch_index)) WITHOUT ROWID`,
] as const;

interface MetaRow extends SqliteRow { schema_version: number; filesystem_id: string; main_revision: number; root_inode: string; cow_page_bytes: number }
interface ScalarRow extends SqliteRow { value: number }

function oneNumber(tx: FilesystemSQLiteTransaction, sql: string): number {
  const rows = tx.all<ScalarRow>(sql, [], { maxRows: 1, maxBytes: 1024 });
  const value = rows[0]?.value;
  if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new Error(`invalid scalar result for ${sql}`);
  return value;
}

function inspect(tx: FilesystemSQLiteTransaction): { applicationId: number; userVersion: number; objectCount: number } {
  return {
    applicationId: oneNumber(tx, "SELECT application_id AS value FROM pragma_application_id"),
    userVersion: oneNumber(tx, "SELECT user_version AS value FROM pragma_user_version"),
    objectCount: oneNumber(tx, "SELECT count(*) AS value FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'"),
  };
}

function validateCurrent(tx: FilesystemSQLiteTransaction, requestedPageBytes?: CowPageBytes): MetaRow {
  const state = inspect(tx);
  if (state.applicationId !== EFS_APPLICATION_ID) throw new Error("ESCHEMA: wrong SQLite application_id");
  if (state.userVersion !== EFS_SCHEMA_VERSION) throw new Error("ESCHEMA: unsupported or mismatched schema version");
  const rows = tx.all<MetaRow>("SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 4096 });
  const meta = rows[0]; if (!meta || meta.schema_version !== EFS_SCHEMA_VERSION) throw new Error("ECORRUPT: invalid efs_meta singleton");
  if (requestedPageBytes !== undefined && meta.cow_page_bytes !== requestedPageBytes) throw new Error("ESCHEMA: persisted COW page size differs from requested value");
  const roots = tx.all("SELECT i.id AS inode_id,r.revision AS revision FROM efs_inodes i, efs_revisions r WHERE i.id=? AND i.type=1 AND r.revision=?", [meta.root_inode, meta.main_revision], { maxRows: 1, maxBytes: 4096 });
  if (roots.length !== 1) throw new Error("ECORRUPT: metadata head references missing root or revision");
  return meta;
}

function migrateV1ToV2(tx: FilesystemSQLiteTransaction): void {
  const state = inspect(tx);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 1) throw new Error("ESCHEMA: schema v1 migration precondition failed");
  const meta = tx.all<MetaRow>("SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 4096 })[0];
  if (!meta || meta.schema_version !== 1) throw new Error("ECORRUPT: invalid schema v1 metadata");
  tx.run("ALTER TABLE efs_cow_pages RENAME TO efs_cow_pages_v1");
  tx.run(`CREATE TABLE efs_cow_page_versions (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL CHECK(page_index>=0), generation INTEGER NOT NULL CHECK(generation>=0), bytes BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index,generation)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_cow_page_heads (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`);
  tx.run("INSERT INTO efs_cow_page_versions(branch_id,inode_id,page_index,generation,bytes,created_at_ms) SELECT branch_id,inode_id,page_index,generation,bytes,0 FROM efs_cow_pages_v1");
  tx.run("INSERT INTO efs_cow_page_heads(branch_id,inode_id,page_index,generation) SELECT branch_id,inode_id,page_index,generation FROM efs_cow_pages_v1");
  tx.run("DROP TABLE efs_cow_pages_v1");
  tx.run("ALTER TABLE efs_patches RENAME TO efs_patches_v1");
  tx.run(`CREATE TABLE efs_patches (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), generation INTEGER NOT NULL CHECK(generation>=0), offset INTEGER NOT NULL CHECK(offset>=0), delete_length INTEGER NOT NULL CHECK(delete_length>=0), insert_length INTEGER NOT NULL CHECK(insert_length>=0), PRIMARY KEY(branch_id,inode_id,sequence)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_patch_segments (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, segment_index INTEGER NOT NULL CHECK(segment_index>=0), bytes BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id,sequence,segment_index), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE CASCADE) WITHOUT ROWID`);
  tx.run("INSERT INTO efs_patches(branch_id,inode_id,sequence,generation,offset,delete_length,insert_length) SELECT branch_id,inode_id,sequence,sequence,offset,delete_length,length(insert_bytes) FROM efs_patches_v1");
  tx.run("INSERT INTO efs_patch_segments(branch_id,inode_id,sequence,segment_index,bytes) SELECT branch_id,inode_id,sequence,0,insert_bytes FROM efs_patches_v1 WHERE length(insert_bytes)>0");
  tx.run("DROP TABLE efs_patches_v1");
  tx.run("ALTER TABLE efs_leases ADD COLUMN owner_nonce BLOB NOT NULL DEFAULT X''");
  tx.run("ALTER TABLE efs_leases ADD COLUMN branch_id TEXT");
  tx.run("ALTER TABLE efs_leases ADD COLUMN generation INTEGER");
  tx.run("ALTER TABLE efs_leases ADD COLUMN created_at_ms INTEGER NOT NULL DEFAULT 0");
  tx.run("ALTER TABLE efs_leases ADD COLUMN last_renewal_at_ms INTEGER NOT NULL DEFAULT 0");
  tx.run(`CREATE TABLE efs_lease_objects (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,object_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_lease_staged_manifests (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1)), manifest_hash BLOB NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,kind,manifest_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_staging_entries (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, entry_index INTEGER NOT NULL CHECK(entry_index>=0), object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, length INTEGER NOT NULL CHECK(length>0), PRIMARY KEY(lease_id,entry_index)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_staging_level_records (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, level INTEGER NOT NULL CHECK(level>=0), record_index INTEGER NOT NULL CHECK(record_index>=0), node_hash BLOB NOT NULL REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, span INTEGER NOT NULL CHECK(span>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), PRIMARY KEY(lease_id,level,record_index)) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_lease_cow_pages (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,page_index,generation), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_lease_patches (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,sequence), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE RESTRICT) WITHOUT ROWID`);
  tx.run("ALTER TABLE efs_staging_certificates RENAME TO efs_staging_certificates_v1");
  tx.run(`CREATE TABLE efs_staging_certificates (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB, chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), sealed INTEGER NOT NULL CHECK(sealed IN (0,1)), verified INTEGER NOT NULL CHECK(verified IN (0,1))) WITHOUT ROWID`);
  tx.run("INSERT INTO efs_staging_certificates(lease_id,owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified) SELECT lease_id,X'',manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,object_count+node_count,object_count+node_count,0,0 FROM efs_staging_certificates_v1");
  tx.run("DROP TABLE efs_staging_certificates_v1");
  tx.run("UPDATE efs_meta SET schema_version=2 WHERE singleton=1");
  tx.run("PRAGMA user_version=2");
}

function migrateV2ToV3(tx: FilesystemSQLiteTransaction): void {
  const state = inspect(tx);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 2) throw new Error("ESCHEMA: schema v2 migration precondition failed");
  const meta = tx.all<MetaRow>("SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1", [], { maxRows: 1, maxBytes: 4096 })[0];
  if (!meta || meta.schema_version !== 2) throw new Error("ECORRUPT: invalid schema v2 metadata");
  tx.run(`CREATE TABLE efs_staging_reconciliations (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), complete INTEGER NOT NULL CHECK(complete IN (0,1))) WITHOUT ROWID`);
  tx.run(`CREATE TABLE efs_staging_reconciliation_queue (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1,2)), hash BLOB NOT NULL CHECK(length(hash)=32), sequence INTEGER NOT NULL CHECK(sequence>=0), declared_size INTEGER NOT NULL CHECK(declared_size>=0), declared_span INTEGER, declared_entry_count INTEGER, edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0), processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(lease_id,kind,hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`);
  tx.run("UPDATE efs_meta SET schema_version=3 WHERE singleton=1");
  tx.run("PRAGMA user_version=3");
}

export interface StorageMetadata { readonly filesystemId: string; readonly mainRevision: number; readonly rootInode: string; readonly cowPageBytes: CowPageBytes }

export function initializeOrValidateSchema(driver: FilesystemSQLiteDriver, options: { readonly cowPageBytes?: CowPageBytes; readonly now?: number } = {}): StorageMetadata {
  const requestedPageBytes = options.cowPageBytes;
  const state = driver.transaction("read", (tx) => inspect(tx));
  if (state.applicationId === EFS_APPLICATION_ID) {
    if (state.userVersion === 1) {
      if (driver.readOnly) throw new Error("ESCHEMA: schema v1 requires a writable migration");
      driver.transaction("exclusive", (tx) => { migrateV1ToV2(tx); migrateV2ToV3(tx); });
    }
    const afterV1 = driver.transaction("read", (tx) => inspect(tx));
    if (afterV1.userVersion === 2) {
      if (driver.readOnly) throw new Error("ESCHEMA: schema v2 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV2ToV3(tx));
    }
    const meta = driver.transaction("read", (tx) => validateCurrent(tx, requestedPageBytes));
    return Object.freeze({ filesystemId: meta.filesystem_id, mainRevision: meta.main_revision, rootInode: meta.root_inode, cowPageBytes: meta.cow_page_bytes as CowPageBytes });
  }
  if (state.applicationId !== 0 || state.objectCount !== 0 || state.userVersion !== 0) throw new Error("ESCHEMA: database is not an empty Ephemeral AI FS database");
  if (driver.readOnly) throw new Error("EROFS: cannot initialize a read-only database");
  const pageBytes = requestedPageBytes ?? 8192; const now = options.now ?? Date.now();
  const filesystemId = globalThis.crypto.randomUUID(); const rootInode = globalThis.crypto.randomUUID();
  driver.transaction("exclusive", (tx) => {
    const recheck = inspect(tx); if (recheck.applicationId !== 0 || recheck.objectCount !== 0 || recheck.userVersion !== 0) throw new Error("ESCHEMA: database changed during initialization");
    tx.run(`PRAGMA application_id=${EFS_APPLICATION_ID}`);
    for (const statement of CREATE_STATEMENTS) tx.run(statement);
    tx.run("INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(0,NULL,?,'bootstrap',1)", [now]);
    tx.run("INSERT INTO efs_meta(singleton,schema_version,filesystem_id,main_revision,root_inode,root_mutation_generation,next_allocation_sequence,cow_page_bytes,created_at_ms) VALUES(1,?,?,?,?,0,1,?,?)", [EFS_SCHEMA_VERSION, filesystemId, 0, rootInode, pageBytes, now]);
    tx.run("INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,256)");
    tx.run("INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,1,493,?,?,?,?,NULL,NULL,NULL,0)", [rootInode, now, now, now, 1]);
    tx.run("INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(0,?,0,?)", [rootInode, utf8Json({ id: rootInode, type: 1, mode: 493, birthtime_ms: now, mtime_ms: now, ctime_ms: now, nlink: 1, size: null, manifest_hash: null, symlink_target: null, token: 0 })]);
    tx.run(`PRAGMA user_version=${EFS_SCHEMA_VERSION}`);
    validateCurrent(tx, pageBytes);
  });
  return Object.freeze({ filesystemId, mainRevision: 0, rootInode, cowPageBytes: pageBytes });
}

function utf8Json(value: unknown): Uint8Array { return new TextEncoder().encode(JSON.stringify(value)); }

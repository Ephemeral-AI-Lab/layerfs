import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SQLiteSchemaIdentityMode,
  SqliteRow,
} from "./driver.js";
import type { CowPageBytes } from "../cow/pages.js";
import {
  CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES,
  MAX_CONTENT_OBJECT_BYTES,
} from "../resources/limits.js";
import { utf8ByteLength } from "../namespace/utf8.js";
import { certifyLegacyManifests } from "./legacy-manifest-certification.js";
import {
  CHARGED_ROW_BYTES,
  DIRECT_CHARGED_METADATA_EXPRESSION,
  DIRECT_CHARGED_METADATA_EXPRESSION_LEGACY,
  DIRECT_USAGE_TABLES,
  GC_MARK_RESERVATION_BYTES,
  USAGE_COUNTER_COLUMNS,
  USAGE_INTEGRITY_SQL,
  usageIntegrityToken,
} from "./usage-repository.js";

export const EFS_APPLICATION_ID = 0x45414653;
export const EFS_SCHEMA_VERSION = 13;
export const EFS_DURABLE_IDENTITY_TABLE = "efs_schema_identity";
export const EFS_DURABLE_IDENTITY_DDL = `CREATE TABLE ${EFS_DURABLE_IDENTITY_TABLE} (singleton INTEGER PRIMARY KEY CHECK(singleton=1), application_id INTEGER NOT NULL, user_version INTEGER NOT NULL CHECK(user_version>=0))`;
const MAX_ATOMIC_MIGRATION_RECOUNT_ROWS = 100_000;
const MAX_ATOMIC_LEGACY_TRANSFORM_BYTES =
  MAX_CONTENT_OBJECT_BYTES + CONTENT_OBJECT_TRANSACTION_OVERHEAD_BYTES;
const MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES = 64 * 1024 * 1024;
const MAX_ATOMIC_MIGRATION_MS = 5_000;

/** Frozen released schema-v3 DDL. Changes belong in a forward migration. */
export const EFS_SCHEMA_V3_CREATE_STATEMENTS = Object.freeze([
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
] as const);

const PATCH_SEQUENCE_DELETE_TRIGGER = `CREATE TRIGGER efs_patch_sequence_delete BEFORE DELETE ON efs_patches WHEN (SELECT state FROM efs_branches WHERE id=OLD.branch_id)=0 AND NOT EXISTS(SELECT 1 FROM efs_branch_inode_overlays o WHERE o.branch_id=OLD.branch_id AND o.inode_id=OLD.inode_id AND CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER)>=OLD.generation) BEGIN SELECT RAISE(ABORT,'active structural patch sequence is immutable'); END`;

const SCHEMA_V4_STATEMENTS = Object.freeze([
  `ALTER TABLE efs_usage ADD COLUMN mutation_sequence INTEGER NOT NULL DEFAULT 0 CHECK(mutation_sequence>=0)`,
  `ALTER TABLE efs_usage ADD COLUMN ingest_reservation_bytes INTEGER NOT NULL DEFAULT 0 CHECK(ingest_reservation_bytes>=0)`,
  `ALTER TABLE efs_usage ADD COLUMN integrity_token TEXT NOT NULL DEFAULT ''`,
  `ALTER TABLE efs_gc_marks ADD COLUMN edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0)`,
  `CREATE INDEX efs_gc_marks_pending ON efs_gc_marks(run_id,processed,kind,hash)`,
  `ALTER TABLE efs_staging_certificates ADD COLUMN ingest_reservation_bytes INTEGER NOT NULL DEFAULT 0 CHECK(ingest_reservation_bytes>=0)`,
  `ALTER TABLE efs_staging_certificates ADD COLUMN metadata_reservation_bytes INTEGER NOT NULL DEFAULT 0 CHECK(metadata_reservation_bytes>=0)`,
  `ALTER TABLE efs_meta ADD COLUMN max_manifest_entries INTEGER NOT NULL DEFAULT 4294967295 CHECK(max_manifest_entries BETWEEN 1 AND 4294967295)`,
  `ALTER TABLE efs_meta ADD COLUMN max_manifest_depth INTEGER NOT NULL DEFAULT 8 CHECK(max_manifest_depth BETWEEN 1 AND 64)`,
  `ALTER TABLE efs_meta ADD COLUMN max_file_bytes INTEGER NOT NULL DEFAULT 17179869184 CHECK(max_file_bytes>0)`,
  `ALTER TABLE efs_meta ADD COLUMN writer_profile TEXT NOT NULL DEFAULT '' CHECK(length(CAST(writer_profile AS BLOB))<=8192)`,
  `ALTER TABLE efs_staging_reconciliations ADD COLUMN leaf_depth INTEGER CHECK(leaf_depth IS NULL OR leaf_depth BETWEEN 1 AND 64)`,
  `CREATE TABLE efs_manifest_validations (manifest_hash BLOB PRIMARY KEY REFERENCES efs_manifest_roots(hash) ON DELETE RESTRICT, tree_depth INTEGER NOT NULL CHECK(tree_depth BETWEEN 1 AND 64)) WITHOUT ROWID`,
  `CREATE TRIGGER efs_manifest_validation_update BEFORE UPDATE ON efs_manifest_validations BEGIN SELECT RAISE(ABORT,'manifest validation certificate is immutable'); END`,
  `CREATE TRIGGER efs_manifest_validation_delete BEFORE DELETE ON efs_manifest_validations WHEN EXISTS(SELECT 1 FROM efs_inodes i WHERE i.manifest_hash=OLD.manifest_hash) OR EXISTS(SELECT 1 FROM efs_revision_manifest_roots r WHERE r.manifest_hash=OLD.manifest_hash) OR EXISTS(SELECT 1 FROM efs_branch_manifest_roots b WHERE b.manifest_hash=OLD.manifest_hash) OR EXISTS(SELECT 1 FROM efs_lease_manifests l WHERE l.manifest_hash=OLD.manifest_hash) OR EXISTS(SELECT 1 FROM efs_lease_staged_manifests s WHERE s.kind=0 AND s.manifest_hash=OLD.manifest_hash) BEGIN SELECT RAISE(ABORT,'rooted manifest validation certificate is immutable'); END`,
  `CREATE TRIGGER efs_inode_manifest_validation_insert BEFORE INSERT ON efs_inodes WHEN NEW.manifest_hash IS NOT NULL AND NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'inode manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_inode_manifest_validation_update BEFORE UPDATE OF manifest_hash ON efs_inodes WHEN NEW.manifest_hash IS NOT NULL AND NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'inode manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_revision_manifest_validation_insert BEFORE INSERT ON efs_revision_manifest_roots WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'revision manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_revision_manifest_validation_update BEFORE UPDATE OF manifest_hash ON efs_revision_manifest_roots WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'revision manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_branch_manifest_validation_insert BEFORE INSERT ON efs_branch_manifest_roots WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'branch manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_branch_manifest_validation_update BEFORE UPDATE OF manifest_hash ON efs_branch_manifest_roots WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'branch manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_lease_manifest_validation_insert BEFORE INSERT ON efs_lease_manifests WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'lease manifest requires validation certificate'); END`,
  `CREATE TRIGGER efs_lease_manifest_validation_update BEFORE UPDATE OF manifest_hash ON efs_lease_manifests WHEN NOT EXISTS(SELECT 1 FROM efs_manifest_validations v WHERE v.manifest_hash=NEW.manifest_hash) BEGIN SELECT RAISE(ABORT,'lease manifest requires validation certificate'); END`,
  `CREATE TABLE efs_staging_manifest_validation_queue (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, path BLOB NOT NULL CHECK(length(path)<=64), node_hash BLOB NOT NULL REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, declared_span INTEGER NOT NULL CHECK(declared_span>=0), declared_entry_count INTEGER NOT NULL CHECK(declared_entry_count>=0), depth INTEGER NOT NULL CHECK(depth BETWEEN 1 AND 64), final_at_level INTEGER NOT NULL CHECK(final_at_level IN (0,1)), edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0), processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(lease_id,path)) WITHOUT ROWID`,
  `CREATE INDEX efs_staging_manifest_validation_pending ON efs_staging_manifest_validation_queue(lease_id,processed,path)`,
  `CREATE INDEX efs_staging_reconciliation_pending ON efs_staging_reconciliation_queue(lease_id,processed,sequence)`,
  `CREATE TRIGGER efs_sealed_validation_queue_insert BEFORE INSERT ON efs_staging_manifest_validation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed manifest validation queue is immutable'); END`,
  `CREATE TRIGGER efs_sealed_validation_queue_update BEFORE UPDATE ON efs_staging_manifest_validation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed manifest validation queue is immutable'); END`,
  `CREATE TRIGGER efs_sealed_validation_queue_delete BEFORE DELETE ON efs_staging_manifest_validation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed manifest validation queue is immutable'); END`,
  `CREATE TABLE efs_lease_cleanups (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, phase INTEGER NOT NULL CHECK(phase BETWEEN 0 AND 12), cursor_text TEXT, cursor_blob BLOB, released_staging_bytes INTEGER NOT NULL CHECK(released_staging_bytes>=0), tombstoned_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE INDEX efs_lease_cleanups_phase ON efs_lease_cleanups(phase,lease_id)`,
  `CREATE INDEX efs_leases_expiry ON efs_leases(expires_at_ms,id)`,
  `CREATE INDEX efs_inodes_manifest_hash ON efs_inodes(manifest_hash) WHERE manifest_hash IS NOT NULL`,
  `CREATE INDEX efs_revision_manifest_hash ON efs_revision_manifest_roots(manifest_hash)`,
  `CREATE INDEX efs_branch_manifest_hash ON efs_branch_manifest_roots(manifest_hash)`,
  `CREATE UNIQUE INDEX efs_branch_manifest_path ON efs_branch_manifest_roots(branch_id,path)`,
  `CREATE INDEX efs_lease_manifest_hash ON efs_lease_manifests(manifest_hash)`,
  `CREATE INDEX efs_lease_staged_manifest_hash ON efs_lease_staged_manifests(kind,manifest_hash)`,
  `CREATE INDEX efs_staging_level_node_hash ON efs_staging_level_records(node_hash)`,
  `CREATE INDEX efs_lease_object_hash ON efs_lease_objects(object_hash)`,
  `CREATE INDEX efs_staging_entry_object_hash ON efs_staging_entries(object_hash)`,
  `CREATE TRIGGER efs_lease_tombstone_guard BEFORE UPDATE OF state ON efs_leases WHEN OLD.state IN (0,1) AND NEW.state=2 AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x WHERE x.lease_id=OLD.id AND x.owner_nonce=OLD.owner_nonce) BEGIN SELECT RAISE(ABORT,'lease tombstone requires bounded cleanup state'); END`,
  `CREATE TABLE efs_staging_workspaces (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, source_manifest_hash BLOB CHECK(source_manifest_hash IS NULL OR length(source_manifest_hash)=32), edit_offset INTEGER NOT NULL CHECK(edit_offset>=0), delete_length INTEGER NOT NULL CHECK(delete_length>=0), insert_length INTEGER NOT NULL CHECK(insert_length>=0), source_entry_cursor INTEGER NOT NULL DEFAULT -1 CHECK(source_entry_cursor>=-1), output_entry_index INTEGER NOT NULL DEFAULT 0 CHECK(output_entry_index>=0), phase INTEGER NOT NULL DEFAULT 0 CHECK(phase BETWEEN 0 AND 10), cdc_buffer BLOB NOT NULL DEFAULT X'', updated_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_staging_reused_subtrees (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, node_hash BLOB NOT NULL REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, source_manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash) ON DELETE RESTRICT, source_path BLOB NOT NULL CHECK(length(source_path)<=64), span INTEGER NOT NULL CHECK(span>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), validated_nonfinal_leaf_delta INTEGER CHECK(validated_nonfinal_leaf_delta IS NULL OR validated_nonfinal_leaf_delta BETWEEN 0 AND 63), validated_final_leaf_delta INTEGER CHECK(validated_final_leaf_delta IS NULL OR validated_final_leaf_delta BETWEEN 0 AND 63), PRIMARY KEY(lease_id,node_hash)) WITHOUT ROWID`,
  `CREATE INDEX efs_staging_reused_node_hash ON efs_staging_reused_subtrees(node_hash)`,
  `CREATE INDEX efs_staging_reused_source_manifest_hash ON efs_staging_reused_subtrees(source_manifest_hash)`,
  `CREATE TRIGGER efs_lease_delete_guard BEFORE DELETE ON efs_leases WHEN OLD.state<>2 OR NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x WHERE x.lease_id=OLD.id AND x.owner_nonce=OLD.owner_nonce AND x.phase=12) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_entries e WHERE e.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_level_records r WHERE r.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_reconciliation_queue q WHERE q.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_manifest_validation_queue q WHERE q.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_reused_subtrees s WHERE s.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_lease_objects o WHERE o.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_lease_staged_manifests m WHERE m.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_lease_manifests m WHERE m.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_lease_cow_pages p WHERE p.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_lease_patches p WHERE p.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_reconciliations r WHERE r.lease_id=OLD.id) OR EXISTS(SELECT 1 FROM efs_staging_workspaces w WHERE w.lease_id=OLD.id) BEGIN SELECT RAISE(ABORT,'lease deletion requires completed bounded cleanup'); END`,
  `CREATE TRIGGER efs_sealed_certificate_update BEFORE UPDATE ON efs_staging_certificates WHEN OLD.sealed=1 BEGIN SELECT RAISE(ABORT,'sealed staging certificate is immutable'); END`,
  `CREATE TRIGGER efs_sealed_certificate_delete BEFORE DELETE ON efs_staging_certificates WHEN OLD.sealed=1 AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging certificate is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reconciliation_insert BEFORE INSERT ON efs_staging_reconciliations WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reconciliation_update BEFORE UPDATE ON efs_staging_reconciliations WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reconciliation_delete BEFORE DELETE ON efs_staging_reconciliations WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation is immutable'); END`,
  `CREATE TRIGGER efs_sealed_queue_insert BEFORE INSERT ON efs_staging_reconciliation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation queue is immutable'); END`,
  `CREATE TRIGGER efs_sealed_queue_update BEFORE UPDATE ON efs_staging_reconciliation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation queue is immutable'); END`,
  `CREATE TRIGGER efs_sealed_queue_delete BEFORE DELETE ON efs_staging_reconciliation_queue WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging reconciliation queue is immutable'); END`,
  `CREATE TRIGGER efs_sealed_object_member_insert BEFORE INSERT ON efs_lease_objects WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_object_member_update BEFORE UPDATE ON efs_lease_objects WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_object_member_delete BEFORE DELETE ON efs_lease_objects WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_manifest_member_insert BEFORE INSERT ON efs_lease_staged_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_manifest_member_update BEFORE UPDATE ON efs_lease_staged_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_manifest_member_delete BEFORE DELETE ON efs_lease_staged_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging membership is immutable'); END`,
  `CREATE TRIGGER efs_sealed_root_link_insert BEFORE INSERT ON efs_lease_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging root link is immutable'); END`,
  `CREATE TRIGGER efs_sealed_root_link_update BEFORE UPDATE ON efs_lease_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed staging root link is immutable'); END`,
  `CREATE TRIGGER efs_sealed_root_link_delete BEFORE DELETE ON efs_lease_manifests WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed staging root link is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reused_subtree_insert BEFORE INSERT ON efs_staging_reused_subtrees WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed reused subtree is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reused_subtree_update BEFORE UPDATE ON efs_staging_reused_subtrees WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) OR EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=NEW.lease_id AND c.sealed=1) BEGIN SELECT RAISE(ABORT,'sealed reused subtree is immutable'); END`,
  `CREATE TRIGGER efs_sealed_reused_subtree_delete BEFORE DELETE ON efs_staging_reused_subtrees WHEN EXISTS(SELECT 1 FROM efs_staging_certificates c WHERE c.lease_id=OLD.lease_id AND c.sealed=1) AND NOT EXISTS(SELECT 1 FROM efs_lease_cleanups x JOIN efs_leases l ON l.id=x.lease_id AND l.owner_nonce=x.owner_nonce WHERE x.lease_id=OLD.lease_id AND l.state=2) BEGIN SELECT RAISE(ABORT,'sealed reused subtree is immutable'); END`,
  `CREATE TRIGGER efs_patch_sequence_insert BEFORE INSERT ON efs_patches WHEN NEW.sequence<>(SELECT coalesce(max(sequence),-1)+1 FROM efs_patches WHERE branch_id=NEW.branch_id AND inode_id=NEW.inode_id) BEGIN SELECT RAISE(ABORT,'structural patch sequence must be contiguous'); END`,
  `CREATE TRIGGER efs_patch_sequence_update BEFORE UPDATE OF branch_id,inode_id,sequence ON efs_patches WHEN OLD.branch_id<>NEW.branch_id OR OLD.inode_id<>NEW.inode_id OR OLD.sequence<>NEW.sequence BEGIN SELECT RAISE(ABORT,'structural patch sequence is immutable'); END`,
  PATCH_SEQUENCE_DELETE_TRIGGER,
  `UPDATE efs_usage SET object_count=(SELECT count(*) FROM efs_cas_objects),object_bytes=(SELECT coalesce(sum(size),0) FROM efs_cas_objects) WHERE singleton=1`,
  `UPDATE efs_usage SET manifest_root_count=(SELECT count(*) FROM efs_manifest_roots),manifest_root_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_roots),manifest_node_count=(SELECT count(*) FROM efs_manifest_nodes),manifest_node_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_nodes) WHERE singleton=1`,
  `UPDATE efs_usage SET page_count=(SELECT count(*) FROM efs_cow_page_versions),page_bytes=(SELECT coalesce(sum(length(bytes)),0) FROM efs_cow_page_versions),patch_count=(SELECT count(*) FROM efs_patches),patch_bytes=(SELECT coalesce(sum(length(bytes)),0) FROM efs_patch_segments) WHERE singleton=1`,
  `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION_LEGACY} WHERE singleton=1`,
  `UPDATE efs_usage SET staging_bytes=(SELECT (SELECT coalesce(sum(o.size),0) FROM efs_lease_objects o JOIN efs_leases l ON l.id=o.lease_id WHERE l.state IN (0,1))+(SELECT coalesce(sum(m.size),0) FROM efs_lease_staged_manifests m JOIN efs_leases l ON l.id=m.lease_id WHERE l.state IN (0,1))) WHERE singleton=1`,
  `UPDATE efs_usage SET ingest_reservation_bytes=0 WHERE singleton=1`,
  `UPDATE efs_usage SET result_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_operation_results),permanent_identifiers=(SELECT count(*) FROM efs_branch_ids)+(SELECT count(*) FROM efs_operation_ids) WHERE singleton=1`,
  `UPDATE efs_usage SET maintenance_bytes=(SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_cas_objects)+(SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_manifest_roots)+(SELECT (count(*)*${GC_MARK_RESERVATION_BYTES}) FROM efs_manifest_nodes)+(SELECT count(*)*${CHARGED_ROW_BYTES}+coalesce(sum(length(root_id)),0) FROM efs_root_journal)+(SELECT count(*)*512+coalesce(sum(2*length(CAST(id AS BLOB))),0) FROM efs_gc_runs) WHERE singleton=1`,
  `UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL} WHERE singleton=1`,
] as const);

const SCHEMA_V5_STATEMENTS = Object.freeze([
  `ALTER TABLE efs_staging_certificates ADD COLUMN chain_fold BLOB NOT NULL DEFAULT X'0000000000000000000000000000000000000000000000000000000000000000' CHECK(length(chain_fold)=32)`,
  `ALTER TABLE efs_staging_reconciliations ADD COLUMN closure_fold BLOB NOT NULL DEFAULT X'0000000000000000000000000000000000000000000000000000000000000000' CHECK(length(closure_fold)=32)`,
] as const);

const SCHEMA_V6_STATEMENTS = Object.freeze([
  `CREATE TABLE efs_manifest_subtree_summaries (node_hash BLOB PRIMARY KEY REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), closure_fold BLOB NOT NULL CHECK(length(closure_fold)=32), chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_bloom BLOB NOT NULL CHECK(length(object_bloom)=1024), node_bloom BLOB NOT NULL CHECK(length(node_bloom)=1024), object_members BLOB NOT NULL CHECK(length(object_members)%32=0), node_members BLOB NOT NULL CHECK(length(node_members)%32=0)) WITHOUT ROWID`,
  `CREATE INDEX efs_manifest_subtree_summary_node_hash ON efs_manifest_subtree_summaries(node_hash)`,
  `ALTER TABLE efs_staging_reused_subtrees ADD COLUMN summary_usable INTEGER NOT NULL DEFAULT 0 CHECK(summary_usable IN (0,1))`,
] as const);

const SCHEMA_V7_STATEMENTS = Object.freeze([
  `CREATE TABLE efs_branch_inode_overlays (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, expected_token INTEGER, encoded BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id)) WITHOUT ROWID`,
] as const);

const SCHEMA_V8_STATEMENTS = Object.freeze([
  `CREATE TABLE efs_subtree_tokens (inode_id TEXT PRIMARY KEY, token INTEGER NOT NULL CHECK(token>=0)) WITHOUT ROWID`,
] as const);

const SCHEMA_V9_STATEMENTS = Object.freeze([
  `CREATE TABLE efs_revision_checkpoints (target_revision INTEGER PRIMARY KEY REFERENCES efs_revisions(revision), state INTEGER NOT NULL CHECK(state IN (0,1)), phase INTEGER NOT NULL CHECK(phase BETWEEN 0 AND 7), inode_cursor TEXT, entry_parent TEXT, entry_name_sort BLOB, inode_count INTEGER NOT NULL DEFAULT 0 CHECK(inode_count>=0), entry_count INTEGER NOT NULL DEFAULT 0 CHECK(entry_count>=0), created_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_checkpoint_inodes (target_revision INTEGER NOT NULL REFERENCES efs_revision_checkpoints(target_revision) ON DELETE CASCADE, inode_id TEXT NOT NULL, tombstone INTEGER NOT NULL CHECK(tombstone IN (0,1)), encoded BLOB, PRIMARY KEY(target_revision,inode_id)) WITHOUT ROWID`,
  `CREATE TABLE efs_checkpoint_entries (target_revision INTEGER NOT NULL REFERENCES efs_revision_checkpoints(target_revision) ON DELETE CASCADE, parent_inode TEXT NOT NULL, name_sort BLOB NOT NULL, tombstone INTEGER NOT NULL CHECK(tombstone IN (0,1)), encoded BLOB, PRIMARY KEY(target_revision,parent_inode,name_sort)) WITHOUT ROWID`,
  `CREATE TABLE efs_checkpoint_manifest_roots (target_revision INTEGER NOT NULL REFERENCES efs_revision_checkpoints(target_revision) ON DELETE CASCADE, inode_id TEXT NOT NULL, manifest_hash BLOB NOT NULL REFERENCES efs_manifest_roots(hash), PRIMARY KEY(target_revision,inode_id,manifest_hash)) WITHOUT ROWID`,
  `CREATE INDEX efs_checkpoint_manifest_hash ON efs_checkpoint_manifest_roots(manifest_hash)`,
] as const);
const SCHEMA_V9_ALTER_STATEMENTS = Object.freeze([
  `ALTER TABLE efs_operation_results ADD COLUMN revision INTEGER REFERENCES efs_revisions(revision)`,
] as const);
const SCHEMA_V10_STATEMENTS = Object.freeze([
  `ALTER TABLE efs_operation_ids ADD COLUMN reservation_nonce BLOB NOT NULL DEFAULT X'00000000000000000000000000000000' CHECK(length(reservation_nonce)=16)`,
] as const);
const SCHEMA_V11_STATEMENTS = Object.freeze([
  "DROP TRIGGER efs_patch_sequence_delete",
  PATCH_SEQUENCE_DELETE_TRIGGER,
] as const);
const SCHEMA_V12_STATEMENTS = Object.freeze([
  "ALTER TABLE efs_branches ADD COLUMN merged_revision INTEGER REFERENCES efs_revisions(revision)",
] as const);
const SCHEMA_V13_STATEMENTS = Object.freeze([
  `ALTER TABLE efs_meta ADD COLUMN last_root_removal_generation INTEGER NOT NULL DEFAULT 0 CHECK(last_root_removal_generation>=0 AND last_root_removal_generation<=root_mutation_generation)`,
  `CREATE TABLE efs_storage_snapshots (singleton INTEGER PRIMARY KEY CHECK(singleton=1), state INTEGER NOT NULL CHECK(state BETWEEN 1 AND 7), high_water INTEGER NOT NULL CHECK(high_water>=0), root_generation INTEGER NOT NULL CHECK(root_generation>=0), last_root_removal_generation INTEGER NOT NULL CHECK(last_root_removal_generation>=0 AND last_root_removal_generation<=root_generation), evaluation_time_ms INTEGER NOT NULL CHECK(evaluation_time_ms>=0), next_root_expiry_ms INTEGER CHECK(next_root_expiry_ms IS NULL OR next_root_expiry_ms>=0), root_kind INTEGER NOT NULL CHECK(root_kind BETWEEN 0 AND 9), root_cursor BLOB, mark_kind INTEGER NOT NULL CHECK(mark_kind BETWEEN 0 AND 3), mark_cursor BLOB, stored_kind INTEGER NOT NULL CHECK(stored_kind BETWEEN 0 AND 3), stored_cursor INTEGER NOT NULL CHECK(stored_cursor>=0), logical_cursor TEXT NOT NULL, logical_complete INTEGER NOT NULL DEFAULT 0 CHECK(logical_complete IN (0,1)), logical_bytes INTEGER NOT NULL DEFAULT 0 CHECK(logical_bytes>=0), overlay_kind INTEGER NOT NULL DEFAULT 0 CHECK(overlay_kind BETWEEN 0 AND 2), overlay_branch_cursor TEXT NOT NULL DEFAULT '', overlay_inode_cursor TEXT NOT NULL DEFAULT '', overlay_sequence_cursor INTEGER NOT NULL DEFAULT -1 CHECK(overlay_sequence_cursor>=-1), overlay_index_cursor INTEGER NOT NULL DEFAULT -1 CHECK(overlay_index_cursor>=-1), stored_page_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stored_page_bytes>=0), stored_patch_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stored_patch_bytes>=0), reclaimable_overlay_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reclaimable_overlay_bytes>=0), result_bytes INTEGER NOT NULL DEFAULT 0 CHECK(result_bytes>=0), charged_metadata_bytes INTEGER NOT NULL DEFAULT 0 CHECK(charged_metadata_bytes>=0), revision_count INTEGER NOT NULL DEFAULT 0 CHECK(revision_count>=0), stored_object_count INTEGER NOT NULL DEFAULT 0 CHECK(stored_object_count>=0), stored_object_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stored_object_bytes>=0), stored_manifest_root_count INTEGER NOT NULL DEFAULT 0 CHECK(stored_manifest_root_count>=0), stored_manifest_root_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stored_manifest_root_bytes>=0), stored_manifest_node_count INTEGER NOT NULL DEFAULT 0 CHECK(stored_manifest_node_count>=0), stored_manifest_node_bytes INTEGER NOT NULL DEFAULT 0 CHECK(stored_manifest_node_bytes>=0), reachable_object_count INTEGER NOT NULL DEFAULT 0 CHECK(reachable_object_count>=0), reachable_object_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reachable_object_bytes>=0), reachable_manifest_root_count INTEGER NOT NULL DEFAULT 0 CHECK(reachable_manifest_root_count>=0), reachable_manifest_root_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reachable_manifest_root_bytes>=0), reachable_manifest_node_count INTEGER NOT NULL DEFAULT 0 CHECK(reachable_manifest_node_count>=0), reachable_manifest_node_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reachable_manifest_node_bytes>=0), branch_exclusive_object_bytes INTEGER NOT NULL DEFAULT 0 CHECK(branch_exclusive_object_bytes>=0), branch_exclusive_manifest_root_bytes INTEGER NOT NULL DEFAULT 0 CHECK(branch_exclusive_manifest_root_bytes>=0), branch_exclusive_manifest_node_bytes INTEGER NOT NULL DEFAULT 0 CHECK(branch_exclusive_manifest_node_bytes>=0), committed_batches INTEGER NOT NULL DEFAULT 0 CHECK(committed_batches>=0), created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL) WITHOUT ROWID`,
  `CREATE TABLE efs_storage_marks (kind INTEGER NOT NULL CHECK(kind IN (0,1,2)), hash BLOB NOT NULL CHECK(length(hash)=32), edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0), processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), accounted INTEGER NOT NULL DEFAULT 0 CHECK(accounted IN (0,1)), scope_mask INTEGER NOT NULL CHECK(scope_mask BETWEEN 0 AND 7), PRIMARY KEY(kind,hash)) WITHOUT ROWID`,
  `CREATE INDEX efs_storage_marks_pending ON efs_storage_marks(processed,kind,hash)`,
  `CREATE TABLE efs_root_holds (id TEXT PRIMARY KEY, kind INTEGER NOT NULL CHECK(kind IN (0,1)), root_id BLOB NOT NULL CHECK(length(root_id)=32)) WITHOUT ROWID`,
  `CREATE INDEX efs_root_holds_kind ON efs_root_holds(kind,root_id)`,
  `ALTER TABLE efs_gc_runs ADD COLUMN reclaimed_overlay_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reclaimed_overlay_bytes>=0)`,
] as const);

const REQUIRED_V4_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V4_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_V6_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V6_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_V7_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V7_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_V8_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V8_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_V9_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V9_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_V13_SCHEMA_OBJECTS = Object.freeze(
  SCHEMA_V13_STATEMENTS.flatMap((sql) => {
    const matched = /^CREATE (?:TABLE|INDEX|TRIGGER) ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [Object.freeze({ name: matched[1], sql })] : [];
  }),
);
const REQUIRED_SCHEMA_OBJECTS = Object.freeze([
  ...REQUIRED_V4_SCHEMA_OBJECTS.filter(
    ({ name }) => name !== "efs_staging_reused_subtrees",
  ),
  ...REQUIRED_V6_SCHEMA_OBJECTS,
  ...REQUIRED_V7_SCHEMA_OBJECTS,
  ...REQUIRED_V8_SCHEMA_OBJECTS,
  ...REQUIRED_V9_SCHEMA_OBJECTS,
  ...REQUIRED_V13_SCHEMA_OBJECTS,
]);
const OWNED_TABLE_NAMES = Object.freeze(
  [
    EFS_DURABLE_IDENTITY_DDL,
    ...EFS_SCHEMA_V3_CREATE_STATEMENTS,
    ...SCHEMA_V4_STATEMENTS,
    ...SCHEMA_V6_STATEMENTS,
    ...SCHEMA_V7_STATEMENTS,
    ...SCHEMA_V8_STATEMENTS,
    ...SCHEMA_V9_STATEMENTS,
    ...SCHEMA_V13_STATEMENTS,
  ].flatMap((sql) => {
    const matched = /^CREATE TABLE ([a-z0-9_]+)/u.exec(sql);
    return matched?.[1] ? [matched[1]] : [];
  }),
);
const REQUIRED_OWNED_TRIGGER_COUNT = REQUIRED_V4_SCHEMA_OBJECTS.filter(({ sql }) =>
  sql.startsWith("CREATE TRIGGER "),
).length;

interface MetaRow extends SqliteRow {
  schema_version: number;
  filesystem_id: string;
  main_revision: number;
  root_inode: string;
  root_mutation_generation: number;
  last_root_removal_generation: number;
  next_allocation_sequence: number;
  cow_page_bytes: number;
  max_manifest_entries: number;
  max_manifest_depth: number;
  max_file_bytes: number;
  writer_profile: string;
}
interface PersistedManifestLimits {
  readonly maxManifestEntries: number;
  readonly maxManifestDepth: number;
  readonly maxFileBytes: number;
  readonly maxContentObjectBytes: number;
}
interface ScalarRow extends SqliteRow {
  value: number;
}

function oneNumber(tx: FilesystemSQLiteTransaction, sql: string): number {
  const rows = tx.all<ScalarRow>(sql, [], { maxRows: 1, maxBytes: 1024 });
  const value = rows[0]?.value;
  if (typeof value !== "number" || !Number.isSafeInteger(value))
    throw new Error(`invalid scalar result for ${sql}`);
  return value;
}

function sqlText(value: string): string {
  return `'${value.replaceAll("'", "''")}'`;
}

function inspectIdentity(
  tx: FilesystemSQLiteTransaction,
  mode: SQLiteSchemaIdentityMode,
): { applicationId: number; userVersion: number } {
  if (mode === "sqlite-header")
    return {
      applicationId: oneNumber(
        tx,
        "SELECT application_id AS value FROM pragma_application_id",
      ),
      userVersion: oneNumber(
        tx,
        "SELECT user_version AS value FROM pragma_user_version",
      ),
    };
  const exists = oneNumber(
    tx,
    `SELECT count(*) AS value FROM sqlite_schema WHERE type='table' AND name=${sqlText(EFS_DURABLE_IDENTITY_TABLE)}`,
  );
  if (exists === 0) return { applicationId: 0, userVersion: 0 };
  if (exists !== 1) throw new Error("ESCHEMA: invalid durable schema identity table");
  if (
    oneNumber(
      tx,
      `SELECT count(*) AS value FROM sqlite_schema WHERE type='table' AND name=${sqlText(EFS_DURABLE_IDENTITY_TABLE)} AND sql=${sqlText(EFS_DURABLE_IDENTITY_DDL)}`,
    ) !== 1
  )
    throw new Error("ESCHEMA: durable schema identity table definition is invalid");
  const rows = tx.all<{ application_id: number; user_version: number } & SqliteRow>(
    `SELECT application_id,user_version FROM ${EFS_DURABLE_IDENTITY_TABLE} WHERE singleton=1`,
    [],
    { maxRows: 2, maxBytes: 256 },
  );
  const identity = rows[0];
  if (
    rows.length !== 1 ||
    !identity ||
    !Number.isSafeInteger(identity.application_id) ||
    !Number.isSafeInteger(identity.user_version) ||
    identity.user_version < 0
  )
    throw new Error("ESCHEMA: invalid durable schema identity singleton");
  return {
    applicationId: identity.application_id,
    userVersion: identity.user_version,
  };
}

function initializeIdentity(
  tx: FilesystemSQLiteTransaction,
  mode: SQLiteSchemaIdentityMode,
): void {
  if (mode === "sqlite-header") {
    tx.run(`PRAGMA application_id=${EFS_APPLICATION_ID}`);
    return;
  }
  tx.run(EFS_DURABLE_IDENTITY_DDL);
  tx.run(
    `INSERT INTO ${EFS_DURABLE_IDENTITY_TABLE}(singleton,application_id,user_version) VALUES(1,?,0)`,
    [EFS_APPLICATION_ID],
  );
}

function setUserVersion(
  tx: FilesystemSQLiteTransaction,
  mode: SQLiteSchemaIdentityMode,
  version: number,
): void {
  if (!Number.isSafeInteger(version) || version < 0)
    throw new RangeError("invalid SQLite schema version");
  if (mode === "sqlite-header") {
    tx.run(`PRAGMA user_version=${version}`);
    return;
  }
  const result = tx.run(
    `UPDATE ${EFS_DURABLE_IDENTITY_TABLE} SET user_version=? WHERE singleton=1`,
    [version],
  );
  if (result.changes !== 1)
    throw new Error("ESCHEMA: durable schema identity singleton is missing");
}

function inspect(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): {
  applicationId: number;
  userVersion: number;
  objectCount: number;
} {
  const identity = inspectIdentity(tx, identityMode);
  return {
    ...identity,
    objectCount: oneNumber(
      tx,
      "SELECT count(*) AS value FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
    ),
  };
}

function inspectForOpen(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): ReturnType<typeof inspect> {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID) return state;
  const metaVersion = oneNumber(
    tx,
    "SELECT schema_version AS value FROM efs_meta WHERE singleton=1",
  );
  if (metaVersion !== state.userVersion)
    throw new Error(
      "ESCHEMA: selected identity user_version does not match efs_meta.schema_version",
    );
  return state;
}

function validateCurrent(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
  requestedPageBytes?: CowPageBytes,
  requestedManifest?: PersistedManifestLimits,
  requestedWriterProfile = "",
): MetaRow {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID)
    throw new Error("ESCHEMA: wrong SQLite application_id");
  if (state.userVersion !== EFS_SCHEMA_VERSION)
    throw new Error("ESCHEMA: unsupported or mismatched schema version");
  const rows = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,root_mutation_generation,last_root_removal_generation,next_allocation_sequence,cow_page_bytes,max_manifest_entries,max_manifest_depth,max_file_bytes,writer_profile FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  );
  const meta = rows[0];
  if (!meta || meta.schema_version !== EFS_SCHEMA_VERSION)
    throw new Error("ECORRUPT: invalid efs_meta singleton");
  if (requestedPageBytes !== undefined && meta.cow_page_bytes !== requestedPageBytes)
    throw new Error("ESCHEMA: persisted COW page size differs from requested value");
  if (
    requestedManifest &&
    (meta.max_manifest_entries !== requestedManifest.maxManifestEntries ||
      meta.max_manifest_depth !== requestedManifest.maxManifestDepth ||
      meta.max_file_bytes !== requestedManifest.maxFileBytes)
  )
    throw new Error("ESCHEMA: persisted manifest limits differ from requested values");
  if (meta.writer_profile !== requestedWriterProfile)
    throw new Error(
      "ESCHEMA: persisted writer limit profile differs from requested values",
    );
  if (
    !Number.isSafeInteger(meta.main_revision) ||
    meta.main_revision < 0 ||
    !Number.isSafeInteger(meta.root_mutation_generation) ||
    meta.root_mutation_generation < 0 ||
    !Number.isSafeInteger(meta.last_root_removal_generation) ||
    meta.last_root_removal_generation < 0 ||
    meta.last_root_removal_generation > meta.root_mutation_generation ||
    !Number.isSafeInteger(meta.next_allocation_sequence) ||
    meta.next_allocation_sequence < 1 ||
    !Number.isSafeInteger(meta.max_manifest_entries) ||
    !Number.isSafeInteger(meta.max_manifest_depth) ||
    !Number.isSafeInteger(meta.max_file_bytes)
  )
    throw new Error("ECORRUPT: invalid persisted filesystem metadata");
  const gcRuns = tx.all<
    {
      id: string;
      id_bytes: number;
      state: number;
      high_water: number;
      root_generation: number;
      cursor_kind: number;
      cursor_valid: number;
      created_at_ms: number;
      examined_roots: number;
      deleted_roots: number;
      examined_nodes: number;
      deleted_nodes: number;
      examined_objects: number;
      deleted_objects: number;
      reclaimed_object_bytes: number;
      reclaimed_manifest_bytes: number;
      reclaimed_overlay_bytes: number;
    } & SqliteRow
  >(
    "SELECT id,length(CAST(id AS BLOB)) id_bytes,state,high_water,root_generation,cursor_kind,CASE WHEN cursor_value IS NULL OR (typeof(cursor_value)='blob' AND length(cursor_value)=32) THEN 1 ELSE 0 END cursor_valid,created_at_ms,examined_roots,deleted_roots,examined_nodes,deleted_nodes,examined_objects,deleted_objects,reclaimed_object_bytes,reclaimed_manifest_bytes,reclaimed_overlay_bytes FROM efs_gc_runs ORDER BY id LIMIT 3",
    [],
    { maxRows: 3, maxBytes: 4096 },
  );
  if (gcRuns.length > 2)
    throw new Error("ECORRUPT: too many retained garbage-collection runs");
  let nonterminalRuns = 0;
  for (const run of gcRuns) {
    const counters = [
      run.high_water,
      run.root_generation,
      run.cursor_kind,
      run.created_at_ms,
      run.examined_roots,
      run.deleted_roots,
      run.examined_nodes,
      run.deleted_nodes,
      run.examined_objects,
      run.deleted_objects,
      run.reclaimed_object_bytes,
      run.reclaimed_manifest_bytes,
      run.reclaimed_overlay_bytes,
    ];
    if (
      typeof run.id !== "string" ||
      run.id_bytes < 1 ||
      run.id_bytes > 256 ||
      !Number.isSafeInteger(run.state) ||
      run.state < 0 ||
      run.state > 8 ||
      !counters.every((value) => Number.isSafeInteger(value) && value >= 0) ||
      run.cursor_kind > 9 ||
      run.cursor_valid !== 1
    )
      throw new Error("ECORRUPT: invalid retained garbage-collection state");
    if (run.state !== 7 && run.state !== 8) nonterminalRuns += 1;
  }
  if (nonterminalRuns > 1)
    throw new Error("ECORRUPT: multiple garbage-collection runs are nonterminal");
  const invalidSnapshotState = oneNumber(
    tx,
    "SELECT count(*) value FROM (SELECT 1 FROM efs_storage_snapshots WHERE state NOT BETWEEN 1 AND 7 OR root_kind NOT BETWEEN 0 AND 9 OR mark_kind NOT BETWEEN 0 AND 3 OR stored_kind NOT BETWEEN 0 AND 3 OR overlay_kind NOT BETWEEN 0 AND 2 OR root_generation<last_root_removal_generation OR evaluation_time_ms<0 OR (next_root_expiry_ms IS NOT NULL AND next_root_expiry_ms<0) OR (root_cursor IS NOT NULL AND (typeof(root_cursor)<>'blob' OR length(root_cursor)<>32)) OR (mark_cursor IS NOT NULL AND (typeof(mark_cursor)<>'blob' OR length(mark_cursor)<>32)) OR high_water>9007199254740991 OR root_generation>9007199254740991 OR committed_batches>9007199254740991 OR stored_object_bytes>9007199254740991 OR stored_manifest_root_bytes>9007199254740991 OR stored_manifest_node_bytes>9007199254740991 OR reachable_object_bytes>9007199254740991 OR reachable_manifest_root_bytes>9007199254740991 OR reachable_manifest_node_bytes>9007199254740991 OR reclaimable_overlay_bytes>9007199254740991 LIMIT 1)",
  );
  if (invalidSnapshotState !== 0)
    throw new Error("ECORRUPT: invalid durable storage-snapshot state");
  const invalidSnapshotMarks = oneNumber(
    tx,
    "SELECT count(*) value FROM (SELECT 1 FROM efs_storage_marks WHERE kind NOT IN (0,1,2) OR typeof(hash)<>'blob' OR length(hash)<>32 OR edge_cursor<0 OR edge_cursor>9007199254740991 OR processed NOT IN (0,1) OR accounted NOT IN (0,1) OR scope_mask NOT BETWEEN 0 AND 7 LIMIT 1)",
  );
  if (invalidSnapshotMarks !== 0)
    throw new Error("ECORRUPT: invalid durable storage-snapshot mark");
  const roots = tx.all(
    "SELECT i.id AS inode_id,r.revision AS revision FROM efs_inodes i, efs_revisions r WHERE i.id=? AND i.type=1 AND r.revision=?",
    [meta.root_inode, meta.main_revision],
    { maxRows: 1, maxBytes: 4096 },
  );
  if (roots.length !== 1)
    throw new Error("ECORRUPT: metadata head references missing root or revision");
  const schemaMatches = oneNumber(
    tx,
    `SELECT count(*) value FROM sqlite_schema WHERE ${REQUIRED_SCHEMA_OBJECTS.map(
      ({ name, sql }) => `(name=${sqlText(name)} AND sql=${sqlText(sql)})`,
    ).join(" OR ")}`,
  );
  if (schemaMatches !== REQUIRED_SCHEMA_OBJECTS.length) {
    const actual = new Set(
      tx
        .all<{ name: string; sql: string } & SqliteRow>(
          "SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL",
          [],
          { maxRows: 256, maxBytes: 128 * 1024 },
        )
        .map((row) => `${row.name}\u0000${row.sql}`),
    );
    const missing = REQUIRED_SCHEMA_OBJECTS.filter(
      ({ name, sql }) => !actual.has(`${name}\u0000${sql}`),
    ).map(({ name }) => name);
    throw new Error(
      `ECORRUPT: required schema-v11 table, index, or trigger is missing (${schemaMatches}/${REQUIRED_SCHEMA_OBJECTS.length}): ${missing.join(",")}`,
    );
  }
  const durableColumns = tx.all(
    "SELECT name FROM pragma_table_info('efs_branches') WHERE name='merged_revision' UNION ALL SELECT name FROM pragma_table_info('efs_operation_results') WHERE name='revision' ORDER BY name",
    [],
    { maxRows: 2, maxBytes: 512 },
  );
  if (durableColumns.length !== 2)
    throw new Error("ECORRUPT: durable branch publication columns are missing");
  const invalidBranchStates = oneNumber(
    tx,
    "SELECT count(*) value FROM efs_branches WHERE (state=0 AND (terminal_at_ms IS NOT NULL OR merged_revision IS NOT NULL)) OR (state=1 AND (terminal_at_ms IS NULL OR merged_revision IS NULL)) OR (state=2 AND (terminal_at_ms IS NULL OR merged_revision IS NOT NULL))",
  );
  if (invalidBranchStates !== 0)
    throw new Error("ECORRUPT: branch terminal metadata invariant is violated");
  const reservationNonce = tx.all(
    "SELECT name FROM pragma_table_info('efs_operation_ids') WHERE name='reservation_nonce'",
    [],
    { maxRows: 1, maxBytes: 256 },
  );
  if (reservationNonce.length !== 1)
    throw new Error("ECORRUPT: operation reservations lack a durable nonce");
  const ownedTriggerCount = oneNumber(
    tx,
    `SELECT count(*) value FROM sqlite_schema WHERE type='trigger' AND tbl_name IN (${OWNED_TABLE_NAMES.map(sqlText).join(",")})`,
  );
  if (ownedTriggerCount !== REQUIRED_OWNED_TRIGGER_COUNT)
    throw new Error("ECORRUPT: unexpected trigger mutates an owned filesystem table");
  const usage = tx.all<SqliteRow>(
    `SELECT ${USAGE_COUNTER_COLUMNS.join(",")},mutation_sequence,integrity_token FROM efs_usage WHERE singleton=1`,
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!usage) throw new Error("ECORRUPT: missing usage singleton");
  for (const column of [...USAGE_COUNTER_COLUMNS, "mutation_sequence"] as const)
    if (!Number.isSafeInteger(usage[column]) || (usage[column] as number) < 0)
      throw new Error(`ECORRUPT: invalid usage counter ${column}`);
  if (
    typeof usage.integrity_token !== "string" ||
    usage.integrity_token !==
      usageIntegrityToken(
        usage as Readonly<Record<(typeof USAGE_COUNTER_COLUMNS)[number], number>> & {
          readonly mutation_sequence: number;
        },
      )
  )
    throw new Error("ECORRUPT: usage integrity token mismatch");
  return meta;
}

function migrateV1ToV2(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
  deadline: number,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 1)
    throw new Error("ESCHEMA: schema v1 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 1)
    throw new Error("ECORRUPT: invalid schema v1 metadata");
  tx.run("ALTER TABLE efs_cow_pages RENAME TO efs_cow_pages_v1");
  tx.run(
    `CREATE TABLE efs_cow_page_versions (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL CHECK(page_index>=0), generation INTEGER NOT NULL CHECK(generation>=0), bytes BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index,generation)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_cow_page_heads (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(branch_id,inode_id,page_index), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`,
  );
  const cowPages = tx.all<
    {
      branch_id: string;
      inode_id: string;
      page_index: number;
      generation: number;
      bytes: Uint8Array;
    } & SqliteRow
  >(
    "SELECT branch_id,inode_id,page_index,generation,bytes FROM efs_cow_pages_v1 ORDER BY branch_id,inode_id,page_index",
    [],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  for (const row of cowPages) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
    tx.run(
      "INSERT INTO efs_cow_page_versions(branch_id,inode_id,page_index,generation,bytes,created_at_ms) VALUES(?,?,?,?,?,0)",
      [row.branch_id, row.inode_id, row.page_index, row.generation, row.bytes],
    );
    tx.run(
      "INSERT INTO efs_cow_page_heads(branch_id,inode_id,page_index,generation) VALUES(?,?,?,?)",
      [row.branch_id, row.inode_id, row.page_index, row.generation],
    );
  }
  tx.run("DROP TABLE efs_cow_pages_v1");
  tx.run("ALTER TABLE efs_patches RENAME TO efs_patches_v1");
  tx.run(
    `CREATE TABLE efs_patches (branch_id TEXT NOT NULL REFERENCES efs_branches(id) ON DELETE CASCADE, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), generation INTEGER NOT NULL CHECK(generation>=0), offset INTEGER NOT NULL CHECK(offset>=0), delete_length INTEGER NOT NULL CHECK(delete_length>=0), insert_length INTEGER NOT NULL CHECK(insert_length>=0), PRIMARY KEY(branch_id,inode_id,sequence)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_patch_segments (branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, segment_index INTEGER NOT NULL CHECK(segment_index>=0), bytes BLOB NOT NULL, PRIMARY KEY(branch_id,inode_id,sequence,segment_index), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE CASCADE) WITHOUT ROWID`,
  );
  const patches = tx.all<
    {
      branch_id: string;
      inode_id: string;
      sequence: number;
      offset: number;
      delete_length: number;
      insert_bytes: Uint8Array;
    } & SqliteRow
  >(
    "SELECT branch_id,inode_id,sequence,offset,delete_length,insert_bytes FROM efs_patches_v1 ORDER BY branch_id,inode_id,sequence",
    [],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  for (const row of patches) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
    tx.run(
      "INSERT INTO efs_patches(branch_id,inode_id,sequence,generation,offset,delete_length,insert_length) VALUES(?,?,?,?,?,?,?)",
      [
        row.branch_id,
        row.inode_id,
        row.sequence,
        row.sequence,
        row.offset,
        row.delete_length,
        row.insert_bytes.length,
      ],
    );
    if (row.insert_bytes.length)
      tx.run(
        "INSERT INTO efs_patch_segments(branch_id,inode_id,sequence,segment_index,bytes) VALUES(?,?,?,0,?)",
        [row.branch_id, row.inode_id, row.sequence, row.insert_bytes],
      );
  }
  tx.run("DROP TABLE efs_patches_v1");
  tx.run("ALTER TABLE efs_leases ADD COLUMN owner_nonce BLOB NOT NULL DEFAULT X''");
  tx.run("ALTER TABLE efs_leases ADD COLUMN branch_id TEXT");
  tx.run("ALTER TABLE efs_leases ADD COLUMN generation INTEGER");
  tx.run("ALTER TABLE efs_leases ADD COLUMN created_at_ms INTEGER NOT NULL DEFAULT 0");
  tx.run(
    "ALTER TABLE efs_leases ADD COLUMN last_renewal_at_ms INTEGER NOT NULL DEFAULT 0",
  );
  tx.run(
    `CREATE TABLE efs_lease_objects (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,object_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_lease_staged_manifests (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1)), manifest_hash BLOB NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>=0), size INTEGER NOT NULL CHECK(size>=0), PRIMARY KEY(lease_id,kind,manifest_hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_staging_entries (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, entry_index INTEGER NOT NULL CHECK(entry_index>=0), object_hash BLOB NOT NULL REFERENCES efs_cas_objects(hash) ON DELETE RESTRICT, length INTEGER NOT NULL CHECK(length>0), PRIMARY KEY(lease_id,entry_index)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_staging_level_records (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, level INTEGER NOT NULL CHECK(level>=0), record_index INTEGER NOT NULL CHECK(record_index>=0), node_hash BLOB NOT NULL REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, span INTEGER NOT NULL CHECK(span>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), PRIMARY KEY(lease_id,level,record_index)) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_lease_cow_pages (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, page_index INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,page_index,generation), FOREIGN KEY(branch_id,inode_id,page_index,generation) REFERENCES efs_cow_page_versions(branch_id,inode_id,page_index,generation) ON DELETE RESTRICT) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_lease_patches (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, branch_id TEXT NOT NULL, inode_id TEXT NOT NULL, sequence INTEGER NOT NULL, PRIMARY KEY(lease_id,branch_id,inode_id,sequence), FOREIGN KEY(branch_id,inode_id,sequence) REFERENCES efs_patches(branch_id,inode_id,sequence) ON DELETE RESTRICT) WITHOUT ROWID`,
  );
  tx.run("ALTER TABLE efs_staging_certificates RENAME TO efs_staging_certificates_v1");
  tx.run(
    `CREATE TABLE efs_staging_certificates (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB, chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), sealed INTEGER NOT NULL CHECK(sealed IN (0,1)), verified INTEGER NOT NULL CHECK(verified IN (0,1))) WITHOUT ROWID`,
  );
  const certificates = tx.all<
    {
      lease_id: string;
      manifest_hash: Uint8Array;
      chain_digest: Uint8Array;
      object_count: number;
      object_bytes: number;
      node_count: number;
      node_bytes: number;
    } & SqliteRow
  >(
    "SELECT lease_id,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes FROM efs_staging_certificates_v1 ORDER BY lease_id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (certificates.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error(
      "ESCHEMA: legacy staging certificate migration exceeds its row cap",
    );
  const leaseNonces = new Map<string, Uint8Array>();
  const legacyLeases = tx.all<{ id: string } & SqliteRow>(
    "SELECT id FROM efs_leases ORDER BY id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (legacyLeases.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error("ESCHEMA: legacy lease migration exceeds its row cap");
  for (const row of legacyLeases) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
    const nonce = new Uint8Array(16);
    globalThis.crypto.getRandomValues(nonce);
    leaseNonces.set(row.id, nonce);
    tx.run("UPDATE efs_leases SET owner_nonce=? WHERE id=?", [nonce, row.id]);
  }
  for (const row of certificates) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
    const membershipCount = row.object_count + row.node_count;
    const ownerNonce = leaseNonces.get(row.lease_id);
    if (!ownerNonce)
      throw new Error("ECORRUPT: staging certificate references a missing lease");
    tx.run(
      "INSERT INTO efs_staging_certificates(lease_id,owner_nonce,manifest_hash,chain_digest,object_count,object_bytes,node_count,node_bytes,membership_count,next_sequence,sealed,verified) VALUES(?,?,?,?,?,?,?,?,?,?,0,0)",
      [
        row.lease_id,
        ownerNonce,
        row.manifest_hash,
        row.chain_digest,
        row.object_count,
        row.object_bytes,
        row.node_count,
        row.node_bytes,
        membershipCount,
        membershipCount,
      ],
    );
  }
  tx.run("DROP TABLE efs_staging_certificates_v1");
  tx.run("UPDATE efs_meta SET schema_version=2 WHERE singleton=1");
  setUserVersion(tx, identityMode, 2);
}

function migrateV2ToV3(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 2)
    throw new Error("ESCHEMA: schema v2 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 2)
    throw new Error("ECORRUPT: invalid schema v2 metadata");
  tx.run(
    `CREATE TABLE efs_staging_reconciliations (lease_id TEXT PRIMARY KEY REFERENCES efs_leases(id) ON DELETE CASCADE, owner_nonce BLOB NOT NULL, manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32), next_sequence INTEGER NOT NULL CHECK(next_sequence>=0), object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), complete INTEGER NOT NULL CHECK(complete IN (0,1))) WITHOUT ROWID`,
  );
  tx.run(
    `CREATE TABLE efs_staging_reconciliation_queue (lease_id TEXT NOT NULL REFERENCES efs_leases(id) ON DELETE CASCADE, kind INTEGER NOT NULL CHECK(kind IN (0,1,2)), hash BLOB NOT NULL CHECK(length(hash)=32), sequence INTEGER NOT NULL CHECK(sequence>=0), declared_size INTEGER NOT NULL CHECK(declared_size>=0), declared_span INTEGER, declared_entry_count INTEGER, edge_cursor INTEGER NOT NULL DEFAULT 0 CHECK(edge_cursor>=0), processed INTEGER NOT NULL DEFAULT 0 CHECK(processed IN (0,1)), PRIMARY KEY(lease_id,kind,hash), UNIQUE(lease_id,sequence)) WITHOUT ROWID`,
  );
  tx.run("UPDATE efs_meta SET schema_version=3 WHERE singleton=1");
  setUserVersion(tx, identityMode, 3);
}

function migrateV3ToV4(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
  manifest?: PersistedManifestLimits,
  writerProfile = "",
): void {
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 3)
    throw new Error("ESCHEMA: schema v3 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 3)
    throw new Error("ECORRUPT: invalid schema v3 metadata");
  assertBoundedMigrationRecount(tx);
  for (const statement of SCHEMA_V4_STATEMENTS) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: v4 atomic migration exceeds its time cap");
    tx.run(statement);
  }
  if (manifest) {
    tx.run(
      "UPDATE efs_meta SET max_manifest_entries=?,max_manifest_depth=?,max_file_bytes=?,writer_profile=? WHERE singleton=1",
      [
        manifest.maxManifestEntries,
        manifest.maxManifestDepth,
        manifest.maxFileBytes,
        writerProfile,
      ],
    );
    certifyLegacyManifests(tx, manifest);
    tx.run(
      `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION_LEGACY} WHERE singleton=1`,
    );
    tx.run(`UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL}`);
  }
  tx.run("UPDATE efs_meta SET schema_version=4 WHERE singleton=1");
  setUserVersion(tx, identityMode, 4);
  if (performance.now() > deadline)
    throw new Error("ESCHEMA: v4 atomic migration exceeds its time cap");
}

function migrateV4ToV5(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 4)
    throw new Error("ESCHEMA: schema v5 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 4)
    throw new Error("ECORRUPT: invalid schema v4 metadata");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const statement of SCHEMA_V5_STATEMENTS) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: v5 atomic migration exceeds its time cap");
    tx.run(statement);
  }
  tx.run("UPDATE efs_meta SET schema_version=5 WHERE singleton=1");
  setUserVersion(tx, identityMode, 5);
  if (performance.now() > deadline)
    throw new Error("ESCHEMA: v5 atomic migration exceeds its time cap");
}

function migrateV5ToV6(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 5)
    throw new Error("ESCHEMA: schema v6 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 5)
    throw new Error("ECORRUPT: invalid schema v5 metadata");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const statement of SCHEMA_V6_STATEMENTS) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: v6 atomic migration exceeds its time cap");
    tx.run(statement);
  }
  tx.run(
    `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION_LEGACY} WHERE singleton=1`,
  );
  tx.run("UPDATE efs_meta SET schema_version=6 WHERE singleton=1");
  setUserVersion(tx, identityMode, 6);
  if (performance.now() > deadline)
    throw new Error("ESCHEMA: v6 atomic migration exceeds its time cap");
}

function migrateV6ToV7(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 6)
    throw new Error("ESCHEMA: schema v7 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 6)
    throw new Error("ECORRUPT: invalid schema v6 metadata");
  for (const statement of SCHEMA_V7_STATEMENTS) tx.run(statement);
  tx.run("UPDATE efs_meta SET schema_version=7 WHERE singleton=1");
  setUserVersion(tx, identityMode, 7);
}

function migrateV7ToV8(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 7)
    throw new Error("ESCHEMA: schema v8 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 7)
    throw new Error("ECORRUPT: invalid schema v7 metadata");
  for (const statement of SCHEMA_V8_STATEMENTS) tx.run(statement);
  const directories = tx.all<{ id: string } & SqliteRow>(
    "SELECT id FROM efs_inodes WHERE type=1 ORDER BY id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (directories.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error("ESCHEMA: subtree-token migration exceeds its row cap");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const directory of directories) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: v8 subtree-token migration exceeds its time cap");
    tx.run("INSERT INTO efs_subtree_tokens(inode_id,token) VALUES(?,?)", [
      directory.id,
      meta.main_revision,
    ]);
  }
  tx.run("UPDATE efs_meta SET schema_version=8 WHERE singleton=1");
  setUserVersion(tx, identityMode, 8);
}

function migrateV8ToV9(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 8)
    throw new Error("ESCHEMA: schema v9 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 8)
    throw new Error("ECORRUPT: invalid schema v8 metadata");
  for (const statement of SCHEMA_V9_STATEMENTS) tx.run(statement);
  for (const statement of SCHEMA_V9_ALTER_STATEMENTS) tx.run(statement);
  const legacyMergedResults = tx.all<
    { operation_id: string; revision: number } & SqliteRow
  >(
    "SELECT operation_id,CAST(json_extract(CAST(encoded AS TEXT),'$.revision') AS INTEGER) revision FROM efs_operation_results WHERE outcome=1 AND length(encoded)>0 ORDER BY operation_id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (legacyMergedResults.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error("ESCHEMA: operation result revision backfill exceeds its row cap");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const row of legacyMergedResults) {
    if (performance.now() > deadline)
      throw new Error(
        "ESCHEMA: operation result revision backfill exceeds its time cap",
      );
    if (!Number.isSafeInteger(row.revision) || row.revision < 0)
      throw new Error("ECORRUPT: merged operation result has no valid revision");
    tx.run(
      "UPDATE efs_operation_results SET revision=? WHERE operation_id=? AND revision IS NULL",
      [row.revision, row.operation_id],
    );
  }
  tx.run(
    `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION} WHERE singleton=1`,
  );
  tx.run(
    `UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL} WHERE singleton=1`,
  );
  tx.run("UPDATE efs_meta SET schema_version=9 WHERE singleton=1");
  setUserVersion(tx, identityMode, 9);
}

function migrateV9ToV10(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 9)
    throw new Error("ESCHEMA: schema v10 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 9)
    throw new Error("ECORRUPT: invalid schema v9 metadata");
  for (const statement of SCHEMA_V10_STATEMENTS) tx.run(statement);
  const pending = tx.all<{ id: string; missing_result: number } & SqliteRow>(
    "SELECT i.id,CASE WHEN r.operation_id IS NULL THEN 1 ELSE 0 END missing_result FROM efs_operation_ids i LEFT JOIN efs_operation_results r ON r.operation_id=i.id WHERE r.operation_id IS NULL OR (r.outcome=-1 AND length(r.encoded)=0) ORDER BY i.id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (pending.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error("ESCHEMA: operation reservation migration exceeds its row cap");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const row of pending) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: operation reservation migration exceeds its time cap");
    if (row.missing_result) {
      tx.run(
        "INSERT INTO efs_operation_results(operation_id,outcome,encoded,expires_at_ms,revision) VALUES(?,2,X'',0,NULL)",
        [row.id],
      );
    } else {
      const nonce = new Uint8Array(16);
      globalThis.crypto.getRandomValues(nonce);
      tx.run("UPDATE efs_operation_ids SET reservation_nonce=? WHERE id=?", [
        nonce,
        row.id,
      ]);
    }
  }
  tx.run(
    `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION} WHERE singleton=1`,
  );
  tx.run(
    `UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL} WHERE singleton=1`,
  );
  tx.run("UPDATE efs_meta SET schema_version=10 WHERE singleton=1");
  setUserVersion(tx, identityMode, 10);
}

function migrateV10ToV11(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 10)
    throw new Error("ESCHEMA: schema v11 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 10)
    throw new Error("ECORRUPT: invalid schema v10 metadata");
  for (const statement of SCHEMA_V11_STATEMENTS) tx.run(statement);
  tx.run("UPDATE efs_meta SET schema_version=11 WHERE singleton=1");
  setUserVersion(tx, identityMode, 11);
}

function migrateV11ToV12(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 11)
    throw new Error("ESCHEMA: schema v12 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 11)
    throw new Error("ECORRUPT: invalid schema v11 metadata");
  for (const statement of SCHEMA_V12_STATEMENTS) tx.run(statement);
  const mergedBranches = tx.all<{ id: string; revision: number | null } & SqliteRow>(
    "SELECT b.id,(SELECT r.revision FROM efs_operation_ids i JOIN efs_operation_results r ON r.operation_id=i.id WHERE i.branch_id=b.id AND r.outcome=1 AND r.revision IS NOT NULL ORDER BY r.revision DESC LIMIT 1) revision FROM efs_branches b WHERE b.state=1 AND b.merged_revision IS NULL ORDER BY b.id LIMIT ?",
    [MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1],
    {
      maxRows: MAX_ATOMIC_MIGRATION_RECOUNT_ROWS + 1,
      maxBytes: MAX_ATOMIC_LEGACY_MATERIALIZATION_BYTES,
    },
  );
  if (mergedBranches.length > MAX_ATOMIC_MIGRATION_RECOUNT_ROWS)
    throw new Error("ESCHEMA: merged branch revision backfill exceeds its row cap");
  const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
  for (const row of mergedBranches) {
    if (performance.now() > deadline)
      throw new Error("ESCHEMA: merged branch revision backfill exceeds its time cap");
    let revision = row.revision;
    if (revision === null) {
      revision =
        tx.all<{ revision: number } & SqliteRow>(
          "SELECT revision FROM efs_revisions WHERE writer_id=? ORDER BY revision DESC LIMIT 1",
          [`branch:${row.id}`],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.revision ?? null;
    }
    if (revision === null || !Number.isSafeInteger(revision) || revision < 0)
      throw new Error(`ECORRUPT: merged branch ${row.id} has no durable revision`);
    tx.run("UPDATE efs_branches SET merged_revision=? WHERE id=? AND state=1", [
      revision,
      row.id,
    ]);
  }
  tx.run(
    `UPDATE efs_usage SET charged_metadata_bytes=${DIRECT_CHARGED_METADATA_EXPRESSION} WHERE singleton=1`,
  );
  tx.run(
    `UPDATE efs_usage SET integrity_token=${USAGE_INTEGRITY_SQL} WHERE singleton=1`,
  );
  tx.run("UPDATE efs_meta SET schema_version=12 WHERE singleton=1");
  setUserVersion(tx, identityMode, 12);
}

function migrateV12ToV13(
  tx: FilesystemSQLiteTransaction,
  identityMode: SQLiteSchemaIdentityMode,
): void {
  const state = inspect(tx, identityMode);
  if (state.applicationId !== EFS_APPLICATION_ID || state.userVersion !== 12)
    throw new Error("ESCHEMA: schema v13 migration precondition failed");
  const meta = tx.all<MetaRow>(
    "SELECT schema_version,filesystem_id,main_revision,root_inode,cow_page_bytes FROM efs_meta WHERE singleton=1",
    [],
    { maxRows: 1, maxBytes: 4096 },
  )[0];
  if (!meta || meta.schema_version !== 12)
    throw new Error("ECORRUPT: invalid schema v12 metadata");
  for (const statement of SCHEMA_V13_STATEMENTS) tx.run(statement);
  tx.run("UPDATE efs_meta SET schema_version=13 WHERE singleton=1");
  setUserVersion(tx, identityMode, 13);
}

function assertBoundedLegacyTransformBytes(tx: FilesystemSQLiteTransaction): void {
  const bytes = tx.all<{ bytes: number } & SqliteRow>(
    "SELECT (SELECT coalesce(sum(length(bytes)+256),0) FROM efs_cow_pages)+(SELECT coalesce(sum(length(insert_bytes)+512),0) FROM efs_patches) bytes",
    [],
    { maxRows: 1, maxBytes: 128 },
  )[0]?.bytes;
  if (!Number.isSafeInteger(bytes) || bytes! < 0)
    throw new Error("ECORRUPT: invalid legacy migration byte envelope");
  if (bytes! > MAX_ATOMIC_LEGACY_TRANSFORM_BYTES)
    throw new Error(
      `ESCHEMA: legacy transformed payload exceeds ${MAX_ATOMIC_LEGACY_TRANSFORM_BYTES} bytes`,
    );
}

function assertBoundedMigrationRecount(tx: FilesystemSQLiteTransaction): void {
  const existing = new Set(
    tx
      .all<{ name: string } & SqliteRow>(
        "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
        [],
        { maxRows: 128, maxBytes: 16 * 1024 },
      )
      .map((row) => row.name),
  );
  let remaining = MAX_ATOMIC_MIGRATION_RECOUNT_ROWS;
  for (const table of DIRECT_USAGE_TABLES) {
    if (!existing.has(table)) continue;
    const limit = remaining + 1;
    const rows = tx.all<{ count: number } & SqliteRow>(
      `SELECT count(*) count FROM (SELECT 1 FROM ${table} LIMIT ?)`,
      [limit],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.count;
    if (!Number.isSafeInteger(rows) || rows! < 0 || rows! > remaining)
      throw new Error(
        `ESCHEMA: v4 atomic usage recount exceeds ${MAX_ATOMIC_MIGRATION_RECOUNT_ROWS} rows`,
      );
    remaining -= rows!;
  }
}

function assertBoundedLegacyMigrationRows(tx: FilesystemSQLiteTransaction): void {
  const tables = tx.all<{ name: string } & SqliteRow>(
    "SELECT name FROM sqlite_schema WHERE type='table' AND name LIKE 'efs_%' ORDER BY name",
    [],
    { maxRows: 128, maxBytes: 16 * 1024 },
  );
  let remaining = MAX_ATOMIC_MIGRATION_RECOUNT_ROWS;
  for (const table of tables) {
    if (!/^[a-z0-9_]+$/u.test(table.name))
      throw new Error("ESCHEMA: invalid legacy table name");
    const limit = remaining + 1;
    const rows = tx.all<{ count: number } & SqliteRow>(
      `SELECT count(*) count FROM (SELECT 1 FROM ${table.name} LIMIT ?)`,
      [limit],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.count;
    if (!Number.isSafeInteger(rows) || rows! < 0 || rows! > remaining)
      throw new Error(
        `ESCHEMA: legacy migration exceeds ${MAX_ATOMIC_MIGRATION_RECOUNT_ROWS} rows`,
      );
    remaining -= rows!;
  }
}

export interface StorageMetadata {
  readonly filesystemId: string;
  readonly mainRevision: number;
  readonly rootInode: string;
  readonly cowPageBytes: CowPageBytes;
}

export function initializeOrValidateSchema(
  driver: FilesystemSQLiteDriver,
  options: {
    readonly cowPageBytes?: CowPageBytes;
    readonly now?: number;
    readonly maxManifestEntries?: number;
    readonly maxManifestDepth?: number;
    readonly maxFileBytes?: number;
    readonly maxContentObjectBytes?: number;
    readonly writerProfile?: string;
  } = {},
): StorageMetadata {
  const identityMode =
    driver.capabilities.schemaIdentityMode ?? ("sqlite-header" as const);
  const requestedPageBytes = options.cowPageBytes;
  const requestedWriterProfile = options.writerProfile ?? "";
  if (utf8ByteLength(requestedWriterProfile) > 8192)
    throw new RangeError("writerProfile exceeds the persisted schema envelope");
  const requestedManifest = Object.freeze({
    maxManifestEntries: options.maxManifestEntries ?? 0xffff_ffff,
    maxManifestDepth: options.maxManifestDepth ?? 8,
    maxFileBytes: options.maxFileBytes ?? 16 * 1024 ** 3,
    maxContentObjectBytes: options.maxContentObjectBytes ?? 16 * 1024 * 1024,
  });
  if (
    !Number.isSafeInteger(requestedManifest.maxManifestEntries) ||
    requestedManifest.maxManifestEntries < 1 ||
    requestedManifest.maxManifestEntries > 0xffff_ffff ||
    !Number.isSafeInteger(requestedManifest.maxManifestDepth) ||
    requestedManifest.maxManifestDepth < 1 ||
    requestedManifest.maxManifestDepth > 64 ||
    !Number.isSafeInteger(requestedManifest.maxFileBytes) ||
    requestedManifest.maxFileBytes < 1 ||
    !Number.isSafeInteger(requestedManifest.maxContentObjectBytes) ||
    requestedManifest.maxContentObjectBytes < 1 ||
    requestedManifest.maxContentObjectBytes > MAX_CONTENT_OBJECT_BYTES
  )
    throw new RangeError("invalid persisted manifest storage envelope");
  const state = driver.transaction("read", (tx) => inspectForOpen(tx, identityMode));
  if (state.applicationId === EFS_APPLICATION_ID) {
    if (state.userVersion === 1) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v1 requires a writable migration");
      driver.transaction("exclusive", (tx) => {
        const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
        assertBoundedLegacyMigrationRows(tx);
        assertBoundedLegacyTransformBytes(tx);
        migrateV1ToV2(tx, identityMode, deadline);
        if (performance.now() > deadline)
          throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
        migrateV2ToV3(tx, identityMode);
        migrateV3ToV4(tx, identityMode, requestedManifest, requestedWriterProfile);
        migrateV4ToV5(tx, identityMode);
      });
    }
    const afterV1 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV1.userVersion === 2) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v2 requires a writable migration");
      driver.transaction("exclusive", (tx) => {
        const deadline = performance.now() + MAX_ATOMIC_MIGRATION_MS;
        assertBoundedLegacyMigrationRows(tx);
        migrateV2ToV3(tx, identityMode);
        if (performance.now() > deadline)
          throw new Error("ESCHEMA: legacy atomic migration exceeds its time cap");
        migrateV3ToV4(tx, identityMode, requestedManifest, requestedWriterProfile);
        migrateV4ToV5(tx, identityMode);
      });
    }
    const afterV2 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV2.userVersion === 3) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v3 requires a writable migration");
      driver.transaction("exclusive", (tx) => {
        migrateV3ToV4(tx, identityMode, requestedManifest, requestedWriterProfile);
        migrateV4ToV5(tx, identityMode);
      });
    }
    const afterV3 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV3.userVersion === 4) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v4 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV4ToV5(tx, identityMode));
    }
    const afterV4 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV4.userVersion === 5) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v5 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV5ToV6(tx, identityMode));
    }
    const afterV5 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV5.userVersion === 6) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v6 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV6ToV7(tx, identityMode));
    }
    const afterV6 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV6.userVersion === 7) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v7 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV7ToV8(tx, identityMode));
    }
    const afterV7 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV7.userVersion === 8) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v8 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV8ToV9(tx, identityMode));
    }
    const afterV8 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV8.userVersion === 9) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v9 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV9ToV10(tx, identityMode));
    }
    const afterV9 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV9.userVersion === 10) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v10 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV10ToV11(tx, identityMode));
    }
    const afterV10 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV10.userVersion === 11) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v11 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV11ToV12(tx, identityMode));
    }
    const afterV11 = driver.transaction("read", (tx) => inspect(tx, identityMode));
    if (afterV11.userVersion === 12) {
      if (driver.readOnly)
        throw new Error("ESCHEMA: schema v12 requires a writable migration");
      driver.transaction("exclusive", (tx) => migrateV12ToV13(tx, identityMode));
    }
    const meta = driver.transaction("read", (tx) =>
      validateCurrent(
        tx,
        identityMode,
        requestedPageBytes,
        requestedManifest,
        requestedWriterProfile,
      ),
    );
    return Object.freeze({
      filesystemId: meta.filesystem_id,
      mainRevision: meta.main_revision,
      rootInode: meta.root_inode,
      cowPageBytes: meta.cow_page_bytes as CowPageBytes,
    });
  }
  if (state.applicationId !== 0)
    throw new Error("ESCHEMA: wrong SQLite application_id");
  if (state.objectCount !== 0 || state.userVersion !== 0)
    throw new Error("ESCHEMA: database is not an empty Ephemeral AI FS database");
  if (driver.readOnly) throw new Error("EROFS: cannot initialize a read-only database");
  const pageBytes = requestedPageBytes ?? 8192;
  const now = options.now ?? Date.now();
  const filesystemId = globalThis.crypto.randomUUID();
  const rootInode = globalThis.crypto.randomUUID();
  driver.transaction("exclusive", (tx) => {
    const recheck = inspect(tx, identityMode);
    if (
      recheck.applicationId !== 0 ||
      recheck.objectCount !== 0 ||
      recheck.userVersion !== 0
    )
      throw new Error("ESCHEMA: database changed during initialization");
    initializeIdentity(tx, identityMode);
    for (const statement of EFS_SCHEMA_V3_CREATE_STATEMENTS) tx.run(statement);
    tx.run(
      "INSERT INTO efs_revisions(revision,parent_revision,created_at_ms,writer_id,change_count) VALUES(0,NULL,?,'bootstrap',1)",
      [now],
    );
    tx.run(
      "INSERT INTO efs_meta(singleton,schema_version,filesystem_id,main_revision,root_inode,root_mutation_generation,next_allocation_sequence,cow_page_bytes,created_at_ms) VALUES(1,?,?,?,?,0,1,?,?)",
      [3, filesystemId, 0, rootInode, pageBytes, now],
    );
    tx.run("INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,256)");
    tx.run(
      "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,1,493,?,?,?,?,NULL,NULL,NULL,0)",
      [rootInode, now, now, now, 1],
    );
    tx.run(
      "INSERT INTO efs_inode_revisions(revision,inode_id,tombstone,encoded) VALUES(0,?,0,?)",
      [
        rootInode,
        utf8Json({
          id: rootInode,
          type: 1,
          mode: 493,
          birthtime_ms: now,
          mtime_ms: now,
          ctime_ms: now,
          nlink: 1,
          size: null,
          manifest_hash: null,
          symlink_target: null,
          token: 0,
        }),
      ],
    );
    setUserVersion(tx, identityMode, 3);
    migrateV3ToV4(tx, identityMode, requestedManifest, requestedWriterProfile);
    migrateV4ToV5(tx, identityMode);
    migrateV5ToV6(tx, identityMode);
    migrateV6ToV7(tx, identityMode);
    migrateV7ToV8(tx, identityMode);
    migrateV8ToV9(tx, identityMode);
    migrateV9ToV10(tx, identityMode);
    migrateV10ToV11(tx, identityMode);
    migrateV11ToV12(tx, identityMode);
    migrateV12ToV13(tx, identityMode);
    validateCurrent(
      tx,
      identityMode,
      pageBytes,
      requestedManifest,
      requestedWriterProfile,
    );
  });
  return Object.freeze({
    filesystemId,
    mainRevision: 0,
    rootInode,
    cowPageBytes: pageBytes,
  });
}

function utf8Json(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

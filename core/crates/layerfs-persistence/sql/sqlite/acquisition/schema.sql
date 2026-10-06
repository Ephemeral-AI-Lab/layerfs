CREATE TABLE init_operation (
 operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
 owner_epoch INTEGER NOT NULL CHECK(owner_epoch>0),
 phase INTEGER NOT NULL CHECK(phase BETWEEN 1 AND 6),
 source_device BLOB NOT NULL CHECK(length(source_device)=8),
 source_inode BLOB NOT NULL CHECK(length(source_inode)=8),
 stack_id BLOB NOT NULL CHECK(length(stack_id)=16),
 scope BLOB NOT NULL CHECK(length(scope)=32),
 held_rows INTEGER NOT NULL DEFAULT 0 CHECK(held_rows>=0),
 held_bytes INTEGER NOT NULL DEFAULT 0 CHECK(held_bytes>=0),
 peak_rows INTEGER NOT NULL DEFAULT 0 CHECK(peak_rows>=held_rows),
 peak_bytes INTEGER NOT NULL DEFAULT 0 CHECK(peak_bytes>=held_bytes),
 removed_rows INTEGER NOT NULL DEFAULT 0 CHECK(removed_rows>=0),
 removed_bytes INTEGER NOT NULL DEFAULT 0 CHECK(removed_bytes>=0)
) STRICT;
CREATE TABLE init_entry (
 operation_id INTEGER NOT NULL REFERENCES init_operation(operation_id),
 parent_position INTEGER NOT NULL CHECK(parent_position>=-1),
 name BLOB NOT NULL CHECK(length(name)<=255),
 position INTEGER CHECK(position>=0),
 kind INTEGER NOT NULL CHECK(kind BETWEEN 1 AND 3),
 canonical_position INTEGER CHECK(canonical_position>0),
 metadata_root BLOB NOT NULL CHECK(length(metadata_root)=32),
 content_root BLOB CHECK(length(content_root)=32),
 native_path BLOB CHECK(length(native_path) BETWEEN 1 AND 4096),
 native BLOB CHECK(length(native)=60),
 PRIMARY KEY(operation_id,parent_position,name)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX init_entry_directory ON init_entry(operation_id,position) WHERE kind=2;
CREATE TABLE init_native_file (
 operation_id INTEGER NOT NULL REFERENCES init_operation(operation_id),
 canonical_position INTEGER NOT NULL CHECK(canonical_position>0),
 device BLOB NOT NULL CHECK(length(device)=8),
 inode BLOB NOT NULL CHECK(length(inode)=8),
 evidence BLOB NOT NULL CHECK(length(evidence)=44),
 native_path BLOB NOT NULL CHECK(length(native_path) BETWEEN 1 AND 4096),
 aliases INTEGER NOT NULL DEFAULT 0 CHECK(aliases>=0),
 file_root BLOB CHECK(length(file_root)=32),
 PRIMARY KEY(operation_id,canonical_position)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX init_native_file_identity ON init_native_file(operation_id,device,inode);

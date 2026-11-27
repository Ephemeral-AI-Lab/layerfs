CREATE TABLE init_operation (
 operation_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 owner_epoch bigint NOT NULL CHECK (owner_epoch > 0),
 phase bigint NOT NULL CHECK (phase BETWEEN 1 AND 6),
 source_device bytea STORAGE PLAIN NOT NULL CHECK (octet_length(source_device) = 8),
 source_inode bytea STORAGE PLAIN NOT NULL CHECK (octet_length(source_inode) = 8),
 stack_id bytea STORAGE PLAIN NOT NULL CHECK (octet_length(stack_id) = 16),
 scope bytea STORAGE PLAIN NOT NULL CHECK (octet_length(scope) = 32),
 held_rows bigint NOT NULL DEFAULT 0 CHECK (held_rows >= 0),
 held_bytes bigint NOT NULL DEFAULT 0 CHECK (held_bytes >= 0),
 peak_rows bigint NOT NULL DEFAULT 0 CHECK (peak_rows >= held_rows),
 peak_bytes bigint NOT NULL DEFAULT 0 CHECK (peak_bytes >= held_bytes),
 removed_rows bigint NOT NULL DEFAULT 0 CHECK (removed_rows >= 0),
 removed_bytes bigint NOT NULL DEFAULT 0 CHECK (removed_bytes >= 0)
);
CREATE TABLE init_entry (
 operation_id bigint NOT NULL REFERENCES init_operation (operation_id),
 parent_position bigint NOT NULL CHECK (parent_position >= -1),
 name bytea STORAGE PLAIN NOT NULL CHECK (octet_length(name) <= 255),
 position bigint CHECK (position >= 0),
 kind bigint NOT NULL CHECK (kind BETWEEN 1 AND 3),
 canonical_position bigint CHECK (canonical_position > 0),
 metadata_root bytea STORAGE PLAIN NOT NULL CHECK (octet_length(metadata_root) = 32),
 content_root bytea STORAGE PLAIN CHECK (octet_length(content_root) = 32),
 host_path bytea STORAGE PLAIN CHECK (octet_length(host_path) BETWEEN 1 AND 4096),
 host_identity bytea STORAGE PLAIN CHECK (octet_length(host_identity) = 60),
 PRIMARY KEY (operation_id, parent_position, name)
);
CREATE UNIQUE INDEX init_entry_directory ON init_entry (operation_id, position) WHERE kind = 2;
CREATE TABLE init_host_file (
 operation_id bigint NOT NULL REFERENCES init_operation (operation_id),
 canonical_position bigint NOT NULL CHECK (canonical_position > 0),
 device bytea STORAGE PLAIN NOT NULL CHECK (octet_length(device) = 8),
 inode bytea STORAGE PLAIN NOT NULL CHECK (octet_length(inode) = 8),
 evidence bytea STORAGE PLAIN NOT NULL CHECK (octet_length(evidence) = 44),
 host_path bytea STORAGE PLAIN NOT NULL CHECK (octet_length(host_path) BETWEEN 1 AND 4096),
 aliases bigint NOT NULL DEFAULT 0 CHECK (aliases >= 0),
 file_root bytea STORAGE PLAIN CHECK (octet_length(file_root) = 32),
 PRIMARY KEY (operation_id, canonical_position)
);
CREATE UNIQUE INDEX init_host_file_identity ON init_host_file (operation_id, device, inode);

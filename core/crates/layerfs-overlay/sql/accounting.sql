-- Exact backed counts: namespace zero is the daemon aggregate.
CREATE TABLE accounting (
    ns INTEGER PRIMARY KEY CHECK(ns>=0),
    wait_refs INTEGER NOT NULL DEFAULT 0 CHECK(wait_refs>=0),
    namespaces INTEGER NOT NULL DEFAULT 0 CHECK(namespaces>=0),
    inode_rows INTEGER NOT NULL DEFAULT 0 CHECK(inode_rows>=0),
    directory_entry_rows INTEGER NOT NULL DEFAULT 0 CHECK(directory_entry_rows>=0),
    payload_cells INTEGER NOT NULL DEFAULT 0 CHECK(payload_cells>=0),
    payload_bytes INTEGER NOT NULL DEFAULT 0 CHECK(payload_bytes>=0),
    shrink_rows INTEGER NOT NULL DEFAULT 0 CHECK(shrink_rows>=0),
    operation_record_rows INTEGER NOT NULL DEFAULT 0 CHECK(operation_record_rows>=0),
    operation_record_bytes INTEGER NOT NULL DEFAULT 0 CHECK(operation_record_bytes>=0),
    orphan_rows INTEGER NOT NULL DEFAULT 0 CHECK(orphan_rows>=0),
    owner_rows INTEGER NOT NULL DEFAULT 0 CHECK(owner_rows>=0),
    source_rows INTEGER NOT NULL DEFAULT 0 CHECK(source_rows>=0),
    owner_details INTEGER NOT NULL DEFAULT 0 CHECK(owner_details>=0),
    reply_tickets INTEGER NOT NULL DEFAULT 0 CHECK(reply_tickets>=0),
    retire_targets INTEGER NOT NULL DEFAULT 0 CHECK(retire_targets>=0),
    maintenance_targets INTEGER NOT NULL DEFAULT 0 CHECK(maintenance_targets>=0),
    ready_targets INTEGER NOT NULL DEFAULT 0 CHECK(ready_targets>=0)
) STRICT;
INSERT INTO accounting(ns) VALUES(0);
CREATE TRIGGER workspace_account_insert AFTER INSERT ON workspace BEGIN
    INSERT INTO accounting(ns,namespaces) VALUES(NEW.ns,1);
    UPDATE accounting SET namespaces=namespaces+1 WHERE ns=0;
END;
CREATE TRIGGER workspace_account_delete AFTER DELETE ON workspace BEGIN
    DELETE FROM accounting WHERE ns=OLD.ns;
    UPDATE accounting SET namespaces=namespaces-1 WHERE ns=0;
END;
CREATE TRIGGER inode_account_insert AFTER INSERT ON inode BEGIN
    UPDATE accounting SET inode_rows=inode_rows+1 WHERE ns=0;
    UPDATE accounting SET inode_rows=inode_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER inode_account_delete AFTER DELETE ON inode BEGIN
    UPDATE accounting SET inode_rows=inode_rows-1 WHERE ns=0;
    UPDATE accounting SET inode_rows=inode_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER directory_entry_account_insert AFTER INSERT ON directory_entry BEGIN
    UPDATE accounting SET directory_entry_rows=directory_entry_rows+1 WHERE ns=0;
    UPDATE accounting SET directory_entry_rows=directory_entry_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER directory_entry_account_delete AFTER DELETE ON directory_entry BEGIN
    UPDATE accounting SET directory_entry_rows=directory_entry_rows-1 WHERE ns=0;
    UPDATE accounting SET directory_entry_rows=directory_entry_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER payload_account_insert AFTER INSERT ON payload BEGIN
    UPDATE accounting SET payload_cells=payload_cells+1,payload_bytes=payload_bytes+(length(NEW.data)+ifnull(length(NEW.validity),0)) WHERE ns=0;
    UPDATE accounting SET payload_cells=payload_cells+1,payload_bytes=payload_bytes+(length(NEW.data)+ifnull(length(NEW.validity),0)) WHERE ns=NEW.ns;
END;
CREATE TRIGGER payload_account_delete AFTER DELETE ON payload BEGIN
    UPDATE accounting SET payload_cells=payload_cells-1,payload_bytes=payload_bytes-(length(OLD.data)+ifnull(length(OLD.validity),0)) WHERE ns=0;
    UPDATE accounting SET payload_cells=payload_cells-1,payload_bytes=payload_bytes-(length(OLD.data)+ifnull(length(OLD.validity),0)) WHERE ns=OLD.ns;
END;
CREATE TRIGGER shrink_account_insert AFTER INSERT ON shrink BEGIN
    UPDATE accounting SET shrink_rows=shrink_rows+1 WHERE ns=0;
    UPDATE accounting SET shrink_rows=shrink_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER shrink_account_delete AFTER DELETE ON shrink BEGIN
    UPDATE accounting SET shrink_rows=shrink_rows-1 WHERE ns=0;
    UPDATE accounting SET shrink_rows=shrink_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER operation_record_account_insert AFTER INSERT ON operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=NEW.ns;
END;
CREATE TRIGGER operation_record_account_delete AFTER DELETE ON operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=OLD.ns;
END;
CREATE TRIGGER owned_operation_record_account_insert AFTER INSERT ON owned_operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=NEW.ns;
END;
CREATE TRIGGER owned_operation_record_account_delete AFTER DELETE ON owned_operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=OLD.ns;
END;
CREATE TRIGGER indexed_operation_record_account_insert AFTER INSERT ON indexed_operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows+1,operation_record_bytes=operation_record_bytes+length(NEW.value) WHERE ns=NEW.ns;
END;
CREATE TRIGGER indexed_operation_record_account_delete AFTER DELETE ON indexed_operation_record BEGIN
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=0;
    UPDATE accounting SET operation_record_rows=operation_record_rows-1,operation_record_bytes=operation_record_bytes-length(OLD.value) WHERE ns=OLD.ns;
END;
CREATE TRIGGER orphan_account_insert AFTER INSERT ON orphan BEGIN
    UPDATE accounting SET orphan_rows=orphan_rows+1 WHERE ns=0;
    UPDATE accounting SET orphan_rows=orphan_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER orphan_account_delete AFTER DELETE ON orphan BEGIN
    UPDATE accounting SET orphan_rows=orphan_rows-1 WHERE ns=0;
    UPDATE accounting SET orphan_rows=orphan_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER lease_account_insert AFTER INSERT ON lease BEGIN
    UPDATE accounting SET owner_rows=owner_rows+1 WHERE ns=0;
    UPDATE accounting SET owner_rows=owner_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER lease_account_delete AFTER DELETE ON lease BEGIN
    UPDATE accounting SET owner_rows=owner_rows-1 WHERE ns=0;
    UPDATE accounting SET owner_rows=owner_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER base_source_account_insert AFTER INSERT ON base_source BEGIN
    UPDATE accounting SET source_rows=source_rows+1 WHERE ns=0;
    UPDATE accounting SET source_rows=source_rows+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER base_source_account_delete AFTER DELETE ON base_source BEGIN
    UPDATE accounting SET source_rows=source_rows-1 WHERE ns=0;
    UPDATE accounting SET source_rows=source_rows-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER file_handle_account_insert AFTER INSERT ON file_handle BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER file_handle_account_delete AFTER DELETE ON file_handle BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER file_read_account_insert AFTER INSERT ON file_read BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER file_read_account_delete AFTER DELETE ON file_read BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER captured_reader_account_insert AFTER INSERT ON captured_reader BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER captured_reader_account_delete AFTER DELETE ON captured_reader BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER operation_owner_account_insert AFTER INSERT ON operation_owner BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER operation_owner_account_delete AFTER DELETE ON operation_owner BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER lookup_owner_account_insert AFTER INSERT ON lookup_owner BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER lookup_owner_account_delete AFTER DELETE ON lookup_owner BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER request_account_insert AFTER INSERT ON request BEGIN
    UPDATE accounting SET reply_tickets=reply_tickets+1 WHERE ns=0;
    UPDATE accounting SET reply_tickets=reply_tickets+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER request_account_delete AFTER DELETE ON request BEGIN
    UPDATE accounting SET reply_tickets=reply_tickets-1 WHERE ns=0;
    UPDATE accounting SET reply_tickets=reply_tickets-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER reclaim_account_insert AFTER INSERT ON reclaim BEGIN
    UPDATE accounting SET retire_targets=retire_targets+1 WHERE ns=0;
    UPDATE accounting SET retire_targets=retire_targets+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER reclaim_account_delete AFTER DELETE ON reclaim BEGIN
    UPDATE accounting SET retire_targets=retire_targets-1 WHERE ns=0;
    UPDATE accounting SET retire_targets=retire_targets-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER maintenance_account_insert AFTER INSERT ON maintenance BEGIN
    UPDATE accounting SET maintenance_targets=maintenance_targets+1,ready_targets=ready_targets+NEW.ready WHERE ns=0;
    UPDATE accounting SET maintenance_targets=maintenance_targets+1,ready_targets=ready_targets+NEW.ready WHERE ns=NEW.ns;
END;
CREATE TRIGGER maintenance_account_delete AFTER DELETE ON maintenance BEGIN
    UPDATE accounting SET maintenance_targets=maintenance_targets-1,ready_targets=ready_targets-OLD.ready WHERE ns=0;
    UPDATE accounting SET maintenance_targets=maintenance_targets-1,ready_targets=ready_targets-OLD.ready WHERE ns=OLD.ns;
END;
CREATE TRIGGER payload_account_update AFTER UPDATE ON payload BEGIN
    UPDATE accounting SET payload_bytes=payload_bytes-(length(OLD.data)+ifnull(length(OLD.validity),0))+(length(NEW.data)+ifnull(length(NEW.validity),0)) WHERE ns=0;
    UPDATE accounting SET payload_bytes=payload_bytes-(length(OLD.data)+ifnull(length(OLD.validity),0))+(length(NEW.data)+ifnull(length(NEW.validity),0)) WHERE ns=NEW.ns;
END;
CREATE TRIGGER operation_record_account_update AFTER UPDATE ON operation_record BEGIN
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-(length(OLD.value))+(length(NEW.value)) WHERE ns=0;
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-(length(OLD.value))+(length(NEW.value)) WHERE ns=NEW.ns;
END;
CREATE TRIGGER owned_operation_record_account_update AFTER UPDATE ON owned_operation_record BEGIN
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-(length(OLD.value))+(length(NEW.value)) WHERE ns=0;
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-(length(OLD.value))+(length(NEW.value)) WHERE ns=NEW.ns;
END;
CREATE TRIGGER indexed_operation_record_account_update AFTER UPDATE ON indexed_operation_record BEGIN
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-length(OLD.value)+length(NEW.value) WHERE ns=0;
    UPDATE accounting SET operation_record_bytes=operation_record_bytes-length(OLD.value)+length(NEW.value) WHERE ns=NEW.ns;
END;
CREATE TRIGGER maintenance_account_update AFTER UPDATE ON maintenance BEGIN
    UPDATE accounting SET ready_targets=ready_targets-(OLD.ready)+(NEW.ready) WHERE ns=0;
    UPDATE accounting SET ready_targets=ready_targets-(OLD.ready)+(NEW.ready) WHERE ns=NEW.ns;
END;
CREATE TRIGGER orphan_wait_account_insert AFTER INSERT ON orphan_wait BEGIN
    UPDATE accounting SET wait_refs=wait_refs+1 WHERE ns=0;
    UPDATE accounting SET wait_refs=wait_refs+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER orphan_wait_account_delete AFTER DELETE ON orphan_wait BEGIN
    UPDATE accounting SET wait_refs=wait_refs-1 WHERE ns=0;
    UPDATE accounting SET wait_refs=wait_refs-1 WHERE ns=OLD.ns;
END;
CREATE TRIGGER file_custody_account_insert AFTER INSERT ON file_custody BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER file_custody_account_delete AFTER DELETE ON file_custody BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;

CREATE TRIGGER native_mount_account_insert AFTER INSERT ON native_mount BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER native_mount_account_delete AFTER DELETE ON native_mount BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;

CREATE TRIGGER native_lookup_account_insert AFTER INSERT ON native_lookup BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER native_lookup_account_delete AFTER DELETE ON native_lookup BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;

CREATE TRIGGER native_source_account_insert AFTER INSERT ON native_source BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER native_source_account_delete AFTER DELETE ON native_source BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;

CREATE TRIGGER native_read_account_insert AFTER INSERT ON native_read BEGIN
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details+1 WHERE ns=NEW.ns;
END;
CREATE TRIGGER native_read_account_delete AFTER DELETE ON native_read BEGIN
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=0;
    UPDATE accounting SET owner_details=owner_details-1 WHERE ns=OLD.ns;
END;

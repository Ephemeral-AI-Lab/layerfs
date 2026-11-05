CREATE TABLE change_state (
 id INTEGER PRIMARY KEY CHECK(id=1),
 state BLOB NOT NULL CHECK(length(state)=32),
 seq INTEGER NOT NULL CHECK(seq>=0)
) STRICT;
CREATE TABLE change_log (
 seq INTEGER PRIMARY KEY CHECK(seq>0),
 tbl INTEGER NOT NULL CHECK(tbl BETWEEN 0 AND 19),
 k1 ANY,
 k2 ANY,
 k3 ANY,
 CHECK((tbl=0)=(k1 IS NULL))
) STRICT;
CREATE TRIGGER change_pack_insert AFTER INSERT ON pack WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(1,new.pack_id); END;
CREATE TRIGGER change_object_location_insert AFTER INSERT ON object_location WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(3,new.object_id); END;
CREATE TRIGGER change_object_location_update AFTER UPDATE ON object_location WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(3,new.object_id);
 INSERT INTO change_log(tbl,k1) SELECT 3,old.object_id WHERE old.object_id IS NOT new.object_id; END;
CREATE TRIGGER change_object_location_delete AFTER DELETE ON object_location WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(3,old.object_id); END;
CREATE TRIGGER change_inode_value_insert AFTER INSERT ON inode_value WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(4,new.ordinal); END;
CREATE TRIGGER change_inode_value_update AFTER UPDATE ON inode_value WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(4,new.ordinal);
 INSERT INTO change_log(tbl,k1) SELECT 4,old.ordinal WHERE old.ordinal IS NOT new.ordinal; END;
CREATE TRIGGER change_inode_value_delete AFTER DELETE ON inode_value WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(4,old.ordinal); END;
CREATE TRIGGER change_content_signature_insert AFTER INSERT ON content_signature WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(5,new.slot); END;
CREATE TRIGGER change_content_signature_update AFTER UPDATE ON content_signature WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(5,new.slot);
 INSERT INTO change_log(tbl,k1) SELECT 5,old.slot WHERE old.slot IS NOT new.slot; END;
CREATE TRIGGER change_content_signature_delete AFTER DELETE ON content_signature WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(5,old.slot); END;
CREATE TRIGGER change_store_policy_insert AFTER INSERT ON store_policy WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(6,new.id); END;
CREATE TRIGGER change_store_policy_update AFTER UPDATE ON store_policy WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(6,new.id);
 INSERT INTO change_log(tbl,k1) SELECT 6,old.id WHERE old.id IS NOT new.id; END;
CREATE TRIGGER change_store_policy_delete AFTER DELETE ON store_policy WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(6,old.id); END;
CREATE TRIGGER change_store_totals_insert AFTER INSERT ON store_totals WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(7,new.id); END;
CREATE TRIGGER change_store_totals_update AFTER UPDATE ON store_totals WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(7,new.id);
 INSERT INTO change_log(tbl,k1) SELECT 7,old.id WHERE old.id IS NOT new.id; END;
CREATE TRIGGER change_store_totals_delete AFTER DELETE ON store_totals WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(7,old.id); END;
CREATE TRIGGER change_history_meta_insert AFTER INSERT ON history_meta WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(8,new.id); END;
CREATE TRIGGER change_history_meta_update AFTER UPDATE ON history_meta WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(8,new.id);
 INSERT INTO change_log(tbl,k1) SELECT 8,old.id WHERE old.id IS NOT new.id; END;
CREATE TRIGGER change_history_meta_delete AFTER DELETE ON history_meta WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(8,old.id); END;
CREATE TRIGGER change_layer_stack_insert AFTER INSERT ON layer_stack WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(9,new.layer_stack_id); END;
CREATE TRIGGER change_layer_stack_update AFTER UPDATE ON layer_stack WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(9,new.layer_stack_id);
 INSERT INTO change_log(tbl,k1) SELECT 9,old.layer_stack_id WHERE old.layer_stack_id IS NOT new.layer_stack_id; END;
CREATE TRIGGER change_layer_stack_delete AFTER DELETE ON layer_stack WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(9,old.layer_stack_id); END;
CREATE TRIGGER change_layer_insert AFTER INSERT ON layer WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(10,new.layer_id); END;
CREATE TRIGGER change_layer_update AFTER UPDATE ON layer WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(10,new.layer_id);
 INSERT INTO change_log(tbl,k1) SELECT 10,old.layer_id WHERE old.layer_id IS NOT new.layer_id; END;
CREATE TRIGGER change_layer_delete AFTER DELETE ON layer WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(10,old.layer_id); END;
CREATE TRIGGER change_commit_insert AFTER INSERT ON "commit" WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(11,new.commit_id); END;
CREATE TRIGGER change_commit_update AFTER UPDATE ON "commit" WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(11,new.commit_id);
 INSERT INTO change_log(tbl,k1) SELECT 11,old.commit_id WHERE old.commit_id IS NOT new.commit_id; END;
CREATE TRIGGER change_commit_delete AFTER DELETE ON "commit" WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(11,old.commit_id); END;
CREATE TRIGGER change_branch_insert AFTER INSERT ON branch WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(12,new.branch_id); END;
CREATE TRIGGER change_branch_update AFTER UPDATE ON branch WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(12,new.branch_id);
 INSERT INTO change_log(tbl,k1) SELECT 12,old.branch_id WHERE old.branch_id IS NOT new.branch_id; END;
CREATE TRIGGER change_branch_delete AFTER DELETE ON branch WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(12,old.branch_id); END;
CREATE TRIGGER change_scope_allocator_insert AFTER INSERT ON scope_allocator WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(13,new.scope_id); END;
CREATE TRIGGER change_scope_allocator_update AFTER UPDATE ON scope_allocator WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(13,new.scope_id);
 INSERT INTO change_log(tbl,k1) SELECT 13,old.scope_id WHERE old.scope_id IS NOT new.scope_id; END;
CREATE TRIGGER change_scope_allocator_delete AFTER DELETE ON scope_allocator WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(13,old.scope_id); END;
CREATE TRIGGER change_binding_hint_insert AFTER INSERT ON binding_hint WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2,k3) VALUES(14,new.child,new.parent,new.name); END;
CREATE TRIGGER change_binding_hint_update AFTER UPDATE ON binding_hint WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2,k3) VALUES(14,new.child,new.parent,new.name);
 INSERT INTO change_log(tbl,k1,k2,k3) SELECT 14,old.child,old.parent,old.name WHERE old.child IS NOT new.child OR old.parent IS NOT new.parent OR old.name IS NOT new.name; END;
CREATE TRIGGER change_binding_hint_delete AFTER DELETE ON binding_hint WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2,k3) VALUES(14,old.child,old.parent,old.name); END;
CREATE TRIGGER change_layer_lineage_insert AFTER INSERT ON layer_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(15,new.layer_id); END;
CREATE TRIGGER change_layer_lineage_update AFTER UPDATE ON layer_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(15,new.layer_id);
 INSERT INTO change_log(tbl,k1) SELECT 15,old.layer_id WHERE old.layer_id IS NOT new.layer_id; END;
CREATE TRIGGER change_layer_lineage_delete AFTER DELETE ON layer_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(15,old.layer_id); END;
CREATE TRIGGER change_commit_lineage_insert AFTER INSERT ON commit_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(16,new.commit_id); END;
CREATE TRIGGER change_commit_lineage_update AFTER UPDATE ON commit_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(16,new.commit_id);
 INSERT INTO change_log(tbl,k1) SELECT 16,old.commit_id WHERE old.commit_id IS NOT new.commit_id; END;
CREATE TRIGGER change_commit_lineage_delete AFTER DELETE ON commit_lineage WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(16,old.commit_id); END;
CREATE TRIGGER change_absorbed_insert AFTER INSERT ON absorbed WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(17,new.run,new.depth); END;
CREATE TRIGGER change_absorbed_update AFTER UPDATE ON absorbed WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(17,new.run,new.depth);
 INSERT INTO change_log(tbl,k1,k2) SELECT 17,old.run,old.depth WHERE old.run IS NOT new.run OR old.depth IS NOT new.depth; END;
CREATE TRIGGER change_absorbed_delete AFTER DELETE ON absorbed WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(17,old.run,old.depth); END;
CREATE TRIGGER change_reach_insert AFTER INSERT ON reach WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(18,new.key,new.x); END;
CREATE TRIGGER change_reach_update AFTER UPDATE ON reach WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(18,new.key,new.x);
 INSERT INTO change_log(tbl,k1,k2) SELECT 18,old.key,old.x WHERE old.key IS NOT new.key OR old.x IS NOT new.x; END;
CREATE TRIGGER change_reach_delete AFTER DELETE ON reach WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(18,old.key,old.x); END;
CREATE TRIGGER change_file_digest_insert AFTER INSERT ON file_digest WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(19,new.digest,new.length); END;
CREATE TRIGGER change_file_digest_update AFTER UPDATE ON file_digest WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(19,new.digest,new.length);
 INSERT INTO change_log(tbl,k1,k2) SELECT 19,old.digest,old.length WHERE old.digest IS NOT new.digest OR old.length IS NOT new.length; END;
CREATE TRIGGER change_file_digest_delete AFTER DELETE ON file_digest WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1,k2) VALUES(19,old.digest,old.length); END;

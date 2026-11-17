CREATE TRIGGER change_pack_unit_insert AFTER INSERT ON pack_unit WHEN EXISTS(SELECT 1 FROM change_state) BEGIN
 INSERT INTO change_log(tbl,k1) VALUES(2,new.unit_id); END;

INSERT INTO objects(object_id,object_role,canonical_length,pack_id,group_number,record_number,save_id) VALUES (?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))

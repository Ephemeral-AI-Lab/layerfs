INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) VALUES (?,?,?,?,?,?) ON CONFLICT(object_id) DO NOTHING RETURNING object_id

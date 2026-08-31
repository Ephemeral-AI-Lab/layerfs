-- family: objects
-- name: insert
-- parameters: ?1 ObjectId, ?2 canonical bytes
-- affected rows: one inserted or zero on conflict
INSERT INTO objects(object_id, bytes)
VALUES (?1, ?2)
ON CONFLICT(object_id) DO NOTHING;

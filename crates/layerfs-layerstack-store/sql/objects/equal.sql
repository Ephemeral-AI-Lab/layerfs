-- family: objects
-- name: equal
-- parameters: ?1 ObjectId, ?2 canonical bytes
-- results: one row iff stored bytes are equal
SELECT 1 FROM objects WHERE object_id = ?1 AND bytes = ?2;

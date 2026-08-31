-- family: objects
-- name: get
-- parameters: ?1 ObjectId
-- results: bytes
SELECT bytes FROM objects WHERE object_id = ?1;

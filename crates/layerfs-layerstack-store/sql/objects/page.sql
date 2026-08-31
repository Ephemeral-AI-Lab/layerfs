-- family: objects
-- name: page
-- parameters: ?1 exclusive ObjectId cursor, ?2 limit
-- results: object_id, encoded_length in keyset order
SELECT object_id, length(bytes)
FROM objects
WHERE object_id > ?1
ORDER BY object_id
LIMIT ?2;

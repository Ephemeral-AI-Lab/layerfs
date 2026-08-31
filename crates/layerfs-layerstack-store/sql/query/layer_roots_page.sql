-- family: query
-- name: layer_roots_page
-- parameters: ?1 exclusive LayerId cursor, ?2 limit
-- results: layer_id, root_id in keyset order
SELECT layer_id, root_id
FROM layers
WHERE layer_id > ?1
ORDER BY layer_id
LIMIT ?2;

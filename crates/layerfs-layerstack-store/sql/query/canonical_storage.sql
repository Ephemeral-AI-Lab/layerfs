-- family: query
-- name: canonical_storage
-- parameters: none
-- results: physical object count and encoded bytes
SELECT count(*), COALESCE(sum(length(bytes)), 0) FROM objects;

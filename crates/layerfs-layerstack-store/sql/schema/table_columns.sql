-- family: schema
-- name: table_columns
-- parameters: ?1 table name
-- results: cid, name, type, not_null, default_value, primary_key
SELECT cid, name, type, "notnull", dflt_value, pk
FROM pragma_table_info(?1)
ORDER BY cid;

-- family: schema
-- name: schema_objects
-- parameters: none
-- results: type, name, table_name, sql
SELECT type, name, tbl_name, sql
FROM sqlite_schema
WHERE name NOT LIKE 'sqlite_%'
ORDER BY type, name;

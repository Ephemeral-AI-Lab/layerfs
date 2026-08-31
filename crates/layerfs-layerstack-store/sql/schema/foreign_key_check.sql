-- family: schema
-- name: foreign_key_check
-- parameters: none
-- results: violating foreign-key rows
SELECT "table", rowid, parent, fkid
FROM pragma_foreign_key_check;

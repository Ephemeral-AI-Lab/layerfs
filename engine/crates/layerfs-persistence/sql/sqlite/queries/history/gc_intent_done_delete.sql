-- Delete exactly one row the operator exported: `state = 'done'` keeps a live
-- intent out of the archive's prune even if its op id were named, and
-- `RETURNING` proves whether the row was still there. Portable to PostgreSQL.
DELETE FROM gc_intent
WHERE state = 'done' AND op_id = $1
RETURNING op_id;

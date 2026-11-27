-- SQLite's single `BEGIN IMMEDIATE` writer already excludes another marker
-- transaction, so the removal only needs the existence check; the epoch bump
-- in the same transaction is the serialization.
SELECT epoch FROM project_epoch WHERE layer_stack_id = $1;

-- One intent row whatever its state, `done` included: the owner's own
-- release reads it to unlock the PostgreSQL session keys it took at
-- registration even when an operator already retained the row as `done`.
-- It reads; the release never deletes or revives a `done` row.
SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase,
       evidence
FROM gc_intent
WHERE op_id = $1;

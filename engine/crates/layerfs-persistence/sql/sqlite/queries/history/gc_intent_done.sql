-- The retained `done` audit rows, oldest first. Live admission queries
-- filter `state <> 'done'`; this is the only read that includes them.
SELECT op_id, kind, scope_kind, scope_id, project, branch, machine, sandbox, phase, evidence, updated_ms
FROM gc_intent
WHERE state = 'done'
ORDER BY updated_ms, op_id
LIMIT $1 OFFSET $2;

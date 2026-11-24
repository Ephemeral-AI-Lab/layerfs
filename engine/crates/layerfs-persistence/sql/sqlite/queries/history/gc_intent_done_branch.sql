-- The retained `done` audit rows of one Branch, oldest first. The operator
-- audit names the project and Branch it reads, so one Branch's retained rows
-- never leak into another's page, and a removed Branch's rows stay readable
-- by the names they recorded. Live admission queries filter `state <> 'done'`.
SELECT op_id, kind, scope_kind, scope_id, project, branch, machine, sandbox, phase, evidence, updated_ms
FROM gc_intent
WHERE state = 'done' AND project = $1 AND branch = $2
ORDER BY updated_ms, op_id
LIMIT $3 OFFSET $4;

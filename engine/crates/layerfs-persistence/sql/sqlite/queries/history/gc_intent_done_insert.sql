-- The durable operator preview row: a `done` row written without a live
-- intent to release. The owner and place columns stay NULL and `evidence`
-- holds the audit text (never a `place:v1` value), so the unique owner key
-- never blocks a later mount and admission, which filters `state <> 'done'`,
-- never sees it. `gc_intent_done` is the bounded read that does. Portable to
-- PostgreSQL, as the other intent statements are.
INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase, evidence, updated_ms)
VALUES ($1, $2, $3, $4, NULL, NULL, NULL, $5, $6, NULL, NULL, 'done', FALSE, $7, $8, $9)
RETURNING op_id;

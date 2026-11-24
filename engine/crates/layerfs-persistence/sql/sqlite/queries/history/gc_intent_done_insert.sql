-- The durable operator preview row: a `done` row written without a live
-- intent to release. The owner columns and every place column stay NULL, so
-- the unique owner key never blocks a later mount and admission, which
-- filters `state <> 'done'`, never sees it. `gc_intent_done` is the bounded
-- read that does. Portable to PostgreSQL, as the other intent statements are.
INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase, machine_incarnation, sandbox_generation, evidence, updated_ms)
VALUES ($1, $2, $3, $4, NULL, NULL, NULL, $5, $6, NULL, NULL, 'done', FALSE, $7, NULL, NULL, $8, $9)
RETURNING op_id;

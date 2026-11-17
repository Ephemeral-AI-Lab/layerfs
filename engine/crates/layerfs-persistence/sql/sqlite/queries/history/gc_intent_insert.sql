-- A retry reuses the operation id. It must also replace the ownership and
-- placement evidence of its predecessor: a row that kept a dead process's
-- owner token would let a concurrent GC call settle the retried, live
-- operation as dead instead of reporting it `Busy` (§5.1).
INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase, generation, artifact, high_water, evidence, updated_ms)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, ($13 <> 0), $14, NULL, NULL, NULL, NULL, $15)
ON CONFLICT(op_id) DO UPDATE SET
    state = excluded.state,
    uncertain = excluded.uncertain,
    phase = excluded.phase,
    workspace = excluded.workspace,
    sandbox = excluded.sandbox,
    machine = excluded.machine,
    owner_runtime = excluded.owner_runtime,
    owner_boot = excluded.owner_boot,
    updated_ms = excluded.updated_ms
RETURNING op_id;

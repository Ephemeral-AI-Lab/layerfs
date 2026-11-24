-- A retry reuses the operation id. It must also replace the ownership,
-- placement and authority evidence of its predecessor: a row that kept a
-- dead process's owner token would let a concurrent GC call settle the
-- retried, live operation as dead instead of reporting it `Busy` (§5.1).
INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase, machine_incarnation, sandbox_generation, lease_expires_ms, fence_token, generation, artifact, high_water, evidence, updated_ms)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, ($13 <> 0), $14, $15, $16, $17, $18, NULL, NULL, NULL, NULL, $19)
ON CONFLICT(op_id) DO UPDATE SET
    state = excluded.state,
    uncertain = excluded.uncertain,
    phase = excluded.phase,
    workspace = excluded.workspace,
    sandbox = excluded.sandbox,
    machine = excluded.machine,
    owner_runtime = excluded.owner_runtime,
    owner_boot = excluded.owner_boot,
    machine_incarnation = excluded.machine_incarnation,
    sandbox_generation = excluded.sandbox_generation,
    lease_expires_ms = excluded.lease_expires_ms,
    fence_token = excluded.fence_token,
    updated_ms = excluded.updated_ms
RETURNING op_id;

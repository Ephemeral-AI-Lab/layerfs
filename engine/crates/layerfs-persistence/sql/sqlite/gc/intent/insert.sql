-- A retry reuses the operation id. It must also replace the ownership,
-- placement and place-token evidence of its predecessor: a row that kept a
-- dead process's owner token would let a concurrent GC call settle the
-- retried, live operation as dead instead of reporting it `Busy` (§5.1).
-- `evidence` carries a live row's place tokens (`place:v1;...`). A retained
-- `done` row is never revived while it is retained: the conflict branch
-- skips it, so no row is returned and the registration is refused. Once the
-- Branch's cap (`gc_intent_done_cap`) has deleted that done row, its op id
-- has no row at all and can register again as a new live intent.
INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase, generation, artifact, high_water, evidence, updated_ms)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, ($13 <> 0), $14, NULL, NULL, NULL, $15, $16)
ON CONFLICT(op_id) DO UPDATE SET
    state = excluded.state,
    uncertain = excluded.uncertain,
    phase = excluded.phase,
    workspace = excluded.workspace,
    sandbox = excluded.sandbox,
    machine = excluded.machine,
    owner_runtime = excluded.owner_runtime,
    owner_boot = excluded.owner_boot,
    evidence = excluded.evidence,
    updated_ms = excluded.updated_ms
WHERE gc_intent.state <> 'done'
RETURNING op_id;

SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase,
       machine_incarnation, sandbox_generation, lease_expires_ms, fence_token
FROM gc_intent
WHERE state <> 'done'
  AND scope_kind = $1 AND scope_id = $2
ORDER BY updated_ms, op_id;

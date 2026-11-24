SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase,
       evidence
FROM gc_intent
WHERE state <> 'done'
  AND op_id <> $1;

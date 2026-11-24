SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase,
       evidence
FROM gc_intent
WHERE state <> 'done'
  AND (
      scope_kind = 'store'
      OR (scope_kind = 'project' AND scope_id = $1)
      OR (scope_kind = 'branch' AND scope_id = $2)
  )
  AND op_id <> $3;

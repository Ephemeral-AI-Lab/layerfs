SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase
FROM gc_intent
WHERE state <> 'done'
  AND kind IN ('removal', 'compaction')
  AND (
      scope_kind = 'store'
      OR ($1 IS NOT NULL AND scope_kind = 'project' AND scope_id = $1)
      OR (scope_kind = 'branch' AND scope_id = $2)
  )
  AND op_id <> $3;

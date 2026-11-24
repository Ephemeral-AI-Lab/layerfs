SELECT op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project, branch,
       owner_runtime, owner_boot, state, uncertain, phase,
       machine_incarnation, sandbox_generation
FROM gc_intent
WHERE state <> 'done'
  AND kind IN ('init', 'merge', 'removal', 'compaction')
  AND (
      scope_kind = 'store'
      OR (scope_kind = 'project' AND scope_id = $1)
      OR (scope_kind = 'branch' AND (
          (project = $2)
          OR scope_id IN (SELECT branch_id FROM branch WHERE layer_stack_id = $1)
      ))
  )
  AND op_id <> $3;

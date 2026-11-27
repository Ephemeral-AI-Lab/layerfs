SELECT 0, s.layer_stack_id, s.name, s.scope_id, s.profile_id, s.head_layer_id,
  NULL, NULL, NULL, NULL, NULL, NULL,
  c.commit_id, c.layer_stack_id, c.root_id, c.parent_commit_id, c.base_layer_id, c.second_parent_id, c.branch_id, c.via, c.message, c.merged_from_id, c.created_ms, c.logical, c.new_canonical, c.new_stored, c.new_metadata, c.lww, c.displaced_id, c.attempt,
  y.layer_id, y.layer_stack_id, y.parent_layer_id, y.root_id, y.source_branch_id, y.source_commit_id, y.logical, y.overwrote_count,
  COALESCE(c.history_serial, y.history_serial),
  a.branch_id, a.layer_stack_id, a.name, a.base_layer_id, a.head_commit_id, a.forked_from_id,
  NULL, NULL, NULL
FROM {s}.layer_stack s
LEFT JOIN {s}."commit" c ON c.commit_id = $2
LEFT JOIN (SELECT v.layer_id, v.layer_stack_id, v.parent_layer_id, v.root_id, v.source_branch_id, v.source_commit_id, v.logical, v.history_serial, l.overwrote_count FROM {s}.layer v JOIN {s}.layer_lineage l ON l.layer_id = v.layer_id) y ON y.layer_id = $3
LEFT JOIN {s}.branch a ON a.branch_id = c.branch_id
WHERE s.name = $1 AND NOT EXISTS (SELECT 1 FROM {s}.layer_stack_retired d WHERE d.layer_stack_id = s.layer_stack_id)
UNION ALL
SELECT 1,
  NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
  NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
  NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
  NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
  m.path, m.frontier_serial, m.removed_ms
FROM {s}.layer_stack s JOIN {s}.path_removed m ON m.layer_stack_id = s.layer_stack_id
WHERE s.name = $1 AND NOT EXISTS (SELECT 1 FROM {s}.layer_stack_retired d WHERE d.layer_stack_id = s.layer_stack_id)
ORDER BY 1, 46;

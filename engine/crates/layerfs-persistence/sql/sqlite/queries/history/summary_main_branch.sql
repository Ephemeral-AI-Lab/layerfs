SELECT s.layer_stack_id, b.head_commit_id
FROM layer_stack AS s
LEFT JOIN branch AS b INDEXED BY branch_names
  ON b.layer_stack_id = s.layer_stack_id AND b.name = 'main'
WHERE s.name = $1;

SELECT b.name, CASE WHEN r.branch_id IS NULL THEN 0 ELSE 1 END FROM branch b LEFT JOIN branch_retired r ON r.branch_id = b.branch_id WHERE b.layer_stack_id = $1 AND b.name = $2;

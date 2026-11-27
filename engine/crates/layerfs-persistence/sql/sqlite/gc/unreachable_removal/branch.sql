DELETE FROM branch
 WHERE layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM kept_node k
                    WHERE k.kind = 'branch' AND k.node_id = branch.branch_id);

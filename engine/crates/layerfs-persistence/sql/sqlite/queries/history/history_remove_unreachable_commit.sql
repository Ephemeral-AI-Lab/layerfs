DELETE FROM "commit"
 WHERE layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM kept_node k
                    WHERE k.kind = 'commit' AND k.node_id = "commit".commit_id);

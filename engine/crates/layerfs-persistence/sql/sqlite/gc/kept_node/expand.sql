WITH RECURSIVE
n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i+1<?2),
closure(layer_stack_id,kind,node_id,serial) AS (
    SELECT substr(?1,i*50+1,17),
           CASE substr(?1,i*50+18,1)
                WHEN X'12' THEN 'commit' WHEN X'32' THEN 'layer'
                WHEN X'11' THEN 'branch' ELSE 'stack' END,
           substr(?1,i*50+18,CASE WHEN substr(?1,i*50+18,1) IN (X'12',X'32') THEN 33 ELSE 17 END),
           -1
      FROM n
    UNION
    SELECT t.layer_stack_id,'commit',t.commit_id,t.history_serial
      FROM closure k
     CROSS JOIN branch b ON b.branch_id=k.node_id
     CROSS JOIN "commit" t ON t.commit_id IN (b.head_commit_id,b.forked_from_id)
     WHERE k.kind='branch'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='commit' AND x.node_id=t.commit_id)
    UNION
    SELECT t.layer_stack_id,'layer',t.layer_id,t.history_serial
      FROM closure k
     CROSS JOIN branch b ON b.branch_id=k.node_id
     CROSS JOIN layer t ON t.layer_id IN (b.base_layer_id,b.forked_from_id)
     WHERE k.kind='branch'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='layer' AND x.node_id=t.layer_id)
    UNION
    SELECT t.layer_stack_id,'layer',t.layer_id,t.history_serial
      FROM closure k
     CROSS JOIN layer l ON l.layer_id=k.node_id
     CROSS JOIN layer t ON t.layer_id=l.parent_layer_id
     WHERE k.kind='layer'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='layer' AND x.node_id=t.layer_id)
    UNION
    SELECT t.layer_stack_id,'commit',t.commit_id,t.history_serial
      FROM closure k
     CROSS JOIN layer l ON l.layer_id=k.node_id
     CROSS JOIN "commit" t ON t.commit_id=l.source_commit_id
     WHERE k.kind='layer'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='commit' AND x.node_id=t.commit_id)
    UNION
    SELECT t.layer_stack_id,'branch',t.branch_id,0
      FROM closure k
     CROSS JOIN layer l ON l.layer_id=k.node_id
     CROSS JOIN branch t ON t.branch_id=l.source_branch_id
     WHERE k.kind='layer'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='branch' AND x.node_id=t.branch_id)
    UNION
    SELECT t.layer_stack_id,'commit',t.commit_id,t.history_serial
      FROM closure k
     CROSS JOIN "commit" c ON c.commit_id=k.node_id
     CROSS JOIN "commit" t ON t.commit_id IN
           (c.parent_commit_id,c.second_parent_id,c.merged_from_id,c.displaced_id)
     WHERE k.kind='commit'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='commit' AND x.node_id=t.commit_id)
    UNION
    SELECT t.layer_stack_id,'layer',t.layer_id,t.history_serial
      FROM closure k
     CROSS JOIN "commit" c ON c.commit_id=k.node_id
     CROSS JOIN layer t ON t.layer_id IN
           (c.second_parent_id,c.base_layer_id,c.merged_from_id,c.displaced_id)
     WHERE k.kind='commit'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='layer' AND x.node_id=t.layer_id)
    UNION
    SELECT t.layer_stack_id,'branch',t.branch_id,0
      FROM closure k
     CROSS JOIN "commit" c ON c.commit_id=k.node_id
     CROSS JOIN branch t ON t.branch_id=c.branch_id
     WHERE k.kind='commit'
       AND NOT EXISTS (SELECT 1 FROM kept_node x WHERE x.layer_stack_id=t.layer_stack_id
                          AND x.kind='branch' AND x.node_id=t.branch_id)
    LIMIT ?3
)
INSERT INTO kept_node(layer_stack_id,kind,node_id,serial)
SELECT layer_stack_id,kind,node_id,serial FROM closure WHERE serial>=0
ON CONFLICT DO NOTHING
RETURNING layer_stack_id,node_id

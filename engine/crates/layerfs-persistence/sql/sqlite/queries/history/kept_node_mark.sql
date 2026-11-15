WITH RECURSIVE
roots(kind,node_id) AS (
    SELECT 'stack',s.layer_stack_id FROM layer_stack s
     WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id)
    UNION
    SELECT 'layer',l.layer_id FROM layer l
     WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=l.layer_stack_id)
    UNION
    SELECT 'branch',b.branch_id FROM branch b
     WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id)
       AND NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=b.layer_stack_id)
),
edges(from_kind,from_id,to_kind,to_id) AS (
    SELECT 'branch',b.branch_id,'commit',b.head_commit_id FROM branch b
      JOIN "commit" c ON c.commit_id=b.head_commit_id
    UNION ALL
    SELECT 'branch',b.branch_id,'layer',b.base_layer_id FROM branch b
    UNION ALL
    SELECT 'branch',b.branch_id,'commit',b.forked_from_id FROM branch b
      JOIN "commit" c ON c.commit_id=b.forked_from_id
    UNION ALL
    SELECT 'branch',b.branch_id,'layer',b.forked_from_id FROM branch b
      JOIN layer l ON l.layer_id=b.forked_from_id
    UNION ALL
    SELECT 'layer',l.layer_id,'layer',l.parent_layer_id FROM layer l
     WHERE l.parent_layer_id IS NOT NULL
    UNION ALL
    SELECT 'layer',l.layer_id,'commit',l.source_commit_id FROM layer l
     WHERE l.source_commit_id IS NOT NULL
    UNION ALL
    SELECT 'layer',l.layer_id,'branch',l.source_branch_id FROM layer l
     WHERE l.source_branch_id IS NOT NULL
    UNION ALL
    SELECT 'commit',c.commit_id,'commit',c.parent_commit_id FROM "commit" c
     WHERE c.parent_commit_id IS NOT NULL
    UNION ALL
    SELECT 'commit',c.commit_id,'commit',c.second_parent_id FROM "commit" c
      JOIN "commit" s ON s.commit_id=c.second_parent_id
    UNION ALL
    SELECT 'commit',c.commit_id,'layer',c.second_parent_id FROM "commit" c
      JOIN layer l ON l.layer_id=c.second_parent_id
    UNION ALL
    SELECT 'commit',c.commit_id,'layer',c.base_layer_id FROM "commit" c
    UNION ALL
    SELECT 'commit',c.commit_id,'branch',c.branch_id FROM "commit" c
     WHERE c.branch_id IS NOT NULL
    UNION ALL
    SELECT 'commit',c.commit_id,'commit',c.merged_from_id FROM "commit" c
      JOIN "commit" s ON s.commit_id=c.merged_from_id
    UNION ALL
    SELECT 'commit',c.commit_id,'layer',c.merged_from_id FROM "commit" c
      JOIN layer l ON l.layer_id=c.merged_from_id
    UNION ALL
    SELECT 'commit',c.commit_id,'commit',c.displaced_id FROM "commit" c
      JOIN "commit" s ON s.commit_id=c.displaced_id
    UNION ALL
    SELECT 'commit',c.commit_id,'layer',c.displaced_id FROM "commit" c
      JOIN layer l ON l.layer_id=c.displaced_id
),
closure(kind,node_id) AS (
    SELECT kind,node_id FROM roots
    UNION
    SELECT e.to_kind,e.to_id
      FROM closure k JOIN edges e ON e.from_kind=k.kind AND e.from_id=k.node_id
)
INSERT INTO kept_node(layer_stack_id,kind,node_id,serial)
SELECT s.layer_stack_id,'stack',s.layer_stack_id,0
  FROM closure k JOIN layer_stack s ON s.layer_stack_id=k.node_id
 WHERE k.kind='stack'
UNION ALL
SELECT b.layer_stack_id,'branch',b.branch_id,0
  FROM closure k JOIN branch b ON b.branch_id=k.node_id
 WHERE k.kind='branch'
UNION ALL
SELECT l.layer_stack_id,'layer',l.layer_id,l.history_serial
  FROM closure k JOIN layer l ON l.layer_id=k.node_id
 WHERE k.kind='layer'
UNION ALL
SELECT c.layer_stack_id,'commit',c.commit_id,c.history_serial
  FROM closure k JOIN "commit" c ON c.commit_id=k.node_id
 WHERE k.kind='commit'

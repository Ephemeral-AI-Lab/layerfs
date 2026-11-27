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
closure(kind,node_id) AS (
    SELECT kind,node_id FROM roots
    UNION
    SELECT e.to_kind,e.to_id
      FROM closure k
     CROSS JOIN LATERAL (
        SELECT t.to_kind,t.to_id FROM branch b
         CROSS JOIN LATERAL (VALUES
            ('commit',b.head_commit_id),
            ('layer',b.base_layer_id),
            ('commit',b.forked_from_id),
            ('layer',b.forked_from_id)) t(to_kind,to_id)
         WHERE k.kind='branch' AND b.branch_id=k.node_id
        UNION ALL
        SELECT t.to_kind,t.to_id FROM layer l
         CROSS JOIN LATERAL (VALUES
            ('layer',l.parent_layer_id),
            ('commit',l.source_commit_id),
            ('branch',l.source_branch_id)) t(to_kind,to_id)
         WHERE k.kind='layer' AND l.layer_id=k.node_id
        UNION ALL
        SELECT t.to_kind,t.to_id FROM "commit" c
         CROSS JOIN LATERAL (VALUES
            ('commit',c.parent_commit_id),
            ('commit',c.second_parent_id),
            ('layer',c.second_parent_id),
            ('layer',c.base_layer_id),
            ('branch',c.branch_id),
            ('commit',c.merged_from_id),
            ('layer',c.merged_from_id),
            ('commit',c.displaced_id),
            ('layer',c.displaced_id)) t(to_kind,to_id)
         WHERE k.kind='commit' AND c.commit_id=k.node_id
     ) e
     WHERE e.to_id IS NOT NULL
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

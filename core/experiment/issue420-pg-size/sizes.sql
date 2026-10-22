-- Per-relation sizes for tables matching :'pat' (main fork, fsm, vm, toast, indexes).
SELECT c.relname, c.reltuples::bigint AS rows,
  pg_relation_size(c.oid) AS heap, pg_relation_size(c.oid,'fsm')+pg_relation_size(c.oid,'vm') AS fsm_vm,
  coalesce(pg_total_relation_size(nullif(c.reltoastrelid,0)),0) AS toast,
  pg_indexes_size(c.oid) AS indexes, pg_total_relation_size(c.oid) AS total
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace AND n.nspname='public'
WHERE c.relkind='r' AND c.relname ~ :'pat' ORDER BY c.relname;

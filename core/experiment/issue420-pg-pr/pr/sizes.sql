-- Per-table bytes (heap, toast, indexes, total), live/dead tuples, and the database total.
SELECT c.relname, s.n_live_tup live, s.n_dead_tup dead, pg_relation_size(c.oid) heap,
  coalesce(pg_total_relation_size(nullif(c.reltoastrelid, 0)), 0) toast, pg_indexes_size(c.oid) indexes,
  pg_total_relation_size(c.oid) total
FROM pg_class c JOIN pg_stat_user_tables s ON s.relid = c.oid ORDER BY total DESC;
SELECT current_database() db, pg_database_size(current_database()) bytes, round(pg_database_size(current_database()) / 1e6, 1) mb;

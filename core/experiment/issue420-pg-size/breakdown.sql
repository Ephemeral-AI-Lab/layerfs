-- Byte anatomy of one heap relation and its indexes (pageinspect, pgstattuple).
-- Usage: psql -v rel=<table> -v cols='<sum of pg_column_size(...) expression>'
\set ON_ERROR_STOP 1
SELECT :'rel' AS rel,
       pg_relation_size(:'rel') AS main_fork,
       pg_relation_size(:'rel','fsm') AS fsm, pg_relation_size(:'rel','vm') AS vm,
       pg_relation_size(:'rel')/8192 AS pages;
WITH it AS (
  SELECT lp_len, t_hoff
  FROM generate_series(0, pg_relation_size(:'rel')/8192 - 1) b,
       LATERAL heap_page_items(get_raw_page(:'rel', b::int)) h
  WHERE lp_flags = 1)
SELECT count(*) AS tuples,
       4*count(*) AS line_pointers,
       sum(t_hoff) AS tuple_headers,
       sum(lp_len - t_hoff) AS tuple_data,
       sum(((lp_len + 7)/8)*8 - lp_len) AS tail_align_pad,
       24*(pg_relation_size(:'rel')/8192) AS page_headers
FROM it;
SELECT sum(:cols) AS column_datums FROM :rel;
SELECT tuple_count, tuple_len, dead_tuple_count, free_space, free_percent FROM pgstattuple(:'rel');
SELECT c.relname AS index, pg_relation_size(c.oid) AS bytes, s.*
FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid,
     LATERAL pgstatindex(c.oid) s
WHERE i.indrelid = :'rel'::regclass;

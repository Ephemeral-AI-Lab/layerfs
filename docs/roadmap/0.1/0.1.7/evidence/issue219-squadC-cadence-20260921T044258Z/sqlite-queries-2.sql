-- Squad C (#219) cadence evidence, query set 2 (corrected: `values` is reserved in
-- SQLite and the first query set's alias for it failed with "near \"values\": syntax error").
-- Run against a byte COPY of the diagnostic store:
--   cp <run>/pipeline-namespace-10000/sample.sqlite /tmp/layerfs-squadC-cadence-20260921T044258Z/
-- The copy is never written by these statements (all read-only).
.headers on
.mode list

-- 1. Store geometry, the fields the task states, re-read here.
SELECT 'objects_total' AS q, COUNT(*) AS n FROM objects;
SELECT 'objects_by_save' AS q, save_id, COUNT(*) AS n, SUM(canonical_length) AS bytes FROM objects GROUP BY save_id;
SELECT 'roles' AS q, object_role, COUNT(*) AS n, SUM(canonical_length) AS bytes FROM objects GROUP BY object_role ORDER BY object_role;
SELECT 'packs_total' AS q, COUNT(*) AS n, SUM(LENGTH(data)) AS bytes FROM object_packs;
SELECT 'value_groups_total' AS q, COUNT(*) AS n FROM metadata_value_groups;
SELECT 'saves_total' AS q, COUNT(*) AS n FROM saves;
SELECT 'saves_rows' AS q, save_id, active_slot, publication, pack_ceiling FROM saves ORDER BY save_id;
SELECT 'sqlite_sequence' AS q, name, seq FROM sqlite_sequence;
SELECT 'store_policy' AS q, max_concurrent_writes, publication_sequence, retained_pack_ceiling, next_pack_id, next_ordinal, metadata_window_start, metadata_window_values, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, metadata_delta_max_depth FROM store_policy;
SELECT 'page_count' AS q, (SELECT * FROM pragma_page_count()) AS pages, (SELECT * FROM pragma_page_size()) AS page_bytes, (SELECT * FROM pragma_freelist_count()) AS freelist;

-- 2. One group = one seal = one commit. Uniqueness and density of (pack_id, group_number, record_number).
SELECT 'groups_in_objects' AS q, COUNT(*) AS n FROM (SELECT DISTINCT pack_id, group_number FROM objects);
SELECT 'dup_pack_group_record' AS q, COUNT(*) AS n FROM (SELECT pack_id, group_number, record_number, COUNT(*) AS c FROM objects GROUP BY pack_id, group_number, record_number HAVING c > 1);
SELECT 'groups_nondense_record_numbers' AS q, COUNT(*) AS n FROM (SELECT pack_id, group_number, COUNT(*) AS c, MAX(record_number) AS mx FROM objects GROUP BY pack_id, group_number HAVING mx != c - 1);
SELECT 'records_per_group_hist' AS q, records, COUNT(*) AS n_groups FROM (SELECT pack_id, group_number, COUNT(*) AS records FROM objects GROUP BY pack_id, group_number) GROUP BY records ORDER BY records;

-- 3. Per-role lane picture: whole-file records are one record per group (their own lane),
--    chunks pair up in the native lane, the rest share the ordinary lane.
SELECT 'role_lane' AS q, object_role, COUNT(*) AS objects, SUM(canonical_length) AS canonical_bytes,
       (SELECT COUNT(*) FROM (SELECT DISTINCT pack_id, group_number FROM objects o2 WHERE o2.object_role = o.object_role)) AS n_groups,
       COUNT(DISTINCT pack_id) AS packs
FROM objects o GROUP BY object_role ORDER BY object_role;

-- 4. The candidate ring: every insertion is a whole-file FULL winner; the stamp is the
--    ring's own insertion counter, so max(stamp) is that population exactly.
SELECT 'content_signatures' AS q, MIN(stamp) AS min_stamp, MAX(stamp) AS max_stamp, COUNT(*) AS rows_held, COUNT(DISTINCT stamp) AS distinct_stamps FROM content_signatures;
SELECT 'content_signatures_by_save' AS q, save_id, COUNT(*) AS n, MIN(stamp) AS lo, MAX(stamp) AS hi FROM content_signatures GROUP BY save_id;

-- 5. Packs that hold no object rows at all (the pooled value-group lane's packs).
SELECT 'packs_without_object_rows' AS q, COUNT(*) AS n FROM object_packs p WHERE NOT EXISTS (SELECT 1 FROM objects o WHERE o.pack_id = p.pack_id);
SELECT 'packs_with_value_groups' AS q, COUNT(DISTINCT pack_id) AS n FROM metadata_value_groups;

-- Squad C (#219) cadence evidence: read-only queries against a COPY of the
-- diagnostic store, taken from
-- core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/
--   ns17-squadA-profile-20260921T044041Z/pipeline-namespace-10000/sample.sqlite
-- The copy is byte-identical (cp) and is never written by these statements.
.headers on
.mode list
SELECT 'objects_total' AS q, COUNT(*) AS v FROM objects;
SELECT 'roles' AS q, object_role, COUNT(*) AS n FROM objects GROUP BY object_role ORDER BY object_role;
SELECT 'distinct_object_groups' AS q, COUNT(*) AS v FROM (SELECT DISTINCT pack_id, group_number FROM objects);
SELECT 'packs_total' AS q, COUNT(*) AS v FROM object_packs;
SELECT 'packs_by_save' AS q, save_id, COUNT(*) AS packs, SUM(LENGTH(data)) AS bytes FROM object_packs GROUP BY save_id;
SELECT 'value_groups_total' AS q, COUNT(*) AS v FROM metadata_value_groups;
SELECT 'value_group_ordinals' AS q, MIN(first_ordinal) AS lo, MAX(first_ordinal + count) AS hi, SUM(count) AS values FROM metadata_value_groups;
SELECT 'saves_total' AS q, COUNT(*) AS v FROM saves;
SELECT 'saves_rows' AS q, save_id, active_slot, publication, pack_ceiling FROM saves ORDER BY save_id;
SELECT 'sqlite_sequence' AS q, name, seq FROM sqlite_sequence;
SELECT 'store_policy' AS q, max_concurrent_writes, publication_sequence, retained_pack_ceiling, next_pack_id, next_ordinal, metadata_window_start, metadata_window_values, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, metadata_delta_max_depth FROM store_policy;
SELECT 'content_signatures' AS q, COUNT(*) AS v FROM content_signatures;
SELECT 'page_count' AS q, (SELECT * FROM pragma_page_count()) AS pages, (SELECT * FROM pragma_page_size()) AS page_bytes;

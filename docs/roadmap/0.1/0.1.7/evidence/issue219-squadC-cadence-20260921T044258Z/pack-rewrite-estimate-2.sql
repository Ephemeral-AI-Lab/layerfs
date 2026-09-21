-- Squad C (#219) pack-append amplification, CORRECTED. The first file
-- (pack-rewrite-estimate.sql) put the WITH clause in a separate statement; sqlite3
-- runs statements independently, so the CTE was not visible to the SELECTs and the
-- last two failed with "no such table: per_pack". This file repeats the CTE in each
-- statement and is the one whose output (pack-rewrite-estimate-2.txt) the README cites.
-- Method: every seal places exactly one group and issues exactly one pack write
-- (cas/placement.rs:111-200 -> pack/placement.rs:75-154 -> cas/placement.rs:202-245),
-- and the write binds the WHOLE assembled pack body (sqlite/write.rs:79-89). So a pack
-- of final length L holding k groups is written k times, and under equal increments
-- the bytes handed to the pager are L*(k+1)/2. DERIVED, not measured: the store keeps
-- only the final pack length, so the intermediate lengths cannot be read back.
.headers on
.mode list
WITH og AS (
  SELECT pack_id, group_number,
         CASE WHEN object_role = 1 THEN 'WholeFile'
              WHEN object_role = 2 THEN 'Native'
              ELSE 'Ordinary' END AS lane
  FROM objects
),
ogc AS (
  SELECT pack_id, lane, COUNT(*) AS k
  FROM (SELECT DISTINCT pack_id, group_number, lane FROM og) GROUP BY pack_id, lane
),
vg AS (
  SELECT pack_id, 'PooledMetadata' AS lane, COUNT(*) AS k
  FROM (SELECT DISTINCT pack_id, group_number FROM metadata_value_groups) GROUP BY pack_id
),
per_pack AS (
  SELECT p.pack_id, LENGTH(p.data) AS len,
         COALESCE(ogc.lane, vg.lane) AS lane,
         COALESCE(ogc.k, 0) + COALESCE(vg.k, 0) AS k
  FROM object_packs p
  LEFT JOIN ogc ON ogc.pack_id = p.pack_id
  LEFT JOIN vg  ON vg.pack_id  = p.pack_id
)
SELECT lane, COUNT(*) AS packs, SUM(k) AS writes, SUM(len) AS final_bytes,
       CAST(SUM(len * (k + 1) / 2.0) AS INTEGER) AS equal_growth_bytes,
       ROUND(SUM(len * (k + 1) / 2.0) / SUM(len), 3) AS amplification
FROM per_pack GROUP BY lane
UNION ALL
SELECT 'TOTAL', COUNT(*), SUM(k), SUM(len),
       CAST(SUM(len * (k + 1) / 2.0) AS INTEGER),
       ROUND(SUM(len * (k + 1) / 2.0) / SUM(len), 3)
FROM per_pack;
WITH og AS (
  SELECT pack_id, group_number,
         CASE WHEN object_role = 1 THEN 'WholeFile'
              WHEN object_role = 2 THEN 'Native'
              ELSE 'Ordinary' END AS lane
  FROM objects
),
ogc AS (
  SELECT pack_id, lane, COUNT(*) AS k
  FROM (SELECT DISTINCT pack_id, group_number, lane FROM og) GROUP BY pack_id, lane
),
vg AS (
  SELECT pack_id, 'PooledMetadata' AS lane, COUNT(*) AS k
  FROM (SELECT DISTINCT pack_id, group_number FROM metadata_value_groups) GROUP BY pack_id
),
per_pack AS (
  SELECT p.pack_id, LENGTH(p.data) AS len,
         COALESCE(ogc.lane, vg.lane) AS lane,
         COALESCE(ogc.k, 0) + COALESCE(vg.k, 0) AS k
  FROM object_packs p
  LEFT JOIN ogc ON ogc.pack_id = p.pack_id
  LEFT JOIN vg  ON vg.pack_id  = p.pack_id
)
SELECT 'writes_per_pack' AS q, k AS writes, COUNT(*) AS packs, SUM(len) AS final_bytes,
       CAST(SUM(len * (k + 1) / 2.0) AS INTEGER) AS equal_growth_bytes
FROM per_pack GROUP BY k ORDER BY k;

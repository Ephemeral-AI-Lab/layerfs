#!/usr/bin/env python3
"""Emit the EXPLAIN QUERY PLAN script for the Squad B save-path statement table.

Every statement below is quoted from the source at the file:line named beside it.
Nothing here writes to the originals: the databases live in /tmp/squadB (copies).
"""
import sys

ph128 = ",".join("?" * 128) if False else ",".join("?" for _ in range(128))
ph1 = "?"
ph109 = ",".join("?" for _ in range(109))

def row128():
    row = "(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))"
    return ",".join([row] * 128)

def row109():
    row = "(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))"
    return ",".join([row] * 109)

def row1():
    return "(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))"

E = []   # (label, source, sql, binds)
E.append(("L1-candidates-page128", "sqlite/lookup.rs:76-82 (LOOKUP_PAGE_IDS=128)",
  "SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,o.record_number,o.save_id,"
  "(o.save_id = r.save_id OR s.publication <= r.publication) "
  "FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r "
  "WHERE o.object_id IN (%s) AND o.pack_id <= ?%d" % (ph128, 129),
  ["x'00'"]*128 + ["1250"]))
E.append(("L1b-candidates-page1", "sqlite/lookup.rs:76-82 (page width 1)",
  "SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,o.record_number,o.save_id,"
  "(o.save_id = r.save_id OR s.publication <= r.publication) "
  "FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r "
  "WHERE o.object_id IN (?) AND o.pack_id <= ?2",
  ["x'00'", "1250"]))
E.append(("L2-pack-bytes", "sqlite/lookup.rs:161",
  "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r "
  "WHERE p.pack_id = ?1 AND length(p.data) BETWEEN 32 AND ?2 AND (p.save_id = r.save_id OR s.publication <= r.publication)",
  ["1", "16781312"]))
E.append(("L3-highest-pack-id", "sqlite/lookup.rs:174",
  "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs", []))
E.append(("W2-insert-pack", "sqlite/write.rs:70",
  "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, (SELECT save_id FROM temp.layerfs_read_scope))",
  ["1", "x'00'"]))
E.append(("W3-append-pack", "sqlite/write.rs:82",
  "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = (SELECT save_id FROM temp.layerfs_read_scope) "
  "AND EXISTS (SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id AND publication IS NULL)",
  ["1", "x'00'"]))
E.append(("W4a-insert-objects-1row", "sqlite/write.rs:171-189 (chunk=1)",
  "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES " + row1(), []))
E.append(("W4b-insert-objects-109rows", "sqlite/write.rs:171-189 (largest group in sample.sqlite)",
  "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES " + row109(), []))
E.append(("W4c-insert-objects-128rows", "sqlite/write.rs:171-189 (OBJECT_INSERT_CHUNK_CAP=128)",
  "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES " + row128(), []))
E.append(("O2-scope-update", "sqlite/ownership.rs:77",
  "UPDATE temp.layerfs_read_scope SET save_id = ?1, publication = ?2", ["2", "2"]))
E.append(("O3-publication", "sqlite/ownership.rs:88",
  "SELECT publication_sequence FROM store_policy WHERE id = 1", []))
E.append(("O4-live-owners", "sqlite/ownership.rs:107",
  "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL", []))
E.append(("O5-slot-alloc", "sqlite/ownership.rs:116-120",
  "WITH RECURSIVE candidate(slot) AS ( SELECT 1 UNION ALL SELECT slot + 1 FROM candidate WHERE slot < ?1 ) "
  "SELECT MIN(slot) FROM candidate WHERE NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = candidate.slot)", ["2"]))
E.append(("O6-insert-save", "sqlite/ownership.rs:131",
  "INSERT INTO saves (active_slot) VALUES (?1)", ["1"]))
E.append(("O7-publish-policy", "sqlite/ownership.rs:140-142",
  "UPDATE store_policy SET publication_sequence = publication_sequence + 1, "
  "retained_pack_ceiling = MAX(retained_pack_ceiling, (SELECT pack_ceiling FROM saves WHERE save_id = ?1)) "
  "WHERE id = 1 AND publication_sequence < 9223372036854775807", ["2"]))
E.append(("O8-publish-save", "sqlite/ownership.rs:148-150",
  "UPDATE saves SET active_slot = NULL, publication = (SELECT publication_sequence FROM store_policy WHERE id = 1) "
  "WHERE save_id = ?1 AND active_slot IS NOT NULL AND publication IS NULL", ["2"]))
E.append(("O9-next-pack", "sqlite/ownership.rs:161",
  "SELECT next_pack_id FROM store_policy WHERE id = 1", []))
E.append(("O10-advance-pack", "sqlite/ownership.rs:169",
  "UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1", ["1251"]))
E.append(("O11-next-ordinal", "sqlite/ownership.rs:180",
  "SELECT next_ordinal FROM store_policy WHERE id = 1", []))
E.append(("O12-reserve-ordinals", "sqlite/ownership.rs:189-192",
  "UPDATE store_policy SET next_ordinal = ?1, "
  "metadata_window_start = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?3 ELSE metadata_window_start END, "
  "metadata_window_values = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?2 ELSE metadata_window_values + ?2 END "
  "WHERE id = 1", ["10164", "49", "10115"]))
E.append(("P1-ordinal-end", "sqlite/pool.rs:46-47",
  "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups "
  "WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2", ["1", "165"]))
E.append(("P2-insert-group", "sqlite/pool.rs:75-76",
  "INSERT INTO metadata_value_groups (first_ordinal, count, pack_id, group_number, digest) VALUES (?1, ?2, ?3, ?4, ?5)",
  ["10115", "49", "1248", "0", "x'00'"]))
E.append(("P3-group-for", "sqlite/pool.rs:97-100",
  "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest "
  "FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r "
  "WHERE g.first_ordinal = (SELECT MAX(first_ordinal) FROM metadata_value_groups WHERE first_ordinal <= ?1) "
  "AND (p.save_id = r.save_id OR s.publication <= r.publication)", ["10115"]))
E.append(("P4-for-each-group", "sqlite/pool.rs:160-162",
  "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest "
  "FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r "
  "WHERE g.first_ordinal >= ?1 AND (p.save_id = r.save_id OR s.publication <= r.publication) ORDER BY g.first_ordinal", ["1"]))
E.append(("P5-group-count", "sqlite/pool.rs:174",
  "SELECT COUNT(*) FROM metadata_value_groups", []))
E.append(("P6-window-start", "sqlite/pool.rs:183",
  "SELECT metadata_window_start FROM store_policy WHERE id = 1", []))
E.append(("S1-index-present", "sqlite/schema.rs:135",
  "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1 AND type = 'index'", ["packs_save"]))
E.append(("S2-unexpected-table", "sqlite/schema.rs:144-147",
  "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN "
  "('store_policy','object_packs','metadata_value_groups','objects','content_signatures','saves')", []))
E.append(("S3-table-sql", "sqlite/schema.rs:203 and :221",
  "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1", ["objects"]))
E.append(("S4-table-info", "sqlite/schema.rs:235",
  "PRAGMA table_info(objects)", []))
E.append(("S5-load-policy", "sqlite/schema.rs:252-255",
  "SELECT format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, "
  "metadata_delta_max_depth FROM store_policy WHERE id = ?1", ["1"]))
E.append(("S6-max-concurrent-writes", "sqlite/schema.rs:301",
  "SELECT max_concurrent_writes FROM store_policy WHERE id = 1", []))
E.append(("S7-retained-pack-ceiling", "sqlite/schema.rs:338",
  "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1", []))
E.append(("S9-insert-policy-row", "sqlite/schema.rs:105-109",
  "INSERT INTO store_policy (id, format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, "
  "chunk_delta_max_depth, metadata_delta_max_depth, max_concurrent_writes, publication_sequence) "
  "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", []))
E.append(("C1-pack-ceiling-update", "cas/placement.rs:238",
  "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) WHERE save_id = ?1 AND active_slot IS NOT NULL", ["2", "1"]))
E.append(("C2-signature-flush", "encoding/delta/candidates.rs:363-364",
  "INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature, save_id) "
  "VALUES (?1, ?2, ?3, ?4, (SELECT save_id FROM temp.layerfs_read_scope))", ["1", "1", "x'00'", "x'00'"]))
E.append(("C3-signature-read", "encoding/delta/candidates.rs:284",
  "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r "
  "WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp", []))
E.append(("X1-cleanup-objects", "sqlite/cleanup.rs:42 (failure path)",
  "DELETE FROM objects WHERE save_id=?1 AND object_id IN (SELECT object_id FROM objects WHERE save_id=?1 ORDER BY object_id LIMIT ?2)",
  ["2", "128"]))
E.append(("X2-cleanup-signatures", "sqlite/cleanup.rs:46 (failure path)",
  "DELETE FROM content_signatures WHERE slot IN (SELECT slot FROM content_signatures WHERE save_id=?1 ORDER BY slot LIMIT ?2)",
  ["2", "128"]))
E.append(("X3-cleanup-packs", "sqlite/cleanup.rs:52 (failure path)",
  "DELETE FROM object_packs WHERE pack_id=(SELECT pack_id FROM object_packs WHERE save_id=?1 ORDER BY pack_id LIMIT 1)", ["2"]))
E.append(("X4-cleanup-save", "sqlite/cleanup.rs:71 (failure path)",
  "DELETE FROM saves WHERE save_id=?1 AND active_slot IS NOT NULL", ["2"]))
E.append(("X5-schema-count-objects", "sqlite/schema.rs:354 (create path only)",
  "SELECT COUNT(*) FROM objects", []))

out = []
out.append("-- EXPLAIN QUERY PLAN, Squad B save-path statement table.")
out.append("-- Generated by eqp.py; run against COPIES in /tmp/squadB, mode=ro.")
for label, src, sql, binds in E:
    out.append("SELECT '===== %s | %s =====';" % (label, src))
    out.append("EXPLAIN QUERY PLAN %s;" % sql)
    out.append("")
print("\n".join(out))

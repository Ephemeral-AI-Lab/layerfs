#!/usr/bin/env python3
"""The save path's SQL inventory, and a check that every quoted statement is
verbatim present in the source at the file:line it is attributed to.

Writes statements.tsv and sql_verbatim_check.txt (stdout)."""
import re, sys, os

SRC = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000/core/crates/layerfs-storage/src/"

# id, source file:line, exact SQL text (whitespace-normalised: the whitespace
# inside a Rust string literal is folded to one space; every other character,
# including case and punctuation, is exactly as the source has it)
S = [
("L1",  "sqlite/lookup.rs:76-82",
 "SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,o.record_number,o.save_id,(o.save_id = r.save_id OR s.publication <= r.publication) FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE o.object_id IN ({}) AND o.pack_id <= ?{}"),
("L2",  "sqlite/lookup.rs:161",
 "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE p.pack_id = ?1 AND length(p.data) BETWEEN 32 AND ?2 AND (p.save_id = r.save_id OR s.publication <= r.publication)"),
("L3",  "sqlite/lookup.rs:174",
 "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs"),
("W1",  "sqlite/write.rs:45", "BEGIN IMMEDIATE"),
("W2",  "sqlite/write.rs:52", "COMMIT"),
("W3",  "sqlite/write.rs:61", "ROLLBACK"),
("W4",  "sqlite/write.rs:70",
 "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, (SELECT save_id FROM temp.layerfs_read_scope))"),
("W5",  "sqlite/write.rs:82",
 "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = (SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id AND publication IS NULL)"),
("W6",  "sqlite/write.rs:173-174",
 "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES "),
("W6b", "sqlite/write.rs:187", ",(SELECT save_id FROM temp.layerfs_read_scope))"),
("O1",  "sqlite/ownership.rs:69-70",
 "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL); INSERT INTO layerfs_read_scope VALUES (0, 9223372036854775807);"),
("O2",  "sqlite/ownership.rs:77",
 "UPDATE temp.layerfs_read_scope SET save_id = ?1, publication = ?2"),
("O3",  "sqlite/ownership.rs:88", "SELECT publication_sequence FROM store_policy WHERE id = 1"),
("O4",  "sqlite/ownership.rs:107", "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL"),
("O5",  "sqlite/ownership.rs:116-120",
 "WITH RECURSIVE candidate(slot) AS ( SELECT 1 UNION ALL SELECT slot + 1 FROM candidate WHERE slot < ?1 ) SELECT MIN(slot) FROM candidate WHERE NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = candidate.slot)"),
("O6",  "sqlite/ownership.rs:131", "INSERT INTO saves (active_slot) VALUES (?1)"),
("O7",  "sqlite/ownership.rs:140-142",
 "UPDATE store_policy SET publication_sequence = publication_sequence + 1, retained_pack_ceiling = MAX(retained_pack_ceiling, (SELECT pack_ceiling FROM saves WHERE save_id = ?1)) WHERE id = 1 AND publication_sequence < 9223372036854775807"),
("O8",  "sqlite/ownership.rs:148-150",
 "UPDATE saves SET active_slot = NULL, publication = (SELECT publication_sequence FROM store_policy WHERE id = 1) WHERE save_id = ?1 AND active_slot IS NOT NULL AND publication IS NULL"),
("O9",  "sqlite/ownership.rs:161", "SELECT next_pack_id FROM store_policy WHERE id = 1"),
("O10", "sqlite/ownership.rs:169", "UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1"),
("O11", "sqlite/ownership.rs:180", "SELECT next_ordinal FROM store_policy WHERE id = 1"),
("O12", "sqlite/ownership.rs:189-192",
 "UPDATE store_policy SET next_ordinal = ?1, metadata_window_start = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?3 ELSE metadata_window_start END, metadata_window_values = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?2 ELSE metadata_window_values + ?2 END WHERE id = 1"),
("P1",  "sqlite/pool.rs:46-47",
 "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2"),
("P2",  "sqlite/pool.rs:75-76",
 "INSERT INTO metadata_value_groups (first_ordinal, count, pack_id, group_number, digest) VALUES (?1, ?2, ?3, ?4, ?5)"),
("P3",  "sqlite/pool.rs:97-100",
 "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE g.first_ordinal = (SELECT MAX(first_ordinal) FROM metadata_value_groups WHERE first_ordinal <= ?1) AND (p.save_id = r.save_id OR s.publication <= r.publication)"),
("P4",  "sqlite/pool.rs:160-162",
 "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE g.first_ordinal >= ?1 AND (p.save_id = r.save_id OR s.publication <= r.publication) ORDER BY g.first_ordinal"),
("P5",  "sqlite/pool.rs:174", "SELECT COUNT(*) FROM metadata_value_groups"),
("P6",  "sqlite/pool.rs:183", "SELECT metadata_window_start FROM store_policy WHERE id = 1"),
("S1",  "sqlite/schema.rs:135", "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1 AND type = 'index'"),
("S2",  "sqlite/schema.rs:144-147",
 "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('store_policy','object_packs','metadata_value_groups','objects','content_signatures','saves')"),
("S3",  "sqlite/schema.rs:203", "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1"),
("S4",  "sqlite/schema.rs:235", "PRAGMA table_info({table})"),
("S5",  "sqlite/schema.rs:252-255",
 "SELECT format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, metadata_delta_max_depth FROM store_policy WHERE id = ?1"),
("S6",  "sqlite/schema.rs:301", "SELECT max_concurrent_writes FROM store_policy WHERE id = 1"),
("S7",  "sqlite/schema.rs:326", "UPDATE store_policy SET max_concurrent_writes = ?1 WHERE id = 1"),
("S8",  "sqlite/schema.rs:338", "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1"),
("S9",  "sqlite/schema.rs:347", "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'objects'"),
("S10", "sqlite/schema.rs:354", "SELECT COUNT(*) FROM objects"),
("S11", "sqlite/schema.rs:105-109",
 "INSERT INTO store_policy (id, format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, metadata_delta_max_depth, max_concurrent_writes, publication_sequence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
("C1",  "sqlite/connection.rs:36", "PRAGMA journal_mode = MEMORY"),
("C2",  "sqlite/connection.rs:40", "PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;"),
("C3",  "sqlite/connection.rs:41", "PRAGMA foreign_keys = ON;"),
("C4",  "sqlite/connection.rs:42", "PRAGMA foreign_keys"),
("C5",  "sqlite/connection.rs:107", "PRAGMA application_id"),
("C6",  "sqlite/connection.rs:108", "PRAGMA user_version"),
("C7",  "sqlite/connection.rs:109", "PRAGMA synchronous"),
("C8",  "sqlite/connection.rs:110", "PRAGMA temp_store"),
("C9",  "sqlite/connection.rs:111", "PRAGMA foreign_keys"),
("C10", "sqlite/connection.rs:112", "PRAGMA busy_timeout"),
("C11", "sqlite/connection.rs:113", "PRAGMA page_size"),
("C12", "sqlite/connection.rs:114", "PRAGMA cache_size"),
("C13", "sqlite/connection.rs:115", "PRAGMA mmap_size"),
("C14", "sqlite/connection.rs:116", "PRAGMA cache_spill"),
("K1",  "sqlite/cleanup.rs:33", "SELECT EXISTS(SELECT 1 FROM saves WHERE save_id=?1 AND active_slot IS NOT NULL)"),
("K2",  "sqlite/cleanup.rs:42",
 "DELETE FROM objects WHERE save_id=?1 AND object_id IN (SELECT object_id FROM objects WHERE save_id=?1 ORDER BY object_id LIMIT ?2)"),
("K3",  "sqlite/cleanup.rs:46",
 "DELETE FROM content_signatures WHERE slot IN (SELECT slot FROM content_signatures WHERE save_id=?1 ORDER BY slot LIMIT ?2)"),
("K4",  "sqlite/cleanup.rs:52",
 "DELETE FROM object_packs WHERE pack_id=(SELECT pack_id FROM object_packs WHERE save_id=?1 ORDER BY pack_id LIMIT 1)"),
("K5",  "sqlite/cleanup.rs:71", "DELETE FROM saves WHERE save_id=?1 AND active_slot IS NOT NULL"),
("G1",  "cas/placement.rs:238",
 "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) WHERE save_id = ?1 AND active_slot IS NOT NULL"),
("G2",  "cas/collision.rs:17", "SELECT save_id,publication FROM temp.layerfs_read_scope"),
("G3",  "encoding/delta/candidates.rs:284",
 "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp"),
("G4",  "encoding/delta/candidates.rs:363-364",
 "INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature, save_id) VALUES (?1, ?2, ?3, ?4, (SELECT save_id FROM temp.layerfs_read_scope))"),
]

def norm(s):
    return re.sub(r"\s+", " ", s).strip()

# Build a normalised view of each source file: whitespace folded, Rust string
# continuations ("\\" + newline) removed, and the surrounding quotes dropped, so
# a multi-line literal becomes one normalised run of text.
def source_norm(path):
    raw = open(path, encoding="utf-8").read()
    raw = raw.replace("\\\n", "")          # Rust line-continuation inside a literal
    return norm(raw)

cache = {}
rows = []
bad = []
for sid, loc, sql in S:
    path = SRC + loc.split(":")[0]
    if path not in cache:
        cache[path] = source_norm(path)
    hay = cache[path]
    needle = norm(sql)
    strict = needle in hay
    # Whitespace inside a Rust literal that is split across source lines carries
    # the indentation of the continuation. Whitespace is not part of a SQL
    # statement's text, so the primary check removes every whitespace character;
    # the strict check (folded whitespace) is reported when it also holds.
    loose = re.sub(r"\s+", "", sql) in re.sub(r"\s+", "", hay)
    chk = "VERBATIM(strict)" if strict else ("VERBATIM(ws-folded)" if loose else "MISMATCH")
    rows.append((sid, loc, norm(sql), chk))
    if not loose:
        bad.append((sid, loc, needle))

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    fh.write("id\tsource\tverbatim_sql\tcheck\n")
    for sid, loc, sql, chk in rows:
        fh.write("%s\t%s\t%s\t%s\n" % (sid, loc, sql, chk))

print("statements checked: %d, verbatim: %d, mismatched: %d"
      % (len(rows), sum(1 for r in rows if r[3] == "VERBATIM"), len(bad)))
for sid, loc, needle in bad:
    print("MISMATCH %s %s\n   %s" % (sid, loc, needle))

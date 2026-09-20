#!/usr/bin/env python3
"""D1 -- the declared profile's own plan and bytecode for every statement one step issues.

Read against a copy of the run's own Store with the product's declared pragma profile,
the temp read-scope table the product creates, and foreign_keys = ON (which the
product's configure sets and the CLI does not). The SQL is copied verbatim from the
call site named beside it; nothing here is a paraphrase.

    python3 scratch/plan_probe.py <store.sqlite> scratch/plans.txt
"""
import subprocess
import sys

SQLITE = "/usr/bin/sqlite3"

# (site, exact SQL, call site in the product)
STATEMENTS = [
    ("S2.next_pack", "SELECT next_pack_id FROM store_policy WHERE id = 1",
     "sqlite/ownership.rs:131 next_pack (query_row, prepared fresh per call)"),
    ("S3.insert_pack",
     "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, (SELECT save_id FROM temp.layerfs_read_scope))",
     "sqlite/write.rs:69 insert_pack (execute, prepared fresh per call)"),
    ("S4.append_pack",
     "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = (SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id AND publication IS NULL)",
     "sqlite/write.rs:81 append_pack (execute, prepared fresh per call)"),
    ("S5.insert_objects_1",
     "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES (?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))",
     "sqlite/write.rs:171 object_insert_sql(1) (prepare_cached)"),
    ("S6.ceiling",
     "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) WHERE save_id = ?1 AND active_slot IS NOT NULL",
     "cas/placement.rs:238 write_pack, on the write that creates a pack"),
    ("S7.advance_pack",
     "UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1",
     "sqlite/ownership.rs:139 advance_pack"),
    ("S8.publish_policy",
     "UPDATE store_policy SET publication_sequence = publication_sequence + 1, retained_pack_ceiling = MAX(retained_pack_ceiling, (SELECT pack_ceiling FROM saves WHERE save_id = ?1)) WHERE id = 1 AND publication_sequence < 9223372036854775807",
     "sqlite/ownership.rs:110 publish (once per save)"),
    ("S9.publish_save",
     "UPDATE saves SET active_slot = NULL, publication = (SELECT publication_sequence FROM store_policy WHERE id = 1) WHERE save_id = ?1 AND active_slot IS NOT NULL AND publication IS NULL",
     "sqlite/ownership.rs:118 publish (once per save)"),
    ("S10.acquire_slot",
     "SELECT CASE WHEN NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = 1) THEN 1 WHEN NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = 2) THEN 2 END",
     "sqlite/ownership.rs:91 acquire (once per save)"),
    ("S11.insert_save", "INSERT INTO saves (active_slot) VALUES (?1)",
     "sqlite/ownership.rs:102 acquire (once per save)"),
    ("S12.next_ordinal", "SELECT next_ordinal FROM store_policy WHERE id = 1",
     "sqlite/ownership.rs:150 reserve_ordinals"),
    ("S13.reserve_ordinals",
     "UPDATE store_policy SET next_ordinal = ?1, metadata_window_start = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?3 ELSE metadata_window_start END, metadata_window_values = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?2 ELSE metadata_window_values + ?2 END WHERE id = 1",
     "sqlite/ownership.rs:159 reserve_ordinals"),
    ("S14.insert_value_group",
     "INSERT INTO metadata_value_groups (first_ordinal, count, pack_id, group_number, digest, save_id) VALUES (?1,?2,?3,?4,?5,(SELECT save_id FROM temp.layerfs_read_scope))",
     "sqlite/pool.rs:75 write_value_groups (shape; see call site)"),
    ("S15.pool_latest",
     "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2",
     "sqlite/pool.rs:46 latest reservation window"),
    ("S16.locator_1",
     "SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,o.record_number,o.save_id,(o.save_id = r.save_id OR s.publication <= r.publication) FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE o.object_id IN (?) AND o.pack_id <= ?2",
     "sqlite/lookup.rs:76 candidates (prepare_cached; 380,444 calls a run, resolve_ns)"),
    ("S17.pack_bytes",
     "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE p.pack_id = ?1 AND length(p.data) BETWEEN 32 AND ?2 AND (p.save_id = r.save_id OR s.publication <= r.publication)",
     "sqlite/lookup.rs:161 pack_bytes"),
    ("S18.candidate_index_read",
     "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp",
     "encoding/delta/candidates.rs:284 index flush read"),
    ("S19.candidate_index_write",
     "INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature, save_id) VALUES (?1, ?2, ?3, ?4, (SELECT save_id FROM temp.layerfs_read_scope))",
     "encoding/delta/candidates.rs:363 index flush write"),
]

PREAMBLE = (
    "PRAGMA foreign_keys=ON;\n"
    "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL);\n"
    "INSERT INTO layerfs_read_scope VALUES (17, 17);\n"
)

FACTS = [
    ("pragma.page_count", "SELECT * FROM pragma_page_count()"),
    ("pragma.freelist_count", "SELECT * FROM pragma_freelist_count()"),
    ("pragma.page_size", "SELECT * FROM pragma_page_size()"),
    ("pragma.cache_size", "SELECT * FROM pragma_cache_size()"),
    ("pragma.cache_spill", "SELECT * FROM pragma_cache_spill()"),
    ("pragma.mmap_size", "SELECT * FROM pragma_mmap_size()"),
    ("pragma.auto_vacuum", "SELECT * FROM pragma_auto_vacuum()"),
    ("objects.count", "SELECT COUNT(*) FROM objects"),
    ("packs.count", "SELECT COUNT(*) FROM object_packs"),
    ("packs.body_bytes", "SELECT SUM(length(data)) FROM object_packs"),
    ("packs.max_body", "SELECT MAX(length(data)) FROM object_packs"),
    ("stat1.present", "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'sqlite_stat%'"),
]


def run(store, script):
    return subprocess.run([SQLITE, str(store)], input=script, capture_output=True,
                          text=True).stdout


def main():
    store, out = sys.argv[1], sys.argv[2]
    version = subprocess.run([SQLITE, "--version"], capture_output=True,
                             text=True).stdout.strip()
    lines = ["store: " + store, "sqlite3: " + version, ""]
    lines.append("== profile and store facts ==")
    for name, sql in FACTS:
        value = run(store, ".mode list\n" + sql + ";\n").strip()
        lines.append(name + ": " + value)
    lines.append("")
    for name, sql, site in STATEMENTS:
        lines.append("== " + name + " ==")
        lines.append("call site: " + site)
        lines.append("sql: " + sql)
        lines.append("-- EXPLAIN QUERY PLAN --")
        lines.append(run(store, PREAMBLE + "EXPLAIN QUERY PLAN " + sql + ";\n").rstrip())
        listing = run(store, PREAMBLE + "EXPLAIN " + sql + ";\n")
        opcodes = {}
        for line in listing.splitlines()[2:]:
            parts = line.split()
            if len(parts) > 1:
                opcodes[parts[1]] = opcodes.get(parts[1], 0) + 1
        lines.append("-- opcode histogram (EXPLAIN) --")
        lines.append("; ".join(str(count) + " x " + op for op, count in
                               sorted(opcodes.items(), key=lambda kv: (-kv[1], kv[0]))))
        lines.append("-- EXPLAIN listing --")
        lines.append(listing.rstrip())
        lines.append("")
    with open(out, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print("wrote " + out + " (" + str(len(lines)) + " lines)")


if __name__ == "__main__":
    main()

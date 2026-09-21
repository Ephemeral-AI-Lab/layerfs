#!/usr/bin/env python3
"""Controls for the two multi-row statements, to separate engine cost from the
Python harness's own per-row materialisation overhead. DIAGNOSTIC on a copy."""
import sqlite3, json, statistics, time
con = sqlite3.connect("file:/tmp/squadB/sample.sqlite?mode=ro", uri=True, isolation_level=None)
con.execute("PRAGMA temp_store = MEMORY")
con.execute("CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")
con.execute("INSERT INTO layerfs_read_scope VALUES (2,2)")

Q = {
 "a_COUNT_only (0 rows returned)": "SELECT COUNT(*) FROM content_signatures",
 "b_scan_no_order (rows returned)": "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c",
 "c_scan_order_by_stamp": "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c ORDER BY c.stamp",
 "d_C3_full (join+scope+order)": "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp",
 "e_P4_no_order": "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE g.first_ordinal >= ? AND (p.save_id = r.save_id OR s.publication <= r.publication)",
 "f_P4_with_order": "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE g.first_ordinal >= ? AND (p.save_id = r.save_id OR s.publication <= r.publication) ORDER BY g.first_ordinal",
 "g_empty_result_control": "SELECT c.stamp FROM content_signatures c WHERE 0",
}
out = {}
for k, sql in Q.items():
    args = (1,) if "?" in sql else ()
    for _ in range(20):
        con.execute(sql, args).fetchall()
    s = []
    for _ in range(400):
        t0 = time.perf_counter_ns(); con.execute(sql, args).fetchall(); s.append(time.perf_counter_ns() - t0)
    s.sort()
    out[k] = {"n": len(s), "median_ns": statistics.median(s), "mean_ns": statistics.mean(s)}
    print("%-34s median=%9.1f us  mean=%9.1f us" % (k, out[k]["median_ns"]/1000, out[k]["mean_ns"]/1000))
per_row = (out["b_scan_no_order (rows returned)"]["median_ns"] - out["a_COUNT_only (0 rows returned)"]["median_ns"]) / 8192
print("\nPython per-row materialisation+fetch, content_signatures (8192 rows): %.1f ns/row" % per_row)
per_row4 = (out["e_P4_no_order"]["median_ns"] - out["a_COUNT_only (0 rows returned)"]["median_ns"]) / 207
print("Python per-row materialisation+fetch, metadata_value_groups (207 rows): %.1f ns/row" % per_row4)
print("\nORDER BY c.stamp cost (sorted-scan minus scan): %.1f us" % ((out["c_scan_order_by_stamp"]["median_ns"]-out["b_scan_no_order (rows returned)"]["median_ns"])/1000))
print("ORDER BY g.first_ordinal cost: %.1f us" % ((out["f_P4_with_order"]["median_ns"]-out["e_P4_no_order"]["median_ns"])/1000))
json.dump({"queries": out, "python_per_row_ns_content_signatures": per_row,
           "python_per_row_ns_metadata_value_groups": per_row4}, open("controls.json","w"), indent=2)

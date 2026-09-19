import sys, json, sqlite3
sys.path.insert(0, "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/benchmark/fs-bench-pro-storage-content")
from shared import space
KEYS = ["delta.trials","delta.prefix_selected","delta.no_candidate","delta.full_losses",
        "delta.ineligible_candidates","delta.absent_candidates","delta.work_exceeded","delta.prepared_full"]
def counters(p):
    v = {}
    for line in open(p + "/trace.jsonl"):
        d = json.loads(line)
        if d.get("key") in KEYS: v[d["key"]] = d["value"]
    return v
rows = []
for label, p in (("PRODUCT", sys.argv[1]), ("HARNESS", sys.argv[2])):
    f = space.footprint(p + "/sample.sqlite")
    perf = json.load(open(p + "/phases-perf.json"))
    c = counters(p)
    db = sqlite3.connect(p + "/sample.sqlite")
    n = db.execute("SELECT COUNT(*) FROM content_signatures").fetchone()[0]
    pg = db.execute("SELECT SUM(pgsize), COUNT(*) FROM dbstat WHERE name=?", ("content_signatures",)).fetchone()
    uv = db.execute("PRAGMA user_version").fetchone()[0]
    rows.append((label, f, perf, c, n, pg, uv))
    print("=====", label)
    print("  apparent_bytes     ", f.stat.apparent_bytes)
    print("  allocated_bytes    ", f.stat.allocated_bytes)
    print("  pack_bodies        ", f.pack_bodies)
    print("  by_lane            ", f.pack_directory.by_lane)
    print("  objects            ", f.objects, " quick_check", f.quick_check, " incomplete", f.incomplete)
    print("  operation_ns       ", perf["operation_ns"], "=", round(perf["operation_ns"]/1e9,2), "s")
    print("  invocation_ns      ", perf["invocation_ns"], "=", round(perf["invocation_ns"]/1e9,2), "s")
    print("  cpu_user_ns        ", perf["cpu_user_ns"], "=", round(perf["cpu_user_ns"]/1e9,2), "s")
    print("  cpu_system_ns      ", perf["cpu_system_ns"], "=", round(perf["cpu_system_ns"]/1e9,2), "s")
    print("  peak_rss_bytes     ", perf["process_peak_rss_bytes"])
    print("  content_sig rows   ", n, " dbstat", pg, " user_version", uv)
    for k in KEYS: print("  " + k.ljust(28), c.get(k))
print()
print("DELTA product - harness: apparent", rows[0][1].stat.apparent_bytes - rows[1][1].stat.apparent_bytes)
print("DELTA product - harness: whole-file", rows[0][1].pack_directory.by_lane["whole-file"] - rows[1][1].pack_directory.by_lane["whole-file"])

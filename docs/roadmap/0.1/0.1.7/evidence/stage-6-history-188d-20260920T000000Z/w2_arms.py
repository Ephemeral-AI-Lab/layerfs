import sys, json
sys.path.insert(0, "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/benchmark/fs-bench-pro-storage-content")
from shared import space

KEYS = ["delta.trials","delta.prefix_selected","delta.no_candidate","delta.full_losses",
        "delta.ineligible_candidates","delta.absent_candidates","delta.work_exceeded","delta.prepared_full"]

def counters(path):
    vals = {}
    for line in open(path + "/trace.jsonl"):
        d = json.loads(line)
        if d.get("key") in KEYS:
            vals[d["key"]] = d["value"]
    return vals

def report(label, path):
    f = space.footprint(path + "/sample.sqlite")
    perf = json.load(open(path + "/phases-perf.json"))
    c = counters(path)
    print("=====", label)
    print("  apparent_bytes      ", f.stat.apparent_bytes)
    print("  allocated_bytes     ", f.stat.allocated_bytes)
    print("  pack_bodies         ", f.pack_bodies)
    print("  by_lane             ", f.pack_directory.by_lane)
    print("  objects             ", f.objects)
    print("  quick_check         ", f.quick_check, " incomplete", f.incomplete)
    print("  tables              ", f.schema_shape["tables"])
    print("  operation_ns        ", perf["operation_ns"], "=", round(perf["operation_ns"]/1e9, 2), "s")
    print("  invocation_ns       ", perf["invocation_ns"], "=", round(perf["invocation_ns"]/1e9, 2), "s")
    print("  cpu_user_ns         ", perf["cpu_user_ns"], "=", round(perf["cpu_user_ns"]/1e9, 2), "s")
    print("  cpu_system_ns       ", perf["cpu_system_ns"], "=", round(perf["cpu_system_ns"]/1e9, 2), "s")
    print("  peak_rss_bytes      ", perf["process_peak_rss_bytes"])
    for k in KEYS:
        print("  " + k.ljust(28), c.get(k))
    return f, perf, c

report("ARM 1  product index, harness switch OFF", sys.argv[1])
print()
report("ARM 2  harness index ON, current tree", sys.argv[2])

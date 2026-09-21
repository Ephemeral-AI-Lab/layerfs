#!/usr/bin/env python3
"""S5 independent re-derivation for #219 S0/S1/S2, from raw receipts only.

Run from the worktree root. Reads no report; re-computes every headline number
from perf.jsonl / verification.json and compares it to the value the reports claim.
Exits non-zero if any check fails.
"""
import json, sys

FAILS = []


def chk(label, got, want, tol=0.0):
    good = abs(got - want) <= tol if isinstance(want, float) else got == want
    if not good:
        FAILS.append(label)
    print(f"  [{'OK ' if good else 'BAD'}] {label}: {got}  (report claims {want})")


def load(path):
    return [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]


def sample(path):
    ls = load(path)
    return (next(d for d in ls if d["kind"] == "sample"),
            next(d for d in ls if d["kind"] == "summary"))


PR = "benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl"
TX = "benchmark-results/issue219/s1-ns10000-text-v1-candidate-20260921T031259Z/perf.jsonl"
PRV = "benchmark-results/issue219/s1-ns10000-verify-20260921T031259Z/verification.json"
TXV = "benchmark-results/issue219/s1-ns10000-text-v1-verify-20260921T031259Z/verification.json"

print("== S1 headline ==")
rows = {}
for tag, path, want in (("pseudorandom", PR, 944.881), ("text-v1", TX, 708.991)):
    s, m = sample(path)
    r = s["records"][0]
    rows[tag] = (s, r)
    chk(f"{tag} median ms", round(m["median_ns"] / 1e6, 3), want, 0.0005)
    chk(f"{tag} median == record layerstack_init_ns",
        m["median_ns"] == r["layerstack_init_ns"], True)
    want_rate = 423.3 if tag == "pseudorandom" else 564.2
    chk(f"{tag} rate over 400 MB", round(400_000_000 / (m["median_ns"] / 1e9) / 1e6, 1),
        want_rate, 0.05)

print("== verification ==")
for tag, path, want_ns in (("pseudorandom", PRV, 77619792), ("text-v1", TXV, 67837542)):
    d = json.load(open(path))
    c = d["checks"][0]
    chk(f"{tag} status", c["status"], "pass")
    chk(f"{tag} cleanup", c["cleanup_status"], "pass")
    chk(f"{tag} verification_ns", c["verification_ns"], want_ns)
    chk(f"{tag} scanned", (c["scanned_files"], c["scanned_bytes"]), (10000, 300000000))

print("== identity match, performance row vs verification receipt ==")
for path, vpath in ((PR, PRV), (TX, TXV)):
    ids = sample(path)[0]["identities"]
    d = json.load(open(vpath))
    for key, ident in (("source_identity", "source_identity"),
                       ("input_identity", "input_identity"),
                       ("product_identity", "product_identity"),
                       ("image_identity", "image"),
                       ("harness_identity", "harness_identity")):
        chk(f"  {key}", d[key], ids[ident])
    for ident in ("LAYERFS_SOURCE_COMMIT", "LAYERFS_PRODUCT_SEAL",
                  "LAYERFS_COMPILATION_SEAL", "LAYERFS_DEPENDENCY_SEAL",
                  "WORKLOAD_SOURCE_SHA256"):
        chk(f"  {ident}", d["host_executor"][ident], ids["host_executor"][ident])

print("== S2 mechanism ==")
a, b = rows["pseudorandom"][1], rows["text-v1"][1]
chk("admission transactions identical",
    a["initialize_admission_transactions"] == b["initialize_admission_transactions"] == 73, True)
chk("store growth ratio", round(a["store_growth_bytes"] / b["store_growth_bytes"], 1), 36.2, 0.05)
chk("system CPU delta ms",
    round((a["initialization_system_cpu_ns"] - b["initialization_system_cpu_ns"]) / 1e6, 1), 370.5, 0.05)
chk("user CPU delta ms",
    round((a["initialization_user_cpu_ns"] - b["initialization_user_cpu_ns"]) / 1e6, 1), -113.6, 0.05)
chk("anchor / max single transaction",
    round(a["anchor_bytes"] / a["initialize_max_transaction_bytes"], 1), 24.1, 0.05)
chk("marginal ms per MB",
    round((a["layerstack_init_ns"] - b["layerstack_init_ns"]) / 1e6 / 300, 3), 0.786, 0.0005)
chk("disk read MB (pseudorandom)", round(a["initialization_disk_read_bytes"] / 1e6, 2), 0.73, 0.005)
chk("container command-window CPU ms (pseudorandom)",
    round(rows["pseudorandom"][0]["resources"]["command_window_cpu_ns"] / 1e6, 1), 12.0, 0.05)
chk("CPU / wall (pseudorandom)",
    round((a["initialization_user_cpu_ns"] + a["initialization_system_cpu_ns"])
          / a["layerstack_init_ns"], 2), 1.89, 0.005)

print("== cache declaration ==")
for tag, (s, r) in rows.items():
    h = next(d for d in load(PR if tag == "pseudorandom" else TX) if d["kind"] == "header")
    chk(f"{tag} cache_contract", h["cache_contract"], None)
    chk(f"{tag} fixture_cache_profile", r["fixture_cache_profile"],
        "reused-first-sample-uncontrolled")

print()
if FAILS:
    print(f"FAILED: {len(FAILS)} check(s): {FAILS}")
    sys.exit(1)
print("ALL CHECKS PASS")

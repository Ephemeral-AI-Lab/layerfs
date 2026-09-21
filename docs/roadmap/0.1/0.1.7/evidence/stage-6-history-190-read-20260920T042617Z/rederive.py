#!/usr/bin/env python3
"""Independent re-derivation from raw receipts only.

Deliberately does not import analyze.py / write_results.py / compare_stores.py:
the arithmetic below is recomputed from `raw/timing.json`, `raw/trace-perf.jsonl`,
`raw/phases-perf.json` and `perf-receipt.json`, and every identity/custody hash is
recomputed from the files themselves.
"""
import hashlib, json
from pathlib import Path

C = Path(__file__).resolve().parent
FAIL = []


def check(label, got, want):
    ok = got == want
    print(f"{'OK  ' if ok else 'FAIL'} {label}: {got!r}" + ("" if ok else f" != {want!r}"))
    if not ok:
        FAIL.append(label)


def sha(path):
    d = hashlib.sha256()
    with path.open("rb") as h:
        for b in iter(lambda: h.read(1 << 20), b""):
            d.update(b)
    return d.hexdigest()


def tree(node, path, out):
    key = f"{path}/{node['name']}"
    out[key] = node["elapsed_ns"]
    for child in node.get("children", []):
        tree(child, key, out)


def load(arm, case):
    run = C / "runs" / f"{arm}-{case}"
    timing = {}
    tree(json.loads((run / "raw/timing.json").read_text()), "", timing)
    res = {}
    for line in (run / "trace-perf.jsonl").read_text().splitlines():
        row = json.loads(line)
        try:
            value = int(row["value"])
        except (TypeError, ValueError):
            continue
        if row["kind"] == "resource" and row["key"].startswith("history.state."):
            res[row["key"]] = value
    phases = json.loads((run / "raw/phases-perf.json").read_text())
    receipt = json.loads((run / "perf-receipt.json").read_text())
    return timing, res, phases, receipt, run


results = json.loads((C / "results.json").read_text())
for case, short in (("history-stride10", "stride10"), ("history-stride3", "stride3")):
    print(f"=== {case} ===")
    ops, prov, cat, stmts, stores = {}, {}, {}, {}, {}
    for arm in ("baseline2", "candidate2"):
        timing, res, phases, receipt, run = load(arm, case)
        # Direct children of the root only, and every one of them: the established
        # #190 convention includes the first state, which is what
        # `phases-perf.operation_ns` measures.
        measured = sorted({int(c["name"].split(".")[-1]) for c in
                           json.loads((run / "raw/timing.json").read_text())["children"]
                           if c["name"].startswith("history.state.")})
        op = sum(timing[f"/history/history.state.{s}"] for s in measured)
        ops[arm] = op
        check(f"{arm} operation == phases-perf.operation_ns", op, phases["operation_ns"])
        check(f"{arm} measured states", len(measured), 17 if short == "stride10" else 53)
        prov[arm] = sum(v for k, v in res.items() if k.endswith("filesystem.provider.read_elapsed_ns"))
        cat[arm] = sum(v for k, v in res.items() if k.endswith("filesystem.provider.work.catalogue_ns"))
        stmts[arm] = sum(v for k, v in res.items() if k.endswith("filesystem.provider.work.catalogue_calls"))
        check(f"{arm} invocation <= wall", phases["invocation_ns"] <= receipt["wall_ns"], True)
        identity = json.loads((C / f"{arm}-identity.json").read_text())
        check(f"{arm} binary sha", sha(Path(identity["binary"])), identity["binary_sha256"])
        check(f"{arm} binary sha == results.json", identity["binary_sha256"],
              results["cases"][case]["arms"][arm]["binary_sha256"])
        stores[arm] = sha(run / "raw/sample.sqlite")
        check(f"{arm} store sha == results.json", stores[arm],
              results["cases"][case]["arms"][arm]["store"]["sha256"])
    check("store byte-identical between arms", stores["baseline2"], stores["candidate2"])
    check("operation reduction", ops["baseline2"] - ops["candidate2"],
          results["cases"][case]["operation_reduction_ns"])
    check("provider reduction", prov["baseline2"] - prov["candidate2"],
          results["cases"][case]["provider_reduction_ns"])
    check("catalogue reduction", cat["baseline2"] - cat["candidate2"],
          results["cases"][case]["catalogue_reduction_ns"])
    print(f"     statements {stmts['baseline2']:,} -> {stmts['candidate2']:,}")
    print(f"     operation  {ops['baseline2']:,} -> {ops['candidate2']:,} "
          f"({(ops['baseline2']-ops['candidate2'])/ops['baseline2']*100:.6f}%)")

custody = json.loads((C / "custody.json").read_text())
for entry in custody:
    path = Path(entry["path"])
    check(f"custody {path.name}/{path.parent.parent.name}", sha(path), entry["sha256"])

ledger = (C.parents[2] / "0.1.6/evidence/issue151-experiment-ledger.md").read_text()
check("ledger has L42", "## L42 —" in ledger, True)
readme = (C / "README.md").read_text()
for needle in ("2,017,475,457", "7,096,329,758", "65,337", "368,074", "85,722 → 85,725"):
    check(f"README states {needle}", needle in readme, True)

print()
print("FAILURES:", FAIL if FAIL else "none")
raise SystemExit(1 if FAIL else 0)

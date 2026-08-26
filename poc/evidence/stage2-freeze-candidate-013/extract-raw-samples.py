#!/usr/bin/env python3
import json
from pathlib import Path

root = Path(__file__).resolve().parent
populations = {}
for control in ("var", "tmp"):
    raw = json.loads((root / f"fs-bench-{control}.json").read_text())
    rows = []
    for row in raw["results"]:
        assert row["samples"] == 3
        samples = [row["minNs"], row["medianNs"], row["maxNs"]]
        assert row["meanNs"] == sum(samples) // 3
        assert row["p95Ns"] == samples[-1]
        rows.append({
            "scenario": row["scenario"],
            "target": row["target"],
            "samples_sorted_ns": samples,
            "execution_order": None,
            "execution_order_reason": "fs-bench emits min/median/max for n=3 but does not emit the unseeded shuf execution order",
        })
    populations[control] = rows

receipt = {
    "schema": "layerfs-stage2-raw-latency-arrays-v1",
    "status": "PASS",
    "derivation": "for n=3, the exact sorted sample array is [minNs, medianNs, maxNs]",
    "populations": populations,
}
with (root / "raw-latency-arrays.json").open("x") as output:
    json.dump(receipt, output, indent=2, sort_keys=True)
    output.write("\n")

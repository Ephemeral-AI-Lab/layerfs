#!/usr/bin/env python3
import hashlib
import json
import math
import re
import sys

SCENARIOS = [
    "create 1000 files",
    "stat 1000 files",
    "rm 1000 files",
    "mkdir tree (10x10x10)",
    "find tree",
    "write 64 MiB",
    "copy 64 MiB",
    "read 64 MiB",
    "pure read 64 MiB",
    "pure copy 64 MiB",
    "overwrite 64 MiB",
    "git init + commit 100 files",
]
BUDGET_MS = [300, 900, 400, 850, 950, 80, 250, 130, 55, 180, 100, 300]
CLOUDFLARE = {
    "overlay": [
        47.99863781755993,
        2.8361612918000367,
        46.018841912836585,
        2.938581383855921,
        3.0008105147893724,
        5.637418623922647,
        11.257277771460702,
        7.398305084745763,
        11.034601973095558,
        16.867803364553968,
        13.967312342333491,
        34.088251380012224,
    ],
    "tmpfs": [
        105.85889485256469,
        2.9466310474695425,
        112.0832896762616,
        2.9672212498494988,
        3.1075732080119245,
        3.717999108100616,
        9.160583730712352,
        6.460185302345052,
        10.636982840611156,
        20.107444581224637,
        7.7600729038864555,
        53.351721265564635,
    ],
}


def stdout_checks(stdout_bytes):
    stdout = re.sub(rb"\x1b\[[0-9;]*m", b"", stdout_bytes).decode(
        "utf-8", errors="replace"
    )
    return {
        "stdout_fail_markers": sum(
            "FAIL" in line for line in stdout.splitlines()
        )
        == 0,
        "stdout_network_scenarios": sum(
            "git clone (shallow, ~1MB)" in line for line in stdout.splitlines()
        )
        == 0,
    }


def main():
    if sys.argv[1:] == ["--self-test"]:
        assert all(stdout_checks(b"\x1b[32mOK\x1b[0m filtered\n").values())
        assert not stdout_checks(b"\x1b[31mFAIL\x1b[0m hidden\n")[
            "stdout_fail_markers"
        ]
        assert not stdout_checks(b"git clone (shallow, ~1MB)\n")[
            "stdout_network_scenarios"
        ]
        return
    if len(sys.argv) != 5 or sys.argv[3] not in CLOUDFLARE:
        raise SystemExit(
            "usage: verify_fs_bench.py RAW.json STDOUT overlay|tmpfs OUTPUT.json"
        )
    raw_path, stdout_path, control, output_path = sys.argv[1:]
    raw_bytes = open(raw_path, "rb").read()
    stdout_bytes = open(stdout_path, "rb").read()
    raw = json.loads(raw_bytes)
    config = raw.get("config", {})
    checks = {
        "reps": config.get("reps") == 3,
        "warmup": config.get("warmup") == 1,
        "randomized": config.get("randomizeTargets") == 1,
        "mount": config.get("mount") == "/workspace",
        "base": config.get("base") == ("/var/tmp" if control == "overlay" else "/tmp"),
        **stdout_checks(stdout_bytes),
    }
    rows = raw.get("results", [])
    by_key = {(row.get("scenario"), row.get("target")): row for row in rows}
    checks["row_count"] = len(rows) == 24
    checks["unique_rows"] = len(by_key) == 24
    checks["matrix"] = set(by_key) == {
        (scenario, target) for scenario in SCENARIOS for target in ("computerd", "base")
    }
    verified = []
    layer_sum = base_sum = maximum_sum = 0
    ratios = []
    for index, scenario in enumerate(SCENARIOS):
        layer = by_key.get((scenario, "computerd"), {})
        base = by_key.get((scenario, "base"), {})
        for row in (layer, base):
            valid = (
                row.get("samples") == 3
                and 0 < row.get("minNs", 0) <= row.get("medianNs", 0) <= row.get("maxNs", 0)
                and row.get("p95Ns") == row.get("maxNs")
                and row.get("meanNs")
                == (row.get("minNs", 0) + row.get("medianNs", 0) + row.get("maxNs", 0)) // 3
            )
            checks[f"statistics:{scenario}:{row.get('target', 'missing')}"] = valid
        layer_median = layer.get("medianNs", 0)
        base_median = base.get("medianNs", 0)
        ratio = layer_median / base_median if base_median else math.inf
        layer_sum += layer_median
        base_sum += base_median
        maximum_sum += layer.get("maxNs", 0)
        ratios.append(ratio)
        verified.append(
            {
                "scenario": scenario,
                "layerfs_median_ns": layer_median,
                "layerfs_max_ns": layer.get("maxNs"),
                "base_median_ns": base_median,
                "median_ratio": ratio,
                "optimized_budget_ns": BUDGET_MS[index] * 1_000_000,
                "budget_miss_ns": max(0, layer_median - BUDGET_MS[index] * 1_000_000),
                "cloudflare_ratio": CLOUDFLARE[control][index],
                "cloudflare_ratio_limit": 1.1 * CLOUDFLARE[control][index],
                "cloudflare_ratio_pass": ratio <= 1.1 * CLOUDFLARE[control][index],
            }
        )
    ratio_sum = layer_sum / base_sum if base_sum else math.inf
    geometric = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
    spread = maximum_sum / layer_sum if layer_sum else math.inf
    gates = {
        "SL": layer_sum <= 4_500_000_000,
        "Rsum": ratio_sum <= (2.85 if control == "overlay" else 3.10),
        "G": geometric <= (7.00 if control == "overlay" else 7.75),
        "Spread": spread <= 1.15,
        "cloudflare_rows": all(row["cloudflare_ratio_pass"] for row in verified),
    }
    status = "PASS_OPTIMIZED" if all(checks.values()) and all(gates.values()) else "REVISE"
    receipt = {
        "schema": "layerfs-stage2-fs-bench-verification-v2",
        "status": status,
        "control": control,
        "raw_sha256": hashlib.sha256(raw_bytes).hexdigest(),
        "stdout_sha256": hashlib.sha256(stdout_bytes).hexdigest(),
        "checks": checks,
        "gates": gates,
        "aggregates": {
            "SL_ns": layer_sum,
            "SB_ns": base_sum,
            "Rsum": ratio_sum,
            "G": geometric,
            "Spread": spread,
        },
        "rows": verified,
    }
    with open(output_path, "x") as output:
        json.dump(receipt, output, indent=2, sort_keys=True)
        output.write("\n")
    print(json.dumps(receipt, sort_keys=True))
    raise SystemExit(0 if status == "PASS_OPTIMIZED" else 1)


if __name__ == "__main__":
    main()

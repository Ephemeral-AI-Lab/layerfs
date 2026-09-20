#!/usr/bin/env python3
"""Assemble results.json from the retained receipts; never invents a number."""
import hashlib, json
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
CASES = {"history-stride10": "stride10", "history-stride3": "stride3"}
ARMS = ("baseline2", "candidate2")


def load(name):
    return json.loads((CAMPAIGN / name).read_text())


def main():
    out = {"schema": "h190-read-results-v1",
           "status": "Research; diagnostic evidence, not release admission",
           "corrections": [
               "Operation and region figures were first derived over states 2..N; the "
               "harness's phases-perf.operation_ns and the retained L40 table sum over "
               "all selected states including the first. Corrected in this file; the "
               "superseded derived JSON is kept and labelled in SUPERSEDED-ANALYSIS.md.",
               "The stride10 provider's unattributed remainder was first reported as "
               "1,339,076,683 ns; the disjoint set leaves 1,305,940,532 ns.",
               "Both were found by rederive.py, which recomputes the headline arithmetic "
               "from the raw receipts without importing this script.",
           ],
           "measured_candidate": "candidate2 (ordinal-ordered catalogue resolution)",
           "rejected_candidate": "candidate (one catalogue statement per leaf ordinal span)",
           "cases": {}}
    for case, short in CASES.items():
        body = {"arms": {}}
        for arm in ARMS:
            run = CAMPAIGN / "runs" / f"{arm}-{case}"
            analysis = load(f"{arm}-{short}-analysis.json")
            perf = json.loads((run / "raw/phases-perf.json").read_text())
            verify = json.loads((run / "raw/phases-verify.json").read_text())
            receipt = json.loads((run / "perf-receipt.json").read_text())
            identity = json.loads((CAMPAIGN / f"{arm}-identity.json").read_text())
            store = run / "raw/sample.sqlite"
            body["arms"][arm] = {
                "binary_sha256": identity["binary_sha256"],
                "operation_ns": analysis["operation_ns"],
                "regions_ns": analysis["regions_ns"],
                "unattributed_state_body_ns": analysis["unattributed_state_body_ns"],
                "provider": analysis["provider"],
                "pooled": analysis["pooled"],
                "work": analysis["work"],
                "per_state_operation_ns": analysis["per_state_operation_ns"],
                "complete_command": {
                    "wall_ns": receipt["wall_ns"],
                    "invocation_ns": perf["invocation_ns"],
                    "preparation_ns": perf["preparation_ns"],
                    "cpu_user_ns": perf["cpu_user_ns"],
                    "cpu_system_ns": perf["cpu_system_ns"],
                    "cpu_total_ns": perf["cpu_user_ns"] + perf["cpu_system_ns"],
                    "process_peak_rss_bytes": perf["process_peak_rss_bytes"],
                    "exit_code": receipt["exit_code"],
                    "limit_ns": receipt["limit_ns"],
                    "quiet_last_cpu_idle_percent": receipt["pre_observation"]["last_cpu_idle_percent"],
                    "competing_resource_processes": receipt["pre_observation"]["competing_resource_processes"],
                },
                "store": {
                    "apparent_bytes": store.stat().st_size,
                    "allocated_bytes": store.stat().st_blocks * 512,
                    "sha256": hashlib.sha256(store.read_bytes()).hexdigest(),
                },
                "verification": {
                    "ns": verify["verification_ns"],
                    "wall_ns": json.loads((run / "verify-receipt.json").read_text())["wall_ns"],
                    "limit_ns": json.loads((run / "verify-receipt.json").read_text())["limit_ns"],
                },
            }
        left, right = body["arms"]["baseline2"], body["arms"]["candidate2"]
        body["operation_reduction_ns"] = left["operation_ns"] - right["operation_ns"]
        body["operation_reduction_percent"] = round(
            (left["operation_ns"] - right["operation_ns"]) / left["operation_ns"] * 100, 6)
        body["complete_command_reduction_ns"] = (left["complete_command"]["wall_ns"]
                                                 - right["complete_command"]["wall_ns"])
        body["cpu_reduction_ns"] = (left["complete_command"]["cpu_total_ns"]
                                    - right["complete_command"]["cpu_total_ns"])
        body["provider_reduction_ns"] = (left["provider"]["elapsed_ns"]
                                         - right["provider"]["elapsed_ns"])
        body["catalogue_reduction_ns"] = (left["work"]["catalogue_ns"]
                                          - right["work"]["catalogue_ns"])
        body["store_byte_identical"] = left["store"]["sha256"] == right["store"]["sha256"]
        body["verification_target_ns"] = 10_000_000_000 if short == "stride10" else 20_000_000_000
        out["cases"][case] = body
    # the rejected span candidate, where it was sampled
    for case, short in CASES.items():
        path = CAMPAIGN / f"candidate-{short}-analysis.json"
        if path.exists():
            rejected = json.loads(path.read_text())
            baseline = out["cases"][case]["arms"]["baseline2"]
            out["cases"][case]["rejected_span_candidate"] = {
                "operation_ns": rejected["operation_ns"],
                "operation_delta_ns": rejected["operation_ns"] - baseline["operation_ns"],
                "catalogue_calls": rejected["work"]["catalogue_calls"],
                "catalogue_ns": rejected["work"]["catalogue_ns"],
                "nanoseconds_per_statement": rejected["work"]["catalogue_ns"]
                // max(rejected["work"]["catalogue_calls"], 1),
            }
    (CAMPAIGN / "results.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
    print(json.dumps({case: {k: v for k, v in body.items()
                             if not isinstance(v, dict) or k == "rejected_span_candidate"}
                      for case, body in out["cases"].items()}, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

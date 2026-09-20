#!/usr/bin/env python3
"""Read one retained run's timing tree and trace; write a fresh named JSON."""
import argparse, collections, json
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent


def walk(node, path, out):
    key = f"{path}/{node['name']}"
    out[key] = node["elapsed_ns"]
    for child in node.get("children", []):
        walk(child, key, out)
    return out


def load(arm, case):
    run = CAMPAIGN / "runs" / f"{arm}-{case}"
    timing = walk(json.loads((run / "raw/timing.json").read_text()), "", {})
    counters, resources = collections.Counter(), {}
    for line in (run / "trace-perf.jsonl").read_text().splitlines():
        row = json.loads(line)
        try:
            value = int(row["value"])
        except (TypeError, ValueError):
            continue
        if row["kind"] == "counter":
            counters[row["key"]] += value
        elif row["kind"] == "resource" and row["key"].startswith("history.state."):
            resources[row["key"]] = value
    return timing, counters, resources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arm"); parser.add_argument("case")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    timing, counters, resources = load(args.arm, args.case)
    states = sorted({int(k.split("/")[2].split(".")[-1]) for k in timing if k.startswith("/history/history.state.")})
    measured = [s for s in states if s != 1]
    regions = ["content", "filesystem", "storage.begin", "storage.accept_loop",
               "storage.finish", "harness.index", "harness.input", "harness.predecessors"]
    totals = {r: sum(timing.get(f"/history/history.state.{s}/{r}", 0) for s in measured) for r in regions}
    totals["store.create"] = sum(timing.get(f"/history/history.state.{s}/store.create", 0) for s in measured)
    operation = sum(timing[f"/history/history.state.{s}"] for s in measured)
    # provider elapsed from the harness's own resource rows
    def work_total(suffix):
        return sum(v for k, v in resources.items() if k.endswith(f"filesystem.provider.work.{suffix}"))
    def pooled_total(suffix):
        return sum(v for k, v in resources.items() if k.endswith(f"filesystem.provider.pooled.{suffix}"))
    def provider_total(suffix):
        return sum(v for k, v in resources.items() if k.endswith(f"filesystem.provider.{suffix}"))
    document = {
        "schema": "h190-read-analysis-v1", "arm": args.arm, "case": args.case,
        "states": measured, "operation_ns": operation,
        "regions_ns": totals,
        "unattributed_state_body_ns": operation - sum(totals.values()),
        "provider": {
            "elapsed_ns": provider_total("read_elapsed_ns"),
            "waves": provider_total("read_waves"),
            "requested_objects": provider_total("requested_objects"),
            "returned_objects": provider_total("returned_objects"),
            "returned_bytes": provider_total("returned_canonical_bytes"),
            "connection_opens": provider_total("connection_opens"),
            "group_decodes": provider_total("group_decodes"),
        },
        "pooled": {s: pooled_total(s) for s in
                   ("leaf_requests", "chain_edges", "physical_record_calls", "physical_group_decodes",
                    "physical_group_decoded_bytes", "physical_group_cache_hits", "value_group_decodes",
                    "pack_fetches", "pack_bytes")},
        "work": {s: work_total(s) for s in
                 ("pack_demands", "pack_fetches", "pack_bytes", "pack_fetch_ns", "control_ns",
                  "group_decode_ns", "record_frame_ns", "value_decode_ns", "locator_ns",
                  "catalogue_ns", "catalogue_calls", "covering_reuse", "physical_rebuild_ns",
                  "body_decode_ns", "leaf_encode_ns", "decode_call_ns")},
        "per_state_operation_ns": {s: timing[f"/history/history.state.{s}"] for s in measured},
    }
    (CAMPAIGN / args.out).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print(json.dumps(document, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

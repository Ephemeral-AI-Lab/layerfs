#!/usr/bin/env python3
"""Count host<->sandbox round trips inside the frozen #232 Exec/FUSE timed windows.

Reads retained evidence only (telemetry.lft1 + driver-receipt.json + the committed
registry). It never runs a workload and never produces a wall-clock claim.

Usage:
  python3 analyze_roundtrips.py --repo /path/to/layerfs \
      --results benchmark-results/fs-bench-pro/sdk-exec-fuse [--case SUBSTRING]

Emits:
  1. per-case decomposition: root/edit/commit, in-window upstream ops, spans, gaps
  2. the upstream_calls cross-check (product counter vs observed op count)
  3. per-block projection-request ratio for the shift algorithms
"""
import argparse, glob, json, math, os, sys, collections, statistics as st

BLOCK = 131072
UPSTREAM = ("Inspect", "ReadFile", "EditFile", "UpdatePortableMetadata", "HistoryCommand")


def load_ops(path):
    out = []
    with open(path, errors="replace") as fh:
        for line in fh:
            if line.startswith("LFT1 "):
                rec = json.loads(line[5:])
                if rec.get("kind") == "operation":
                    out.append(rec)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True, help="checkout containing the registry")
    ap.add_argument("--results", required=True, help=".../fs-bench-pro/sdk-exec-fuse")
    ap.add_argument("--case", default="", help="only scenarios containing this substring")
    args = ap.parse_args()

    reg_path = os.path.join(args.repo, "core/benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json")
    registry = {c["scenario_id"]: c for c in json.load(open(reg_path))["cases"]}

    rows = []
    for tel in sorted(glob.glob(os.path.join(args.results, "*", "*", "telemetry.lft1"))):
        folder = os.path.dirname(tel)
        scenario = os.path.basename(folder)
        if args.case and args.case not in scenario:
            continue
        ops = load_ops(tel)
        roots = [o for o in ops if o["timing"]["name"] == "sdk.edit_commit.fuse"]
        if not roots:
            continue
        root = roots[0]
        children = {c["name"]: c["elapsed_ns"] for c in root["timing"]["children"]}
        start, end = root["opened_ns"], root["closed_ns"]
        edit_end = start + children.get("edit", 0)
        inside = [o for o in ops if o["timing"]["name"] != "sdk.edit_commit.fuse"
                  and o["opened_ns"] >= start and o["closed_ns"] <= end]
        try:
            receipt = json.load(open(os.path.join(folder, "driver-receipt.json")))
        except Exception:
            receipt = {}
        span = sum(o["timing"]["elapsed_ns"] for o in inside)
        counts = collections.Counter(o["timing"]["name"] for o in inside)
        rows.append(dict(
            scenario=scenario, root=root["timing"]["elapsed_ns"], edit=children.get("edit"),
            commit=children.get("commit"), inside=inside, span=span, counts=counts,
            upstream=receipt.get("upstream_calls"),
            projection=dict(kv.split("=") for kv in (receipt.get("projection_counts") or "").split(",") if "=" in kv),
            bytes=dict(kv.split("=") for kv in (receipt.get("projection_bytes") or "").split(",") if "=" in kv),
            sizes=dict(kv.split("=") for kv in (receipt.get("projection_size_histogram") or "").split(",") if "=" in kv),
            status=receipt.get("status"), edit_end=edit_end))

    if not rows:
        sys.exit("no telemetry.lft1 found")

    ms = lambda v: f"{v/1e6:.2f}" if v is not None else "-"
    print(f"{len(rows)} windows  (| means inside the timed window)")
    print(f"{'scenario':46} {'root':>8} {'edit':>8} {'commit':>8} | "
          f"{'rt':>3} {'upstream span':>13} {'share':>6} | ops")
    for r in sorted(rows, key=lambda r: r["root"]):
        rt = sum(r["counts"].values())
        share = 100.0 * r["span"] / r["root"]
        ops = ",".join(f"{k}x{v}" for k, v in sorted(r["counts"].items()))
        print(f"{r['scenario'][:46]:46} {ms(r['root']):>8} {ms(r['edit']):>8} {ms(r['commit']):>8} | "
              f"{rt:>3} {r['span']/1e6:>10.2f} ms {share:>5.1f}% | {ops}")

    completed = [r for r in rows if r["commit"] is not None]
    bad = [r["scenario"] for r in completed if r["upstream"] != sum(r["counts"].values())]
    print(f"\nupstream_calls cross-check: {len(completed)-len(bad)}/{len(completed)} match", end="")
    print("" if not bad else "  MISMATCHES: " + ", ".join(bad))

    print("\nprojection byte totals and non-empty request-size buckets:")
    print(f"  {'scenario':46} {'bytes':>44}  buckets")
    for r in rows:
        bytes_ = r["bytes"]
        buckets = {k: v for k, v in r["sizes"].items() if int(v)}
        print(f"  {r['scenario'][:46]:46} " +
              " ".join(f"{k}={v}" for k, v in sorted(bytes_.items())).rjust(44) +
              "  " + (" ".join(f"{k}={v}" for k, v in sorted(buckets.items())) or "-"))

    print("\nper-op host-side span inside the windows:")
    inv = collections.defaultdict(list)
    for r in rows:
        for o in r["inside"]:
            inv[o["timing"]["name"]].append(o["timing"]["elapsed_ns"])
    for name in UPSTREAM:
        if name in inv:
            print(f"  {name:24} n={len(inv[name]):5} median={ms(st.median(inv[name])):>8} ms "
                  f"max={ms(max(inv[name])):>8} ms")

    print("\nprojection requests per declared 128 KiB block (shift algorithms):")
    print(f"  {'scenario':46} {'blocks':>6} {'FUSE rd':>7} {'rd/blk':>6} {'FUSE wr':>7} {'wr/blk':>6} "
          f"{'ReadFile':>8}")
    for r in sorted(rows, key=lambda r: r["scenario"]):
        c = registry.get(r["scenario"])
        if not c or not c["editor_algorithm"].startswith("in-place-window-shift"):
            continue
        shifted = c["fixture_bytes"] - (c["edit_start"] + c["delete_len"])
        blocks = math.ceil(shifted / BLOCK)
        rd, wr = int(r["projection"].get("read", -1)), int(r["projection"].get("write", -1))
        rf = r["counts"].get("ReadFile", 0)
        ratio = lambda v: f"{v/blocks:.2f}" if blocks and v >= 0 else "-"
        print(f"  {r['scenario'][:46]:46} {blocks:>6} {rd:>7} {ratio(rd):>6} {wr:>7} {ratio(wr):>6} {rf:>8}")

    print("\nNOTE: every wall here is a single retained observation. It is evidence for a")
    print("count-driven diagnosis; it is not a new sample and must never be promoted.")


if __name__ == "__main__":
    main()

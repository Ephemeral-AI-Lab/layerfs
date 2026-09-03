#!/usr/bin/env python3
"""Render the source-bound supporting-family release refresh, without rerunning work."""
import hashlib
import json
import statistics
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "benchmark-results/fs-bench-pro"
RUN = "release-v012-e978edd1"


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def cell(data, key, scale=1_000_000):
    values = [r[key] / scale for r in data]
    return f"{statistics.median(values):.3f} ({min(values):.3f}–{max(values):.3f})"


def main():
    namespace = ROOT / "init_namespace" / f"{RUN}-performance"
    ns_verify = ROOT / "init_namespace" / f"{RUN}-verification"
    store = ROOT / "store-footprint" / f"{RUN}-performance"
    store_verify = ROOT / "store-footprint" / f"{RUN}-verification"
    ns = rows(namespace / "performance/raw.jsonl")
    nv = rows(ns_verify / "verification/raw.jsonl")
    st = rows(store / "performance/raw.jsonl")
    sv = rows(store_verify / "verification/raw.jsonl")
    assert len(ns) == 12 and len(nv) == 4 and len(st) == 9 and len(sv) == 3
    assert {r["seed"] for r in ns} == {1, 2, 3}
    assert all(r["cleanup_status"] == "pass" and r["verification_status"] == "not-run-performance-mode" for r in ns)
    assert all(r["status"] == "pass" for r in nv + st + sv)
    assert all(r["mode"] == "performance" for r in ns + st)
    assert all(r["mode"] == "verify" for r in nv + sv)
    assert {r["scenario_id"] for r in ns} == {r["scenario_id"] for r in nv}
    assert {r["control_id"] for r in st} == {r["control_id"] for r in sv}
    lines = ["# v0.1.2 supporting benchmark refresh", "",
             "> **Status:** Completed release-source supporting-family measurements and separate verification.", "",
             "Measured source: `e978edd19f189d56ca8678bae4dcdc7b6cd4f409`. These are fresh candidate observations, not a paired product-speedup claim. Namespace initialization and Store construction use their frozen historical lifecycle controls; they do not replace or claim the semantics of the three SDK-only edit families.", "",
             "Environment: native macOS SDK/Store, Docker Desktop managed Linux container, real FUSE, no host bind mount. Every sample initializes its own Store. Input files may be reused, but initialized output Stores and measured results are not. Performance was collected before final full verification. OS caches are not flushed.", "",
             "All elapsed-time cells below are **median (minimum–maximum), in milliseconds**. Namespace first-sample and subsequent-sample cache cohorts are separated. MB is decimal; MiB is binary.", "",
             "## Namespace initialization", "",
             "| Files | Logical MB | Cache cohort | N | Initialization ms | MB/s at median init | Create ms | Commit/visibility ms | Lifecycle ms | Process lifetime RSS MiB |",
             "| ---: | ---: | --- | ---: | --- | ---: | --- | --- | --- | --- |"]
    for scenario in sorted({r["scenario_id"] for r in ns}, key=lambda x:int(x.split("-")[-1])):
        for first in (True, False):
            data = [r for r in ns if r["scenario_id"] == scenario and (r["seed"] == 1) == first]
            assert len(data) == (1 if first else 2)
            size = data[0]["scanned_bytes"]
            throughput = size / statistics.median(r["layerstack_init_ns"] for r in data) * 1000
            lines.append(f"| {data[0]['scanned_files']:,} | {size/1e6:g} | {'first' if first else 'subsequent'} | {len(data)} | {cell(data,'layerstack_init_ns')} | {throughput:.1f} | {cell(data,'workspace_create_ns')} | {cell(data,'commit_api_ns')} | {cell(data,'complete_lifecycle_ns')} | {cell(data,'process_peak_rss_bytes',1048576)} |")
    lines += ["", "Initialization throughput is logical bytes divided by median initialization wall. Lifecycle excludes initialization and includes Create, historical execution, Commit/visibility acknowledgement and End. This Commit boundary is not identical to the SDK-only edit report's Commit-return boundary. Process lifetime RSS includes initialization; it is not edit-only incremental memory.", "",
              "## Durable Store footprint", "",
              "| Control | N | Logical MB | Durable MiB | Canonical MiB | Initialization ms | Commit ms | Reopen ms | Complete ms | Process lifetime RSS MiB |",
              "| --- | ---: | ---: | --- | --- | --- | --- | --- | --- | --- |"]
    for control in sorted({r["control_id"] for r in st}):
        data = [r for r in st if r["control_id"] == control]
        assert len(data) == 3 and {r["seed"] for r in data} == {1,2,3}
        values = [cell(data,k,1048576 if k.endswith("bytes") else 1000000) for k in ("total_durable_store_bytes","canonical_bytes","initialization_ns","commit_ns","reopen_ns","complete_ns","process_peak_rss_bytes")]
        lines.append(f"| `{control}` | 3 | 500 | " + " | ".join(values) + " |")
    lines += ["", "All persistent Store files are included in durable bytes. SQLite dbstat, census and digest work are outside product timers, but add external harness wall time. The original 600 MB primary footprint target remains a documented limitation, not an achieved target. Far-future storage alternatives in #18 are unscheduled and are not part of this release.", "",
              "## Separate verification", "",
              "| Family | Proofs | Verification ms, median (min–max) | Result |",
              "| --- | ---: | --- | --- |",
              f"| Namespace | {len(nv)} | {cell(nv,'verification_ns')} | pass |",
              f"| Store footprint | {len(sv)} | {cell(sv,'verification_ns')} | pass |", "",
              "Verification times aggregate different controls only to show verifier cost, not product performance. Per-control raw records remain authoritative.", "", "## Raw evidence", ""]
    for path in (namespace,ns_verify,store,store_verify):
        rel=path.relative_to(REPO)
        manifest=path/("environment/evidence.sha256" if path.parent.name == "init_namespace" else "evidence.sha256")
        lines.append(f"- `{rel}`; manifest SHA-256 `{hashlib.sha256(manifest.read_bytes()).hexdigest()}`.")
    (Path(__file__).parent/"supporting-benchmarks.md").write_text("\n".join(lines)+"\n")
    subprocess.run(["git","diff","--exit-code","e978edd19f189d56ca8678bae4dcdc7b6cd4f409","--","crates","Cargo.toml","Cargo.lock"],cwd=REPO,check=True)
    evidence = {"schema":"layerfs-v012-release-refresh-v1", "measured_source":"e978edd19f189d56ca8678bae4dcdc7b6cd4f409",
                "sdk_measured_source":"3337728e9846a200d7a5cc08d076de18f1d5436c", "sdk_performance_samples":560, "sdk_verification_proofs":112,
                "namespace_performance_samples":len(ns), "namespace_verification_proofs":len(nv),
                "store_performance_samples":len(st), "store_verification_proofs":len(sv),
                "legacy_payload":"archival diagnostic; excluded from current release decision by user direction",
                "raw_sha256":{str(p.relative_to(REPO)):hashlib.sha256(p.read_bytes()).hexdigest() for p in (
                    namespace/"performance/raw.jsonl",ns_verify/"verification/raw.jsonl",store/"performance/raw.jsonl",store_verify/"verification/raw.jsonl")}}
    (Path(__file__).parent/"release-evidence.json").write_text(json.dumps(evidence,indent=2)+"\n")


if __name__ == "__main__":
    assert cell([{"x":1_000_000},{"x":3_000_000}],"x") == "2.000 (1.000–3.000)"
    main()

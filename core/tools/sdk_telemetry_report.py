#!/usr/bin/env python3
"""Summarize one #236 route from retained LayerFS LFT1 telemetry records."""
import argparse
import hashlib
import json
from pathlib import Path

PHASES = [
    ("create", 1001, "sdk.sandbox.create", None),
    ("mount", 1002, "sdk.workspace.mount", "WorkspaceOpen"),
    ("exec", 1003, "sdk.workspace.exec", "WorkspaceExec"),
    ("commit", 1004, "sdk.workspace.commit", "WorkspaceCommit"),
    ("unmount", 1005, "sdk.workspace.unmount", "WorkspaceUnmount"),
]


def records(path):
    for line in path.read_text(errors="replace").splitlines():
        _, mark, tail = line.partition("LFT1 ")
        if mark:
            yield json.loads(tail)


def one(events, key=None, name=None):
    matches = [event for event in events if event.get("kind") == "operation"
               and (key is None or event.get("key") == key)
               and event.get("timing") and event["timing"].get("name") == name]
    if len(matches) != 1:
        raise ValueError(f"expected one {name} telemetry event, got {len(matches)}")
    return matches[0]


def metrics(event):
    return {
        "elapsed_ns": event["timing"]["elapsed_ns"],
        "success": event["success"],
        "resource_status": event["resource_status"],
        "cpu_shared_ns": event.get("cpu_shared_ns"),
        "sampled_max_rss_bytes": event.get("sampled_max_rss"),
        "samples": event.get("samples"),
        "gaps": event.get("gaps"),
        "largest_gap_ns": event.get("largest_gap_ns"),
        "first_sample_ns": event.get("first_ns"),
        "last_sample_ns": event.get("last_ns"),
        "opened_ns": event.get("opened_ns"),
        "closed_ns": event.get("closed_ns"),
        "concurrency": event.get("concurrency"),
        "scope": event.get("scope"),
        "source": event.get("source"),
    }


def summarize(root):
    identity = json.loads((root / "identities.json").read_text())
    run = f"{identity['telemetry_run']:032x}"
    host = [event for event in records(root / "raw-host.log")
            if event.get("run") == run and event.get("role") == 1]
    sandbox = (root / "primary-sandbox-id.txt").read_text().strip()
    daemon = [event for event in records(root / f"layerfs-{sandbox}.stderr")
              if event.get("run") == run and event.get("role") == 2]
    route = metrics(one(host, 1000, "sdk.route"))
    lookup_recorded = any(event.get("key") == 2001 for event in host)
    phases = []
    for name, key, host_name, daemon_name in PHASES:
        sdk = one(host, key, host_name)
        phase = {"name": name, "sdk_host_process": metrics(sdk)}
        if lookup_recorded and name != "create":
            start = sdk["opened_ns"]
            within = [event for event in host
                      if start <= event.get("opened_ns", -1) < start + sdk["timing"]["elapsed_ns"]]
            phase["owner_lookup"] = {
                "docker_port": metrics(one(within, 2001, "owner.docker_port")),
                "hello": metrics(one(within, 2002, "owner.hello")),
            }
        if daemon_name:
            phase["daemon_process"] = metrics(one(daemon, name=daemon_name))
        else:
            phase["daemon_process"] = None
        phases.append(phase)
    if not route["success"] or not all(p["sdk_host_process"]["success"]
                                        and (p["daemon_process"] is None or p["daemon_process"]["success"])
                                        for p in phases):
        raise ValueError("one operation failed")
    call_sum = sum(p["sdk_host_process"]["elapsed_ns"] for p in phases)
    if route["elapsed_ns"] < call_sum:
        raise ValueError("route wall is shorter than nested public calls")
    return {
        "kind": "functional-sdk-telemetry-diagnostic",
        "identity": identity,
        "functional_status": "PASS",
        "performance_admission": "INELIGIBLE",
        "admission_eligible": False,
        "cache_contract": "uncontrolled OS/source cache; no cold or warm claim",
        "resource_scope": "LayerFS process-shared sampled CPU/RSS only: host SDK and Service share one process; daemon is separate; Exec child and Docker/container cgroup are excluded",
        "route_host_process": route,
        "phases": phases,
        "public_call_sum_ns": call_sum,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    root = args.output
    report = summarize(root)
    (root / "report.json").open("x").write(json.dumps(report, indent=2, sort_keys=True) + "\n")
    manifest = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                for path in sorted(root.iterdir()) if path.is_file() and path.name != "manifest.json"}
    (root / "manifest.json").open("x").write(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["functional_status"], "admission": report["performance_admission"],
                      "route_elapsed_ns": report["route_host_process"]["elapsed_ns"],
                      "files": len(manifest)}))


if __name__ == "__main__":
    main()

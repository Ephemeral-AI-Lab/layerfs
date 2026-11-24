#!/usr/bin/env python3
"""Test selection for the non-blocking GitHub CI (.github/workflows/).

One place decides which product test binaries run on every push and which wait for
the nightly run. Nothing here blocks a push or merge. The push job builds only
the targets it runs: building, not running, is what its time goes to.

  fast     every default (non-ignored) test except the three nightly groups
  slow     binaries that take 5 s or more to run (macOS host, 2026-10-23)
  heavy    binaries that take 5 s or more of build CPU (moved by the owner,
           2026-10-23)
  mount    daemon targets and the control mount targets that are Linux-only (`#![cfg(target_os =
           "linux")]`): they mount FUSE and need /dev/fuse, SYS_ADMIN and a
           fresh container each (forced_unavailable needs fusectl unmounted)
  nightly  slow and heavy, run together on the nightly runner

Usage (from the repository root):
  ci_tests.py filter fast|nightly    nextest filterset expression
  ci_tests.py targets fast|nightly   `--test NAME` arguments for cargo
  ci_tests.py mount-targets          `--test NAME` arguments for cargo
  ci_tests.py featured               one line `<package>\t<feature>\t--test NAME...`
                                     for each group of targets that are empty
                                     without a cargo feature of their package
  ci_tests.py executables            read `cargo test --no-run --message-format
                                     json` on stdin, print `<dir>\\t<exe>` for
                                     each mount test binary

`--test NAME` selects every package's target of that name. captured_namespace
exists in layerfs-overlay (fast) and layerfs-workspace (heavy), so the push job
also builds the heavy one; the filter keeps it from running.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "Cargo.toml"
LINUX_ONLY = '#![cfg(target_os = "linux")]'
MOUNT_PACKAGE = "layerfs-daemon"
# The management library's daemon mount targets (moved from layerfs-api, #536).
CONTROL_PACKAGE = "layerfs-control"
API_MOUNT_TARGETS = {"kernel_mount_routes", "kernel_application", "observed_application", "r5_1_actual_daemon_commit"}

# (package, test target): run seconds on the macOS host, 2026-10-23.
SLOW = {
    # Native CLI forwarded cancellation, phase 1 verification, 2026-11-09.
    ("layerfs-cli", "cancel_forwarded"): 6.09,
    # Independent overwrite closure and interruption fixture (4096 Commits).
    ("layerfs-persistence", "diagnostic_overwrite"): 15.33,
    ("layerfs-nfs", "helper_deadline"): 60.1,
    # Six Linux FUSE teardown cases, including two bounded Join observations.
    ("layerfs-daemon", "stalled_unmount"): 21.28,
    # Linux native binding proof, owner-excluded from CI; run explicitly.
    ("layerfs-node", "native_admission"): 60.52,
    # Linux native binding proof (#508), run in the runtime image, 2026-10-30.
    ("layerfs-node", "native_connection_wait"): 66.11,
    # Linux SDK mount proof after the ownership move, 2026-10-27.
    ("layerfs-control", "observed_application"): 6.30,
    # Linux functional run at the relocated tree, 2026-10-27.
    ("layerfs-persistence", "writer_admission"): 9.74,
    # Two cases each hold a lane 3 s (finding #20), macOS host, 2026-11-23.
    ("layerfs-persistence", "diagnostics_during_lane"): 6.06,
    # 40 random ancestor-list histories (#758 phase 2), macOS host, 2026-11-23.
    ("layerfs-persistence", "postgresql_chain_property"): 21.83,
    # History scaling stage 1 of 5 (#758 phase 3); stages 2-5 are #[ignore]d
    # and run by name in the order the test file states, macOS host, 2026-11-23.
    ("layerfs-persistence", "postgresql_history_scaling"): 6.05,
    ("layerfs-store-service", "scale_lineage_cap"): 24.3,
    ("layerfs-store-service", "sdk_merge_scale"): 23.8,
    ("layerfs-daemon", "complete_installed_roots"): 9.0,
    ("layerfs-store-service", "sdk_diff_entries"): 8.1,
    ("layerfs-store-service", "history_ancestry_oracle"): 7.6,
    ("layerfs-store-service", "sdk_list_entries"): 7.6,
    ("layerfs-store-service", "history_diff_oracle"): 6.0,
    ("layerfs-store-service", "sdk_text_scan"): 7.0,
    # Inspect admission waits out the 6 s hard deadline (layerfs-project#178), 2026-11-16.
    ("layerfs-sandbox", "inspect_admission"): 11.58,
    # Two requests wait out the 4 s soft budget (layerfs-project#164), 2026-11-16.
    ("layerfs-sandbox", "inspect_stats"): 8.30,
    # Marks of 1.1 million objects and of 20,000 packs through sort files
    # (layerfs-project#41), 2026-11-18.
    ("layerfs-persistence", "kept_objects_sort_files"): 24.54,
    # History closures of 9,000 Commits and 24,000 Branches against the one
    # recursive statement (layerfs-project#41), 2026-11-18.
    ("layerfs-persistence", "kept_node_closure"): 5.32,
    # A provider call waits out its own 10 s and 30 s bounds (cp-platform
    # fix of tool::run's deadline), estimated from those bounds, 2026-11-23.
    ("layerfs-platform", "machine_deadline"): 31.0,
    # #768 adversarial Commit tests against the private PostgreSQL test
    # server (each skips its PostgreSQL half without one), macOS host,
    # 2026-11-25: a fault at every server message of one Commit's flight.
    # Re-measured on stage 2b (fused Commit), slowest of three runs; the
    # raced sweeps run four shares of the flight in parallel.
    ("layerfs-store-service", "commit_fault_drop"): 16.0,
    ("layerfs-store-service", "commit_fault_kill"): 16.7,
    ("layerfs-store-service", "commit_fault_first_drop"): 25.2,
    ("layerfs-store-service", "commit_fault_first_kill"): 26.5,
    ("layerfs-store-service", "commit_fault_race_drop"): 37.4,
    ("layerfs-store-service", "commit_fault_race_kill"): 40.0,
    ("layerfs-store-service", "commit_lane_kill"): 15.4,
    ("layerfs-store-service", "commit_random_model"): 8.4,
    ("layerfs-store-service", "commit_stress"): 6.7,
    # Fused Commits of 8,000 new files and of 400 rewrites over a 20,000-file
    # seed on both backends (#768 stage 2b review), macOS host, 2026-11-25.
    ("layerfs-store-service", "commit_fused_held"): 45.1,
}

# (package, test target): build CPU seconds in the cold mold build of CI run
# 39924955411 (4-CPU runner, test profile without debuginfo).
HEAVY = {
    ("layerfs-control", "mount_control"): 20.6,
    ("layerfs-daemon", "store_commit"): 19.1,
    ("layerfs-workspace", "captured_namespace_cursor"): 9.3,
    ("layerfs-workspace", "captured_namespace_holds"): 9.0,
    ("layerfs-workspace", "captured_namespace_perturbed"): 9.0,
    ("layerfs-workspace", "captured_namespace_custody"): 9.0,
    ("layerfs-workspace", "captured_namespace_states"): 8.4,
    ("layerfs-workspace", "captured_namespace_rebound"): 8.3,
    ("layerfs-workspace", "captured_namespace_order"): 7.2,
    ("layerfs-workspace", "captured_namespace_fragments"): 6.5,
    ("layerfs-workspace", "captured_namespace"): 6.4,
    ("layerfs-workspace", "directory_links_install"): 6.0,
    ("layerfs-workspace", "captured_namespace_metadata"): 6.0,
    ("layerfs-workspace", "captured_namespace_inline"): 5.7,
    ("layerfs-project", "backed_acquisition"): 5.5,
    ("layerfs-project", "complete_root"): 5.1,
    ("layerfs-content", "filesystem_sorted_localized"): 5.0,
}

# (package, test target): the cargo feature of that package the target needs. Without
# it the target builds empty (`#![cfg(feature = ...)]`), so both lanes pass over it;
# the nightly run builds and runs each group alone, with only that package's feature
# on, so no other binary is ever built with it. `worker-fault` is the worker's fault
# switches (layerfs-project#177): run seconds on the macOS host, 2026-11-16.
FEATURED = {
    ("layerfs-control", "worker_fault"): ("worker-fault", 4.87),
    ("layerfs-control", "worker_fault_store"): ("worker-fault", 3.26),
    ("layerfs-control", "worker_fault_inspect"): ("worker-fault", 19.16),
}

# SDK timings are recorded above without adding the owner-excluded API to CI.
# stalled_unmount's timing is recorded above; it runs in the mount lane.
NIGHTLY = {
    target
    for target in set(SLOW) | set(HEAVY)
    if target[0] != "layerfs-node"
    and not (target[0] == CONTROL_PACKAGE and target[1] in API_MOUNT_TARGETS)
    and target != (MOUNT_PACKAGE, "stalled_unmount")
}


def metadata():
    out = subprocess.run(
        ["cargo", "+1.85.1", "metadata", "--manifest-path", str(MANIFEST), "--no-deps", "--format-version", "1"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def test_targets(meta):
    """Yield (package, target, source path) for every integration test target."""
    for package in meta["packages"]:
        # The owner leaves the JavaScript API out of both CI lanes.
        if package["name"] == "layerfs-node":
            continue
        for target in package["targets"]:
            if "test" in target["kind"]:
                yield package["name"], target["name"], Path(target["src_path"])


def mount_targets(meta):
    found = []
    for package, name, source in test_targets(meta):
        if package == CONTROL_PACKAGE and name in API_MOUNT_TARGETS:
            found.append((package, name))
            continue
        if package != MOUNT_PACKAGE:
            continue
        if LINUX_ONLY in source.read_text().splitlines():
            found.append((package, name))
    return sorted(found)


def fast_targets(meta):
    left_out = NIGHTLY | set(mount_targets(meta))
    return sorted({(package, name) for package, name, _ in test_targets(meta)} - left_out)


def missing_listed(meta):
    known = {(package, name) for package, name, _ in test_targets(meta)}
    return sorted((NIGHTLY | set(FEATURED)) - known)


def featured_groups():
    """Yield (package, feature, `--test NAME` flags) for each package and feature."""
    groups = {}
    for (package, name), (feature, _) in FEATURED.items():
        groups.setdefault((package, feature), set()).add((package, name))
    for (package, feature), pairs in sorted(groups.items()):
        yield package, feature, test_flags(pairs)


def ids(pairs):
    return " | ".join(f"binary_id({package}::{name})" for package, name in sorted(pairs))


def lane_targets(lane, meta):
    if lane == "fast":
        return fast_targets(meta)
    if lane == "nightly":
        return sorted(NIGHTLY)
    raise SystemExit(f"unknown lane {lane!r}: use fast or nightly")


def filter_for(lane, meta):
    if lane == "nightly":
        return ids(NIGHTLY)
    if lane == "fast":
        return f"not ({ids(NIGHTLY | set(mount_targets(meta)))})"
    raise SystemExit(f"unknown lane {lane!r}: use fast or nightly")


def test_flags(pairs):
    return " ".join(f"--test {name}" for name in sorted({name for _, name in pairs}))


def executables(lines, mount, meta):
    wanted = set(mount)
    names = {Path(p["manifest_path"]): p["name"] for p in meta["packages"]}
    for line in lines:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        target = message["target"]
        if "test" not in target["kind"]:
            continue
        manifest = Path(message["manifest_path"])
        if (names.get(manifest), target["name"]) in wanted:
            yield manifest.parent, message["executable"]


def main(argv):
    if not argv:
        raise SystemExit(__doc__)
    meta = metadata()
    gone = missing_listed(meta)
    if gone:
        raise SystemExit(f"SLOW or HEAVY names targets that no longer exist: {gone}")
    if argv[0] == "filter" and len(argv) == 2:
        print(filter_for(argv[1], meta))
    elif argv[0] == "targets" and len(argv) == 2:
        print(test_flags(lane_targets(argv[1], meta)))
    elif argv[0] == "mount-targets":
        print(test_flags(mount_targets(meta)))
    elif argv[0] == "featured":
        for package, feature, flags in featured_groups():
            print(f"{package}\t{feature}\t{flags}")
    elif argv[0] == "executables":
        for directory, exe in executables(sys.stdin, mount_targets(meta), meta):
            print(f"{directory}\t{exe}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])

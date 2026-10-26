#!/usr/bin/env python3
"""Test selection for the non-blocking GitHub CI (.github/workflows/).

One place decides which product test binaries run on every push and which wait for
the nightly run. Nothing here blocks a push or merge. The push job builds only
the targets it runs: building, not running, is what its time goes to.

  fast     every default (non-ignored) test except the three nightly groups
  slow     binaries that take 5 s or more to run (macOS host, 2026-10-23)
  heavy    binaries that take 5 s or more of build CPU (moved by the owner,
           2026-10-23)
  mount    daemon targets and the SDK mount targets that are Linux-only (`#![cfg(target_os =
           "linux")]`): they mount FUSE and need /dev/fuse, SYS_ADMIN and a
           fresh container each (forced_unavailable needs fusectl unmounted)
  nightly  slow and heavy, run together on the nightly runner

Usage (from the repository root):
  ci_tests.py filter fast|nightly    nextest filterset expression
  ci_tests.py targets fast|nightly   `--test NAME` arguments for cargo
  ci_tests.py mount-targets          `--test NAME` arguments for cargo
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
API_MOUNT_TARGETS = {"kernel_mount_routes", "kernel_application", "observed_application", "r5_1_actual_daemon_commit"}

# (package, test target): run seconds on the macOS host, 2026-10-23.
SLOW = {
    # Linux native binding proof, owner-excluded from CI; run explicitly.
    ("layerfs-node", "native_admission"): 60.52,
    # Linux SDK mount proof after the ownership move, 2026-10-27.
    ("layerfs-api", "observed_application"): 6.30,
    # Linux functional run at the relocated tree, 2026-10-27.
    ("layerfs-persistence", "writer_admission"): 9.74,
    ("layerfs-api", "scale_lineage_cap"): 24.3,
    ("layerfs-api", "sdk_merge_scale"): 23.8,
    ("layerfs-daemon", "complete_installed_roots"): 9.0,
    ("layerfs-api", "sdk_diff_entries"): 8.1,
    ("layerfs-api", "history_ancestry_oracle"): 7.6,
    ("layerfs-api", "sdk_list_entries"): 7.6,
    ("layerfs-api", "history_diff_oracle"): 6.0,
    ("layerfs-api", "sdk_text_scan"): 5.9,
}

# (package, test target): build CPU seconds in the cold mold build of CI run
# 39924955411 (4-CPU runner, test profile without debuginfo).
HEAVY = {
    ("layerfs-api", "mount_control"): 20.6,
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

# SDK timings are recorded above without adding the owner-excluded API to CI.
NIGHTLY = {
    target
    for target in set(SLOW) | set(HEAVY)
    if target[0] != "layerfs-node" and not (target[0] == "layerfs-api" and target[1] in API_MOUNT_TARGETS)
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
        if package == "layerfs-api" and name in API_MOUNT_TARGETS:
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
    return sorted(NIGHTLY - known)


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
    elif argv[0] == "executables":
        for directory, exe in executables(sys.stdin, mount_targets(meta), meta):
            print(f"{directory}\t{exe}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])

#!/usr/bin/env python3
"""Test selection for the non-blocking GitHub CI (.github/workflows/).

One place decides which core test binaries run on every push and which wait for
the nightly run. Nothing here blocks a push or merge.

  fast   every default (non-ignored) test except the two nightly groups
  slow   binaries measured at 5 s or more (macOS host, 2026-10-23, one sample)
  mount  layerfs-daemon targets that are Linux-only (`#![cfg(target_os =
         "linux")]`): they mount FUSE and need /dev/fuse, SYS_ADMIN and a
         fresh container each (forced_unavailable needs fusectl unmounted)

Usage (from the repository root):
  ci_tests.py filter fast|slow    nextest filterset expression
  ci_tests.py mount-targets       `--test NAME` arguments for cargo
  ci_tests.py executables         read `cargo test --no-run --message-format
                                  json` on stdin, print `<dir>\\t<exe>` for each
                                  mount test binary
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "core" / "Cargo.toml"
LINUX_ONLY = '#![cfg(target_os = "linux")]'
MOUNT_PACKAGE = "layerfs-daemon"

# (package, test target): seconds on the macOS host, 2026-10-23.
SLOW = {
    ("layerfs-api", "scale_lineage_cap"): 24.3,
    ("layerfs-api", "sdk_merge_scale"): 23.8,
    ("layerfs-daemon", "complete_installed_roots"): 9.0,
    ("layerfs-api", "sdk_diff_entries"): 8.1,
    ("layerfs-api", "history_ancestry_oracle"): 7.6,
    ("layerfs-api", "sdk_list_entries"): 7.6,
    ("layerfs-api", "history_diff_oracle"): 6.0,
    ("layerfs-api", "sdk_text_scan"): 5.9,
}


def metadata():
    out = subprocess.run(
        ["cargo", "+1.85.1", "metadata", "--manifest-path", str(MANIFEST),
         "--no-deps", "--format-version", "1"],
        check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def test_targets(meta):
    """Yield (package, target, source path) for every integration test target."""
    for package in meta["packages"]:
        for target in package["targets"]:
            if "test" in target["kind"]:
                yield package["name"], target["name"], Path(target["src_path"])


def mount_targets(meta):
    found = []
    for package, name, source in test_targets(meta):
        if package != MOUNT_PACKAGE:
            continue
        if LINUX_ONLY in source.read_text().splitlines():
            found.append((package, name))
    return sorted(found)


def missing_slow(meta):
    known = {(package, name) for package, name, _ in test_targets(meta)}
    return sorted(set(SLOW) - known)


def ids(pairs):
    return " | ".join(f"binary_id({package}::{name})" for package, name in sorted(pairs))


def filter_for(lane, meta):
    if lane == "slow":
        return ids(SLOW)
    if lane == "fast":
        return f"not ({ids(set(SLOW) | set(mount_targets(meta)))})"
    raise SystemExit(f"unknown lane {lane!r}: use fast or slow")


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
    gone = missing_slow(meta)
    if gone:
        raise SystemExit(f"SLOW names targets that no longer exist: {gone}")
    if argv[0] == "filter" and len(argv) == 2:
        print(filter_for(argv[1], meta))
    elif argv[0] == "mount-targets":
        print(" ".join(f"--test {name}" for _, name in mount_targets(meta)))
    elif argv[0] == "executables":
        for directory, exe in executables(sys.stdin, mount_targets(meta), meta):
            print(f"{directory}\t{exe}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])

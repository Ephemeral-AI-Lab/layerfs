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
                                     json` on stdin, print `<dir>\\t<exe>\\t<filter>`
                                     for each mount run: a one-file binary, or one
                                     module of an area binary

A key is `(package, binary)` or, for one module of an area binary that joins
several test files (`tests/<area>/main.rs`, #828), `(package, binary, module)`.
A module key leaves its binary in the push job's build and filters the module's
tests by name (`test(/^module::/)`); the mount lane runs each module of an area
binary on its own (libtest filter `module::`), one container and 120 s cap each.

`--test NAME` selects every package's target of that name. A name shared by two
packages (for example `postgresql` in layerfs-persistence and
layerfs-store-service) is built for both; the filter keeps each lane to its own.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "Cargo.toml"
LINUX_ONLY = '#![cfg(target_os = "linux")]'
MOUNT_PACKAGE = "layerfs-daemon"
# The control layer's daemon mount targets (moved from layerfs-api, #536; from
# layerfs-control to layerfs-sandbox, r5_1_actual_daemon_commit renamed
# kernel_commit, #828).
CONTROL_PACKAGE = "layerfs-sandbox"
API_MOUNT_TARGETS = {"kernel_mount_routes", "kernel_application", "observed_application", "kernel_commit"}

# (package, test target): run seconds on the macOS host, 2026-10-23.
SLOW = {
    # Native CLI forwarded cancellation, phase 1 verification, 2026-11-09.
    ("layerfs-cli", "exec_cancel_forwarded"): 6.09,
    # Independent overwrite closure and interruption fixture (4096 Commits).
    ("layerfs-persistence", "diagnostic", "overwrite"): 15.33,
    ("layerfs-nfs", "helper_deadline"): 60.1,
    # Six Linux FUSE teardown cases, including two bounded Join observations.
    ("layerfs-daemon", "unmount_stalled"): 21.28,
    # Linux native binding proof, owner-excluded from CI; run explicitly.
    ("layerfs-node", "native_admission"): 60.52,
    # Linux native binding proof (#508), run in the runtime image, 2026-10-30.
    ("layerfs-node", "native_connection_wait"): 66.11,
    # Linux SDK mount proof after the ownership move, 2026-10-27.
    ("layerfs-sandbox", "observed_application"): 6.30,
    # Linux functional run at the relocated tree, 2026-10-27.
    ("layerfs-persistence", "sqlite_session", "writer_admission"): 9.74,
    # Two cases each hold a lane 3 s (finding #20), macOS host, 2026-11-23.
    ("layerfs-persistence", "sqlite_session", "counters_during_lane"): 6.06,
    # 40 random ancestor-list histories (#758 phase 2), macOS host, 2026-11-23.
    ("layerfs-persistence", "postgresql_metadata", "chain_property"): 21.83,
    # History scaling stage 1 of 5 (#758 phase 3); stages 2-5 are #[ignore]d
    # and run by name in the order the test file states, macOS host, 2026-11-23.
    ("layerfs-persistence", "postgresql", "history_scaling"): 6.05,
    ("layerfs-store-service", "history", "scale_lineage_cap"): 24.3,
    ("layerfs-store-service", "merge", "scale"): 23.8,
    ("layerfs-daemon", "install", "complete_installed_roots"): 9.0,
    ("layerfs-store-service", "compare", "diff_entries"): 8.1,
    ("layerfs-store-service", "history", "ancestry_oracle"): 7.6,
    ("layerfs-store-service", "filesystem", "list_entries"): 7.6,
    ("layerfs-store-service", "compare", "history_diff_oracle"): 6.0,
    ("layerfs-store-service", "filesystem", "text_scan"): 7.0,
    # Inspect admission waits out the 6 s hard deadline (layerfs-project#178), 2026-11-16.
    ("layerfs-inspect", "inspect_admission"): 11.58,
    # Two requests wait out the 4 s soft budget (layerfs-project#164), 2026-11-16.
    ("layerfs-inspect", "machine_stats"): 8.30,
    # Marks of 1.1 million objects and of 20,000 packs through sort files
    # (layerfs-project#41), 2026-11-18.
    ("layerfs-persistence", "gc_mark", "kept_objects_sort_files"): 24.54,
    # History closures of 9,000 Commits and 24,000 Branches against the one
    # recursive statement (layerfs-project#41), 2026-11-18.
    ("layerfs-persistence", "gc_mark", "kept_node_closure"): 5.32,
    # A provider call waits out its own 10 s and 30 s bounds (cp-platform
    # fix of tool::run's deadline), estimated from those bounds, 2026-11-23.
    ("layerfs-machine", "setup_tool_deadline"): 31.0,
    # #768 adversarial Commit tests against the private PostgreSQL test
    # server (each skips its PostgreSQL half without one), macOS host,
    # 2026-11-25: a fault at every server message of one Commit's flight.
    # Re-measured on stage 2b (fused Commit), slowest of three runs; the
    # raced sweeps run four shares of the flight in parallel.
    ("layerfs-store-service", "commit_fault", "drop"): 16.0,
    ("layerfs-store-service", "commit_fault", "kill"): 16.7,
    ("layerfs-store-service", "commit_fault", "first_drop"): 25.2,
    ("layerfs-store-service", "commit_fault", "first_kill"): 26.5,
    ("layerfs-store-service", "commit_fault", "race_drop"): 37.4,
    ("layerfs-store-service", "commit_fault", "race_kill"): 40.0,
    # The raced flight's fused replies after its refusal and the fallback's
    # writer messages, dropped and killed (#768 verification fixes).
    ("layerfs-store-service", "commit_fault", "race_custody"): 22.2,
    # #816 captured Save publication, all 34 messages per fault mode.
    ("layerfs-persistence", "postgresql", "captured_save_fault"): 19.10,
    # #818 Phase 0 round-trip pins, both backends, macOS with PostgreSQL,
    # 2026-11-27 (the probe fs_call_round_trips runs only with FS818_ARM).
    ("layerfs-store-service", "round_trips", "init_10k"): 38.8,
    ("layerfs-store-service", "history_round_trips", "counts"): 9.7,
    # #810 read batching pins and fault sweeps, macOS with PostgreSQL.
    ("layerfs-store-service", "commit", "batched_base_files"): 8.48,
    ("layerfs-store-service", "commit", "batched_reads"): 21.07,
    ("layerfs-store-service", "commit_fault", "directories"): 50.84,
    ("layerfs-store-service", "commit_fault_wide", "drop_0"): 39.81,
    ("layerfs-store-service", "commit_fault_wide", "drop_1"): 35.75,
    ("layerfs-store-service", "commit_fault_wide", "drop_2"): 44.07,
    ("layerfs-store-service", "commit_fault_wide", "drop_3"): 32.32,
    ("layerfs-store-service", "commit_fault_wide", "kill_0"): 33.89,
    ("layerfs-store-service", "commit_fault_wide", "kill_1"): 34.34,
    ("layerfs-store-service", "commit_fault_wide", "kill_2"): 42.82,
    ("layerfs-store-service", "commit_fault_wide", "kill_3"): 46.57,
    ("layerfs-store-service", "commit_race", "lane_kill"): 15.4,
    ("layerfs-store-service", "commit_model", "random_model"): 8.4,
    ("layerfs-store-service", "commit_model", "stress"): 6.7,
    # Fused Commits of 8,000 new files and of 400 rewrites over a 20,000-file
    # seed on both backends (#768 stage 2b review), macOS host, 2026-11-25.
    ("layerfs-store-service", "commit_fused", "held"): 45.1,
    # A 4,200-file Commit refused after its hints spilled, both backends
    # (#768 verification fixes), macOS host, 2026-11-25.
    ("layerfs-store-service", "commit_fused", "hint_spill"): 16.4,
    # Fused-versus-forced size-boundary matrix on both backends (#768 stage
    # 2b), slowest of the a36e9b997c runs, macOS host, 2026-11-25.
    ("layerfs-store-service", "commit_fused", "sizes"): 38.0,
    ("layerfs-store-service", "commit_fused", "sizes_bytes"): 23.5,
    ("layerfs-store-service", "commit_fused", "sizes_moves"): 47.0,
    ("layerfs-store-service", "commit_fused", "sizes_rounds"): 22.5,
    ("layerfs-store-service", "commit_fused_sizes_seeded"): 60.6,
    # Independent verification suite of the fused Commit (#768), serial runs on
    # the macOS host, 2026-11-25.
    # Split into nine binaries by the writer reopen's fix round (warm halves,
    # cold quarters, raced ninths), a direct Commit every fifth run of a part
    # and the audit deadline scaled by located objects (review round 2),
    # --test-threads 3, macOS host at a 1-minute load of 3-7 on 14 cores,
    # 2026-11-26.
    ("layerfs-store-service", "allocators", "unknown"): 38.1,
    ("layerfs-store-service", "allocators", "warm_b"): 36.8,
    ("layerfs-store-service", "allocators", "cold_a"): 31.0,
    ("layerfs-store-service", "allocators", "cold_b"): 30.2,
    ("layerfs-store-service", "allocators", "cold_c"): 31.0,
    ("layerfs-store-service", "allocators", "cold_d"): 30.3,
    ("layerfs-store-service", "allocators", "raced_a"): 38.5,
    ("layerfs-store-service", "allocators", "raced_b"): 38.3,
    ("layerfs-store-service", "allocators", "raced_c"): 38.4,
    ("layerfs-store-service", "commit_abort_paused_commit_sweeps"): 69.5,
    ("layerfs-store-service", "commit_abort", "paused_commit_sweeps_third_a"): 45.6,
    ("layerfs-store-service", "commit_abort", "paused_commit_sweeps_third_b"): 45.9,
    ("layerfs-store-service", "ids", "generation_and_kept_ids"): 29.7,
    ("layerfs-store-service", "commit_model", "differential_faults"): 16.1,
    ("layerfs-store-service", "commit_abort", "fused_injected_aborts"): 10.8,
    ("layerfs-store-service", "commit_model", "history_readback"): 10.0,
    ("layerfs-store-service", "commit_model", "two_daemons_one_branch"): 7.7,
    ("layerfs-store-service", "commit_model", "edge_commits"): 7.3,
    ("layerfs-store-service", "commit_abort", "fallback_custody"): 23.9,
    ("layerfs-store-service", "ids", "ordinal_cycles"): 11.2,
    ("layerfs-store-service", "ids", "two_services_rounds"): 55.2,
    # Third independent #768 round at 02059c5435, --test-threads 3, macOS host,
    # 2026-11-25. failed_save ran in three bounded invocations (its two
    # 6,000-file PostgreSQL halves alone); the figure is their sum.
    ("layerfs-store-service", "ids", "interleavings"): 33.5,
    ("layerfs-store-service", "commit_abort", "release_faults"): 46.6,
    ("layerfs-store-service", "commit_abort_failed_save"): 192.5,
    # #768 review fixes, --test-threads 3, macOS host, 2026-11-25: the idle
    # reader's check waits its 5 s cap once.
    ("layerfs-store-service", "postgresql", "reader_idle"): 6.6,
    # The independent #768 tester's binaries, same host and day.
    ("layerfs-store-service", "postgresql", "stale_reader"): 9.1,
    ("layerfs-store-service", "postgresql", "interleave"): 7.0,
    # The independent writer-reopen tester's binaries, --test-threads 3,
    # macOS host at a 1-minute load of 13-25, 2026-11-26 (the reopen sweep is
    # four half-flight tests).
    ("layerfs-store-service", "writer_reopen_sweep"): 86.4,
    ("layerfs-store-service", "writer_reopen", "resolve_sweep"): 20.2,
    ("layerfs-store-service", "writer_reopen", "races"): 14.9,
    # The second writer-reopen round's audit measurement (one warm half-flight
    # sweep, then the audit), same host, load 5-6, 2026-11-26. Its other
    # binaries take under 5 s.
    ("layerfs-store-service", "writer_reopen", "audit_cost"): 38.2,
    # `store upgrade` on the PostgreSQL fixture, one restore per case, and
    # the cut at each of its 42 server messages (#804), macOS host, 2026-11-25.
    ("layerfs-persistence", "upgrade", "postgresql"): 5.0,
    ("layerfs-persistence", "upgrade", "postgresql_wire"): 12.16,
}

# (package, test target): build CPU seconds in the cold mold build of CI run
# 39924955411 (4-CPU runner, test profile without debuginfo).
HEAVY = {
    ("layerfs-sandbox", "mount_control"): 20.6,
    ("layerfs-daemon", "store", "commit"): 19.1,
    # #828 joined eleven captured_namespace* binaries of layerfs-workspace
    # (9.3, 9.0, 9.0, 9.0, 8.4, 8.3, 7.2, 6.5, 6.4, 6.0, 5.7) into one: their sum,
    # an upper bound until the next CI run measures it.
    ("layerfs-vfs", "captured_namespace"): 84.8,
    # directory_links_install (6.0) is a module of the namespace binary (#828).
    ("layerfs-vfs", "namespace"): 6.0,
    ("layerfs-project", "acquisition", "backed"): 5.5,
    ("layerfs-project", "init", "complete_root"): 5.1,
    ("layerfs-content", "filesystem_pages_localized"): 5.0,
}

# (package, test target): the cargo feature of that package the target needs. Without
# it the target builds empty (`#![cfg(feature = ...)]`), so both lanes pass over it;
# the nightly run builds and runs each group alone, with only that package's feature
# on, so no other binary is ever built with it. `worker-fault` is the worker's fault
# switches (layerfs-project#177): run seconds on the macOS host, 2026-11-16, of the
# three binaries (4.87, 3.26 and 19.16) #828 joined into layerfs-api's one.
FEATURED = {
    ("layerfs-api", "worker_fault"): ("worker-fault", 27.29),
}

# SDK timings are recorded above without adding the owner-excluded API to CI.
# unmount_stalled's timing is recorded above; it runs in the mount lane.
NIGHTLY = {
    target
    for target in set(SLOW) | set(HEAVY)
    if target[0] != "layerfs-node"
    and not (target[0] == CONTROL_PACKAGE and target[1] in API_MOUNT_TARGETS)
    and target != (MOUNT_PACKAGE, "unmount_stalled")
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


def modules(main):
    """Yield (module path, file) for the modules of an area binary's `main.rs`:
    its sibling `<name>.rs` files and `<name>/mod.rs` folders."""
    folder = main.parent
    for file in sorted(folder.glob("*.rs")):
        if file != main:
            yield file.stem, file
    for file in sorted(folder.glob("*/mod.rs")):
        yield file.parent.name, file


def mount_targets(meta):
    """Mount keys: a whole binary whose root is Linux-only, or, in an area binary
    that joins portable and Linux-only modules (#828), each Linux-only module."""
    found = []
    for package, name, source in test_targets(meta):
        if package == CONTROL_PACKAGE and name in API_MOUNT_TARGETS:
            found.append((package, name))
            continue
        if package != MOUNT_PACKAGE:
            continue
        if LINUX_ONLY in source.read_text().splitlines():
            found.append((package, name))
        elif source.name == "main.rs":
            for module, file in modules(source):
                if LINUX_ONLY in file.read_text().splitlines():
                    found.append((package, name, module))
    return sorted(found)


def fast_targets(meta):
    whole = {key for key in NIGHTLY | set(mount_targets(meta)) if len(key) == 2}
    return sorted({(package, name) for package, name, _ in test_targets(meta)} - whole)


def missing_listed(meta):
    """Listed keys whose binary, or whose module in it, no longer exists."""
    sources = {(package, name): source for package, name, source in test_targets(meta)}
    gone = []
    for key in NIGHTLY | set(FEATURED):
        source = sources.get(key[:2])
        if source is None:
            gone.append(key)
        elif len(key) == 3:
            path = source.parent / key[2].replace("::", "/")
            if not (path.with_suffix(".rs").exists() or (path / "mod.rs").exists()):
                gone.append(key)
    return sorted(gone)


def featured_groups():
    """Yield (package, feature, `--test NAME` flags) for each package and feature."""
    groups = {}
    for key, (feature, _) in FEATURED.items():
        groups.setdefault((key[0], feature), set()).add(key[:2])
    for (package, feature), pairs in sorted(groups.items()):
        yield package, feature, test_flags(pairs)


def ident(key):
    """A nextest filterset for one key: a binary, or one module of it."""
    whole = f"binary_id({key[0]}::{key[1]})"
    return whole if len(key) == 2 else f"({whole} & test(/^{key[2]}::/))"


def ids(keys):
    return " | ".join(ident(key) for key in sorted(keys))


def lane_targets(lane, meta):
    if lane == "fast":
        return fast_targets(meta)
    if lane == "nightly":
        return sorted({key[:2] for key in NIGHTLY})
    raise SystemExit(f"unknown lane {lane!r}: use fast or nightly")


def filter_for(lane, meta):
    if lane == "nightly":
        return ids(NIGHTLY)
    if lane == "fast":
        return f"not ({ids(NIGHTLY | set(mount_targets(meta)))})"
    raise SystemExit(f"unknown lane {lane!r}: use fast or nightly")


def test_flags(keys):
    return " ".join(f"--test {name}" for name in sorted({key[1] for key in keys}))


def executables(lines, mount, meta):
    """Yield (directory, executable, libtest filter) runs, each under its own cap.

    A one-file binary listed whole runs every test in one run. An area binary
    (`tests/<area>/main.rs`, #828) runs one module per run, whether it is listed
    whole or by module: its modules were one binary each before #828, and each
    keeps the 120 s cap and the fresh container it had then."""
    wanted = {}
    for key in mount:
        wanted.setdefault(key[:2], set())
        if len(key) == 3:
            wanted[key[:2]].add(key[2])
    whole = {key[:2] for key in mount if len(key) == 2}
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
        binary = (names.get(manifest), target["name"])
        if binary not in wanted:
            continue
        source = Path(target.get("src_path", ""))
        if binary in whole and source.name == "main.rs":
            selected = sorted(module for module, _ in modules(source))
        elif binary in whole:
            yield manifest.parent, message["executable"], ""
            continue
        else:
            selected = sorted(wanted[binary])
        for module in selected:
            yield manifest.parent, message["executable"], module_filter(module, selected)


def module_filter(module, siblings):
    """The libtest filter that selects module `module`'s tests alone. libtest
    filters by substring, so `module::` must not also match a sibling."""
    clash = [other for other in siblings if other != module and other.endswith(module)]
    if clash:
        raise SystemExit(f"mount module filter {module}:: also selects {clash}")
    return f"{module}::"


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
        for directory, exe, filters in executables(sys.stdin, mount_targets(meta), meta):
            print(f"{directory}\t{exe}\t{filters}")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])

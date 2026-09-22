#!/usr/bin/env python3
"""Host-owned SQLite benchmarks; Docker runs daemon/FUSE/workloads only."""
from __future__ import annotations

import time
ENTRY_STARTED_NS = time.monotonic_ns()

import argparse
import fcntl
import contextlib
import hashlib
import json
import math
import os
import platform
import re
import shutil
from pathlib import Path
import statistics
import tempfile
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
REPO = BENCH.parent.parent
sys.path.insert(0, str(HERE))
import runtime
import cold
import isolation

HOST_FAMILIES = ("payload_create_read", "dedup_workspace_reuse", "dedup_cross_file", "dedup_cdc_locality",
                 "edit_length_preserving", "edit_length_changing", "edit_canonical_chunk_count",
                 "init_namespace", "store_footprint", "tiny_file_churn",
                 "namespace_mutation", "directory_construction_traversal",
                 "workspace_change_locality", "dedup_branch_history", "git_tool_workflow",
                 "mixed_load_bearing", "workspace_reliability", "file_size_transition",
                 "multi_workspace_development", "branch_development", "historical_access",
                 "local_snapshot")
PRODUCT_TARGET_NS = 15_000_000_000
HISTORICAL_PRODUCT_TARGET_SCOPE = (
    "reporting-only historical 15-second family target; not a collection acceptance gate"
)
# One owned writable cargo cache per toolchain/profile (#117). Cargo's own
# fingerprints decide what is rebuilt; immutable per-seal executables stay in
# binary-archive/ and every build receipt names the packages that recompiled.
INCREMENTAL_TARGET_NAME = "incremental-1.85.1-release"
BUILD_MODES = ("incremental", "sealed")
DEFAULT_RETAINED_SEALED_BUILDS = 2


def performance_target_status(elapsed_ns):
    return "PASS" if elapsed_ns <= PRODUCT_TARGET_NS else "TARGET_MISS"


def verification_policy(selection):
    sequence = selection.get("sequence") or {}
    scaled = (selection.get("family") == "init_namespace"
              and sequence.get("schema") == "workspace-sequence-v1"
              and sequence.get("edit_count", 0) * sequence.get("commits", 0) > 1000)
    if v016_verify_watchdog_seconds(selection) is not None:
        # The v0.1.6 M1 cases declare their own verification ceiling: the
        # extended case's own fixed watchdog, and the unchanged 25-second regular
        # ceiling. The 15-second family target stays reported separately.
        declared = v016_verify_watchdog_seconds(selection)
        return {"id": "v016-selected-verification-v1",
                "work_limit_seconds": float(declared), "hard_limit_seconds": float(declared) + 10.0,
                "cleanup_reserve_seconds": 3.0, "publication_guard_seconds": 0.25,
                "declared_complete_deadline_seconds": declared}
    return {"id": "workspace-sequence-scaling-v1" if scaled else "selected-verification-v1",
            "work_limit_seconds": 600.0 if scaled else 45.0,
            "hard_limit_seconds": 614.0 if scaled else 59.0,
            "cleanup_reserve_seconds": 4.0, "publication_guard_seconds": 0.25}


# The complete-command deadline of each v0.1.6 case, in seconds. The three
# explicit extended cases carry their own frozen watchdog; every regular case
# carries the 15-second regular deadline. Keyed by the exact registered ID so a
# renamed or unregistered case is never silently admitted.
V016_EXTENDED_WATCHDOGS = {
    "v016-mixed-exhaustive-100mb-5000-k100-v1": 120,
    "v016-mixed-exhaustive-500mb-30000-k100-v1": 300,
    "v016-workspace-four-100mb-5000-k100-v1": 60,
}
# Owner ruling on #154 (2026-09-16): the declared complete-command allowance for
# a regular v0.1.6 invocation is 60 seconds. This is an *execution allowance*, not
# a redefinition of the family target: the 15-second regular target stays reported
# separately (`historical_product_target_status` / `performance_target_status`),
# and a row that finishes above it is still a TARGET_MISS. The three-second
# cleanup reserve lives inside the allowance, so the worker stop is 57 seconds.
# The measured wall of every invocation is reported either way.
V016_REGULAR_DEADLINE_SECONDS = 60
# The verification gate is unchanged by that ruling: a regular v0.1.6 verification
# still carries the pre-existing 25-second complete-command ceiling (three-second
# cleanup reserve inside it). Making a verification fit is a job for removing
# redundant work in the verifier, never for this number.
V016_VERIFY_DEADLINE_SECONDS = 25
# Owner ruling on #154 (2026-09-16, second ruling on this gate): *one* regular
# verification row is declared an exception above that ceiling. Its measured wall
# (22.99 s complete command, of which 19.13 s is the product command: ~11.4 s for
# the 210-commit replay and ~7.7 s of proof) is recorded in the ledger and the
# group report, and the row is reported as a declared exception — never as a
# silent watchdog change. Everything else is untouched: the 15-second family
# target is still reported separately, the three-second cleanup reserve still sits
# inside the ceiling, the verifier's oracle and coverage are unchanged, and every
# other regular verification keeps 25 seconds. Keyed by the exact registered ID so
# a renamed or unregistered case is never silently admitted.
V016_VERIFY_EXCEPTIONS = {
    "v016-branch-mixed-500mb-30000-k100-v1": 30,
}
V016_TARGET_SECONDS = 15


def v016_watchdog_seconds(selection):
    """The declared complete-command allowance of one v0.1.6 *performance*
    invocation, or None. The owner-granted 60 seconds applies here."""
    case = selection.get("case") or selection.get("scenario_id") or ""
    if not case.startswith("v016-"):
        return None
    return V016_EXTENDED_WATCHDOGS.get(case, V016_REGULAR_DEADLINE_SECONDS)


def v016_verify_watchdog_seconds(selection):
    """The declared complete-command ceiling of one v0.1.6 *verification*
    invocation, or None. The 25-second regular ceiling is unchanged except for the
    one case the owner declared an exception for."""
    case = selection.get("case") or selection.get("scenario_id") or ""
    if not case.startswith("v016-"):
        return None
    if case in V016_VERIFY_EXCEPTIONS:
        return V016_VERIFY_EXCEPTIONS[case]
    return V016_EXTENDED_WATCHDOGS.get(case, V016_VERIFY_DEADLINE_SECONDS)


def issue47_assessment(selection, elapsed_ns):
    if selection.get("family") != "tiny_file_churn" or selection.get("case") not in (
            "tiny-bulk-create-100-mixed-v3", "tiny-bulk-delete-100-mixed-v3"):
        return None
    return {"contract": "issue47-mixed-v3", "target_ns": 1_000_000_000,
            "strict_less_than": True, "timer": "pure_call_sum_ns", "elapsed_ns": elapsed_ns,
            "status": "PASS" if elapsed_ns < 1_000_000_000 else "TARGET_MISS",
            "qualification": "performance-only; final independent proofs pending"}


TIMERS = {"workspace": "pure_call_sum_ns", "sdk": "edit_commit_ns",
          "namespace": "layerstack_init_ns", "store-footprint": "product_call_sum_ns"}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def harness_identity():
    paths = [HERE / "runner.py", HERE / "runtime.py", HERE / "cold.py", BENCH / "verify-selected.py"]
    return digest({str(path.relative_to(BENCH)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths})


def build_parser(include_modes=True):
    p = argparse.ArgumentParser(description="Full Docker/FUSE workloads; fast means one full sample, never reduced work.")
    p.add_argument("--family", required=True)
    p.add_argument("--topology", choices=("host-store",), default="host-store")
    p.add_argument("--host-binary", default=str(REPO / "target/release/fs-benchmark-pro"))
    p.add_argument("--source-arm", choices=("baseline", "candidate"), default="candidate")
    p.add_argument("--performance-rows", default="-")
    p.add_argument("--case")
    p.add_argument("--sequence", type=int, metavar="COUNT",
                   help="Explicit Workspace SDK edit + Commit sequence over the selected namespace fixture; separate from Init qualification")
    p.add_argument("--sequence-commits", type=int, default=1)
    p.add_argument("--sequence-reopen", action="store_true")
    p.add_argument("--sequence-active-cache", action="store_true",
                   help="Explicit sequence variant: read targets through FUSE before SDK edits")
    p.add_argument("--pseudorandom", action=argparse.BooleanOptionalAction, default=None,
                   help="Namespace content: default/on keeps existing random bytes; --no-pseudorandom selects a distinct structured-text case with identical file sizes (explicit --case required)")
    p.add_argument("--seed", type=int)
    p.add_argument("--repetition", type=int)
    p.add_argument("--setup", choices=("fresh", "clone"))
    if include_modes:
        mode = p.add_mutually_exclusive_group()
        mode.add_argument("--perf-fast", action="store_true")
        mode.add_argument("--perf-samples", type=int)
        mode.add_argument("--verification", action="store_true")
    p.add_argument("--prepare-only", action="store_true")
    p.add_argument("--extended", action="store_true",
                   help="Explicitly select one declared extended case by its exact ID; never a regular default")
    p.add_argument("--smoke", action="store_true", help="Select the smallest registered supported case; performance/setup only")
    p.add_argument("--list", action="store_true", help="Print registered selections without running workloads")
    p.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    p.add_argument("--source", "--source-identity", dest="source")
    p.add_argument("--input", "--input-identity", dest="input")
    p.add_argument("--output", default=str(REPO / "benchmark-results" / "infra" / ("run-" + uuid.uuid4().hex[:12])))
    p.add_argument("--timeout", type=float, default=130, help="Outer performance command allowance; must exceed --product-timeout; historical 15-second pass target remains reporting-only unless collection-mode is unset")
    p.add_argument("--product-timeout", type=int, default=120, help="Workspace diagnostic product allowance in seconds; does not change historical 15-second reporting")
    p.add_argument("--setup-timeout", type=float, default=120)
    p.add_argument("--collection-mode", action="store_true",
                   help="Statistics collection: completed work is PASS regardless of the historical 15-second target; record that target as reporting-only")
    p.add_argument("--cpus", type=int, default=2)
    p.add_argument("--memory-mib", type=int, default=2048)
    return p


def _deadline(end):
    return runtime.Deadline(end)


def _command(argv, end, **kw):
    return runtime.run(argv, deadline=_deadline(end), **kw)


def _host_command_env(args, selection=None):
    image = (selection or {}).get("image") or getattr(args, "image", None)
    return {"LAYERFS_V013_IMAGE": image} if image else {}


def _text(value):
    return value.decode() if isinstance(value, bytes) else value


def records(output):
    result = []
    for line in _text(output).splitlines():
        start = line.find("{")
        if start >= 0:
            try:
                value = json.loads(line[start:])
                if isinstance(value, dict):
                    result.append(value)
            except json.JSONDecodeError:
                pass
    return result


def initialization_diagnostics(output):
    return [{"kind": "initialization-debug-text", "details": line}
            for line in _text(output).splitlines()
            if line.startswith(("layerfs-initialization-diagnostic-",
                                "layerfs-initialization-producer-",
                                "layerfs-initialization-commits-"))]


def host_build_jobs():
    jobs = int(os.environ.get("CARGO_BUILD_JOBS", min(8, os.cpu_count() or 1)))
    if not 1 <= jobs <= 8:
        raise ValueError("qualified host CARGO_BUILD_JOBS must be in 1..=8")
    return jobs


def compilation_seals():
    # Native inputs isolate incompatible arms without recompiling for collector/docs edits.
    paths = sorted(p for root in (REPO / "crates", REPO / "tools", BENCH / "src", BENCH / "workload", BENCH / "families")
                   for p in root.rglob("*") if p.is_file() and p.suffix in (".rs", ".toml", ".sql")
                   and "target" not in p.parts)
    paths += [REPO / "Cargo.toml", REPO / "Cargo.lock", BENCH / "Cargo.toml", BENCH / "Dockerfile.layerfs"]
    if (REPO / ".dockerignore").is_file():
        paths.append(REPO / ".dockerignore")
    if (BENCH / "build.rs").is_file():
        paths.append(BENCH / "build.rs")
    paths += sorted({p for root in ([parent / ".cargo" for parent in (REPO, *REPO.parents)] +
                                  [Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))])
                     for p in (root / "config", root / "config.toml") if p.is_file()})
    value = hashlib.sha256(b"rust-1.85.1;release;host-bins;linux-daemon;fuse-proxy;workload-O3;v1")
    dependency = value.copy()
    for path in paths:
        part = str(path).encode() + b"\0" + path.read_bytes()
        value.update(part)
        if not (path.suffix == ".rs" and any(
                path.is_relative_to(root) for root in (BENCH / "src", BENCH / "families"))):
            dependency.update(part)
    configuration = bytearray()
    configuration.extend(json.dumps({k:v for k,v in os.environ.items() if k.startswith(
        ("RUST", "CARGO_", "CC", "CXX", "CFLAGS", "CPPFLAGS", "LDFLAGS", "AR", "PKG_CONFIG")) or k in
        ("PATH", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "CPATH", "LIBRARY_PATH")}, sort_keys=True).encode())
    for command in (["rustc", "+1.85.1", "-vV"], ["cargo", "+1.85.1", "-V"], ["cc", "--version"]):
        configuration.extend(runtime.run(command, cwd=REPO, deadline=runtime.Deadline.after(10)).stdout)
    configuration.extend(platform.platform().encode())
    configuration.extend(f"host-jobs={host_build_jobs()}".encode())
    # Both seals bind the same configuration, observed once per source check.
    for seal in (value, dependency):
        seal.update(configuration)
    return value.hexdigest(), dependency.hexdigest()


def source_build_args():
    def git(*argv):
        return _text(runtime.run(["git", "-C", str(REPO), *argv], deadline=runtime.Deadline.after(10)).stdout).strip()
    paths = sorted(p for root in (REPO / "crates", REPO / "tools", BENCH)
                   for p in root.rglob("*") if p.is_file() and p.suffix in (".rs", ".toml", ".sh", ".py", ".sql")
                   and "target" not in p.parts and "__pycache__" not in p.parts)
    paths += [REPO / "Cargo.toml", REPO / "Cargo.lock", BENCH / "Dockerfile.layerfs"]
    source = hashlib.sha256()
    product = hashlib.sha256()
    for path in paths:
        part = str(path.relative_to(REPO)).encode() + b"\0" + path.read_bytes()
        source.update(part)
        if "crates" in path.relative_to(REPO).parts:
            product.update(part)
    native, dependencies = compilation_seals()
    return {"LAYERFS_HOST_BUILD_JOBS": str(host_build_jobs()), "LAYERFS_COMPILATION_SEAL": native,
            "LAYERFS_DEPENDENCY_SEAL": dependencies, "LAYERFS_SOURCE_COMMIT": git("rev-parse", "HEAD"),
            "LAYERFS_SOURCE_TREE": git("rev-parse", "HEAD^{tree}"),
            "LAYERFS_SOURCE_DIRTY": "true" if git("status", "--porcelain") else "false", "LAYERFS_SOURCE_SEAL": source.hexdigest(),
            "LAYERFS_PRODUCT_SEAL": product.hexdigest(),
            "WORKLOAD_SOURCE_SHA256": hashlib.sha256((BENCH / "workload/main.rs").read_bytes()).hexdigest()}


def image_info(image, deadline):
    value = runtime._inspect_image(image, _deadline(deadline))
    if value.get("Config", {}).get("Volumes"):
        raise ValueError("image-declared volumes are forbidden")
    return value


def mixed_fixture_info(args, host_identity, seed, deadline):
    # Cache only the untimed immutable oracle identity, never product/output state.
    key = {"binary_sha256": host_identity["binary_sha256"], "family": args.family,
           "case": args.case, "seed": seed, "profile": "tiny-bulk-mixed-v3"}
    path = HOST_ROOT / "fixture-identities" / (digest(key) + ".json")
    if path.exists():
        saved = json.loads(path.read_text())
        if saved.get("key") != key or saved.get("sha256") != digest(saved.get("fixture")):
            raise ValueError("mixed-v3 fixture identity cache mismatch")
        fixture = saved["fixture"]
    else:
        if getattr(args, "verification", False):
            raise ValueError("mixed-v3 proof requires the matching performance fixture identity cache")
        fixture = records(_command([args.host_binary, "infra-fixture-info", args.family, args.case, str(seed)], deadline).stdout)[-1]
        path.parent.mkdir(parents=True, exist_ok=True)
        staging = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
        try:
            staging.write_text(json.dumps({"key": key, "fixture": fixture, "sha256": digest(fixture)}, sort_keys=True))
            staging.chmod(0o444)
            staging.replace(path)
        finally:
            staging.unlink(missing_ok=True)
    if fixture.get("fixture_profile") != "tiny-bulk-mixed-v3" or not fixture.get("populated_manifest_sha256"):
        raise ValueError("mixed-v3 fixture custody missing")
    return fixture


def normalize_namespace_content(args):
    choice = getattr(args, "pseudorandom", None)
    if choice is None:
        return
    if args.family != "init_namespace" or not args.case:
        raise ValueError("--[no-]pseudorandom requires init_namespace and an explicit --case")
    text = args.case.endswith("-text-v1")
    if choice and text:
        raise ValueError("--pseudorandom conflicts with a structured-text case")
    if not choice and not text:
        args.case += "-text-v1"


def resolve_selection(args, deadline):
    normalize_namespace_content(args)
    sequence = getattr(args, "sequence", None)
    if sequence is not None and (args.family != "init_namespace" or not args.case
            or not 0 <= sequence <= 100_000 or not 1 <= args.sequence_commits <= 1000):
        raise ValueError("sequence requires explicit namespace fixture, count 0..100000 and commits 1..1000")
    if sequence is None and (getattr(args, "sequence_commits", 1) != 1 or getattr(args, "sequence_reopen", False)
                            or getattr(args, "sequence_active_cache", False)):
        raise ValueError("sequence options require --sequence")
    if args.topology != "host-store":
        raise ValueError("Docker-owned SQLite is prohibited; use host-store")
    if getattr(args, "_selection", None):
        return args._selection
    if not args.image:
        raise ValueError("select a built Linux image with --image or LAYERFS_BENCH_IMAGE; use shared/runner.py --build-image separately")
    if (not 1 <= args.cpus <= 8 or args.memory_mib <= 0
            or not math.isfinite(args.timeout) or args.timeout <= 0
            or args.product_timeout <= 0 or args.product_timeout >= args.timeout
            or not math.isfinite(args.setup_timeout) or args.setup_timeout <= 0):
        raise ValueError("invalid resource/budget selection")
    if getattr(args, "perf_samples", None) is not None and args.perf_samples <= 0:
        raise ValueError("--perf-samples must be a positive integer")
    info = image_info(args.image, deadline)
    identity = info.get("Config", {}).get("Labels", {}) or {}
    source = identity.get("dev.layerfs.source-seal")
    if not source:
        raise ValueError("image lacks source seal")
    if platform.system() != "Darwin":
        raise ValueError("host Store qualification requires macOS + Docker Desktop")
    if args.family not in HOST_FAMILIES:
        raise ValueError("family is not admitted to host-store execution")
    host_identity = json.loads(Path(args.host_binary + ".identity.json").read_text())
    if runtime.file_sha256(args.host_binary) != host_identity["binary_sha256"]:
        raise ValueError("host binary seal mismatch; rebuild with --build-host")
    if host_identity["LAYERFS_PRODUCT_SEAL"] != identity.get("dev.layerfs.product-seal"):
        raise ValueError("host and Linux image product seals differ")
    if not host_identity.get("LAYERFS_COMPILATION_SEAL") or host_identity["LAYERFS_COMPILATION_SEAL"] != identity.get("dev.layerfs.compilation-seal"):
        raise ValueError("host and Linux image compilation seals differ; rebuild relevant executables")
    source = host_identity["LAYERFS_SOURCE_SEAL"]
    result = _command([args.host_binary, "infra-list", args.family]
                      + ([args.case] if args.case else []), deadline)
    rows = [row for row in records(result.stdout) if row.get("family_id") == args.family]
    if not rows:
        raise ValueError("unknown or archival family: " + args.family)
    if args.list:
        return {"rows": rows, "source_identity": source, "image": info["Id"]}
    if not args.case and args.smoke:
        rows = [row for row in rows if row.get("supported", True) and row.get("smoke_supported", True)]
        if not getattr(args, "verification", False):
            rows = [row for row in rows if not row.get("proof_only")]
        rows.sort(key=lambda r: (r.get("fixture_bytes") or 0, r.get("tier") or 0, r["scenario_id"]))
        if rows:
            args.case = rows[0]["scenario_id"]
        else:
            raise ValueError("no bounded low-tier smoke case: explicit large/proof selections are not run automatically")
    matches = [row for row in rows if row.get("scenario_id") == args.case]
    if not getattr(args, "extended", False):
        matches = [row for row in matches if not row.get("extended")]
    else:
        matches = [row for row in matches if row.get("extended")]
    if len(matches) != 1:
        raise ValueError("select exactly one registered --case (or --smoke for performance)")
    row = matches[0]
    if not row.get("supported", True) and not getattr(args, "extended", False):
        raise ValueError(row.get("unsupported_reason", "historical/unsupported selection"))
    if args.seed is not None and args.repetition is not None:
        raise ValueError("choose seed or inherited repetition, not both")
    inherited = row.get("inherited", row.get("route") == "sdk" or args.family == "edit_length_changing_capped")
    if inherited and args.seed is not None:
        raise ValueError("inherited cases use --repetition, not --seed")
    if not inherited and args.repetition is not None:
        raise ValueError("this case uses --seed, not --repetition")
    seed = args.repetition if args.repetition is not None else (args.seed if args.seed is not None else 1)
    if not row.get("seed_min", 1) <= seed <= row.get("seed_max", 3):
        raise ValueError("invalid registered seed/repetition")
    fresh = row.get("setup_policy") == "fresh-output"
    if fresh and args.setup == "clone":
        raise ValueError("initialization requires a fresh output Store; clone is not applicable")
    setup = "fresh-output" if fresh else (args.setup or "clone")
    input_recipe = {"family": args.family, "case": args.case, "seed": 1 if inherited else seed,
                    "source": source, "recipe": row}
    fixture_info = None
    if row.get("fixture_profile") == "tiny-bulk-mixed-v3":
        fixture_info = mixed_fixture_info(args, host_identity, seed, deadline)
        input_recipe["fixture"] = fixture_info
    input_identity = digest(input_recipe)
    if args.source and args.source != source:
        raise ValueError("selected source identity does not match image")
    if args.input and args.input != input_identity:
        raise ValueError("selected input identity does not match recipe")
    selection = {**row, "family": args.family, "case": args.case, "seed": seed,
                 "repetition": args.repetition, "source_identity": source,
                 "input_identity": input_identity, "setup_identity": setup,
                 "image": info["Id"], "runtime_image": info["Id"],
                 "product_identity": identity.get("dev.layerfs.product-seal"),
                 "harness_identity": harness_identity(), "environment": {"os": info.get("Os"), "architecture": info.get("Architecture"),
                     "container_cpus": args.cpus, "container_memory_mib": args.memory_mib,
                     "topology": args.topology, "host_cpu_capped": False},
                 "verification_supported": row.get("verification_supported", True)}
    if fixture_info is not None:
        selection["fixture_info"] = fixture_info
    selection["source_arm"] = args.source_arm
    if args.family == "init_namespace":
        selection["pseudorandom"] = not args.case.endswith("-text-v1")
        selection["namespace_content_profile"] = (
            "pseudorandom-v1" if selection["pseudorandom"] else "structured-text-v1")
    selection["timer"] = TIMERS.get(row.get("route"))
    if sequence is not None:
        selection["sequence"] = {"schema": "workspace-sequence-v1", "edit_count": sequence,
            "commits": args.sequence_commits, "reopen": args.sequence_reopen,
            "active_cache": args.sequence_active_cache,
            "surface": "Client::edit_workspace_file_range + commit_workspace_session_with_status"}
        selection["timer"] = "edit_commit_ns"
    selection["product_execution_allowance_seconds"] = args.product_timeout
    selection["topology"] = args.topology
    selection.update(host_executor=host_identity, image_source_identity=identity.get("dev.layerfs.source-seal"),
                     host_environment={"os": platform.system(), "architecture": platform.machine(), "cpu_count": os.cpu_count()})
    args._selection = selection
    return selection


HOST_ROOT = isolation.HOST_ROOT


def _host_acquire(args, selection, deadline):
    sdk = selection.get("route") == "sdk"
    host_env = _host_command_env(args, selection)
    fixture_command = [args.host_binary, "infra-fixture-info", selection["family"], selection["case"], str(selection["seed"])]
    fixture = selection.get("fixture_info") or records(_command(fixture_command, deadline, env=host_env).stdout)[-1]
    native = selection["setup_identity"] == "fresh-output"
    compatibility = {"contract": "sdk-edit-prepared-store-cache-v1" if sdk else "layerfs-canonical-v5-workspace-fixture-v1",
        "fixture": fixture, "schema_sha256": selection["host_executor"]["schema_sha256"]}
    if not sdk and selection.get("route") not in ("namespace", "store-footprint"):
        compatibility["seed"] = selection["seed"]
    if native:
        compatibility.update(family=selection["family"], case=selection["case"])
    if selection["family"] == "git_tool_workflow":
        # The Git reference oracle is case-specific even when the Store fixture is shared.
        compatibility["case"] = selection["case"]
    key = digest(compatibility)
    fresh = selection["setup_identity"] == "fresh"
    root = HOST_ROOT / ("fixtures" if native else "prepared") / key
    if fresh:
        root = HOST_ROOT / "samples" / ("prepare-" + uuid.uuid4().hex)
    hit = root.exists()
    if not hit:
        root.parent.mkdir(parents=True, exist_ok=True)
        staging = HOST_ROOT / "samples" / ("prepare-" + uuid.uuid4().hex)
        staging.parent.mkdir(parents=True, exist_ok=True)
        try:
            if sdk:
                staging.mkdir()
                prepared = records(_command([args.host_binary, "sdk-edit-prepare", str(staging / "payload"),
                                             str(selection["fixture_bytes"])], deadline, env=host_env).stdout)[-1]
                (staging / "payload/branch-id").write_text(prepared["branch_id"])
                qualifications = ["family\tcase\tplan\tinitial\texpected\tfile\tmap\tinitial_count\tfinal_count\tdigest\n"]
                for family in ("edit_length_preserving", "edit_length_changing", "edit_canonical_chunk_count"):
                    listed = records(_command([args.host_binary, "infra-list", family], deadline, env=host_env).stdout)
                    for row in listed:
                        if row.get("fixture_bytes") == selection["fixture_bytes"] and row.get("supported", True):
                            output = _command([args.host_binary, "sdk-edit-qualify", str(staging / "payload"),
                                               prepared["branch_id"], family, row["scenario_id"]], deadline, env=host_env).stdout
                            qualifications.append(_text(output))
                (staging / "qualification.tsv").write_text("".join(qualifications))
                (staging / "manifest.json").write_text(json.dumps({
                    "schema": "fs-bench-infra-prepared-v1",
                    "input_qualification_sha256": runtime.file_sha256(staging / "qualification.tsv")
                }))
            else:
                _command([args.host_binary, "infra-prepare", selection["family"], selection["case"], str(selection["seed"]), str(staging)], deadline, env=host_env)
            seal = None
            if selection["family"] == "historical_access":
                seal = _seal_producer(args, selection, staging, deadline)
            (staging / "host-owner.json").write_text(json.dumps({"owner": runtime.OWNER}))
            if not native:
                master = staging / "payload/store.sqlite"
                # Preparation process has exited. Validate a disposable copy before protecting the master.
                checked = staging / "checked.sqlite"
                runtime.closed_store_copy(master, checked, deadline=_deadline(deadline))
                checked.unlink()
                master.rename(staging / "store.sqlite")
                (staging / "store.sqlite").chmod(0o444)
            files = runtime.host_tree_identity(staging, _deadline(deadline))
            manifest = {"compatibility": compatibility, "producer": selection["host_executor"], "created_ns": time.time_ns(),
                        "files": files, "data_bytes": sum(item.get("bytes", 0) for item in files.values())}
            if seal is not None:
                manifest["producer_seal"] = seal
            (staging / "host-cache.json").write_text(json.dumps(manifest, sort_keys=True))
            for path in staging.rglob("*"):
                if path.is_file() and not path.is_symlink() and not (native and path.is_relative_to(staging / "payload")):
                    path.chmod(path.stat().st_mode & ~0o222)
            staging.rename(root)
        except BaseException:
            if staging.exists():
                (staging / "host-owner.json").write_text(json.dumps({"owner": runtime.OWNER}))
                runtime.remove_host_owned(staging)
            raise
    manifest = json.loads((root / "host-cache.json").read_text())
    if manifest["compatibility"] != compatibility or json.loads((root / "host-owner.json").read_text()).get("owner") != runtime.OWNER:
        raise ValueError("host prepared cache compatibility/content mismatch")
    # ponytail: owned native inputs trust their prepared recipe; sampled verification
    # detects selected content faults. Recreate this disposable cache if it is modified.
    if not native and runtime.host_tree_identity(root, _deadline(deadline)) != manifest["files"]:
        raise ValueError("host prepared Store content mismatch")
    # Immutable fixtures/prepared inputs are protected, not sample/build scratch.
    # Input retirement is an explicit owner action; acquiring a new case must
    # not delete another case's qualified master (#118).
    removed = []
    if native:
        fixture = json.loads((root / "fixture.json").read_text())
    return {"image": selection["image"], "host_root": str(root), "cache_key": key, "cache_hit": hit,
            "one_shot": fresh, "producer": manifest["producer"], "compatibility": compatibility,
            "fixture": fixture, "data_bytes": manifest["data_bytes"], "evicted": removed,
            "input_validation": "owned-prepared-recipe" if native else "full-master-identity"}


def _seal_producer(args, selection, staging, deadline):
    """Publish one `historical_access` producer into a freshly prepared master.

    An access case reads one selected retained state of a *sealed* producer, so
    the store a case acquires must already carry that producer's history. The
    sealing step runs here, once, with its own container and outside every
    measured window; the access invocation itself never builds history.
    """
    name = "layerfs-infra-seal-" + uuid.uuid4().hex[:12]
    environment = {
        **os.environ,
        **_host_command_env(args, selection),
        "LAYERFS_EXEC_TRANSPORT": "daemon",
        "LAYERFS_FUSE_TRANSPORT": "daemon",
        "LAYERFS_BENCH_WORKLOAD": "/usr/local/bin/fs-benchmark-workload",
        "LAYERFS_BENCH_PREPARED_INPUT": str(Path(staging) / "payload" / "input"),
        "TMPDIR": str(staging),
    }
    sample = runtime.start_sample(
        selection["image"], name,
        {"family": selection["family"], "case": selection["case"], "run": name, "phase": "seal"},
        deadline=_deadline(deadline))
    try:
        started = time.monotonic()
        command = _command([args.host_binary, "infra-seal-producer", selection["family"],
                            selection["case"], str(selection["seed"]), str(staging), sample.id],
                           deadline, env=environment, output_limit=16 * 1024**2)
        wall = time.monotonic() - started
        published = [row for row in records(command.stdout) if row.get("kind") == "v016-access-seal"]
        if command.returncode or len(published) != 1:
            raise RuntimeError(
                f"producer sealing failed: {_text(command.stderr)[-2048:]}")
        receipt = published[0]
        return {"schema": "v016-access-producer-seal-v1",
                "case": selection["case"], "seed": selection["seed"],
                "image": selection["image"], "container": sample.id,
                "wall_seconds": round(wall, 3),
                "producer": receipt.get("producer"),
                "producer_family": receipt.get("producer_family"),
                "selected_ordinal": receipt.get("selected_ordinal"),
                "declared_roots": receipt.get("declared_roots"),
                "branch_role": receipt.get("branch_role")}
    finally:
        runtime.run(["docker", "rm", "--force", name],
                    deadline=runtime.Deadline.after(60), output_limit=4096, check=False)
        control = Path(staging) / "container-control"
        if control.exists():
            shutil.rmtree(control, ignore_errors=True)


def _host_sample(prepared, selection, name, deadline):
    master = Path(prepared["host_root"])
    sample = HOST_ROOT / "samples" / name
    sample.mkdir(parents=True, exist_ok=False)
    (sample / "host-owner.json").write_text(json.dumps({"owner": runtime.OWNER}))
    (sample / "payload").mkdir()
    fixture = dict(prepared["fixture"])
    receipt = {"sample_root": str(sample), "prepared_root": str(master), "setup_mode": selection["setup_identity"]}
    if selection["setup_identity"] == "fresh-output":
        if (master / "input-qualification.tsv").exists():
            shutil.copyfile(master / "input-qualification.tsv", sample / "input-qualification.tsv")
        source = master / ("payload" if selection.get("route") in ("namespace", "store-footprint") else "payload/input")
        receipt.update(clone_method="not-applicable", fixture_reuse_method="host-prepared-source",
                       prepared_input_root=str(source), fresh_output_stores=[str(sample / "payload/store.sqlite"), str(sample / "work/store.sqlite")])
    else:
        receipt.update(runtime.closed_store_copy(master / "store.sqlite", sample / "payload/store.sqlite", deadline=_deadline(deadline)))
        fixture["branch_id"] = (master / "payload/branch-id").read_text().strip()
        (sample / "payload/branch-id").write_text(fixture["branch_id"])
        if selection.get("route") == "sdk":
            shutil.copyfile(master / "qualification.tsv", sample / "qualification.tsv")
    encoded = json.dumps(fixture, separators=(",", ":")) + "\n"
    (sample / "fixture.json").write_text(encoded)
    manifest = json.loads((master / "manifest.json").read_text())
    manifest.update(family_id=selection["family"], scenario_id=selection["case"], seed=selection["seed"],
                    fixture_receipt_sha256=hashlib.sha256(encoded.encode()).hexdigest())
    if selection.get("route") == "sdk":
        manifest["input_qualification_sha256"] = runtime.file_sha256(sample / "qualification.tsv")
    (sample / "manifest.json").write_text(json.dumps(manifest, separators=(",", ":")))
    (sample / "selection.tsv").write_text(f"{selection['family']}\t{selection['case']}\t{selection['seed']}\n")
    return receipt


def cgroup_snapshot(sample, deadline):
    command = _command(["docker", "exec", sample.id, "sh", "-c",
        "cat /sys/fs/cgroup/cpu.stat; printf 'memory_peak '; cat /sys/fs/cgroup/memory.peak; "
        "printf 'memory_current '; cat /sys/fs/cgroup/memory.current; "
        "printf 'swap_current '; cat /sys/fs/cgroup/memory.swap.current; "
        "cat /sys/fs/cgroup/memory.events"], deadline)
    result = {}
    for line in _text(command.stdout).splitlines():
        fields = line.split()
        if len(fields) == 2:
            result[fields[0]] = int(fields[1])
    return result


def sdk_store_observation(path):
    started = time.monotonic_ns()
    if not path.is_file() or path.is_symlink():
        raise ValueError("closed SDK Store absent or symlinked")
    files = []
    for suffix in ("", "-wal", "-shm", "-journal"):
        item = Path(str(path) + suffix)
        if not item.exists():
            continue
        if item.is_symlink():
            raise ValueError("SDK Store sidecar symlink")
        stat = item.stat()
        files.append({"path": str(item), "device": stat.st_dev, "inode": stat.st_ino,
                      "apparent_bytes": stat.st_size, "allocated_bytes": stat.st_blocks * 512,
                      "mtime_ns": stat.st_mtime_ns})
    return {"boundary": "native-command-closed-after-resource-window-before-cleanup",
            "files": files, "allocated_bytes": sum(f["allocated_bytes"] for f in files),
            "apparent_bytes": sum(f["apparent_bytes"] for f in files),
            "observation_ns": time.monotonic_ns() - started,
            "content_hash": None, "identity_kind": "quiescent physical inode/device; no verification digest"}


def execute_selected(args, *, deadline, verification=False):
    """No receipt files here: the caller publishes after this function cleans up."""
    started = time.monotonic_ns()
    selection = resolve_selection(args, deadline)
    policy = verification_policy(selection) if verification else None
    if policy is not None:
        selection["verification_policy"] = policy
    if selection.get("proof_only") and not verification and not args.prepare_only:
        raise ValueError("proof-only case does not support performance")
    if verification and not selection["verification_supported"]:
        return {"status": "INCOMPLETE", "identities": selection, "omissions": ["proof cannot fit bounded verification"], "cleanup": {"status": "PASS", "not_started": True}}
    sample = None
    sample_name = None
    host_sample_path = None
    prepared = None
    result = {"status": "INCOMPLETE", "identities": selection, "checks": [], "omissions": [],
              "phase": "preparation",
              "sampled_paths_or_ranges": [], "reused_proof_identities": [],
              "resource_precision": "separate host process CPU/RSS/IO and container lifetime peak/command CPU"}
    work_end = deadline - (policy["cleanup_reserve_seconds"] if policy else 4)
    try:
        setup_started = time.monotonic_ns()
        prepared = _host_acquire(args, selection, work_end)
        result["preparation"] = prepared
        if args.prepare_only:
            result["status"] = "PASS"
            return result
        name = "layerfs-infra-sample-" + uuid.uuid4().hex[:12]
        sample_name = name
        result["phase"] = "runtime-start"
        sample = runtime.start_sample(prepared["image"], name,
            {"family": selection["family"], "run": name}, deadline=_deadline(work_end),
            cpus=args.cpus, memory_bytes=args.memory_mib * 1024**2)
        result["phase"] = "sample-setup"
        result["environment_observation"] = sample.observation
        host_sample_path = HOST_ROOT / "samples" / name
        result["setup"] = _host_sample(prepared, selection, name, work_end)
        if selection["family"] == "git_tool_workflow":
            reference = Path(prepared["host_root"]) / "reference" / "input"
            if not reference.is_dir():
                raise RuntimeError("prepared Git reference tree is missing")
            runtime.install_tree(sample.name, reference, "/qualified/git-reference", _deadline(work_end))
            runtime.ensure_container_dir(sample.name, "/verification", _deadline(work_end))
            result["setup"]["git_reference_install"] = {
                "method": "docker-cp",
                "source": str(reference),
                "destination": "/qualified/git-reference",
                "host_data_sharing_mount": False,
            }
        result["preparation_wall_ns"] = time.monotonic_ns() - setup_started
        result["phase"] = "product-command"
        command_env = {"LAYERFS_V013_IMAGE": selection["image"],
                       "LAYERFS_BENCH_SOURCE_ARM": selection["source_arm"]}
        if not verification:
            declared = v016_watchdog_seconds(selection)
            if declared is None:
                command_env["LAYERFS_BENCH_PRODUCT_TIMEOUT_SECONDS"] = str(args.product_timeout)
            else:
                # The roadmap's worker stop, with the three-second cleanup
                # reserve inside the declared complete-command deadline.
                command_env["LAYERFS_BENCH_PRODUCT_TIMEOUT_SECONDS"] = str(
                    max(1, declared - 3)
                )
        if args.performance_rows != "-":
            command_env["LAYERFS_SDK_EDIT_PERFORMANCE_ROWS"] = args.performance_rows
        if selection["family"] in ("dedup_cross_file", "dedup_cdc_locality"):
            command_env["LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE"] = selection["input_identity"][:16]
        if selection["family"] == "workspace_reliability":
            prepared_input = str(Path(prepared["host_root"]))
        else:
            prepared_input = result["setup"].get(
                "prepared_input_root", str(Path(prepared["host_root"]) / "payload/input")
            )
        command_env.update(LAYERFS_EXEC_TRANSPORT="daemon", LAYERFS_FUSE_TRANSPORT="daemon",
            LAYERFS_BENCH_WORKLOAD="/usr/local/bin/fs-benchmark-workload",
            LAYERFS_BENCH_PREPARED_INPUT=prepared_input,
            TMPDIR=str(host_sample_path))
        operation = ["infra-run", selection["family"], selection["case"], str(selection["seed"]),
                     "verify" if verification else "performance", str(host_sample_path), sample.id]
        if sequence := selection.get("sequence"):
            operation = ["workspace-sequence", str(host_sample_path), prepared_input, sample.id,
                selection["case"], str(sequence["edit_count"]), str(sequence["commits"]),
                str(sequence["reopen"]).lower(), str(sequence["active_cache"]).lower(),
                "verify" if verification else "performance"]
        if not verification and cold.applies(selection):
            result["cold_diagnostic_environment"] = any(os.environ.get(key) for key in (
                "LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE", "LAYERFS_BENCH_INITIALIZATION_SEED_HEX"))
            result["cold_acquisition"] = cold.acquire(prepared, host_sample_path,
                min(work_end - args.timeout, time.monotonic() + args.setup_timeout))
            result["preparation_wall_ns"] += result["cold_acquisition"]["wall_ns"]
        before = cgroup_snapshot(sample, work_end)
        run_started = time.monotonic_ns()
        result["product_command_started_ns"] = run_started
        command_end = min(work_end, time.monotonic() + (policy["work_limit_seconds"] if policy else args.timeout))
        command = _command([args.host_binary, *operation], command_end, env=command_env, output_limit=16 * 1024**2)
        result["command_wall_ns"] = time.monotonic_ns() - run_started
        result["records"] = records(command.stdout)
        result["records"].extend(initialization_diagnostics(command.stderr))
        diagnostics = [r for r in records(command.stderr) if r.get("kind") == "edit-diagnostic"]
        if diagnostics:
            result["edit_diagnostics"] = diagnostics
            result["diagnostic_only"] = True
        for record in result["records"]:
            if record.get("kind") == "sampled-canonical-verification":
                result["sampled_paths_or_ranges"].extend(record["sampled_paths_or_ranges"])
                result["omissions"].append("sampled Workspace proof: no exhaustive namespace, full-file bytes, object census, aliases, or failure injection")
        timer, elapsed = _timer(result)
        if command.truncated:
            result["omissions"].append("selected command output exceeded the 16 MiB compact receipt limit")
            if not verification or selection.get("sequence"):
                raise RuntimeError("selected command output exceeded compact receipt limit")
        if not verification and elapsed is None and command.returncode == 0:
            raise RuntimeError(f"missing declared product timer: {timer}")
        result["phase"] = "resource-finalization"
        after = cgroup_snapshot(sample, work_end)
        result["resources"] = {"command_window_cpu_ns": (after["usage_usec"] - before["usage_usec"]) * 1000,
            "sample_container_lifetime_peak_bytes": after["memory_peak"],
            "memory_current_bytes": after["memory_current"], "swap_current_bytes": after["swap_current"],
            "oom_kill_delta": after.get("oom_kill", 0) - before.get("oom_kill", 0),
            "measurement_scope": "Linux daemon/FUSE container command window; host coordinator/Store process CPU/RSS/IO reported separately in records; host CPU is not container-capped"}
        if selection.get("route") in ("sdk", "namespace", "store-footprint") and not verification:
            store_folder = "payload" if selection["route"] == "sdk" else "work"
            result["store_boundary"] = sdk_store_observation(host_sample_path / store_folder / "store.sqlite")
        result["status"] = "PASS" if command.returncode == 0 and result["records"] and not result["resources"]["oom_kill_delta"] else "FAIL"
        result["slow"] = result["command_wall_ns"] >= 5_000_000_000
        result["checks"] = [r for r in result["records"] if "verif" in str(r.get("kind", "")) or "proof" in str(r.get("kind", ""))]
        if result["status"] != "PASS":
            result["error"] = _text(command.stderr)[-8192:]
        else:
            result["phase"] = "complete"
            if not verification:
                result["completion_status"] = "COMPLETE"
                result["product_target_ns"] = PRODUCT_TARGET_NS
                historical = performance_target_status(elapsed)
                result["historical_product_target_status"] = historical
                result["historical_product_target_scope"] = HISTORICAL_PRODUCT_TARGET_SCOPE
                assessment = issue47_assessment(selection, elapsed)
                if assessment is not None:
                    result["issue47_assessment"] = assessment
                declared_complete = v016_watchdog_seconds(selection)
                if declared_complete is not None:
                    # The v0.1.6 complete-command gate: the whole selected
                    # invocation, including acquisition, runtime readiness,
                    # scheduled work, receipt publication and cleanup, must fit
                    # the case's declared allowance. The 15-second family target
                    # is reported separately and never silently reduces work.
                    complete = result["command_wall_ns"]
                    limit_ns = declared_complete * 1_000_000_000
                    result["family_target_ns"] = PRODUCT_TARGET_NS
                    result["family_target_status"] = (
                        "PASS" if complete <= PRODUCT_TARGET_NS else "TARGET_MISS"
                    )
                    result["declared_complete_deadline_ns"] = limit_ns
                    result["declared_complete_deadline_seconds"] = declared_complete
                    result["complete_command_status"] = (
                        "PASS" if complete <= limit_ns else "TARGET_MISS"
                    )
                    if result["complete_command_status"] == "TARGET_MISS":
                        result["status"] = "TARGET_MISS"
                        result["error"] = (
                            f"complete selected invocation {complete} ns exceeds the declared "
                            f"{declared_complete}-second deadline"
                        )
                elif getattr(args, "collection_mode", False):
                    result["status"] = "PASS"
                    if historical == "TARGET_MISS":
                        result["historical_product_target_note"] = (
                            f"complete product-call sum {elapsed} ns exceeds the historical 15-second target"
                        )
                else:
                    result["status"] = historical
                    if result["status"] == "TARGET_MISS":
                        result["error"] = f"complete product-call sum {elapsed} ns exceeds the 15-second target"
    except Exception as error:
        result["status"] = "TIMEOUT" if isinstance(error, TimeoutError) or "timeout" in str(error).lower() or "deadline" in str(error).lower() else "FAIL"
        failed_command = getattr(error, "result", None)
        detail = _text(failed_command.stderr)[-8192:] if failed_command is not None else ""
        result["error"] = (str(error) + "\n" + detail)[-8192:]
        if failed_command is not None:
            result.setdefault("records", records(failed_command.stdout))
            if result["phase"] == "product-command":
                result["command_wall_ns"] = failed_command.wall_ns
        if any(record.get("kind") == "product-time-budget-exceeded" for record in result.get("records", [])):
            result["status"] = "TIMEOUT"
        result["completion_status"] = "INCOMPLETE"
        result["slow"] = result["status"] == "TIMEOUT"
    finally:
        cleanup_started = time.monotonic_ns()
        try:
            if sample is not None:
                sample.remove(deadline=_deadline(deadline))
            elif sample_name is not None:
                remaining = _command(["docker", "container", "inspect", sample_name], deadline, check=False)
                if remaining.returncode == 0:
                    item = json.loads(_text(remaining.stdout))[0]
                    labels = item.get("Config", {}).get("Labels") or {}
                    if labels.get(runtime.OWNER_LABEL) != runtime.OWNER or labels.get("run") != sample_name:
                        raise RuntimeError("startup cleanup refused mismatched ownership")
                    _command(["docker", "rm", "--force", item["Id"]], deadline)
                elif "No such" not in _text(remaining.stderr):
                    raise RuntimeError("cannot confirm failed-start container cleanup")
            if host_sample_path is not None and host_sample_path.exists():
                runtime.remove_host_owned(host_sample_path)
            if prepared and selection["setup_identity"] != "fresh-output":
                master = Path(prepared["host_root"])
                manifest = json.loads((master / "host-cache.json").read_text())
                if runtime.host_tree_identity(master, _deadline(deadline)) != manifest["files"]:
                    raise RuntimeError("prepared host master changed during sample")
                result["prepared_master_unchanged"] = True
            if prepared and prepared.get("one_shot"):
                runtime.remove_host_owned(prepared["host_root"])
            result["cleanup"] = {"status": "PASS", "wall_ns": time.monotonic_ns() - cleanup_started}
        except Exception as error:
            result["cleanup"] = {"status": "FAIL", "error": str(error)[-2048:]}
            result["status"] = "INCOMPLETE"
        result["wall_ns"] = time.monotonic_ns() - started
    if result.get("diagnostic_only") and result["status"] == "PASS":
        result["status"] = "DIAGNOSTIC"
        result["performance_distribution"] = False
    return cold.enforce(result) if not verification and not args.prepare_only else result


def performance_summary(samples, count, collection_mode):
    # Reclassify raw evidence even when a caller supplies saved PASS summaries.
    samples = [cold.enforce(row) for row in samples]
    gated = any(cold.applies(row.get("identities", {})) for row in samples)
    valid = [row for row in samples if row["status"] == "PASS"]
    completed = [row for row in samples if row["status"] in ("PASS", "TARGET_MISS")]
    times = [value for row in completed if (value := _timer(row)[1]) is not None]
    if collection_mode and not gated:
        status = "PASS" if len(valid) == count else "INCOMPLETE"
    else:
        status = "PASS" if len(valid) == count else ("TARGET_MISS" if len(completed) == count else "INCOMPLETE")
    return {"kind": "summary", "requested": count, "attempted": len(samples), "valid": len(valid),
            "completed": len(completed), "product_target_ns": cold.TARGET_NS if gated else PRODUCT_TARGET_NS,
            "collection_mode": bool(collection_mode),
            "historical_product_target_scope": "not applicable; fixed cold contract" if gated else HISTORICAL_PRODUCT_TARGET_SCOPE,
            "status": status, "verification_status": "NOT_RUN", "admission_eligible": False,
            "timer": _timer(completed[0])[0] if completed else None,
            "median_ns": statistics.median(times) if times else None,
            "min_ns": min(times) if times else None, "max_ns": max(times) if times else None,
            "diagnostic_samples": sum(row.get("status") == "INELIGIBLE" for row in samples)}


def _timer(row):
    declared = row.get("identities", {}).get("timer")
    for record in reversed(row.get("records", [])):
        keys = (declared,) if declared else ("layerstack_init_ns",) if row.get("identities", {}).get("route") == "namespace" else (
            "edit_commit_ns", "edit_commit_end_ns", "pure_call_sum_ns", "initialize_ns", "execution_ns", "complete_ns", "complete_lifecycle_ns")
        for key in keys:
            if isinstance(record.get(key), (float, int)):
                return key, record[key]
    if declared in (None, "pure_call_sum_ns", "product_call_sum_ns"):
        total = 0
        found = False
        for record in row.get("records", []):
            if record.get("kind") == "phase" and isinstance(record.get("elapsed_ns"), (float, int)):
                total += record["elapsed_ns"]
                found = True
        if found:
            return declared or "pure_call_sum_ns", total
    return declared or "unavailable", None


def seed_host_dependencies(build_target, values, binary):
    """Reuse #104's independent-copy recipe only for benchmark-source edits.

    Called under the runner measurement lock. Cargo still validates every copied
    dependency; benchmark executables, dep-info and fingerprints are never seeded.
    """
    identity_path = Path(str(binary) + ".identity.json")
    if build_target.exists() or not identity_path.exists():
        return None
    previous = json.loads(identity_path.read_text())
    if previous.get("LAYERFS_DEPENDENCY_SEAL") != values["LAYERFS_DEPENDENCY_SEAL"]:
        return None
    source = HOST_ROOT / "builds" / ("native-" + previous["LAYERFS_COMPILATION_SEAL"])
    if previous.get("build_target") != str(source) or not source.is_dir():
        return None
    if runtime.file_sha256(binary) != previous["binary_sha256"]:
        raise ValueError("dependency seed producer binary identity mismatch")
    started = time.monotonic_ns()
    staging = build_target.with_name(build_target.name + ".seed-" + uuid.uuid4().hex)
    shutil.copytree(source, staging, ignore=shutil.ignore_patterns("fs-benchmark-pro*", "fs_benchmark_pro*"))
    staging.rename(build_target)
    receipt = {"source_target": str(source), "destination_target": str(build_target),
               "dependency_seal": values["LAYERFS_DEPENDENCY_SEAL"],
               "producer_binary_sha256": previous["binary_sha256"],
               "copy_wall_ns": time.monotonic_ns() - started,
               "benchmark_outputs_copied": False, "independent_copy": True}
    print("DEPENDENCY_REUSE " + json.dumps(receipt, sort_keys=True), file=sys.stderr, flush=True)
    return receipt


def build_mode():
    """Select the owned build cache. `sealed` keeps the pre-#117 per-seal target."""
    mode = os.environ.get("LAYERFS_BUILD_ISOLATION", "incremental")
    if mode not in BUILD_MODES:
        raise ValueError(f"LAYERFS_BUILD_ISOLATION must be one of {BUILD_MODES}")
    return mode


def incremental_build_target():
    return HOST_ROOT / "builds" / INCREMENTAL_TARGET_NAME


def recompiled_packages(result):
    """Name every cargo unit the build actually recompiled; reused units are absent."""
    packages = []
    for line in (result.stdout + b"\n" + result.stderr).decode("utf-8", "replace").splitlines():
        match = re.match(r"\s*Compiling\s+(\S+)\s+v(\S+)", line)
        if match and match.group(1) not in packages:
            packages.append(match.group(1))
    return packages


def prune_build_caches(keep=DEFAULT_RETAINED_SEALED_BUILDS, apply=False):
    """Bounded retention for writable build caches (#117).

    Retains the owned incremental cache, the newest `keep` per-seal native
    targets, and anything outside `builds/`. Never touches fixtures, prepared
    inputs, samples or the immutable binary/image archives.
    """
    if type(keep) is not int or keep < 0:
        raise ValueError("retained build count must be a nonnegative integer")
    builds = HOST_ROOT / "builds"
    if builds.is_symlink() or builds.resolve().parent != HOST_ROOT.resolve():
        raise ValueError("build cache root is not independently owned")
    sealed = sorted((p for p in builds.iterdir() if re.fullmatch(r"native-[0-9a-f]{64}", p.name)),
                    key=lambda p: p.lstat().st_mtime, reverse=True) if builds.exists() else []
    retained, candidates = [], []
    for index, path in enumerate(sealed):
        marker = path / "CACHEDIR.TAG"
        if path.is_symlink() or not path.is_dir() or marker.is_symlink() or not marker.is_file() or not marker.read_text().startswith(
                "Signature: 8a477f597d28d172789f06886806bc55"):
            raise ValueError("refusing unrecognized Cargo cache: " + str(path))
        if index < keep:
            retained.append(str(path))
            continue
        size = sum(f.lstat().st_size for f in path.rglob("*") if not f.is_symlink() and f.is_file())
        candidates.append({"path": str(path), "bytes": size})
    # Validate the complete selection before deleting any owned cache. Archives
    # and sample/input roots never enter this selection; rmtree does not follow links.
    if apply:
        for entry in candidates:
            shutil.rmtree(entry["path"])
    receipt = {"schema": "layerfs-build-retention-v1", "policy": {
        "retained_sealed_targets": keep, "incremental_cache": INCREMENTAL_TARGET_NAME,
        "protected": ["fixtures", "prepared", "samples", "binary-archive", "image-archive"]},
        "retained": retained, "incremental_target": str(incremental_build_target()),
        "candidates": candidates, "removed": candidates if apply else [],
        "candidate_bytes": sum(entry["bytes"] for entry in candidates),
        "reclaimed_bytes": sum(entry["bytes"] for entry in candidates) if apply else 0,
        "bytes_basis": "apparent file bytes; not a measured free-space delta", "apply": apply}
    return receipt


def image_retention(keep=2, apply=False):
    """Bounded retention for the benchmark's own Docker images (#118).

    Every source change tags a new `layerfs-bench-infra:<source-seal-16>`, so the
    build path accumulates one ~2.4 GB image per edit until it is pruned. The
    newest `keep` images are retained. Every other owned image is first given its
    immutable binary archive (`image-archive/<image-id>/`: the two daemon-side
    binaries, the workload helper and the full image identity), so a deleted
    image is always reconstructible from the archived binaries plus the seals the
    receipts already carry. Unrecognized or foreign images are never candidates.
    """
    if type(keep) is not int or keep < 0:
        raise ValueError("retained image count must be a nonnegative integer")
    archive = HOST_ROOT / "image-archive"
    listed = runtime.run(
        ["docker", "images", "--format", "{{.Repository}}\t{{.Tag}}\t{{.ID}}\t{{.CreatedAt}}"],
        deadline=runtime.Deadline.after(30),
    ).stdout.decode("utf-8", "replace")
    owned = []
    for line in listed.splitlines():
        fields = line.split("\t")
        if len(fields) != 4 or fields[0] != "layerfs-bench-infra":
            continue
        owned.append({"tag": fields[1], "id": fields[2], "created": fields[3]})
    owned.sort(key=lambda row: row["created"], reverse=True)
    complete = {"fs-benchmark-workload", "layerfs-daemon", "layerfs-fuse", "identity.json"}
    retained, candidates = [], []
    for index, row in enumerate(owned):
        if index < keep:
            retained.append({**row, "reason": "newest retained image"})
            continue
        # The binary archive is keyed by the full image ID, which `docker images`
        # reports shortened; resolve the full digest before deciding.
        info = image_info(row["id"], time.monotonic() + 30)
        full_id = info["Id"]
        root = archive / full_id.split(":")[-1]
        files = sorted(p.name for p in root.iterdir()) if root.is_dir() else []
        needs_archive = not complete.issubset(set(files))
        candidates.append({**row, "full_id": full_id, "archive": str(root),
                           "archive_complete": not needs_archive,
                           "archive_required": needs_archive})
    for entry in candidates:
        if entry["archive_required"]:
            archive_image(entry["full_id"])
            entry["archive_complete"] = True
            entry["archive_required"] = False
        root = Path(entry["archive"])
        files = sorted(p.name for p in root.iterdir()) if root.is_dir() else []
        if not complete.issubset(set(files)):
            raise ValueError("refusing to delete an image without a complete archive: " + entry["tag"])
        saved = json.loads((root / "identity.json").read_text())
        for name, digest in saved["binaries"].items():
            if runtime.file_sha256(root / name) != digest:
                raise ValueError("archived image binary changed: " + entry["tag"])
    if apply:
        for entry in candidates:
            runtime.run(
                ["docker", "image", "rm", entry["full_id"]],
                deadline=runtime.Deadline.after(120),
            )
    return {
        "schema": "layerfs-image-retention-v1",
        "policy": {"retained_newest": keep, "protected": ["image-archive", "binary-archive",
                                                          "fixtures", "prepared", "samples"]},
        "owned_images": len(owned),
        "retained": retained,
        "candidates": candidates,
        "removed": [entry["tag"] for entry in candidates] if apply else [],
        "removed_count": len(candidates) if apply else 0,
        "apply": apply,
    }


def verify_linked_schema(binary, expected):
    with tempfile.TemporaryDirectory(prefix="layerfs-build-schema-") as folder:
        observed = int(runtime.run([str(binary), "infra-schema-probe", str(Path(folder) / "store.sqlite")],
                                   deadline=runtime.Deadline.after(15)).stdout)
    if observed != expected:
        raise ValueError(f"linked Store schema {observed} differs from source schema {expected}; stale build")
    return observed


def copy_executable(source, destination, *, readonly=False):
    """Replace, never overwrite an inode that may be linked to a control/cache."""
    staging = destination.with_name(destination.name + ".copy-" + uuid.uuid4().hex)
    try:
        shutil.copy2(source, staging)
        staging.chmod(0o555 if readonly else 0o755)
        staging.replace(destination)
    finally:
        staging.unlink(missing_ok=True)


def archive_binary(binary):
    if not binary.exists():
        return
    digest = runtime.file_sha256(binary)
    folder = HOST_ROOT / "binary-archive" / digest
    folder.mkdir(parents=True, exist_ok=True)
    archived = folder / binary.name
    if not archived.exists():
        copy_executable(binary, archived, readonly=True)
    if archived.is_symlink() or archived.stat().st_nlink != 1:
        raise ValueError("binary archive must be an independent copy")
    archived.chmod(0o555)
    identity = Path(str(binary) + ".identity.json")
    if identity.exists():
        destination = folder / (binary.name + ".identity.json")
        if not destination.exists():
            shutil.copy2(identity, destination)
            destination.chmod(0o444)
    observation = folder / (binary.name + ".archive-observation.json")
    if not observation.exists():
        saved = json.loads(identity.read_text()) if identity.exists() else None
        observation.write_text(json.dumps({"observed_binary_sha256":digest,"original_path":str(binary),
            "identity_present":saved is not None,"identity_matches_binary":saved is not None and saved.get("binary_sha256")==digest},sort_keys=True))
    if runtime.file_sha256(archived) != digest:
        raise ValueError("binary archive custody mismatch")


def archive_image(tag):
    info = image_info(tag,time.monotonic()+30)
    folder = HOST_ROOT / "image-archive" / info["Id"].split(":")[-1]
    if (folder / "identity.json").exists():
        saved=json.loads((folder/"identity.json").read_text())
        if any(runtime.file_sha256(folder/name)!=digest for name,digest in saved["binaries"].items()):
            raise ValueError("archived image binary changed")
        return
    folder.mkdir(parents=True,exist_ok=True)
    name="layerfs-build-archive-"+uuid.uuid4().hex[:12]
    created=False
    try:
        runtime.run(["docker","create","--name",name,"--label",runtime.OWNER_LABEL+"="+runtime.OWNER,tag],deadline=runtime.Deadline.after(30))
        created=True
        binaries={}
        for binary in ("layerfs-daemon","layerfs-fuse","fs-benchmark-workload"):
            runtime.run(["docker","cp",name+":/usr/local/bin/"+binary,str(folder/binary)],deadline=runtime.Deadline.after(30))
            binaries[binary]=runtime.file_sha256(folder/binary)
    finally:
        if created: runtime.run(["docker","rm",name],deadline=runtime.Deadline.after(30))
    (folder/"identity.json").write_text(json.dumps({"image":info,"binaries":binaries},sort_keys=True))


def verify_integrated_format(binary):
    with tempfile.TemporaryDirectory(prefix="layerfs-build-format-") as folder:
        root = Path(folder); (root / "tmp").mkdir()
        raw = runtime.run([str(binary), "storage-format-probe", str(root)],
            deadline=runtime.Deadline.after(30), env={"TMPDIR":str(root/"tmp"),"SQLITE_TMPDIR":str(root/"tmp")})
        records = [json.loads(line) for line in raw.stdout.decode().splitlines()]
        probes = [r for r in records if r.get("kind") == "storage-format-probe"]
        if len(probes) != 1 or probes[0].get("status") != "PASS" or probes[0].get("schema_version") != 10 or probes[0].get("storage_policy") != "ordinary":
            raise ValueError("linked integrated product format probe failed")
        return {**probes[0], "records":records, "stdout_sha256":hashlib.sha256(raw.stdout).hexdigest()}


def main(argv=None):
    access_started_ns = ENTRY_STARTED_NS
    argv = sys.argv[1:] if argv is None else argv
    if "--integration-smoke" in argv:
        import integrated_storage
        return integrated_storage.main(argv)
    if "--family" in argv and argv[argv.index("--family") + 1:][:1] == ["repository_history"]:
        import repository_history
        return repository_history.main(argv)
    if "--family" in argv and argv[argv.index("--family") + 1:][:1] == ["historical_access"]:
        import historical_access
        return historical_access.main(argv, access_started_ns)
    if "--family" in argv and argv[argv.index("--family") + 1:][:1] == ["small_file_delta_smoke"]:
        import small_file_delta_family
        return small_file_delta_family.main(argv)
    if "--deepseek-full" in argv:
        import storage_smoke
        return storage_smoke.main(["--storage-smoke", "deepseek-full", *[arg for arg in argv if arg != "--deepseek-full"]])
    if "--storage-smoke" in argv:
        import storage_smoke
        return storage_smoke.main(argv)
    if argv[:1] == ["--prune-builds"] or argv[:1] == ["--prune-images"] or argv in (["--build-image"], ["--build-host"], ["--build-storage-smoke-image"]):
        # Pruning deletes state a running lane in *this* worktree may be about to
        # read, so it takes this worktree's own lock. A build takes none (owner
        # direction, 2026-09-21): it writes only this worktree's target directory,
        # and two worktrees no longer exclude each other for it.
        takes_lock = argv[:1] in (["--prune-builds"], ["--prune-images"])
        with contextlib.ExitStack() as stack:
            if takes_lock:
                lock = stack.enter_context(isolation.worktree_lock_path().open("a"))
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError as error:
                    raise RuntimeError(
                        "another run in this worktree owns the measurement lock"
                    ) from error
            if argv[:1] == ["--prune-images"]:
                pruning = argparse.ArgumentParser(description="Retain recent benchmark images whose immutable binary archive is complete")
                pruning.add_argument("--prune-images", nargs="?", type=int, default=DEFAULT_RETAINED_SEALED_BUILDS,
                                     const=DEFAULT_RETAINED_SEALED_BUILDS, metavar="KEEP")
                pruning.add_argument("--apply", action="store_true", help="Delete the selected images (default: preview only)")
                options = pruning.parse_args(argv)
                receipt = image_retention(keep=options.prune_images, apply=options.apply)
                print(json.dumps(receipt, sort_keys=True))
                return 0
            if argv[:1] == ["--prune-builds"]:
                pruning = argparse.ArgumentParser(description="Retain recent owned Cargo targets; preserve immutable archives and inputs")
                pruning.add_argument("--prune-builds", nargs="?", type=int, default=DEFAULT_RETAINED_SEALED_BUILDS,
                                     const=DEFAULT_RETAINED_SEALED_BUILDS, metavar="KEEP")
                pruning.add_argument("--apply", action="store_true", help="Delete the selected old targets (default: preview only)")
                options = pruning.parse_args(argv)
                receipt = prune_build_caches(keep=options.prune_builds, apply=options.apply)
                print(json.dumps(receipt, sort_keys=True))
                return 0
            values = source_build_args()
            if argv == ["--build-host"]:
                binary = REPO / "target/release/fs-benchmark-pro"
                mode = build_mode()
                dependency_reuse = None
                if mode == "sealed":
                    build_target = HOST_ROOT / "builds" / ("native-" + values["LAYERFS_COMPILATION_SEAL"])
                    dependency_reuse = seed_host_dependencies(build_target, values, binary)
                else:
                    build_target = incremental_build_target()
                    build_target.mkdir(parents=True, exist_ok=True)
                # Both branches write inside this worktree; a target directory that
                # escapes it would be shared build state with a sibling worktree.
                isolation.assert_target_owned(build_target)
                try:
                    result = runtime.run(["cargo", "+1.85.1", "build", "--locked", "--release", "-j" + values["LAYERFS_HOST_BUILD_JOBS"], "-p", "fs-benchmark-pro", "--bin", "fs-benchmark-pro", "--target-dir", str(build_target)],
                        deadline=runtime.Deadline.after(900), cwd=REPO, output_limit=1024**2, stream_output=True)
                except runtime.CommandFailure as error:
                    print(_text(error.result.stderr), file=sys.stderr)
                    return error.result.returncode or 1
                version = re.search(r"pub const SCHEMA_VERSION:\s*i64\s*=\s*(\d+)",
                    (REPO / "crates/layerfs-layerstack-store/src/schema.rs").read_text())
                if version is None:
                    raise ValueError("active Store schema version missing")
                schema_path = REPO / f"crates/layerfs-layerstack-store/sql/schema/v{version.group(1)}.sql"
                built = build_target / "release/fs-benchmark-pro"
                observed_schema = verify_linked_schema(built, int(version.group(1)))
                integrated_probe = verify_integrated_format(built) if int(version.group(1)) >= 10 else None
                if source_build_args() != values:
                    raise ValueError("source changed during qualified build")
                binary.parent.mkdir(parents=True, exist_ok=True)
                archive_binary(binary)
                copy_executable(built, binary)
                identity = {**values, "native_build_wall_ns": result.wall_ns, "dependency_reuse": dependency_reuse,
                            "build_mode": mode, "build_target": str(build_target),
                            "recompiled_packages": recompiled_packages(result),
                            "observed_schema_version": observed_schema, "binary_sha256": runtime.file_sha256(binary),
                            "platform": platform.platform(), "rust_toolchain": "1.85.1",
                            "schema_sha256": runtime.file_sha256(schema_path), "integrated_format_probe":integrated_probe}
                Path(str(binary) + ".identity.json").write_text(json.dumps(identity, sort_keys=True))
                archive_binary(binary)
                print(binary)
                return 0
            tag = "layerfs-bench-infra:" + values["LAYERFS_SOURCE_SEAL"][:16]
            if argv == ["--build-storage-smoke-image"]:
                # The owner limits executable verification to three mounted smokes.
                values["LAYERFS_BUILD_SELF_CHECK"] = "0"
            try:
                result = runtime.build_image(REPO, tag, values, deadline=runtime.Deadline.after(900), jobs=2)
            except runtime.CommandFailure as error:
                print(_text(error.result.stderr)[-16384:], file=sys.stderr)
                return error.result.returncode or 1
            if result.returncode:
                print(_text(result.stderr)[-16384:], file=sys.stderr)
                return result.returncode
            if any(values.get(key) != value for key, value in source_build_args().items()):
                raise ValueError("source changed during qualified image build")
            archive_image(tag)
            print(tag)
            return 0
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.output == parser.get_default("output"):
        args.output = str(HOST_ROOT / "results" / ("run-" + uuid.uuid4().hex[:12]))
    if args.verification:
        parser.error("use the family verify.sh or verify-selected.py for bounded verification")
    if args.prepare_only and (args.perf_fast or args.perf_samples is not None):
        parser.error("preparation-only cannot also select a performance mode")
    # One lock per worktree (owner direction, 2026-09-21): two runs in this
    # worktree still never overlap, because they share the host store, the prepared
    # masters and the image archive. A run in another worktree is not excluded and
    # does not exclude this one; the interference it causes is recorded, not
    # prevented.
    lock_path = isolation.worktree_lock_path()
    with lock_path.open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("another run in this worktree owns the measurement lock")
        selection = resolve_selection(args, time.monotonic() + 20)
        if args.list:
            print(json.dumps(selection, sort_keys=True))
            return 0
        if args.prepare_only:
            result = execute_selected(args, deadline=time.monotonic() + args.setup_timeout)
            print(json.dumps(result, sort_keys=True))
            return 0 if result["status"] == "PASS" else 1
        if selection.get("proof_only"):
            parser.error("proof-only case cannot run performance")
        count = args.perf_samples or 1
        output = Path(args.output)
        output.mkdir(parents=True, exist_ok=True)
        path = output / "perf.jsonl"
        samples = []
        with path.open("x") as stream:
            def emit(row):
                stream.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
                stream.flush()
            emit({"kind": "header", "schema": "layerfs-perf-v1", "identities": selection,
                  "requested_samples": count, "full_workload": True, "cpus": args.cpus,
                  "product_target_ns": cold.TARGET_NS if cold.applies(selection) else PRODUCT_TARGET_NS,
                  "cache_contract": cold.CONTRACT if cold.applies(selection) else None,
                  "command_allowance_seconds": args.timeout,
                  "product_execution_allowance_seconds": args.product_timeout,
                  "collection_mode": bool(args.collection_mode),
                  "historical_product_target_scope": HISTORICAL_PRODUCT_TARGET_SCOPE,
                  "memory_mib": args.memory_mib, "resource_limit_scope": "Linux container only; host CPU not capped", "verification_status": "NOT_RUN"})
            for index in range(1, count + 1):
                row = execute_selected(args, deadline=time.monotonic() + args.setup_timeout + args.timeout + 10)
                row.update(kind="sample", sample_index=index)
                emit(row)
                samples.append(row)
                key, value = _timer(row)
                print(f"{args.family} {args.case} sample={index} {row['status']} {key}={value} historical_target={row.get('historical_product_target_status')} slow={row.get('slow', False)}", flush=True)
                if row["status"] not in ("PASS", "TARGET_MISS", "INELIGIBLE"):
                    (output / "failure.log").write_text(str(row.get("error", row.get("cleanup")))[:1024**2])
                    break
                if row["status"] == "TARGET_MISS" and not args.collection_mode:
                    (output / "failure.log").write_text(str(row.get("error", row.get("cleanup")))[:1024**2])
                    break
            summary = performance_summary(samples, count, args.collection_mode)
            emit(summary)
        return 0 if summary["status"] == "PASS" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, TimeoutError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)

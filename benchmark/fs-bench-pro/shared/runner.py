#!/usr/bin/env python3
"""Selected Docker/Linux benchmarks. One compact log, independent samples."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import math
import os
import platform
import shutil
from pathlib import Path
import statistics
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
REPO = BENCH.parent.parent
sys.path.insert(0, str(HERE))
import runtime

HOST_FAMILIES = ("payload_create_read", "dedup_workspace_reuse", "dedup_cross_file", "dedup_cdc_locality",
                 "edit_length_preserving", "edit_length_changing", "edit_canonical_chunk_count",
                 "init_namespace", "store_footprint")
TIMERS = {"workspace": "pure_call_sum_ns", "sdk": "edit_commit_ns",
          "namespace": "layerstack_init_ns", "store-footprint": "product_call_sum_ns"}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def harness_identity():
    paths = [HERE / "runner.py", HERE / "runtime.py", BENCH / "verify-selected.py"]
    return digest({str(path.relative_to(BENCH)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths})


def build_parser(include_modes=True):
    p = argparse.ArgumentParser(description="Full Docker/FUSE workloads; fast means one full sample, never reduced work.")
    p.add_argument("--family", required=True)
    p.add_argument("--topology", choices=("docker", "host-store"), default="host-store")
    p.add_argument("--host-binary", default=str(REPO / "target/release/fs-benchmark-pro"))
    p.add_argument("--source-arm", choices=("baseline", "candidate"), default="candidate")
    p.add_argument("--performance-rows", default="-")
    p.add_argument("--case")
    p.add_argument("--seed", type=int)
    p.add_argument("--repetition", type=int)
    p.add_argument("--setup", choices=("fresh", "clone"))
    if include_modes:
        mode = p.add_mutually_exclusive_group()
        mode.add_argument("--perf-fast", action="store_true")
        mode.add_argument("--perf-samples", type=int)
        mode.add_argument("--verification", action="store_true")
    p.add_argument("--prepare-only", action="store_true")
    p.add_argument("--smoke", action="store_true", help="Select the smallest registered supported case; performance/setup only")
    p.add_argument("--list", action="store_true", help="Print registered selections without running workloads")
    p.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    p.add_argument("--source", "--source-identity", dest="source")
    p.add_argument("--input", "--input-identity", dest="input")
    p.add_argument("--output", default=str(REPO / "benchmark-results" / "infra" / ("run-" + uuid.uuid4().hex[:12])))
    p.add_argument("--timeout", type=float, default=15, help="Selected product command budget in seconds; slow cases stop, not scale up")
    p.add_argument("--setup-timeout", type=float, default=120)
    p.add_argument("--cpus", type=int, default=2)
    p.add_argument("--memory-mib", type=int, default=2048)
    return p


def _deadline(end):
    return runtime.Deadline(end)


def _command(argv, end, **kw):
    return runtime.run(argv, deadline=_deadline(end), **kw)


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
    return {"LAYERFS_SOURCE_COMMIT": git("rev-parse", "HEAD"),
            "LAYERFS_SOURCE_TREE": git("rev-parse", "HEAD^{tree}"),
            "LAYERFS_SOURCE_DIRTY": "true", "LAYERFS_SOURCE_SEAL": source.hexdigest(),
            "LAYERFS_PRODUCT_SEAL": product.hexdigest(),
            "WORKLOAD_SOURCE_SHA256": hashlib.sha256((BENCH / "workload/main.rs").read_bytes()).hexdigest()}


def image_info(image, deadline):
    value = json.loads(_text(_command(["docker", "image", "inspect", image], deadline).stdout))[0]
    if value.get("Config", {}).get("Volumes"):
        raise ValueError("image-declared volumes are forbidden")
    return value


def resolve_selection(args, deadline):
    if getattr(args, "_selection", None):
        return args._selection
    if not args.image:
        raise ValueError("select a built Linux image with --image or LAYERFS_BENCH_IMAGE; use shared/runner.py --build-image separately")
    if (not 1 <= args.cpus <= 8 or args.memory_mib <= 0
            or not math.isfinite(args.timeout) or args.timeout <= 0
            or not math.isfinite(args.setup_timeout) or args.setup_timeout <= 0):
        raise ValueError("invalid resource/budget selection")
    if getattr(args, "perf_samples", None) is not None and args.perf_samples <= 0:
        raise ValueError("--perf-samples must be a positive integer")
    info = image_info(args.image, deadline)
    identity = info.get("Config", {}).get("Labels", {}) or {}
    source = identity.get("dev.layerfs.source-seal")
    if not source:
        raise ValueError("image lacks source seal")
    host = args.topology == "host-store"
    host_identity = None
    if host:
        if platform.system() != "Darwin":
            raise ValueError("host Store qualification requires macOS + Docker Desktop")
        if args.family not in HOST_FAMILIES:
            raise ValueError("family host migration is deferred to issue #39")
        host_identity = json.loads(Path(args.host_binary + ".identity.json").read_text())
        if runtime.file_sha256(args.host_binary) != host_identity["binary_sha256"]:
            raise ValueError("host binary seal mismatch; rebuild with --build-host")
        if host_identity["LAYERFS_PRODUCT_SEAL"] != identity.get("dev.layerfs.product-seal"):
            raise ValueError("host and Linux image product seals differ")
        source = host_identity["LAYERFS_SOURCE_SEAL"]
    result = _command(([args.host_binary, "infra-list", args.family] if host else ["docker", "run", "--rm", "--network", "none", "--cpus", "1", "--memory", "256m",
                       "--entrypoint", "/usr/local/bin/fs-benchmark-pro", info["Id"], "infra-list", args.family])
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
    if len(matches) != 1:
        raise ValueError("select exactly one registered --case (or --smoke for performance)")
    row = matches[0]
    if not row.get("supported", True):
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
    input_identity = digest({"family": args.family, "case": args.case, "seed": 1 if inherited else seed,
                             "source": source, "recipe": row})
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
                     "topology": args.topology, "host_cpu_capped": False if host else None},
                 "verification_supported": row.get("verification_supported", True)}
    selection["source_arm"] = args.source_arm
    selection["timer"] = TIMERS.get(row.get("route"))
    selection["topology"] = args.topology
    if host:
        selection.update(host_executor=host_identity, image_source_identity=identity.get("dev.layerfs.source-seal"),
                         host_environment={"os": platform.system(), "architecture": platform.machine(), "cpu_count": os.cpu_count()})
    args._selection = selection
    return selection


HOST_ROOT = REPO / "benchmark-results/host-store"


def _host_acquire(args, selection, deadline):
    sdk = selection.get("route") == "sdk"
    fixture_command = [args.host_binary, "infra-fixture-info", selection["family"], selection["case"], str(selection["seed"])]
    fixture = records(_command(fixture_command, deadline).stdout)[-1]
    native = selection["setup_identity"] == "fresh-output"
    compatibility = {"contract": "sdk-edit-prepared-store-cache-v1" if sdk else "layerfs-canonical-v5-workspace-fixture-v1",
        "fixture": fixture, "schema_sha256": selection["host_executor"]["schema_sha256"]}
    if not sdk and selection.get("route") not in ("namespace", "store-footprint"):
        compatibility["seed"] = selection["seed"]
    if native:
        compatibility.update(family=selection["family"], case=selection["case"])
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
                                             str(selection["fixture_bytes"])], deadline).stdout)[-1]
                (staging / "payload/branch-id").write_text(prepared["branch_id"])
                qualifications = ["family\tcase\tplan\tinitial\texpected\tfile\tmap\tinitial_count\tfinal_count\tdigest\n"]
                for family in ("edit_length_preserving", "edit_length_changing", "edit_canonical_chunk_count"):
                    listed = records(_command([args.host_binary, "infra-list", family], deadline).stdout)
                    for row in listed:
                        if row.get("fixture_bytes") == selection["fixture_bytes"] and row.get("supported", True):
                            output = _command([args.host_binary, "sdk-edit-qualify", str(staging / "payload"),
                                               prepared["branch_id"], family, row["scenario_id"]], deadline).stdout
                            qualifications.append(_text(output))
                (staging / "qualification.tsv").write_text("".join(qualifications))
                (staging / "manifest.json").write_text(json.dumps({
                    "schema": "fs-bench-infra-prepared-v1",
                    "input_qualification_sha256": runtime.file_sha256(staging / "qualification.tsv")
                }))
            else:
                _command([args.host_binary, "infra-prepare", selection["family"], selection["case"], str(selection["seed"]), str(staging)], deadline,
                         env={"LAYERFS_BENCH_HOST_STORE": "1", "LAYERFS_BENCH_LOCAL_RUNTIME": "0"})
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
    removed = [] if fresh else runtime.evict_host_cache(HOST_ROOT, root)
    if native:
        fixture = json.loads((root / "fixture.json").read_text())
    return {"image": selection["image"], "host_root": str(root), "cache_key": key, "cache_hit": hit,
            "one_shot": fresh, "producer": manifest["producer"], "compatibility": compatibility,
            "fixture": fixture, "data_bytes": manifest["data_bytes"], "evicted": removed,
            "input_validation": "owned-prepared-recipe" if native else "full-master-identity"}


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


def _acquire(args, selection, deadline):
    if args.topology == "host-store":
        return _host_acquire(args, selection, deadline)
    key = digest({"input": selection["input_identity"], "runtime": selection["image"], "setup": selection["setup_identity"]})
    fresh = selection["setup_identity"] == "fresh"
    if fresh:
        key = digest({"key": key, "one_shot": uuid.uuid4().hex})
    cache = runtime.PreparedCache("layerfs-bench-infra")
    labels = {"family": selection["family"], "source": selection["source_identity"], "input": selection["input_identity"]}
    prepared = runtime.create_prepared_image(selection["image"], key,
        ["infra-prepare", selection["family"], selection["case"], str(selection["seed"]), "/var/lib/fs-bench/prepared"],
        labels, deadline=_deadline(deadline), cache=cache, retain=not fresh)
    return prepared


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


def execute_selected(args, *, deadline, verification=False):
    """No receipt files here: the caller publishes after this function cleans up."""
    started = time.monotonic_ns()
    selection = resolve_selection(args, deadline)
    if selection.get("proof_only") and not verification and not args.prepare_only:
        raise ValueError("proof-only case does not support performance")
    if verification and not selection["verification_supported"]:
        return {"status": "INCOMPLETE", "identities": selection, "omissions": ["proof cannot fit bounded verification"], "cleanup": {"status": "PASS", "not_started": True}}
    sample = None
    sample_name = None
    host_sample_path = None
    host = args.topology == "host-store"
    prepared = None
    result = {"status": "INCOMPLETE", "identities": selection, "checks": [], "omissions": [],
              "phase": "preparation",
              "sampled_paths_or_ranges": [], "reused_proof_identities": [],
              "resource_precision": "separate host process CPU/RSS/IO and container lifetime peak/command CPU" if host else "sample-container lifetime peak; command-window CPU/IO deltas"}
    work_end = deadline - 4
    try:
        setup_started = time.monotonic_ns()
        prepared = _acquire(args, selection, work_end)
        result["preparation"] = prepared
        if args.prepare_only:
            result["status"] = "PASS"
            return result
        name = "layerfs-infra-sample-" + uuid.uuid4().hex[:12]
        sample_name = name
        result["phase"] = "runtime-start"
        sample = runtime.start_sample(prepared["image"], name,
            {"family": selection["family"], "run": name}, deadline=_deadline(work_end),
            cpus=args.cpus, memory_bytes=args.memory_mib * 1024**2, host_store=host)
        result["phase"] = "sample-setup"
        result["environment_observation"] = sample.observation
        if host:
            host_sample_path = HOST_ROOT / "samples" / name
        result["setup"] = _host_sample(prepared, selection, name, work_end) if host else runtime.prepare_sample(sample, mode="clone" if selection["setup_identity"] == "fresh" else selection["setup_identity"], deadline=_deadline(work_end),
            reuse_prepared_input=selection["family"] in ("dedup_cross_file", "dedup_cdc_locality"))
        result["preparation_wall_ns"] = time.monotonic_ns() - setup_started
        before = cgroup_snapshot(sample, work_end)
        run_started = time.monotonic_ns()
        result["phase"] = "product-command"
        command_end = min(work_end, time.monotonic() + (45 if verification else args.timeout))
        command_env = {"LAYERFS_V013_IMAGE": selection["image"],
                       "LAYERFS_BENCH_SOURCE_ARM": selection["source_arm"]}
        if args.performance_rows != "-":
            command_env["LAYERFS_SDK_EDIT_PERFORMANCE_ROWS"] = args.performance_rows
        if selection["family"] in ("dedup_cross_file", "dedup_cdc_locality"):
            command_env["LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE"] = selection["input_identity"][:16]
        if host:
            command_env.update(LAYERFS_BENCH_HOST_STORE="1", LAYERFS_BENCH_LOCAL_RUNTIME="0",
                LAYERFS_EXEC_TRANSPORT="daemon", LAYERFS_FUSE_TRANSPORT="daemon",
                LAYERFS_BENCH_WORKLOAD="/usr/local/bin/fs-benchmark-workload",
                LAYERFS_BENCH_PREPARED_INPUT=result["setup"].get("prepared_input_root", str(Path(prepared["host_root"]) / "payload/input")),
                TMPDIR=str(host_sample_path))
        operation = ["infra-run", selection["family"], selection["case"], str(selection["seed"]),
                     "verify" if verification else "performance", str(host_sample_path) if host else "/var/lib/fs-bench/sample", sample.id]
        command = (_command([args.host_binary, *operation], command_end, env=command_env, output_limit=1024**2)
                   if host else sample.exec_coordinator(operation, deadline=_deadline(command_end), output_limit=1024**2, env=command_env))
        result["command_wall_ns"] = time.monotonic_ns() - run_started
        result["records"] = records(command.stdout)
        result["records"].extend(initialization_diagnostics(command.stderr))
        if not verification:
            timer, elapsed = _timer(result)
            if elapsed is None:
                raise RuntimeError(f"missing declared product timer: {timer}")
            if elapsed > 15_000_000_000:
                raise RuntimeError("product time exceeded the shared 15-second limit")
        if command.truncated:
            raise RuntimeError("selected command output exceeded compact receipt limit")
        result["phase"] = "resource-finalization"
        after = cgroup_snapshot(sample, work_end)
        result["resources"] = {"command_window_cpu_ns": (after["usage_usec"] - before["usage_usec"]) * 1000,
            "sample_container_lifetime_peak_bytes": after["memory_peak"],
            "memory_current_bytes": after["memory_current"], "swap_current_bytes": after["swap_current"],
            "oom_kill_delta": after.get("oom_kill", 0) - before.get("oom_kill", 0),
            "measurement_scope": "inclusive container command window including coordinator; not operation-only peak"}
        if host:
            result["resources"]["measurement_scope"] = "Linux daemon/FUSE container command window; host coordinator/Store process CPU/RSS/IO reported separately in records; host CPU is not container-capped"
        result["status"] = "PASS" if command.returncode == 0 and result["records"] and not result["resources"]["oom_kill_delta"] else "FAIL"
        result["slow"] = result["command_wall_ns"] >= 5_000_000_000
        result["checks"] = [r for r in result["records"] if "verif" in str(r.get("kind", "")) or "proof" in str(r.get("kind", ""))]
        if result["status"] != "PASS":
            result["error"] = _text(command.stderr)[-8192:]
        else:
            result["phase"] = "complete"
    except Exception as error:
        result["status"] = "TIMEOUT" if isinstance(error, TimeoutError) or "timeout" in str(error).lower() or "deadline" in str(error).lower() else "FAIL"
        failed_command = getattr(error, "result", None)
        detail = _text(failed_command.stderr)[-8192:] if failed_command is not None else ""
        result["error"] = (str(error) + "\n" + detail)[-8192:]
        if failed_command is not None:
            result.setdefault("records", records(failed_command.stdout))
            if result["phase"] == "product-command":
                result["command_wall_ns"] = failed_command.wall_ns
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
            if host and prepared and selection["setup_identity"] != "fresh-output":
                master = Path(prepared["host_root"])
                manifest = json.loads((master / "host-cache.json").read_text())
                if runtime.host_tree_identity(master, _deadline(deadline)) != manifest["files"]:
                    raise RuntimeError("prepared host master changed during sample")
                result["prepared_master_unchanged"] = True
            if prepared and prepared.get("one_shot") and host:
                runtime.remove_host_owned(prepared["host_root"])
            elif prepared and prepared.get("one_shot"):
                _command(["docker", "image", "rm", prepared.get("cache_tag", prepared["image"])], deadline)
            result["cleanup"] = {"status": "PASS", "wall_ns": time.monotonic_ns() - cleanup_started}
        except Exception as error:
            result["cleanup"] = {"status": "FAIL", "error": str(error)[-2048:]}
            result["status"] = "INCOMPLETE"
        result["wall_ns"] = time.monotonic_ns() - started
    return result


def _timer(row):
    declared = row.get("identities", {}).get("timer")
    for record in reversed(row.get("records", [])):
        keys = (declared,) if declared else ("layerstack_init_ns",) if row.get("identities", {}).get("route") == "namespace" else (
            "edit_commit_ns", "edit_commit_end_ns", "pure_call_sum_ns", "initialize_ns", "execution_ns", "complete_ns", "complete_lifecycle_ns")
        for key in keys:
            if isinstance(record.get(key), (float, int)):
                return key, record[key]
    return declared or "unavailable", None


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv in (["--build-image"], ["--build-host"]):
        lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
        with lock_path.open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError("another benchmark owns the measurement lock") from error
            values = source_build_args()
            if argv == ["--build-host"]:
                binary = REPO / "target/release/fs-benchmark-pro"
                result = runtime.run(["cargo", "+1.85.1", "build", "--locked", "--release", "-j2", "-p", "fs-benchmark-pro"],
                    deadline=runtime.Deadline.after(900), cwd=REPO, output_limit=1024**2)
                identity = {**values, "binary_sha256": runtime.file_sha256(binary), "platform": platform.platform(), "rust_toolchain": "1.85.1", "schema_sha256": runtime.file_sha256(REPO / "crates/layerfs-layerstack-store/sql/schema/v5.sql")}
                Path(str(binary) + ".identity.json").write_text(json.dumps(identity, sort_keys=True))
                print(binary)
                return 0
            tag = "layerfs-bench-infra:" + values["LAYERFS_SOURCE_SEAL"][:16]
            try:
                result = runtime.build_image(REPO, tag, values, deadline=runtime.Deadline.after(900), jobs=2)
            except runtime.CommandFailure as error:
                print(_text(error.result.stderr)[-16384:], file=sys.stderr)
                return error.result.returncode or 1
        if result.returncode:
            print(_text(result.stderr)[-16384:], file=sys.stderr)
            return result.returncode
        print(tag)
        return 0
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.topology == "host-store" and args.output == parser.get_default("output"):
        args.output = str(HOST_ROOT / "results" / ("run-" + uuid.uuid4().hex[:12]))
    if args.verification:
        parser.error("use the family verify.sh or verify-selected.py for bounded verification")
    if args.prepare_only and (args.perf_fast or args.perf_samples is not None):
        parser.error("preparation-only cannot also select a performance mode")
    # ponytail: one process lock serializes this benchmark; no concurrent resource-sensitive samples.
    lock_path = Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock"
    with lock_path.open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("another benchmark owns the measurement lock")
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
                  "memory_mib": args.memory_mib, "resource_limit_scope": "Linux container only; host CPU not capped" if args.topology == "host-store" else "entire Linux sample container", "verification_status": "NOT_RUN"})
            for index in range(1, count + 1):
                row = execute_selected(args, deadline=time.monotonic() + args.setup_timeout + args.timeout + 10)
                row.update(kind="sample", sample_index=index)
                emit(row)
                samples.append(row)
                key, value = _timer(row)
                print(f"{args.family} {args.case} sample={index} {row['status']} {key}={value} slow={row.get('slow', False)}", flush=True)
                if row["status"] != "PASS":
                    (output / "failure.log").write_text(str(row.get("error", row.get("cleanup")))[:1024**2])
                    break
            valid = [row for row in samples if row["status"] == "PASS"]
            times = [value for row in valid if (value := _timer(row)[1]) is not None]
            summary = {"kind": "summary", "requested": count, "attempted": len(samples), "valid": len(valid),
                       "status": "PASS" if len(valid) == count else "INCOMPLETE", "verification_status": "NOT_RUN",
                       "timer": _timer(valid[0])[0] if valid else None,
                       "median_ns": statistics.median(times) if times else None,
                       "min_ns": min(times) if times else None, "max_ns": max(times) if times else None}
            emit(summary)
        return 0 if summary["status"] == "PASS" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, TimeoutError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)

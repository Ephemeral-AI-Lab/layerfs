#!/usr/bin/env python3
"""Run one bounded, identity-pinned fs-bench-pro verification."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time
import uuid


STARTED_NS = time.monotonic_ns()
HARD_LIMIT_NS = 59_000_000_000
WORK_LIMIT_NS = 54_000_000_000
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_exclusive(path, value):
    data = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o444)
    try:
        os.write(descriptor, data)
    finally:
        os.close(descriptor)


def stop_group(child):
    cleanup_started = time.monotonic_ns()
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=1)
    return time.monotonic_ns() - cleanup_started


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--family", required=True)
    parser.add_argument("--case", required=True)
    parser.add_argument("--seed", required=True, type=int, choices=(1, 2, 3))
    parser.add_argument("--source-arm", required=True, choices=("baseline", "candidate"))
    parser.add_argument("--assets", required=True)
    parser.add_argument("--output", required=True)
    proof = parser.add_mutually_exclusive_group()
    proof.add_argument("--verification-certificate")
    proof.add_argument("--independent-current", action="store_true")
    args = parser.parse_args()
    for value in (args.family, args.case):
        if value == "all" or any(marker in value for marker in ("..", ",", "*", "[", "]")):
            parser.error("bulk selections and ranges are forbidden")
    return args


def namespace_command(args, assets, evidence):
    if not args.independent_current or args.verification_certificate:
        raise ValueError("namespace verification requires --independent-current")
    manifest = assets / "scenarios" / args.case / "fixture-manifest.json"
    container = assets / "environment" / "container-inspect.json"
    if not manifest.is_file() or not container.is_file():
        raise ValueError("namespace assets require one sealed benchmark input and container identity")
    fixture = json.loads(manifest.read_text())
    inspected = json.loads(container.read_text())
    if isinstance(inspected, list):
        if len(inspected) != 1:
            raise ValueError("namespace container identity cardinality")
        inspected = inspected[0]
    name = str(inspected.get("Name", "")).removeprefix("/")
    if fixture.get("scenario") != args.case or not name:
        raise ValueError("namespace input identity mismatch")
    run_id = "verify-selected-" + uuid.uuid4().hex
    command = [
        str(HERE / "run-namespace.sh"), run_id, name,
        "--case", args.case, "--seed", str(args.seed),
        "--source", args.source_arm, "--mode", "verify",
    ]
    environment = dict(os.environ)
    environment.update(
        LAYERFS_NAMESPACE_RESULTS_ROOT=str(evidence),
        LAYERFS_NAMESPACE_FIXTURE_ROOT=str(assets / "scenarios"),
        CARGO_BUILD_JOBS="8",
    )
    environment_root = assets / "environment"
    def identity(name):
        path = environment_root / name
        return path.read_text().strip() if path.is_file() else None
    source_seal = identity("source-seal.sha256")
    current_seal = subprocess.check_output(
        [str(HERE / "run-namespace.sh"), "--source-seal"], text=True, cwd=REPO
    ).strip()
    if not source_seal or source_seal != current_seal:
        raise ValueError("namespace assets do not match the current source seal")
    return command, environment, evidence / run_id, {
        "source_revision": identity("harness-head.commit"),
        "source_seal": source_seal,
        "product_identity": identity("product-source-seal.sha256"),
        "harness_identity": identity("harness-source-seal.sha256"),
        "fixture_manifest": str(manifest),
        "fixture_manifest_sha256": sha256(manifest),
        "fixture_digest": fixture.get("fixture_digest"),
        "logical_bytes": fixture.get("logical_bytes"),
        "regular_files": fixture.get("regular_files"),
    }


def workspace_command(args, assets, evidence):
    build = assets / "evidence" / "build.json"
    if not build.is_file():
        raise ValueError("Workspace assets require a sealed build")
    proof_case = args.case.endswith("-proof")
    command = [
        sys.executable, str(HERE / "workspace-runner.py"),
        "--family", args.family, "--case", args.case, "--seed", str(args.seed),
        "--source-arm", "baseline" if args.source_arm == "baseline" else "corrected",
        "--mode", "verify" if proof_case else "fast-verify",
        "--assets", str(assets), "--output", str(evidence),
    ]
    if proof_case:
        if not args.independent_current or args.verification_certificate:
            raise ValueError("proof verification requires --independent-current")
    elif args.verification_certificate:
        command += ["--verification-certificate", str(Path(args.verification_certificate).resolve())]
    elif args.independent_current:
        command.append("--fast-no-reuse")
    else:
        raise ValueError("Workspace verification requires one certificate or --independent-current")
    build_identity = json.loads(build.read_text())
    return command, dict(os.environ), evidence, {
        "build_manifest": str(build),
        "build_manifest_sha256": sha256(build),
        "source_revision": build_identity.get("revision"),
        "product_identity": build_identity.get("product_seal"),
        "harness_identity": build_identity.get("harness_seal"),
        "image_id": build_identity.get("image_id"),
    }


def namespace_result(evidence, case, arm, seed):
    path = evidence / "scenarios" / case / arm / str(seed) / "result.json"
    if not path.is_file():
        return None, [], [], None, "incomplete"
    row = json.loads(path.read_text())
    passed = (
        row.get("mode") == "verify"
        and row.get("status") == "pass"
        and row.get("cleanup_status") == "pass"
        and row.get("resource_status") == "pass"
    )
    checks = [
        "independent fixture digest", "fresh reopened root", "bounded verifier resources",
        "runtime cleanup",
    ]
    sampled = row.get("sampled_paths", [{"scope": "bounded selected case", "regular_files": row.get("observed_file_count")}])
    return passed, checks, sampled, {"result": str(path), "sha256": sha256(path)}, row.get("cleanup_status")


def workspace_result(evidence, case, seed):
    slots = evidence / "slots.json"
    if not slots.is_file():
        return None, [], [], None, "incomplete"
    rows = [row for row in json.loads(slots.read_text()).values()
            if row.get("scenario_id") == case and row.get("seed") == seed
            and row.get("mode") == ("verify" if case.endswith("-proof") else "fast-verify")]
    if len(rows) != 1:
        return None, [], [], None, "incomplete"
    row = rows[0]
    passed = (
        row.get("coverage_status") == "executed"
        and row.get("product_status") == "pass"
        and row.get("harness_status") not in {"fail", "needs-review"}
        and row.get("supervisor_cleanup_status") == "pass"
        and not row.get("timeout")
    )
    checks = ["existing Workspace oracle", "selected canonical/content checks", "resource receipt", "runtime cleanup"]
    sampled = row.get("fast_sampled_paths", row.get("sampled_paths", []))
    return passed, checks, sampled, {"result": row.get("evidence_path"), "slots_sha256": sha256(slots)}, row.get("supervisor_cleanup_status")


def main():
    args = parse_args()
    output = Path(args.output).resolve()
    if output.exists():
        raise SystemExit("output already exists; verification receipts are immutable")
    output.mkdir(parents=True)
    receipt_path = output / "verification.json"
    evidence = output / "evidence"
    evidence.mkdir()
    assets = Path(args.assets).resolve()
    status = "INCOMPLETE"
    cleanup = {"status": "not-started", "wall_ns": 0}
    checks, sampled, proof = [], [], None
    child = None
    stdout = output / "runner.stdout"
    stderr = output / "runner.stderr"
    error = None
    try:
        if not assets.is_dir():
            raise ValueError("assets must be an existing directory")
        if args.family == "init_namespace":
            command, environment, result_root, input_identity = namespace_command(args, assets, evidence)
        else:
            command, environment, result_root, input_identity = workspace_command(args, assets, evidence)
        remaining = (WORK_LIMIT_NS - (time.monotonic_ns() - STARTED_NS)) / 1_000_000_000
        if remaining <= 0:
            raise TimeoutError("setup consumed the verification work budget")
        with stdout.open("xb") as out, stderr.open("xb") as err:
            child = subprocess.Popen(command, cwd=REPO, env=environment, stdout=out, stderr=err,
                                     start_new_session=True)
            try:
                child.wait(timeout=remaining)
            except subprocess.TimeoutExpired as cause:
                raise TimeoutError("verification work exceeded 54 seconds") from cause
        cleanup["status"] = "pass"
        if args.family == "init_namespace":
            passed, checks, sampled, proof, verifier_cleanup = namespace_result(result_root, args.case, args.source_arm, args.seed)
        else:
            passed, checks, sampled, proof, verifier_cleanup = workspace_result(result_root, args.case, args.seed)
        cleanup["verifier_status"] = verifier_cleanup
        status = "INCOMPLETE" if passed is None else "PASS" if child.returncode == 0 and passed else "FAIL"
    except TimeoutError as cause:
        status, error = "TIMEOUT", str(cause)
    except BaseException as cause:
        status, error = "INCOMPLETE", f"{type(cause).__name__}: {cause}"
    finally:
        if child is not None:
            try:
                cleanup["wall_ns"] = stop_group(child)
                cleanup["status"] = "pass"
            except BaseException as cause:
                cleanup.update(status="fail", error=f"{type(cause).__name__}: {cause}")
                if status == "PASS":
                    status = "INCOMPLETE"
        finished = time.monotonic_ns()
        if finished - STARTED_NS >= HARD_LIMIT_NS:
            status = "TIMEOUT" if status != "INCOMPLETE" else status
            error = error or "hard 59-second end-to-end limit reached"
        environment = {
            "platform": platform.platform(), "machine": platform.machine(), "python": platform.python_version()
        }
        environment_identity = hashlib.sha256(
            json.dumps(environment, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        receipt = {
            "schema": "layerfs-selected-verification-v1",
            "companion_sha256": sha256(__file__),
            "status": status,
            "family": args.family,
            "case": args.case,
            "seed": args.seed,
            "source_arm": args.source_arm,
            "source_identity": {
                key: input_identity.get(key) for key in ("source_revision", "source_seal")
            } if "input_identity" in locals() else None,
            "harness_identity": input_identity.get("harness_identity") if "input_identity" in locals() else None,
            "product_identity": input_identity.get("product_identity") if "input_identity" in locals() else None,
            "environment_identity": environment_identity,
            "environment": environment,
            "input_identity": input_identity if "input_identity" in locals() else {"assets": str(assets)},
            "checks": checks,
            "sampled_paths_or_ranges": sampled,
            "reused_proof_identities": [proof] if proof else [],
            "omissions": ["no exhaustive Phase 1 replay", "no per-sample full-file verification", "no history replay"],
            "cleanup": cleanup,
            "monotonic_start_ns": STARTED_NS,
            "monotonic_end_ns": finished,
            "wall_ns": finished - STARTED_NS,
            "hard_limit_ns": HARD_LIMIT_NS,
            "evidence_path": str(evidence),
            "stdout_sha256": sha256(stdout) if stdout.is_file() else None,
            "stderr_sha256": sha256(stderr) if stderr.is_file() else None,
            "error": error,
        }
        write_exclusive(receipt_path, receipt)
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())

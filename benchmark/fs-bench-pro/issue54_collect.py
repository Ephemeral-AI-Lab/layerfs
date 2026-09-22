#!/usr/bin/env python3
"""Collect the historical remaining-family scope or the complete #74/#75 checkpoint."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE / "shared"))
import runner
import isolation

PERF_FAMILIES = (
    "namespace_mutation",
    "directory_construction_traversal",
    "workspace_change_locality",
    "dedup_branch_history",
    "git_tool_workflow",
    "mixed_load_bearing",
)
PROOF_FAMILY = "workspace_reliability"
EXPECTED_PERF = {
    "namespace_mutation": 4,
    "directory_construction_traversal": 12,
    "workspace_change_locality": 16,
    "dedup_branch_history": 20,
    "git_tool_workflow": 4,
    "mixed_load_bearing": 4,
}
EXPECTED_PROOFS = 28
PRODUCT_TIMEOUT = 300
COMMAND_TIMEOUT = 310
SETUP_TIMEOUT = 600
SEED = 1


LOCK_REFUSAL = "another benchmark owns the measurement lock"


def retain_lock_refusal(output, stderr=""):
    """Retry only a proven pre-work refusal; keep immutable proof failures."""
    output = Path(output)
    receipt = output / "verification.json"
    archive = None
    if receipt.is_file():
        saved = json.loads(receipt.read_text())
        if (saved.get("status") != "INCOMPLETE" or LOCK_REFUSAL not in (saved.get("error") or "")
                or saved.get("source_identity") is not None or saved.get("checks")):
            return False
        archive = output.with_name(output.name + ".lock-refused-" + uuid.uuid4().hex[:8])
        output.rename(archive)
    elif output.exists() or LOCK_REFUSAL not in stderr:
        return False
    output.parent.mkdir(parents=True, exist_ok=True)
    with (output.parent / "lock-refusals.jsonl").open("a") as stream:
        stream.write(json.dumps({"time_ns": time.time_ns(), "output": str(output),
                                 "reason": LOCK_REFUSAL, "work_started": False,
                                 "stderr": stderr, "archived_output": str(archive) if archive else None}) + "\n")
    return True


def _run(argv, cwd=None):
    while True:
        if "--list" not in argv:
            lock_path = isolation.worktree_lock_path()
            with lock_path.open("a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
        result = subprocess.run(argv, cwd=cwd or REPO, text=True, capture_output=True)
        if result.returncode == 0 or "--output" not in argv:
            return result
        output = Path(argv[argv.index("--output") + 1])
        if not retain_lock_refusal(output, result.stderr):
            return result
        print(f"QUEUE pre-work lock refusal retained: {output.name}", flush=True)


def _sha256(path: Path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def validate_campaign(campaign, selected_families):
    """Bind the complete live registry to the prospective declaration before work."""
    if campaign.get("schema") != "layerfs-v014-campaign-v1":
        raise ValueError("unsupported campaign schema")
    expected_policy = {"sample_count": 1, "seed": SEED,
                       "product_timeout_seconds": PRODUCT_TIMEOUT,
                       "outer_timeout_seconds": COMMAND_TIMEOUT,
                       "setup_timeout_seconds": SETUP_TIMEOUT,
                       "verification_work_seconds": 45, "verification_hard_seconds": 59}
    if any(campaign.get(key) != value for key, value in expected_policy.items()):
        raise ValueError("campaign sample/timing policy differs from supported collector contract")
    if list(campaign["families"]) != list(selected_families):
        raise ValueError("campaign family membership/order differs")
    rows = [row for members, _ in selected_families.values() for row in members]
    ids = [row["scenario_id"] for row in rows]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate campaign case")
    for family, (members, _) in selected_families.items():
        if any(row["family_id"] != family for row in members):
            raise ValueError("case registered under incompatible family")
        counts = {"performance": sum(not row.get("proof_only") for row in members),
                  "proof_only": sum(bool(row.get("proof_only")) for row in members)}
        if counts != campaign["families"][family]:
            raise ValueError("campaign family cardinality differs: " + family)
    perf = [row["scenario_id"] for row in rows if not row.get("proof_only")]
    if perf != campaign["performance_order"] or ids != campaign["verification_order"]:
        raise ValueError("campaign case membership/order differs")
    encoded = "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows).encode()
    if hashlib.sha256(encoded).hexdigest() != campaign["registry_sha256"]:
        raise ValueError("campaign registry contract hash differs")
    excluded = campaign.get("long_test_exclusion")
    unsupported = [row["scenario_id"] for row in rows if not row.get("verification_supported", True)]
    if excluded:
        matched = [row for row in rows if row["scenario_id"] == excluded]
        if len(matched) != 1 or not matched[0].get("proof_only") or not campaign.get("long_test_reason", "").strip():
            raise ValueError("campaign exclusion must name one proof with a concrete reason")
    if any(case != excluded for case in unsupported):
        raise ValueError("unsupported verifier lacks explicit campaign exclusion")


def list_family(args, family):
    listed = _run([
        sys.executable, str(HERE / "shared/runner.py"),
        "--family", family, "--list", "--image", args.image,
        "--host-binary", args.host_binary,
    ])
    if listed.returncode != 0:
        raise RuntimeError(f"infra-list {family} failed: {listed.stderr[-4000:]}")
    payload = json.loads(listed.stdout)
    return payload["rows"], payload


def perf_status(path: Path):
    if not path.is_file():
        return None
    last = None
    with path.open() as stream:
        for line in stream:
            last = json.loads(line)
    return last


def load_performance(path, family, row, source, image):
    records = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    samples = [record for record in records if record.get("kind") == "sample"]
    if len(samples) != 1 or records[-1].get("kind") != "summary":
        raise ValueError(f"incomplete or duplicate performance receipt: {path}")
    sample = samples[0]
    identity = sample.get("identities", {})
    if any(identity.get(key) != value for key, value in (
        ("family", family), ("case", row["scenario_id"]),
        ("source_identity", source), ("image", image),
        ("harness_identity", runner.harness_identity()),
    )):
        raise ValueError(f"incompatible performance receipt: {path}")
    timer, elapsed = runner._timer(sample)
    ids = [record["row_id"] for record in sample.get("records", []) if "row_id" in record]
    if row.get("route") == "sdk" and len(ids) != 1:
        raise ValueError("SDK sample must supply one performance row identity")
    identity = {**identity, "performance_rows": ",".join(ids) or "-"}
    cold_result = runner.cold.assess(sample) if runner.cold.applies(identity) else None
    return {"family": family, "case": row["scenario_id"], "proof_only": False,
            "fixture_profile": row.get("fixture_profile"), "tier": row.get("tier"),
            "seed": SEED, "sample_count": 1, "route": row.get("route"),
            "status": cold_result["status"] if cold_result else records[-1].get("status"),
            "execution_status": sample.get("completion_status", sample.get("status")),
            "cold_qualification": cold_result,
            "diagnostic_elapsed_ns": elapsed if cold_result and not cold_result["qualification_eligible"] else None,
            "completion_status": sample.get("completion_status"),
            "historical_product_target_status": sample.get("historical_product_target_status"),
            "cleanup": sample.get("cleanup"), "resources": sample.get("resources"),
            "preparation_wall_ns": sample.get("preparation_wall_ns"),
            "command_wall_ns": sample.get("command_wall_ns"), "error": sample.get("error"),
            "timer": timer, "elapsed_ns": cold_result["eligible_elapsed_ns"] if cold_result else elapsed,
            "identities": identity,
            "receipt": str(path), "receipt_sha256": _sha256(path)}


def collect_row(args, family, row, output):
    case = row["scenario_id"]
    case_dir = output / "performance" / family / case
    receipt = case_dir / "perf.jsonl"
    listed = args.listed_families[family]
    if receipt.exists():
        result = load_performance(receipt, family, row, listed["source_identity"], listed["image"])
        return {**result, "reused": True}, "reuse"
    if case_dir.exists():
        raise ValueError(f"attempt directory exists without a receipt: {case_dir}")
    command = [
        sys.executable, str(HERE / "shared/runner.py"),
        "--family", family, "--case", case,
        "--repetition" if row.get("route") == "sdk" else "--seed", str(SEED),
        "--setup", "fresh" if row.get("setup_policy") == "fresh-output" else "clone",
        "--perf-fast", "--collection-mode",
        "--product-timeout", str(PRODUCT_TIMEOUT), "--timeout", str(COMMAND_TIMEOUT),
        "--setup-timeout", str(SETUP_TIMEOUT),
        "--image", args.image, "--host-binary", args.host_binary,
        "--source-arm", getattr(args, "source_arm", "candidate"),
        "--output", str(case_dir),
    ]
    started = time.monotonic()
    proc = _run(command)
    if receipt.exists():
        result = load_performance(receipt, family, row, listed["source_identity"], listed["image"])
    else:
        result = {"family": family, "case": case, "status": "INCOMPLETE", "sample_count": 0}
    result.update(returncode=proc.returncode, wall_seconds=time.monotonic() - started,
                  stdout_tail=proc.stdout[-4000:], stderr_tail=proc.stderr[-4000:])
    return result, "ran"


def validate_proof_preparation(spec, selected_families):
    rows = {row["scenario_id"]: row for members, _ in selected_families.values() for row in members}
    cases = spec.get("cases", [])
    if (spec.get("schema") != "issue91-proof-preparation-v1" or not cases
            or len(cases) != len(set(cases))
            or any(case not in rows or rows[case].get("setup_policy") != "fresh-output" for case in cases)
            or spec.get("setup_timeout_seconds") != SETUP_TIMEOUT
            or spec.get("verification_work_seconds") != 45
            or spec.get("verification_hard_seconds") != 59):
        raise ValueError("invalid proof preparation declaration or applicability")


def prepare_proof(args, family, row, output, identities):
    directory = output / "preparation" / family / row["scenario_id"]
    receipt = directory / "preparation.json"
    expected = {key: identities[key] for key in ("source_identity", "product_identity", "input_identity", "image")}
    expected.update(family=family, case=row["scenario_id"], seed=SEED,
                    setup_identity="fresh-output", harness_identity=runner.harness_identity())
    declaration_sha = _sha256(args.proof_preparation_declaration)
    if receipt.exists():
        saved = json.loads(receipt.read_text())
        raw = directory / "runner.json"
        if (saved.get("identities") != expected or saved.get("declaration_sha256") != declaration_sha
                or not raw.is_file() or _sha256(raw) != saved.get("runner_sha256")):
            raise ValueError("incompatible or changed proof preparation receipt")
    else:
        directory.mkdir(parents=True, exist_ok=False)
        command = [sys.executable, str(HERE / "shared/runner.py"), "--family", family,
                   "--case", row["scenario_id"], "--prepare-only", "--setup", "fresh",
                   "--repetition" if row.get("route") == "sdk" else "--seed", str(SEED),
                   "--image", args.image, "--host-binary", args.host_binary,
                   "--setup-timeout", str(SETUP_TIMEOUT), "--output", str(directory)]
        _write(directory / "command.json", command)
        started = time.monotonic_ns()
        proc = _run(command)
        wall_ns = time.monotonic_ns() - started
        raw = directory / "runner.json"
        raw.write_text(proc.stdout)
        (directory / "stderr.log").write_text(proc.stderr)
        try:
            result = json.loads(proc.stdout)
        except (ValueError, TypeError):
            result = {}
        actual = result.get("identities", {})
        matched = all(actual.get(key) == value for key, value in expected.items())
        passed = proc.returncode == 0 and result.get("status") == "PASS" and matched and result.get("cleanup", {}).get("status") == "PASS"
        saved = {"status": "PASS" if passed else "INCOMPLETE", "identities": expected,
                 "declaration_sha256": declaration_sha, "command": command,
                 "returncode": proc.returncode, "wall_ns": wall_ns,
                 "runner_sha256": _sha256(raw), "error": None if passed else "preparation failed or identity/cleanup incomplete"}
        _write(receipt, saved)
    return {**saved, "receipt": str(receipt), "receipt_sha256": _sha256(receipt)}


def verify_row(args, family, row, output, identities):
    case = row["scenario_id"]
    case_dir = output / "verification" / family / case
    receipt = case_dir / "verification.json"
    result = {"family": family, "case": case, "seed": SEED}
    campaign = getattr(args, "campaign_spec", None)
    if campaign and case == campaign.get("long_test_exclusion"):
        result.update(status="NOT_RUN_OPTIONAL", exception="declared-optional",
                      omissions=[campaign["long_test_reason"]])
        _write(case_dir / "exception.json", result)
        return result
    if campaign and not row.get("verification_supported", True):
        raise ValueError("unsupported verifier lacks explicit campaign exclusion")
    if not campaign and (case.endswith("sustained-600s-compact-v2-proof") or row.get("kind") == "sustained-600s" or not row.get("verification_supported", True)):
        result.update(
            status="INCOMPLETE",
            exception="duration-incompatible",
            omissions=["sustained-600s cannot fit the 60-second verification ceiling or 300-second performance allowance"],
            follow_up="preserve original 600-second definition; do not shorten; track under a dedicated proof issue",
        )
        _write(case_dir / "exception.json", result)
        return result
    if receipt.is_file():
        retain_lock_refusal(case_dir)
    if receipt.is_file():
        saved = json.loads(receipt.read_text())
        if not identities or any(saved.get(key) != identities.get(key) for key in
                                 ("source_identity", "product_identity", "input_identity")):
            raise ValueError(f"incompatible verification receipt: {receipt}")
        if saved.get("harness_identity") != runner.harness_identity() or saved.get("image_identity") != identities.get("image"):
            raise ValueError(f"incompatible verification harness/image: {receipt}")
        if case in getattr(args, "proof_preparation_spec", {}).get("cases", []):
            if not (output / "preparation" / family / case / "preparation.json").is_file():
                raise ValueError("existing proof lacks declared preparation receipt")
            result["independent_preparation"] = prepare_proof(args, family, row, output, identities)
            if result["independent_preparation"]["status"] != "PASS":
                raise ValueError("existing proof has unsuccessful declared preparation")
            result["preparation_plus_verification_wall_seconds"] = result["independent_preparation"]["wall_ns"] / 1e9 + saved["wall_seconds"]
        result.update(status=saved.get("status"), reused=True, receipt=str(receipt),
                      receipt_sha256=_sha256(receipt), wall_seconds=saved.get("wall_seconds"))
        return result
    if not identities or not identities.get("source_identity") or not identities.get("input_identity"):
        result.update(status="INCOMPLETE", error="missing identity-matched performance/source identities")
        return result
    if case in getattr(args, "proof_preparation_spec", {}).get("cases", []):
        preparation = prepare_proof(args, family, row, output, identities)
        result["independent_preparation"] = preparation
        if preparation["status"] != "PASS":
            result.update(status="INCOMPLETE", error=preparation["error"])
            return result
    case_dir.parent.mkdir(parents=True, exist_ok=True)
    command = [
        sys.executable, str(HERE / "verify-selected.py"),
        "--family", family, "--case", case,
        "--repetition" if row.get("route") == "sdk" else "--seed", str(SEED),
        "--setup", "fresh" if identities.get("setup_identity") == "fresh-output" else identities.get("setup_identity") or "clone",
        "--performance-rows", identities.get("performance_rows", "-"),
        "--source", identities["source_identity"],
        "--input", identities["input_identity"],
        "--image", args.image, "--host-binary", args.host_binary,
        "--source-arm", getattr(args, "source_arm", "candidate"),
        "--output", str(case_dir),
        "--setup-timeout", str(SETUP_TIMEOUT),
    ]
    started = time.monotonic()
    proc = _run(command)
    result.update(returncode=proc.returncode, wall_seconds=time.monotonic() - started,
                  stdout_tail=proc.stdout[-4000:], stderr_tail=proc.stderr[-4000:])
    if receipt.is_file():
        saved = json.loads(receipt.read_text())
        result.update(status=saved.get("status"), receipt=str(receipt), receipt_sha256=_sha256(receipt),
                      error=saved.get("error"), cleanup=saved.get("cleanup"),
                      omissions=saved.get("omissions"), wall_seconds=saved.get("wall_seconds"))
    else:
        result["status"] = "TIMEOUT" if "timeout" in (proc.stderr + proc.stdout).lower() else "INCOMPLETE"
        result["error"] = (proc.stderr or proc.stdout)[-2000:]
    if "independent_preparation" in result:
        result["preparation_plus_verification_wall_seconds"] = result["independent_preparation"]["wall_ns"] / 1e9 + result["wall_seconds"]
    return result


def parse_args():
    parser = argparse.ArgumentParser(description="Collect issue #54 remaining-family statistics")
    parser.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"), required=not os.environ.get("LAYERFS_BENCH_IMAGE"))
    parser.add_argument("--host-binary", default=str(REPO / "target/release/fs-benchmark-pro"))
    parser.add_argument("--output", default=str(REPO / "benchmark-results/host-store/campaigns/issue54"))
    parser.add_argument("--phase", choices=("inventory", "performance", "verification", "all"), default="all")
    parser.add_argument("--checkpoint", action="store_true",
                        help="All host-admitted families and routine proofs for #74/#75; one shared campaign")
    parser.add_argument("--campaign", type=Path, help="Frozen prospective campaign declaration")
    parser.add_argument("--proof-preparation-declaration", type=Path)
    parser.add_argument("--family")
    parser.add_argument("--case")
    parser.add_argument("--proofs", choices=("selected", "all"), default="selected",
                       help="selected (default): compact/tier-1/10 plus reliability short proofs; all: every completed case")
    return parser.parse_args()


def main():
    args = parse_args()
    args.campaign_spec = json.loads(args.campaign.read_text()) if args.campaign else None
    if args.campaign and (not args.checkpoint or args.family or args.case):
        raise ValueError("--campaign requires complete --checkpoint selection")
    args.proof_preparation_spec = json.loads(args.proof_preparation_declaration.read_text()) if args.proof_preparation_declaration else {}
    if args.proof_preparation_spec and not args.campaign_spec:
        raise ValueError("proof preparation requires a frozen --campaign")
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    inventory = {"families": {}, "performance_cases": [], "proof_cases": [], "mismatches": []}
    identities_by_case = {}
    selected_families = {}
    args.listed_families = {}
    families = PERF_FAMILIES + ((PROOF_FAMILY,) if args.family in (None, PROOF_FAMILY) else ())
    if args.checkpoint:
        families = runner.HOST_FAMILIES
        args.proofs = "all"
    if args.family:
        families = (args.family,)
    for family in families:
        rows, listed = list_family(args, family)
        if args.case:
            rows = [row for row in rows if row["scenario_id"] == args.case]
        selected_families[family] = (rows, listed)
        args.listed_families[family] = listed
        inventory["families"][family] = {
            "count": len(rows),
            "source_identity": listed.get("source_identity"),
            "image": listed.get("image"),
            "cases": [row["scenario_id"] for row in rows],
            "proof_only": [row["scenario_id"] for row in rows if row.get("proof_only")],
        }
        expected = None if args.campaign_spec else EXPECTED_PROOFS if family == PROOF_FAMILY else EXPECTED_PERF.get(family)
        if not args.case and expected is not None and len(rows) != expected:
            inventory["mismatches"].append({"family": family, "expected": expected, "actual": len(rows)})
        for row in rows:
            if row.get("proof_only"):
                inventory["proof_cases"].append(row["scenario_id"])
            else:
                inventory["performance_cases"].append(row["scenario_id"])
    if args.campaign_spec:
        validate_campaign(args.campaign_spec, selected_families)
        if args.proof_preparation_spec:
            validate_proof_preparation(args.proof_preparation_spec, selected_families)
            inventory["proof_preparation_declaration_sha256"] = _sha256(args.proof_preparation_declaration)
        inventory["campaign"] = {"issue": args.campaign_spec["issue"],
                                 "release": args.campaign_spec["release"],
                                 "declaration_sha256": _sha256(args.campaign)}
    if args.checkpoint and not args.campaign_spec and not args.family and not args.case:
        if len(inventory["performance_cases"]) != 198 or len(inventory["proof_cases"]) != 29:
            raise ValueError("checkpoint registry differs from the frozen 198 performance / 29 proof contract")
    selection_path = output / "selections.json"
    if selection_path.exists() and json.loads(selection_path.read_text()) != inventory:
        raise ValueError("campaign inventory/source changed; preserve this campaign and select a new output")
    _write(selection_path, inventory)
    registry_path = output / "registry.jsonl"
    registry_text = "".join(json.dumps(row, sort_keys=True) + "\n" for rows, _ in selected_families.values() for row in rows)
    if registry_path.exists() and registry_path.read_text() != registry_text:
        raise ValueError("frozen registry changed")
    registry_path.write_text(registry_text)
    print(json.dumps({
        "phase": "inventory",
        "performance_cases": len(inventory["performance_cases"]),
        "proof_cases": len(inventory["proof_cases"]),
        "mismatches": inventory["mismatches"],
    }), flush=True)
    if args.phase == "inventory":
        return 0 if not inventory["mismatches"] else 1

    ledger = []
    if args.phase in ("performance", "all"):
        for family in families:
            if family == PROOF_FAMILY:
                continue
            rows, _ = selected_families[family]
            if args.case:
                rows = [row for row in rows if row["scenario_id"] == args.case]
            for row in rows:
                if row.get("proof_only"):
                    continue
                result, how = collect_row(args, family, row, output)
                ledger.append(result)
                _write(output / "performance-ledger.json", ledger)
                print(f"PERF {family} {row['scenario_id']} {result.get('status')} {how} timer={result.get('timer')} elapsed_ns={result.get('elapsed_ns')}", flush=True)
                if result.get("status") == "PASS" and result.get("identities"):
                    identities_by_case[row["scenario_id"]] = result["identities"]
        _write(output / "performance-ledger.json", ledger)

    proofs = []
    if args.phase in ("verification", "all"):
        identity_cache = identities_by_case
        if (output / "performance-ledger.json").is_file():
            for item in json.loads((output / "performance-ledger.json").read_text()):
                if item.get("status") == "PASS" and item.get("identities"):
                    identity_cache[item["case"]] = item["identities"]
        for family in families:
            rows, listed = selected_families[family]
            if args.case:
                rows = [row for row in rows if row["scenario_id"] == args.case]
            for row in rows:
                if args.campaign_spec and row["scenario_id"] == args.campaign_spec.get("long_test_exclusion"):
                    proofs.append(verify_row(args, family, row, output, None))
                    _write(output / "verification-ledger.json", proofs)
                    continue
                identities = identity_cache.get(row["scenario_id"])
                if row.get("proof_only"):
                    identities = identities or {
                        "source_identity": listed.get("source_identity"),
                        "setup_identity": "clone",
                    }
                    listed_row = row
                    # Resolve input identity through the runner without executing.
                    resolved = _run([
                        sys.executable, str(HERE / "shared/runner.py"),
                        "--family", family, "--case", row["scenario_id"], "--seed", str(SEED),
                        "--list", "--image", args.image, "--host-binary", args.host_binary,
                    ])
                    if resolved.returncode == 0:
                        payload = json.loads(resolved.stdout)
                        # --list with a case still lists; resolve_selection needs execute path.
                    resolve = argparse.Namespace(
                        family=family, case=row["scenario_id"], seed=SEED, image=args.image,
                        host_binary=args.host_binary, topology="host-store",
                        setup="fresh" if row.get("setup_policy") == "fresh-output" else "clone",
                        source=None, input=None, cpus=2, memory_mib=2048,
                        timeout=COMMAND_TIMEOUT, product_timeout=PRODUCT_TIMEOUT,
                        setup_timeout=SETUP_TIMEOUT, source_arm="candidate",
                        performance_rows="-", repetition=None, smoke=False, list=False,
                        verification=True, prepare_only=False,
                    )
                    try:
                        selection = runner.resolve_selection(resolve, time.monotonic() + 30)
                        identities = {
                            "source_identity": selection["source_identity"],
                            "input_identity": selection["input_identity"],
                            "setup_identity": selection["setup_identity"],
                            "product_identity": selection.get("product_identity"),
                            "image": selection.get("image"),
                        }
                    except Exception as error:
                        proofs.append({"family": family, "case": row["scenario_id"],
                                       "status": "INCOMPLETE", "error": str(error)})
                        print(f"PROOF {family} {row['scenario_id']} INCOMPLETE resolve {error}", flush=True)
                        continue
                elif not identities:
                    proofs.append({"family": family, "case": row["scenario_id"],
                                   "status": "INCOMPLETE", "error": "no matching performance identities"})
                    print(f"PROOF {family} {row['scenario_id']} INCOMPLETE missing identities", flush=True)
                    continue
                if getattr(args, "proofs", "selected") == "selected" and family != PROOF_FAMILY and (row.get("tier") or 1) > 10:
                    proofs.append({"family": family, "case": row["scenario_id"], "status": "SKIPPED",
                                   "reason": "selected-shape coverage uses compact/tier-1/10 proofs; large-tier proofs are not replayed under the 45s/59s ceiling"})
                    print(f"PROOF {family} {row['scenario_id']} SKIPPED selected-shape", flush=True)
                    continue
                result = verify_row(args, family, row, output, identities)
                proofs.append(result)
                _write(output / "verification-ledger.json", proofs)
                print(f"PROOF {family} {row['scenario_id']} {result.get('status')} wall={result.get('wall_seconds')}", flush=True)
        _write(output / "verification-ledger.json", proofs)

    terminal = {
        "issue": args.campaign_spec["issue"] if args.campaign_spec else [74, 75] if args.checkpoint else 54,
        "campaign": inventory.get("campaign"),
        "performance_cases": len(inventory["performance_cases"]),
        "proof_definitions": len(inventory["proof_cases"]),
        "mismatches": inventory["mismatches"],
        "performance_outcomes": {},
        "verification_outcomes": {},
    }
    for item in ledger:
        terminal["performance_outcomes"][item.get("status") or "unknown"] = (
            terminal["performance_outcomes"].get(item.get("status") or "unknown", 0) + 1
        )
    for item in proofs:
        terminal["verification_outcomes"][item.get("status") or "unknown"] = (
            terminal["verification_outcomes"].get(item.get("status") or "unknown", 0) + 1
        )
    _write(output / "terminal-assessment.json", terminal)
    print(json.dumps(terminal, sort_keys=True), flush=True)
    if args.checkpoint:
        failed_perf = any(item.get("status") != "PASS" for item in ledger)
        allowed_exception = "declared-optional" if args.campaign_spec else "duration-incompatible"
        failed_proofs = any(item.get("status") != "PASS" and item.get("exception") != allowed_exception for item in proofs)
        return int(bool(inventory["mismatches"]) or failed_perf or failed_proofs)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, TimeoutError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)

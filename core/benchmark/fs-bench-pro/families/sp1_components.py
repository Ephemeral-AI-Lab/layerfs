"""Nine one-observation SP1 component screens; existing runner owns selection."""
from dataclasses import dataclass
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import time
import uuid

from families import workspace_write as shared

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[2]
ADAPTER = ROOT / "core/benchmark/phase6-live"
SPEC = ROOT / "core/docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/speed/SPEC-v1.md"
SCHEMA = "core-sp1-components-run-v1"
PROOF_SCHEMA = "core-sp1-components-proof-v1"
PROFILE = "sp1-component-speed-v1"
PREFIX = "sp1-speed-v1-"
COMMAND_BUDGET_NS = 15_000_000_000
PROOF_BUDGET_NS = 9_999_999_999
BUILD_BUDGET_NS = 30_000_000_000
EXAMPLE = "sp1_speed"


@dataclass(frozen=True)
class Case:
    id: str
    master: str
    alert_ns: int


CASES = {case.id: case for case in (
    Case(PREFIX + "metadata-local", "metadata-local", 20_000_000),
    Case(PREFIX + "metadata-native", "metadata-local", 20_000_000),
    Case(PREFIX + "small-new-full", "small-new-full", 250_000_000),
    Case(PREFIX + "small-exact-reuse", "small-exact-reuse", 250_000_000),
    Case(PREFIX + "payload-prefix-write", "small-exact-reuse", 250_000_000),
    Case(PREFIX + "payload-full-read", "small-exact-reuse", 100_000_000),
    Case(PREFIX + "payload-prefix-read", "payload-prefix-read", 100_000_000),
    Case(PREFIX + "candidate-hydration", "candidate-hydration", 100_000_000),
    Case(PREFIX + "metadata-transfer", "small-new-full", 250_000_000),
)}
GROUPS = {
    "sp1-metadata": tuple(CASES)[:2],
    "sp1-small-save": tuple(CASES)[2:4],
    "sp1-payload": (tuple(CASES)[2], *tuple(CASES)[4:7]),
    "sp1-sql": tuple(CASES)[7:],
    "sp1-components": tuple(CASES),
}


def select(selection):
    if selection in CASES:
        return (selection,)
    if selection in GROUPS:
        return GROUPS[selection]
    raise ValueError("unknown SP1 component selection")


def files(root, *, exclude=()):
    result = {}
    for path in sorted(Path(root).rglob("*")):
        if path.is_symlink():
            raise ValueError("symlink in SP1 custody")
        if path.is_file() and path.name not in exclude:
            result[str(path.relative_to(root))] = {"sha256": shared.sha256(path), "bytes": path.stat().st_size}
    return result


def identity(common):
    value = common.identities()
    inputs = [*sorted((ROOT / "core/crates").glob("**/src/**/*.rs")),
              *sorted((ROOT / "core/crates").glob("**/sql/**/*.sql")),
              *sorted((ROOT / "core/crates").glob("**/src/**/*.sql")),
              *sorted((ROOT / "core/crates").glob("**/Cargo.toml")),
              *sorted(ADAPTER.glob("src/**/*.rs")), *sorted(ADAPTER.glob("src/**/*.sql")),
              *sorted(ADAPTER.glob("examples/**/*.rs")),
              ROOT / "core/Cargo.toml", ROOT / "core/Cargo.lock",
              ADAPTER / "Cargo.toml", ADAPTER / "Cargo.lock", ROOT / ".cargo/config.toml"]
    value.update(compilation_seal=common.seal(inputs),
                 dependency_seal=common.seal([ROOT / "core/Cargo.lock", ADAPTER / "Cargo.lock"]),
                 harness_seal=common.seal([*sorted(HERE.glob("**/*.py")), SPEC,
                     ROOT / "core/benchmark/fs-bench-pro-storage-content/shared/residency.py"]),
                 specification_sha256=shared.sha256(SPEC), build_profile="release")
    return value


def invoke(command, destination, timeout_ns, *, env):
    record, _, _ = shared.execute(command, destination, timeout=max(0, timeout_ns) / 1e9, env=env)
    shared.save(destination.with_suffix(".command.json"), record)
    return record


def successful(record):
    return record["exit_code"] == 0 and not record["timeout"]


def provider_environment(bucket):
    result = {**os.environ, "LAYERFS_CONSTRUCTION_WORKERS": "1", "SP1_MINIO_BUCKET": bucket}
    for key in ("SP1_MINIO_AUTHORITY", "SP1_MINIO_ACCESS", "SP1_MINIO_SECRET"):
        if not result.get(key):
            raise ValueError(f"missing private provider configuration: {key}")
    return result


def provider_identity():
    image = os.environ.get("SP1_MINIO_IMAGE_DIGEST")
    path = os.environ.get("SP1_MINIO_IDENTITY_JSON")
    if not image or not path:
        raise ValueError("sealed provider image/identity required")
    record = json.loads(Path(path).read_text())
    if not isinstance(record, dict):
        raise ValueError("provider identity must be a sanitized object")
    forbidden = ("secret", "password", "access_key", "authorization", "credential")
    if any(word in json.dumps(record).lower() for word in forbidden):
        raise ValueError("provider identity contains private configuration")
    return {"image_digest": image, "record": record, "record_sha256": shared.sha256(path)}


def driver_command(binary, phase, case, root, evidence, store, output, *, perf=None):
    command = [binary["path"], "--phase", phase, "--case", case,
               "--root", str(root), "--evidence", str(evidence), "--store", str(store), "--out", str(output)]
    if perf is not None:
        command += ["--perf", str(perf)]
    return command


def build(out, common, expected):
    cache = common.RESULTS / "sp1-build-release-v1.json"
    prior = json.loads(cache.read_text()) if cache.exists() else {}
    binary = prior.get("binary", {})
    if (prior.get("compilation_seal") == expected["compilation_seal"]
            and prior.get("build_profile") == "release" and Path(binary.get("path", "/absent")).is_file()
            and shared.sha256(binary["path"]) == binary.get("sha256")):
        return {**prior, "mode": "exact-binary-reuse", "wall_ns": 0, "command": None}
    target = common.target_path()
    command = ["cargo", "+1.85.1", "build", "--release", "--locked", "--manifest-path",
               str(ADAPTER / "Cargo.toml"), "--example", EXAMPLE]
    record = invoke(command, out / "build", BUILD_BUDGET_NS,
                    env={**os.environ, "CARGO_TARGET_DIR": str(target)})
    after = identity(common)
    if after != expected:
        raise ValueError("source changed during SP1 release build")
    result = {**record, "compilation_seal": expected["compilation_seal"], "build_profile": "release",
              "mode": "worktree-local-locked-release", "dependency_reuse": "owned worktree target",
              "status": "PASS" if successful(record) else "FAIL", "budget_ns": BUILD_BUDGET_NS}
    if successful(record):
        source = target / "release/examples" / EXAMPLE
        digest = shared.sha256(source)
        archive = common.RESULTS / "binary-archive" / digest / EXAMPLE
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            shutil.copy2(source, archive)
            archive.chmod(0o555)
        if shared.sha256(archive) != digest:
            raise ValueError("archived SP1 binary changed")
        result["binary"] = {"path": str(archive), "sha256": digest}
        shared.save(cache, result)
    return result


def validate_master(path, manifest):
    if files(path, exclude=("master.json",)) != manifest["files"]:
        raise ValueError("prepared SP1 master changed")
    store = Path(path) / "catalog.sqlite"
    if not store.is_file() or store.is_symlink() or store.stat().st_nlink != 1:
        raise ValueError("prepared catalog is not exclusive")
    for suffix in ("-wal", "-shm", "-journal"):
        if Path(str(store) + suffix).exists():
            raise ValueError("prepared catalog has unresolved sidecar")


def prepare_master(name, common, identities, binary, evidence, evidence_identity, provider):
    compatibility = {"compilation_seal": identities["compilation_seal"],
                     "specification_sha256": identities["specification_sha256"],
                     "evidence": evidence_identity, "provider_image": provider["image_digest"], "provider_instance_sha256": provider["record_sha256"], "master": name}
    key = hashlib.sha256(json.dumps(compatibility, sort_keys=True).encode()).hexdigest()
    path = common.RESULTS / "sp1-prepared-v1" / key
    manifest_path = path / "master.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
        if manifest["compatibility"] != compatibility:
            raise ValueError("prepared SP1 compatibility mismatch")
        validate_master(path, manifest)
        return {**manifest, "reuse": True}
    path.mkdir(parents=True, exist_ok=False)
    bucket = "sp1-master-" + uuid.uuid4().hex
    output = path / "preparation.json"
    command = driver_command(binary, "prepare", PREFIX + name, path, evidence, path / "catalog.sqlite", output)
    record = invoke(command, path / "prepare", COMMAND_BUDGET_NS, env=provider_environment(bucket))
    if not successful(record) or not output.is_file():
        raise RuntimeError(f"master preparation failed; retained {path}")
    receipt = json.loads(output.read_text())
    if receipt.get("status") != "PASS" or receipt.get("phase") != "prepare":
        raise ValueError("master readiness not acknowledged")
    manifest = {"path": str(path), "bucket": bucket, "compatibility": compatibility,
                "producer_identity": identities, "binary": binary, "preparation": record,
                "files": files(path, exclude=("master.json",)), "reuse": False}
    validate_master(path, manifest)
    for member in manifest["files"]:
        (path / member).chmod(0o444)
    shared.save(manifest_path, manifest)
    return manifest


def clone_master(master, destination):
    source = Path(master["path"])
    validate_master(source, master)
    destination.mkdir(exist_ok=False)
    copied = {}
    for relative, expected in master["files"].items():
        if relative.endswith((".stdout", ".stderr", ".command.json")):
            continue
        origin, target = source / relative, destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(origin, target)
        target.chmod(0o600)
        if shared.sha256(target) != expected["sha256"]:
            raise ValueError("SP1 clone byte identity mismatch")
        if (origin.stat().st_dev, origin.stat().st_ino) == (target.stat().st_dev, target.stat().st_ino):
            raise ValueError("SP1 clone aliases master")
        copied[relative] = expected
    validate_master(source, master)
    return {"setup": "clone", "clone_method": "closed-independent-byte-copy",
            "master_unchanged": True, "master_path": str(source), "files": copied}


def precondition(store):
    helper = ROOT / "core/benchmark/fs-bench-pro-storage-content/shared/residency.py"
    record = {"method": "existing-host-whole-file-residency-v1", "status": "UNAVAILABLE",
              "sqlite_pager": "UNVERIFIED", "minio_process_vm_volume_device": "UNVERIFIED"}
    try:
        spec = importlib.util.spec_from_file_location("sp1_host_residency", helper)
        module = importlib.util.module_from_spec(spec)
        # dataclasses resolves its module while loading the existing instrument.
        import sys
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        failures = module.self_check()
        if failures:
            raise ValueError("; ".join(failures))
        record.update(self_check="PASS", before=module.residency(store).as_fields(),
                      invalidation=module.de_warm(store).as_fields(), after=module.residency(store).as_fields())
        record["status"] = "ZERO_HOST_RESIDENCY" if record["after"]["resident_pages"] == 0 else "RESIDENT"
    except (OSError, ValueError) as error:
        record["reason"] = str(error)
    record["finished_ns"] = time.monotonic_ns()
    return record


def base_row(case, status="NOT_RUN", reason="unselected registered row"):
    return {"case": case.id, "status": status, "reason": reason, "sample_count": 0,
            "operation_ns": None, "operation_ms": None, "cpu_user_ns": None, "cpu_system_ns": None,
            "command_ns": None, "provider_setup_ns": None, "cleanup_ns": None,
            "alert_ns": case.alert_ns, "alert_status": "UNAVAILABLE", "verification_status": "NOT_RUN", "admission_reason": "exploratory component screen; SQLite pager and MinIO cache domains unverified",
            "cache_status": "INELIGIBLE", "admission_eligible": False, "numeric_speed_status": "INELIGIBLE",
            "cleanup_status": "NOT_RUN", "sample_local_cleanup_status": "NOT_RUN", "driver": None,
            "metrics": {name: None for name in ("writer", "delta", "pool", "provider", "native", "candidate", "read")},
            "missing_metrics": {"operation_ns": "not collected", "cpu_user_ns": "not collected", "cpu_system_ns": "not collected",
                                "physical_rss": "not instrumented", "cgroup_phase_peak": "not instrumented",
                                "device_reads": "not instrumented", "total_provider_allocation": "not instrumented"}}


def attempt(out, case, master, binary, evidence, identities, provider):
    folder = out / case.id
    folder.mkdir()
    row = base_row(case, "FAIL", "selected operation not acknowledged")
    started = time.monotonic_ns()
    bucket = "sp1-sample-" + uuid.uuid4().hex
    assets = folder / "assets"
    row.update(identity=identities, binary=binary, provider_identity=provider,
               sample_bucket=bucket, master=master, operation_contract_id=PROFILE)
    deadline = started + COMMAND_BUDGET_NS
    def remaining():
        return max(0, deadline - time.monotonic_ns())
    try:
        row["clone"] = clone_master(master, assets)
        store = assets / "catalog.sqlite"
        setup_output = folder / "provider-setup.json"
        setup = invoke(driver_command(binary, "setup-provider", case.id, assets, evidence, store, setup_output),
                       folder / "provider-setup", remaining(), env=provider_environment(bucket))
        row["provider_setup_ns"] = setup["wall_ns"]
        row["provider_setup"] = setup
        if not successful(setup):
            raise RuntimeError("sample provider byte-copy setup failed")
        if not setup_output.is_file():
            raise ValueError("provider setup acknowledgement missing")
        row["provider_copy"] = json.loads(setup_output.read_text())
        if (row["provider_copy"].get("status") != "PASS" or row["provider_copy"].get("case") != case.id
                or row["provider_copy"].get("phase") != "setup-provider"):
            raise ValueError("provider setup identity/acknowledgement mismatch")
        row["cache"] = precondition(store)
        perf = folder / "performance.json"
        row["sample_count"] = 1
        command = invoke(driver_command(binary, "perf", case.id, assets, evidence, store, perf),
                         folder / "performance", remaining(), env=provider_environment(bucket))
        row["performance"] = command
        row["cache"]["launch_gap_ns"] = command["started_ns"] - row["cache"]["finished_ns"]
        if not perf.is_file():
            raise RuntimeError("component operation lacked receipt")
        driver = json.loads(perf.read_text())
        if (driver.get("phase") != "perf" or driver.get("case") != case.id
                or driver.get("operation_contract_id") != PROFILE):
            raise ValueError("component receipt identity mismatch")
        row["driver"] = driver
        row["missing_metrics"].update(driver.get("missing_metrics", {}))
        if not isinstance(driver.get("metrics"), dict):
            raise ValueError("component metrics object missing")
        operation = driver.get("operation_ns")
        if type(operation) is not int or operation < 0:
            raise ValueError("component operation clock missing")
        row.update(operation_ns=operation, operation_ms=operation / 1_000_000,
                   cpu_user_ns=driver.get("cpu_user_ns"), cpu_system_ns=driver.get("cpu_system_ns"),
                   metrics={key: driver["metrics"].get(key) for key in row["metrics"]})
        for key in ("operation_ns", "cpu_user_ns", "cpu_system_ns"):
            if driver.get(key) is not None:
                row["missing_metrics"].pop(key, None)
        if not successful(command) or driver.get("status") != "PASS":
            raise RuntimeError("component operation failed; raw clocks/counters retained")
        validate_master(master["path"], master)
        row.update(status="OBSERVED", reason=None, driver=driver, operation_ns=operation,
                   operation_ms=operation / 1_000_000, cpu_user_ns=driver.get("cpu_user_ns"),
                   cpu_system_ns=driver.get("cpu_system_ns"), metrics={key: driver.get("metrics", {}).get(key) for key in row["metrics"]},
                   alert_status="UNDER_ALERT" if operation <= case.alert_ns else "OVER_ALERT",
                   cleanup_status="OWNED_RETAINED_FOR_SEPARATE_PROOF", sample_local_cleanup_status="PASS: driver process exited and closed owned threads/connections", performance_sha256=shared.sha256(perf),
                   input_identity=master["compatibility"], clone_input_files=row["clone"]["files"])
    except Exception as error:
        row["reason"] = repr(error)
        row["cleanup_status"] = "RETAINED_FAILURE_CUSTODY"
    row["command_ns"] = time.monotonic_ns() - started
    row["command_budget_ns"] = COMMAND_BUDGET_NS
    row["command_budget_status"] = "PASS" if row["command_ns"] <= COMMAND_BUDGET_NS else "FAIL"
    if row["command_budget_status"] != "PASS":
        row["status"] = "FAIL"
        row["reason"] = "complete sample command exceeded frozen 15s bound"
    shared.save(folder / "receipt.json", row)
    return row


def run(selection, output, common):
    selected = select(selection)
    out = common.owned(output)
    identities = identity(common)
    if identities["source_dirty"]:
        raise ValueError("commit SP1 source/harness/spec before collection")
    out.mkdir(parents=True)
    rows = {name: base_row(case) for name, case in CASES.items()}
    summary = {"schema": SCHEMA, "profile": PROFILE, "identity": identities,
               "selection": list(selected), "rows": [], "admission_eligible": False}
    with (common.RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            evidence = Path(os.environ["SP1_EVIDENCE_DIR"]).resolve(strict=True)
            evidence_identity = files(evidence)
            provider = provider_identity()
            built = build(out, common, identities)
            summary.update(build=built, provider_identity=provider, evidence=str(evidence))
            shared.save(out / "build-seal.json", built)
            if built["status"] != "PASS":
                raise RuntimeError("SP1 release build unavailable")
            masters = {}
            for name in selected:
                representative = CASES[name].master
                if representative not in masters:
                    masters[representative] = prepare_master(representative, common, identities, built["binary"], evidence, evidence_identity, provider)
            shared.save(out / "prepared.json", masters)
            for name in selected:
                rows[name] = attempt(out, CASES[name], masters[CASES[name].master], built["binary"], evidence, identities, provider)
        except Exception as error:
            summary["error"] = repr(error)
            for name in selected:
                if rows[name]["status"] == "NOT_RUN":
                    rows[name]["reason"] = "prerequisite unavailable: " + repr(error)
        summary["rows"] = list(rows.values())
        summary["status"] = "OBSERVED" if all(rows[name]["status"] == "OBSERVED" for name in selected) else "FAIL"
        for name, row in rows.items():
            folder = out / name
            folder.mkdir(exist_ok=True)
            shared.save(folder / "receipt.json", row)
        shared.save(out / "run.json", summary)
        (out / "report.txt").write_text(report(out))
        common.manifest_run(out)
    return out


def prove(performance_out, output, common):
    performance_out = common.owned(performance_out, existing=True)
    common.verify_run_manifest(performance_out)
    run = json.loads((performance_out / "run.json").read_text())
    if run["schema"] != SCHEMA or identity(common) != run["identity"]:
        raise ValueError("SP1 proof source identity mismatch")
    out = common.owned(output)
    out.mkdir(parents=True)
    results = []
    with (common.RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for row in run["rows"]:
            result = base_row(CASES[row["case"]], "NOT_RUN", "no successful component observation")
            result["verification_invocation_count"] = 0
            if row["status"] == "OBSERVED":
                folder = out / row["case"]
                folder.mkdir()
                start = time.monotonic_ns()
                try:
                    if shared.sha256(row["binary"]["path"]) != row["binary"]["sha256"]:
                        raise ValueError("proof executable identity mismatch")
                    perf = performance_out / row["case"] / "performance.json"
                    if shared.sha256(perf) != row["performance_sha256"]:
                        raise ValueError("proof performance clock/counter identity mismatch")
                    validate_master(row["master"]["path"], row["master"])
                    if files(run["evidence"]) != row["input_identity"]["evidence"]:
                        raise ValueError("proof independent input identity mismatch")
                    if provider_identity() != row["provider_identity"]:
                        raise ValueError("proof provider identity mismatch")
                    assets = performance_out / row["case"] / "assets"
                    command = driver_command(row["binary"], "verify", row["case"], assets, run["evidence"], assets / "catalog.sqlite", folder / "proof.json", perf=perf)
                    result["verification_invocation_count"] = 1
                    record = invoke(command, folder / "proof", max(0, PROOF_BUDGET_NS - (time.monotonic_ns() - start)), env=provider_environment(row["sample_bucket"]))
                    proof = json.loads((folder / "proof.json").read_text()) if (folder / "proof.json").exists() else {}
                    if not successful(record) or proof.get("status") != "PASS" or proof.get("case") != row["case"] or proof.get("phase") != "verify":
                        raise ValueError("independent persisted-output proof failed")
                    result.update(status="PASS", reason=None, command=record, driver=proof,
                                  performance_sha256=row["performance_sha256"], input_identity=row["input_identity"],
                                  binary=row["binary"], source_identity=run["identity"],
                                  cleanup_status="OWNED_RETAINED_FOR_CAMPAIGN_TEARDOWN", admission_eligible=False, numeric_speed_status="INELIGIBLE")
                    validate_master(row["master"]["path"], row["master"])
                except Exception as error:
                    result.update(status="FAIL", reason=repr(error), cleanup_status="RETAINED_FAILURE_CUSTODY")
                result["wall_ns"] = time.monotonic_ns() - start
                result["budget_ns"] = PROOF_BUDGET_NS
                if result["wall_ns"] > PROOF_BUDGET_NS:
                    result.update(status="FAIL", reason="proof exceeded strict <10s bound")
            result["verification_status"] = result["status"]
            results.append(result)
        summary = {"schema": PROOF_SCHEMA, "performance_run": str(performance_out),
                   "performance_manifest_sha256": shared.sha256(performance_out / "manifest.json"),
                   "identity": run["identity"], "rows": results,
                   "status": "PASS" if all(r["status"] == "PASS" for r in results if r["case"] in run["selection"]) else "FAIL",
                   "admission_eligible": False}
        shared.save(out / "run.json", summary)
        common.manifest_run(out)
    return out


def verify(run, common):
    common.verify_run_manifest(run)
    value = json.loads((Path(run) / "run.json").read_text())
    if value.get("schema") not in (SCHEMA, PROOF_SCHEMA):
        raise ValueError("SP1 evidence schema mismatch")
    if [r["case"] for r in value["rows"]] != list(CASES) or any(r.get("admission_eligible", False) for r in value["rows"]):
        raise ValueError("SP1 retained row cardinality/eligibility mismatch")
    return "PASS: retained SP1 evidence custody only; numeric speed INELIGIBLE"


def report(run):
    value = json.loads((Path(run) / "run.json").read_text())
    lines = ["case\tstatus\toperation_ns\toperation_ms\tcommand_ns\talert\tnumeric_speed\tproof"]
    for row in value["rows"]:
        lines.append("\t".join(str(row.get(key)) for key in ("case", "status", "operation_ns", "operation_ms", "command_ns", "alert_status", "numeric_speed_status", "verification_status")))
    lines.append("investigation\tregistered_rows (shared FULL row collected once)")
    for group, names in GROUPS.items():
        lines.append(group + "\t" + ",".join(names))
    return "\n".join(lines) + "\n"

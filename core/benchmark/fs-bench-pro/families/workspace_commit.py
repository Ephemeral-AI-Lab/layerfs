"""Family 4: same-Workspace retained Commit controls; separate fast-lane proof."""
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil

from families import workspace_write as write

PROFILE = "sdk-live-retained-commit-fast-v3"
PRELUDE = "/fixtures/bin/write-separated dispersed data.bin 4097"
PRIOR_IMAGE = write.ROOT / "benchmark-results/fs-bench-pro/issue286-family3-cold-r044/prepared.json"


@dataclass(frozen=True)
class Case:
    id: str
    command: str
    final_writes: int
    clean: bool
    command_budget_ns: int = 15_000_000_000
    verifier_budget_ns: int = 9_000_000_000
    pin_read_bytes: int = 0
    role: str = "performance-diagnostic"


CASES = {case.id: case for case in (
    Case("workspace-commit-clean-retained-writes-4097-v2", "true", 0, True, pin_read_bytes=16_384),
    Case("workspace-commit-one-edit-retained-writes-4097-v2",
         "/fixtures/bin/write-separated data.bin 1", 1, False, pin_read_bytes=16_384),
    Case("workspace-commit-clean-retained-writes-4097-v3", "true", 0, True),
    Case("workspace-commit-one-edit-retained-writes-4097-v3",
         "/fixtures/bin/write-separated data.bin 1", 1, False),
    Case("workspace-commit-full-pin-retained-writes-4097-read-31kib-v1",
         "/fixtures/bin/write-separated data.bin 1", 1, False,
         command_budget_ns=60_000_000_000, pin_read_bytes=31_744, role="functional-oracle"),
)}
SELECTED = tuple(name for name in CASES if name.endswith("-v3"))
REMAINING = {
    "issue273-clean-commit-v1": "historical committed/reattached control cannot hold its live private journal; use distinct v2 scenario",
    "issue273-one-edit-commit-v1": "historical committed/reattached control cannot hold its live private journal; use distinct v2 scenario",
    "workspace-commit-full-lowering-size-64mib-v1": "native 64 MiB/eight-literal-byte lowering proof pending",
    "workspace-commit-stage-headroom-quota-2mib-v1": "occupied Stage/same-fund allocation and refund proof pending",
    "workspace-commit-headroom-quota-4mib-v1": "occupied Commit/same-fund allocation and refund proof pending",
    "workspace-commit-reordered-base-copy-v1": "public SDK old/new/G2/pinned-byte copy proof pending",
    "workspace-commit-live-g1-g2-v1": "deterministic live successor/known Commit proof pending",
    "workspace-commit-known-unknown-local-c5-v1": "source-matched failure custody proof pending",
    "workspace-commit-sdk-stopping-refusal-v1": "deterministic live SDK stopping/refusal proof pending",
}


def oracle():
    retained = write.expected("dispersed", 4097)
    one_edit = bytearray(retained)
    one_edit[0] = ord("X")
    return {"retained": write.manifest(retained),
            **{case.id: write.manifest(retained if case.clean else one_edit) for case in CASES.values()}}, hashlib.sha256(retained).hexdigest()


def control_line(stdout):
    lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"CONTROL\t")]
    return json.loads(lines[0]) if len(lines) == 1 else None


def attempt(out, case, prepared, pin_sha):
    from shell_package import case_spec

    folder = out / case.id
    folder.mkdir()
    master = prepared["master"]
    for name in ("store", "history"):
        source = Path(master["path"]) / f"{name}.sqlite"
        target = folder / f"{name}.sqlite"
        if write.sha256(source) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} master seal changed")
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if write.sha256(target) != master[f"{name}_sha256"]:
            raise ValueError(f"{name} clone seal mismatch")
    fields = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                           "genesis_root_serial", "branch_id", "old_commit")}
    fields.update(scenario_id=case.id, command_hex=case.command.encode().hex(), expected_failure="0",
                  prelude_command_hex=PRELUDE.encode().hex(), clean_commit=str(int(case.clean)),
                  pin_read_bytes=str(case.pin_read_bytes),
                  telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    case_spec(folder / "case.before", fields)
    command = [prepared["artifacts"]["benchmark_shell"]["path"], "run", str(folder / "case.before"),
               str(folder / "store.sqlite"), str(folder / "history.sqlite"), prepared["image_id"]]
    env = {**os.environ, "LAYERFS_CONSTRUCTION_WORKERS": "1",
           "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"]}
    perf, stdout, stderr = write.execute(command, folder / "driver",
                                          timeout=case.command_budget_ns / 1e9, env=env)
    driver, control = write.receipt_line(stdout), control_line(stdout)
    counts = dict(item.split("=", 1) for item in (driver or {}).get("projection_counts", "").split(",")
                  if "=" in item)
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                   and control and control.get("pin_release_ok") and b"sandbox shutdown retained" not in stderr)
    route = bool(driver and control and driver.get("commit_called")
                 and int(counts.get("write", -1)) == 4097 + case.final_writes
                 and bool(control.get("up_to_date")) == case.clean
                 and control.get("prelude_head_commit")
                 and (driver.get("head_commit") == control["prelude_head_commit"]) == case.clean)
    pin = bool(control and control.get("pin_generation", 0) > 0 and control.get("pin_observation_ok"))
    full_pin = bool(control and control.get("pinned_bytes") == write.SIZE and control.get("pinned_sha256") == pin_sha)
    if case.pin_read_bytes:
        pin = pin and full_pin
    budget = perf["wall_ns"] <= case.command_budget_ns and not perf["timeout"]
    complete = bool(perf["exit_code"] == 0 and driver and driver.get("status") == "COMPLETE"
                    and route and pin and cleanup and budget)
    row = {"schema": "core-workspace-commit-fast-attempt-v2", "family_id": "workspace_commit",
           "profile": PROFILE, "case": case.id, "sample_count": 1, "source": prepared["identity"],
           "role": case.role,
           "artifacts": prepared["artifacts"], "image_id": prepared["image_id"], "image_reuse": prepared["image_build"],
           "master": master, "clone_method": prepared["clone_method"], "setup": "clone",
           "prelude_command": PRELUDE, "shell_command": case.command, "performance": perf,
           "command_budget_ns": case.command_budget_ns, "command_status": "PASS" if budget else "FAIL",
           "driver": driver, "control": control, "write_callbacks": counts.get("write"),
           "route_status": "PASS" if route else "FAIL", "pin_status": "PASS" if pin else "FAIL",
           "pin_verification_status": "PASS" if case.pin_read_bytes and full_pin else "FAIL" if case.pin_read_bytes else "SKIPPED",
           "pin_oracle_sha256": pin_sha, "cleanup_status": "PASS" if cleanup else "UNKNOWN" if perf["timeout"] else "FAIL",
           "verification": {"status": "SKIPPED", "reason": "fast lane; prove retained Stores separately without performance replay"},
           "functional_status": "INCOMPLETE", "performance_claim": False,
           "cache_contract": "host/container cache uncontrolled; whole Commit cache domain unproved",
           "numeric_latency_status": "INELIGIBLE", "status": "COMPLETE_DIAGNOSTIC" if complete else "FAIL"}
    write.save(folder / "receipt.json", row)
    return row


def run(selection, output, common):
    out = common.owned(output)
    identity = common.identities()
    if identity["source_dirty"]:
        raise ValueError("commit the Family 4 scenario before an attempt")
    out.mkdir(parents=True)
    selected = SELECTED if selection == "workspace-commit" else (selection,)
    summary = {"schema": "core-workspace-commit-fast-run-v2", "profile": PROFILE,
               "selected": list(selected), "identity": identity, "status": "INCOMPLETE", "rows": [],
               "earlier_family_policy": "reuse unaffected evidence; no automatic earlier-family benchmark runs"}
    try:
        with (common.RESULTS / ".run.lock").open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            prior = json.loads(PRIOR_IMAGE.read_text()) if PRIOR_IMAGE.exists() else None
            prepared = write.build(out, common, identity, reuse_image=prior)
            manifests, pin_sha = oracle()
            for name, content in manifests.items():
                (out / f"{name}.tsv").write_text(content)
            stop = None
            for name in selected:
                if stop:
                    row = {"case": name, "status": "NOT_RUN", "sample_count": 0, "reason": stop}
                    (out / name).mkdir()
                    write.save(out / name / "receipt.json", row)
                else:
                    try:
                        row = attempt(out, CASES[name], prepared, pin_sha)
                        if row["cleanup_status"] != "PASS":
                            stop = "prior cleanup failed or unknown"
                    except Exception as error:
                        stop = repr(error)
                        folder = out / name
                        folder.mkdir(exist_ok=True)
                        row = {"case": name, "status": "FAIL", "sample_count": int((folder / "driver.stdout").exists()), "reason": stop}
                        write.save(folder / "receipt.json", row)
                summary["rows"].append({"case": name, "status": row["status"]})
            summary["status"] = "COMPLETE_DIAGNOSTIC" if all(r["status"] == "COMPLETE_DIAGNOSTIC" for r in summary["rows"]) else "INCOMPLETE"
    except Exception as error:
        summary["error"] = repr(error)
    for name in selected[len(summary["rows"]):]:
        (out / name).mkdir(exist_ok=True)
        write.save(out / name / "receipt.json", {"case": name, "status": "NOT_RUN", "sample_count": 0,
                                                 "reason": summary.get("error", "setup failed")})
        summary["rows"].append({"case": name, "status": "NOT_RUN"})
    summary["remaining"] = REMAINING
    write.save(out / "run.json", summary)
    common.manifest_run(out)
    return out


def prove(performance_out, output, common):
    from shell_package import case_spec

    out = common.owned(output)
    common.verify_run_manifest(performance_out)
    original = json.loads((performance_out / "run.json").read_text())
    current = common.identities()
    if current["source_dirty"] or any(current[key] != original["identity"][key] for key in ("product_seal", "harness_seal", "cargo_lock_sha256")):
        raise ValueError("Family 4 proof requires the exact measured product/harness/dependency identity")
    prepared = json.loads((performance_out / "prepared.json").read_text())
    verifier = prepared["artifacts"]["verify_checkpoint5"]
    if write.sha256(verifier["path"]) != verifier["sha256"]:
        raise ValueError("independent verifier seal changed")
    out.mkdir(parents=True)
    rows = []
    with (common.RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for name in original["selected"]:
            folder = out / name
            folder.mkdir()
            source = performance_out / name
            receipt = json.loads((source / "receipt.json").read_text())
            proof = {"case": name, "sample_count": 0, "performance_receipt_sha256": write.sha256(source / "receipt.json"),
                     "status": "NOT_RUN", "reason": "performance/route/pin/cleanup did not complete"}
            if receipt["status"] == "COMPLETE_DIAGNOSTIC":
                fields = dict(line.split("=", 1) for line in (source / "case.before").read_text().splitlines())
                fields.update(old_commit=receipt["control"]["prelude_head_commit"],
                              expected_old_parent=receipt["master"]["old_commit"],
                              expected_head_commit=receipt["driver"]["head_commit"])
                case_spec(folder / "case.verify", fields)
                command = [verifier["path"], str(folder / "case.verify"), str(source / "store.sqlite"),
                           str(source / "history.sqlite"), str(performance_out / "retained.tsv"),
                           str(performance_out / f"{name}.tsv")]
                check, stdout, _ = write.execute(command, folder / "verifier", timeout=9,
                    env={**os.environ, "LAYERFS_CONSTRUCTION_WORKERS": "1", "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"]})
                try:
                    child = json.loads(stdout) if check["exit_code"] == 0 else None
                except (ValueError, UnicodeDecodeError):
                    child = None
                passed = bool(child and child.get("status") == "PASS" and
                              child.get("advanced") == (not CASES[name].clean) and
                              child.get("old_bytes") == child.get("new_bytes") == write.SIZE
                              and check["wall_ns"] <= CASES[name].verifier_budget_ns and not check["timeout"])
                proof.update(status="PASS" if passed else "FAIL", verification=check, child=child,
                             verifier_sha256=verifier["sha256"], numeric_latency_status="INELIGIBLE")
            write.save(folder / "proof.json", proof)
            rows.append(proof)
    common.verify_run_manifest(performance_out)
    write.save(out / "run.json", {"schema": "core-workspace-commit-proof-v2", "performance_run": str(performance_out),
        "performance_manifest_sha256": write.sha256(performance_out / "manifest.json"), "source": original["identity"],
        "rows": rows, "status": "PASS" if all(row["status"] == "PASS" for row in rows) else "INCOMPLETE"})
    common.manifest_run(out)
    return out


def report(out):
    run = json.loads((out / "run.json").read_text())
    if run["schema"] == "core-workspace-commit-proof-v2":
        return "\n".join(f"{row['case']}\tproof={row['status']}\tverifier_ns={row.get('verification', {}).get('wall_ns')}"
                         for row in run["rows"]) + "\n"
    rows = ["case\tcommand_ns\tfinal_exec_ns\tfinal_commit_ns\tprelude_exec_ns\tpin\tcleanup\tverification\tnumeric"]
    for item in run["rows"]:
        row = json.loads((out / item["case"] / "receipt.json").read_text())
        driver, control = row.get("driver") or {}, row.get("control") or {}
        rows.append("\t".join(map(str, (item["case"], row.get("performance", {}).get("wall_ns"),
            driver.get("exec_ns"), driver.get("commit_ns"), control.get("prelude_exec_ns"),
            row.get("pin_status"), row.get("cleanup_status"), row.get("verification", {}).get("status"),
            row.get("numeric_latency_status", "NOT_RUN")))))
    rows.extend(f"{name}\tNOT_RUN: {reason}" for name, reason in REMAINING.items())
    return "\n".join(rows) + "\n"

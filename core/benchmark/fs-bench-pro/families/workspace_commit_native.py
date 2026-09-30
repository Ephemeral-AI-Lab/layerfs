"""Family 4 native functional controls, using the existing external SDK test fixture."""
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import uuid

from families import workspace_write as shared

PROFILE = "native-inprocess-commit-functional-clone-v1"
SCHEMA = "core-workspace-commit-native-run-v1"


@dataclass(frozen=True)
class Case:
    id: str
    test: str
    master: str = "small"
    clones: int = 1
    retained: bool = False
    budget_ns: int = 60_000_000_000
    ignored: bool = False
    owner_deferred: bool = False


CASES = {case.id: case for case in (
    Case("workspace-commit-full-lowering-size-64mib-native-v2", "phase_b_commit::full_lowering_64mib", "large", ignored=True),
    Case("workspace-commit-stage-headroom-quota-2mib-native-v2", "phase_b_commit::stage_headroom_2mib", retained=True),
    Case("workspace-commit-headroom-quota-4mib-native-v2", "phase_b_commit::commit_headroom_4mib"),
    Case("workspace-commit-live-g1-g2-native-v2", "phase_a_selected_origins_overlap_zero_hardlinks_and_completion_credit"),
    Case("workspace-commit-pin-custody-native-v2", "phase_a_lease_admission_frozen_names_and_checked_release"),
    Case("workspace-commit-known-unknown-native-v2", "phase_a_known_save_and_unknown_commit_keep_custody_without_replay", clones=2, retained=True),
    Case("workspace-commit-local-c5-native-v2", "phase_a_known_canonical_local_failure_resumes_same_selector_once"),
    Case("workspace-commit-reordered-base-copy-native-v2", "phase_a_large_payload_append_resize_and_backward_base_copy"),
    Case("workspace-commit-native-count-writes-8192-v2", "phase_b_commit::native_count_8192", "count", ignored=True),
    Case("workspace-commit-native-count-writes-10240-v2", "phase_b_commit::native_count_10240", "count", ignored=True, owner_deferred=True),
)}
SELECTED = tuple(CASES)[:8]
COUNTS = tuple(CASES)[8:]
RECONCILIATION = tuple(name for name in SELECTED if name in {
    "workspace-commit-full-lowering-size-64mib-native-v2",
    "workspace-commit-headroom-quota-4mib-native-v2",
    "workspace-commit-live-g1-g2-native-v2",
    "workspace-commit-local-c5-native-v2",
    "workspace-commit-reordered-base-copy-native-v2",
})


def invoke(command, destination, timeout=60):
    record, stdout, stderr = shared.execute(command, destination, timeout=timeout)
    shared.save(destination.with_suffix(".json"), record)
    return record, stdout, stderr


def checked(command, destination, timeout=60):
    record, stdout, stderr = invoke(command, destination, timeout)
    if record["exit_code"] != 0 or record["timeout"]:
        raise RuntimeError(f"command failed: {command}; see {destination}.json")
    return stdout


def build(out, common, identity):
    files = [common.ROOT / "core/crates/layerfs-api/sdk/tests/inherited_workspace.rs",
             *sorted((common.ROOT / "core/crates/layerfs-api/sdk/tests/support").glob("*.rs"))]
    test_seal = common.seal(files)
    inputs = {"product_seal": identity["product_seal"], "test_source_seal": test_seal,
              "cargo_lock_sha256": identity["cargo_lock_sha256"], "profile": "release",
              "sqlite_provider": "locked published rusqlite/bundled; native functional binary only"}
    cache = common.RESULTS / "workspace-commit-native-build-v1.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior["inputs"] == inputs and shared.sha256(prior["binary"]["path"]) == prior["binary"]["sha256"]:
        result = {**prior, "mode": "exact-binary-reuse", "wall_ns": 0}
    else:
        command = ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path", "core/Cargo.toml",
                   "--locked", "--offline", "--target", "aarch64-unknown-linux-musl", "-p", "layerfs-sdk",
                   "-p", "layerfs-storage", "--features", "rusqlite/bundled", "--test", "inherited_workspace"]
        record, _, _ = invoke(command, out / "build", 180)
        if record["exit_code"] != 0:
            raise RuntimeError("native release test build failed")
        binaries = [file for file in (common.target_path() / "aarch64-unknown-linux-musl/release/deps").glob("inherited_workspace-*")
                    if file.is_file() and file.suffix == "" and os.access(file, os.X_OK)]
        if len(binaries) != 1:
            raise ValueError("native test executable identity is ambiguous")
        sha = shared.sha256(binaries[0])
        archive = common.RESULTS / "binary-archive" / sha / "inherited_workspace"
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            shutil.copy2(binaries[0], archive)
            archive.chmod(0o555)
        result = {"inputs": inputs, "binary": {"path": str(archive), "sha256": sha},
                  "mode": "locked-release-build", "wall_ns": record["wall_ns"],
                  "build_budget_ns": 30_000_000_000,
                  "build_status": "PASS" if record["wall_ns"] <= 30_000_000_000 else "BUILD_SLOW"}
        shared.save(cache, result)
    shared.save(out / "build-seal.json", result)
    return result


def masters(out, container, common, identity, binary, needed, *, layouts=None):
    root = common.RESULTS / "workspace-commit-native-master-v1"
    result = {}
    layouts = layouts or {"small": (0, 0, False, 0), "large": (64 << 20, 0, False, 0), "count": (327_680, 0, False, 0)}
    for name, (size, extra, deep, unrelated) in layouts.items():
        if name not in needed:
            continue
        path = root / name
        seal = path / "prepared.json"
        reused = seal.exists()
        if reused:
            prepared = json.loads(seal.read_text())
            if (prepared["size"], prepared.get("extra", 0), prepared.get("deep", False), prepared.get("unrelated", 0)) != (size, extra, deep, unrelated):
                raise ValueError("prepared layout mismatch")
        else:
            if path.exists():
                raise ValueError("incomplete master retained; no overwrite")
            path.mkdir(parents=True)
            record, stdout, _ = invoke(["docker", "exec", "-e", f"LAYERFS_PHASE_B_PREPARE=/work/master-{name}",
                                       "-e", f"LAYERFS_PHASE_B_SIZE={size}",
                                       "-e", f"LAYERFS_PHASE_B_EXTRA={extra}", "-e", f"LAYERFS_PHASE_B_DEEP={int(deep)}",
                                       "-e", f"LAYERFS_PHASE_B_UNRELATED={unrelated}", container, "/test",
                                       "--exact", "phase_b_commit::prepare_master", "--ignored", "--nocapture"],
                                      out / f"prepare-{name}")
            if record["exit_code"] != 0 or b"closed=true" not in stdout:
                raise RuntimeError("master preparation failed")
            for file in ("store.sqlite", "history.sqlite", "fixture.before"):
                checked(["docker", "cp", f"{container}:/work/master-{name}/{file}", str(path / file)], out / f"acquire-{name}-{file}")
                (path / file).chmod(0o444)
            prepared = {"size": size, "extra": extra, "deep": deep, "unrelated": unrelated, "path": str(path), "preparation": record,
                        "producer_source": identity, "producer_binary": binary,
                        "files": {file: shared.sha256(path / file) for file in ("store.sqlite", "history.sqlite", "fixture.before")},
                        "oracle": "grand-base/sibling-base; large data[i]=i%251; full relevant bytes checked in native child"}
            shared.save(seal, prepared)
        if any(shared.sha256(path / file) != sha for file, sha in prepared["files"].items()):
            raise ValueError("prepared master changed")
        result[name] = {**prepared, "reuse": reused}
    shared.save(out / "prepared.json", result)
    return result


def attempt(out, case, prepared, container):
    folder = out / case.id
    folder.mkdir()
    source = Path(prepared["path"])
    for index in range(case.clones):
        clone = folder / f"clone-{index}"
        clone.mkdir()
        for file, sha in prepared["files"].items():
            if shared.sha256(source / file) != sha:
                raise ValueError("master changed before clone")
            shutil.copyfile(source / file, clone / file)
            (clone / file).chmod(0o644)
            if shared.sha256(clone / file) != sha:
                raise ValueError("clone hash mismatch")
    checked(["docker", "cp", str(folder), f"{container}:/work/"], folder / "copy")
    checked(["docker", "exec", container, "chown", "-R", "0:0", f"/work/{case.id}"], folder / "clone-owner")
    for index in range(case.clones):
        for file, sha in prepared["files"].items():
            output = checked(["docker", "exec", container, "sha256sum", f"/work/{case.id}/clone-{index}/{file}"], folder / f"hash-{index}-{file}")
            if output.decode().split()[0] != sha:
                raise ValueError("Linux clone hash mismatch")
    command = ["docker", "exec", "-e", f"LAYERFS_PHASE_B_CLONES=/work/{case.id}",
               "-e", f"LAYERFS_TEST_BACKING_ROOT=/work/{case.id}", "-e", "LAYERFS_CONSTRUCTION_WORKERS=1",
               container, "/test", "--exact", case.test, "--test-threads=1", "--nocapture"]
    if case.ignored:
        command.append("--include-ignored")
    for key, value in getattr(case, "environment", ()):
        command[2:2] = ["-e", f"{key}={value}"]
    record, stdout, stderr = invoke(command, folder / "functional", case.budget_ns / 1e9)
    ok = record["exit_code"] == 0 and not record["timeout"] and record["wall_ns"] <= case.budget_ns and b"1 passed" in stdout and b"initialized=false" in stdout
    expected_retained = case.retained or any(line.startswith(b"PHASE_B_COUNT") and b"retained=true" in line for line in stdout.splitlines())
    if expected_retained or not ok:
        checked(["docker", "cp", f"{container}:/work/{case.id}", str(folder / "retained-linux-state")], folder / "custody-copy")
    row = {"case": case.id, "test": case.test, "route": "Linux native Workspace + in-process production Server",
           "functional": record, "functional_budget_ns": case.budget_ns,
           "functional_status": "PASS" if ok else "FAIL", "status": "PASS" if ok else "FAIL",
           "sample_count": 1, "performance_claim": False, "numeric_latency_status": "INELIGIBLE",
           "verification_scope": "in-child full relevant old/new/G1/G2/pin bytes, quota/custody assertions; no separate speed arm",
           "cleanup_status": "EXPECTED_RETAINED" if ok and expected_retained else "PASS" if ok else "UNKNOWN",
           "clone_method": "independent byte copy; Linux setup ownership 0:0; host and Linux byte hashes matched; closed prepared master",
           "prepared": prepared, "observations": [line for line in stdout.decode(errors="replace").splitlines() if line.startswith(("PHASE_", "INHERITED_", "RENAME_"))],
           "sdk_time": "N/A: native component route", "cache_contract": "uncontrolled, functional only"}
    shared.save(folder / "receipt.json", row)
    return row


def run(selection, output, common, *, cases=None, selected=None, profile=PROFILE, schema=SCHEMA, prepare=masters):
    out = common.owned(output)
    identity = common.identities()
    if identity["source_dirty"]:
        raise ValueError("commit the native control before collection")
    out.mkdir(parents=True)
    cases = CASES if cases is None else cases
    selected = selected if selected is not None else SELECTED if selection == "workspace-commit-native" else SELECTED[1:] if selection == "workspace-commit-native-tail" else COUNTS if selection == "workspace-commit-native-counts" else RECONCILIATION if selection == "workspace-commit-native-reconciliation" else (selection,)
    summary = {"schema": schema, "profile": profile, "identity": identity, "selected": list(selected), "rows": [],
               "earlier_family_policy": "unaffected production and earlier evidence reused; no earlier resampling"}
    if all(cases[name].owner_deferred for name in selected):
        summary["rows"] = [{"case": name, "status": "SKIPPED / OWNER-DEFERRED", "sample_count": 0,
            "reason": "owner explicitly deferred further10240 attempts and bounded-memory architecture work to issue276",
            "cleanup_status": "N/A: no resources acquired"} for name in selected]
        summary.update(status="OWNER-DEFERRED", external_cleanup=[])
        shared.save(out / "run.json", summary)
        common.manifest_run(out)
        return out
    container = "issue286-native-" + uuid.uuid4().hex[:16]
    volume = container + "-backing"
    admitted = False
    try:
        with (common.RESULTS / ".run.lock").open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            build_record = build(out, common, identity)
            summary["build"] = build_record
            checked(["docker", "volume", "create", volume], out / "volume-create")
            checked(["docker", "run", "-d", "--name", container,
                     "--mount", f"type=volume,source={volume},target=/work",
                     "--mount", f"type=bind,source={build_record['binary']['path']},target=/test,readonly",
                     "-e", "LAYERFS_CONSTRUCTION_WORKERS=1", shared.BASE, "sleep", "86400"], out / "container-create")
            admitted = True
            summary["image"] = json.loads(checked(["docker", "inspect", container], out / "container-inspect"))[0]["Image"]
            summary["backing"] = checked(["docker", "exec", container, "stat", "-f", "-c", "%t %S", "/work"], out / "backing-profile").decode().strip()
            if summary["backing"] != "ef53 4096":
                raise ValueError("required Linux backing filesystem profile missing")
            prepared = prepare(out, container, common, identity, build_record["binary"], {cases[name].master for name in selected if not cases[name].owner_deferred})
            for name in selected:
                if cases[name].owner_deferred:
                    summary["rows"].append({"case": name, "status": "SKIPPED / OWNER-DEFERRED", "sample_count": 0,
                        "reason": "owner explicitly deferred further10240 attempts and bounded-memory architecture work to issue276",
                        "cleanup_status": "N/A: no resources acquired"})
                    continue
                try:
                    row = attempt(out, cases[name], prepared[cases[name].master], container)
                except Exception as error:
                    row = {"case": name, "status": "FAIL", "reason": repr(error)}
                    shared.save(out / f"{name}-failure.json", row)
                summary["rows"].append(row)
                if row["status"] != "PASS":
                    break
    except Exception as error:
        summary["error"] = repr(error)
    finally:
        cleanup = []
        if admitted:
            cleanup.append(invoke(["docker", "rm", "-f", container], out / "external-container-cleanup")[0])
        cleanup.append(invoke(["docker", "volume", "rm", volume], out / "external-volume-cleanup")[0])
        summary["external_cleanup"] = cleanup
    for name in selected[len(summary["rows"]):]:
        summary["rows"].append({"case": name, "status": "NOT_RUN", "reason": summary.get("error", "prior case failed")})
    qualified = all(row["status"] in ("PASS", "SKIPPED / OWNER-DEFERRED") for row in summary["rows"])
    summary["status"] = ("SCOPED_PASS_WITH_OWNER_DEFERRED" if any(cases[name].owner_deferred for name in selected) else "PASS") if qualified and all(row["exit_code"] == 0 for row in cleanup) else "INCOMPLETE"
    shared.save(out / "run.json", summary)
    common.manifest_run(out)
    return out


def report(out):
    data = json.loads((out / "run.json").read_text())
    lines = ["| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |", "| --- | ---: | --- | --- | --- |"]
    for row in data["rows"]:
        wall = row.get("functional", {}).get("wall_ns", "UNAVAILABLE")
        lines.append(f"| {row['case']} | {wall} / 60 s | {row['status']} | {row.get('cleanup_status', 'UNAVAILABLE')} | INELIGIBLE |")
    return "\n".join(lines) + "\n"

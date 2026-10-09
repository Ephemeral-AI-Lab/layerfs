"""Frozen #286 public SDK/POSIX write matrix and one-attempt runner."""

from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from shared import retired_families


HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[2]
SIZE = 10 << 20
MASTER = Path(
    "/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs/"
    "benchmark-results/fs-bench-pro/issue271/fourhop-patterns-prepared-v1/prepared.json"
)
MASTER_STORE_SHA256 = "20ea70dc9509e1819b11667bf97ab2a5ab3bd2b5d628d0a48f4c4fa6876691be"
MASTER_HISTORY_SHA256 = "55a02d7da0c35b084b721706354dcaf6e30aee3f824ad32543cf2e108dbdfd19"
MASTER_PROOF_SHA256 = "d2f535e0a405eca68746b7968350118b85ddbe69b95a66416a38238a7da5f6c0"
OLD_MANIFEST_SHA256 = "d664c30d37508679421ab4989c3256f5da430edda0c4dc4d148d863085249927"
PATTERNS = ("append", "dispersed", "repeated")
COUNTS = (100, 512, 4097)


@dataclass(frozen=True)
class Case:
    id: str
    pattern: str
    writes: int
    command_budget_ns: int
    verifier_budget_ns: int = 9_000_000_000

    @property
    def command(self):
        return f"/fixtures/bin/write-separated {self.pattern} data.bin {self.writes}"


CASES = {case.id: case for case in (
    Case(f"workspace-write-{pattern}-writes-{count}-v1", pattern, count,
         (25 if count == 4097 else 15) * 1_000_000_000)
    for pattern in PATTERNS for count in COUNTS
)}
SELECTED = tuple(CASES)
BASE = "alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8"
WRITER_SHA256 = "f293d71f2a16aaeecb4de6c1c2cf5b74dfcb0e616204e591f83c3ef802a686d3"
WRITER_SOURCE_SHA256 = "dc21c66ddb85be7c5d27c164b292cbf19a82f4e5a3352a8197050cfcae15d8f0"
CACHE_CONTRACT = "darwin-zero-source-residency-linux-direct-backing-v1"
MAX_LAUNCH_GAP_NS = 1_000_000_000
DIRECT_IO_FILES = {
    "core/crates/layerfs-workspace/src/backing/segments.rs": "dd2286500231693d0498379f7825af3314cc902f65bdc0b27c122b74b24c735d",
    "core/crates/layerfs-fuse/src/adapter.rs": "e361037d9c87ac42dda651e06a708053b999bfa54a69a7e5038a3e1398380fa7",
}
RESIDENCY_HELPER = ROOT / "core/benchmark/fs-bench-pro-storage-content/shared/residency.py"
sys.path.insert(0, str(RESIDENCY_HELPER.parent))
import residency  # noqa: E402


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def master():
    """Pin the independent prepared master without reading it into a timed phase."""
    prepared = json.loads(MASTER.read_text())
    row = prepared["master"]
    source = Path(row["path"])
    for name, expected in (("store", MASTER_STORE_SHA256),
                           ("history", MASTER_HISTORY_SHA256),
                           ("verify.stdout", MASTER_PROOF_SHA256)):
        path = source / (name if name == "verify.stdout" else f"{name}.sqlite")
        if sha256(path) != expected:
            raise ValueError(f"prepared {name} seal changed")
    if (row["store_sha256"] != MASTER_STORE_SHA256
            or row["history_sha256"] != MASTER_HISTORY_SHA256
            or row["proof_sha256"] != MASTER_PROOF_SHA256):
        raise ValueError("prepared master record changed")
    return {**row, "path": str(source), "prepared": str(MASTER)}


def expected(pattern, writes):
    if pattern not in PATTERNS or writes not in COUNTS:
        raise ValueError("unregistered write matrix cell")
    data = bytearray(b"A" * SIZE)
    for i in range(writes):
        value = ord("B") + i % 24
        if pattern == "append":
            data.append(value)
        elif pattern == "dispersed":
            data[(104729 + i * 2654435761) % SIZE] = value
        else:
            data[5 << 20] = value
    return bytes(data)


def manifest(data):
    return (f".\td\t493\t0\t-\n"
            f"data.bin\tf\t420\t{len(data)}\t{hashlib.sha256(data).hexdigest()}\n")


def manifests():
    old = manifest(b"A" * SIZE)
    if hashlib.sha256(old.encode()).hexdigest() != OLD_MANIFEST_SHA256:
        raise ValueError("old oracle manifest differs from frozen #271")
    return {"old": old, **{case.id: manifest(expected(case.pattern, case.writes))
                            for case in CASES.values()}}


def save(path, value):
    Path(path).write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")


def execute(command, folder, *, timeout=None, env=None):
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=timeout, env=env)
        stdout, stderr, code, expired = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, expired = error.stdout or b"", error.stderr or b"", None, True
    wall = time.monotonic_ns() - started
    (folder.with_suffix(".stdout")).write_bytes(stdout)
    (folder.with_suffix(".stderr")).write_bytes(stderr)
    return {"command": command, "started_ns": started, "wall_ns": wall, "exit_code": code, "timeout": expired,
            "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
            "stderr_sha256": hashlib.sha256(stderr).hexdigest()}, stdout, stderr


def receipt_line(stdout):
    lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    return json.loads(lines[0]) if len(lines) == 1 else None


def assess_numeric_cache(row):
    if row.get("family_id") != "workspace_write":
        raise ValueError("Family 3 cache assessment requires a Workspace write row")
    # The frozen #273 checkpoint-5 spec explicitly says host source eviction
    # and private O_DIRECT do not establish the whole Exec-to-Commit cache state.
    return {"status": "INELIGIBLE",
            "reason": "Commit cache domain is not proved by cold host source plus private direct I/O"}


def build(out, runner, identity, *, reuse_image=None, artifacts_only=False):
    retired_families.require_compatible(ROOT)
    if sys.platform != "darwin":
        raise RuntimeError("this cold-source profile requires Darwin mincore/msync and libproc")
    for name, expected in DIRECT_IO_FILES.items():
        if sha256(ROOT / name) != expected:
            raise ValueError(f"direct-I/O source identity changed: {name}")
    failures = residency.self_check()
    save(out / "cache-self-check.json", {"status": "PASS" if not failures else "FAIL",
                                         "failures": failures, "helper_sha256": sha256(RESIDENCY_HELPER)})
    if failures:
        raise RuntimeError("host residency instrument self-check failed")
    target = runner.target_path()
    artifacts = {}
    builds = (
        ("sdk", ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked",
                 "-p", "layerfs-sdk", "--example", "benchmark_shell"]),
        ("verifier", ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked",
                      "-p", "layerfs-server", "--example", "verify_checkpoint5"]),
        ("daemon", ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path", "core/Cargo.toml", "--locked",
                    "--offline", "--target", "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"]),
    )
    for name, command in builds:
        result, _, _ = execute(command, out / f"build-{name}", env={**os.environ, "CARGO_TARGET_DIR": str(target)})
        save(out / f"build-{name}.json", result)
        if result["exit_code"] != 0:
            raise RuntimeError(f"{name} locked release build failed")
    sources = {
        "benchmark_shell": target / "release/examples/benchmark_shell",
        "verify_checkpoint5": target / "release/examples/verify_checkpoint5",
        "layerfs-daemon": target / "aarch64-unknown-linux-musl/release/layerfs-daemon",
    }
    for name, source in sources.items():
        digest = sha256(source)
        archive = runner.RESULTS / "binary-archive" / digest / name
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            shutil.copyfile(source, archive)
            archive.chmod(0o555)
        if sha256(archive) != digest:
            raise ValueError(f"{name} archive seal mismatch")
        artifacts[name] = {"path": str(archive), "sha256": digest}
    context = out / "image-context"
    (context / "bin").mkdir(parents=True)
    prior = MASTER.parent / "image-context/bin/write-separated"
    source = HERE / "writers/write-separated.c"
    if sha256(source) != WRITER_SOURCE_SHA256 or sha256(prior) != WRITER_SHA256:
        raise ValueError("sealed writer/source changed")
    shutil.copyfile(prior, context / "bin/write-separated")
    (context / "bin/write-separated").chmod(0o555)
    shutil.copyfile(artifacts["layerfs-daemon"]["path"], context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o555)
    (context / "Dockerfile").write_text(
        f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\nCOPY bin /fixtures/bin\n"
        'ENTRYPOINT ["/layerfs-daemon"]\n')
    if (reuse_image and reuse_image["artifacts"]["layerfs-daemon"]["sha256"] == artifacts["layerfs-daemon"]["sha256"]
            and reuse_image["writer_binary_sha256"] == WRITER_SHA256
            and reuse_image["image_dockerfile_sha256"] == sha256(context / "Dockerfile")):
        image_id = reuse_image["image_id"]
        image = {"mode": "exact-image-reuse", "image_id": image_id, "wall_ns": 0,
                 "daemon_sha256": artifacts["layerfs-daemon"]["sha256"], "command": None}
    else:
        image, stdout, _ = execute(["docker", "build", "-q", str(context)], out / "image-build")
        image["mode"] = "build"
        if image["exit_code"] != 0:
            save(out / "image-build.json", image)
            raise RuntimeError("current-daemon image build failed")
        image_id = stdout.decode().strip()
    save(out / "image-build.json", image)
    if subprocess.check_output(["docker", "image", "inspect", image_id, "--format", "{{.Id}}"], text=True).strip() != image_id:
        raise ValueError("image ID mismatch")
    if artifacts_only:
        prepared = {"identity": identity, "artifacts": artifacts, "image_id": image_id,
                    "image_dockerfile_sha256": sha256(context / "Dockerfile"),
                    "writer_binary_sha256": WRITER_SHA256, "image_build": image,
                    "build_mode": "worktree-local locked release; exact daemon/image reuse when sealed"}
        save(out / "prepared-artifacts.json", prepared)
        return prepared
    old = master()
    oracle = manifests()
    (out / "old.tsv").write_text(oracle.pop("old"))
    for name, content in oracle.items():
        (out / f"{name}.tsv").write_text(content)
    cursor = json.loads(MASTER.read_text())["cursor_key"]
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": cursor, "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    proof, stdout, _ = execute([artifacts["verify_checkpoint5"]["path"],
        str(Path(old["path"]) / "verify.case"), str(Path(old["path"]) / "store.sqlite"),
        str(Path(old["path"]) / "history.sqlite"), str(out / "old.tsv"), str(out / "old.tsv")],
        out / "master-reproof", timeout=9, env=env)
    save(out / "master-reproof.json", proof)
    if proof["exit_code"] != 0 or json.loads(stdout).get("status") != "PASS":
        raise RuntimeError("current verifier rejected sealed old master")
    prepared = {"identity": identity, "artifacts": artifacts, "image_id": image_id,
                "image_dockerfile_sha256": sha256(context / "Dockerfile"),
                "writer_source_sha256": WRITER_SOURCE_SHA256, "writer_binary_sha256": WRITER_SHA256,
                "image_build": image,
                "master": old, "cursor_key": cursor, "master_reproof": proof,
                "clone_method": "shutil.copyfile independent writable byte copy",
                "cache_contract": CACHE_CONTRACT,
                "cold_helper_sha256": sha256(RESIDENCY_HELPER),
                "direct_io_source_sha256": DIRECT_IO_FILES,
                "device_read_threshold": "90% of cloned Store+history allocated bytes",
                "build_mode": "worktree-local locked release; sealed static writer reuse"}
    save(out / "prepared.json", prepared)
    return prepared


def case_run(out, case, prepared):
    from shell_package import case_spec, lft1

    folder = out / case.id
    folder.mkdir()
    old = prepared["master"]
    clone = {}
    for name in ("store", "history"):
        source = Path(old["path"]) / f"{name}.sqlite"
        target = folder / f"{name}.sqlite"
        if sha256(source) != old[f"{name}_sha256"]:
            raise ValueError(f"master {name} seal changed")
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if sha256(target) != old[f"{name}_sha256"]:
            raise ValueError(f"cloned {name} seal mismatch")
        clone[name] = str(target)
    fields = {key: old[key] for key in ("project_id", "genesis_layer", "genesis_root",
                                           "genesis_root_serial", "branch_id", "old_commit")}
    fields.update(scenario_id=case.id, command_hex=case.command.encode().hex(),
                  expected_failure="0", write_pattern=case.pattern, pattern_count=str(case.writes),
                  telemetry_run=str(int.from_bytes(os.urandom(16), "big") or 1))
    case_spec(folder / "case.before", fields)
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"],
           "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    command = [prepared["artifacts"]["benchmark_shell"]["path"], "run", str(folder / "case.before"),
               clone["store"], clone["history"], prepared["image_id"]]
    cold = {}
    for name, path in clone.items():
        with open(path, "rb") as source:
            os.fsync(source.fileno())  # Closed setup copy, never Workspace backing.
        report = residency.de_warm(path)
        cold[name] = {**report.as_fields(),
                      "allocated_bytes": Path(path).stat().st_blocks * 512}
        save(folder / "cold-source.json", cold)
    cold["final_residency"] = {name: residency.residency(path).as_fields()
                                for name, path in clone.items()}
    cold["finished_ns"] = time.monotonic_ns()
    save(folder / "cold-source.json", cold)
    performance, stdout, stderr = execute(command, folder / "driver",
                                           timeout=case.command_budget_ns / 1e9, env=env)
    cold["launch_gap_ns"] = performance["started_ns"] - cold["finished_ns"]
    save(folder / "cold-source.json", cold)
    driver = receipt_line(stdout)
    verification = {"status": "NOT_RUN", "reason": "no confirmed new head"}
    if driver and driver.get("head_commit"):
        case_spec(folder / "case.verify", {**fields, "expected_head_commit": driver["head_commit"]})
        check, checked, _ = execute([prepared["artifacts"]["verify_checkpoint5"]["path"],
            str(folder / "case.verify"), clone["store"], clone["history"],
            str(out / "old.tsv"), str(out / f"{case.id}.tsv")], folder / "verifier",
            timeout=case.verifier_budget_ns / 1e9, env=env)
        try:
            child = json.loads(checked) if check["exit_code"] == 0 else None
        except (ValueError, UnicodeDecodeError):
            child = None
        verification = {**check, "child": child,
                        "status": "PASS" if child and child.get("status") == "PASS"
                        and child.get("advanced") and child.get("pattern_runs") ==
                        (case.writes if case.pattern == "dispersed" else 1) else "FAIL"}
    counts = dict(item.split("=", 1) for item in driver.get("projection_counts", "").split(",")
                  if "=" in item) if driver else {}
    cleanup = bool(driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                   and b"sandbox shutdown retained" not in stderr)
    route = bool(driver and driver.get("commit_called") and
                 int(counts.get("write", -1)) == case.writes)
    under_budget = performance["wall_ns"] <= case.command_budget_ns and not performance["timeout"]
    device_floor = (cold["store"]["allocated_bytes"] + cold["history"]["allocated_bytes"]) * 9 // 10
    device_bytes = driver.get("host_disk_read_bytes") if driver else None
    cache_reasons = []
    if sha256(RESIDENCY_HELPER) != prepared["cold_helper_sha256"]:
        cache_reasons.append("residency helper seal changed")
    if any(value["resident_pages"] != 0 for value in cold["final_residency"].values()):
        cache_reasons.append("source pages resident at launch")
    if not 0 <= cold["launch_gap_ns"] <= MAX_LAUNCH_GAP_NS:
        cache_reasons.append("source check stale at launch")
    if not isinstance(device_bytes, int) or device_bytes < device_floor:
        cache_reasons.append("host device-read attestation below frozen floor")
    source_cache_eligible = not cache_reasons
    functional = bool(performance["exit_code"] == 0 and under_budget and driver
                      and driver.get("status") == "COMPLETE" and route and cleanup
                      and verification["status"] == "PASS")
    row = {"schema": "core-workspace-write-attempt-v1", "family_id": "workspace_write",
           "scenario_id": case.id, "pattern": case.pattern, "writes": case.writes,
           "source": prepared["identity"], "artifacts": prepared["artifacts"],
           "image_id": prepared["image_id"], "master": old, "clone": clone,
           "clone_method": prepared["clone_method"], "cache_contract": prepared["cache_contract"],
           "cold_source": cold, "device_read_floor_bytes": device_floor,
           "host_disk_read_bytes": device_bytes,
           "cache_eligibility_reasons": cache_reasons,
           "source_cache_status": "PASS" if source_cache_eligible else "INELIGIBLE",
           "direct_io_source_sha256": prepared["direct_io_source_sha256"],
           "operation_surface": "public WorkspaceApi mount/exec/commit; ordinary POSIX-FUSE writer",
           "shell_command": case.command, "performance": performance,
           "performance_budget_ns": case.command_budget_ns,
           "performance_budget_status": "PASS" if under_budget else "FAIL",
           "driver": driver, "callback_counts": counts, "route_status": "PASS" if route else "FAIL",
           "lft1": lft1(stderr), "verification": verification,
           "verification_budget_ns": case.verifier_budget_ns,
           "old_manifest_sha256": sha256(out / "old.tsv"),
           "new_manifest_sha256": sha256(out / f"{case.id}.tsv"),
           "cleanup_status": "PASS" if cleanup else "UNKNOWN" if performance["timeout"] else "FAIL",
           "functional_status": "PASS" if functional else "FAIL",
           "sample_count": 1,
           "row_status": "INELIGIBLE" if functional else "FAIL"}
    row["numeric_cache_assessment"] = assess_numeric_cache(row)
    row["numeric_latency_status"] = row["numeric_cache_assessment"]["status"]
    save(folder / "receipt.json", row)
    return row


def run(selection, output, runner):
    out = runner.owned(output)
    identity = runner.identities()
    if not retired_families.compatible(ROOT):
        selected = SELECTED if selection == "workspace_write" else (selection,)
        retired_families.record(out, selection, selected, identity)
        runner.manifest_run(out)
        return out
    if identity["source_dirty"]:
        raise ValueError("commit the Family 3 runner before measurement")
    out.mkdir(parents=True)
    selected = SELECTED if selection == "workspace_write" else (selection,)
    summary = {"schema": "core-workspace-write-run-v1", "selection": selection,
               "selected": list(selected), "identity": identity, "rows": [], "status": "INCOMPLETE"}
    lock_path = runner.RESULTS / ".run.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with lock_path.open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            prepared = build(out, runner, identity)
            stop = None
            for name in selected:
                if stop:
                    row = {"scenario_id": name, "sample_count": 0, "row_status": "NOT_RUN", "reason": stop}
                    folder = out / name
                    folder.mkdir()
                    save(folder / "receipt.json", row)
                else:
                    try:
                        row = case_run(out, CASES[name], prepared)
                        if row["cleanup_status"] != "PASS":
                            stop = "prior cleanup failed or unknown"
                    except Exception as error:
                        stop = f"case exception: {error!r}"
                        folder = out / name
                        folder.mkdir(exist_ok=True)
                        attempted = (folder / "driver.stdout").exists()
                        row = {"scenario_id": name, "sample_count": int(attempted),
                               "row_status": "FAIL" if attempted else "NOT_RUN",
                               "cleanup_status": "UNKNOWN" if attempted else "NOT_RUN", "reason": stop}
                        save(folder / "receipt.json", row)
                summary["rows"].append({"scenario_id": name, "row_status": row["row_status"]})
            summary["status"] = ("PASS" if all(row["row_status"] == "PASS" for row in summary["rows"])
                                 else "FUNCTIONAL_PASS_NUMERIC_INELIGIBLE"
                                 if all(row["row_status"] in ("PASS", "INELIGIBLE") for row in summary["rows"])
                                 else "INCOMPLETE")
    except Exception as error:
        summary["error"] = repr(error)
    for name in selected[len(summary["rows"]):]:
        folder = out / name
        folder.mkdir(exist_ok=True)
        save(folder / "receipt.json", {"scenario_id": name, "sample_count": 0,
                                       "row_status": "NOT_RUN", "reason": summary.get("error", "setup failed")})
        summary["rows"].append({"scenario_id": name, "row_status": "NOT_RUN"})
    save(out / "run.json", summary)
    runner.manifest_run(out)
    return out


def verify(out, runner):
    runner.verify_run_manifest(out)
    summary = json.loads((out / "run.json").read_text())
    for row in summary["rows"]:
        receipt = json.loads((out / row["scenario_id"] / "receipt.json").read_text())
        if receipt["row_status"] != row["row_status"]:
            raise ValueError("Family 3 summary/receipt mismatch")
    return "PASS"


def report(out):
    summary = json.loads((out / "run.json").read_text())
    rows = ["case\tsamples\tcommand_s\tlimit_s\texec_s\tcommit_s\tverifier_s\tfunctional\tlatency\tcleanup"]
    for row in summary["rows"]:
        receipt = json.loads((out / row["scenario_id"] / "receipt.json").read_text())
        driver = receipt.get("driver") or {}
        perf = receipt.get("performance") or {}
        check = receipt.get("verification") or {}
        seconds = lambda value: f"{value / 1e9:.3f}" if isinstance(value, int) else "-"
        rows.append("\t".join((row["scenario_id"], str(receipt.get("sample_count", 0)),
            seconds(perf.get("wall_ns")), seconds(receipt.get("performance_budget_ns")),
            seconds(driver.get("exec_ns")), seconds(driver.get("commit_ns")),
            seconds(check.get("wall_ns")), receipt.get("functional_status", "NOT_RUN"),
            assess_numeric_cache(receipt)["status"] if receipt.get("family_id") == "workspace_write"
            else "NOT_RUN", receipt.get("cleanup_status", "NOT_RUN"))))
    return "\n".join(rows) + "\n"

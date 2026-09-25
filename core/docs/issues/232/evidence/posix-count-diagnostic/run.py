#!/usr/bin/env python3
"""Prepare and run two append-only POSIX shift count diagnostics."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
CORE = ROOT / "core"
sys.path.insert(0, str(CORE / "benchmark/fs-bench-pro"))
from runner import RESULTS, identities  # noqa: E402
from phase1c import cursor_key, digest, write  # noqa: E402
from shared import edit_route  # noqa: E402

PRE = HERE / "PRE_RUN.json"
REGISTRY = CORE / "benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json"
IMAGE = CORE / "target/posix-count-diagnostic/image.json"
TARGET = CORE / "target/posix-count-host"
PRIOR = CORE / "docs/issues/232/evidence/phase2-all-ioctl/PRE_RUN.json"
BUILD = ["cargo", "+1.85.1", "build", "--manifest-path", "core/Cargo.toml",
         "--locked", "--offline", "--release", "-p", "layerfs-sdk", "-p", "layerfs-server",
         "--example", "benchmark_edit", "--example", "verify_edit"]
LABELS = {
    "1mib": "insert-middle-4k-on-1mib-ops-1-exec-v2",
    "10mib": "insert-middle-4k-on-10mib-ops-1-exec-v2",
}


def rows():
    registry = json.loads(REGISTRY.read_text())
    assert len(registry["cases"]) == 56
    return {label: next(row for row in registry["cases"]
                        if row["scenario_id"] == scenario)
            for label, scenario in LABELS.items()}


def prepare():
    identity = identities()
    assert not identity["source_dirty"] and not PRE.exists()
    assert digest(REGISTRY) == "05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823"
    build = RESULTS / "posix-count-build"
    build.mkdir(parents=True, exist_ok=False)
    with (build / "host.log").open("wb") as log:
        result = subprocess.run(BUILD, cwd=ROOT, stdout=log,
                                stderr=subprocess.STDOUT,
                                env={**os.environ, "CARGO_TARGET_DIR": str(TARGET)})
    if result.returncode:
        raise RuntimeError(f"locked host build failed; log retained at {build}")
    binaries = {}
    for name in ("benchmark_edit", "verify_edit"):
        source = TARGET / "release/examples" / name
        sha = digest(source)
        archive = RESULTS / "binary-archive" / sha / name
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            shutil.copy2(source, archive)
            archive.chmod(0o555)
        assert digest(archive) == sha
        binaries[name] = {"path": str(archive), "sha256": sha}
    # Init inputs did not change; its exact archived binary reuses the closed
    # compatible masters rather than rebuilding the fixture setup.
    binaries["benchmark_init"] = json.loads(PRIOR.read_text())["binaries"]["benchmark_init"]
    assert digest(binaries["benchmark_init"]["path"]) == binaries["benchmark_init"]["sha256"]
    IMAGE.parent.mkdir(parents=True, exist_ok=True)
    built = subprocess.run(
        ["python3", "core/benchmark/fs-bench-pro/image/build.py",
         "--scenario-version", "4", "--complexity-diagnostic", "--out", str(IMAGE)],
        cwd=ROOT, capture_output=True, text=True,
    )
    (build / "image.stdout").write_text(built.stdout)
    (build / "image.stderr").write_text(built.stderr)
    if built.returncode:
        raise RuntimeError(f"locked diagnostic image build failed; log retained at {build}")
    image = json.loads(IMAGE.read_text())
    assert image["complexity_diagnostic"] and image["product_seal"] == identity["product_seal"]
    selected = rows()
    payload = image["payloads"]["insert-middle-4k.bin"]
    assert all(row["replacement_sha256"] == payload["sha256"] for row in selected.values())
    key = cursor_key()
    source_row = json.loads((CORE / "benchmark/fs-bench-pro/registry/workspace-exec-insert-v4.json").read_text())["cases"][0]
    masters = {}
    for size in sorted({row["fixture_bytes"] for row in selected.values()}):
        master = edit_route.master(RESULTS / "prepared", size,
                                   {name: item["path"] for name, item in binaries.items()},
                                   identity, key, row=source_row)
        assert master["reuse"] == "exact-sealed-key"
        masters[str(size)] = {name: master[name] for name in
                              ("fixture_bytes", "fixture_sha256", "fixture_canonical_root",
                               "fixture_extent_count", "project_id", "genesis_layer",
                               "genesis_root", "genesis_root_serial", "store", "history",
                               "store_sha256", "history_sha256")}
    outputs = {label: str(RESULTS / "posix-count-diagnostic" / label) for label in LABELS}
    assert all(not Path(path).exists() for path in outputs.values())
    write(PRE, {"schema": "issue232-posix-count-pre-run-v1", "status": "PROSPECTIVE",
                "source_before_pre_run_commit": identity["source_commit"],
                "product_seal": identity["product_seal"],
                "harness_seal": identity["harness_seal"],
                "cargo_lock_sha256": identity["cargo_lock_sha256"],
                "registry_sha256": digest(REGISTRY), "binaries": binaries,
                "image": image, "masters": masters, "outputs": outputs,
                "cases": selected, "cursor_key_sha256": hashlib.sha256(bytes.fromhex(key)).hexdigest(),
                "one_attempt_per_case": True, "construction_workers": 1,
                "complete_command_budget_ns": 15_000_000_000,
                "verifier_budget_ns": 10_000_000_000,
                "cache_contract": "uncontrolled Linux FUSE backing; cause-finding only; cold latency INELIGIBLE",
                "clone_method": "independent-writable-byte-copy"})
    print(PRE)


def case_fields(row, master, run, label):
    scenario = f"posix-count-insert-middle-{label}-v1"
    return {"family_id": row["family_id"], "scenario_id": scenario,
            "route": "sdk-exec-fuse-posix-count-diagnostic-v1",
            "operation_contract_id": "workspace-exec-posix-count-diagnostic-v1",
            "fixture_bytes": row["fixture_bytes"], "edit_start": row["edit_start"],
            "delete_len": row["delete_len"], "replacement_len": row["replacement_len"],
            "replacement_sha256": row["replacement_sha256"],
            "final_bytes": row["final_bytes"], "final_sha256": row["final_sha256"],
            "g2_target_ms": 0, "command": row["command"],
            "project_id": master["project_id"], "genesis_layer": master["genesis_layer"],
            "genesis_root": master["genesis_root"],
            "genesis_root_serial": master["genesis_root_serial"],
            "stack_body": master["project_id"][2:34],
            "branch_body": hashlib.sha256(scenario.encode()).hexdigest()[:32],
            "fixture_canonical_root": master["fixture_canonical_root"],
            "fixture_extent_count": master["fixture_extent_count"],
            "full_file_digest": 1, "window_count": 0,
            "canonical_root_expected": "-", "canonical_count_expected": "-",
            "telemetry_run": run}


def sample(label):
    assert label in LABELS
    pre = json.loads(PRE.read_text())
    identity = identities()
    assert not identity["source_dirty"]
    for name in ("product_seal", "harness_seal", "cargo_lock_sha256"):
        assert identity[name] == pre[name]
    assert digest(REGISTRY) == pre["registry_sha256"]
    for item in pre["binaries"].values():
        assert digest(item["path"]) == item["sha256"]
    image = subprocess.check_output(
        ["docker", "image", "inspect", pre["image"]["image_id"], "--format", "{{.Id}}"],
        text=True).strip()
    assert image == pre["image"]["image_id"]
    key = cursor_key()
    assert hashlib.sha256(bytes.fromhex(key)).hexdigest() == pre["cursor_key_sha256"]
    folder = Path(pre["outputs"][label])
    with (RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        folder.mkdir(parents=True, exist_ok=False)
        try:
            _sample(folder, pre, identity, key, label)
        except Exception as error:
            write(folder / "attempt-error.json", {"status": "FAIL", "error": repr(error),
                                                  "source_commit": identity["source_commit"]})
            raise


def _sample(folder, pre, identity, key, label):
    row = pre["cases"][label]
    master = pre["masters"][str(row["fixture_bytes"])]
    for name in ("store", "history"):
        assert digest(master[name]) == master[name + "_sha256"]
        shutil.copyfile(master[name], folder / f"{name}.sqlite")
        assert digest(folder / f"{name}.sqlite") == master[name + "_sha256"]
    fields = case_fields(row, master, int.from_bytes(os.urandom(16), "big") or 1, label)
    spec = folder / "case.txt"
    spec.write_text("".join(f"{key}={value}\n" for key, value in sorted(fields.items())))
    store, history = folder / "store.sqlite", folder / "history.sqlite"
    command = [pre["binaries"]["benchmark_edit"]["path"],
               ";".join(f"{key}={value}" for key, value in sorted(fields.items())),
               str(store), str(history), pre["image"]["image_id"],
               "posix-count-" + label]
    environment = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key,
                   "LAYERFS_CONSTRUCTION_WORKERS": "1",
                   "LAYERFS_COMPLEXITY_DIAGNOSTIC": "1",
                   "LAYERFS_FINISH_DIAGNOSTIC": "1"}
    started = time.monotonic_ns()
    try:
        result = subprocess.run(command, capture_output=True, timeout=15, env=environment)
        stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    wall = time.monotonic_ns() - started
    (folder / "driver.raw.stdout").write_bytes(stdout)
    (folder / "driver.raw.stderr").write_bytes(stderr)
    lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    driver = json.loads(lines[-1]) if len(lines) == 1 else None
    verification = {"status": "NOT_RUN"}
    if code == 0 and driver and driver.get("status") == "COMPLETE":
        fields["branch_id"] = driver["branch_id"]
        fields["expected_head_commit"] = driver["head_commit"]
        spec.write_text("".join(f"{key}={value}\n" for key, value in sorted(fields.items())))
        checked = subprocess.run([pre["binaries"]["verify_edit"]["path"], str(spec),
                                  str(store), str(history)], capture_output=True,
                                 timeout=10, env=environment)
        (folder / "verifier.stdout").write_bytes(checked.stdout)
        (folder / "verifier.stderr").write_bytes(checked.stderr)
        verification = {"status": "PASS" if checked.returncode == 0 else "FAIL",
                        "exit_code": checked.returncode,
                        "result": json.loads(checked.stdout) if checked.returncode == 0 else None}
    raw_lines = stderr.decode(errors="replace").splitlines()
    counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",")) if driver else {}
    piece_counts = [line for line in raw_lines if line.startswith("LFS_PIECE_COUNT ")]
    piece_pages = [line for line in raw_lines if line.startswith("LFS_PIECE_PAGES ")]
    finish = [line for line in raw_lines if line.startswith("LFS_FINISH_SUBSTEP ")]
    expected_writes = (row["fixture_bytes"] - row["edit_start"] - row["delete_len"]) // (128 * 1024) + 1
    route_ok = (driver is not None and int(counts.get("write", 0)) >= expected_writes
                and int(counts.get("setattr", 0)) >= 1
                and counts.get("range_state") == "0" and counts.get("range_edit") == "0"
                and len(piece_counts) >= expected_writes and len(piece_pages) >= expected_writes
                and len(finish) == 1)
    receipt = {"schema": "issue232-posix-count-diagnostic-receipt-v1",
               "source_commit": identity["source_commit"],
               "product_seal": identity["product_seal"],
               "harness_seal": identity["harness_seal"],
               "label": label, "sample_count": 1, "cache_status": "INELIGIBLE",
               "performance_claim": False, "driver_exit_code": code, "driver_timeout": timed_out,
               "complete_command_wall_ns": wall, "driver": driver,
               "projection_counts": counts, "route_ok": route_ok,
               "piece_count_lines": piece_counts, "piece_page_lines": piece_pages,
               "finish_substep_lines": finish, "verification": verification,
               "raw_stdout_sha256": digest(folder / "driver.raw.stdout"),
               "raw_stderr_sha256": digest(folder / "driver.raw.stderr")}
    receipt["status"] = ("PASS" if code == 0 and not timed_out and wall <= 15_000_000_000
                         and route_ok and driver["unmount_ok"] and driver["sandbox_delete_ok"]
                         and verification["status"] == "PASS" else "FAIL")
    write(folder / "receipt.json", receipt)
    print(label, receipt["status"], counts, len(piece_counts), len(finish))


if __name__ == "__main__":
    if sys.argv[1:] == ["prepare"]:
        prepare()
    elif len(sys.argv) == 3 and sys.argv[1] == "run":
        sample(sys.argv[2])
    else:
        raise SystemExit("usage: run.py prepare | run 1mib|10mib")

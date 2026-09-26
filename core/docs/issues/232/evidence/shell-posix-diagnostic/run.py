#!/usr/bin/env python3
"""One retained mounted POSIX-write diagnostic; no performance admission."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

CORE = Path(__file__).resolve().parents[5]
ROOT = CORE.parent
sys.path.insert(0, str(CORE / "benchmark/fs-bench-pro"))
from runner import identities  # noqa: E402
from phase1c import cursor_key  # noqa: E402

PRE = Path(__file__).with_name("PRE_RUN.json")


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main():
    pre = json.loads(PRE.read_text())
    identity = identities()
    assert not identity["source_dirty"]
    for name in ("product_seal", "harness_seal", "cargo_lock_sha256"):
        assert identity[name] == pre[name], name
    for name in ("driver", "verifier"):
        assert digest(pre[name]["path"]) == pre[name]["sha256"], name
    master = pre["master"]
    for name in ("store", "history"):
        assert digest(master[name]) == master[name + "_sha256"], name
    image = subprocess.check_output(
        ["docker", "image", "inspect", pre["image_id"], "--format", "{{.Id}}"],
        text=True,
    ).strip()
    assert image == pre["image_id"]
    output = ROOT / pre["output"]
    with (output.parent / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        output.mkdir(parents=True, exist_ok=False)
        for name in ("store", "history"):
            target = output / (name + ".sqlite")
            shutil.copyfile(master[name], target)
            assert digest(target) == master[name + "_sha256"]
        scenario = pre["scenario_id"]
        fields = {
            "family_id": "edit_length_preserving",
            "scenario_id": scenario,
            "route": "sdk-exec-fuse-posix-diagnostic-v1",
            "operation_contract_id": "workspace-exec-fuse-posix-overwrite-diagnostic-v1",
            "fixture_bytes": pre["fixture_bytes"],
            "edit_start": pre["edit_start"],
            "delete_len": pre["delete_len"],
            "replacement_len": pre["replacement_len"],
            "replacement_sha256": pre["replacement_sha256"],
            "final_bytes": pre["final_bytes"],
            "final_sha256": pre["final_sha256"],
            "g2_target_ms": 0,
            "command": pre["command"],
            "project_id": master["project_id"],
            "genesis_layer": master["genesis_layer"],
            "genesis_root": master["genesis_root"],
            "genesis_root_serial": master["genesis_root_serial"],
            "stack_body": master["project_id"][2:34],
            "branch_body": hashlib.sha256(scenario.encode()).hexdigest()[:32],
            "fixture_canonical_root": master["fixture_canonical_root"],
            "fixture_extent_count": master["fixture_extent_count"],
            "full_file_digest": 1,
            "window_count": 0,
            "canonical_root_expected": "-",
            "canonical_count_expected": "-",
            "telemetry_run": int.from_bytes(os.urandom(16), "big") or 1,
        }
        spec = output / "case.txt"
        spec.write_text("".join(f"{key}={value}\n" for key, value in sorted(fields.items())))
        command = [pre["driver"]["path"],
                   ";".join(f"{key}={value}" for key, value in sorted(fields.items())),
                   str(output / "store.sqlite"), str(output / "history.sqlite"),
                   image, "shell-posix-" + hashlib.sha256(scenario.encode()).hexdigest()[:20]]
        environment = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": cursor_key(),
                       "LAYERFS_CONSTRUCTION_WORKERS": "1"}
        started = time.monotonic_ns()
        try:
            result = subprocess.run(command, capture_output=True,
                                    timeout=pre["complete_command_timeout_seconds"],
                                    env=environment)
            stdout, stderr, code, timed_out = result.stdout, result.stderr, result.returncode, False
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, timed_out = error.stdout or b"", error.stderr or b"", None, True
        wall = time.monotonic_ns() - started
        (output / "driver.raw.stdout").write_bytes(stdout)
        (output / "driver.raw.stderr").write_bytes(stderr)
        lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
        driver = json.loads(lines[-1]) if len(lines) == 1 else None
        verification = {"status": "NOT_RUN"}
        if code == 0 and driver and driver.get("status") == "COMPLETE":
            fields["branch_id"] = driver["branch_id"]
            fields["expected_head_commit"] = driver["head_commit"]
            spec.write_text("".join(f"{key}={value}\n" for key, value in sorted(fields.items())))
            checked = subprocess.run(
                [pre["verifier"]["path"], str(spec), str(output / "store.sqlite"),
                 str(output / "history.sqlite")], capture_output=True,
                timeout=pre["verifier_timeout_seconds"], env=environment,
            )
            (output / "verifier.stdout").write_bytes(checked.stdout)
            (output / "verifier.stderr").write_bytes(checked.stderr)
            verification = {"status": "PASS" if checked.returncode == 0 else "FAIL",
                            "exit_code": checked.returncode,
                            "result": json.loads(checked.stdout) if checked.returncode == 0 else None}
        counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",")) if driver else {}
        route_ok = (driver is not None and int(counts.get("write", -1)) >= 1
                    and counts.get("range_state") == "0" and counts.get("range_edit") == "0"
                    and driver["range_accepted_payload_bytes"] == 0
                    and driver["range_shifted_suffix_bytes"] == 0)
        receipt = {"schema": "issue232-arbitrary-shell-posix-diagnostic-result-v1",
                   "source_commit": identity["source_commit"], "source_tree": identity["source_tree"],
                   "product_seal": identity["product_seal"], "harness_seal": identity["harness_seal"],
                   "sample_count": 1, "complete_command_wall_ns": wall,
                   "driver_exit_code": code, "driver_timeout": timed_out,
                   "driver": driver, "route_ok": route_ok, "verification": verification,
                   "cache_status": "INELIGIBLE", "performance_claim": False,
                   "raw_stdout_sha256": digest(output / "driver.raw.stdout"),
                   "raw_stderr_sha256": digest(output / "driver.raw.stderr")}
        receipt["status"] = ("PASS" if code == 0 and not timed_out and route_ok
                             and verification["status"] == "PASS" else "FAIL")
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
        print(json.dumps({"output": str(output), "status": receipt["status"],
                          "counts": counts, "verification": verification["status"]}, sort_keys=True))
        if receipt["status"] != "PASS":
            raise SystemExit(1)


if __name__ == "__main__":
    main()

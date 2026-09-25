#!/usr/bin/env python3
"""Read retained #243 raw rows; never run a product command or change raw evidence."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CASES = ("mixed-refresh-v1", "overwrite-4k-v1", "repeated-one-byte-v1",
         "failed-command-no-commit-v1")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def events(raw):
    result = []
    for line in raw.splitlines():
        if line.startswith(b"LFT1 "):
            result.append(json.loads(line[5:]))
    return result


def derive():
    rows = []
    for name in CASES:
        folder = ROOT / name
        receipt = json.loads((folder / "receipt.json").read_text())
        stdout = (folder / "driver.stdout").read_bytes()
        stderr = (folder / "driver.stderr").read_bytes()
        parsed = [json.loads(line[8:]) for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
        assert len(parsed) == 1 and parsed[0] == receipt["driver"]
        data = events(stderr)
        host = [entry for entry in data if entry.get("kind") == "operation" and entry.get("role") == 1
                and entry.get("key") == 243000]
        assert len(host) == 1 and host[0] == receipt["lft1"]["operation"]
        daemon = [entry for entry in data if entry.get("kind") == "operation" and entry.get("role") == 2
                  and entry.get("timing", {}).get("name") in ("WorkspaceExec", "WorkspaceCommit")]
        assert len([entry for entry in daemon if entry["timing"]["name"] == "WorkspaceExec"]) == 1
        assert len([entry for entry in daemon if entry["timing"]["name"] == "WorkspaceCommit"]) == int(receipt["driver"]["commit_called"])
        counts = {item.split("=", 1)[0]: int(item.split("=", 1)[1])
                  for item in receipt["driver"]["projection_counts"].split(",")}
        assert counts["range_state"] == counts["range_edit"] == 0
        row = {"scenario_id": name, "row_status": receipt["row_status"],
               "functional_status": receipt["functional_status"],
               "complete_command_wall_ns": receipt["complete_command_wall_ns"],
               "exec_ns": receipt["driver"]["exec_ns"], "commit_ns": receipt["driver"]["commit_ns"],
               "verifier": receipt["verifier"], "cleanup_status": receipt["cleanup_status"],
               "driver_exit_code": receipt["driver_exit_code"],
               "driver_detail": receipt["driver"]["detail"],
               "callback_status_counts": counts,
               "host_operation_lft1": host[0], "daemon_operations_lft1": daemon,
               "piece_count_lines": stderr.count(b"LFS_PIECE_COUNT v=1"),
               "piece_page_lines": stderr.count(b"LFS_PIECE_PAGES v=1"),
               "finish_substep_lines": stderr.count(b"LFS_FINISH_SUBSTEP v=1"),
               "raw_sha256": {file: digest(folder / file) for file in ("receipt.json", "driver.stdout", "driver.stderr")}}
        rows.append(row)
    return {"schema": "issue243-shell-derived-v1", "generator_sha256": digest(Path(__file__)),
            "case_order": list(CASES), "rows": rows}


if __name__ == "__main__":
    (ROOT / "DERIVED.json").write_text(json.dumps(derive(), indent=2, sort_keys=True) + "\n")

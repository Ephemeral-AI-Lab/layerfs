#!/usr/bin/env python3
"""Derive the two cause-finding rows from retained raw output and seals."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRE = json.loads((HERE / "PRE_RUN.json").read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fields(line):
    return dict(part.split("=", 1) for part in line.split()[1:])


def derive(label):
    folder = HERE / "attempt-01" / label
    for entry in (folder / "SHA256SUMS").read_text().splitlines():
        expected, name = entry.split("  ", 1)
        assert digest(folder / name) == expected
    receipt = json.loads((folder / "receipt.json").read_text())
    assert receipt["status"] == "PASS" and receipt["sample_count"] == 1
    assert receipt["cache_status"] == "INELIGIBLE" and not receipt["performance_claim"]
    for key in ("product_seal", "harness_seal"):
        assert receipt[key] == PRE[key]
    assert digest(folder / "driver.raw.stdout") == receipt["raw_stdout_sha256"]
    assert digest(folder / "driver.raw.stderr") == receipt["raw_stderr_sha256"]
    driver = receipt["driver"]
    assert driver["status"] == "COMPLETE" and driver["operation_contract_id"] == "workspace-exec-posix-count-diagnostic-v1"
    assert PRE["cases"][label]["command"] in (folder / "case.txt").read_text()
    assert driver["unmount_ok"] and driver["sandbox_delete_ok"]
    verification = receipt["verification"]
    assert verification["status"] == "PASS"
    checked = verification["result"]
    assert checked["status"] == "PASS" and checked["full_file_bytes_verified"]
    assert checked["historical_root_match"] and checked["content_match"]
    assert checked["observed_digest"] == PRE["cases"][label]["final_sha256"]
    counts = {key: int(value) for key, value in receipt["projection_counts"].items()}
    assert counts["range_state"] == counts["range_edit"] == 0
    assert counts["write"] > 0 and counts["setattr"] == 1
    pieces = [fields(line) for line in receipt["piece_count_lines"]]
    pages = [fields(line) for line in receipt["piece_page_lines"]]
    assert len(pieces) == len(pages) == counts["write"] + counts["setattr"]
    for key in ("old", "new", "prefix_visits", "suffix_visits", "replacement_visits", "summary_visits"):
        assert all(int(item[key]) >= 0 for item in pieces)
    finish = [fields(line) for line in receipt["finish_substep_lines"]]
    assert len(finish) == 1 and finish[0]["scope"] == "save-wide"
    finish = {key: int(value) if key not in ("v", "scope") else value
              for key, value in finish[0].items()}
    all_in_final_drain = finish["finish_batch_objects"] == finish["inserted"] + finish["reused"]
    stderr = (folder / "driver.raw.stderr").read_text(errors="replace")
    lft = [json.loads(line[5:]) for line in stderr.splitlines() if line.startswith("LFT1 ")]
    roots = [item for item in lft if item.get("namespace") == 7 and item.get("role") == 1
             and item.get("kind") == "operation" and item.get("key") == 232000]
    summaries = [item for item in lft if item.get("namespace") == 7 and item.get("role") == 1
                 and item.get("kind") == "run-summary"]
    assert len(roots) == len(summaries) == 1 and roots[0]["success"]
    assert summaries[0]["dropped"] == 0 and not summaries[0]["overflow"]
    return {
        "status": "INELIGIBLE", "functional": "PASS", "verifier": "PASS",
        "source_commit": receipt["source_commit"],
        "complete_command_wall_ns": receipt["complete_command_wall_ns"],
        "lft1_edit_commit_ns": roots[0]["timing"]["elapsed_ns"],
        "fixture_bytes": PRE["cases"][label]["fixture_bytes"],
        "shifted_suffix_bytes": PRE["cases"][label]["fixture_bytes"]
            - PRE["cases"][label]["edit_start"] - PRE["cases"][label]["delete_len"],
        "projection": counts,
        "piece_events": len(pieces), "piece_pages_written": sum(int(item["written"]) for item in pages),
        "last_piece_count": int(pieces[-1]["new"]),
        "piece_totals": {key: sum(int(item[key]) for item in pieces)
                         for key in ("old", "new", "prefix_visits", "suffix_visits",
                                     "replacement_visits", "summary_visits")},
        "finish_substeps": finish,
        "save_wide_totals_equal_final_drain": all_in_final_drain,
        "full_file_sha256": checked["observed_digest"],
        "cleanup": "PASS",
    }


def main():
    rows = {label: derive(label) for label in ("1mib", "10mib")}
    assert rows["1mib"]["projection"]["read"] == 8
    assert rows["1mib"]["projection"]["write"] == 9
    assert rows["10mib"]["projection"]["read"] == 80
    assert rows["10mib"]["projection"]["write"] == 81
    assert rows["1mib"]["save_wide_totals_equal_final_drain"]
    assert not rows["10mib"]["save_wide_totals_equal_final_drain"]
    print(json.dumps({"schema": "issue232-posix-count-derived-v1", "rows": rows,
                      "claim": "functional/count diagnostic only; no cold speed comparison"},
                     indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

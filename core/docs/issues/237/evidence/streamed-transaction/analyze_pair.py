"""Reproduce the one-shot streamed-transaction decision from retained sidecars."""

import json
from pathlib import Path
import re


ROOT = Path(__file__).parent / "pair"
FIELDS = (
    "inserted", "reused", "packs_created", "pack_appends", "commits",
    "statements", "presence_queries", "sql_ns", "commit_ns",
    "insert_objects_ns", "accept_plumbing_ns", "finish_drop_ns",
    "stream_segments", "stream_lock_ns", "stream_lock_max_ns",
    "stream_transaction_rows_max", "stream_transaction_bytes_max",
)


def arm(name):
    path = ROOT / name
    receipt = json.loads((path / "receipt.json").read_text())
    geometry = json.loads((path / "geometry.json").read_text())
    readback = json.loads((path / "readback.json").read_text())
    cold = json.loads((path / "cold-recheck.json").read_text())
    log = path / "service.stderr"
    if not log.exists():
        log = path / "service-diagnostics-from-receipt.txt"
    file_line = next(line for line in log.read_text().splitlines()
                     if line.startswith("LFS237_SAVE file "))
    save = {}
    for key in FIELDS:
        match = re.search(r"\b" + key + r": (\d+)", file_line)
        save[key] = int(match.group(1)) if match else None
    return {
        "status": receipt["status"],
        "raw_operation_ns": receipt["raw_operation_ns"],
        "complete_command_ns": receipt["performance_command_wall_ns"],
        "verification": receipt["verification"],
        "cleanup": receipt["cleanup"],
        "competing_work": receipt["competing_work"],
        "source_payload_resident_pages": cold["resident_pages"],
        "recheck_to_timer_ns": json.loads((path / "cold-launch.json").read_text())["recheck_to_timer_ns"],
        "service_sampled_max_rss_bytes": receipt["telemetry"]["m2_local_roots"]["service"][0]["sampled_max_rss"],
        "service_file_loop_ns": next(row["elapsed_ns"] for row in receipt["telemetry"]["m3_service_children"]
                                     if row["name"] == "history.import_files"),
        "lifecycle_user_cpu_s": receipt["external_resources"]["user_cpu_s"],
        "lifecycle_system_cpu_s": receipt["external_resources"]["system_cpu_s"],
        "file_save": save,
        "geometry": geometry,
        "readback_status": readback["status"],
        "readback": json.loads(readback["stdout"]),
        "root": json.loads((path / "perf.jsonl").read_text())["root"],
    }


def main():
    control, candidate = arm("control"), arm("candidate")
    assert control["source_payload_resident_pages"] == candidate["source_payload_resident_pages"] == 0
    assert control["geometry"]["sqlite_page_size"] == candidate["geometry"]["sqlite_page_size"] == 4096
    assert control["root"] == candidate["root"]
    assert control["geometry"]["ordered_object_ids_sha256"] == candidate["geometry"]["ordered_object_ids_sha256"]
    assert control["readback_status"] == candidate["readback_status"] == "PASS"
    assert control["readback"]["files"] == candidate["readback"]["files"] == 10000
    g = lambda key: candidate["geometry"][key] - control["geometry"][key]
    rss_growth = candidate["service_sampled_max_rss_bytes"] - control["service_sampled_max_rss_bytes"]
    gates = {
        "commits_at_most_9": candidate["file_save"]["commits"] <= 9,
        "transaction_rows_at_most_8191": candidate["file_save"]["stream_transaction_rows_max"] <= 8191,
        "transaction_bytes_at_most_128_mib": candidate["file_save"]["stream_transaction_bytes_max"] <= 128 * 1024 * 1024,
        "raw_public_no_slower": candidate["raw_operation_ns"] <= control["raw_operation_ns"],
        "sampled_service_rss_growth_at_most_16_mib": rss_growth <= 16 * 1024 * 1024,
        "store_apparent_growth_at_most_1_mib": g("file_bytes") <= 1024 * 1024,
        "pack_capacity_growth_at_most_1_mib": g("pack_capacity_bytes") <= 1024 * 1024,
        "longest_lock_at_most_200_ms": candidate["file_save"]["stream_lock_max_ns"] <= 200_000_000,
        "exact_root_ids_full_readback": True,
    }
    return {
        "schema": "issue237-streamtxn-decision-v1",
        "control": control,
        "candidate": candidate,
        "delta": {
            "raw_operation_ns": candidate["raw_operation_ns"] - control["raw_operation_ns"],
            "file_save_commits": candidate["file_save"]["commits"] - control["file_save"]["commits"],
            "file_save_commit_ns": candidate["file_save"]["commit_ns"] - control["file_save"]["commit_ns"],
            "file_save_sql_ns": candidate["file_save"]["sql_ns"] - control["file_save"]["sql_ns"],
            "sampled_service_rss_bytes": rss_growth,
            "store_apparent_bytes": g("file_bytes"),
            "pack_capacity_bytes": g("pack_capacity_bytes"),
            "pack_spare_bytes": g("pack_spare_bytes"),
        },
        "gates": gates,
        "go": all(gates.values()),
        "admission_eligible": False,
        "reason": "one-shot exploratory pair; control telemetry incomplete and metadata cache unqualified",
    }


if __name__ == "__main__":
    print(json.dumps(main(), indent=2, sort_keys=True))

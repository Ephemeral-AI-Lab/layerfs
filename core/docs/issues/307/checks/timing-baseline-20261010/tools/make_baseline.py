#!/usr/bin/env python3
"""Build the timing baseline record from the twelve class-B arm-L sample receipts.

Reads each sample's receipt.json as written by the R7 runner and copies its
values; nothing is measured, averaged or selected here. A value a receipt does
not supply is written as UNAVAILABLE, never as zero.

Usage: make_baseline.py <first-receipt-number> <output.json> <output.md>
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[7] if "core/docs" in str(Path(__file__).resolve()) else Path.cwd()
CHECKS = ROOT / "core/docs/issues/307/checks/r7-optimization-20261009"
CELLS = ["C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10", "C11", "C12"]
COMMANDS = {
    "C01": "create 1000 files", "C02": "create 1000, stat 1000", "C03": "create 1000, remove them",
    "C04": "100 directories, 1000 files", "C05": "that tree, then `find`", "C06": "write 64 MiB",
    "C07": "write 64 MiB, copy it", "C08": "write 64 MiB, read it", "C09": "read a 64 MiB base file",
    "C10": "copy a 64 MiB base file", "C11": "overwrite a 64 MiB base file",
    "C12": "`git init`, 100 files, add, commit"}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def span(phases, key):
    entry = phases.get(key) or {}
    return entry["value"]["duration_ns"] if entry.get("status") == "AVAILABLE" else None


def observed(row, name):
    entry = row["observations"].get(name) or {}
    return entry["value"] if entry.get("status") == "AVAILABLE" else None


def regime(row):
    threads = (row.get("observations") or {}).get("daemon_threads") or {}
    if threads.get("status") != "AVAILABLE":
        return None
    receive = [t for t in threads["value"]["threads"] if t["comm"] == "fuser-0"]
    if not receive:
        return None
    voluntary, involuntary = receive[0]["voluntary_switches"], receive[0]["nonvoluntary_switches"]
    share = involuntary / (voluntary + involuntary) if voluntary + involuntary else 0
    return "SEPARATE" if share < 0.02 else "STACKED" if share > 0.90 else "MIXED"


def one(cell, directory):
    path = directory / "sample" / "receipt.json"
    if not path.is_file():
        outer = json.loads((directory / "result.json").read_text()) if (directory / "result.json").is_file() else {}
        return dict(cell=cell, selection_id=cell + ":B:L", receipt=None, row_status="NO_RECEIPT",
                    wrapper_status=outer.get("status"), wrapper_exit=outer.get("exit_code"))
    row = json.loads(path.read_text())
    phases = row.get("phases") or {}
    counted = next((p for p in row.get("phase_counts") or [] if p.get("status") == "AVAILABLE" and "raw" in p), None)
    requests = statements = jobs = None
    if counted:
        if "opcodes" in counted["raw"]:
            requests = sum(counted["raw"]["opcodes"]["by_opcode"])
        statements = sum(r["executions"] for r in counted["adjusted"]["sql_foreground"])
        jobs = counted["adjusted"]["owner"]["admitted"]
    cache = row.get("cache") or {}
    return dict(
        cell=cell, selection_id=row["selection_id"], command=COMMANDS[cell],
        receipt=str(path.relative_to(ROOT)), receipt_sha256=sha(path),
        source_commit=(row.get("identities") or {}).get("source_commit"),
        row_status=row["row_status"], reason=row.get("reason"),
        verifier=row.get("verification_status"), custody=row.get("custody_status"),
        cleanup=row.get("cleanup_status"), completion_gaps=row.get("completion_gaps"),
        attempted=row["attempted_operation_count"], completed=row.get("completed_operation_count"),
        mount_ns=span(phases, "mount"), command_ns=span(phases, "command"),
        command_in_container_ns=span(phases, "command_in_container"),
        unmount_ns=span(phases, "unmount"), cleanup_ns=span(phases, "cleanup"), verifier_ns=span(phases, "verifier"),
        complete_command_ns=row.get("complete_command_ns"), complete_command_wall_stop_ns=row.get("complete_command_wall_stop_ns"),
        regime=regime(row), cache_class=cache.get("class"), class_b_object_demands=cache.get("object_demands"),
        requests=requests, owner_jobs=jobs, statements=statements,
        store_logical_bytes=observed(row, "store_logical_bytes"), store_allocated_bytes=observed(row, "store_allocated_bytes"),
        overlay_logical_bytes=observed(row, "overlay_logical_bytes"), overlay_allocated_bytes=observed(row, "overlay_allocated_bytes"),
        daemon_vmhwm_bytes=observed(row, "daemon_vmhwm_bytes"), admission_eligible=row.get("admission_eligible"))


def ms(value):
    return "UNAVAILABLE" if value is None else f"{value / 1e6:.1f}"


def main():
    first, out_json, out_md = int(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    commit = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--short=9", "HEAD"], text=True).strip()
    rows, number = [], first + 4
    for cell in CELLS:
        number += 3
        matches = sorted(CHECKS.glob(f"*-{cell}-B-L-sample-{commit}"))
        assert len(matches) == 1, (cell, matches)
        rows.append(one(cell, matches[0]))
    record = dict(schema="layerfs-timing-baseline-v1", source_commit=commit, selections=rows)
    out_json.write_text(json.dumps(record, indent=1, sort_keys=True) + "\n")
    lines = ["| Cell | Command | Command ms, in-container clock | Command ms, host clock | Mount ms | Unmount ms | Requests | Owner jobs | Statements | Overlay logical bytes | Store logical bytes | Regime | Verifier / custody / cleanup |",
             "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"]
    for r in rows:
        if r["row_status"] == "NO_RECEIPT":
            lines.append(f"| {r['cell']} | {COMMANDS[r['cell']]} | NO RECEIPT (wrapper {r['wrapper_status']}, exit {r['wrapper_exit']}) | | | | | | | | | | |")
            continue
        show = lambda v: "UNAVAILABLE" if v is None else str(v)
        lines.append(f"| {r['cell']} | {r['command']} | {ms(r['command_in_container_ns'])} | {ms(r['command_ns'])} | {ms(r['mount_ns'])} | {ms(r['unmount_ns'])} | "
                     f"{show(r['requests'])} | {show(r['owner_jobs'])} | {show(r['statements'])} | {show(r['overlay_logical_bytes'])} | {show(r['store_logical_bytes'])} | "
                     f"{show(r['regime'])} | {r['verifier']} / {r['custody']} / {r['cleanup']} |")
    out_md.write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Reproduce the bounded #241 failure-probe observations from retained raw files."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).parent / "attempts"
request = bytearray(4128)
request[:4] = b"LFR1"
request[4:6] = (1).to_bytes(2, "little")
request[8:16] = (4093).to_bytes(8, "little")
request[24:28] = (4096).to_bytes(4, "little")
request[32:] = bytes(i % 239 for i in range(4096))
accepted = bytearray(i % 251 for i in range(8192))
accepted[4093:4093] = request[32:]
expected_cases = {
    "ro_mount": (0, 1),
    "closed_fd": (0, 0),
    "lost_reply": (101, 1),
    "notifier_after_unmount": (101, 0),
    "lost_reply_state": (0, 1),
}
assert {p.name for p in root.iterdir() if p.is_dir()} == set(expected_cases)
rows = []
for case, (exit_code, callbacks) in expected_cases.items():
    path = root / case
    receipt = json.loads((path / "receipt.json").read_text())
    events = (path / "events.log").read_text().splitlines()
    requests = [line for line in events if line.startswith("ioctl ino=")]
    assert receipt["exit_code"] == exit_code
    assert receipt["timed_out"] is False and receipt["docker_cp_exit_code"] == 0
    assert len(requests) == callbacks
    for line in requests:
        assert "ino=2 fh=40 " in line
        assert "cmd=0x5020f541 in=4128 out=0 " in line
        assert bytes.fromhex(line.split("hex=", 1)[1]) == request
    if case == "ro_mount":
        assert " ro," in (path / "mountinfo").read_text()
        assert any(line.startswith("refused readonly mount before publication") for line in events)
        assert "caller ioctl errno=Some(30)" in (path / "stdout").read_text()
    if case == "closed_fd":
        assert "release ino=2 fh=40" in events
        assert "errno=Some(9)" in (path / "stdout").read_text()
    if case.startswith("lost_reply"):
        assert "errno=Some(103)" in (path / "stdout").read_text()
    if case == "notifier_after_unmount":
        assert events[-1] == "real notifier after unmount: Ok(())"
    if case == "lost_reply_state":
        assert (path / "accepted-state.bin").read_bytes() == accepted
    rows.append({
        "case": case,
        "exit_code": exit_code,
        "ioctl_callbacks": callbacks,
        "complete_command_wall_ns": receipt["complete_command_wall_ns"],
        "events_sha256": hashlib.sha256((path / "events.log").read_bytes()).hexdigest(),
    })
print(json.dumps(rows, indent=2))

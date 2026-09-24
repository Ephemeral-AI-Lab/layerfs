#!/usr/bin/env python3
"""Reproduce the #241 caller-ack observations from retained raw attempts."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).parent / "attempts"
cases = ("ack_success", "ack_suppressed", "ack_lost_reply")
assert {p.name for p in root.iterdir() if p.is_dir()} == set(cases)
request = bytearray(4128)
request[:4] = b"LFR1"
request[4:6] = (1).to_bytes(2, "little")
request[8:16] = (4093).to_bytes(8, "little")
request[24:28] = (4096).to_bytes(4, "little")
request[32:] = bytes(i % 239 for i in range(4096))
accepted = bytearray(i % 251 for i in range(8192))
accepted[4093:4093] = request[32:]
rows = []
for case in cases:
    path = root / case
    receipt = json.loads((path / "receipt.json").read_text())
    events = (path / "events.log").read_text().splitlines()
    caller = (path / "caller.log").read_text()
    ioctl = [line for line in events if line.startswith("ioctl ino=")]
    assert receipt["case"] == case and receipt["exit_code"] == 0
    assert receipt["timed_out"] is False and receipt["docker_cp_exit_code"] == 0
    assert len(ioctl) == 1
    assert "ino=2 fh=40 " in ioctl[0]
    assert "cmd=0x5020f541 in=4128 out=0 " in ioctl[0]
    assert bytes.fromhex(ioctl[0].split("hex=", 1)[1]) == request
    assert "initial_size=8192 initial_mtime=1700000000" in caller
    assert "ioctl_calls=1 retry_calls=0" in caller
    assert " rw," in (path / "mountinfo").read_text()
    if case == "ack_success":
        assert "real inval_inode result=Ok(())" in events
        assert "ioctl=Ok(()) errno=None" in caller
        assert "fstat=Ok((12288, 1700000001))" in caller
        assert 'boundary=Ok("4a4b4c000102030405060708090a0b0c")' in caller
        assert 'eof=Ok("")' in caller
        assert "decision=SUCCESS" in caller
    elif case == "ack_suppressed":
        assert "test-only notification suppression" in events
        assert "ioctl=Ok(()) errno=None" in caller
        assert "fstat=Ok((8192, 1700000000))" in caller
        assert "decision=UNCERTAIN" in caller
    else:
        assert "errno=Some(103)" in caller
        assert "decision=UNCERTAIN" in caller
        assert "accepted artifact written; daemon exit 23 before reply" in events
        assert (path / "accepted-state.bin").read_bytes() == accepted
    rows.append({
        "case": case,
        "complete_command_wall_ns": receipt["complete_command_wall_ns"],
        "ioctl_callbacks": len(ioctl),
        "decision": "SUCCESS" if case == "ack_success" else "UNCERTAIN",
        "events_sha256": hashlib.sha256((path / "events.log").read_bytes()).hexdigest(),
    })
print(json.dumps(rows, indent=2))

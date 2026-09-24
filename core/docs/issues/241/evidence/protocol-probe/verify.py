#!/usr/bin/env python3
"""Reproduce exact-size #241 STATE/EDIT observations from retained raw files."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).parent / "attempts"
cases = ("state_exact", "edit_state", "stale_refusal", "suppressed_unknown", "lost_reply_unknown")
assert {p.name for p in root.iterdir() if p.is_dir()} == set(cases)

def state_request():
    b = bytearray(88)
    b[:4] = b"LFS2"
    b[4:6] = (2).to_bytes(2, "little")
    b[6:8] = (1).to_bytes(2, "little")
    return bytes(b)

def edit_request(revision):
    b = bytearray(4192)
    b[:4] = b"LFE2"
    b[4:6] = (2).to_bytes(2, "little")
    b[8:16] = (2).to_bytes(8, "little")
    b[16:48] = bytes([0x24]) * 32
    b[48:56] = (7).to_bytes(8, "little")
    b[56:64] = revision.to_bytes(8, "little")
    b[64:72] = (4093).to_bytes(8, "little")
    b[80:84] = (4096).to_bytes(4, "little")
    b[96:] = bytes(i % 239 for i in range(4096))
    return bytes(b)

def state_reply(revision, size, mtime):
    b = bytearray(88)
    b[:4] = b"LFS2"
    b[4:6] = (2).to_bytes(2, "little")
    b[8:16] = (2).to_bytes(8, "little")
    b[16:48] = bytes([0x24]) * 32
    b[48:56] = (7).to_bytes(8, "little")
    b[56:64] = revision.to_bytes(8, "little")
    b[64:72] = size.to_bytes(8, "little")
    b[72:80] = mtime.to_bytes(8, "little", signed=True)
    return bytes(b)

before = state_reply(3, 8192, 1700000000)
after = state_reply(4, 12288, 1700000001)
accepted = bytearray(i % 251 for i in range(8192))
accepted[4093:4093] = bytes(i % 239 for i in range(4096))
sequence = {
    "state_exact": [(0xc058f540, state_request())],
    "edit_state": [(0xc058f540, state_request()), (0x5060f541, edit_request(3)), (0xc058f540, state_request())],
    "stale_refusal": [(0xc058f540, state_request()), (0x5060f541, edit_request(2)), (0xc058f540, state_request())],
    "suppressed_unknown": [(0xc058f540, state_request()), (0x5060f541, edit_request(3)), (0xc058f540, state_request())],
    "lost_reply_unknown": [(0xc058f540, state_request()), (0x5060f541, edit_request(3))],
}
pubs = {"state_exact": 0, "edit_state": 1, "stale_refusal": 0, "suppressed_unknown": 1, "lost_reply_unknown": 1}
decisions = {"state_exact": "STATE_ONLY", "edit_state": "SUCCESS", "stale_refusal": "DEFINITE_REFUSAL", "suppressed_unknown": "UNKNOWN", "lost_reply_unknown": "UNKNOWN"}
rows = []
for case in cases:
    path = root / case
    receipt = json.loads((path / "receipt.json").read_text())
    events = (path / "events.log").read_text().splitlines()
    caller = (path / "caller.log").read_text()
    ioctl = [x for x in events if x.startswith("ioctl ino=")]
    assert receipt["case"] == case and receipt["exit_code"] == 0 and not receipt["timed_out"]
    assert receipt["docker_cp_exit_code"] == 0 and len(ioctl) == len(sequence[case])
    assert " rw," in (path / "mountinfo").read_text()
    for line, (cmd, request) in zip(ioctl, sequence[case]):
        assert "ino=2 fh=40 " in line
        assert f"cmd={cmd:#x} in={len(request)} out={88 if cmd == 0xc058f540 else 0} " in line
        assert bytes.fromhex(line.split("hex=", 1)[1]) == request
    assert sum(x.startswith("published ") for x in events) == pubs[case]
    assert f"decision={decisions[case]} " in caller and "retry_calls=0" in caller
    assert f"output_hex={before.hex()}" in caller
    if case in ("edit_state", "suppressed_unknown"):
        assert f"output_hex={after.hex()}" in caller
        assert "return=edit rc=0 output_len=0 output_hex= input_unchanged=true" in caller
    if case == "edit_state":
        assert "real inval_inode result=Ok(())" in events
        assert "readback size=12288 mtime=1700000001" in caller
    if case == "stale_refusal":
        assert "errno=Some(116)" in caller and "reply errno=Errno(116) reason=revision" in events
    if case == "suppressed_unknown":
        assert "test-only notification suppression" in events
        assert "readback size=8192 mtime=1700000000" in caller
    if case == "lost_reply_unknown":
        assert "errno=Some(103)" in caller
        assert (path / "accepted-state.bin").read_bytes() == accepted
    rows.append({"case": case, "ioctl_callbacks": len(ioctl), "publications": pubs[case],
                 "decision": decisions[case], "complete_command_wall_ns": receipt["complete_command_wall_ns"],
                 "events_sha256": hashlib.sha256((path / "events.log").read_bytes()).hexdigest()})
print(json.dumps(rows, indent=2))

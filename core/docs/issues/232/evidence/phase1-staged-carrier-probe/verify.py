#!/usr/bin/env python3
"""Check retained carrier observations; no live FUSE run or resampling."""
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent


def case(attempt, name):
    folder = ROOT / f"attempt-{attempt}" / name
    return json.loads((folder / "receipt.json").read_text()), (folder / "events.log").read_text()


def custody(attempt):
    folder = ROOT / f"attempt-{attempt}"
    for line in (folder / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        assert hashlib.sha256((folder / name).read_bytes()).hexdigest() == digest, name
    cleanup = json.loads((folder / "cleanup.json").read_text())
    assert cleanup["post_cleanup_inspect_exit_codes"] == {"container": 1, "volume": 1}


for attempt in ("01", "02", "03"):
    custody(attempt)

names = ("frames", "success", "wrong_order", "bad_digest", "stale_stamp",
         "close", "abort", "lost_reply")
for name in names:
    receipt, events = case("01", name)
    assert receipt["exit_code"] == 0 and not receipt["timed_out"], name
    assert receipt["docker_cp_exit"] == 0, name
    assert receipt["source_commit"].startswith("d75d4a9a"), name

_, frames = case("01", "frames")
assert [int(n) for n in re.findall(r"callback .* in=(\d+) out=0", frames)] == [4096, 8192, 12288, 16368]
assert frames.count("flags=IoctlFlags(0x0)") == 4
assert frames.count("reply frame rc=0 bytes=0") == 4

_, success = case("01", "success")
assert success.count("callback cmd=0x5080f543") == 16
assert success.count("in=4224 out=0") == 16
assert success.count("reply DATA rc=0") == 16
assert success.count("published revision=1 publications=1") == 1
assert "final revision=1 publications=1 stage_present=false unmounted=true" in success

for name, errno in (("wrong_order", 22), ("bad_digest", 22),
                    ("stale_stamp", 116), ("close", 9), ("abort", 9)):
    _, events = case("01", name)
    assert f"reply errno=Errno({errno})" in events, name
    assert "final revision=0 publications=0 stage_present=false unmounted=true" in events, name
assert "release fh=40 stage=discarded" in case("01", "close")[1]
assert "reply ABORT rc=0" in case("01", "abort")[1]

_, lost = case("01", "lost_reply")
assert lost.count("published revision=1 publications=1") == 1
assert "caller APPLY lost cmd=0x4080f544 rc=-1 errno=Some(103)" in lost
assert (ROOT / "attempt-01/lost_reply/accepted.sha256").exists()

first, _ = case("01", "unmount")
second, _ = case("02", "unmount")
third, unmount = case("03", "unmount")
assert first["exit_code"] == 101 and not first["timed_out"]
assert second["timed_out"] and second["exit_code"] is None
assert third["exit_code"] == 0 and not third["timed_out"] and third["docker_cp_exit"] == 0
assert third["source_commit"].startswith("09489a48")
assert unmount.index("unmount stage=discarded before descriptor close") < unmount.index("caller observed stage cleanup before descriptor close")
assert "final revision=0 publications=0 stage_present=false unmounted=true" in unmount
assert (ROOT / "attempt-03/unmount/mountinfo").read_text().find("fuse layerfs-stage-probe") >= 0

result = {"carrier_verdict": "GO_TEST_ONLY", "frame_lengths": [4096, 8192, 12288, 16368],
          "data_callbacks_for_64k": 16, "passed_initial_cases": list(names),
          "unmount_attempts": ["FAIL_EBUSY", "FAIL_TIMEOUT", "PASS_SOURCE_CHANGED"],
          "product_or_sdk_proof": False, "performance_claim": False}
(ROOT / "derived.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
print(json.dumps(result, sort_keys=True))

#!/usr/bin/env python3
"""Reproduce the bounded #241 public-SDK gate result from retained output."""
import json
from pathlib import Path

root = Path(__file__).parent
receipt = json.loads((root / "attempt-001/receipt.json").read_text())
identity = json.loads((root / "identity.json").read_text())
stdout = (root / "attempt-001/stdout").read_text()
stderr = (root / "attempt-001/stderr").read_text()
assert receipt["exit_code"] == 0 and receipt["timed_out"] is False
assert receipt["source_commit"] == identity["source_commit"]
assert receipt["environment_overrides"]["LAYERFS_TEST_IMAGE"] == identity["image_id"]
tool_line = next(line.removeprefix("SDK_RANGE_TOOL ") for line in stdout.splitlines()
                 if line.startswith("SDK_RANGE_TOOL "))
tool = json.loads(tool_line)
assert tool["status"] == "PASS" and tool["operation"] == "splice"
assert tool["final_bytes"] == 12288 and tool["shifted_bytes"] == 0
assert "SDK_RANGE_COMMIT Committed(" in stdout
assert "SDK_RANGE_STATUS mounted=true state=2 edit=1 accepted=4096 shifted=0" in stdout
assert "SDK_RANGE_UNKNOWN exit=Some(75) commit=omitted" in stdout
assert "test result: ok. 1 passed; 0 failed" in stdout
assert "error:" not in stderr
print(json.dumps({"result": "PASS", "image_id": identity["image_id"],
                  "tool": tool, "complete_command_wall_ns": receipt["complete_command_wall_ns"],
                  "unknown_exit": 75, "unknown_commit": "omitted"}, indent=2))

#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

SOURCE = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g4-materialization-acceptance/v1/schedule_g4_v1.py")
EXPECTED_SHA256 = "556183797ec3116a15920494ac9f788ec1da231bd17a23cb9cd6ab081f55ced3"
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != EXPECTED_SHA256:
    raise SystemExit("frozen v1 schedule custody mismatch")
spec = importlib.util.spec_from_file_location("g4_v1_frozen_schedule", SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
SCHEDULE, EXPECTED, assert_schedule = module.SCHEDULE, module.EXPECTED, module.assert_schedule

if __name__ == "__main__":
    if sys.argv[1:] != ["--dry-run"]:
        raise SystemExit("usage: schedule_g4_v1.py --dry-run")
    value = assert_schedule()
    value.update(
        {
            "schema": "phase4-g4-schedule-v11",
            "frozen_v1_schedule_sha256": EXPECTED_SHA256,
            "logical_arm_envelopes": 50,
            "adjacent_estimator_routes": 13,
            "adjacent_estimator_replications_per_role": 2,
            "balanced_orders": ["ABBA", "BAAB"],
            "additional_measured_payloads": 26,
            "total_measured_payload_observations": 76,
            "measured_child_commands_recorded": 76,
            "bucket_partition_sum_ns": 120000000000,
        }
    )
    print(json.dumps(value, indent=2, sort_keys=True))

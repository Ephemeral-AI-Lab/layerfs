import json
import tempfile
import unittest
from pathlib import Path

import sdk_telemetry_report as report


class TelemetryReportTest(unittest.TestCase):
    def test_one_route_uses_only_layerfs_process_telemetry(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "identities.json").write_text('{"telemetry_run": 1}')
            (root / "primary-sandbox-id.txt").write_text("abc")
            host = [self.event(1, 1000, "sdk.route", 2000)]
            daemon = []
            for index, (_, key, host_name, daemon_name) in enumerate(report.PHASES, 1):
                host.append(self.event(1, key, host_name, index * 100))
                if daemon_name:
                    daemon.append(self.event(2, 1, daemon_name, index * 10))
            (root / "raw-host.log").write_text("".join("LFT1 " + json.dumps(e) + "\n" for e in host))
            (root / "layerfs-abc.stderr").write_text("".join("LFT1 " + json.dumps(e) + "\n" for e in daemon))
            value = report.summarize(root)
            self.assertEqual(value["route_host_process"]["elapsed_ns"], 2000)
            self.assertEqual(value["public_call_sum_ns"], 1500)
            self.assertEqual(value["phases"][1]["daemon_process"]["elapsed_ns"], 20)
            self.assertFalse(value["admission_eligible"])
            self.assertNotIn("container_cpu", value)

    @staticmethod
    def event(role, key, name, elapsed):
        return {"kind": "operation", "run": f"{1:032x}", "role": role, "key": key,
                "success": True, "resource_status": "sampled", "samples": 2, "gaps": 0,
                "cpu_shared_ns": [10, 5], "sampled_max_rss": 1024,
                "timing": {"name": name, "elapsed_ns": elapsed}}


if __name__ == "__main__":
    unittest.main()

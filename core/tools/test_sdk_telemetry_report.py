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

    def test_owner_lookup_events_are_assigned_to_the_public_call_window(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "identities.json").write_text('{"telemetry_run": 1}')
            (root / "primary-sandbox-id.txt").write_text("abc")
            route = self.event(1, 1000, "sdk.route", 2000)
            route["opened_ns"] = 0
            host = [route]
            daemon = []
            for index, (_, key, host_name, daemon_name) in enumerate(report.PHASES, 1):
                event = self.event(1, key, host_name, 100)
                event["opened_ns"] = index * 200
                host.append(event)
                if daemon_name:
                    daemon.append(self.event(2, 1, daemon_name, 10))
                    for offset, lookup_key, label in ((10, 2001, "owner.docker_port"),
                                                      (40, 2002, "owner.hello")):
                        step = self.event(1, lookup_key, label, 20)
                        step["opened_ns"] = event["opened_ns"] + offset
                        host.append(step)
            stray = self.event(1, 2001, "owner.docker_port", 20)
            stray["opened_ns"] = 3000
            host.append(stray)
            (root / "raw-host.log").write_text("".join("LFT1 " + json.dumps(e) + "\n" for e in host))
            (root / "layerfs-abc.stderr").write_text("".join("LFT1 " + json.dumps(e) + "\n" for e in daemon))
            value = report.summarize(root)
            self.assertNotIn("owner_lookup", value["phases"][0])
            self.assertEqual(value["phases"][1]["owner_lookup"]["docker_port"]["elapsed_ns"], 20)
            self.assertEqual(value["phases"][4]["owner_lookup"]["hello"]["elapsed_ns"], 20)

            host = [event for event in host if event.get("key") != 2001]
            (root / "raw-host.log").write_text("".join("LFT1 " + json.dumps(e) + "\n" for e in host))
            value = report.summarize(root)
            self.assertNotIn("docker_port", value["phases"][1]["owner_lookup"])
            self.assertEqual(value["phases"][1]["owner_lookup"]["hello"]["elapsed_ns"], 20)

    @staticmethod
    def event(role, key, name, elapsed):
        return {"kind": "operation", "run": f"{1:032x}", "role": role, "key": key,
                "success": True, "resource_status": "sampled", "samples": 2, "gaps": 0,
                "cpu_shared_ns": [10, 5], "sampled_max_rss": 1024,
                "timing": {"name": name, "elapsed_ns": elapsed}}


if __name__ == "__main__":
    unittest.main()

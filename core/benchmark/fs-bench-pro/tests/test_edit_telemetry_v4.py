"""One retained-log contract check for #241 v4."""

import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared.edit_telemetry_v4 import check  # noqa: E402


RUN = "000000000000000000000000000000f1"
SANDBOX = "0123456789abcdef0123456789abcdef"
DRIVER = {"caller_pid": 42, "sandbox": SANDBOX, "telemetry_run": RUN,
          "daemon_log_attempted": True, "daemon_log_truncated": False,
          "daemon_log_error": "None", "daemon_log_bytes": 10000}


def line(role, kind, **fields):
    event = {"v": 1, "role": role, "namespace": 7 if role == 1 else int(SANDBOX[:16], 16),
             "pid": 42 if role == 1 else 1, "run": RUN, "kind": kind} | fields
    return b"LFT1 " + json.dumps(event).encode() + b"\n"


def operation(role, key, name, success=True, children=None, sampled=True):
    resource = ({"resource_status": "sampled", "samples": 2, "gaps": 0,
                 "largest_gap_ns": 10, "scope": "process-shared", "clock": "local-monitor",
                 "first_ns": 0, "opened_ns": 1, "closed_ns": 3, "last_ns": 4,
                 "cpu_shared_ns": [1, 2], "sampled_max_rss": 4096}
                if sampled else {"resource_status": "unavailable", "samples": 0,
                                 "gaps": 0, "cpu_shared_ns": None, "sampled_max_rss": None})
    return line(role, "operation", key=key, timing={"name": name, "elapsed_ns": 2,
                                                  "children": children or []},
                success=success, **resource)


def summary(role):
    return line(role, "run-summary", dropped=0, failed=0, overflow=False)


class RetainedLft1(unittest.TestCase):
    def test_two_producer_identity_loss_and_unavailable_coverage(self):
        edit = {"name": "edit", "elapsed_ns": 1, "children": []}
        commit = {"name": "commit", "elapsed_ns": 1, "children": []}
        host = (operation(1, 3, "Inspect", success=False)
                + operation(1, 232_000, "sdk.edit_commit.fuse", children=[edit, commit])
                + summary(1))
        daemon = (operation(2, 3, "Inspect", success=False)
                  + operation(2, 4, "WorkspaceExec") + summary(2))
        raw = host + b"docker diagnostic\n" + daemon
        report = check(raw, DRIVER, RUN)
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["partial"], [])
        self.assertEqual(report["producers"]["host"]["negative_inspect"], 1)
        self.assertEqual(report["producers"]["daemon"]["negative_inspect"], 1)
        self.assertEqual(report["coverage"]["edit_commit"]["cpu_status"], "SAMPLED")
        self.assertEqual(report["children"]["edit"]["cpu_rss_attribution"],
                         "UNAVAILABLE; root is process-shared")
        self.assertEqual(check(host + daemon[:-1], DRIVER, RUN)["status"], "INCOMPLETE")
        self.assertEqual(check(host + daemon + summary(2), DRIVER, RUN)["status"], "INCOMPLETE")
        unsampled = host + operation(2, 4, "WorkspaceExec", sampled=False) + summary(2)
        self.assertEqual(check(unsampled, DRIVER, RUN)["status"], "UNAVAILABLE")
        partial = host.replace(b'"last_ns": 4', b'"last_ns": 2') + daemon
        result = check(partial, DRIVER, RUN)
        self.assertEqual(result["status"], "PASS")
        self.assertFalse(result["coverage"]["edit_commit"]["boundary_covered"])
        self.assertTrue(result["partial"])


if __name__ == "__main__":
    unittest.main()

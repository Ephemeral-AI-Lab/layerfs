"""Pure custody/registration tests; no builds, providers, or observations."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import sp1_components as sp1


class Sp1Components(unittest.TestCase):
    def test_nine_unique_rows_and_shared_control_collected_once(self):
        self.assertEqual(len(sp1.CASES), 9)
        self.assertEqual(len(set(sp1.select("sp1-components"))), 9)
        full = sp1.PREFIX + "small-new-full"
        self.assertIn(full, sp1.GROUPS["sp1-small-save"])
        self.assertIn(full, sp1.GROUPS["sp1-payload"])
        union = set().union(*(set(ids) for key, ids in sp1.GROUPS.items() if key != "sp1-components"))
        self.assertEqual(union, set(sp1.CASES))
        self.assertEqual(len({case.master for case in sp1.CASES.values()}), 5)
        with self.assertRaises(ValueError):
            sp1.select("unregistered")

    def test_frozen_thresholds_and_missing_metrics_never_zero(self):
        self.assertEqual([case.alert_ns for case in sp1.CASES.values()],
                         [20_000_000, 20_000_000, 250_000_000, 250_000_000,
                          250_000_000, 100_000_000, 100_000_000, 100_000_000, 250_000_000])
        for case in sp1.CASES.values():
            row = sp1.base_row(case)
            self.assertIsNone(row["operation_ns"])
            self.assertFalse(row["admission_eligible"])
            self.assertEqual(row["numeric_speed_status"], "INELIGIBLE")
            self.assertTrue(all(value is None for value in row["metrics"].values()))
        self.assertLess(sp1.PROOF_BUDGET_NS, 10_000_000_000)
        self.assertEqual(sp1.COMMAND_BUDGET_NS, 15_000_000_000)

    def master(self, path):
        path.mkdir()
        (path / "catalog.sqlite").write_bytes(b"closed SQL fixture bytes")
        (path / "provider_objects").mkdir()
        (path / "provider_objects" / ("a" * 64 + ".pack")).write_bytes(b"sealed pack bytes")
        return {"path": str(path), "compatibility": {"input": "sealed"},
                "files": sp1.files(path), "bucket": "immutable-master"}

    def test_byte_clone_is_independent_and_master_mutation_refuses(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            master = self.master(root / "master")
            clone = root / "clone"
            record = sp1.clone_master(master, clone)
            self.assertEqual(record["setup"], "clone")
            self.assertTrue(record["master_unchanged"])
            (clone / "catalog.sqlite").write_bytes(b"mutated sample")
            sp1.validate_master(master["path"], master)
            self.assertNotEqual((clone / "catalog.sqlite").read_bytes(), (root / "master/catalog.sqlite").read_bytes())
            (root / "master/catalog.sqlite").write_bytes(b"mutated master")
            with self.assertRaisesRegex(ValueError, "master changed"):
                sp1.clone_master(master, root / "second")

    def test_sidecar_and_symlink_fail_closed(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            master = self.master(root / "master")
            (root / "master/catalog.sqlite-wal").write_bytes(b"unresolved")
            master["files"] = sp1.files(root / "master")
            with self.assertRaisesRegex(ValueError, "sidecar"):
                sp1.validate_master(master["path"], master)
            (root / "link").symlink_to(root / "master/catalog.sqlite")
            with self.assertRaisesRegex(ValueError, "symlink"):
                sp1.files(root)

    def test_cli_preserves_perf_binding_and_secrets_are_not_arguments(self):
        command = sp1.driver_command({"path": "/sealed/release/sp1_speed"}, "verify",
                                     next(iter(sp1.CASES)), "/assets", "/oracle", "/sql", "/proof.json", perf="/perf.json")
        self.assertEqual(command[-2:], ["--perf", "/perf.json"])
        self.assertEqual(command[1:3], ["--phase", "verify"])
        self.assertNotIn("--secret", command)
        self.assertNotIn("--access", command)

    def test_successful_clock_stays_cache_ineligible_and_retains_for_proof(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            master = self.master(root / "master")
            case = next(iter(sp1.CASES.values()))
            binary = {"path": "/sealed/sp1_speed", "sha256": "binary"}
            def invoke(command, destination, timeout_ns, *, env):
                phase = command[command.index("--phase") + 1]
                output = Path(command[command.index("--out") + 1])
                value = {"phase": phase, "case": case.id, "status": "PASS", "operation_contract_id": sp1.PROFILE}
                if phase == "perf":
                    value.update(operation_ns=1_000_000, cpu_user_ns=None, cpu_system_ns=None, metrics={"provider": None})
                output.write_text(json.dumps(value))
                return {"exit_code": 0, "timeout": False, "wall_ns": 1, "started_ns": 11}
            with patch.object(sp1, "invoke", side_effect=invoke), patch.object(sp1, "provider_environment", return_value={}), patch.object(sp1, "precondition", return_value={"finished_ns": 10, "status": "ZERO_HOST_RESIDENCY"}):
                row = sp1.attempt(root, case, master, binary, "/oracle", {"source": "sealed"}, {})
            self.assertEqual(row["status"], "OBSERVED")
            self.assertEqual(row["sample_count"], 1)
            self.assertEqual(row["operation_ms"], 1)
            self.assertEqual(row["alert_status"], "UNDER_ALERT")
            self.assertEqual(row["numeric_speed_status"], "INELIGIBLE")
            self.assertEqual(row["cleanup_status"], "OWNED_RETAINED_FOR_SEPARATE_PROOF")
            self.assertFalse(row["admission_eligible"])
            self.assertIsNone(row["cpu_user_ns"])

    def test_failed_setup_never_attempts_performance_or_hides_row(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            master = self.master(root / "master")
            case = next(iter(sp1.CASES.values()))
            with patch.object(sp1, "provider_environment", return_value={}), patch.object(sp1, "invoke", return_value={"exit_code": 1, "timeout": False, "wall_ns": 1}) as invoke:
                row = sp1.attempt(root, case, master, {"path": "/sealed/sp1_speed"}, "/oracle", {}, {})
            self.assertEqual(invoke.call_count, 1)
            self.assertEqual(row["sample_count"], 0)
            self.assertEqual(row["status"], "FAIL")
            self.assertEqual(row["cleanup_status"], "RETAINED_FAILURE_CUSTODY")
            self.assertTrue((root / case.id / "receipt.json").is_file())

    def test_release_build_seals_before_after_and_archives_exact_binary(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            results, target, out = root / "results", root / "target", root / "out"
            results.mkdir(); out.mkdir(); (target / "release/examples").mkdir(parents=True)
            (target / "release/examples/sp1_speed").write_bytes(b"locked release executable")
            common = type("Common", (), {"RESULTS": results, "target_path": staticmethod(lambda: target)})
            expected = {"compilation_seal": "source-before-after"}
            captured = []
            def invoke(command, destination, timeout_ns, *, env):
                captured.append(command)
                self.assertEqual(timeout_ns, sp1.BUILD_BUDGET_NS)
                self.assertEqual(env["CARGO_TARGET_DIR"], str(target))
                return {"exit_code": 0, "timeout": False, "wall_ns": 1}
            with patch.object(sp1, "invoke", side_effect=invoke), patch.object(sp1, "identity", return_value=expected):
                result = sp1.build(out, common, expected)
                reused = sp1.build(out, common, expected)
            self.assertEqual(len(captured), 1)
            self.assertIn("--release", captured[0]); self.assertIn("--locked", captured[0])
            self.assertEqual(result["build_profile"], "release")
            self.assertEqual(reused["mode"], "exact-binary-reuse")
            self.assertEqual(sp1.shared.sha256(result["binary"]["path"]), result["binary"]["sha256"])

    def test_source_change_during_build_refuses_publication(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            common = type("Common", (), {"RESULTS": root, "target_path": staticmethod(lambda: root / "target")})
            with patch.object(sp1, "invoke", return_value={"exit_code": 0, "timeout": False, "wall_ns": 1}), patch.object(sp1, "identity", return_value={"compilation_seal": "changed"}):
                with self.assertRaisesRegex(ValueError, "source changed"):
                    sp1.build(root, common, {"compilation_seal": "original"})
            self.assertFalse((root / "sp1-build-release-v1.json").exists())

    def test_report_always_lists_nine_rows_and_four_views(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            value = {"schema": sp1.SCHEMA, "rows": [sp1.base_row(c) for c in sp1.CASES.values()]}
            (path / "run.json").write_text(json.dumps(value))
            report = sp1.report(path)
            self.assertEqual(sum(line.startswith(sp1.PREFIX) for line in report.splitlines()), 9)
            for group in ("sp1-metadata", "sp1-small-save", "sp1-payload", "sp1-sql"):
                self.assertIn(group + "\t", report)

    def test_retained_cardinality_rejects_missing_or_admitted_rows(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            common = type("Common", (), {"verify_run_manifest": staticmethod(lambda _: None)})
            rows = [sp1.base_row(c) for c in sp1.CASES.values()]
            (path / "run.json").write_text(json.dumps({"schema": sp1.SCHEMA, "rows": rows}))
            self.assertIn("custody only", sp1.verify(path, common))
            rows[0]["admission_eligible"] = True
            (path / "run.json").write_text(json.dumps({"schema": sp1.SCHEMA, "rows": rows}))
            with self.assertRaises(ValueError):
                sp1.verify(path, common)
            (path / "run.json").write_text(json.dumps({"schema": sp1.SCHEMA, "rows": rows[1:]}))
            with self.assertRaises(ValueError):
                sp1.verify(path, common)


if __name__ == "__main__":
    unittest.main()

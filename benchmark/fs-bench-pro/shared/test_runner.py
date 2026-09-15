"""Small offline checks for sample identity and compact result parsing."""
import json
import io
from contextlib import redirect_stderr
import time
import tempfile
import sqlite3
from pathlib import Path
import unittest
from unittest.mock import Mock, patch
from types import SimpleNamespace

import runner


class RunnerTests(unittest.TestCase):
    def test_only_explicit_large_sequences_use_scaled_verification(self):
        normal = runner.verification_policy({"family": "init_namespace", "case": "namespace-100000"})
        self.assertEqual((normal["work_limit_seconds"], normal["hard_limit_seconds"]), (45, 59))
        for count, commits, expected in ((100, 1, 45), (100, 3, 45), (1000, 1, 45),
                                          (1001, 1, 600), (1000, 33, 600), (32000, 1, 600)):
            selection = {"family": "init_namespace", "sequence": {
                "schema": "workspace-sequence-v1", "edit_count": count, "commits": commits}}
            self.assertEqual(runner.verification_policy(selection)["work_limit_seconds"], expected)
            self.assertEqual(runner.verification_policy({**selection, "family": "dedup_branch_history"}), normal)
        self.assertEqual(runner.verification_policy({"family": "init_namespace",
            "sequence": {"schema": "unrecognized", "edit_count": 32000, "commits": 1}}), normal)

    def test_verification_command_cap_cleanup_and_sequence_truncation(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, "HOST_ROOT", Path(directory)):
            for count, truncated, deadline, expected_end, expected_status in (
                    (None, False, 700, 145, "PASS"),
                    (1000, False, 700, 145, "PASS"),
                    (32000, False, 700, 696, "PASS"),
                    (32000, True, 700, 696, "FAIL")):
                with self.subTest(count=count, truncated=truncated):
                    selection = {"family": "init_namespace", "case": "namespace-100000", "seed": 1,
                        "verification_supported": True, "route": "namespace", "timer": "edit_commit_ns",
                        "setup_identity": "fresh-output", "source_arm": "candidate", "image": "image"}
                    if count is not None:
                        selection["sequence"] = {"schema": "workspace-sequence-v1", "edit_count": count,
                            "commits": 1, "reopen": False, "active_cache": False}
                    args = SimpleNamespace(prepare_only=False, cpus=2, memory_mib=2048,
                        performance_rows="-", host_binary="fake", timeout=999, product_timeout=998)
                    sample = SimpleNamespace(id="fake", observation={}, remove=Mock())
                    command_result = SimpleNamespace(returncode=0, truncated=truncated, stderr=b"",
                        stdout=b'{"kind":"workspace-sequence","status":"PASS","edit_commit_ns":1}\n')
                    snapshot = {"usage_usec": 0, "memory_peak": 0, "memory_current": 0,
                                "swap_current": 0, "oom_kill": 0}
                    with patch.object(runner, "resolve_selection", return_value=selection), \
                         patch.object(runner, "_host_acquire", return_value={"host_root": directory, "image": "image"}), \
                         patch.object(runner.runtime, "start_sample", return_value=sample), \
                         patch.object(runner, "_host_sample", return_value={"prepared_input_root": directory}), \
                         patch.object(runner, "cgroup_snapshot", return_value=snapshot), \
                         patch.object(runner.time, "monotonic", return_value=100), \
                         patch.object(runner, "_command", return_value=command_result) as command:
                        result = runner.execute_selected(args, deadline=deadline, verification=True)
                    self.assertEqual(command.call_args.args[1], expected_end)
                    self.assertEqual(command.call_args.kwargs["output_limit"], 16 * 1024**2)
                    self.assertEqual(sample.remove.call_args.kwargs["deadline"].end, deadline)
                    self.assertEqual(result["status"], expected_status)
                    self.assertEqual(result["cleanup"]["status"], "PASS")

    def test_sequence_is_explicit_bounded_and_not_an_init_gate(self):
        for options in (["--sequence", "-1"], ["--sequence", "100001"],
                        ["--sequence", "1", "--sequence-commits", "0"],
                        ["--sequence-reopen"]):
            args = runner.build_parser().parse_args(
                ["--family", "init_namespace", "--case", "namespace-100000", *options])
            with self.assertRaises(ValueError):
                runner.resolve_selection(args, time.monotonic() + 1)
        row = {"family": "init_namespace", "case": "namespace-100000"}
        self.assertTrue(runner.cold.applies(row))
        self.assertFalse(runner.cold.applies({**row, "sequence": {"edit_count": 100}}))
        self.assertEqual(runner._timer({"identities": {"timer": "edit_commit_ns"},
            "records": [{"layerstack_init_ns": 1, "edit_commit_ns": 7}]}), ("edit_commit_ns", 7))

    def test_linked_schema_rejects_false_source_label(self):
        with patch.object(runner.runtime, "run", return_value=SimpleNamespace(stdout=b"7\n")):
            self.assertEqual(runner.verify_linked_schema("binary", 7), 7)
            with self.assertRaisesRegex(ValueError, "stale build"):
                runner.verify_linked_schema("binary", 9)

    def test_docker_topology_rejected_before_execution(self):
        with patch.object(runner, "_command") as command:
            for modes in (True, False):
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                    runner.build_parser(include_modes=modes).parse_args(
                        ["--family", "payload_create_read", "--topology", "docker"])
                self.assertEqual(error.exception.code, 2)
            args = runner.build_parser().parse_args(["--family", "payload_create_read"])
            args.topology = "docker"
            args._selection = {"topology": "docker"}
            with self.assertRaisesRegex(ValueError, "host-store"):
                runner.resolve_selection(args, time.monotonic() + 1)
            command.assert_not_called()

    def test_host_cache_semantic_reuse_and_sample_isolation(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, "HOST_ROOT", Path(directory)):
            fixture = {"fixture_profile": "test-v1", "input_mode": "store", "input_plan_sha256": "plan", "fixture_bytes": 0}
            selection = {"family": "payload_create_read", "case": "first", "seed": 1, "setup_identity": "clone", "image": "image-a", "host_executor": {"source": "a", "schema_sha256": "schema-a"}}
            args = SimpleNamespace(host_binary="fake")
            preparations = []
            def command(argv, deadline, **kwargs):
                if argv[1] == "infra-prepare":
                    root = Path(argv[-1])
                    (root / "payload").mkdir(parents=True)
                    db = sqlite3.connect(root / "payload/store.sqlite")
                    db.execute("CREATE TABLE state(value)")
                    db.execute("INSERT INTO state VALUES (1)")
                    db.commit()
                    db.close()
                    (root / "payload/branch-id").write_text("branch")
                    (root / "manifest.json").write_text('{}')
                    preparations.append(root)
                return SimpleNamespace(stdout=json.dumps(fixture))
            with patch.object(runner, "_command", side_effect=command), patch.object(runner.runtime, "evict_host_cache") as evict:
                first = runner._host_acquire(args, selection, time.monotonic() + 5)
                changed_executor = {**selection, "case": "second", "image": "image-b", "host_executor": {"source": "b", "schema_sha256": "schema-a"}}
                second = runner._host_acquire(args, changed_executor, time.monotonic() + 5)
                self.assertFalse(first["cache_hit"])
                self.assertTrue(second["cache_hit"])
                self.assertEqual(len(preparations), 1)
                evict.assert_not_called()
                self.assertEqual(second["producer"], selection["host_executor"])
                one = runner._host_sample(first, selection, "one", time.monotonic() + 5)
                two = runner._host_sample(second, changed_executor, "two", time.monotonic() + 5)
                self.assertNotEqual(one["sample_store_inode"], two["sample_store_inode"])
                self.assertEqual(one["sample_store_sha256"], two["sample_store_sha256"])
                fixture["input_plan_sha256"] = "changed"
                third = runner._host_acquire(args, selection, time.monotonic() + 5)
                self.assertFalse(third["cache_hit"])
                self.assertNotEqual(first["cache_key"], third["cache_key"])
                fixture["fixture_profile"] = "tiny-bulk-mixed-v3"
                fixture["populated_manifest_sha256"] = "mixed-target-full-digests"
                fourth = runner._host_acquire(args, selection, time.monotonic() + 5)
                self.assertFalse(fourth["cache_hit"])
                self.assertNotEqual(third["cache_key"], fourth["cache_key"])
                fixture["fixture_profile"] = "workspace-mixed-v4"
                fixture["fixture_bytes"] = 104857600
                fixture["regular_files"] = 2000
                fifth = runner._host_acquire(args, selection, time.monotonic() + 5)
                self.assertFalse(fifth["cache_hit"])
                self.assertNotEqual(fourth["cache_key"], fifth["cache_key"])

    def test_mixed_v3_strict_classifier_and_cache_invalidation(self):
        for operation in ("create", "delete"):
            selection = {"family": "tiny_file_churn", "case": f"tiny-bulk-{operation}-100-mixed-v3"}
            self.assertEqual(runner.issue47_assessment(selection, 999_999_999)["status"], "PASS")
            self.assertEqual(runner.issue47_assessment(selection, 1_000_000_000)["status"], "TARGET_MISS")
            for case in (f"tiny-bulk-{operation}-100", f"tiny-bulk-{operation}-500-mixed-v3"):
                self.assertIsNone(runner.issue47_assessment({**selection, "case": case}, 1))
        old = {"fixture_profile": "workspace-input-v1", "input_plan_sha256": "old-target"}
        new = {"fixture_profile": "tiny-bulk-mixed-v3", "input_plan_sha256": "new-target", "populated_manifest_sha256": "full-digests"}
        mixed_v4 = {"fixture_profile": "workspace-mixed-v4", "input_plan_sha256": "mixed-v4-target", "fixture_bytes": 104857600, "regular_files": 2000}
        self.assertNotEqual(runner.digest(old), runner.digest(new))
        self.assertNotEqual(runner.digest(old), runner.digest(mixed_v4))
        self.assertNotEqual(runner.digest(new), runner.digest(mixed_v4))

    def test_mixed_v4_profile_is_isolated_from_shards_and_compact(self):
        shards = {"fixture_profile": "workspace-input-v1", "input_plan_sha256": "plan", "fixture_bytes": 104857600}
        compact = {"fixture_profile": "compact-low-tier-v2", "input_plan_sha256": "plan", "fixture_bytes": 10485760}
        mixed = {"fixture_profile": "workspace-mixed-v4", "input_plan_sha256": "plan", "fixture_bytes": 104857600}
        self.assertNotEqual(runner.digest(shards), runner.digest(mixed))
        self.assertNotEqual(runner.digest(compact), runner.digest(mixed))

    def test_history_unrelated_mixed_v2_profile_is_isolated_from_shards(self):
        shards = {"fixture_profile": "workspace-input-v1", "input_plan_sha256": "plan", "fixture_bytes": 1048576, "regular_files": 200}
        mixed = {"fixture_profile": "history-unrelated-mixed-v2", "input_plan_sha256": "plan", "fixture_bytes": 1048576, "regular_files": 10}
        self.assertNotEqual(runner.digest(shards), runner.digest(mixed))

    def test_mixed_oracle_identity_reuse_keeps_proof_bounded(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, "HOST_ROOT", Path(directory)):
            args = SimpleNamespace(host_binary="host", family="tiny_file_churn", case="tiny-bulk-create-100-mixed-v3", verification=False)
            identity = {"binary_sha256": "binary"}
            fixture = {"fixture_profile": "tiny-bulk-mixed-v3", "populated_manifest_sha256": "full-digests"}
            with patch.object(runner, "_command", return_value=SimpleNamespace(stdout=json.dumps(fixture))) as command:
                self.assertEqual(runner.mixed_fixture_info(args, identity, 1, 10), fixture)
                args.verification = True
                self.assertEqual(runner.mixed_fixture_info(args, identity, 1, 10), fixture)
                command.assert_called_once()
                with self.assertRaisesRegex(ValueError, "matching performance"):
                    runner.mixed_fixture_info(args, identity, 2, 10)
                path = next((Path(directory) / "fixture-identities").glob("*.json"))
                saved = json.loads(path.read_text())
                saved["fixture"]["populated_manifest_sha256"] = "corrupt"
                path.chmod(0o644)
                path.write_text(json.dumps(saved))
                with self.assertRaisesRegex(ValueError, "cache mismatch"):
                    runner.mixed_fixture_info(args, identity, 1, 10)

    def test_explicit_diagnostic_product_allowance_keeps_target(self):
        _, selection = self.resolve(["--product-timeout", "600", "--timeout", "630"], {})
        self.assertEqual(selection["product_execution_allowance_seconds"], 600)
        self.assertEqual(runner.performance_target_status(16_000_000_000), "TARGET_MISS")
        for arguments in (["--product-timeout", "0"], ["--product-timeout", "130"], ["--product-timeout", "600"]):
            with self.assertRaisesRegex(ValueError, "resource/budget"):
                self.resolve(arguments, {})

    def test_deadline_units(self):
        remaining = runner._deadline(time.monotonic() + 5).remaining()
        self.assertGreater(remaining, 4)
        self.assertLessEqual(remaining, 5)

    def resolve(self, argv, row, image_native="native"):
        args = runner.build_parser().parse_args(["--family", "payload_create_read", "--image", "sealed", "--case", "case", *argv])
        image = {"Id": "sha256:image", "Os": "linux", "Architecture": "arm64",
                 "Config": {"Labels": {"dev.layerfs.source-seal": "source", "dev.layerfs.product-seal": "product", "dev.layerfs.compilation-seal": image_native}}}
        host_identity = {"binary_sha256": "binary", "LAYERFS_PRODUCT_SEAL": "product", "LAYERFS_SOURCE_SEAL": "source", "LAYERFS_COMPILATION_SEAL": "native"}
        with patch.object(runner.platform, "system", return_value="Darwin"), patch.object(runner.runtime, "file_sha256", return_value="binary"), patch.object(Path, "read_text", return_value=json.dumps(host_identity)), patch.object(runner, "image_info", return_value=image), patch.object(runner, "_command", return_value=SimpleNamespace(stdout=json.dumps({"family_id": "payload_create_read", "scenario_id": "case", **row}))):
            return args, runner.resolve_selection(args, 999999999)

    def test_image_reuse_requires_matching_compilation_inputs(self):
        for identity in (None, "older-workload"):
            with self.assertRaisesRegex(ValueError, "compilation seals differ"):
                self.resolve([], {}, image_native=identity)

    def test_n_does_not_change_seed(self):
        one_args, one = self.resolve(["--seed", "2", "--perf-fast"], {})
        many_args, many = self.resolve(["--seed", "2", "--perf-samples", "10"], {})
        self.assertEqual(one["input_identity"], many["input_identity"])
        self.assertEqual(many["seed"], 2)
        self.assertEqual(many_args.perf_samples, 10)

    def test_initialization_rejects_clone(self):
        with self.assertRaisesRegex(ValueError, "fresh output"):
            self.resolve(["--setup", "clone"], {"setup_policy": "fresh-output"})

    def test_inherited_requires_repetition(self):
        with self.assertRaisesRegex(ValueError, "repetition"):
            self.resolve(["--seed", "1"], {"inherited": True})
        _, selection = self.resolve(["--repetition", "1"], {"inherited": True})
        self.assertEqual(selection["repetition"], 1)

    def test_source_and_input_authentication(self):
        with self.assertRaisesRegex(ValueError, "source identity"):
            self.resolve(["--source", "other"], {})
        with self.assertRaisesRegex(ValueError, "input identity"):
            self.resolve(["--input", "other"], {})

    def test_count_rejected(self):
        with self.assertRaisesRegex(ValueError, "positive"):
            self.resolve(["--perf-samples", "0"], {})

    def test_shared_fast_settings_and_explicit_timer(self):
        for family in runner.HOST_FAMILIES:
            args = runner.build_parser().parse_args(["--family", family])
            self.assertEqual((args.topology, args.cpus, args.memory_mib, args.timeout),
                             ("host-store", 2, 2048, 130))
            self.assertIsNone(args.perf_samples)
        self.assertEqual(len(runner.HOST_FAMILIES), 21)
        self.assertIn("tiny_file_churn", runner.HOST_FAMILIES)
        for family in ("workspace_change_locality", "dedup_branch_history", "git_tool_workflow",
                       "mixed_load_bearing", "workspace_reliability"):
            self.assertIn(family, runner.HOST_FAMILIES)
        row = {"identities": {"timer": "product_call_sum_ns"}, "command_wall_ns": 999,
               "records": [{"complete_ns": 888}, {"product_call_sum_ns": 123}]}
        self.assertEqual(runner._timer(row), ("product_call_sum_ns", 123))
        row["records"] = [{"complete_ns": 888}]
        self.assertEqual(runner._timer(row), ("product_call_sum_ns", None))

    def test_v016_declared_deadlines_match_the_frozen_case_plan(self):
        """The declared v0.1.6 deadlines are the frozen cases.json values, not the
        overlay line's wider allowances: an extended case carries its own
        watchdog and every regular case the 25-second exception ceiling."""
        plan = json.loads((runner.REPO / "docs/roadmap/0.1/0.1.6/cases.json").read_text())
        cases = {case["id"]: case for case in plan["cases"]}
        for case_id, seconds in runner.V016_EXTENDED_WATCHDOGS.items():
            self.assertEqual(cases[case_id]["lane"], "extended", case_id)
            self.assertEqual(cases[case_id]["verify_complete_limit_ms"], seconds * 1000, case_id)
        self.assertEqual(runner.V016_REGULAR_DEADLINE_SECONDS, 25)
        for case_id, case in cases.items():
            if case["lane"] == "regular":
                self.assertEqual(case["perf_complete_limit_ms"], 15_000, case_id)
        # A renamed or unregistered case is never silently admitted.
        self.assertIsNone(runner.v016_watchdog_seconds({"family": "dedup_branch_history", "case": "dedup-history-hotset-10"}))
        self.assertEqual(runner.v016_watchdog_seconds({"family": "file_size_transition", "case": "v016-boundary-exact-v1"}), 25)
        self.assertEqual(runner.verification_policy({"family": "file_size_transition", "case": "v016-boundary-exact-v1"})["work_limit_seconds"], 25.0)
        extended = runner.verification_policy({"family": "mixed_load_bearing",
                                               "case": "v016-mixed-exhaustive-500mb-30000-k100-v1"})
        self.assertEqual(extended["declared_complete_deadline_seconds"], 300)

    def test_timer_reconstructs_pure_call_sum_from_phase_records(self):
        row = {"identities": {"timer": "pure_call_sum_ns"},
               "records": [
                   {"kind": "phase", "phase": "create", "elapsed_ns": 10},
                   {"kind": "phase", "phase": "exec", "elapsed_ns": 20},
                   {"kind": "phase", "phase": "commit", "elapsed_ns": 30},
               ]}
        self.assertEqual(runner._timer(row), ("pure_call_sum_ns", 60))

    def test_execution_allowance_does_not_relax_product_target(self):
        self.assertEqual(runner.PRODUCT_TARGET_NS, 15_000_000_000)
        self.assertEqual(runner.performance_target_status(15_000_000_000), "PASS")
        self.assertEqual(runner.performance_target_status(15_000_000_001), "TARGET_MISS")
        self.assertEqual(runner.performance_target_status(119_000_000_000), "TARGET_MISS")

    def test_collection_mode_keeps_historical_target_classifier(self):
        args = runner.build_parser().parse_args([
            "--family", "namespace_mutation", "--collection-mode",
            "--product-timeout", "300", "--timeout", "310",
        ])
        self.assertTrue(args.collection_mode)
        self.assertEqual(args.product_timeout, 300)
        self.assertEqual(args.timeout, 310)
        self.assertEqual(runner.performance_target_status(16_000_000_000), "TARGET_MISS")
        self.assertIn("reporting-only", runner.HISTORICAL_PRODUCT_TARGET_SCOPE)

    def test_collection_timeouts_must_still_be_ordered(self):
        with self.assertRaisesRegex(ValueError, "resource/budget"):
            self.resolve(["--collection-mode", "--product-timeout", "300", "--timeout", "300"], {})
        _, selection = self.resolve(
            ["--collection-mode", "--product-timeout", "300", "--timeout", "310"], {}
        )
        self.assertEqual(selection["product_execution_allowance_seconds"], 300)

    def test_sdk_repetitions_share_input_identity(self):
        _, one = self.resolve(["--repetition", "1"], {"route": "sdk", "inherited": True, "seed_max": 5})
        _, five = self.resolve(["--repetition", "5"], {"route": "sdk", "inherited": True, "seed_max": 5})
        self.assertEqual(one["input_identity"], five["input_identity"])

    def test_parse_only_complete_json(self):
        self.assertEqual(runner.records('log\nPREFIX\t{"kind":"done"}\n{"truncated":'), [{"kind": "done"}])

    def test_native_diagnostics_remain_explicit_debug_text(self):
        line = "layerfs-initialization-producer-v1 nonce=abcd producer=0 files=100"
        self.assertEqual(runner.initialization_diagnostics("unrelated\n" + line),
                         [{"kind": "initialization-debug-text", "details": line}])


if __name__ == "__main__":
    unittest.main()

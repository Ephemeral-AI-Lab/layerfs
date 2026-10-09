"""Deterministic external oracle-author tests; synthetic fixtures, no product run.

Recipe callbacks and subprocess fixtures are test-owned. They establish neither
native staging provenance nor a performance/oracle receipt for a selected arm.
"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "fs-bench-pro"))
from r7 import oracle, registry, workloads

MODULE = importlib.util.spec_from_file_location("r7_test_oracle_prepare", HERE / "oracle_prepare.py")
author = importlib.util.module_from_spec(MODULE)
MODULE.loader.exec_module(author)
CASES = {row["case_id"]: row for row in registry.cases()}


class Recipes(unittest.TestCase):
    def schedule(self, case, cache=None, roots=None, failure=None):
        events = []
        roots = roots or ([Path("/native-scan"), Path("/native-writer")] if case == "W01" else [Path("/native")])
        def body(label, command, root):
            events.append(("body", label, author.body_argv(command), root))
            if failure is not None:
                raise failure
            return {"label": label, "stdout": label + ".stdout"}
        def observe(label, root, role="primary", **binding):
            events.append(("observe", label, root, role, binding))
        result = author.reference_schedule(case, cache, CASES[case]["workload"], roots, body, observe)
        return events, result

    def test_k02_k03_preserve_registered_argv_and_payload_oracle(self):
        for case, dependency, text in (("K02", "E12", "python3 /code/workload.py copy"),
                                       ("K03", "E13", "python3 /code/workload.py link")):
            with self.subTest(case=case):
                events, result = self.schedule(case)
                self.assertEqual(events[0], ("body", "expectation-1", ["/bin/bash", "-o", "pipefail", "-c", text], Path("/native")))
                self.assertEqual(events[0][2], CASES[case]["workload"]["commands"][0])
                self.assertEqual(events[1], ("observe", "checkpoint-1", Path("/native"), "primary",
                                            {"command_labels": ["expectation-1"], "checkpoint": 1}))
                self.assertEqual(author.observer_for(case), {"kind": "port", "case": dependency, "compare_case": dependency})
                self.assertEqual(result["last"]["label"], "expectation-1")
                self.assertIn("actual product Commit", result["commit_custody"])
                self.assertIsNone(author.not_run(case, None))

    def test_k04_observes_every_frontier_before_next_body(self):
        events, result = self.schedule("K04")
        self.assertEqual(len(events), 10)
        for index in range(1, 6):
            self.assertEqual(events[2 * index - 2][2][-1],
                             "printf 'r7-increment-%s\\n' '" + str(index) + "' > r7-increment-" + str(index))
            self.assertEqual(events[2 * index - 1][1], "checkpoint-" + str(index))
            self.assertEqual(events[2 * index - 1][4], {"command_labels": ["expectation-" + str(index)], "checkpoint": index})
        self.assertEqual(result["last"]["label"], "expectation-5")

    def test_k04_first_body_failure_prevents_observer_and_later_bodies(self):
        original = OSError("original body refused")
        failure = author.ReferenceFailure(original, {"label": "expectation-1", "pid": 71}, [], "command_wait")
        events = []
        def body(*args):
            events.append(args)
            raise failure
        with self.assertRaises(author.ReferenceFailure) as caught:
            author.reference_schedule("K04", None, CASES["K04"]["workload"], [Path("/native")], body,
                                      lambda *args, **kwargs: self.fail("observer must not run after failed original body"))
        self.assertIs(caught.exception, failure)
        self.assertIs(caught.exception.original, original)
        self.assertEqual(len(events), 1)

    def test_class_c_warmup_then_expectation_on_same_root(self):
        events, _ = self.schedule("C10", "C")
        self.assertEqual([event[1] for event in events], ["warmup", "expectation", "expected"])
        self.assertEqual(events[0][2], events[1][2])
        self.assertEqual(events[0][3], events[1][3])
        self.assertEqual(events[2][4], {"command_labels": ["expectation"]})

    def test_stdout_only_case_has_no_tree_observer(self):
        for case in ("E05", "E06"):
            events, _ = self.schedule(case, "A")
            self.assertEqual(len(events), 1)
            self.assertEqual(events[0][2], workloads.workload(case)["argv"])

    def test_w01_preserves_distinct_scan_writer_roots_and_roles(self):
        events, _ = self.schedule("W01", "C")
        self.assertEqual([row[1] for row in events], ["warmup-parallel", "expectation-parallel", "scan", "writer"])
        for row in events[:2]:
            self.assertIn("cd -- /native-scan", row[2][-1])
            self.assertIn("cd -- /native-writer", row[2][-1])
            self.assertIn("find . | wc -l", row[2][-1])
            self.assertIn("python3 /code/workload.py append", row[2][-1])
            self.assertIn('wait "$pid"', row[2][-1])
        self.assertEqual(events[2][2:4], (Path("/native-scan"), "scan"))
        self.assertEqual(events[3][2:4], (Path("/native-writer"), "writer"))
        self.assertEqual(author.observer_for("W01", "scan")["case"], "E02")
        self.assertEqual(author.observer_for("W01", "writer")["selection"], "log")

    def test_w02_sequential_stage_precedes_each_parallel_stage(self):
        events, _ = self.schedule("W02", "C")
        self.assertEqual([row[1] for row in events], ["warmup-sequential", "warmup-parallel",
                                                   "expectation-sequential", "expectation-parallel", "shared"])
        self.assertEqual(events[0][2][-1], "true")
        self.assertEqual(events[2][2][-1], "true")
        self.assertEqual(events[-1][2:4], (Path("/native"), "primary"))
        self.assertEqual(events[-1][4]["command_labels"], ["expectation-sequential", "expectation-parallel"])

    def test_peer_alias_and_wrong_population_are_refused_before_body(self):
        for roots in ([Path("/native")], [Path("/native"), Path("/native")]):
            with self.assertRaisesRegex(ValueError, "peer"):
                self.schedule("W01", "C", roots=roots)

    def test_argv_malformed_or_alternate_launcher_is_not_coerced(self):
        for value in (["echo", "x"], ["/bin/bash", "-o", "pipefail", "-c", ["wrong"]], 4):
            with self.assertRaisesRegex(ValueError, "Bash"):
                author.body_argv(value)

    def test_nonrepeatable_and_pending_selections_remain_explicit(self):
        self.assertIn("nonrepeatable", author.not_run("E13", "C"))
        for case in ("E03", "E07", "E08", "E09", "E19"):
            self.assertIsNotNone(author.not_run(case, None))
        self.assertIn("nonrepeatable", author.not_run("C12", "C"))

    def test_read_only_git_cases_preserve_original_body_and_exact_scoped_observer(self):
        for case in ("E04", "E18"):
            events, _ = self.schedule(case, "A")
            self.assertEqual(events[0][2], ["/bin/bash", "-o", "pipefail", "-c",
                                          "git -c core.fsmonitor=false status --porcelain=v1"])
            self.assertEqual(author.observer_for(case), {"kind": "git", "case": case, "compare_case": case,
                                                         "oracle_schema": "r7-git-index-scoped-v2"})
            self.assertIsNone(author.not_run(case, "A"))
        self.assertIn("refresh", author.not_run("E18", "C"))
        self.assertIsNone(author.not_run("E18", "B"))
        with self.assertRaisesRegex(ValueError, "nonrepeatable"):
            self.schedule("E18", "C")

    def test_mutable_git_bodies_are_original_once_and_nonrepeatable_init_refuses(self):
        for case in ("E10", "E11", "C12"):
            events, _ = self.schedule(case, "B")
            self.assertEqual([row[0] for row in events], ["body","observe"])
            self.assertEqual(events[0][2], CASES[case]["workload"]["argv"])
            self.assertEqual(events[1][4]["command_labels"], ["expectation"])
            self.assertEqual(author.observer_for(case)["kind"], "git")
        with self.assertRaisesRegex(ValueError, "nonrepeatable"):
            self.schedule("C12", "C")

    def test_canonical_init_requires_declared_and_actual_empty_namespace(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-empty-test-", dir="/tmp") as folder:
            root = Path(folder)
            with self.assertRaisesRegex(ValueError, "declared"):
                author.require_empty(root, {})
            author.require_empty(root, {"fixture_kind":"empty"})
            (root / ".git").mkdir()
            with self.assertRaisesRegex(ValueError, "actual verified root empty"):
                author.require_empty(root, {"fixture_kind":"empty"})


class Specification(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def spec(self, case, *, assets=True, replay=True):
        inputs = {"fixture_identity": {"synthetic": "never a sealed product fixture"},
                  "roots": [{"role": "primary", "path": "/native", "inventory": "/code/native.jsonl",
                             "inventory_sha256": "1" * 64, "content_metadata_set_sha256": "2" * 64}],
                  "code_assets": {"node-roots.json": "3" * 64} if assets else {},
                  "external": [{"path": "/replay", "inventory": "/code/replay.jsonl",
                                "inventory_sha256": "4" * 64, "content_metadata_set_sha256": "5" * 64}] if replay else []}
        source = self.root / (case + ".inputs.json")
        source.write_text(json.dumps(inputs))
        output = self.root / (case + ".spec.json")
        result = author.emit_spec(SimpleNamespace(case=case, cache_class="NONE", inputs=source,
                                                 inputs_sha256=author.sha(source), output=output))
        return result, json.loads(output.read_text())

    def test_large_commit_specs_bind_registered_body_inputs_and_frozen_limits(self):
        for case in ("K02", "K03"):
            result, spec = self.spec(case)
            self.assertEqual(result["status"], "DECLARED_SETUP_ONLY")
            self.assertEqual(spec["case"]["workload"], CASES[case]["workload"])
            self.assertEqual(spec["case"]["complete_command_wall_stop_ns"], 15_000_000_000)
            self.assertEqual(spec["verifier_wall_stop_ns"], 9_500_000_000)
            self.assertEqual(spec["environment"]["LAYERFS_CONSTRUCTION_WORKERS"], "1")
            self.assertEqual(spec["inputs"]["external"][0]["path"], "/replay")
            self.assertIn("oracle_prepare.py", spec["helpers"])

    def test_large_commit_requires_closed_replay_and_node_roots(self):
        with self.assertRaisesRegex(ValueError, "code-data"):
            self.spec("K02", assets=False)
        with self.assertRaisesRegex(ValueError, "replay"):
            self.spec("K03", replay=False)

    def test_generated_verifier_observes_and_compares_correct_payload_case(self):
        row = author.verifier_script(self.root, "/code/oracles/synthetic", author.observer_for("K03"), "checkpoint-1")
        text = (self.root / "checkpoint-1.verify.sh").read_text()
        self.assertIn("observe --root", text)
        self.assertIn("compare --case E13", text)
        self.assertIn("/code/oracles/synthetic/checkpoint-1.jsonl", text)
        self.assertIn('"$PWD"', text)
        self.assertIn("secrets.token_hex(16)", text)
        self.assertTrue(row["performs_comparison"])
        self.assertEqual(row["wall_stop_ns"], 9_500_000_000)
        self.assertEqual(text, author.verifier_body("/code/oracles/synthetic", author.observer_for("K03"), "checkpoint-1"))

    def test_git_verifier_binds_all_expected_operands_without_mutable_git_scope(self):
        operands = {"manifest": "expected.index.json", "manifest_sha256": "1" * 64,
                    "pin": "git-queries/pin.json", "pin_sha256": "2" * 64,
                    "policy": "git-default-policy.json", "policy_sha256": "3" * 64,
                    "tree_sha256": "4" * 64}
        text = author.verifier_body("/code/oracles/git", author.observer_for("E18"), "expected", operands)
        self.assertIn("python3 -B /code/git_queries.py verify", text)
        self.assertIn("--case E18", text)
        self.assertIn("--expected-index /code/oracles/git/expected.index.json", text)
        self.assertIn("--pin /code/oracles/git/git-queries/pin.json", text)
        self.assertIn("--policy /code/oracles/git/git-default-policy.json", text)
        for value in ("1" * 64, "2" * 64, "3" * 64, "4" * 64):
            self.assertIn(value, text)
        self.assertNotIn("git status", text)
        with self.assertRaisesRegex(ValueError, "operands"):
            author.verifier_body("/code/oracles/git", author.observer_for("E04"), "expected")

    def test_git_spec_requires_qualified_policy_asset_and_extended_helper_closure(self):
        inputs = {"fixture_identity": {"synthetic": "not a product fixture"},
                  "roots": [{"role": "primary", "path": "/native", "inventory": "/code/native.jsonl",
                             "inventory_sha256": "1" * 64, "content_metadata_set_sha256": "2" * 64}],
                  "external": [], "code_assets": {"git-policy.json": "3" * 64},
                  "git_default_policy": {"path": "/code/git-policy.json", "sha256": "3" * 64}}
        path = self.root / "git.inputs.json"
        path.write_text(json.dumps(inputs))
        output = self.root / "git.spec.json"
        author.emit_spec(SimpleNamespace(case="E04", cache_class="A", inputs=path,
                                         inputs_sha256=author.sha(path), output=output))
        spec = json.loads(output.read_text())
        self.assertEqual(spec["status"], "DECLARED_SETUP_ONLY")
        self.assertEqual(set(spec["helpers"]), {"oracle.py", "workload.py", "r7_deployment.py",
                                                 "oracle_prepare.py", "git_queries.py", "git_index_oracle.py", "workloads.py"})
        inputs["code_assets"] = {}
        path.write_text(json.dumps(inputs))
        with self.assertRaisesRegex(ValueError, "policy asset"):
            author.emit_spec(SimpleNamespace(case="E18", cache_class="A", inputs=path,
                                             inputs_sha256=author.sha(path), output=self.root / "missing.spec.json"))

    def test_mutable_specs_require_sealed_tracked_or_empty_input_and_exact_body(self):
        inputs = dict(fixture_identity={"synthetic":"test only"},roots=[dict(role="primary",path="/native",inventory="/code/native.jsonl",
            inventory_sha256="1"*64,content_metadata_set_sha256="2"*64)],external=[],code_assets={"policy.json":"3"*64},
            git_default_policy=dict(path="/code/policy.json",sha256="3"*64))
        path = self.root / "mutable.inputs.json"
        for case in ("E10","E11","C12"):
            if case == "E11":
                inputs["code_assets"]["tracked-paths.json"] = "4"*64
            if case == "C12":
                inputs["fixture_identity"]["fixture_kind"] = "empty"
            path.write_text(json.dumps(inputs))
            output = self.root / (case+".spec.json")
            result = author.emit_spec(SimpleNamespace(case=case,cache_class="B",inputs=path,inputs_sha256=author.sha(path),output=output))
            self.assertEqual(result["status"],"DECLARED_SETUP_ONLY")
            self.assertEqual(json.loads(output.read_text())["case"]["workload"]["argv"],CASES[case]["workload"]["argv"])
        del inputs["fixture_identity"]["fixture_kind"]
        path.write_text(json.dumps(inputs))
        with self.assertRaisesRegex(ValueError,"empty fixture"):
            author.emit_spec(SimpleNamespace(case="C12",cache_class="B",inputs=path,inputs_sha256=author.sha(path),output=self.root / "bad-empty.spec"))
        del inputs["code_assets"]["tracked-paths.json"]
        path.write_text(json.dumps(inputs))
        with self.assertRaisesRegex(ValueError,"code-data"):
            author.emit_spec(SimpleNamespace(case="E11",cache_class="B",inputs=path,inputs_sha256=author.sha(path),output=self.root / "bad-tracked.spec"))


class HostStdout(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / "bundle"
        self.bundle.mkdir()

    def compare(self, expected, actual, *, kind="exact-file-sha256", event_change=None, recipe_change=None):
        want, got = self.bundle / "expectation.stdout", self.root / "original.stdout"
        want.write_bytes(expected)
        got.write_bytes(actual)
        recipe = {"kind": kind, "expected_bundle_relative": want.name, "expected_sha256": author.sha(want), "required": True}
        if recipe_change:
            recipe_change(recipe)
        closed = self.bundle / "closed.json"
        closed.write_text(json.dumps({"status": "CLOSED_EXPECTED_SETUP_ONLY", "host_stdout_recipe": recipe}))
        event = {"event": "command", "fields": {"exit_code": "0", "registered_execs": "0", "stdout": str(got)}}
        if event_change:
            event_change(event)
        event_path = self.root / "original.event.json"
        event_path.write_text(json.dumps(event))
        return author.compare_stdout(SimpleNamespace(closed=closed, bundle=self.bundle, actual_event=event_path))

    def test_exact_original_host_file_pass_and_full_byte_mismatch_fail(self):
        self.assertEqual(self.compare(b"14104\n", b"14104\n")["status"], "PASS")
        self.assertEqual(self.compare(b"14104\n", b"14103\n")["status"], "FAIL")

    def test_parallel_complete_line_order_is_unpriced_and_content_is_exact(self):
        expected = b'42\n{"bytes_written": 1000000, "entries_completed": 10000}\n'
        actual = b'{"bytes_written": 1000000, "entries_completed": 10000}\n42\n'
        self.assertEqual(self.compare(expected, actual, kind="unordered-complete-lines")["status"], "PASS")
        self.assertEqual(self.compare(expected, actual.replace(b"1000000", b"1000001"), kind="unordered-complete-lines")["status"], "FAIL")

    def test_parallel_partial_missing_or_extra_child_output_refuses(self):
        for actual in (b"42\n", b"42\nwriter", b"42\nwriter\nextra\n", b"x" * (author.WINDOW + 1)):
            with self.assertRaises(ValueError):
                self.compare(b"42\nwriter\n", actual, kind="unordered-complete-lines")

    def test_wrong_original_event_exit_registration_or_host_path_refuses(self):
        mutations = [lambda row: row.update(event="verify"),
                     lambda row: row["fields"].update(exit_code="1"),
                     lambda row: row["fields"].update(registered_execs="1"),
                     lambda row: row["fields"].update(stdout="relative.stdout"),
                     lambda row: row["fields"].update(stdout="/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness/forbidden")]
        for mutation in mutations:
            with self.assertRaises(ValueError):
                self.compare(b"same\n", b"same\n", event_change=mutation)

    def test_seal_unknown_recipe_and_bundle_escape_refuse(self):
        for mutation in (lambda row: row.update(expected_sha256="0" * 64),
                         lambda row: row.update(kind="guess-an-output"),
                         lambda row: row.update(expected_bundle_relative="../original.stdout")):
            with self.assertRaises(ValueError):
                self.compare(b"same\n", b"same\n", recipe_change=mutation)

    def test_original_host_stdout_leaf_alias_is_refused_before_payload_read(self):
        link = self.root / "alias.stdout"
        def use_alias(event):
            link.symlink_to(event["fields"]["stdout"])
            event["fields"]["stdout"] = str(link)
        with self.assertRaisesRegex(ValueError, "regular file"):
            self.compare(b"same\n", b"same\n", event_change=use_alias)


class FailureCustody(unittest.TestCase):
    def test_original_hash_read_error_keeps_phase_and_prior_close_failures(self):
        original = OSError("exact original payload read")
        original.original_phase = "already recorded original read phase"
        original.independent_close_failures = ["earlier source close"]
        stream = mock.Mock()
        stream.read.side_effect = original
        stream.close.side_effect = OSError("independent hash reader close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                author.sha("test-owned-unused-payload")
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "already recorded original read phase")
        self.assertEqual(original.independent_close_failures, ["earlier source close", "independent hash reader close"])
        stream.read.assert_called_once_with(author.WINDOW)
        stream.close.assert_called_once_with()

    def test_original_receipt_write_error_and_close_error_do_not_resend(self):
        original = OSError("exact original receipt write")
        stream = mock.Mock()
        stream.write.side_effect = original
        stream.close.side_effect = OSError("independent receipt close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                author.write_new("test-owned-unused-output", {"attempt": 1})
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "receipt_write")
        self.assertEqual(original.independent_close_failures, ["independent receipt close"])
        stream.write.assert_called_once()
        stream.close.assert_called_once_with()

    def test_json_decode_failure_is_not_replaced_by_reader_close(self):
        stream = mock.Mock()
        stream.read.return_value = b"{malformed original input"
        stream.close.side_effect = OSError("independent JSON reader close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(json.JSONDecodeError) as caught:
                author.read_json("test-owned-unused-input")
        self.assertEqual(caught.exception.original_phase, "closed_json_read")
        self.assertEqual(caught.exception.independent_close_failures, ["independent JSON reader close"])
        stream.read.assert_called_once_with()
        stream.close.assert_called_once_with()

    def test_payload_read_and_both_closes_keep_exact_first_cause(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp") as directory:
            folder = Path(directory)
            root = folder / "view"
            root.mkdir()
            payload = root / "experiment-large"
            payload.write_bytes(b"known bytes")
            output = folder / "actual.jsonl"
            original = OSError("exact original known payload read")
            reader, writer = mock.Mock(), mock.Mock()
            reader.read.side_effect = original
            reader.close.side_effect = OSError("independent payload close")
            writer.close.side_effect = OSError("independent manifest close")
            def opened(path, *args, **kwargs):
                self.assertEqual(kwargs.get("buffering"), 0)
                self.assertIn(path, {payload.resolve(), output})
                return reader if path == payload.resolve() else writer
            with mock.patch.object(author, "load_helper", return_value=oracle), \
                    mock.patch.object(Path, "open", opened):
                with self.assertRaises(OSError) as caught:
                    author.observe_known(root, "large", output)
            self.assertIs(caught.exception, original)
            self.assertEqual(original.original_phase, "payload_hash_read")
            self.assertEqual(original.independent_close_failures, ["independent payload close", "independent manifest close"])
            reader.read.assert_called_once_with(author.WINDOW)
            reader.close.assert_called_once_with()
            writer.close.assert_called_once_with()
            writer.write.assert_not_called()

    def test_known_manifest_write_error_is_not_replaced_by_close(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp") as directory:
            folder = Path(directory)
            root = folder / "view"
            root.mkdir()
            (root / "experiment-large").write_bytes(b"known bytes")
            output = folder / "actual.jsonl"
            original = OSError("exact original known manifest write")
            writer = mock.Mock()
            writer.write.side_effect = original
            writer.close.side_effect = OSError("independent manifest close")
            actual_open = Path.open
            def opened(path, *args, **kwargs):
                return writer if path == output else actual_open(path, *args, **kwargs)
            with mock.patch.object(author, "load_helper", return_value=oracle), \
                    mock.patch.object(Path, "open", opened):
                with self.assertRaises(OSError) as caught:
                    author.observe_known(root, "large", output)
            self.assertIs(caught.exception, original)
            self.assertEqual(original.original_phase, "known_manifest_write")
            self.assertEqual(original.independent_close_failures, ["independent manifest close"])
            writer.write.assert_called_once()
            writer.close.assert_called_once_with()

    def test_close_only_failure_is_original_and_stops_owner(self):
        original = OSError("original reader close failed")
        stream = mock.Mock()
        stream.read.return_value = b""
        stream.close.side_effect = original
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                author.sha("test-owned-unused-payload")
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "payload_hash_read_close")
        stream.close.assert_called_once_with()

    def test_output_close_appends_prior_independent_errors_without_masking_original(self):
        original = OSError("original payload error")
        original.independent_close_failures = ["prior source close"]
        stream = mock.Mock()
        stream.close.side_effect = OSError("independent receipt close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                with author.OutputFile("unused"):
                    raise original
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_close_failures, ["prior source close", "independent receipt close"])
        stream.close.assert_called_once_with()

    def test_short_receipt_write_has_no_resend_and_retains_close_failure(self):
        stream = mock.Mock()
        stream.write.return_value = 1
        stream.close.side_effect = OSError("independent close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaisesRegex(ValueError, "short original receipt write") as caught:
                author.write_new("unused", {"declared": "one attempt"})
        self.assertEqual(stream.write.call_count, 1)
        self.assertEqual(caught.exception.independent_close_failures, ["independent close"])

    def test_failed_original_wait_and_receipt_output_keep_separate_causes(self):
        original = OSError("original wait failed")
        process = mock.Mock(pid=77)
        process.wait.side_effect = original
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp") as directory:
            writes = []
            def receipt(path, value):
                writes.append(Path(path).name)
                if str(path).endswith(".result.json"):
                    raise OSError("independent result output failed")
            with mock.patch.object(author.subprocess, "Popen", return_value=process) as launched, \
                    mock.patch.object(author, "write_new", side_effect=receipt), \
                    mock.patch.object(author.time, "monotonic", return_value=1), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(author.ReferenceFailure) as caught:
                    author.invoke(["/bin/bash", "-o", "pipefail", "-c", "true"], Path(directory), Path(directory),
                                  "original", {}, 10, 3, [])
            self.assertIs(caught.exception.original, original)
            self.assertEqual(caught.exception.origin_phase, "command_wait")
            self.assertEqual(caught.exception.custody["pid"], 77)
            self.assertEqual(caught.exception.output_errors, ["independent result output failed"])
            self.assertEqual(writes, ["original.attempt.json", "original.result.json"])
            launched.assert_called_once()
            process.wait.assert_called_once()

    def test_timeout_has_one_original_wait_one_cancel_and_no_exit_guess(self):
        expired = subprocess.TimeoutExpired("declared original", 3)
        process = mock.Mock(pid=79)
        process.wait.side_effect = [expired, subprocess.TimeoutExpired("cancellation custody", 1)]
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp") as directory:
            with mock.patch.object(author.subprocess, "Popen", return_value=process) as launched, \
                    mock.patch.object(author.os, "killpg") as cancelled, \
                    mock.patch.object(author.time, "monotonic", return_value=1), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(author.ReferenceFailure) as caught:
                    author.invoke(["/bin/bash", "-o", "pipefail", "-c", "true"], Path(directory), Path(directory),
                                  "timed", {}, 10, 3, [])
            self.assertIs(caught.exception.original, expired)
            self.assertEqual(caught.exception.custody["exit_after_cancel"], "UNAVAILABLE")
            self.assertNotIn("exit_code", caught.exception.custody)
            self.assertEqual(process.wait.call_count, 2)
            cancelled.assert_called_once_with(79, author.signal.SIGKILL)
            launched.assert_called_once()


class AuthorClosure(unittest.TestCase):
    """Exercise the author boundary with explicit test-only staging/I/O doubles."""
    def run_author(self, failure_label=None, case="K04"):
        temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        folder = Path(temporary.name).resolve()
        root = folder / "reference"
        root.mkdir()
        inventory = folder / "native.inventory.jsonl"
        inventory.write_text('{"synthetic":"not a staging proof"}\n')
        physical = root.stat()
        inputs = {"fixture_identity": {"synthetic": "not a product fixture"},
                  "roots": [{"role": "primary", "path": "/native", "inventory": str(inventory),
                             "inventory_sha256": author.sha(inventory), "content_metadata_set_sha256": "1" * 64}],
                  "external": [], "code_assets": {}, "precondition_identity": None}
        if case == "C12":
            inputs["fixture_identity"]["fixture_kind"] = "empty"
        if case in author.GIT_CASES:
            policy = folder / "qualified-policy.json"
            policy.write_text('{"synthetic":"only a controller double, never a qualified policy"}')
            inputs["git_default_policy"] = dict(path=str(policy),sha256=author.sha(policy))
        verification = {"root": str(root), "root_device": physical.st_dev, "root_inode": physical.st_ino,
                        "inventory_sha256": author.sha(inventory), "content_metadata_set_sha256": "1" * 64}
        spec = {"schema": "r7-oracle-preparation-spec-v1", "status": "DECLARED_SETUP_ONLY", "reason": None,
                "image_id": author.IMAGE, "uid": 501, "gid": 20, "setup_wall_stop_seconds": 300,
                "case_id": case, "cache_class": None if case.startswith("K") else "B", "case": CASES[case], "inputs": inputs,
                "helpers": {}, "environment": dict(workloads.ENV), "registry_sha256": "2" * 64,
                "workload_source_sha256": "3" * 64}
        if case in author.GIT_CASES:
            spec.update(oracle_variant="r7-git-index-scoped-v2",registry_variant_identity="5"*64,registry_variant_source_sha256="6"*64)
        staging = {"schema": "r7-container-staging-v1", "status": "PASS", "container": "4" * 64,
                   "image_id": author.IMAGE, "uid": 501, "gid": 20, "helper_sha256": {}, "verifications": [verification]}
        spec_path, staging_path = folder / "spec.json", folder / "staging.json"
        spec_path.write_text(json.dumps(spec))
        staging_path.write_text(json.dumps(staging))
        output = folder / "output"
        args = SimpleNamespace(spec=spec_path, spec_sha256=author.sha(spec_path), staging_receipt=staging_path,
                               staging_receipt_sha256=author.sha(staging_path), container="4" * 64,
                               asset_root="/code/oracles/synthetic", setup_wall_seconds=300, output=output)
        calls, claims = [], []
        original_stat, original_write = Path.stat, author.write_new
        def stat_for_declared_owner(path, *args, **kwargs):
            info = original_stat(path, *args, **kwargs)
            if path == root:
                return SimpleNamespace(st_uid=501, st_gid=20, st_dev=info.st_dev, st_ino=info.st_ino)
            return info
        def write(path, value):
            if str(path).startswith("/tmp/layerfs-r7-oracle-reference-"):
                claims.append((path, value))
                path = folder / "test-owned-reference.claim"
            return original_write(path, value)
        def invoke(argv, selected_root, destination, label, environment, deadline, allowance, steps):
            calls.append((label, list(argv), selected_root, allowance))
            step = {"label": label, "stdout": str(destination / (label + ".stdout")), "pid": 91}
            steps.append(step)
            if label == failure_label:
                original = OSError("exact synthetic original operation failure")
                raise author.ReferenceFailure(original, step, ["separate synthetic receipt error"], "command_wait")
            Path(step["stdout"]).write_bytes(b"")
            if label == "git-context":
                evidence = destination / "git-queries"
                evidence.mkdir()
                pin = evidence / "pin.json"
                pin.write_text('{"synthetic":"controller recipe only"}')
                queries = evidence / "queries.json"
                queries.write_text('{}')
                Path(step["stdout"]).write_text(json.dumps(dict(schema="r7-original-git-queries-v1",status="OBSERVED",pin={"synthetic":True},
                    pin_path=str(pin),pin_file_sha256=author.sha(pin),queries_path=str(queries),queries_sha256=author.sha(queries))))
            elif case in author.GIT_CASES and label == "expected-observer":
                (destination / "expected.jsonl").write_text('{}\n')
                (destination / "expected.index.json").write_text('{}\n')
            elif argv[0] == "python3":
                Path(argv[argv.index("--output") + 1]).write_text('{"synthetic":"expected observer row"}\n')
            return step
        deployment = SimpleNamespace(verify_tree=lambda *args: {"status": "PASS"})
        with mock.patch.object(author.os, "uname", return_value=SimpleNamespace(sysname="Linux")), \
                mock.patch.object(author.os, "geteuid", return_value=501), \
                mock.patch.object(author, "ordinary_identity"), \
                mock.patch.object(author, "native", return_value=root), \
                mock.patch.object(author, "inventory_path", side_effect=lambda value: Path(value)), \
                mock.patch.object(Path, "stat", stat_for_declared_owner), \
                mock.patch.object(author, "load_helper", return_value=deployment), \
                mock.patch.object(author, "write_new", side_effect=write), \
                mock.patch.object(author, "invoke", side_effect=invoke):
            result = author.author(args)
        return result, inputs, calls, claims, output

    def test_closed_result_retains_selected_inputs_and_every_checkpoint_identity(self):
        result, inputs, calls, claims, output = self.run_author()
        self.assertEqual(result["status"], "CLOSED_EXPECTED_SETUP_ONLY")
        self.assertEqual(result["inputs"], inputs)
        self.assertEqual(json.loads((output / "closed.json").read_text())["inputs"], inputs)
        self.assertEqual(len(claims), 1)
        self.assertEqual(len(calls), 10)
        self.assertEqual([row["checkpoint_index"] for row in result["expected"]], [1, 2, 3, 4, 5])
        for index, row in enumerate(result["expected"], 1):
            self.assertEqual(row["original_reference_commands"], ["expectation-" + str(index)])
            self.assertEqual(row["reference_root"], str(output.parent / "reference"))
            self.assertEqual(row["role"], "primary")
        self.assertEqual([call[3] for call in calls], [15.0, 9.5] * 5)
        self.assertFalse(result["admission_eligible"])
        self.assertIn("never N control/sample", result["performance_claim"])

    def test_original_observer_failure_stops_schedule_and_survives_independent_output_error(self):
        result, _, calls, claims, output = self.run_author("checkpoint-1-observer")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual([call[0] for call in calls], ["expectation-1", "checkpoint-1-observer"])
        self.assertEqual(result["original_observer_failure"]["cause"], "exact synthetic original operation failure")
        self.assertEqual(result["original_observer_failure"]["custody"]["pid"], 91)
        self.assertEqual(result["independent_output_failures"], ["separate synthetic receipt error"])
        self.assertEqual(result["expected"], [])
        self.assertEqual(len(claims), 1)
        self.assertTrue((output / "closed.json").exists())

    def test_c12_context_follows_one_whole_init_commit_body_and_precedes_observer(self):
        result, _, calls, _, _ = self.run_author(case="C12")
        self.assertEqual(result["status"], "CLOSED_EXPECTED_SETUP_ONLY")
        self.assertEqual([call[0] for call in calls], ["expectation","git-context","expected-observer"])
        self.assertEqual(calls[0][1], CASES["C12"]["workload"]["argv"])
        self.assertEqual(result["git_context"]["query_position"], "after original canonical body")
        self.assertIn("--case", calls[1][1])
        self.assertEqual(calls[1][1][calls[1][1].index("--case")+1], "C12")

    def test_mutable_context_failure_prevents_original_body_and_all_observers(self):
        result, _, calls, _, _ = self.run_author("git-context",case="E10")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual([call[0] for call in calls], ["git-context"])
        self.assertEqual(result["expected"], [])
        self.assertEqual(result["original_query_failure"]["cause"], "exact synthetic original operation failure")


class PayloadScope(unittest.TestCase):
    def test_k03_compare_port_requires_alias_relation_and_exact_payload(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-oracle-test-", dir="/tmp") as directory:
            folder = Path(directory)
            want, got = folder / "want.jsonl", folder / "got.jsonl"
            rows = [{"path": path, "kind": "regular", "mode": 0o644, "uid": 501, "gid": 20,
                     "size": 3, "nlink": 2, "device": 11, "inode": 12, "sha256": "a" * 64}
                    for path in ("node_modules/pkg/file", ".experiment-store/7")]
            def save(path, values):
                path.write_text("".join(json.dumps(row) + "\n" for row in values))
            save(want, rows)
            actual = [dict(row, device=21, inode=22) for row in rows]
            save(got, actual)
            self.assertEqual(oracle.compare(want, got, author.observer_for("K03")["compare_case"])["status"], "PASS")
            actual[1]["inode"] = 23
            save(got, actual)
            comparison = oracle.compare(want, got, "E13")
            self.assertEqual(comparison["status"], "FAIL")
            self.assertIn({"reason": "hard-link equivalence classes"}, comparison["differences"])
            actual[1]["inode"] = 22
            actual[0]["sha256"] = "b" * 64
            save(got, actual)
            self.assertEqual(oracle.compare(want, got, "E13")["status"], "FAIL")


if __name__ == "__main__":
    unittest.main()

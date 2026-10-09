"""External Git-query and paired-index recipe proofs; Git never executes here."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import git_index_oracle as index, git_queries, oracle, workloads
from r7.test_git_index_oracle import entry, index as index_bytes


class Queries(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-git-query-test-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name).resolve()
        self.root = self.folder / "view"
        (self.root / ".git").mkdir(parents=True)
        (self.root / ".git/config").write_bytes(b"synthetic config; never passed to Git")
        (self.root / ".git/index").write_bytes(index_bytes([entry()]))
        self.binary = self.folder / "git"
        self.binary.write_bytes(b"synthetic executable; never launched")
        self.policy = dict(schema="r7-git-default-policy-v1", version="git version 2.39.5",
                           git_binary_sha256=index.file_digest(self.binary), defaults=dict(git_queries.DEFAULTS),
                           primary_sources=[dict(url="https://raw.githubusercontent.com/git/git/v2.39.5/config.c", sha256="a" * 64)],
                           review_receipt_sha256="b" * 64)
        self.calls = []

    def popen(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        if argv[-1] == "--version":
            data, code = b"git version 2.39.5\n", 0
        elif argv[-1] == "--show-object-format":
            data, code = b"sha1\n", 0
        elif argv[-1] == "core.fsmonitor":
            data, code = b"false\n", 0
        else:
            data, code = b"", 1
        kwargs["stdout"].write(data)
        process = mock.Mock(pid=9000 + len(self.calls))
        process.wait.return_value = code
        return process

    def collect(self, name="queries"):
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen", side_effect=self.popen):
            return git_queries.collect(self.root, self.policy, self.folder / name)

    def test_all13_original_queries_once_with_literal_absent_keep_and_pid_receipts(self):
        result = self.collect()
        self.assertEqual(len(self.calls), 13)
        self.assertEqual([row["pid"] for row in result["queries"]], list(range(9001, 9014)))
        self.assertTrue(all(row["attempts"] == 1 and "exit_code" in row for row in result["queries"]))
        self.assertEqual(result["pin"]["effective_config"]["core.untrackedcache"], "keep")
        receipt = json.loads(Path(result["queries_path"]).read_text())
        self.assertEqual(receipt["original_values"]["core.untrackedcache"],
                         {"original": "ABSENT", "exit_code": 1, "qualified_literal_default": "keep"})
        for argv, kwargs in self.calls[1:]:
            self.assertEqual(argv[1:3], ["-c", "core.fsmonitor=false"])
            self.assertNotIn("--global", argv)
            self.assertEqual(kwargs["cwd"], self.root)
            self.assertEqual(kwargs["env"], workloads.ENV)
        for row in result["queries"]:
            prefix = Path(row["stdout"]).with_suffix("")
            self.assertEqual(json.loads(prefix.with_suffix(".ack.json").read_text())["pid"], row["pid"])

    def test_mutable_queries_do_not_mask_actual_fsmonitor_configuration(self):
        for case in ("E10", "E11", "C12"):
            with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                    mock.patch.object(git_queries.subprocess, "Popen", side_effect=self.popen):
                result = git_queries.collect(self.root,self.policy,self.folder / case,case=case)
            rows = result["queries"]
            self.assertEqual(len(rows),13)
            self.assertTrue(all("-c" not in row["argv"] for row in rows))
            self.assertEqual(json.loads(Path(result["queries_path"]).read_text())["command_override"],{})
        def unsupported(argv, **kwargs):
            if argv[-1] != "core.fsmonitor":
                return self.popen(argv,**kwargs)
            kwargs["stdout"].write(b"true\n")
            return mock.Mock(pid=9281,wait=mock.Mock(return_value=0))
        with mock.patch.object(git_queries.shutil,"which",return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess,"Popen",side_effect=unsupported):
            with self.assertRaises(git_queries.QueryFailure):
                git_queries.collect(self.root,self.policy,self.folder / "unsupported-fsmonitor",case="E10")

    def test_unreviewed_case_and_unsealed_tracked_input_never_launch_queries(self):
        with mock.patch.object(git_queries.subprocess,"Popen") as launched:
            with self.assertRaisesRegex(ValueError,"reviewed"):
                git_queries.collect(self.root,self.policy,self.folder / "unreviewed",case="E19")
            with self.assertRaisesRegex(ValueError,"sealed tracked"):
                git_queries.verify(SimpleNamespace(case="E11"))
        launched.assert_not_called()

    def test_standalone_import_has_all_staged_dependencies(self):
        staged = self.folder / "code"
        staged.mkdir()
        source = Path(git_queries.__file__).parent
        for name in ("git_queries.py", "git_index_oracle.py", "oracle.py", "workloads.py"):
            (staged / name).write_bytes((source / name).read_bytes())
        with mock.patch.dict(sys.modules):
            for name in ("git_index_oracle", "oracle", "workloads"):
                sys.modules.pop(name, None)
            with mock.patch.object(sys, "path", [str(staged), *[path for path in sys.path if path != str(source)]]):
                spec = importlib.util.spec_from_file_location("standalone_git_queries", staged / "git_queries.py")
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                self.assertEqual(Path(module.workloads.__file__).parent, staged)
                self.assertEqual(module.workloads.ENV, workloads.ENV)

    def test_wrong_policy_or_binary_is_refused_before_any_query(self):
        wrong = dict(self.policy, git_binary_sha256="0" * 64)
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen") as launched:
            with self.assertRaises(git_queries.QueryFailure):
                git_queries.collect(self.root, wrong, self.folder / "wrong")
        launched.assert_not_called()
        with self.assertRaisesRegex(ValueError, "qualified exact"):
            git_queries.policy_shape(dict(self.policy, defaults={}))

    def test_query_timeout_retains_actual_pid_and_no_known_exit_or_cancel(self):
        timeout = subprocess.TimeoutExpired([str(self.binary), "--version"], 9.5)
        process = mock.Mock(pid=971)
        process.wait.side_effect = timeout
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen", return_value=process) as launched:
            with self.assertRaises(git_queries.QueryFailure) as caught:
                git_queries.collect(self.root, self.policy, self.folder / "timeout")
        self.assertIs(caught.exception.original, timeout)
        self.assertEqual(caught.exception.queries[0]["pid"], 971)
        self.assertNotIn("exit_code", caught.exception.queries[0])
        self.assertEqual(caught.exception.original_phase, "original_query_wait")
        launched.assert_called_once()
        process.wait.assert_called_once()
        process.kill.assert_not_called()
        process.terminate.assert_not_called()

    def test_absence_requires_exit1_empty_output_and_empty_stderr(self):
        for code, stdout, stderr in ((2, b"", b""), (1, b"wrong\n", b""), (1, b"", b"original error\n")):
            directory = self.folder / ("absence-" + str(code) + "-" + str(len(stdout)) + "-" + str(len(stderr)))
            directory.mkdir()
            def failed(argv, **kwargs):
                kwargs["stdout"].write(stdout)
                kwargs["stderr"].write(stderr)
                process = mock.Mock(pid=975)
                process.wait.return_value = code
                return process
            with mock.patch.object(git_queries.subprocess, "Popen", side_effect=failed):
                with self.assertRaises(ValueError):
                    git_queries.query(self.root, directory, "field", [str(self.binary)], workloads.ENV,
                                      git_queries.time.monotonic() + 9.5, [], missing=True)

    def test_expired_deadline_has_zero_query_launches(self):
        with mock.patch.object(git_queries.subprocess, "Popen") as launched:
            with self.assertRaises(TimeoutError):
                git_queries.collect(self.root, self.policy, self.folder / "expired", deadline=0)
        launched.assert_not_called()

    def test_actual_context_preserves_witnesses_and_rejects_semantic_differences(self):
        pin = self.collect()["pin"]
        actual = copy.deepcopy(pin)
        actual["effective_config_receipt_sha256"] = "c" * 64
        actual["git"]["version_receipt_sha256"] = "d" * 64
        git_queries.same_context(pin, actual)
        self.assertEqual(actual["effective_config_receipt_sha256"], "c" * 64)
        actual["git"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "context differs"):
            git_queries.same_context(pin, actual)

    def test_original_wait_error_plus_both_closes_keeps_exact_first_cause(self):
        original = OSError("exact original query wait")
        stdout, stderr = mock.Mock(), mock.Mock()
        stdout.close.side_effect = OSError("independent query stdout close")
        stderr.close.side_effect = OSError("independent query stderr close")
        process = mock.Mock(pid=979)
        process.wait.side_effect = original
        queries = []
        with mock.patch.object(index, "write_new"), \
                mock.patch.object(Path, "open", side_effect=[stdout, stderr]), \
                mock.patch.object(git_queries.subprocess, "Popen", return_value=process):
            with self.assertRaises(OSError) as caught:
                git_queries.query(self.root, self.folder, "first", [str(self.binary)], workloads.ENV,
                                  git_queries.time.monotonic()+9.5, queries)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.original_phase, "original_query_wait")
        self.assertEqual(original.independent_close_failures,
                         ["independent query stderr close", "independent query stdout close"])
        self.assertEqual(queries[0]["pid"], 979)

    def test_complete_verify_compares_tree_and_index_with_only_cache_words_excluded(self):
        expected_pin = self.collect()["pin"]
        pin_path, policy_path = self.folder / "expected.pin.json", self.folder / "policy.json"
        index.write_new(pin_path, expected_pin)
        index.write_new(policy_path, self.policy)
        expected_tree, expected_index = self.folder / "expected.tree.jsonl", self.folder / "expected.index.json"
        oracle.observe(self.root, "E04", expected_tree)
        observation = index.observe(self.root, expected_pin)
        observation["root"] = "/native-reference-other-container"
        expected_index.write_text(json.dumps(observation))
        cached = [11, 21, 31, 41, 51, 61, 0o100644, 502, 21, 8]
        (self.root / ".git/index").write_bytes(index_bytes([entry(cache=cached)]))
        args = SimpleNamespace(root=self.root, case="E04", pin=pin_path, pin_sha256=index.file_digest(pin_path),
                               policy=policy_path, policy_sha256=index.file_digest(policy_path), expected_tree=expected_tree,
                               expected_tree_sha256=index.file_digest(expected_tree), expected_index=expected_index,
                               expected_index_sha256=index.file_digest(expected_index), output=self.folder / "actual-verify")
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen", side_effect=self.popen):
            result = git_queries.verify(args)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["expected_operands"]["index_sha256"], args.expected_index_sha256)
        comparison = json.loads(Path(result["comparison_artifact"]).read_text())
        self.assertEqual(comparison["tree"]["differences"], [])
        self.assertEqual(comparison["index"]["differences"], [])
        self.assertNotEqual(comparison["index"]["expected_raw_sha256"], comparison["index"]["actual_raw_sha256"])
        (self.root / ".git/index").write_bytes(index_bytes([entry(oid=b"b" * 20)]))
        args.output = self.folder / "actual-semantic-mismatch"
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen", side_effect=self.popen):
            self.assertEqual(git_queries.verify(args)["status"], "FAIL")
        (self.root / ".git/index").write_bytes(index_bytes([entry(cache=cached)]))
        args.output = self.folder / "declared-tree-mutation"
        actual_collect = git_queries.collect
        def changed_after_initial_seal(*values, **options):
            result = actual_collect(*values, **options)
            rows = [json.loads(line) for line in expected_tree.read_text().splitlines()]
            expected_tree.write_text("".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows))
            return result
        with mock.patch.object(git_queries.shutil, "which", return_value=str(self.binary)), \
                mock.patch.object(git_queries.subprocess, "Popen", side_effect=self.popen), \
                mock.patch.object(git_queries, "collect", side_effect=changed_after_initial_seal):
            with self.assertRaisesRegex(ValueError, "declared expected generic tree operand changed"):
                git_queries.verify(args)

    def test_e11_sealed_tracked_bytes_and_c12_complete_root_are_compared(self):
        tracked = self.folder / "tracked.json"
        tracked.write_text('["tracked.txt"]')
        (self.root / "tracked.txt").write_bytes(b"known tracked payload")
        for case in ("E10","E11","C12"):
            with mock.patch.object(git_queries.shutil,"which",return_value=str(self.binary)), \
                    mock.patch.object(git_queries.subprocess,"Popen",side_effect=self.popen):
                pin = git_queries.collect(self.root,self.policy,self.folder / (case+"-expected-queries"),case=case)["pin"]
            pin_path,policy_path = self.folder / (case+".pin.json"),self.folder / (case+".policy.json")
            index.write_new(pin_path,pin)
            index.write_new(policy_path,self.policy)
            expected_tree,expected_index = self.folder / (case+".tree"),self.folder / (case+".index")
            oracle.observe(self.root,case,expected_tree,tracked={"tracked.txt"} if case == "E11" else None)
            index.write_new(expected_index,index.observe(self.root,pin))
            args = SimpleNamespace(root=self.root,case=case,pin=pin_path,pin_sha256=index.file_digest(pin_path),policy=policy_path,
                policy_sha256=index.file_digest(policy_path),expected_tree=expected_tree,expected_tree_sha256=index.file_digest(expected_tree),
                expected_index=expected_index,expected_index_sha256=index.file_digest(expected_index),output=self.folder / (case+"-verify"),
                tracked=tracked if case == "E11" else None,tracked_sha256=index.file_digest(tracked) if case == "E11" else None)
            with mock.patch.object(git_queries.shutil,"which",return_value=str(self.binary)), \
                    mock.patch.object(git_queries.subprocess,"Popen",side_effect=self.popen):
                result = git_queries.verify(args)
            self.assertEqual(result["status"],"PASS")
            if case == "E11":
                self.assertEqual(result["expected_operands"]["tracked_sha256"],index.file_digest(tracked))
            if case in {"E11","C12"}:
                (self.root / "tracked.txt").write_bytes(b"wrong tracked payload")
                args.output = self.folder / (case+"-changed-payload")
                with mock.patch.object(git_queries.shutil,"which",return_value=str(self.binary)), \
                        mock.patch.object(git_queries.subprocess,"Popen",side_effect=self.popen):
                    self.assertEqual(git_queries.verify(args)["status"],"FAIL")
                (self.root / "tracked.txt").write_bytes(b"known tracked payload")

    def test_tracked_input_requires_canonical_unique_names_and_exact_seal(self):
        for position,rows in enumerate((["../escape"],["/absolute"],["same","same"])):
            path = self.folder / ("bad-tracked-"+str(position))
            path.write_text(json.dumps(rows))
            with self.assertRaises(ValueError):
                git_queries.tracked_paths(path,index.file_digest(path))
        path = self.folder / "good-tracked"
        path.write_text('["one"]')
        with self.assertRaises(ValueError):
            git_queries.tracked_paths(path,"0"*64)


if __name__ == "__main__":
    unittest.main()

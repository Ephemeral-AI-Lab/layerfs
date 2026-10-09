"""External deterministic controller contracts; no SDK/Docker/Git execution."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reference_prepare as reference


class ReferenceController(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="layerfs-r7-reference-test-", dir="/tmp")
        self.root = Path(self.temporary.name).resolve()
        self.config = dict(socket="/tmp/owned-engine.sock", runtime_binary="/tmp/runtime", daemon_binary="/tmp/daemon",
                           volume="owned-fresh-volume", volume_clone_identity="closed byte clone", manifest="/tmp/installed",
                           container_setup=dict(manifest_sha256="a"*64, implementation_sha256="b"*64))
        self.config_path = self.root / "config.json"
        reference.write_json(self.config_path, self.config)

    def tearDown(self):
        self.temporary.cleanup()

    def stage_data(self, case="W01"):
        targets = ["/code", "/replay", "/native-scan", "/native-writer"] if case == "W01" else ["/code", "/native"]
        items = []
        calls, verifications = [], []
        container = "c"*64
        for index, target in enumerate(targets):
            inventory = self.root / (str(index) + ".jsonl")
            inventory.write_bytes(b"closed setup inventory\n")
            item = dict(target=target, inventory=str(inventory), inventory_sha256=str(index)*64,
                        content_metadata_set_sha256="d"*64)
            items.append(item)
            inside = "/tmp/layerfs-r7-setup-original/expected-" + str(index) + ".jsonl"
            calls.append(dict(label="inventory-"+str(index), attempts=1, exit_code=0,
                              argv=["docker", "--host", "unix://"+self.config["socket"], "cp", str(inventory), container+":"+inside]))
            verifications.append(dict(root=target, status="PASS", inventory_sha256=item["inventory_sha256"],
                                      content_metadata_set_sha256=item["content_metadata_set_sha256"]))
        plan = dict(assets=[row for row in items if not row["target"].startswith("/native")],
                    native=[row for row in items if row["target"].startswith("/native")])
        staging = dict(schema="r7-container-staging-v1", status="PASS", container=container,
                       deployment_manifest_sha256="a"*64, implementation_sha256="b"*64,
                       operations=calls, verifications=verifications, helper_sha256={"git-default-policy.json":"e"*64})
        return plan, staging

    def test_inputs_recover_exact_original_inventory_targets_and_peer_roles(self):
        plan, stage = self.stage_data()
        value = reference.input_declaration(self.config, dict(case_id="W01"), plan, stage, {"exact_fixture":"closed"})
        self.assertEqual([row["role"] for row in value["roots"]], ["scan", "writer"])
        self.assertEqual(value["roots"][0]["inventory"], "/tmp/layerfs-r7-setup-original/expected-2.jsonl")
        self.assertEqual(value["roots"][1]["inventory"], "/tmp/layerfs-r7-setup-original/expected-3.jsonl")
        self.assertEqual(value["external"][0]["path"], "/replay")

    def test_ambiguous_unknown_or_mismatched_staging_is_refused(self):
        plan, initial = self.stage_data()
        variants = []
        variant = copy.deepcopy(initial)
        variant["operations"].append(variant["operations"][0])
        variants.append(variant)
        variant = copy.deepcopy(initial)
        del variant["operations"][0]["exit_code"]
        variants.append(variant)
        variant = copy.deepcopy(initial)
        variant["operations"][0]["argv"][-1] = "c"*64 + ":/tmp/guessed.jsonl"
        variants.append(variant)
        variant = copy.deepcopy(initial)
        variant["verifications"][0]["content_metadata_set_sha256"] = "0"*64
        variants.append(variant)
        for stage in variants:
            with self.subTest(stage=stage), self.assertRaises(ValueError):
                reference.input_declaration(self.config, dict(case_id="W01"), plan, stage, {})

    def test_git_policy_operand_uses_actual_staged_asset_hash(self):
        plan, stage = self.stage_data("E04")
        value = reference.input_declaration(self.config, dict(case_id="E04"), plan, stage, {})
        self.assertEqual(value["git_default_policy"], dict(path="/code/git-default-policy.json", sha256="e"*64))
        self.assertEqual(value["code_assets"], {"git-default-policy.json":"e"*64})

    def test_rejected_admission_never_constructs_runtime_or_output(self):
        output = self.root / "refused"
        original = ValueError("sealed daemon differs")
        with mock.patch.object(reference, "validate", side_effect=original), mock.patch.object(reference, "EventProcess") as factory:
            with self.assertRaises(ValueError) as raised:
                reference.run(self.config_path, "E04", "A", output)
        self.assertIs(raised.exception, original)
        factory.assert_not_called()
        self.assertFalse(output.exists())

    def test_elapsed_validation_refuses_runtime_launch_before_original_container(self):
        output = self.root / "expired-admission"
        with mock.patch.object(reference, "validate", return_value=({}, {}, {})), \
                mock.patch.object(reference.time, "monotonic", side_effect=[0,661]), \
                mock.patch.object(reference, "EventProcess") as factory:
            with self.assertRaisesRegex(TimeoutError, "after_reference_admission"):
                reference.run(self.config_path, "E04", "A", output)
        factory.assert_not_called()
        self.assertFalse(output.exists())

    def test_staging_is_not_launched_without_its_full_independent_allowance(self):
        runtime = self.runtime()
        with mock.patch.object(reference, "validate", return_value=({}, {}, {})), \
                mock.patch.object(reference.time, "monotonic", side_effect=[0,1,2,3,361,362]), \
                mock.patch.object(reference, "EventProcess", return_value=runtime), \
                mock.patch.object(reference.deployment, "stage") as stage:
            result = reference.run(self.config_path, "E04", "A", self.root / "stage-expired")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual(result["original_phase"], "original_staging_admission")
        stage.assert_not_called()
        runtime.send.assert_not_called()

    def test_expired_cli_call_never_launches_or_creates_original_attempt(self):
        output = self.root / "expired"
        output.mkdir()
        (output / "operations").mkdir()
        with mock.patch.object(reference.time, "monotonic", return_value=100), mock.patch.object(reference.subprocess, "Popen") as factory:
            with self.assertRaises(TimeoutError):
                reference.cli_call(self.config, SimpleNamespace(container="c"*64), output, {"operations":[]}, "copy", ["cp"], 15, 99)
        factory.assert_not_called()
        self.assertEqual(list((output / "operations").iterdir()), [])

    def test_cli_wait_failure_keeps_original_pid_and_both_independent_closes(self):
        output = self.root / "failure"
        output.mkdir()
        (output / "operations").mkdir()
        original = subprocess.TimeoutExpired(["original Docker action"], 15)
        child = SimpleNamespace(pid=9182, wait=mock.Mock(side_effect=original))
        opened = []
        real = reference.author.OutputFile
        class ClosingFailure:
            def __init__(self, path, phase):
                self.owner = real(path, phase)
                self.stream = self.owner.stream
                opened.append(self)
            def __enter__(self):
                return self.stream
            def __exit__(self, kind, error, traceback):
                self.stream.close()
                closing = OSError("independent close " + str(len(opened)))
                if error is None:
                    raise closing
                error.independent_close_failures = [*getattr(error,"independent_close_failures",[]),str(closing)]
                return False
        result = {"operations":[]}
        with mock.patch.object(reference.author, "OutputFile", ClosingFailure), \
                mock.patch.object(reference.subprocess, "Popen", return_value=child) as launched, \
                mock.patch.object(reference, "write_json"):
            with self.assertRaises(subprocess.TimeoutExpired) as raised:
                reference.cli_call(self.config, SimpleNamespace(container="c"*64), output, result, "author", ["exec"], 15, reference.time.monotonic()+20)
        self.assertIs(raised.exception, original)
        self.assertEqual(original.original_phase, "author_wait")
        self.assertEqual(len(original.independent_close_failures), 2)
        self.assertEqual(result["operations"][0]["pid"], 9182)
        self.assertNotIn("exit_code", result["operations"][0])
        launched.assert_called_once()
        child.wait.assert_called_once()

    def runtime(self):
        runtime = SimpleNamespace(container="c"*64, exec_ids=[], daemon_instance="actual-daemon", child_event_receipts=["actual-original.events"],
                                  event=mock.Mock(), send=mock.Mock(return_value={"event":"container_stopped"}), retain=mock.Mock(return_value={"host_fence":"once"}),
                                  selector=SimpleNamespace(close=mock.Mock()), raw=SimpleNamespace(close=mock.Mock()), stderr=SimpleNamespace(close=mock.Mock()),
                                  process=SimpleNamespace(stdin=SimpleNamespace(close=mock.Mock()), stdout=SimpleNamespace(close=mock.Mock()), wait=mock.Mock(return_value=0)))
        return runtime

    def test_constructor_partial_owner_and_first_cause_survive(self):
        runtime = self.runtime()
        original = OSError("selector failed after original host launch")
        original.event_process = runtime
        original.original_phase = "event_process_selector_create"
        with mock.patch.object(reference, "validate", return_value=(dict(arm="N",case_id="E04",cache_class="A"),{},{})), \
                mock.patch.object(reference, "EventProcess", side_effect=original):
            result = reference.run(self.config_path, "E04", "A", self.root / "partial")
        self.assertEqual(result["original_failure"], str(original))
        self.assertEqual(result["original_phase"], "event_process_selector_create")
        self.assertEqual(result["container"], runtime.container)
        runtime.retain.assert_called_once_with(original)
        runtime.send.assert_not_called()

    def test_staging_failure_does_not_send_normal_stop_or_retry(self):
        runtime = self.runtime()
        original = ValueError("original actual copy failed")
        original.staging_receipt = {"original_error":"original actual copy failed"}
        original.staging_output = str(self.root / "exact-staging-owner")
        original.independent_output_failures = ["independent staging receipt failure"]
        with mock.patch.object(reference, "validate", return_value=(dict(arm="N",case_id="E04",cache_class="A"),{},{})), \
                mock.patch.object(reference, "EventProcess", return_value=runtime), \
                mock.patch.object(reference.deployment, "stage", side_effect=original) as stage:
            result = reference.run(self.config_path, "E04", "A", self.root / "failed-stage")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual(result["container_stop"], "NOT_ATTEMPTED")
        runtime.send.assert_not_called()
        runtime.retain.assert_called_once_with(original)
        stage.assert_called_once()
        self.assertEqual(result["staging_receipt"], original.staging_receipt)
        self.assertEqual(result["staging_output"], original.staging_output)
        self.assertEqual(result["independent_output_failures"], original.independent_output_failures)

    def test_success_authors_once_as_root_setup_then_normal_stop(self):
        runtime = self.runtime()
        plan, staged = self.stage_data("E02")
        selection = dict(arm="N",case_id="E02",cache_class="A")
        output = self.root / "successful"
        def stage(config, selected, container, folder):
            folder.mkdir()
            reference.write_json(folder / "receipt.json", staged)
            return staged
        def spec(args):
            reference.write_json(args.output, {"inputs":reference.read_json(args.inputs)})
            return {"status":"DECLARED_SETUP_ONLY"}
        def call(config, owner, folder, result, label, arguments, seconds, deadline):
            result["operations"].append(dict(label=label,argv=arguments,known_exit=0))
            return dict(label=label,argv=arguments,exit_code=0)
        with mock.patch.object(reference, "validate", return_value=(selection,plan,{"exact":"fixture"})), \
                mock.patch.object(reference, "EventProcess", return_value=runtime), \
                mock.patch.object(reference.deployment, "stage", side_effect=stage), \
                mock.patch.object(reference.author, "emit_spec", side_effect=spec) as emitted, \
                mock.patch.object(reference, "cli_call", side_effect=call) as calls, \
                mock.patch.object(reference, "check_bundle", return_value={"closed":"actualclosed"}), \
                mock.patch.object(reference, "seal_bundle", return_value={"entries":10}):
            result = reference.run(self.config_path, "E02", "A", output)
        self.assertEqual(result["status"], "CLOSED_EXPECTED_FUNCTIONAL_SETUP_ONLY")
        self.assertEqual([row["label"] for row in result["operations"]], ["prepare-author-inputs","copy-spec","copy-staging","author","copy-bundle"])
        author_call = calls.call_args_list[3]
        self.assertEqual(author_call.args[5][:4], ["exec","--user","0:0",runtime.container])
        self.assertEqual(author_call.args[6], 300)
        emitted.assert_called_once()
        runtime.send.assert_called_once()
        self.assertEqual(runtime.send.call_args.args, ("stop",))
        runtime.retain.assert_not_called()
        self.assertEqual(result["performance_samples"], 0)
        for owner in (runtime.selector, runtime.raw, runtime.stderr, runtime.process.stdin, runtime.process.stdout):
            owner.close.assert_called_once()

    def test_bundle_inventory_hashes_all_witnesses_and_rejects_symlinks(self):
        bundle = self.root / "bundle"
        (bundle / "git-queries").mkdir(parents=True)
        (bundle / "closed.json").write_bytes(b"closed")
        (bundle / "git-queries/version.ack.json").write_bytes(b"ack")
        result = reference.seal_bundle(bundle, self.root / "full.inventory.jsonl", reference.time.monotonic()+5)
        self.assertEqual(result["entries"], 3)
        self.assertEqual(result["regular_bytes"], 9)
        self.assertEqual(result["sha256"], reference.sha(self.root / "full.inventory.jsonl"))
        (bundle / "escape").symlink_to(self.config_path)
        with self.assertRaisesRegex(ValueError, "unsupported"):
            reference.seal_bundle(bundle, self.root / "failed.inventory.jsonl", reference.time.monotonic()+5)

    def test_completed_bundle_iterator_close_is_attempted_once_even_if_it_fails(self):
        bundle = self.root / "empty-bundle"
        bundle.mkdir()
        original = OSError("original directory iterator close")
        iterator = mock.Mock()
        iterator.__next__ = mock.Mock(side_effect=StopIteration)
        iterator.close = mock.Mock(side_effect=original)
        class EmptyIterator:
            def __next__(self):
                return iterator.__next__()
            def close(self):
                return iterator.close()
        with mock.patch.object(reference.os, "scandir", return_value=EmptyIterator()):
            with self.assertRaises(OSError) as raised:
                reference.seal_bundle(bundle, self.root / "close-failure.jsonl", reference.time.monotonic()+5)
        self.assertIs(raised.exception, original)
        iterator.close.assert_called_once()

    def test_runtime_closes_all_owners_once_and_keeps_original_before_close_failures(self):
        runtime = self.runtime()
        original = ValueError("original observer failure")
        original.independent_close_failures = ["earlier payload close"]
        runtime.raw.close.side_effect = OSError("raw close")
        runtime.stderr.close.side_effect = OSError("stderr close")
        reference.close_runtime(runtime, original)
        reference.close_runtime(runtime, original)
        self.assertEqual(original.independent_close_failures, ["earlier payload close","raw close","stderr close"])
        for owner in (runtime.selector, runtime.raw, runtime.stderr, runtime.process.stdin, runtime.process.stdout):
            owner.close.assert_called_once()

    def test_success_owner_close_failure_becomes_first_cause_and_other_closes_continue(self):
        runtime = self.runtime()
        original = OSError("original raw close")
        runtime.raw.close.side_effect = original
        runtime.stderr.close.side_effect = OSError("independent stderr close")
        with self.assertRaises(OSError) as raised:
            reference.close_runtime(runtime)
        self.assertIs(raised.exception, original)
        self.assertEqual(original.original_phase, "original_runtime_raw_close")
        self.assertEqual(original.independent_close_failures, ["independent stderr close"])
        reference.close_runtime(runtime, original)
        for owner in (runtime.selector, runtime.raw, runtime.stderr, runtime.process.stdin, runtime.process.stdout):
            owner.close.assert_called_once()

    def closed_fixture(self):
        from r7 import test_verification
        fixture = test_verification.RecipeContract()
        fixture.setUp()
        self.addCleanup(fixture.tearDown)
        closed = fixture.recipe(case="E04", cache="A")
        closed.update(spec_sha256="a"*64, staging_receipt_sha256="b"*64, container="c"*64)
        declaration = dict(inputs=closed["inputs"], helpers=closed["helpers"], case={"case_id":closed["case_id"],"workload":closed["selected_workload"]})
        for key in ("case_id","cache_class","environment","image_id","uid","gid","registry_sha256","workload_source_sha256"):
            declaration[key] = closed[key]
        spec = dict(value=declaration,sha256="a"*64)
        staging = dict(value=dict(container="c"*64,verifications=closed["native_verifications"]),sha256="b"*64)
        stdout = fixture.root / "original-author.stdout"
        def seal_original():
            reference.write_json(fixture.bundle / "closed.json", closed)
            stdout.write_bytes(b'{"event":"started","pid":901}\n'+json.dumps(closed,sort_keys=True).encode()+b"\n")
            return dict(status="ORIGINAL_COMPLETED",exit_code=0,stdout=str(stdout),stdout_sha256=reference.sha(stdout))
        return fixture,closed,spec,staging,seal_original

    def test_copied_closed_result_is_bound_to_original_author_stdout(self):
        fixture,closed,spec,staging,sealed = self.closed_fixture()
        original = sealed()
        result = reference.check_bundle(fixture.bundle,spec,staging,closed["asset_root"],fixture.config,original)
        self.assertEqual(result["status"], "CLOSED_EXPECTED_SETUP_ONLY")
        changed = dict(closed, host_stdout_recipe={})
        (fixture.bundle / "closed.json").write_text(json.dumps(changed))
        with self.assertRaisesRegex(ValueError,"original author final stdout"):
            reference.check_bundle(fixture.bundle,spec,staging,closed["asset_root"],fixture.config,original)

    def test_self_sealed_git_expected_row_still_requires_paired_context_and_tree(self):
        fixture,closed,spec,staging,sealed = self.closed_fixture()
        row = closed["expected"][0]
        row["git_index"]["tree_sha256"] = "0"*64
        script = fixture.bundle / "expected.verify.sh"
        script.write_text(reference.author.verifier_body(closed["asset_root"],row["observer"],row["label"],row["git_index"]))
        row["verifier"]["sha256"] = reference.sha(script)
        original = sealed()
        with self.assertRaisesRegex(ValueError,"paired context/tree"):
            reference.check_bundle(fixture.bundle,spec,staging,closed["asset_root"],fixture.config,original)

    def test_self_sealed_index_payload_cannot_disagree_with_expected_tree(self):
        fixture,closed,spec,staging,sealed = self.closed_fixture()
        row = closed["expected"][0]
        path = fixture.bundle / row["git_index"]["manifest"]
        observed = json.loads(path.read_text())
        observed["filesystem_metadata"]["uid"] = 0
        path.write_text(json.dumps(observed))
        row["git_index"]["manifest_sha256"] = reference.sha(path)
        script = fixture.bundle / "expected.verify.sh"
        script.write_text(reference.author.verifier_body(closed["asset_root"],row["observer"],row["label"],row["git_index"]))
        row["verifier"]["sha256"] = reference.sha(script)
        original = sealed()
        with self.assertRaisesRegex(ValueError,"raw metadata binding"):
            reference.check_bundle(fixture.bundle,spec,staging,closed["asset_root"],fixture.config,original)


if __name__ == "__main__":
    unittest.main()

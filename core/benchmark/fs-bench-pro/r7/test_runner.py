"""Product-free orchestration algebra and original numeric-frame checks."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import receipts, registry, runner


class RunnerContract(unittest.TestCase):
    def test_overdue_buffered_known_reply_keeps_custody_without_budget_success(self):
        process = runner.EventProcess.__new__(runner.EventProcess)
        reply = {"event": "verify", "sequence": 1, "fields": {"container": "c", "exec": "e", "exit_code": "0"}}
        process.buffer = (json.dumps(reply) + "\n").encode()
        process.rows, process.exec_ids, process.last_sequence = [], [], 0
        process.container = process.daemon_instance = process.scope = None
        with mock.patch.object(runner.time, "monotonic", return_value=100):
            with self.assertRaisesRegex(TimeoutError, "buffered event"):
                process.event("verify", 99)
        self.assertEqual(process.rows, [reply])
        self.assertEqual(process.exec_ids, ["e"])

    def test_command_body_binding_refuses_wrong_known_zero_body_without_resend(self):
        with tempfile.TemporaryDirectory() as folder:
            script = Path(folder) / "body.sh"
            script.write_text("printf original\n")
            process = runner.EventProcess.__new__(runner.EventProcess)
            process.process = mock.Mock()
            process.process.stdin.write.side_effect = len
            event = {"event": "command", "fields": {"command_sha256": "0" * 64, "exit_code": "0"}}
            process.event = mock.Mock(return_value=event)
            with mock.patch.object(runner.time, "monotonic", return_value=10):
                with self.assertRaisesRegex(runner.OriginalFailure, "body differs"):
                    process.send("command", "native:/native", script, deadline=20)
            self.assertEqual(process.process.stdin.write.call_count, 1)
            self.assertEqual(process.event.call_count, 1)
            event["fields"]["command_sha256"] = runner.sha(script)
            with mock.patch.object(runner.time, "monotonic", return_value=10):
                self.assertIs(process.send("command", "native:/native", script, deadline=20), event)

    def test_expired_operation_is_not_sent(self):
        process = runner.EventProcess.__new__(runner.EventProcess)
        process.process = mock.Mock()
        with mock.patch.object(runner.time, "monotonic", return_value=100):
            with self.assertRaisesRegex(TimeoutError, "not sent"):
                process.send("verify", deadline=99)
        process.process.stdin.write.assert_not_called()

    def test_changed_checkpoint_keeps_up_to_date_as_failure_without_replay(self):
        runner.require_changed_commit({"event": "commit", "fields": {"commit_kind": "Committed"}})
        for kind in ("UpToDate", None, "UNKNOWN"):
            with self.assertRaisesRegex(runner.OriginalFailure, "typed Committed"):
                runner.require_changed_commit({"event": "commit", "fields": {"commit_kind": kind}})

    def test_effective_claim_ignores_free_labels_and_binds_actual_source_and_execution(self):
        selected = {"selection_id": "E02:A:N", "arm": "N"}
        labels = dict.fromkeys(receipts.IDENTITIES, "old")
        actual = {"source_set_sha256": "a" * 64, "binaries": {"runtime": "b" * 64}, "uid": 501}
        key = runner.claim_key(selected, labels, effective=actual)
        self.assertEqual(key, runner.claim_key(selected, dict.fromkeys(receipts.IDENTITIES, "new"), effective=actual))
        for changed in (dict(actual, source_set_sha256="c" * 64),
                        dict(actual, binaries={"runtime": "d" * 64}), dict(actual, uid=502)):
            self.assertNotEqual(key, runner.claim_key(selected, labels, effective=changed))

    def test_failed_verifier_time_is_excluded_before_custody_fence(self):
        clock = runner.PerformanceClock()
        clock.begin(10)
        clock.begin_verifier(30)
        failed = clock.observe(80)
        self.assertEqual(failed["performance_ns"], 20)
        self.assertEqual(failed["excluded_verifier_intervals"], [(30, 80)])

    def test_terminal_failure_and_later_cleanup_preserve_performance_boundary(self):
        clock = runner.PerformanceClock()
        clock.begin(10)
        clock.begin_verifier(30)
        clock.end_verifier(80)
        self.assertEqual(clock.observe(100)["performance_ns"], 40)
        clock.finish(110)
        self.assertEqual(clock.observe(900)["performance_ns"], 50)

    def test_streamed_p_cold_receipt_requires_fresh_connection(self):
        row = {"class": "A", "scopes": dict.fromkeys(registry.PHASES, "declared"),
               "declared_external_inputs": {"replay_roots": [], "semantic_files": []},
               "fresh_kernel_connection": False}
        with self.assertRaisesRegex(ValueError, "fresh connection"):
            receipts.validate_cache(row, "P", 1)

    def complete_row(self, arm="L"):
        return {"row_status":"INCOMPLETE","source_arm":arm,"scenario_id":"E02","attempted_operation_count":1,
                "completed_operation_count":1,"custody_status":"KNOWN_STOP","verification_status":"PASS","cleanup_status":"Gone","numeric_correlation_status":"PASS",
                "observations":{name:{"status":"AVAILABLE","value":1,"provenance":"synthetic original"} for name in registry.COUNTERS},
                "cache":{"class":"C","scopes":dict.fromkeys(registry.PHASES,"declared synthetic scope"),
                         "warmup_gap_ns":1,"same_mount":True,"warmup_receipt":"original"}}

    def test_complete_l_observations_are_diagnostic_without_resource_admission_pass(self):
        row = self.complete_row()
        runner.diagnostic_completion(row)
        self.assertEqual(row["row_status"],"DIAGNOSTIC")
        self.assertNotEqual(row["resource_status"],"PASS")
        row["observations"]["mount_work"] = receipts.unavailable("original unavailable")
        runner.diagnostic_completion(row)
        self.assertEqual(row["row_status"],"INCOMPLETE")

    def test_missing_b_and_c_predicates_never_become_zero_or_eligible(self):
        for cls,key in (("B","object_demands"),("C","warmup_gap_ns")):
            row = self.complete_row()
            row["cache"].update({"class":cls,"warmup_terminal_unmount":True,"history_read_residency":"declared"})
            row["cache"].pop(key,None)
            runner.diagnostic_completion(row)
            self.assertEqual(row["row_status"],"INCOMPLETE")
            self.assertNotIn(key,row["cache"])
        row = self.complete_row()
        row["cache"]["warmup_gap_ns"] = 60_000_000_000
        runner.diagnostic_completion(row)
        self.assertEqual(row["row_status"],"INELIGIBLE")

    def test_p_missing_kernel_counts_remains_incomplete(self):
        row = self.complete_row("P")
        runner.diagnostic_completion(row)
        self.assertEqual(row["row_status"],"INCOMPLETE")
        self.assertTrue(any("batch/default" in gap for gap in row["completion_gaps"]))

    def test_replay_and_semantic_data_are_declared_separately_from_helpers(self):
        self.assertEqual(runner.external_inputs("E12")["replay_roots"],["/replay"])
        self.assertEqual(runner.external_inputs("E13")["replay_roots"],["/replay"])
        self.assertEqual(runner.external_inputs("E14")["semantic_files"],["/code/node-roots.json"])
        self.assertEqual(runner.external_inputs("E02"),{"replay_roots":[],"semantic_files":[]})

    def test_zero_store_pages_cannot_replace_missing_or_warm_replay_predicate(self):
        cache = {"class":"A","scopes":dict.fromkeys(registry.PHASES,"synthetic scope"),"fresh_daemon_cache":True,"fresh_kernel_connection":True,
                 "store_path":"store","required_residency_paths":["store","store-wal","store-shm","store-journal"],
                 "declared_external_inputs":{"replay_roots":["/replay"],"semantic_files":[]},
                 "residency":{"cache_class":"A","files":[{"path":"store","present":True,"resident_pages":0,"eviction_hint_attempts":1}]+
                   [{"path":"store"+suffix,"present":False,"resident_pages":0} for suffix in ("-wal","-shm","-journal")],
                   "resident_pages":0,"attempts":0,"payload_bytes_read":0,"method":"fadvise mincore"}}
        with self.assertRaisesRegex(ValueError,"replay cold roots"):
            receipts.validate_cache(cache,"L",1)
        component = {"role":"replay","root":"/replay","stream_inventory_artifact":"raw.jsonl","stream_inventory_sha256":"1"*64,
                     "residency":{"schema":"r7-residency-stream-v1","cache_class":"A","status":"ELIGIBLE","resident_pages":0,
                                  "method":"fadvise mincore","payload_bytes_read":0,"attempts":0,"eviction_hint_attempts":1,
                                  "inventory":{"created":True,"complete":True,"sha256":"1"*64,"rows":1,"physical_files":1},
                                  "input_manifest":{"root":"/replay","sha256":"2"*64,"expected_sha256":"2"*64,"declared_files":1,"declared_physical_files":1}}}
        cache["auxiliary_residencies"] = [component]
        receipts.validate_cache(cache,"L",1)
        component["residency"].update(status="INELIGIBLE",resident_pages=1)
        with self.assertRaisesRegex(ValueError,"additional cold predicate"):
            receipts.validate_cache(cache,"L",1)
        receipts.validate_cache(cache,"L",0,True)

    def peers(self, arm="P", case="W01"):
        config = {"native_root": "/native-a", "native_peer_roots": ["/native-a", "/native-b"],
                  "peer_preparation_identity": "closed independent writable byte copies", "oracle_script": "/sealed/oracle.sh",
                  "passthrough_mount": "/p-a", "passthrough_peer_mounts": ["/p-a", "/p-b"]}
        return config, {"arm": arm, "case_id": case}

    def test_actual_peer_inputs_enable_both_controls(self):
        config, selected = self.peers()
        self.assertEqual(runner.control_peers(config, selected), [("/native-a", "/p-a"), ("/native-b", "/p-b")])
        selected["arm"] = "N"
        self.assertEqual(runner.control_peers(config, selected), [("/native-a", "/native-a"), ("/native-b", "/native-b")])

    def test_peer_alias_overlap_or_missing_oracle_refused(self):
        for changes in ({"native_peer_roots": ["/native-a", "/native-a/"]},
                        {"native_peer_roots": ["/native-a", "/native-a/nested"]},
                        {"passthrough_peer_mounts": ["/p-a", "/p-a/"]},
                        {"passthrough_peer_mounts": ["/p-a", "/native-b"]},
                        {"oracle_script": None}, {"peer_preparation_identity": None}):
            config, selected = self.peers()
            config.update(changes)
            with self.assertRaises(ValueError):
                runner.control_peers(config, selected)

    def test_w02_uses_one_retained_native_root_and_mount(self):
        config, selected = self.peers(case="W02")
        config["native_peer_roots"] = ["/native-a"]
        config["passthrough_peer_mounts"] = ["/p-a"]
        self.assertEqual(runner.control_peers(config, selected), [("/native-a", "/p-a")])

    def test_children_require_exact_original_successes(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "stderr"
            path.write_text("other diagnostic\nR7_CHILD_RESULT pid=11 exit=0\nR7_CHILD_RESULT pid=12 exit=0\n")
            event = {"fields": {"stderr": str(path)}}
            self.assertEqual(runner.child_results(event), [{"pid":11,"exit":0},{"pid":12,"exit":0}])
            for body in ("R7_CHILD_RESULT pid=11 exit=0\n", "R7_CHILD_RESULT pid=11 exit=0\nR7_CHILD_RESULT pid=11 exit=0\n",
                         "R7_CHILD_RESULT pid=11 exit=0\nR7_CHILD_RESULT pid=12 exit=2\n"):
                path.write_text(body)
                with self.assertRaises(runner.OriginalFailure):
                    runner.child_results(event)

    def test_ready_without_matched_two_loop_profile_refused(self):
        from types import SimpleNamespace
        profile = {"event":"negotiated", "max_write":131072,"max_readahead":131072,"max_background":1,
                   "congestion_threshold":1,"configured_receive_loops":2,"ttl_seconds":60,"writeback":False,"default_permissions":True}
        ready = {"phase":"Serving","configured":2,"created":2,"entered":2,"exited":0,"joined":0}
        process = SimpleNamespace(rows=[profile])
        self.assertEqual(runner.passthrough_ready(process, ready), profile)
        with self.assertRaises(runner.OriginalFailure):
            runner.passthrough_ready(process, dict(ready,entered=1))
        process.rows = [dict(profile,writeback=True)]
        with self.assertRaises(runner.OriginalFailure):
            runner.passthrough_ready(process, ready)

    def test_control_identity_does_not_follow_product_change(self):
        identity = dict.fromkeys(receipts.IDENTITIES, "first")
        changed = dict(identity, product="second", source_commit="second", source_tree="second", compilation="second")
        selected = {"selection_id": "E02:A:N", "arm": "N"}
        self.assertEqual(runner.claim_key(selected, identity), runner.claim_key(selected, changed))
        selected = {"selection_id": "E02:A:L", "arm": "L"}
        self.assertNotEqual(runner.claim_key(selected, identity), runner.claim_key(selected, changed))

    def test_control_workload_change_requires_new_identity(self):
        identity = dict.fromkeys(receipts.IDENTITIES, "first")
        selected = {"selection_id": "E02:A:P", "arm": "P"}
        self.assertNotEqual(runner.claim_key(selected, identity), runner.claim_key(selected, dict(identity, workload="second")))

    def test_output_never_overwrites_original(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "original.json"
            runner.write_new(path, {"first": True})
            with self.assertRaises(FileExistsError):
                runner.write_new(path, {"second": True})
            self.assertEqual(json.loads(path.read_text()), {"first": True})

    def test_sha_matches_independent_hashlib(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "bytes"
            path.write_bytes(b"abc")
            self.assertEqual(runner.sha(path), hashlib.sha256(b"abc").hexdigest())

    def test_parallel_has_two_children_and_waits_every_pid(self):
        body = runner.shell_parallel([("/first root", "true"), ("/second", "printf x")])
        self.assertEqual(body.count('pids+=("$!")'), 2)
        self.assertIn('for pid in "${pids[@]}"', body)
        self.assertIn('if test "$failure" -eq 0', body)
        self.assertIn("R7_CHILD_RESULT", body)
        self.assertIn("'/first root'", body)

    def test_missing_prepared_input_is_unrun_without_product(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            selection = next(item for item in registry.selections() if item["selection_id"] == "E02:A:L")
            row = runner.one({}, selection, folder / "case", folder / "claims")
            self.assertEqual(row["row_status"], "NOT_RUN")
            self.assertEqual(row["attempted_operation_count"], 0)
            self.assertFalse((folder / "claims").exists())

    def test_owner_unrun_selection_remains_visible(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            selection = next(item for item in registry.selections() if item["selection_id"] == "E09:none:L")
            row = runner.one({}, selection, folder / "case", folder / "claims")
            self.assertEqual(row["row_status"], "NOT_RUN")
            self.assertIn("normalized oracle", row["reason"])

    def test_git_variant_refuses_unrefreshed_index_after_class_c_warmup(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            selection = next(item for item in registry.selections() if item["selection_id"] == "E18:C:L")
            with mock.patch.object(runner, "preflight", side_effect=AssertionError("no operation admitted")):
                row = runner.one({"oracle_variant": runner.registry_variant.NAME}, selection, folder / "case", folder / "claims")
            self.assertEqual(row["row_status"], "NOT_RUN")
            self.assertEqual(row["reason"], runner.registry_variant.E18_C_REASON)
            self.assertEqual(row["sample_count"], 0)
            self.assertFalse((folder / "claims").exists())


def numeric_rows():
    rows = [{"layerfs_observation": 1, "call": 3, "section": 0, "index": 1,
             "available": True, "values": [4, 2, 8388608]},
            {"layerfs_observation": 1, "call": 3, "section": 8, "index": 0,
             "available": True, "values": [2, 4, 1, 2, 0]},
            {"layerfs_observation": 1, "call": 3, "section": 15, "index": 0,
             "available": False, "values": []}]
    rows.append({"layerfs_observation": 1, "call": 3, "section": 99, "index": 0,
                 "available": True, "values": [3, 8, 1]})
    for row in rows:
        row.update(scope=[1,2,3,4], daemon=[5,6,7,8], slot=0)
    return rows


class NumericFraming(unittest.TestCase):
    def parse(self, rows):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "original.stderr"
            path.write_text("retained non-event diagnostic\n" + "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows))
            return runner.telemetry(path)

    def test_original_bound_footer_and_unavailable(self):
        result = self.parse(numeric_rows())
        rows = next(iter(result.values()))
        self.assertEqual(len(rows), 4)
        self.assertFalse(rows[2]["available"])

    def test_missing_original_footer(self):
        with self.assertRaisesRegex(ValueError, "incomplete original"):
            self.parse(numeric_rows()[:-1])

    def test_duplicate_section(self):
        rows = numeric_rows()
        rows.insert(2, copy.deepcopy(rows[1]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.parse(rows)

    def test_footer_cannot_omit_numeric_value(self):
        rows = numeric_rows()
        rows[-1]["values"][1] -= 1
        with self.assertRaisesRegex(ValueError, "footer differs"):
            self.parse(rows)

    def test_unavailable_is_not_zero_array(self):
        rows = numeric_rows()
        rows[2]["values"] = [0]
        with self.assertRaisesRegex(ValueError, "fabricated values"):
            self.parse(rows)

    def test_invalid_numeric_identity_or_u64_never_decoded(self):
        for field,value in (("call",True),("call",0),("section",None),("index",2**63),("values",[2**64])):
            rows = numeric_rows()
            rows[1][field] = value
            with self.assertRaises(ValueError):
                self.parse(rows)

    def test_cumulative_snapshots_are_not_phase_deltas(self):
        observations = {name: receipts.unavailable("missing") for name in registry.COUNTERS}
        runner.ingest_observations(self.parse(numeric_rows()), observations, {})
        self.assertEqual(observations["store_work"]["status"], "AVAILABLE")
        self.assertIn("not fabricated phase deltas", observations["store_work"]["value"]["scope"])
        self.assertEqual(observations["owner_work"]["status"], "UNAVAILABLE")


if __name__ == "__main__":
    unittest.main()

"""Independent synthetic retained-data fixtures; no product execution or sample."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import shutil
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_jobs as jobs


class E01Receipts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "diagnostic"
        self.output.mkdir()
        self.fixture()

    def artifact(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = value if isinstance(value, bytes) else json.dumps(value).encode()
        path.write_bytes(raw)
        return {"path": name, "sha256": jobs.hash_bytes(raw)}

    @staticmethod
    def counters(names):
        return {key: 0 for key in names}

    def sql(self, attempts=0):
        families = [self.counters(jobs.STATEMENT) for _ in range(14)]
        families[0]["attempts"] = attempts
        families[0]["executions"] = attempts
        families[0]["vm_steps"] = 2 * attempts
        return {"scope": "connection-statement-families", "families": families,
                "total": {key: sum(row[key] for row in families) for key in jobs.STATEMENT}}

    def owner(self):
        return {"scope": "same-owner-cumulative", "credit_scope": "queued-executing-caller-held-results",
                "sql_foreground": self.sql(), "sql_maintenance": self.sql(),
                "payload_foreground": self.counters(jobs.PAYLOAD), "payload_maintenance": self.counters(jobs.PAYLOAD),
                "allocation_foreground": self.counters(jobs.ALLOCATION), "allocation_maintenance": self.counters(jobs.ALLOCATION),
                "counters": self.counters(jobs.OWNER), "completed": [0] * 6,
                "queue_wait_ns": [0] * 6, "service_ns": [0] * 6}

    def header(self, kind, index):
        digest = jobs.hash_bytes(self.identity_raw)
        return {"schema": "cluster-two-job-receipts-v1", "case": "E01-startup", "kind": kind,
                "record_index": index, "record_id": jobs.hash_bytes(f"{digest}:{kind}:{index}".encode()),
                "attempt_id": jobs.hash_bytes(f"{digest}:attempt:0".encode()),
                "owner_id": jobs.hash_bytes(f"{digest}:owner:0".encode()),
                "mode": "diagnostic", "sample_count": 0, "admission_eligible": False,
                "qualification_status": "NOT_EVALUATED", "global_persistence": "NOT_IN_SCOPE",
                "source_binding": "base-commit-plus-current-inventory", "build_target_os": "linux",
                "build_target_architecture": "aarch64", "identity": copy.deepcopy(self.identity),
                "identity_sha256": digest, "actual_binary_sha256": self.identity["artifacts"]["binary"]["sha256"],
                "invocation": copy.deepcopy(self.invocation)}

    def fixture(self):
        source = self.artifact("source.rs", b"independent schema fixture source\n")
        product = self.artifact("product.json", {"schema": "cluster-two-e2-source-inventory-v1",
            "source_commit": "1" * 40, "source_tree": "2" * 40, "source_sealed": False, "sources": [source]})
        artifacts = {key: self.artifact(key + ".artifact", (key + " identity").encode()) for key in jobs.ARTIFACTS}
        artifacts["product"] = product
        topology = self.artifact("topology.json", {"schema": "cluster-two-e2-topology-witness-v1",
            "status": "OBSERVED", "scope": "external-observed-execution-domain", "execution_kind": "standalone-host",
            "source": "independent test witness", "os": "linux", "architecture": "aarch64", "kernel": "fixture-kernel"})
        command = self.artifact("command.json", {"schema": "cluster-two-e2-command-v1", "execution_kind": "standalone-host",
            "argv": [str(self.root / "binary.artifact"), str(self.root / "database"), str(self.output), str(self.root / "identity-input.json")]})
        self.identity = {"schema": "cluster-two-e2-identity-v1", "receipt_id": "3" * 64,
            "source_commit": "1" * 40, "source_tree": "2" * 40, "source_sealed": False,
            "source_arm": "diagnostic", "profile": "local-overlay-disposable", "cache_state": "uncontrolled",
            "os": "linux", "architecture": "aarch64", "kernel": "fixture-kernel", "execution_kind": "standalone-host",
            "topology": {"status": "OBSERVED", "artifact": topology}, "command_artifact": command,
            "image": {"status": "NOT_IN_SCOPE", "reason": "standalone-host"}, "artifacts": artifacts}
        self.identity_raw = json.dumps(self.identity).encode()
        self.invocation = {"schema": "cluster-two-e2-invocation-v1", "path_scope": "process-local-canonical-inputs",
            "bound_before_startup": True, "argv": [str(path.resolve()) for path in
                (self.root / "binary.artifact", self.root / "database", self.output, self.root / "identity-input.json")],
            "identity_source_sha256": jobs.hash_bytes(self.identity_raw), "identity_source_bytes": len(self.identity_raw),
            "identity_copy_sha256": jobs.hash_bytes(self.identity_raw),
            "docker_path_mapping": {"status": "UNAVAILABLE", "reason": "no-verified-host-container-path-mapping"}}
        profile = {"sqlite_version": "3.51.0", "schema_version": 16, "compile_options": ["fixture-option"],
            "journal_mode": "memory", "locking_mode": "exclusive", "synchronous": 0, "mmap_size": 0,
            "cache_size": -2048, "page_size": 4096, "max_pages": 4_294_967_294, "explicit_page_quota": None,
            "foreign_keys": 1, "busy_timeout": 0, "temp_store": 1, "auto_vacuum": 0}
        allocation = self.counters(jobs.ALLOCATION)
        allocation.update(attempts=1, requested_bytes=268_435_456, admitted_jobs=1, observations=3)
        self.startup = self.header("startup", 0)
        self.startup.update(scope="original-startup-through-readiness-or-error", attempt_count=1,
            clock="process-local-Instant-elapsed", opened_ns=10, closed_ns=100, owner_elapsed_ns=80,
            configuration={"profile": {"pager_kib": 2048, "max_pages": None}, "owner": {"bytes": 8_388_608,
                "lifecycle_reserve": 65_536, "namespaces": 16, "jobs_per_namespace": 16, "lifecycle_jobs_per_namespace": 2}},
            creation_reported=True, original_outcome={"status": "READY", "error": None, "profile": profile},
            creation={"status": "OBSERVED", "work": {"calls": {"file_create_calls": 1, "sqlite_open_calls": 1,
                "connection_configuration_calls": 2, "cache_configuration_calls": 1}, "sql": self.sql(45),
                "allocation": allocation, "elapsed_ns": 70, "payload": None,
                "payload_status": "UNAVAILABLE-not-reported-by-CreationWork", "allocation_state": {
                    "status": "OBSERVED", "high_water_scope": "daemon-startup-lifetime-observed-allocation",
                    "requested_volume_meaning": "range-call-volume-not-new-disk", "work": copy.deepcopy(allocation),
                    "values": {"logical_bytes": 65536, "allocated_bytes": 268_435_456,
                        "high_water_allocated_bytes": 268_435_456, "reserved_tail_bytes": 268_369_920,
                        "cleanup_headroom_bytes": 134_217_728}}}})
        self.probes = []
        for index, endpoint in enumerate(("post-ready", "pre-stop"), 1):
            probe = self.header("owner-diagnostics", index)
            probe.update(endpoint=endpoint, scope="separate-post-ready-owner-diagnostics", is_sql_command=False,
                at_ns=100 + 10 * index, status="OBSERVED", work=self.owner())
            self.probes.append(probe)
        db = self.root / "database"
        db.write_bytes(b"retained schema fixture")
        stat = db.stat()
        self.outcomes = self.header("outcomes", 3)
        self.outcomes.update(startup_status="READY", stop_status="STOPPED", route=None, route_status="NOT_IN_SCOPE",
            source_owners=None, reply_publications=None, complete_command_ns=None,
            complete_command_status="UNAVAILABLE-external-command-wall-required", collector_span_ns=200,
            collector_span_scope="entry-through-stop-and-log-before-final-files", original_error=None,
            database_artifact={"scope": "separate-post-stop-artifact-stat", "status": "OBSERVED", "path": str(db),
                "logical_bytes": stat.st_size, "allocated_bytes": stat.st_blocks * 512,
                "device": stat.st_dev, "inode": stat.st_ino, "links": stat.st_nlink})
        self.manifest = self.header("manifest", 4)
        self.manifest.update(driver_version="e01-startup-original-receipts-v1", e1_sample_status="NOT_RUN",
            e1_sample_count=0, observation_consistency="NOT_EVALUATED", unavailable=list(jobs.GAPS), database_retained=True,
            forbidden_substitutions={"phase_resident_bytes": None, "eligible_debt_peak_bytes": None,
                "queue_peak_jobs": None, "whole_operation_copy_bytes": None})

    def seal(self, startup_rows=None):
        self.identity_raw = json.dumps(self.identity).encode()
        (self.root / "identity-input.json").write_bytes(self.identity_raw)
        (self.output / "identity.json").write_bytes(self.identity_raw)
        self.manifest["identity_input"] = {"path": "identity.json", "bytes": len(self.identity_raw), "sha256": jobs.hash_bytes(self.identity_raw)}
        ready = self.outcomes["startup_status"] == "READY"
        stopped = ready and self.outcomes["stop_status"] == "STOPPED"
        records = (startup_rows if startup_rows is not None else [self.startup], [], self.probes if ready else [])
        descriptors = []
        for name, rows, status in zip(("startup.jsonl", "jobs.jsonl", "probes.jsonl"), records,
                ("RECORDED", "UNRUN-route_unavailable", "RECORDED" if ready else "UNRUN-startup_failed")):
            raw = b"".join(json.dumps(row).encode() + b"\n" for row in rows)
            (self.output / name).write_bytes(raw)
            descriptors.append({"path": name, "bytes": len(raw), "records": len(rows), "sha256": jobs.hash_bytes(raw), "status": status})
        for name, raw in (("stdout.txt", b"recorded\n" if stopped else b""), ("stderr.txt", b"" if stopped else b"original failure\n")):
            (self.output / name).write_bytes(raw)
            descriptors.append({"path": name, "bytes": len(raw), "records": int(bool(raw)), "sha256": jobs.hash_bytes(raw), "status": "COLLECTOR_LOG"})
        self.manifest["streams"] = descriptors
        raw = json.dumps(self.outcomes).encode() + b"\n"
        (self.output / "outcomes.json").write_bytes(raw)
        self.manifest["outcomes"] = {"path": "outcomes.json", "bytes": len(raw), "sha256": jobs.hash_bytes(raw)}
        (self.output / "manifest.json").write_text(json.dumps(self.manifest) + "\n")

    def reidentify(self):
        self.identity_raw = json.dumps(self.identity).encode()
        self.invocation.update(identity_source_sha256=jobs.hash_bytes(self.identity_raw), identity_source_bytes=len(self.identity_raw),
                               identity_copy_sha256=jobs.hash_bytes(self.identity_raw))
        for record in [self.manifest, self.startup, self.outcomes, *self.probes]:
            header = self.header(record["kind"], record["record_index"])
            record.update(header)

    def validate(self):
        self.seal()
        return jobs.validate(self.output, self.root)

    def refused(self, expected="INCOMPLETE"):
        result = self.validate()
        self.assertEqual(result["observation_consistency"], expected, result)
        self.assertEqual(result["qualification_status"], "NOT_EVALUATED")
        self.assertFalse(result["admission_eligible"])
        self.assertTrue(result["errors"])

    def test_consistency_only_does_not_qualify_or_run_e1(self):
        result = self.validate()
        self.assertEqual(result["observation_consistency"], "PASS", result)
        self.assertEqual(result["qualification_status"], "NOT_EVALUATED")
        self.assertEqual((result["e1_sample_status"], result["e1_sample_count"]), ("NOT_RUN", 0))
        self.assertFalse(result["admission_eligible"])

    def test_missing_original_stream(self):
        self.seal()
        (self.output / "jobs.jsonl").unlink()
        self.assertEqual(jobs.validate(self.output, self.root)["observation_consistency"], "INCOMPLETE")

    def test_duplicate_startup(self):
        self.seal([self.startup, copy.deepcopy(self.startup)])
        self.assertEqual(jobs.validate(self.output, self.root)["observation_consistency"], "INCOMPLETE")

    def test_duplicate_receipt_id(self):
        self.probes[1]["record_id"] = self.probes[0]["record_id"]
        self.refused()

    def test_mixed_source_arm_or_owner(self):
        for key, value in (("identity_sha256", "9" * 64), ("owner_id", "8" * 64), ("attempt_id", "7" * 64)):
            original = self.probes[0][key]
            self.probes[0][key] = value
            self.refused()
            self.probes[0][key] = original

    def test_unavailable_counter_cannot_be_zero_filled(self):
        self.startup["creation_reported"] = False
        self.refused()

    def test_null_required_counter_and_boolean_count_refused(self):
        for value in (None, True):
            self.startup["creation"]["work"]["calls"]["file_create_calls"] = value
            self.refused()

    def test_original_statement_family_sum(self):
        self.startup["creation"]["work"]["sql"]["total"]["vm_steps"] += 1
        self.refused()

    def test_lifetime_allocation_cannot_be_phase_peak(self):
        self.startup["creation"]["work"]["allocation_state"]["high_water_scope"] = "phase-residency"
        self.refused()

    def test_requested_range_volume_not_physical_allocation(self):
        state = self.startup["creation"]["work"]["allocation_state"]
        state["values"]["allocated_bytes"] = 4096
        self.refused()

    def test_unavailable_whole_copy_and_eligible_debt_not_zero(self):
        for field in ("whole_operation_copy_bytes", "eligible_debt_peak_bytes", "queue_peak_jobs", "phase_resident_bytes"):
            self.manifest["forbidden_substitutions"][field] = 0
            self.refused()
            self.manifest["forbidden_substitutions"][field] = None

    def test_unreceipted_foreground_job(self):
        self.probes[1]["work"]["sql_foreground"] = self.sql(1)
        self.refused()

    def test_maintenance_is_separate_and_monotonic(self):
        self.probes[0]["work"]["sql_maintenance"] = self.sql(2)
        self.probes[1]["work"]["sql_maintenance"] = self.sql(1)
        self.refused()

    def test_result_credit_not_queue_only_or_left_held(self):
        self.probes[1]["work"]["counters"]["credited_bytes"] = 1
        self.refused()

    def test_post_ready_probe_not_inside_startup(self):
        self.probes[0]["at_ns"] = self.startup["closed_ns"] - 1
        self.refused()

    def test_complete_command_cannot_be_collector_prefix(self):
        self.outcomes["complete_command_ns"] = self.outcomes["collector_span_ns"]
        self.refused()

    def test_changed_database_custody(self):
        self.outcomes["database_artifact"]["inode"] += 1
        self.refused()

    def test_original_failed_startup_stays_failed(self):
        self.startup["creation_reported"] = False
        self.startup["creation"] = {"status": "UNAVAILABLE", "work": None, "reason": "no-original-creation-receipt"}
        self.startup["original_outcome"] = {"status": "FAILED", "error": "InvalidAdmission", "profile": None}
        self.outcomes.update(startup_status="FAILED", stop_status="UNRUN-no-ready-owner", original_error="InvalidAdmission")
        self.refused("FAIL")

    def test_global_persistence_label_is_not_local_profile(self):
        self.startup["global_persistence"] = "Durable"
        self.refused()

    def test_dirty_source_cannot_be_committed_arm(self):
        self.startup["source_binding"] = "exact-committed-arm"
        self.refused()

    def test_binary_and_artifact_tamper_refused(self):
        (self.root / "binary.artifact").write_bytes(b"unused replacement")
        self.refused()

    def test_topology_requires_observed_witness(self):
        self.identity["kernel"] = "unobserved-kernel"
        self.reidentify()
        self.refused()

    def test_docker_cannot_claim_image_not_in_scope(self):
        self.identity["execution_kind"] = "docker"
        self.identity["topology"]["artifact"] = self.artifact("docker-topology.json", {
            "schema": "cluster-two-e2-topology-witness-v1", "status": "OBSERVED",
            "scope": "external-observed-execution-domain", "execution_kind": "docker", "source": "test witness",
            "os": "linux", "architecture": "aarch64", "kernel": "fixture-kernel"})
        self.reidentify()
        self.refused()

    def test_ready_startup_cannot_omit_construction_counts(self):
        self.startup["creation"]["work"]["calls"]["file_create_calls"] = 0
        self.refused()

    def test_hashed_unused_command_is_not_executable_authority(self):
        self.identity["command_artifact"] = self.artifact("alternate-command.json", {
            "schema": "cluster-two-e2-command-v1", "execution_kind": "standalone-host",
            "argv": [str(self.root / "source.rs"), "database", "output", "identity"]})
        self.reidentify()
        self.refused()

    def test_extra_qualification_or_counter_scope_is_refused(self):
        self.manifest["evidence_admission"] = "PASS"
        self.refused()

    def test_manifest_cannot_claim_admission_or_sample(self):
        self.manifest["admission_eligible"] = True
        self.refused()

    def test_truncated_or_oversized_jsonl(self):
        self.seal()
        (self.output / "startup.jsonl").write_bytes(b"{" * (jobs.WINDOW + 1))
        self.assertEqual(jobs.validate(self.output, self.root)["observation_consistency"], "INCOMPLETE")

    def wrong_command_argument(self, index, path):
        command = json.loads((self.root / self.identity["command_artifact"]["path"]).read_bytes())
        command["argv"][index] = str(path)
        self.identity["command_artifact"] = self.artifact("mismatched-input-command.json", command)
        self.reidentify()

    def test_command_cannot_name_another_database(self):
        self.wrong_command_argument(1, self.root / "another-database")
        self.refused()

    def test_command_cannot_name_another_output(self):
        self.wrong_command_argument(2, self.root / "another-output")
        self.refused()

    def test_command_cannot_name_another_identical_identity_file(self):
        unrelated = self.root / "unrelated-identity.json"
        self.wrong_command_argument(3, unrelated)
        unrelated.write_bytes(self.identity_raw)
        self.refused()

    def test_identity_source_bytes_are_bound_to_original_invocation(self):
        self.seal()
        (self.root / "identity-input.json").write_bytes(b"{}")
        result = jobs.validate(self.output, self.root)
        self.assertEqual(result["observation_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("identity input" in error for error in result["errors"]), result)

    def test_unknown_database_observation_field(self):
        self.startup["creation"]["work"]["sql"]["invented_visited_rows"] = 0
        self.refused()

    def test_unknown_creation_work_field(self):
        self.startup["creation"]["work"]["invented_copy_bytes"] = 0
        self.refused()

    def test_same_unknown_metric_in_both_owner_snapshots_is_refused(self):
        for probe in self.probes:
            probe["work"]["invented_resident_peak"] = 0
        self.refused()

    def test_failed_startup_error_cannot_be_replaced(self):
        self.startup["creation_reported"] = False
        self.startup["creation"] = {"status": "UNAVAILABLE", "work": None, "reason": "no-original-creation-receipt"}
        self.startup["original_outcome"] = {"status": "FAILED", "error": "InvalidAdmission", "profile": None}
        self.outcomes.update(startup_status="FAILED", stop_status="UNRUN-no-ready-owner", original_error="replacement error")
        result = self.validate()
        self.assertEqual(result["observation_consistency"], "FAIL", result)
        self.assertTrue(any("original error was replaced" in error for error in result["errors"]), result)

    def test_copied_identity_reformat_and_rehash_cannot_replace_original_bytes(self):
        self.seal()
        original_source = (self.root / "identity-input.json").read_bytes()
        copied = json.dumps(self.identity, indent=2).encode()
        copied_hash = jobs.hash_bytes(copied)
        self.assertNotEqual(copied_hash, jobs.hash_bytes(original_source.strip()))
        (self.output / "identity.json").write_bytes(copied)
        for record in [self.manifest, self.startup, self.outcomes, *self.probes]:
            record["identity_sha256"] = copied_hash
            record["record_id"] = jobs.hash_bytes(f"{copied_hash}:{record['kind']}:{record['record_index']}".encode())
            record["attempt_id"] = jobs.hash_bytes(f"{copied_hash}:attempt:0".encode())
            record["owner_id"] = jobs.hash_bytes(f"{copied_hash}:owner:0".encode())
            record["invocation"]["identity_copy_sha256"] = copied_hash
        self.manifest["identity_input"].update(bytes=len(copied), sha256=copied_hash)
        for index, rows in ((0, [self.startup]), (2, self.probes)):
            descriptor = self.manifest["streams"][index]
            raw = b"".join(json.dumps(record).encode() + b"\n" for record in rows)
            (self.output / descriptor["path"]).write_bytes(raw)
            descriptor.update(bytes=len(raw), sha256=jobs.hash_bytes(raw))
        raw = json.dumps(self.outcomes).encode() + b"\n"
        (self.output / "outcomes.json").write_bytes(raw)
        self.manifest["outcomes"].update(bytes=len(raw), sha256=jobs.hash_bytes(raw))
        (self.output / "manifest.json").write_text(json.dumps(self.manifest) + "\n")
        self.assertEqual((self.root / "identity-input.json").read_bytes(), original_source)
        result = jobs.validate(self.output, self.root)
        self.assertEqual(result["observation_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("identity input" in error for error in result["errors"]), result)


class E04OriginalWork(unittest.TestCase):
    """Source-shaped Completion totals, independently of any product execution."""

    def work(self):
        return {"sql": E01Receipts().sql(1)["total"], "sql_scope": "original-job-statement-total",
                "statement_family_status": "UNAVAILABLE", "payload": dict.fromkeys(jobs.PAYLOAD, 0),
                "allocation": dict.fromkeys(jobs.ALLOCATION, 0), "parked_turns": 0,
                "queue_wait_ns": 17, "service_ns": 23}

    def endpoints(self):
        before = E01Receipts().owner()
        after = copy.deepcopy(before)
        work = self.work()
        after["sql_foreground"] = E01Receipts().sql(1)
        after["counters"].update(admitted=1, peak_credited_bytes=256)
        after["completed"][1] = 1
        after["queue_wait_ns"][1] = work["queue_wait_ns"]
        after["service_ns"][1] = work["service_ns"]
        totals = jobs.empty_job_sums()
        jobs.add_original_work(totals, work, 1)
        return before, after, totals

    def test_original_statement_total_matches_family_owner_total_without_inventing_job_families(self):
        before, after, totals = self.endpoints()
        jobs.foreground_equals(before, after, totals)

    def test_per_job_database_families_are_not_an_actual_completion_field(self):
        work = self.work()
        work["sql"] = E01Receipts().sql(1)
        with self.assertRaisesRegex(ValueError, "statement total"):
            jobs.original_job_work(work)

    def test_unknown_copy_or_visited_metric_cannot_be_added_to_original_job(self):
        work = self.work()
        work["indexed_visited_rows"] = 0
        with self.assertRaisesRegex(ValueError, "unregistered/missing"):
            jobs.original_job_work(work)

    def test_unavailable_counter_and_boolean_counter_never_mean_zero(self):
        for value in (None, True):
            work = self.work()
            work["sql"]["bound_bytes"] = value
            with self.assertRaisesRegex(ValueError, "unavailable/invalid"):
                jobs.original_job_work(work)

    def test_missing_original_completion_cannot_match_owner_foreground(self):
        before, after, _ = self.endpoints()
        with self.assertRaisesRegex(ValueError, "foreground sql"):
            jobs.foreground_equals(before, after, jobs.empty_job_sums())

    def test_duplicate_original_work_cannot_match_owner_foreground(self):
        before, after, totals = self.endpoints()
        jobs.add_original_work(totals, self.work(), 1)
        with self.assertRaisesRegex(ValueError, "foreground sql"):
            jobs.foreground_equals(before, after, totals)

    def test_automatic_maintenance_is_separate_from_original_job_sums(self):
        before, after, totals = self.endpoints()
        after["sql_maintenance"] = E01Receipts().sql(9)
        after["payload_maintenance"]["cell_zeroed_bytes"] = 4096
        after["counters"].update(maintenance_jobs=2, maintenance_rows=3, maintenance_ns=31)
        jobs.foreground_equals(before, after, totals)

    def test_maintenance_work_cannot_fill_missing_foreground_completion(self):
        before, after, totals = self.endpoints()
        after["sql_foreground"] = E01Receipts().sql()
        after["sql_maintenance"] = E01Receipts().sql(1)
        with self.assertRaisesRegex(ValueError, "foreground sql"):
            jobs.foreground_equals(before, after, totals)

    def test_caller_held_result_credit_must_remain_explicit_at_endpoint(self):
        before, after, totals = self.endpoints()
        after["counters"].update(credited_bytes=256, outstanding=1)
        with self.assertRaisesRegex(ValueError, "endpoint still owns"):
            jobs.foreground_equals(before, after, totals)

    def test_current_credit_can_decrease_without_relabeling_cumulative_work(self):
        before = E01Receipts().owner()
        after = copy.deepcopy(before)
        before["counters"].update(credited_bytes=256, outstanding=1, peak_credited_bytes=256)
        after["counters"]["peak_credited_bytes"] = 256
        jobs.owner_cumulative(before, after)

    def test_class_must_be_actual_six_class_inventory_not_boolean_or_new_lane(self):
        for value in (True, -1, 6):
            with self.assertRaisesRegex(ValueError, "class mismatch"):
                jobs.add_original_work(jobs.empty_job_sums(), self.work(), value)

    def test_source_counter_total_tamper_cannot_be_hidden_in_matching_family_inventory(self):
        before, after, totals = self.endpoints()
        after["sql_foreground"]["families"][0]["vm_steps"] += 1
        after["sql_foreground"]["total"]["vm_steps"] += 1
        with self.assertRaisesRegex(ValueError, "foreground sql"):
            jobs.foreground_equals(before, after, totals)


class E04WriteProtocol(unittest.TestCase):
    """Synthetic public-result chains, not a sampled write or product fixture."""
    def records(self):
        for index in range(jobs.E04_WRITES):
            source = {"namespace": 7, "owner": 100 + index, "root": "a" * 64}
            publication = {"namespace": 7, "generation": 1, "revision": index + 1}
            inode = {"serial": 2, "kind": 1, "mode": 0o600, "nlink": 1, "size": jobs.E04_BYTES,
                     "born": 0, "entries": 0, "inherited_cutoff": jobs.E04_BYTES,
                     "mtime_seconds": -1, "mtime_nanoseconds": 123}
            tick = [index * 10 + 1]
            def record(command, service_class, response, detail, bound_source=None, bound_publication=None):
                opened = tick[0]
                tick[0] += 1
                return {"operation_index": index, "route_ns": 7, "command_kind": command,
                    "opened_ns": opened, "closed_ns": tick[0],
                    "class": service_class, "source": bound_source, "publication": bound_publication,
                    "external_job_id": jobs.hash_bytes(f"{index}:{command}:{response}".encode()),
                    "disposition": "completed", "original_result": {"status": "OK", "response_kind": response,
                        "error": None, "custody": "original-completion-retained-during-record", "details": detail}}
            yield record("AcquireBaseSource", 5, "BaseSource", source)
            if index == 0:
                yield record("Namespace", 1, "Namespace", {"outcome": "Needs", "needs": [{"type": "inode", "serial": 2}]}, source)
            yield record("Namespace", 1, "Namespace", {"outcome": "Applied", "publication": publication, "inode": inode}, source)
            yield record("ReplyAttempted", 3, "Done", None, bound_publication=publication)
            yield record("ReleaseBaseSource", 3, "Done", None, source)

    def checked(self, records):
        return jobs.write_protocol(records, 7, "a" * 64, 2)

    def altered(self, change):
        for count, record in enumerate(self.records()):
            value = change(count, copy.deepcopy(record))
            if value is not None:
                yield value

    def test_complete_original_1000_chain_has_exact_publications_and_needs(self):
        publications, needs = self.checked(self.records())
        self.assertEqual(len(publications), 1000)
        self.assertEqual(len(needs), 1)
        self.assertEqual(publications[-1]["publication"]["revision"], 1000)

    def test_missing_reply_attempt_cannot_be_replaced_by_source_release(self):
        with self.assertRaisesRegex(ValueError, "source release"):
            self.checked(self.altered(lambda n, r: None if n == 3 else r))

    def test_reply_for_another_publication_is_refused(self):
        def change(n, record):
            if n == 3:
                record["publication"]["revision"] = 99
            return record
        with self.assertRaisesRegex(ValueError, "ReplyAttempted"):
            self.checked(self.altered(change))

    def test_release_of_another_original_source_is_refused(self):
        def change(n, record):
            if n == 4:
                record["source"]["owner"] += 1
            return record
        with self.assertRaisesRegex(ValueError, "source release"):
            self.checked(self.altered(change))

    def test_failed_original_completion_cannot_become_write_success(self):
        def change(n, record):
            if n == 2:
                record["original_result"].update(status="FAILED", response_kind="None", details=None, error="original publication failure")
            return record
        with self.assertRaisesRegex(ValueError, "failed/unattempted"):
            self.checked(self.altered(change))

    def test_unattempted_command_without_completion_is_not_applied(self):
        def change(n, record):
            if n == 2:
                record["disposition"] = "unattempted"
                record["original_result"].update(status="FAILED", error="original admission", custody="original-unattempted-command-or-pending-retained")
            return record
        with self.assertRaisesRegex(ValueError, "failed/unattempted"):
            self.checked(self.altered(change))

    def test_duplicate_needs_round_cannot_claim_fresh_base_supply(self):
        def rows():
            for n, record in enumerate(self.records()):
                yield record
                if n == 1:
                    duplicate = copy.deepcopy(record)
                    duplicate["opened_ns"] = duplicate["closed_ns"]
                    yield duplicate
        with self.assertRaisesRegex(ValueError, "duplicate/foreign"):
            self.checked(rows())

    def test_route_or_base_root_substitution_is_refused(self):
        def change(n, record):
            if n == 0:
                record["original_result"]["details"]["root"] = "b" * 64
            return record
        with self.assertRaisesRegex(ValueError, "another route/root/owner"):
            self.checked(self.altered(change))

    def test_publication_of_another_file_is_refused(self):
        def change(n, record):
            if n == 2:
                record["original_result"]["details"]["inode"]["serial"] = 3
            return record
        with self.assertRaisesRegex(ValueError, "another inode"):
            self.checked(self.altered(change))

    def test_truncated_final_source_release_is_not_success(self):
        with self.assertRaisesRegex(ValueError, "missing original write"):
            self.checked(self.altered(lambda n, r: None if n == 4000 else r))


class E04DockerMapping(unittest.TestCase):
    """Pure retained-artifact mapping proofs; no Docker command is invoked."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.cid = "7" * 64
        self.image = "sha256:" + "8" * 64
        self.argv = ["/work/binary", "endpoint:1234", "/work/assignment", "/work/base",
                     "/work/replacements", "/work/database", "/work/output", "/work/identity"]
        self.host = [str(self.root / Path(arg).relative_to("/work")) if index != 1 else arg
                     for index, arg in enumerate(self.argv)]
        self.command = {"image_id": self.image, "external_argv": ["docker", "run", "--cidfile",
            str(self.root / "cid"), "-v", str(self.root) + ":/work", "-w", "/work",
            "-e", "LAYERFS_CONSTRUCTION_WORKERS=1", self.image, *self.argv]}
        self.inspect = [{"Id": self.cid, "Image": self.image, "Path": self.argv[0], "Args": self.argv[1:],
            "Config": {"Entrypoint": None, "Cmd": self.argv.copy(), "WorkingDir": "/work",
                       "Env": ["LAYERFS_CONSTRUCTION_WORKERS=1"]}, "Mounts": [
            {"Type": "bind", "Source": str(self.root), "Destination": "/work", "RW": True}]}]
        self.outer = {"command": self.command["external_argv"], "exit_code": 0, "timed_out": False}
        self.inspected = {"command": ["docker", "inspect", self.cid], "exit_code": 0, "timed_out": False}

    def artifact(self, name, value):
        raw = value if isinstance(value, bytes) else json.dumps(value).encode()
        (self.root / name).write_bytes(raw)
        return {"path": name, "sha256": jobs.hash_bytes(raw)}

    def seal(self):
        self.value = {"schema": "cluster-two-e2-docker-path-mapping-v1", "status": "OBSERVED",
            "scope": "diagnostic-input-path-binding", "source": "independent synthetic retained receipts",
            "container_id": self.cid, "image_id": self.image, "host_root": str(self.root),
            "container_root": "/work", "collector_argv": self.argv, "host_argv": self.host,
            "artifacts": {"command_receipt": self.artifact("outer.json", self.outer),
                "inspect_receipt": self.artifact("inspect.json", self.inspected),
                "inspect_stdout": self.artifact("inspect.stdout", self.inspect),
                "cid_file": self.artifact("cid", (self.cid + "\n").encode())}}
        return self.artifact("mapping.json", self.value)

    def checked(self):
        return jobs.observed_docker_mapping(self.seal(), self.argv, self.command, self.root)

    def test_exact_observed_bind_paths_are_accepted_without_prefix_guess(self):
        self.assertEqual(self.checked(), self.host)

    def test_unavailable_mapping_remains_explicit(self):
        with self.assertRaisesRegex(ValueError, "mapping UNAVAILABLE"):
            jobs.observed_docker_mapping(None, self.argv, self.command, self.root)

    def test_raw_inspect_container_or_image_substitution_is_refused(self):
        for key in ("Id", "Image"):
            original = self.inspect[0][key]
            self.inspect[0][key] = "9" * 64
            with self.assertRaisesRegex(ValueError, "inspect identity"):
                self.checked()
            self.inspect[0][key] = original

    def test_mount_must_be_actual_writable_exact_source(self):
        for field, value in (("Source", str(self.root / "another")), ("RW", False), ("Type", "volume")):
            original = self.inspect[0]["Mounts"][0][field]
            self.inspect[0]["Mounts"][0][field] = value
            with self.assertRaisesRegex(ValueError, "bind mount"):
                self.checked()
            self.inspect[0]["Mounts"][0][field] = original

    def test_submount_cannot_redirect_a_captured_input(self):
        self.inspect[0]["Mounts"].append({"Type": "bind", "Source": str(self.root), "Destination": "/work/base", "RW": True})
        with self.assertRaisesRegex(ValueError, "overlaid"):
            self.checked()

    def test_equivalent_unused_host_input_is_not_the_captured_input(self):
        self.host[3] = str(self.root / "unrelated-base")
        with self.assertRaisesRegex(ValueError, "attested bind path"):
            self.checked()

    def test_failed_outer_command_does_not_attest_successful_mapping(self):
        self.outer["exit_code"] = 1
        with self.assertRaisesRegex(ValueError, "executed outer command"):
            self.checked()

    def test_another_inspect_command_is_not_the_original_container_observation(self):
        self.inspected["command"][-1] = "6" * 64
        with self.assertRaisesRegex(ValueError, "raw inspect"):
            self.checked()

    def test_actual_container_process_must_invoke_declared_collector(self):
        # The image, CID and bind mount can all match an unrelated successful
        # container. Its effective process still must be the captured collector.
        self.inspect[0].update(Path="/bin/true", Args=[],
            Config={"Entrypoint": None, "Cmd": ["/bin/true"]})
        with self.assertRaises(ValueError):
            self.checked()

    def test_actual_entrypoint_cannot_prepend_another_process(self):
        self.inspect[0]["Config"]["Entrypoint"] = ["/bin/sh", "-c"]
        with self.assertRaisesRegex(ValueError, "effective command"):
            self.checked()

    def test_original_outer_command_must_name_exact_image_and_collector(self):
        external = self.command["external_argv"]
        for index in (external.index(self.image), external.index(self.argv[0])):
            with self.subTest(argument=index):
                original = external[index]
                external[index] = "another-image-or-command"
                with self.assertRaisesRegex(ValueError, "image/collector"):
                    self.checked()
                external[index] = original

    def test_declared_runtime_environment_must_match_container(self):
        self.inspect[0]["Config"]["Env"] = ["LAYERFS_CONSTRUCTION_WORKERS=4"]
        with self.assertRaisesRegex(ValueError, "runtime environment differs"):
            self.checked()

    def test_cid_witness_must_be_written_by_declared_outer_command(self):
        # Both command receipts name this alternate --cidfile, while seal()
        # retains the old cid file. Equal CID contents do not bind that file to
        # this original docker run.
        self.command["external_argv"][3] = str(self.root / "unrelated-cid")
        with self.assertRaises(ValueError):
            self.checked()

    def test_mapping_artifact_tamper_is_refused(self):
        descriptor = self.seal()
        (self.root / "inspect.stdout").write_bytes(b"[]")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            jobs.observed_docker_mapping(descriptor, self.argv, self.command, self.root)

    def test_external_mapping_supplies_no_residency_or_cache_scope(self):
        descriptor = self.seal()
        self.value["phase_resident_bytes"] = 0
        descriptor = self.artifact("mapping.json", self.value)
        with self.assertRaisesRegex(ValueError, "unregistered/missing"):
            jobs.observed_docker_mapping(descriptor, self.argv, self.command, self.root)


class E04Trace(unittest.TestCase):
    def setUp(self):
        protocol = E04WriteProtocol()
        self.publications, _ = protocol.checked(protocol.records())

    def trace(self):
        for expected in jobs.write_trace(jobs.E04_BYTES):
            index = expected["index"]
            yield {**expected, "record_index": index, "opened_ns": index * 10,
                   "closed_ns": index * 10 + 8, "outcome": "Applied",
                   "publication": self.publications[index]["publication"]}

    def altered(self, field, value):
        for index, record in enumerate(self.trace()):
            if index == 0:
                record[field] = value
            yield record

    def test_exact_frozen_trace_binds_all_1000_actual_publications(self):
        jobs.write_trace_consistency(self.trace(), self.publications)

    def test_changed_aligned_offset_is_not_the_frozen_input(self):
        with self.assertRaisesRegex(ValueError, "frozen write trace"):
            jobs.write_trace_consistency(self.altered("offset", 4096), self.publications)

    def test_rehashed_replacement_or_candidate_derived_hash_is_refused(self):
        with self.assertRaisesRegex(ValueError, "frozen write trace"):
            jobs.write_trace_consistency(self.altered("replacement_sha256", "c" * 64), self.publications)

    def test_failed_original_trace_cannot_be_described_as_applied(self):
        with self.assertRaisesRegex(ValueError, "original publication/outcome"):
            jobs.write_trace_consistency(self.altered("outcome", "Failed"), self.publications)

    def test_trace_cannot_end_before_actual_source_release(self):
        with self.assertRaisesRegex(ValueError, "causal order"):
            jobs.write_trace_consistency(self.altered("closed_ns", 1), self.publications)

    def test_missing_original_trace_is_not_a_completed_window(self):
        with self.assertRaisesRegex(ValueError, "frozen write trace"):
            jobs.write_trace_consistency((row for row in self.trace() if row["index"] != 1), self.publications)


class E04Retained(unittest.TestCase):
    """Synthetic retained receipts over closed real-sized inputs; no product run."""
    @classmethod
    def setUpClass(cls):
        cls.prepared = tempfile.TemporaryDirectory()
        cls.base = Path(cls.prepared.name) / "base"
        cls.replacements = Path(cls.prepared.name) / "replacements"
        with cls.base.open("wb") as output:
            for chunk in jobs.payload_chunks(1, 0, jobs.E04_BYTES):
                output.write(chunk)
        with cls.replacements.open("wb") as output:
            for index in range(1000):
                for chunk in jobs.payload_chunks(1, index + 1, 4096):
                    output.write(chunk)
        model = bytearray(cls.base.read_bytes())
        values = cls.replacements.read_bytes()
        for index in range(1000):
            offset = 4096 * ((104729 * index) % 4096)
            model[offset:offset + 4096] = values[index * 4096:(index + 1) * 4096]
        cls.expected_sha = hashlib.sha256(model).hexdigest()

    @classmethod
    def tearDownClass(cls):
        cls.prepared.cleanup()

    def setUp(self):
        self.e01 = E01Receipts()
        self.e01.setUp()
        self.addCleanup(self.e01.doCleanups)
        self.root, self.output = self.e01.root.resolve(), self.e01.output.resolve()
        shutil.copyfile(self.base, self.root / "base")
        shutil.copyfile(self.replacements, self.root / "replacements")
        fields = ["layerfs-r4-functional-v1", "a" * 64, "b" * 64, "c" * 64, "d" * 64, "1",
                  "e" * 64, "f" * 34, "a" * 34, "main", "b" * 66, "-", "-", "c" * 64,
                  "a" * 64, "d" * 64, "e" * 64, "1", "durable", "durable"]
        assignment = ("\n".join(fields) + "\n").encode()
        self.e01.artifact("assignment", assignment)
        base_sha = jobs.hash_bytes((self.root / "base").read_bytes())
        replacement_sha = jobs.hash_bytes((self.root / "replacements").read_bytes())
        acquisition = f"layerfs-e04-acquisition-v1\n{base_sha}\n384\n0\n0\n1\n16777216\n16777216\nsource_removed=true\n".encode()
        self.e01.artifact("assignment.fixture", acquisition)
        self.host = [str(self.root / "binary.artifact"), "endpoint:1234", str(self.root / "assignment"),
                     str(self.root / "base"), str(self.root / "replacements"), str(self.root / "database"),
                     str(self.output), str(self.root / "identity-input.json")]
        self.e01.identity["command_artifact"] = self.e01.artifact("command.json", {
            "schema": "cluster-two-e2-command-v1", "execution_kind": "standalone-host", "argv": self.host})
        oracle = self.e01.artifact("e2_writes/oracle.rs", b"independent selected fixture oracle source\n")
        self.e01.identity["artifacts"]["observer"] = oracle
        product_path = self.root / self.e01.identity["artifacts"]["product"]["path"]
        product = json.loads(product_path.read_bytes())
        product["sources"].append(oracle)
        self.e01.identity["artifacts"]["product"] = self.e01.artifact("product.json", product)
        descriptors = {}
        for label, name in (("assignment", "assignment"), ("acquisition", "assignment.fixture"),
                            ("base_input", "base"), ("replacements", "replacements")):
            raw = (self.root / name).read_bytes()
            descriptors[label] = {"path": name, "sha256": jobs.hash_bytes(raw), "bytes": len(raw)}
        bundle = {"schema": "cluster-two-e04-fixture-bundle-v1", "case": "E04-write-16m", "profile": "durable", "seed": 1,
            "source_context": {"source_commit": "1" * 40, "source_tree": "2" * 40, "source_sealed": False,
                "effective_root": "a" * 64, "scope": "d" * 64, "filesystem_profile": "e" * 64, "root_serial": 1}, "inputs": descriptors}
        self.e01.identity["artifacts"]["fixture"] = self.e01.artifact("fixture-bundle.json", bundle)
        self.e01.identity_raw = json.dumps(self.e01.identity).encode()
        self.e01.invocation.update(schema="cluster-two-e2-writes-invocation-v1", argv=self.host,
            identity_source_sha256=jobs.hash_bytes(self.e01.identity_raw), identity_source_bytes=len(self.e01.identity_raw),
            identity_copy_sha256=jobs.hash_bytes(self.e01.identity_raw))
        self.e01.reidentify()
        self.manifest = self.header("manifest", 0)
        observed_inputs = {key: {**copy.deepcopy(descriptors[label]), "path": path} for key, label, path in (
            ("assignment", "assignment", self.host[2]), ("acquisition", "acquisition", str(Path(self.host[2]).with_suffix(".fixture"))),
            ("base", "base_input", self.host[3]), ("replacements", "replacements", self.host[4]))}
        self.manifest.update(driver_version="e04-original-write-receipts-v1", e1_sample_status="NOT_RUN", e1_sample_count=0,
            E05_status="NOT_RUN", observation_consistency="INCOMPLETE", write_window_consistency="NOT_EVALUATED",
            fixture_inputs={**observed_inputs, "seed": 1, "base_identity": 0, "writes": 1000},
            oracle_source={"path": "oracle-source.rs", "bytes": (self.root / oracle["path"]).stat().st_size,
                           "sha256": oracle["sha256"], "source_path": oracle["path"]},
            unavailable=list(jobs.E04_GAPS), forbidden_substitutions={"phase_resident_bytes": None,
                "eligible_debt_peak_bytes": None, "queue_peak_jobs": None, "whole_operation_copy_bytes": None},
            unrun_writes={"from_index": 1000, "to_exclusive": 1000}, database_retained=True)
        shutil.copyfile(self.root / oracle["path"], self.output / "oracle-source.rs")
        self.started = {**self.e01.startup, **self.header("startup", 0, early=True)}
        self.started["opened_ns"], self.started["closed_ns"] = 10, 100
        self.rows, self.probes, self.traces = [], [], []
        self.owner = self.e01.owner()
        self.endpoint("before-attach", 1000, early=True)
        self.job("setup", None, "Open", 3, 1001, {"namespace": 7}, "Opened")
        bootstrap = self.header("bootstrap-messages", 1)
        bootstrap.update(binding_bytes=32, policy_bytes=32, binding_sha256="1" * 64, policy_sha256="2" * 64,
            scope="original-credited-Binding-Policy-replies-retained-through-record")
        self.probes.append(bootstrap)
        self.endpoint("before-writes", 2000)
        protocol = E04WriteProtocol()
        for row in protocol.records():
            index = row["operation_index"]
            tick = row["opened_ns"] - index * 10
            when = 2000 + index * 100 + tick
            value = self.job("writes", index, row["command_kind"], row["class"], when,
                row["original_result"]["details"], row["original_result"]["response_kind"], row["source"], row["publication"])
            detail = value["original_result"]["details"]
            if isinstance(detail, dict) and detail.get("outcome") == "Needs":
                probe = self.header("base-fact-interval", len(self.probes))
                client = dict.fromkeys(jobs.CLIENT, 0)
                probe.update(operation_index=index, needs_job_index=value["record_index"], needs_external_job_id=value["external_job_id"],
                    source=row["source"], needs_count=1, status="SUPPLY_RETURNED", client_before=client, client_after=copy.deepcopy(client),
                    private_fact_provider_trace="UNAVAILABLE", scope="actual-shared-client-counters-between-Needs-and-next-round-not-per-id-provider-trace")
                self.probes.append(probe)
        self.endpoint("after-writes", 102000)
        source = {"namespace": 7, "owner": 1001, "root": "a" * 64}
        self.job("verification", None, "AcquireBaseSource", 5, 102001, source, "BaseSource")
        self.job("verification", None, "SourceInode", 0, 102003, None, "Inode", source)
        oracle = self.header("independent-oracle", len(self.probes))
        oracle.update(status="PASS", scope="separate-in-process-independent-full-state-algorithm-not-separate-process-or-performance-admission",
            bytes=jobs.E04_BYTES, final_sha256=self.expected_sha, expected_sha256=self.expected_sha, file_mode=384, nlink=1,
            mtime_seconds=-7, mtime_nanoseconds=42, old_file_content_root_unchanged=True, binding_unchanged=True,
            namespace_membership_checked=True, root_serial=1, namespace_entries=1,
            binding_scope="captured-runtime-binding-not-current-history-query")
        self.probes.append(oracle)
        self.job("verification", None, "ReleaseBaseSource", 3, 102007, None, "Done", source)
        self.job("cleanup", None, "Close", 3, 102010, None, "Done")
        self.job("cleanup", None, "CleanupState", 3, 102012, {"state": "Gone"}, "CleanupState")
        fence = self.header("native-fence", len(self.probes))
        fence.update(scope="attachment-lifetime-framing-native-reassembly-not-exclusive-physical-io", partial_messages=0,
            live_messages=0, credited_bytes=0, id_copied_bytes=0, send_framing_debug="Ok(original)", send_native_debug="Ok(original)",
            receive_native_debug="Ok(original)", receive_debug="original aggregate")
        self.probes.append(fence)
        self.endpoint("pre-stop", 102100)
        for expected in jobs.write_trace(jobs.E04_BYTES):
            index = expected["index"]
            trace = self.header("write-operation", index)
            trace.update(**expected, public_attempt_id=jobs.hash_bytes(f"{self.manifest['identity_sha256']}:public-write:{index}".encode()),
                operation_index=index, serial=2, source={"namespace": 7, "owner": 100 + index, "root": "a" * 64},
                opened_ns=2000 + index * 100, closed_ns=2020 + index * 100, outcome="Applied",
                publication={"namespace": 7, "generation": 1, "revision": index + 1},
                boundary="caller-input-through-write-reply-attempt-and-source-release", mtime_seconds=-7, mtime_nanoseconds=42)
            self.traces.append(trace)
        stat = (self.root / "database").stat()
        self.outcomes = self.header("outcomes", 0)
        self.outcomes.update(status="RECORDED", write_records=1000, complete_command_ns=None,
            complete_command_status="UNAVAILABLE-external-command-wall-required", collector_span_ns=103000, original_error=None,
            database_retained=True, E05_status="NOT_RUN", startup_status="READY", stop_status="STOPPED", write_attempts=1000,
            fixture={"route_namespace": 7, "serial": 2, "base_root": "a" * 64, "file_root": "b" * 64, "mode": 384,
                "mtime_seconds": 0, "mtime_nanoseconds": 0, "nlink": 1, "logical_len": jobs.E04_BYTES,
                "acquisition": {"base_sha256": base_sha, "source_allocated_bytes": jobs.E04_BYTES,
                                "source_copied_bytes": jobs.E04_BYTES, "source_removed": True}},
            oracle={**{key: oracle[key] for key in ("status", "bytes", "final_sha256", "expected_sha256", "file_mode", "nlink",
                "mtime_seconds", "mtime_nanoseconds", "old_file_content_root_unchanged", "binding_unchanged",
                "namespace_membership_checked", "root_serial", "namespace_entries")},
                "binding_scope": "captured-runtime-binding-not-current-history-query", "eof_checked": True},
            database_artifact={"scope": "separate-post-stop-or-original-failure-artifact-stat",
                "status": "OBSERVED", "path": self.host[5], "logical_bytes": stat.st_size, "allocated_bytes": stat.st_blocks * 512,
                "device": stat.st_dev, "inode": stat.st_ino, "links": stat.st_nlink})

    def header(self, kind, index, early=False):
        value = self.e01.header(kind, index)
        value.update(case="E04-write-16m", global_persistence=None if early else "durable")
        return value

    def endpoint(self, label, when, early=False):
        row = self.header("owner-diagnostics", len(self.probes), early)
        row.update(endpoint=label, at_ns=when, status="OBSERVED", work=copy.deepcopy(self.owner))
        self.probes.append(row)

    def job(self, phase, operation, command, service_class, when, details, response, source=None, publication=None):
        index = len(self.rows)
        row = self.header("owner-job", index)
        work = E04OriginalWork().work()
        work.update(queue_wait_ns=0, service_ns=1)
        if phase == "writes" and command == "Namespace" and isinstance(details, dict) and details.get("outcome") == "Applied":
            work["payload"].update(write_input_bytes=4096, write_cells=1, partial_write_cells=0, cell_copy_bytes=4096)
        row.update(external_job_id=jobs.hash_bytes(f"{self.manifest['identity_sha256']}:job:{index}".encode()), phase=phase,
            operation_index=operation, command_kind=command, **{"class": service_class},
            route_ns=None if command == "Open" else 7, source=source, publication=publication,
            opened_ns=when, closed_ns=when + 1, span_scope="external-attach-inclusive-boundary-exact-Open-JobWork" if command == "Open"
            else "external-command-admission-through-original-result-including-observer-wait", disposition="completed",
            original_result={"status": "OK", "response_kind": response, "error": None,
                "custody": "original-completion-retained-during-record", "details": copy.deepcopy(details)}, work=work,
            copy_scope="UNAVAILABLE-private-input-and-exclusive-copy-accounting")
        self.rows.append(row)
        family = self.owner["sql_foreground"]["families"][0]
        for key, value in work["sql"].items():
            family[key] += value
            self.owner["sql_foreground"]["total"][key] += value
        for key, value in work["payload"].items():
            self.owner["payload_foreground"][key] += value
        self.owner["counters"]["admitted"] += 1
        self.owner["counters"]["peak_credited_bytes"] = 256
        self.owner["completed"][service_class] += 1
        self.owner["service_ns"][service_class] += 1
        return row

    def seal(self):
        (self.root / "identity-input.json").write_bytes(self.e01.identity_raw)
        (self.output / "identity.json").write_bytes(self.e01.identity_raw)
        descriptors = []
        for name, rows in (("startup.jsonl", [self.started]), ("jobs.jsonl", self.rows), ("probes.jsonl", self.probes),
                           ("trace.jsonl", self.traces), ("stdout.txt", []), ("stderr.txt", [])):
            raw = b"".join(json.dumps(row).encode() + b"\n" for row in rows)
            (self.output / name).write_bytes(raw)
            descriptors.append({"path": name, "sha256": jobs.hash_bytes(raw), "bytes": len(raw), "records": len(rows),
                "status": "RECORDED" if rows else "NOT_RUN-or-empty"})
        self.manifest["streams"] = descriptors
        raw = json.dumps(self.outcomes).encode() + b"\n"
        (self.output / "outcomes.json").write_bytes(raw)
        self.manifest["outcomes"] = {"path": "outcomes.json", "sha256": jobs.hash_bytes(raw), "bytes": len(raw)}
        (self.output / "manifest.json").write_text(json.dumps(self.manifest) + "\n")

    def checked(self):
        self.seal()
        return jobs.validate(self.output, self.root)

    def test_retained_complete_write_window_still_leaves_whole_e2_incomplete(self):
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "PASS", result)
        self.assertEqual(result["observation_consistency"], "INCOMPLETE")
        self.assertFalse(result["admission_eligible"])
        self.assertEqual((result["qualification_status"], result["e1_sample_count"], result["E05_status"]), ("NOT_EVALUATED", 0, "NOT_RUN"))

    def test_actual_open_has_no_route_until_original_opened_result(self):
        # driver::run records the actual pre-Open input as route: None. The
        # newly allocated namespace belongs to the retained Opened response.
        opened = self.rows[0]
        self.assertEqual(opened["command_kind"], "Open")
        opened["route_ns"] = None
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "PASS", result)
        self.assertEqual(result["observation_consistency"], "INCOMPLETE")

    def test_open_input_cannot_claim_namespace_allocated_by_its_result(self):
        self.rows[0]["route_ns"] = self.rows[0]["original_result"]["details"]["namespace"]
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("Open input route" in error for error in result["errors"]), result)

    def test_missing_needs_interval_cannot_be_replaced_by_complete_owner_sums(self):
        self.probes = [row for row in self.probes if row["kind"] != "base-fact-interval"]
        for index, row in enumerate(self.probes):
            row.update(self.header(row["kind"], index, row.get("endpoint") == "before-attach"))
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("Needs/base interval" in error for error in result["errors"]), result)

    def test_rehashed_oracle_output_cannot_replace_independent_expected_bytes(self):
        oracle = next(row for row in self.probes if row["kind"] == "independent-oracle")
        oracle["expected_sha256"] = oracle["final_sha256"] = "f" * 64
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("full-state oracle" in error for error in result["errors"]), result)

    def test_assignment_rehash_in_mutable_manifest_cannot_replace_prepared_bundle(self):
        raw = (self.root / "assignment").read_bytes().replace(b"main\n", b"another\n")
        (self.root / "assignment").write_bytes(raw)
        self.manifest["fixture_inputs"]["assignment"]["sha256"] = jobs.hash_bytes(raw)
        result = self.checked()
        self.assertEqual(result["write_window_consistency"], "INCOMPLETE", result)
        self.assertTrue(any("hash mismatch" in error or "sealed bundle" in error for error in result["errors"]), result)

    def test_original_failed_result_is_not_described_as_a_completed_window(self):
        self.outcomes.update(status="FAILED", original_error="original source-provider failure", stop_status="UNRUN-retained-owner")
        result = self.checked()
        self.assertEqual((result["observation_consistency"], result["write_window_consistency"]), ("FAIL", "FAIL"), result)
        self.assertIn("original source-provider failure", result["errors"])

    def test_original_startup_failure_keeps_actual_not_run_stop_prefix(self):
        # Actual create-new refusal has one file-create attempt and no ready
        # owner. E04's Recorder keeps its original NOT_RUN stop disposition.
        original = "Overlay(Io(Kind(AlreadyExists)))"
        self.started["original_outcome"] = {"status": "FAILED", "error": original, "profile": None}
        self.started["creation"] = {"status": "OBSERVED", "work": {
            "calls": {"file_create_calls": 1, "sqlite_open_calls": 0,
                      "connection_configuration_calls": 0, "cache_configuration_calls": 0},
            "sql": self.e01.sql(), "allocation": dict.fromkeys(jobs.ALLOCATION, 0), "elapsed_ns": 1,
            "payload": None, "payload_status": "UNAVAILABLE-not-reported-by-CreationWork",
            "allocation_state": {"status": "UNAVAILABLE", "values": None,
                                 "reason": "allocation-owner-not-established"}}}
        self.rows, self.probes, self.traces = [], [], []
        self.manifest.update(global_persistence=None, unrun_writes={"from_index": 0, "to_exclusive": 1000})
        self.outcomes.update(global_persistence=None, status="FAILED", startup_status="FAILED", stop_status="NOT_RUN",
            original_error=original, write_records=0, write_attempts=0, fixture=None, oracle=None)
        self.seal()
        stderr = ("E04_DIAGNOSTIC_FAILED " + original + "\n").encode()
        (self.output / "stderr.txt").write_bytes(stderr)
        self.manifest["streams"][5].update(sha256=jobs.hash_bytes(stderr), bytes=len(stderr), records=1, status="RECORDED")
        (self.output / "manifest.json").write_text(json.dumps(self.manifest) + "\n")
        result = jobs.validate(self.output, self.root)
        self.assertEqual((result["observation_consistency"], result["write_window_consistency"]), ("FAIL", "FAIL"), result)
        self.assertEqual(result["errors"], [original, "original E04 startup failed; no write result is inferred"])


if __name__ == "__main__":
    unittest.main()

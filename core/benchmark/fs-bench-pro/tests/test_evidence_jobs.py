"""Independent synthetic retained-data fixtures; no product execution or sample."""
import copy
import json
from pathlib import Path
import sys
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


if __name__ == "__main__":
    unittest.main()

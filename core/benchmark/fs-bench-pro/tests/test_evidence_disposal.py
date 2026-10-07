"""Source-shaped v3 disposal and logical bind witnesses; no endpoint execution."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_jobs as jobs, evidence_disposal as disposal
import test_evidence_jobs as fixtures


class DisposalFixture:
    def __init__(self):
        self.v2 = fixtures.E04NativeRetained()
        self.v2.setUp()
        self.case, self.native = self.v2.case, self.v2.native
        case, native, e01 = self.case, self.native, self.case.e01
        self.root = case.root
        self.control_dir = self.root / "control"
        self.control_dir.mkdir()
        self.guest_dir = "/work/control"
        raw = f"layerfs-e04-disposal-control-v1\n{native.receipt_id}\n{self.control_dir}\n{self.guest_dir}\nexplicit-host-fence-v1\n".encode()
        control = self.sized("disposal.control", raw)
        e01.identity["control"] = control
        native.argv.append("/work/disposal.control")
        native.host.append(str(self.root / "disposal.control"))
        native.groups["consumer"]["outer"]["command"].append(native.argv[-1])
        native.groups["consumer"]["info"][0]["Config"]["Cmd"] = native.argv.copy()
        native.groups["consumer"]["info"][0]["Args"] = native.argv[1:]
        e01.identity["command_artifact"] = e01.artifact("command.json", {
            "schema": "cluster-two-e2-command-v1", "execution_kind": "docker", **native.command})
        e01.invocation["schema"] = "cluster-two-e2-writes-invocation-v3"
        e01.reidentify()
        native.value["schema"] = "cluster-two-e2-docker-path-mapping-v3"
        for original in (native.absence, native.created, native.volume_inspected,
                         *(group[key] for group in native.groups.values() for key in ("outer", "inspected"))):
            for key in ("started_monotonic_ns", "finished_monotonic_ns", "wall_ns"):
                original[key] *= 1_000_000
        for role, group in native.groups.items():
            info = group["info"][0]
            source = "/opaque/docker-domain/repository"
            info["HostConfig"].update(Binds=[source + ":/work:rw", native.name + ":/state:" + ("rw" if role == "consumer" else "ro")],
                ContainerIDFile=str(self.root / (role + ".cid")), VolumesFrom=None, Mounts=None, Tmpfs=None)
            info["Config"]["Volumes"] = None
            info["Mounts"][0]["Source"] = source
        for observation in (native.pre, native.post):
            observation.update(schema="e04-native-backing-observation-v2", probe_source={
                "path": "/work/" + native.probe["path"], "bytes": (self.root / native.probe["path"]).stat().st_size,
                "sha256": native.probe["sha256"]})
        case.manifest["driver_version"] = "e04-original-write-receipts-v3"
        for row in [case.manifest, case.started, case.outcomes, *case.rows, *case.probes, *case.traces]:
            self.header(row)
        names = {"binary": native.argv[0], "assignment": native.argv[2], "acquisition": "/work/assignment.fixture",
            "base": native.argv[3], "replacements": native.argv[4], "identity": native.argv[7], "control": native.argv[8]}
        (self.root / "identity-input.json").write_bytes(e01.identity_raw)
        case.manifest["pre_start_inputs"] = {"scope": "original-pre-start-logical-input-bytes", **{
            key: {"path": guest, "bytes": (self.root / Path(guest).relative_to("/work")).stat().st_size,
                  "sha256": jobs.hash_bytes((self.root / Path(guest).relative_to("/work")).read_bytes())} for key, guest in names.items()}}
        fields = (self.root / "assignment").read_text().splitlines()
        self.binding = {"kind": "final-binding", "record_index": 0, "at_ns": 102004,
            "scope": "original-final-credited-Binding-retained-through-host-ack", "complete": True, "correlation": 43,
            "bytes": len(disposal.binding_bytes(fields)), "body_hex": disposal.binding_bytes(fields).hex(),
            "sha256": jobs.hash_bytes(disposal.binding_bytes(fields)), "context": {"runtime": fields[3], "peer": fields[2],
                "workspace": fields[6], "catalog": fields[4], "incarnation": int(fields[5]), "root_serial": int(fields[17]),
                "effective_root": fields[14], "scope": fields[15], "filesystem_profile": fields[16]}}
        self.ready = f"layerfs-e04-disposal-ready-v1\n{native.receipt_id}\n43\n{self.binding['body_hex']}\n".encode()
        self.ack = f"layerfs-e04-disposal-ack-v1\n{native.receipt_id}\n43\n{self.binding['sha256']}\n".encode()
        for name, body in (("consumer-ready", self.ready), ("host-ack", self.ack)):
            (self.control_dir / (name + ".body")).write_bytes(body)
            (self.control_dir / (name + ".complete")).mkdir()
        self.control = {"scope": "Close-Gone-then-ready-host-joined-ack-before-consumer-fence",
            "ready_opened_ns": 102050, "ready_published_ns": 102051, "ack_received_ns": 102060,
            "ready": self.control_receipt("consumer-ready", self.ready), "ack": self.control_receipt("host-ack", self.ack)}
        self.control_probe = {"kind": "application-disposal", "record_index": 0, "control": self.control}
        at = next(index for index, row in enumerate(case.probes) if row["kind"] == "independent-oracle")
        case.probes.insert(at, self.binding)
        at = next(index for index, row in enumerate(case.probes) if row["kind"] == "native-fence")
        case.probes.insert(at, self.control_probe)
        self.fence = case.probes[at + 1]
        self.fence.update(close_status="OK", control_acknowledged=True, opened_ns=102070, closed_ns=102080)
        for index, row in enumerate(case.probes):
            row["record_index"] = index
            self.header(row)
        case.outcomes["application_disposal"] = copy.deepcopy(self.control)
        host_binary = self.sized("e2_writes_host", b"sealed synthetic host binary")
        self.host_argv = [str(self.root / "e2_writes_host"), str(self.root / "store"), str(self.root / "source"),
            str(self.root / "assignment"), str(self.root / "base"), "durable", str(self.root / "host-output"),
            "host.docker.internal", "80", str(self.root / "disposal.control")]
        self.host_command = {"command": self.host_argv, "exit_code": 0, "timed_out": False,
            "started_monotonic_ns": 14_000_000, "finished_monotonic_ns": 21_000_000, "wall_ns": 7_000_000}
        self.seal = {"schema": "e04-pre-host-seal-v3", "at_monotonic_ns": 8_000_000, "receipt_id": native.receipt_id,
            "control": control, "control_directory": str(self.control_dir), "guest_control_directory": self.guest_dir,
            "control_directory_empty": True, "probe_source": {**native.probe, "bytes": (self.root / native.probe["path"]).stat().st_size},
            "host_argv": self.host_argv, "image_id": native.image, "binaries": {"host": host_binary,
                "consumer": {**e01.identity["artifacts"]["binary"], "bytes": (self.root / "binary.artifact").stat().st_size}},
            "source": e01.identity["artifacts"]["product"], "sample_count": 0, "qualification_status": "NOT_EVALUATED"}
        self.rows = self.host_rows()

    def header(self, row):
        early = row["kind"] == "startup" or row.get("endpoint") == "before-attach"
        row.update(self.case.header(row["kind"], row["record_index"], early))
        row["schema"] = "cluster-two-job-receipts-v3"
        key = "external_job_id" if row["kind"] == "owner-job" else "needs_external_job_id" if row["kind"] == "base-fact-interval" else None
        if key:
            index = row["record_index"] if key == "external_job_id" else row["needs_job_index"]
            row[key] = jobs.hash_bytes(f"{row['identity_sha256']}:job:{index}".encode())
        if row["kind"] == "write-operation":
            row["public_attempt_id"] = jobs.hash_bytes(f"{row['identity_sha256']}:public-write:{row['index']}".encode())

    def sized(self, name, raw):
        return {**self.native.artifact(name, raw), "bytes": len(raw)}

    def control_receipt(self, name, body, host=False):
        directory = self.control_dir if host else Path(self.guest_dir)
        return {"run_id": self.native.receipt_id, "correlation": 43, "binding_sha256": self.binding["sha256"],
            "payload_path": str(directory / (name + ".body")), "payload_bytes": len(body), "payload_sha256": jobs.hash_bytes(body),
            "marker_path": str(directory / (name + ".complete"))}

    def host_rows(self):
        config = self.case.e01.identity["control"]
        rows = [{"kind": "disposal-control-input", "control_path": self.host_argv[9], "control_bytes": config["bytes"],
            "control_sha256": config["sha256"], "run_id": self.native.receipt_id, "control_directory": str(self.control_dir),
            "scope": "original-pre-start-control-input-read"},
            {"kind": "setup-start", "scope": "actual-Project-acquisition-and-host-initialization-separate-from-Linux-writes",
             "namespace_init_workers": 4, "ordinary_constructor_environment": "1", "profile": "durable"},
            {"kind": "setup-ready", "effective_root": "a" * 64, "scope": "d" * 64, "root_serial": 1,
             "base_sha256": self.case.manifest["fixture_inputs"]["base"]["sha256"], "source_copied_bytes": jobs.E04_BYTES,
             "source_allocated_bytes": jobs.E04_BYTES, "source_removed": True,
             "source_binding": "actual-closed-native-copy-and-public-Project-Init-not-a-Project-read-counter"},
            {"kind": "native-handshake", "native_work_debug": "original synthetic handshake", "scope": "original-authenticated-handshake-only",
             "handshake_read_timeout_ms": 5000, "host_input_idle_timeout_ms": None, "host_input_stop": "explicit-fence-or-outer-functional-watchdog"}]
        for index, operation in enumerate(("Binding", "Policy", "Binding")):
            row = {"kind": "delivered", "delivery_index": index, "operation": operation, "complete": True,
                "completed_bytes": self.binding["bytes"] if operation == "Binding" else 32, "provider_outcome": "OK",
                "provider_queue_wait_ns": 0, "provider_service_ns": 1, "scope": "original-local-socket-send-not-remote-acknowledgement"}
            if operation == "Binding":
                row.update(binding_correlation=43 if index == 2 else 1, binding_bytes=self.binding["bytes"],
                    binding_sha256=self.binding["sha256"], binding_hex=self.binding["body_hex"], binding_context_matches=True)
            rows.append(row)
        receipt = self.control_receipt("consumer-ready", self.ready, True)
        rows.append({"kind": "explicit-disposal-request", **{("ready_" + key if key.startswith(("payload_", "marker_")) else key): value
            for key, value in receipt.items()}, "final_binding_delivery_index": 2, "original_delivery_matches": True})
        rows.append({"kind": "attachment-fence", "deliveries": 3, "attachment_matches": True, "inflight_request": False,
            "input_events": 0, "output_receipts": 0, "service_cancelled": 0, "service_completed": 0,
            "input_close": "Some(Ok(()))", "output_close": "Some(Err(Quarantined))", "supervisor_failure": None,
            "expected_explicit_fence": True, "scope": "original-explicit-joined-input-output-service-custody-no-product-publication-inference",
            "input_worker": {"status": "JOINED", "original_failure": "Native(Native(Io(Kind(UnexpectedEof))))",
                "stop_kind": "interrupted-idle-read-eof", "complete_record_accounting": True, "partial_messages": 0,
                "undelivered": False, "close_failure": "None", "native_debug": "original counters", "reassembly_debug": "original counters",
                "records": 3, "record_io_attempts": 4, "plaintext_bytes": 100, "wire_bytes": 154, "live_messages": 0, "credited_bytes": 0},
            "output_worker": {"status": "JOINED", "original_failure": "None", "retained_receipts": 0,
                              "framing_debug": "original", "native_debug": "original"}})
        rows.append({"kind": "supervisor-final", **dict.fromkeys(disposal.ZERO, 0), "supervisor_work_debug": "original",
            "service_work_debug": "original", "output_work_debug": "original"})
        rows.append({"kind": "serving-scope-fenced", "save_slots_held": 0, "acknowledged_host_records": 0, "acknowledged_host_bytes": 0,
                     "assignment_sha256": self.case.manifest["fixture_inputs"]["assignment"]["sha256"]})
        rows.append({"kind": "disposal-ack-published", **self.control_receipt("host-ack", self.ack, True),
                     "scope": "original-host-joined-and-zero-owner-acknowledgment"})
        return rows

    def checked(self):
        raw = b""
        for index, row in enumerate(self.rows):
            row.update(schema="cluster-two-e04-host-v3", case="E04-write-16m", mode="diagnostic", sample_count=0,
                admission_eligible=False, qualification_status="NOT_EVALUATED", record_index=index, at_ns=index * 100)
            if row["kind"] == "serving-scope-fenced":
                row.update(acknowledged_host_records=index, acknowledged_host_bytes=len(raw))
            raw += json.dumps(row).encode() + b"\n"
        self.native.value["application_disposal"] = {"host_log": self.native.artifact("host-output/host.jsonl", raw),
            "host_command_receipt": self.native.artifact("host-command.json", self.host_command),
            "pre_host_seal": self.native.artifact("pre-host-seal.json", self.seal)}
        self.case.seal()
        return jobs.validate(self.case.output, self.root, self.native.seal())


class E04ExplicitDisposal(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixtures.E04Retained.setUpClass()

    @classmethod
    def tearDownClass(cls):
        fixtures.E04Retained.tearDownClass()

    def setUp(self):
        self.f = DisposalFixture()
        self.addCleanup(self.f.v2.doCleanups)

    def refused(self, reason):
        result = self.f.checked()
        self.assertEqual(result["write_window_consistency"], "INCOMPLETE", result)
        self.assertTrue(any(reason in value for value in result["errors"]), result)

    def row(self, kind):
        return next(row for row in self.f.rows if row["kind"] == kind)

    def test_complete_v3_requires_both_original_disposals_and_stays_unqualified(self):
        result = self.f.checked()
        self.assertEqual(result["write_window_consistency"], "PASS", result)
        self.assertEqual(result["application_disposal_consistency"], "PASS", result)
        self.assertEqual((result["observation_consistency"], result["qualification_status"], result["admission_eligible"], result["e1_sample_count"]),
                         ("INCOMPLETE", "NOT_EVALUATED", False, 0))

    def test_prospectively_selected_120s_vehicle_keeps_stricter_control_fences(self):
        self.f.host_argv[8] = "110"
        result = self.f.checked()
        self.assertEqual(result["write_window_consistency"], "PASS", result)
        self.assertEqual(result["application_disposal_consistency"], "PASS", result)
        self.assertEqual(result["e1_sample_count"], 0)

    def test_unregistered_product_cutoff_cannot_use_owner_120s_allowance(self):
        self.f.host_argv[8] = "119"
        self.refused("start order")

    def test_pre_io_quarantine_requires_no_new_read_attempt(self):
        worker = self.row("attachment-fence")["input_worker"]
        worker.update(stop_kind="pre-io-quarantined", original_failure="Native(Native(Quarantined))", record_io_attempts=3)
        self.assertEqual(self.f.checked()["write_window_consistency"], "PASS")
        worker["record_io_attempts"] = 4
        self.refused("premature quarantine")

    def test_original_closefailed_is_never_reclassified_as_eof_success(self):
        self.row("attachment-fence")["input_worker"]["original_failure"] = "Native(Native(CloseFailed { original: Io(Kind(UnexpectedEof)), close: NotConnected }))"
        self.refused("bare interrupted")

    def test_truncated_encrypted_eof_is_refused_even_when_boolean_claims_clean(self):
        self.row("attachment-fence")["input_worker"]["wire_bytes"] += 1
        self.refused("truncated encrypted")

    def test_wrong_shutdown_result_refuses_ack(self):
        self.row("attachment-fence")["input_close"] = "Some(Err(Quarantined))"
        self.refused("shutdown failed")

    def test_secondary_close_failure_remains_original_refusal(self):
        self.row("attachment-fence")["input_worker"]["close_failure"] = "Some(NotConnected)"
        self.refused("secondary close")

    def test_failed_output_with_no_retained_receipt_is_still_refused(self):
        self.row("attachment-fence")["output_worker"]["original_failure"] = "Some(original write error)"
        self.refused("output worker failed")

    def test_partial_message_cannot_be_hidden_by_ack(self):
        self.row("attachment-fence")["input_worker"]["partial_messages"] = 1
        self.refused("partial/credit")

    def test_original_final_correlation_must_match_host_delivery(self):
        [row for row in self.f.rows if row["kind"] == "delivered"][-1]["binding_correlation"] += 1
        self.refused("Binding correlation")

    def test_final_binding_body_is_bound_to_original_assignment(self):
        self.f.binding["body_hex"] = "00" * self.f.binding["bytes"]
        self.refused("Binding bytes/context")

    def test_first_binding_correlation_cannot_be_reused_as_final(self):
        [row for row in self.f.rows if row["kind"] == "delivered"][0]["binding_correlation"] = self.f.binding["correlation"]
        self.refused("Binding body/context")

    def test_missing_final_delivery_cannot_use_first_binding(self):
        self.f.rows.remove([row for row in self.f.rows if row["kind"] == "delivered"][-1])
        self.refused("exact original final Delivery")

    def test_missing_host_acknowledgment_record_is_refused(self):
        self.f.rows.pop()
        self.refused("lifecycle/acknowledgment missing")

    def test_changed_control_run_is_not_a_readiness_wait(self):
        path = self.f.control_dir / "host-ack.body"
        path.write_bytes(self.f.ack.replace(self.f.native.receipt_id.encode(), b"f" * 64))
        self.refused("control run/correlation/body")

    def test_partial_control_without_complete_marker_is_refused(self):
        (self.f.control_dir / "host-ack.complete").rmdir()
        (self.f.control_dir / "host-ack.body").write_bytes(self.f.ack[:12])
        self.refused("complete atomic publication")

    def test_consumer_fence_cannot_precede_host_ack(self):
        self.f.fence["opened_ns"] = self.f.control["ack_received_ns"] - 1
        self.refused("consumer fence order")

    def test_host_success_is_not_inferred_from_consumer_success(self):
        self.f.host_command["exit_code"] = 1
        self.refused("command failed")

    def test_control_configuration_must_be_sealed_before_host(self):
        self.f.seal["at_monotonic_ns"] = self.f.host_command["started_monotonic_ns"] + 1
        self.refused("start order")

    def test_original_input_observation_cannot_be_replaced_by_late_rehash(self):
        self.f.case.manifest["pre_start_inputs"]["base"]["sha256"] = "f" * 64
        self.refused("independent pre-start seal")

    def test_probe_observation_must_name_actual_presealed_source_bytes(self):
        self.f.native.post["probe_source"]["sha256"] = "f" * 64
        self.refused("executing probe bytes")

    def test_opaque_docker_source_is_compared_only_to_same_inspect_bind(self):
        info = self.f.native.groups["consumer"]["info"][0]
        info["Mounts"][0]["Source"] = "/host_mnt/guessed-source"
        self.refused("repository bind differs")

    def test_inspect_bind_order_is_not_identity(self):
        for group in self.f.native.groups.values():
            group["info"][0]["HostConfig"]["Binds"].reverse()
        self.assertEqual(self.f.checked()["write_window_consistency"], "PASS")

    def test_inherited_or_extra_mounts_refuse_logical_binding(self):
        self.f.native.groups["consumer"]["info"][0]["HostConfig"]["VolumesFrom"] = ["other"]
        self.refused("VolumesFrom")

    def test_changed_native_cid_file_source_is_refused(self):
        self.f.native.groups["consumer"]["info"][0]["HostConfig"]["ContainerIDFile"] = str(self.f.root / "other.cid")
        self.refused("CID/inherited")


if __name__ == "__main__":
    unittest.main()

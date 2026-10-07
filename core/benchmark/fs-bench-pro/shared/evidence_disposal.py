"""V3 retained application disposal; original native close failures stay failures.

All paths are checked against the sealed host/guest configuration. JSONL is
consumed one bounded row at a time; this module never invokes either endpoint.
"""
import hashlib
import json
from pathlib import Path, PurePosixPath

from .evidence_backing import Validator as BackingValidator

COMMON = {"schema", "case", "mode", "sample_count", "admission_eligible", "qualification_status", "kind", "record_index", "at_ns"}
RECEIPT = {"run_id", "correlation", "binding_sha256", "payload_path", "payload_bytes", "payload_sha256", "marker_path"}
ZERO = ("input_owners", "attachments", "service_outstanding", "service_credited_bytes", "output_owners",
        "output_messages", "output_credited_bytes", "output_reserved_bytes", "output_packet_capacity_bytes")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def binding_bytes(fields):
    """Independently encode the frozen assignment's existing LRP1 Binding body."""
    name = fields[9].encode("utf-8")
    incarnation, root_serial = int(fields[5]), int(fields[17])
    if not (0 < incarnation < 1 << 64 and 0 < root_serial < 1 << 63 and 0 < len(name) <= 63):
        raise ValueError("original Binding integer/name window differs")
    widths = {2: 32, 3: 32, 4: 32, 6: 32, 7: 17, 8: 17, 10: 33, 13: 32, 14: 32, 15: 32, 16: 32}
    for index, size in widths.items():
        if len(bytes.fromhex(fields[index])) != size:
            raise ValueError("original Binding identity width differs")
    if (fields[11] == "-") != (fields[12] == "-"):
        raise ValueError("original Binding optional head context differs")
    for index, size in ((11, 33), (12, 32)):
        if fields[index] != "-" and len(bytes.fromhex(fields[index])) != size:
            raise ValueError("original Binding head identity width differs")
    body = b"LRP1\x00\x0a\x00\x00"
    body += b"".join(bytes.fromhex(fields[index]) for index in (3, 2, 6, 4))
    body += incarnation.to_bytes(8, "big") + root_serial.to_bytes(8, "big")
    body += bytes.fromhex(fields[7]) + bytes.fromhex(fields[8]) + len(name).to_bytes(4, "big") + name
    body += bytes.fromhex(fields[10])
    for index in (11, 12):
        body += b"\x00" if fields[index] == "-" else b"\x01" + bytes.fromhex(fields[index])
    return body + b"".join(bytes.fromhex(fields[index]) for index in (13, 14, 15, 16))


class Validator(BackingValidator):
    def raw(self, path, maximum):
        path = Path(path)
        self.require(path.is_file() and not path.is_symlink(), "original disposal input missing/aliased")
        with path.open("rb") as source:
            raw = source.read(maximum + 1)
        self.require(len(raw) <= maximum, "original disposal read window exceeded")
        return raw

    def fields(self, raw, count):
        self.require(raw.endswith(b"\n") and b"\r" not in raw and b"\x00" not in raw,
                     "original disposal control incomplete or malformed")
        fields = raw.decode("ascii").splitlines()
        self.require(len(fields) == count and all(fields), "original disposal control field inventory differs")
        return fields

    def configuration(self, identity, host, root):
        descriptor = self.exact(identity.get("control"), ("path", "bytes", "sha256"), "sealed disposal configuration")
        path = self.artifact(descriptor, root)
        raw = self.raw(path, 8192)
        self.require(path == Path(host[8]) and self.integer(descriptor["bytes"]) and len(raw) == descriptor["bytes"],
                     "original control configuration path/bytes differs")
        fields = self.fields(raw, 5)
        directory, guest = Path(fields[2]), PurePosixPath(fields[3])
        self.require(fields[0] == "layerfs-e04-disposal-control-v1" and fields[1] == identity["receipt_id"]
            and fields[4] == "explicit-host-fence-v1" and directory.is_absolute() and directory == directory.resolve()
            and directory.is_relative_to(Path(root).resolve()) and directory.is_dir() and not directory.is_symlink()
            and guest == PurePosixPath("/work") / directory.relative_to(Path(root).resolve()).as_posix(),
            "original control run/host/guest directory identity differs")
        return {"run_id": fields[1], "directory": directory, "guest": guest, "descriptor": descriptor, "path": path}

    def pre_host(self, native, identity, config, host, manifest, root):
        witness = self.exact(native["application_disposal"], ("host_log", "host_command_receipt", "pre_host_seal"),
                             "v3 original host disposal witnesses")
        seal = self.read(witness["pre_host_seal"], root)
        self.exact(seal, ("schema", "at_monotonic_ns", "receipt_id", "control", "control_directory", "guest_control_directory",
            "control_directory_empty", "probe_source", "host_argv", "image_id", "binaries", "source", "sample_count", "qualification_status"),
            "v3 original pre-host seal")
        self.require(seal["schema"] == "e04-pre-host-seal-v3" and self.integer(seal["at_monotonic_ns"])
            and seal["receipt_id"] == config["run_id"] and seal["control"] == config["descriptor"]
            and seal["control_directory"] == str(config["directory"]) and seal["guest_control_directory"] == str(config["guest"])
            and seal["control_directory_empty"] is True and seal["image_id"] == native["image_id"]
            and seal["source"] == identity["artifacts"]["product"] and type(seal["sample_count"]) is int
            and seal["sample_count"] == 0 and seal["qualification_status"] == "NOT_EVALUATED",
            "v3 disposal configuration/probe was not sealed before host")
        probe = self.exact(seal["probe_source"], ("path", "bytes", "sha256"), "pre-host probe source")
        probe_path = self.artifact(probe, root)
        self.require(probe_path == self.artifact(native["probe_source"], root)
            and probe["sha256"] == native["probe_source"]["sha256"] and self.integer(probe["bytes"])
            and probe["bytes"] == probe_path.stat().st_size, "v3 executing probe differs from original pre-host source")
        binaries = self.exact(seal["binaries"], ("host", "consumer"), "pre-host binary identities")
        paths = {}
        for role in ("host", "consumer"):
            value = self.exact(binaries[role], ("path", "bytes", "sha256"), "pre-host " + role + " binary")
            paths[role] = self.artifact(value, root)
            self.require(self.integer(value["bytes"]) and value["bytes"] == paths[role].stat().st_size,
                         "pre-host original binary bytes changed")
        self.require(paths["consumer"] == Path(host[0]) and binaries["consumer"]["sha256"] == identity["artifacts"]["binary"]["sha256"],
                     "pre-host consumer binary identity differs")
        # Historical90s and prospectively owner-authorized120s complete vehicles
        # reserve10s for terminal observation/disposal; controls remain5s.
        original = self.receipt(witness["host_command_receipt"], root, seal["host_argv"])
        argv = original["command"]
        host_log = self.artifact(witness["host_log"], root)
        self.require(len(argv) == 10 and argv[0] == str(paths["host"]) and argv[3] == host[2] and argv[4] == host[3]
            and argv[5] == manifest["global_persistence"] and argv[6] == str(host_log.parent)
            and argv[8] in ("80", "110") and argv[9] == host[8] and host_log.name == "host.jsonl"
            and seal["at_monotonic_ns"] <= original["started_monotonic_ns"]
            and seal["at_monotonic_ns"] <= native["pre_start_command"]["started_monotonic_ns"]
            and original["started_monotonic_ns"] <= native["consumer_command"]["started_monotonic_ns"] <= original["finished_monotonic_ns"],
            "v3 original host argv/configuration/start order differs")
        return host_log

    def final_binding(self, record, host):
        fields = self.raw(host[2], 8192).decode("utf-8").splitlines()
        self.require(len(fields) == 20, "final Binding original assignment unavailable")
        expected = binding_bytes(fields)
        self.require(record["scope"] == "original-final-credited-Binding-retained-through-host-ack"
            and record["complete"] is True and self.integer(record["correlation"]) and record["correlation"] > 0
            and self.integer(record["bytes"]) and 0 < record["bytes"] <= 4096
            and record["bytes"] == len(expected) and record["body_hex"] == expected.hex()
            and record["sha256"] == digest(expected), "original final Binding bytes/context/correlation differs")
        context = {"runtime": fields[3], "peer": fields[2], "workspace": fields[6], "catalog": fields[4],
            "incarnation": int(fields[5]), "root_serial": int(fields[17]), "effective_root": fields[14],
            "scope": fields[15], "filesystem_profile": fields[16]}
        self.require(record["context"] == context
            and type(record["context"]["incarnation"]) is int and type(record["context"]["root_serial"]) is int,
            "original final Binding typed context changed")
        return expected

    def control_receipt(self, value, name, config, binding, body, guest=True):
        self.exact(value, RECEIPT, "original " + name + " receipt")
        directory = config["guest"] if guest else config["directory"]
        self.require(value["run_id"] == config["run_id"] and type(value["correlation"]) is int
            and value["correlation"] == binding["correlation"] and value["binding_sha256"] == binding["sha256"]
            and value["payload_path"] == str(directory / (name + ".body"))
            and value["marker_path"] == str(directory / (name + ".complete"))
            and type(value["payload_bytes"]) is int and value["payload_bytes"] == len(body)
            and value["payload_sha256"] == digest(body), "original control receipt differs from run/Binding/published bytes")
        return value

    def controls(self, value, config, binding, expected):
        self.exact(value, ("scope", "ready_opened_ns", "ready_published_ns", "ack_received_ns", "ready", "ack"),
                   "original consumer application disposal")
        self.require(value["scope"] == "Close-Gone-then-ready-host-joined-ack-before-consumer-fence"
            and all(self.integer(value[key]) for key in ("ready_opened_ns", "ready_published_ns", "ack_received_ns"))
            and value["ready_opened_ns"] <= value["ready_published_ns"] <= value["ack_received_ns"]
            and value["ack_received_ns"] - value["ready_published_ns"] <= 5_000_000_000,
            "original consumer control ordering/bound differs")
        ready = f"layerfs-e04-disposal-ready-v1\n{config['run_id']}\n{binding['correlation']}\n{expected.hex()}\n".encode()
        ack = f"layerfs-e04-disposal-ack-v1\n{config['run_id']}\n{binding['correlation']}\n{binding['sha256']}\n".encode()
        for key, name, body in (("ready", "consumer-ready", ready), ("ack", "host-ack", ack)):
            marker = config["directory"] / (name + ".complete")
            self.require(marker.is_dir() and not marker.is_symlink() and not any(marker.iterdir()),
                         "original control lacks complete atomic publication marker")
            self.require(self.raw(config["directory"] / (name + ".body"), 16_384) == body,
                         "original control run/correlation/body changed or is partial")
            self.control_receipt(value[key], name, config, binding, body)
        return ready, ack

    def joined(self, row):
        self.require(row["scope"] == "original-explicit-joined-input-output-service-custody-no-product-publication-inference"
            and row["expected_explicit_fence"] is True and row["attachment_matches"] is True
            and row["inflight_request"] is False and row["supervisor_failure"] is None
            and row["input_close"] == "Some(Ok(()))" and row["output_close"] == "Some(Err(Quarantined))"
            and all(type(row[key]) is int and row[key] == 0 for key in
                ("input_events", "output_receipts", "service_cancelled", "service_completed")),
            "original explicit host shutdown failed or retained custody")
        incoming = self.exact(row["input_worker"], ("status", "original_failure", "stop_kind", "complete_record_accounting",
            "partial_messages", "undelivered", "close_failure", "native_debug", "reassembly_debug", "records", "record_io_attempts",
            "plaintext_bytes", "wire_bytes", "live_messages", "credited_bytes"), "original joined input worker")
        self.require(incoming["status"] == "JOINED" and incoming["complete_record_accounting"] is True
            and incoming["undelivered"] is False and incoming["close_failure"] == "None"
            and all(type(incoming[key]) is int and incoming[key] == 0 for key in ("partial_messages", "live_messages", "credited_bytes"))
            and all(self.integer(incoming[key]) for key in ("records", "record_io_attempts", "plaintext_bytes", "wire_bytes")),
            "original joined input worker has partial/credit/secondary close failure")
        if incoming["stop_kind"] == "pre-io-quarantined":
            self.require(incoming["original_failure"] == "Native(Native(Quarantined))"
                and incoming["record_io_attempts"] == incoming["records"], "premature quarantine cannot replace interrupted idle input")
        else:
            self.require(incoming["stop_kind"] == "interrupted-idle-read-eof"
                and incoming["original_failure"] == "Native(Native(Io(Kind(UnexpectedEof))))"
                and incoming["record_io_attempts"] == incoming["records"] + 1,
                "original joined input is not bare interrupted idle-read EOF")
        self.require(incoming["wire_bytes"] == incoming["plaintext_bytes"] + 18 * incoming["records"],
                     "truncated encrypted EOF cannot qualify explicit disposal")
        outgoing = self.exact(row["output_worker"], ("status", "original_failure", "retained_receipts", "framing_debug", "native_debug"),
                              "original joined output worker")
        self.require(outgoing["status"] == "JOINED" and outgoing["original_failure"] == "None"
            and type(outgoing["retained_receipts"]) is int and outgoing["retained_receipts"] == 0,
            "original joined output worker failed or retained receipt")

    def host_rows(self, path):
        def unique(pairs):
            result = {}
            for key, value in pairs:
                self.require(key not in result, "duplicate original host row key")
                result[key] = value
            return result
        previous = 0
        with path.open("rb") as source:
            for index, line in enumerate(iter(lambda: source.readline(65_537), b"")):
                self.require(len(line) <= 65_536 and line.endswith(b"\n"), "original host JSONL window/truncated row")
                row = json.loads(line, object_pairs_hook=unique)
                self.require(isinstance(row, dict) and row.get("schema") == "cluster-two-e04-host-v3"
                    and row.get("case") == "E04-write-16m" and row.get("mode") == "diagnostic"
                    and type(row.get("sample_count")) is int and row["sample_count"] == 0
                    and row.get("admission_eligible") is False and row.get("qualification_status") == "NOT_EVALUATED"
                    and type(row.get("record_index")) is int and row["record_index"] == index
                    and self.integer(row.get("at_ns")) and row["at_ns"] >= previous,
                    "original host receipt version/index/scope/order differs")
                previous = row["at_ns"]
                yield row, len(line)

    def host(self, path, config, binding, bodies, manifest):
        names = ("disposal-control-input", "setup-start", "setup-ready", "native-handshake", "delivered",
                 "explicit-disposal-request", "attachment-fence", "supervisor-final", "serving-scope-fenced", "disposal-ack-published")
        seen, stage, deliveries, bindings, final_index = set(), -1, 0, 0, None
        request_ns, prefix_bytes = None, 0
        for row, length in self.host_rows(path):
            kind = row.get("kind")
            self.require(kind in names, "unregistered original host lifecycle record")
            current = names.index(kind)
            self.require(current >= stage and (kind == "delivered" or kind not in seen), "original host lifecycle duplicated/reordered")
            stage = current
            seen.add(kind)
            if kind == "delivered":
                self.require(bindings < 2, "runtime delivery followed the final Binding before disposal")
                fields = {"delivery_index", "operation", "complete", "completed_bytes", "provider_outcome", "provider_queue_wait_ns", "provider_service_ns", "scope"}
                if row.get("operation") == "Binding":
                    fields |= {"binding_correlation", "binding_bytes", "binding_sha256", "binding_hex", "binding_context_matches"}
                self.exact(row, COMMON | fields, "original host Delivery")
                self.require(row["delivery_index"] == deliveries and type(row["delivery_index"]) is int
                    and row["complete"] is True and row["provider_outcome"] == "OK" and self.integer(row["completed_bytes"])
                    and row["scope"] == "original-local-socket-send-not-remote-acknowledgement", "original host Delivery failed or incomplete")
                deliveries += 1
                if row["operation"] == "Binding":
                    bindings += 1
                    self.require(bindings <= 2 and row["binding_context_matches"] is True
                        and self.integer(row["binding_correlation"]) and row["binding_correlation"] > 0
                        and (bindings != 1 or row["binding_correlation"] < binding["correlation"])
                        and row["binding_bytes"] == row["completed_bytes"] == binding["bytes"]
                        and row["binding_hex"] == binding["body_hex"] and row["binding_sha256"] == binding["sha256"],
                        "original host Binding body/context differs")
                    if bindings == 2:
                        self.require(row["binding_correlation"] == binding["correlation"], "final original host Binding correlation differs")
                        final_index = row["delivery_index"]
            elif kind == "disposal-control-input":
                self.exact(row, COMMON | {"control_path", "control_bytes", "control_sha256", "run_id", "control_directory", "scope"}, kind)
                self.require(row["control_path"] == str(config["path"]) and row["control_bytes"] == config["descriptor"]["bytes"]
                    and row["control_sha256"] == config["descriptor"]["sha256"] and row["run_id"] == config["run_id"]
                    and row["control_directory"] == str(config["directory"]) and row["scope"] == "original-pre-start-control-input-read",
                    "host original pre-start control read differs")
            elif kind in ("explicit-disposal-request", "disposal-ack-published"):
                prefix = "ready_" if kind == "explicit-disposal-request" else ""
                fields = {prefix + key if key.startswith(("payload_", "marker_")) else key for key in RECEIPT}
                self.exact(row, COMMON | fields | ({"final_binding_delivery_index", "original_delivery_matches"} if prefix else {"scope"}), kind)
                receipt = {key: row[prefix + key if key.startswith(("payload_", "marker_")) else key] for key in RECEIPT}
                self.control_receipt(receipt, "consumer-ready" if prefix else "host-ack", config, binding, bodies[0 if prefix else 1], False)
                if prefix:
                    self.require(bindings == 2 and final_index == row["final_binding_delivery_index"]
                        and row["original_delivery_matches"] is True, "host fence lacks exact original final Delivery")
                    request_ns = row["at_ns"]
                else:
                    self.require(row["scope"] == "original-host-joined-and-zero-owner-acknowledgment"
                        and request_ns is not None and row["at_ns"] - request_ns <= 5_000_000_000,
                        "host acknowledgment precedes joined fence or exceeds original bound")
            elif kind == "attachment-fence":
                self.exact(row, COMMON | {"deliveries", "attachment_matches", "inflight_request", "input_events", "output_receipts",
                    "service_cancelled", "service_completed", "input_close", "output_close", "supervisor_failure", "input_worker",
                    "output_worker", "expected_explicit_fence", "scope"}, kind)
                self.require(row["deliveries"] == deliveries and request_ns is not None, "host native fence preceded matching Ready")
                self.joined(row)
            elif kind == "supervisor-final":
                self.exact(row, COMMON | set(ZERO) | {"supervisor_work_debug", "service_work_debug", "output_work_debug"}, kind)
                self.require(all(type(row[key]) is int and row[key] == 0 for key in ZERO), "host original final credits remain held")
            elif kind == "serving-scope-fenced":
                self.exact(row, COMMON | {"save_slots_held", "acknowledged_host_records", "acknowledged_host_bytes", "assignment_sha256"}, kind)
                self.require(type(row["save_slots_held"]) is int and row["save_slots_held"] == 0
                    and row["acknowledged_host_records"] == row["record_index"]
                    and type(row["acknowledged_host_bytes"]) is int and row["acknowledged_host_bytes"] == prefix_bytes
                    and row["assignment_sha256"] == manifest["fixture_inputs"]["assignment"]["sha256"], "host Save/original log custody differs")
            else:
                expected_fields = {
                    "setup-start": {"scope", "namespace_init_workers", "ordinary_constructor_environment", "profile"},
                    "setup-ready": {"effective_root", "scope", "root_serial", "base_sha256", "source_copied_bytes", "source_allocated_bytes", "source_removed", "source_binding"},
                    "native-handshake": {"native_work_debug", "scope", "handshake_read_timeout_ms", "host_input_idle_timeout_ms", "host_input_stop"},
                }
                self.exact(row, COMMON | expected_fields[kind], kind)
                if kind == "setup-start":
                    self.require(row["namespace_init_workers"] == 4 and type(row["namespace_init_workers"]) is int
                        and row["ordinary_constructor_environment"] == "1" and row["profile"] == manifest["global_persistence"],
                        "original host setup profile/construction scope differs")
                elif kind == "native-handshake":
                    self.require(row["scope"] == "original-authenticated-handshake-only"
                        and row["handshake_read_timeout_ms"] == 5000 and type(row["handshake_read_timeout_ms"]) is int
                        and row["host_input_idle_timeout_ms"] is None
                        and row["host_input_stop"] == "explicit-fence-or-outer-functional-watchdog",
                        "original host handshake/input lifetime profile changed")
            prefix_bytes += length
        self.require(seen == set(names) and bindings == 2, "original host disposal lifecycle/acknowledgment missing")

    def validate(self, manifest, outcomes, binding, disposal, fence, native, host, root, close_ns, gone_ns, after_writes, pre_stop):
        config = self.configuration(manifest["identity"], host, root)
        host_log = self.pre_host(native, manifest["identity"], config, host, manifest, root)
        expected = self.final_binding(binding, host)
        value = disposal["control"]
        bodies = self.controls(value, config, binding, expected)
        self.require(outcomes["application_disposal"] == value and self.integer(binding["at_ns"])
            and after_writes <= binding["at_ns"] <= close_ns <= gone_ns <= value["ready_opened_ns"]
            and fence["close_status"] == "OK" and fence["control_acknowledged"] is True
            and self.integer(fence["opened_ns"]) and self.integer(fence["closed_ns"])
            and value["ack_received_ns"] <= fence["opened_ns"] <= fence["closed_ns"] <= pre_stop
            and fence["closed_ns"] - fence["opened_ns"] <= 5_000_000_000,
            "original Close/Gone/control/consumer fence order or result differs")
        self.host(host_log, config, binding, bodies, manifest)

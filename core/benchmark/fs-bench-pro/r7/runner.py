"""Prospective R7 orchestration over the real SDK harness and ordinary Bash.

Run only under the checkout lock. Inputs/volumes/images/binaries must already be
sealed and independently prepared. This runner never builds, retries, removes,
initializes a replacement fixture, or guesses cancellation after a host timeout.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import selectors
import shlex
import subprocess
import time

from . import counts, deployment, receipts, registry, registry_variant, source_inventory, verification as verification_module, workloads


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for data in iter(lambda: stream.read(65536), b""):
            digest.update(data)
    return digest.hexdigest()


def write_new(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def claim_key(selection, identities, deployment_identity=None, *, effective=None):
    if effective is not None:
        # Free display labels, fresh container/Workspace identities and cloned
        # physical inode numbers cannot manufacture another sample.
        body = [selection["selection_id"], effective]
        return hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()
    # Product changes do not invalidate N/P controls. Their actual binary,
    # image, harness, workload, input and cache treatment remain the identity.
    keys = receipts.IDENTITIES if selection["arm"] == "L" else ["binary_sha256", "image_id", "harness", "workload", "fixture", "oracle", "report_generator"]
    body = [selection["selection_id"], {key: identities[key] for key in keys}, deployment_identity]
    return hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()


def effective_treatment(config, selection, verified_source):
    """Bind a claim to checked bytes and selected semantics, not display labels."""
    if verified_source.get("status") != "PASS" or not verified_source.get("source_set_sha256"):
        raise ValueError("actual verified source set required for sample claim")
    plan = deployment.load_plan(config, selection)
    binaries = {name: sha(config[name + "_binary"]) for name in ("runtime", "daemon")}
    if selection["arm"] == "P":
        binaries["passthrough"] = sha(config["passthrough_binary"])
    scripts = {name: sha(config[name]) for name in ("oracle_script", "fresh_oracle_script") if config.get(name)}
    return {"schema": "r7-effective-treatment-v1", "source_set_sha256": verified_source["source_set_sha256"],
            "binaries": binaries, "image_id": registry.IMAGE, "uid": config["uid"], "gid": config["gid"],
            "workload": workloads.workload_identity(), "environment": workloads.ENV,
            "manifest_sha256": sha(config["manifest"]), "prepared_store_sha256": sha(config["prepared_store_file"]),
            "deployment_implementation": sha(deployment.__file__),
            "inputs": [{"target": item["target"], "content_metadata_set_sha256": item["content_metadata_set_sha256"]}
                       for item in plan["assets"] + plan["native"]],
            "oracle_scripts": scripts, "oracle_recipe_sha256": config.get("oracle_recipe", {}).get("closed_sha256"),
            "oracle_variant": config.get("oracle_variant"),
            "git_index_oracle": {key: value for key, value in config.get("git_index_oracle", {}).items()
                                 if key != "git_version"},
            "passthrough_profile": P_PROFILE if selection["arm"] == "P" else None,
            "native_root": config["native_root"] if selection["arm"] != "L" else None,
            "peers": config.get("native_peer_roots") if selection["arm"] != "L" else None}


class PerformanceClock:
    """Controller performance intervals, excluding even failed verifier attempts."""
    def __init__(self):
        self.started = None
        self.stopped = None
        self.excluded = []
        self.verifier_started = None

    def begin(self, now):
        self.started = now

    def begin_verifier(self, now):
        if self.verifier_started is not None:
            raise ValueError("nested verifier interval")
        self.verifier_started = now

    def end_verifier(self, now):
        if self.verifier_started is None:
            raise ValueError("verifier interval was not started")
        self.excluded.append((self.verifier_started, now))
        self.verifier_started = None

    def finish(self, now):
        self.stopped = now

    def observe(self, now):
        end = self.stopped if self.stopped is not None else now
        excluded = [*self.excluded]
        if self.verifier_started is not None:
            excluded.append((self.verifier_started, end))
        duration = None if self.started is None else end - self.started - sum(right-left for left,right in excluded)
        return {"start_ns": self.started, "end_ns": end if self.started is not None else None,
                "performance_ns": duration, "excluded_verifier_intervals": excluded,
                "scope": "actual controller observation; failed verifier time excluded; cleanup/stop excluded after terminal finish"}


def shell_parallel(commands):
    """One ordinary parent, real concurrent Bash children, explicit waits."""
    lines = ["pids=(); failure=0"]
    for directory, body in commands:
        lines.append(f"(cd -- {shlex.quote(directory)} && /bin/bash -o pipefail -c {shlex.quote(body)}) & pids+=(\"$!\")")
    lines.append('for pid in "${pids[@]}"; do if wait "$pid"; then code=0; else code=$?; if test "$failure" -eq 0; then failure=$code; fi; fi; printf "R7_CHILD_RESULT pid=%s exit=%s\\n" "$pid" "$code" >&2; done; exit "$failure"')
    return "\n".join(lines)


def control_peers(config, selection):
    """Validate supplied prepared peers; no source-copy/setup implementation."""
    if selection["arm"] == "L" or not selection["case_id"].startswith("W"):
        return []
    roots = config.get("native_peer_roots", [config["native_root"]])
    expected = 2 if selection["case_id"] == "W01" else 1
    if not isinstance(roots, list) or len(roots) != expected or roots[0] != config["native_root"]:
        raise ValueError("exact independently prepared native peer roots required")
    if not config.get("peer_preparation_identity") or not (config.get("oracle_script") or config.get("oracle_recipe")):
        raise ValueError("sealed peer preparation identity and scoped concurrency oracle required")
    mounts = config.get("passthrough_peer_mounts", [config.get("passthrough_mount")]) if selection["arm"] == "P" else roots
    if not isinstance(mounts, list) or len(mounts) != expected or (selection["arm"] == "P" and mounts[0] != config["passthrough_mount"]):
        raise ValueError("one independent P mountpoint per prepared native peer required")
    for paths in (roots, mounts):
        for path in paths:
            if not isinstance(path, str) or not path.startswith("/") or any(character in path for character in "\n\r\t") or ".." in Path(path).parts:
                raise ValueError("peer paths must be absolute unambiguous prepared directories")
        if len(set(map(Path, paths))) != len(paths) or any(left != right and Path(left) in Path(right).parents for left in paths for right in paths):
            raise ValueError("peer directories must be distinct and non-overlapping")
    if selection["arm"] == "P" and any(Path(left) == Path(right) or Path(left) in Path(right).parents or Path(right) in Path(left).parents for left in roots for right in mounts):
        raise ValueError("P backing roots and mountpoints must not overlap")
    return list(zip(roots, mounts))


P_PROFILE = {"max_write": 131072, "max_readahead": 131072, "max_background": 1,
             "congestion_threshold": 1, "configured_receive_loops": 2, "ttl_seconds": 60,
             "writeback": False, "default_permissions": True}


def passthrough_ready(process, ready):
    if ready.get("phase") != "Serving" or [ready.get(key) for key in ("configured", "created", "entered", "exited", "joined")] != [2,2,2,0,0]:
        raise OriginalFailure("original P ready event did not establish two live receive loops")
    negotiated = [row for row in process.rows if row.get("event") == "negotiated"]
    expected = P_PROFILE
    if len(negotiated) != 1 or any(negotiated[0].get(key) != value for key,value in expected.items()):
        raise OriginalFailure("original negotiated P profile differs from matched profile")
    return negotiated[0]


def child_results(event):
    path = event.get("fields", {}).get("stderr")
    if not path:
        raise OriginalFailure("original concurrent child stderr custody missing")
    results = []
    with Path(path).open() as stream:
        for line in stream:
            if line.startswith("R7_CHILD_RESULT "):
                fields = dict(item.split("=",1) for item in line.split()[1:])
                results.append({"pid": int(fields["pid"]), "exit": int(fields["exit"])})
    if len(results) != 2 or len({item["pid"] for item in results}) != 2 or any(item["pid"] <= 0 or item["exit"] != 0 for item in results):
        raise OriginalFailure("original concurrent parent did not report exactly two distinct successful children")
    return results


class OriginalFailure(RuntimeError):
    pass


class EventProcess:
    """Continuously drain finite event lines; no timeout-polling exit timer."""
    def __init__(self, argv, folder, label):
        self.folder = Path(folder)
        self.label = label
        self.argv = argv
        self.stderr = self.raw = self.process = self.selector = None
        self.buffer = b""
        self.rows = []
        self.container = None
        self.daemon_instance = None
        self.scope = None
        self.exec_ids = []
        self.last_sequence = 0
        self._host_fence_attempted = False
        self._host_fence_result = None
        self._retained_cause = None
        # Paths are declared child-owned original evidence, not guessed IDs or
        # outcomes. A child may acknowledge a container before stdout is read.
        self.child_event_receipts = [str(argv[index + 1]) for index, value in enumerate(argv[:-1])
                                     if value == "--receipt"]
        phase = "event_process_stderr_open"
        try:
            self.stderr = (self.folder / (label + ".stderr")).open("xb", buffering=0)
            phase = "event_process_raw_open"
            self.raw = (self.folder / (label + ".stdout")).open("xb", buffering=0)
            phase = "event_process_host_launch"
            self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=self.stderr, start_new_session=True, bufsize=0)
            phase = "event_process_selector_create"
            self.selector = selectors.DefaultSelector()
            phase = "event_process_selector_register"
            self.selector.register(self.process.stdout, selectors.EVENT_READ)
            phase = "event_process_host_custody_output"
            write_new(self.folder / (label + ".host-custody.json"),
                      {"argv": argv, "host_pid": self.process.pid, "host_group": self.process.pid,
                       "child_event_receipts": self.child_event_receipts,
                       "timeout_disposition": "host timeout does not stop Docker exec/container; exact owned IDs retained; lead explicit cancellation/stop only"})
        except BaseException as original:
            original.event_process = self
            if not hasattr(original, "original_phase"):
                original.original_phase = phase
            # retain fences in its finally even when its own receipt fails.
            # The exact original exception remains the constructor outcome.
            self.retain(original)
            for name, value in (("selector", self.selector), ("raw", self.raw), ("stderr", self.stderr),
                                ("host_stdin", getattr(self.process, "stdin", None)),
                                ("host_stdout", getattr(self.process, "stdout", None))):
                if value is not None:
                    try:
                        value.close()
                    except BaseException as closing:
                        previous = getattr(original, "independent_close_failures", [])
                        original.independent_close_failures = [*previous, str(closing)]
                        previous = getattr(original, "independent_custody_failures", [])
                        original.independent_custody_failures = [*previous, {"phase": name + "_close", "error": closing}]
            raise

    def event(self, expected, deadline):
        while True:
            while b"\n" in self.buffer:
                line, self.buffer = self.buffer.split(b"\n", 1)
                row = json.loads(line)
                if "sequence" in row:
                    if row["sequence"] != self.last_sequence + 1:
                        raise OriginalFailure("missing or duplicate original runtime event")
                    self.last_sequence = row["sequence"]
                self.rows.append(row)
                fields = row.get("fields", {})
                if fields.get("container"):
                    self.container = fields["container"]
                if fields.get("daemon_instance"):
                    self.daemon_instance = fields["daemon_instance"]
                if fields.get("scope"):
                    self.scope = fields["scope"]
                if fields.get("exec"):
                    self.exec_ids.append(fields["exec"])
                if row.get("event") in {"failed", "startup_not_ready", "startup_failure", "handshake_failure", "owner_panicked"}:
                    raise OriginalFailure("original runtime failure: " + row.get("value", ""))
                if row.get("event") == expected:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("original buffered event exceeded wall stop: " + expected)
                    if expected in {"command", "verify"} and fields.get("exit_code") != "0":
                        raise OriginalFailure("original command exit is not known zero: " + fields.get("exit_code", "UNAVAILABLE"))
                    return row
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError("original event wall stop: " + expected)
            ready = self.selector.select(left)
            if not ready:
                raise TimeoutError("original event wall stop: " + expected)
            data = os.read(self.process.stdout.fileno(), 65536)
            if not data:
                raise OriginalFailure("runtime event EOF before " + expected)
            self.buffer += data
            try:
                if self.raw.write(data) != len(data):
                    raise OriginalFailure("short original runtime evidence write; no resend")
            except BaseException as original:
                if not hasattr(original, "original_phase"):
                    original.original_phase = "event_process_raw_write"
                raise
            if len(self.buffer) > 1024 * 1024 and b"\n" not in self.buffer:
                raise OriginalFailure("runtime event observation exceeds declared 1 MiB harness frame; raw bytes retained")

    def send(self, operation, key="", path=None, *, deadline):
        if time.monotonic() >= deadline:
            raise TimeoutError("original operation not sent after expired wall stop: " + operation)
        body_sha = sha(path) if operation in {"command", "verify"} else None
        fields = [operation]
        if key or path is not None:
            fields.append(key)
        if path is not None:
            fields.append(str(path))
        if time.monotonic() >= deadline:
            raise TimeoutError("original operation preparation exceeded wall stop: " + operation)
        raw = ("\t".join(fields) + "\n").encode()
        try:
            if self.process.stdin.write(raw) != len(raw):
                raise OriginalFailure("short original control write; outcome uncertain; no resend")
        except BaseException as original:
            if not hasattr(original, "original_phase"):
                original.original_phase = "event_process_original_control_write"
            raise
        event = self.event("external_snapshot" if operation in {"observe", "snapshot"} else
                           "container_stopped" if operation == "stop" else operation, deadline)
        if body_sha is not None and event.get("fields", {}).get("command_sha256") != body_sha:
            raise OriginalFailure("original executed " + operation + " body differs from supplied script SHA: expected " + body_sha + "; actual " + str(event.get("fields", {}).get("command_sha256")))
        return event

    def retain(self, cause):
        # A constructor may already have fenced this same owner. Calling its
        # carrier later performs neither another signal nor a failed-operation
        # replay. Keep the first original cause and exact partial owner.
        cause.event_process = self
        if getattr(self, "_host_fence_attempted", False):
            cause.event_process_original_cause = getattr(self, "_retained_cause", cause)
            cause.event_process_retention = self._host_fence_result
            return self._host_fence_result
        self._host_fence_attempted = True
        self._retained_cause = cause
        cause.event_process_original_cause = cause
        process = getattr(self, "process", None)
        result = {"cause_type": type(cause).__name__, "cause": None,
                  "original_phase": getattr(cause, "original_phase", None),
                  "container": getattr(self, "container", None), "exec_ids": getattr(self, "exec_ids", []),
                  "host_pid": getattr(process, "pid", None),
                  "child_event_receipts": getattr(self, "child_event_receipts", []),
                  "container_disposition": "NO_ACTION_OR_INFERENCE; original child evidence retained",
                  "filesystem_drain": "NOT_ESTABLISHED_BY_HOST_FENCE",
                  "original_operations_replayed": False, "host_wait_stop_seconds": 1,
                  "host_signal": "NOT_ATTEMPTED", "independent_failures": []}
        self._host_fence_result = result
        cause.event_process_retention = result
        def note(error, phase):
            previous = getattr(cause, "independent_custody_failures", [])
            cause.independent_custody_failures = [*previous, {"phase": phase, "error": error}]
            try:
                description = str(error)
            except BaseException:
                description = "failure description unavailable"
            result["independent_failures"].append({"phase": phase, "type": type(error).__name__, "cause": description})
        try:
            try:
                result["cause"] = str(cause)
            except BaseException as failure:
                note(failure, "original_cause_format")
            write_new(self.folder / ("retained-" + self.label + "-custody.json"), result)
        except BaseException as failure:
            note(failure, "retained_custody_output")
        finally:
            # One host-session-group signal and one bounded wait. This is not
            # Docker cancellation, Stop, unmount or filesystem drain evidence.
            if process is None:
                result["host_owner"] = "NO_ACKNOWLEDGED_CHILD"
            else:
                try:
                    live = process.poll() is None
                except BaseException as failure:
                    note(failure, "host_poll")
                    live = True  # The acknowledged host group remains owned.
                if live:
                    result["host_signal"] = "ORIGINAL_SIGKILL_ATTEMPT"
                    try:
                        os.killpg(process.pid, 9)
                        result["host_signal"] = "ORIGINAL_SIGKILL_SENT"
                    except ProcessLookupError:
                        result["host_signal"] = "ORIGINAL_GROUP_ABSENT"
                    except BaseException as failure:
                        note(failure, "host_group_fence")
                try:
                    result["observed_host_exit"] = process.wait(timeout=1)
                except subprocess.TimeoutExpired as failure:
                    note(failure, "bounded_host_wait")
                    result["observed_host_exit"] = "UNAVAILABLE after bounded original wait"
                except BaseException as failure:
                    note(failure, "host_wait")
        try:
            write_new(self.folder / ("retained-" + self.label + "-host-fence.json"), result)
        except BaseException as failure:
            note(failure, "host_fence_output")
        return result


def bind_execution_assets(config, selection, staged=None):
    if config.get("cache_observer_inside") != "/code/residency.py":
        raise ValueError("selected cache observer must be the canonical /code/residency.py")
    root = Path(__file__).resolve().parents[2]
    wanted = {name: sha(root / "r7-cache" / name) for name in
              ("residency.py", "stream_manifest.py", "generate_manifest.py")}
    if selection["arm"] == "P":
        inside = config["passthrough_inside"]
        if not inside.startswith("/code/") or ".." in Path(inside).parts or str(Path(inside)) != inside:
            raise ValueError("selected passthrough executable must be a canonical /code file")
        relative = inside[len("/code/"):]
        if relative in wanted:
            raise ValueError("passthrough executable collides with a canonical cache helper")
        wanted[relative] = config["passthrough_sha256"]
    plan = deployment.load_plan(config, selection)
    code = next(item for item in plan["assets"] if item["target"] == "/code")
    actual = verification_module.tree_inventory(code["inventory"], code["inventory_sha256"],
        code["content_metadata_set_sha256"], wanted)
    if actual != wanted:
        raise ValueError("sealed cache/passthrough execution bytes differ from verified source/binary")
    with verification_module.oracle.FileOwner(code["inventory"], "r", "execution_asset_inventory_read") as stream:
        for line in stream:
            row = json.loads(line)
            path = Path(row.get("path", "."))
            if "__pycache__" in path.parts or path.suffix in {".pyc", ".pyo"}:
                raise ValueError("sealed executable assets must not include cached Python bytecode")
    if staged is not None:
        if staged.get("schema") != "r7-container-staging-v1" or staged.get("status") != "PASS" or any(
                staged.get("helper_sha256", {}).get(name) != digest for name, digest in wanted.items()):
            raise ValueError("actual staged cache/passthrough bytes differ from verified source/binary")
    return wanted


def preflight(config, selection):
    if config.get("oracle_variant"):
        if config["oracle_variant"] != registry_variant.NAME or selection["case_id"] not in registry_variant.GIT_TREE_CASES:
            raise ValueError("prospective Git index oracle variant/case mismatch")
        contract = config.get("git_index_oracle",{})
        if not all(contract.get(key) for key in ("helper_sha256","object_format","index_version","effective_config_sha256","git_binary_sha256","git_version")):
            raise ValueError("selected Git index oracle helper/format/version/effective config/binary seals missing")
    required = ["runtime_binary", "runtime_sha256", "daemon_binary", "daemon_sha256", "socket", "volume",
                "manifest", "prepared_store_file", "identities", "build_profile", "uid", "gid", "setup_method", "declared_interference",
                "fixture_kind", "cache_observer_inside", "native_root", "container_setup", "source_inventory", "source_clean", "source_dirty"]
    missing = [key for key in required if key not in config]
    if missing:
        raise ValueError("missing sealed preparation: " + ", ".join(missing))
    if config["build_profile"] != "release" or config["source_clean"] is not True or config["source_dirty"] is not False:
        raise ValueError("clean sealed release preparation required")
    if config["uid"] == 0 or config["identities"]["image_id"] != registry.IMAGE:
        raise ValueError("pinned image and ordinary nonroot identity required")
    if not all(isinstance(config["identities"].get(key), str) and config["identities"][key] for key in receipts.IDENTITIES):
        raise ValueError("complete source/product/build/fixture/oracle identity inventory required")
    if selection["arm"] == "L":
        root = Path(__file__).resolve().parents[4]
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=root, text=True).strip()
        if commit != config["identities"]["source_commit"] or tree != config["identities"]["source_tree"]:
            raise ValueError("sealed L source commit/tree differs from current main")
        dirty = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "core/crates", "core/benchmark/r7-runtime",
                                "core/benchmark/fs-bench-pro/r7", ".cargo", "core/Cargo.toml", "core/Cargo.lock", "patches/fuser-0.18.0"], cwd=root)
        if dirty.returncode:
            raise ValueError("relevant product/compilation/harness source is dirty or source check failed")
    if not config.get("manifest_sha256") or sha(config["manifest"]) != config["manifest_sha256"]:
        raise ValueError("sealed installed manifest identity missing or changed")
    if config["setup_method"] == "clone" and not config.get("volume_clone_identity"):
        raise ValueError("independent writable volume clone identity missing")
    for key in ("runtime", "daemon"):
        if sha(config[key + "_binary"]) != config[key + "_sha256"]:
            raise ValueError("sealed " + key + " binary changed")
    deployment.owned_input(config["prepared_store_file"])
    if selection["arm"] == "P" and not all(key in config for key in ("passthrough_inside", "passthrough_mount", "matched_profile_identity")):
        raise ValueError("sealed matched passthrough binary/profile preparation missing")
    if selection["arm"] == "P":
        if sha(config["passthrough_binary"]) != config.get("passthrough_sha256"):
            raise ValueError("actual passthrough binary seal differs")
        profile_sha = hashlib.sha256(json.dumps(P_PROFILE, sort_keys=True).encode()).hexdigest()
        if config["matched_profile_identity"] != profile_sha:
            raise ValueError("matched profile label differs from the enforced profile")
    control_peers(config, selection)
    deployment.load_plan(config,selection)
    bind_execution_assets(config, selection)
    if config["setup_method"] == "qualified-shared-read-only" and not config.get("shared_binding_identity_proof"):
        raise ValueError("shared binding before/after identity and zero-write proof missing")
    if config["setup_method"] == "qualified-shared-read-only" and selection["case_id"] not in {"E01", "E02", "E03", "E05", "E06", "E07"}:
        raise ValueError("potentially mutating command cannot use prepared shared master; independent clone required")
    if config["setup_method"] not in {"clone", "qualified-shared-read-only", "empty"}:
        raise ValueError("independent setup clone or explicitly qualified shared binding required")
    case = next(case for case in registry.cases() if case["case_id"] == selection["case_id"])
    if case["fixture"] != config["fixture_kind"] and not config.get("reduced_fixture_cut_identity"):
        raise ValueError("registered fixture kind differs; reduced cut must be explicitly sealed and labelled")
    if case["case_id"] in {"C09", "C10", "C11"} and not config.get("preconditioned_empty_root_receipt"):
        raise ValueError("64 MiB setup input survival/cache proof from empty root missing")
    if case["case_id"] == "E19" and not config.get("related_root_preparation_receipt"):
        raise ValueError("exact E19 related-root preparation/oracle receipt missing")
    if selection["case_id"] not in {"E01", "E03"} and not config.get("oracle_recipe"):
        raise ValueError("closed comparison recipe required; an arbitrary zero-exit script is not verification")
    verification_module.load(config, selection)
    return source_inventory.verify(config,selection["arm"])


def require_changed_commit(event):
    if event.get("event") != "commit" or event.get("fields", {}).get("commit_kind") != "Committed":
        raise OriginalFailure("registered nonempty K frontier requires original typed Committed; received " + str(event.get("fields", {}).get("commit_kind")))


def phase_observation(event, provenance):
    span = event.get("span")
    if not span:
        return receipts.unavailable("original timed span unavailable")
    return {"status": "AVAILABLE", "value": {**span, "start_event": event["event"] + ":invocation",
            "end_event": event["event"] + ":original acknowledgment",
            "clock_domain": "host-monotonic" if event["event"].startswith("passthrough") or event["event"] in {"cleanup_to_Gone","container_setup","independent_verifier"} else "sdk-runtime-monotonic-relative-origin"}, "provenance": provenance}


def script(folder, name, body):
    path = Path(folder) / name
    with path.open("x") as stream:
        stream.write(body)
    path.chmod(0o444)
    return path


def external_inputs(case_id):
    return {"replay_roots":["/replay"] if case_id in {"E12","E13","K02","K03"} else [],
            "semantic_files":["/code/node-roots.json"] if case_id == "E14" else ["/code/largest-path"] if case_id == "E15" else []}


def telemetry(path):
    """Validate original bounded numeric framing without parsing Debug text."""
    groups = {}
    with Path(path).open() as stream:
        for line in stream:
            if not line.lstrip().startswith('{"layerfs_observation":'):
                continue
            if len(line.encode()) > 4096:
                raise ValueError("numeric observation exceeds source record bound")
            row = json.loads(line)
            if row.get("layerfs_observation") != 1 or type(row.get("available")) is not bool:
                raise ValueError("original numeric observation schema")
            if not isinstance(row.get("values"), list) or not all(type(value) is int and 0 <= value < 2**64 for value in row["values"]):
                raise ValueError("numeric observation value type")
            if type(row.get("call")) is not int or not 0 < row["call"] < 2**64 or type(row.get("section")) is not int or not 0 <= row["section"] < 2**16 or type(row.get("index")) is not int or not -(2**63) <= row["index"] < 2**63:
                raise ValueError("original numeric call/section/index type and source bounds")
            if not row["available"] and row["values"]:
                raise ValueError("unavailable observation contains fabricated values")
            for name in ("scope", "daemon"):
                if not isinstance(row.get(name), list) or len(row[name]) != 4 or not all(type(value) is int and 0 <= value < 2**64 for value in row[name]):
                    raise ValueError("numeric daemon/scope identity incomplete")
            if row["scope"] == [0,0,0,0] or type(row.get("slot")) is not int or row["slot"] < 0:
                raise ValueError("numeric nonzero scope/admitted slot identity required")
            identity = json.dumps([row["daemon"], row["scope"], row["call"]], separators=(",", ":"))
            groups.setdefault(identity, []).append(row)
    for call, rows in groups.items():
        if rows[0]["section"] != 0 or rows[-1]["section"] != 99 or len(rows[0]["values"]) != 3 or not rows[0]["available"] or not rows[-1]["available"]:
            raise ValueError("incomplete original numeric call " + str(call))
        keys = [(row["section"], row["index"]) for row in rows]
        if len(keys) != len(set(keys)):
            raise ValueError("duplicate numeric section/index")
        body = rows[:-1]
        footer = rows[-1]["values"]
        if footer != [len(body), sum(len(row["values"]) for row in body), sum(not row["available"] for row in body)]:
            raise ValueError("original numeric footer differs from retained rows")
        if len(rows) > 64 + 2 * rows[0]["values"][1]:
            raise ValueError("configured numeric call row bound")
    return groups


def ingest_observations(groups, observations, cache):
    def available(values, provenance):
        return {"status": "AVAILABLE", "value": values, "provenance": provenance}
    views = [counts.decode(group) for group in groups.values()]
    for field, observation_name in (("stored_counts", "stored_counts"), ("pages", "cleanup_debt"),
                                    ("reader_storage", "reader_storage_diagnostics"), ("observer", "observer_work")):
        selected = [{"identity": view["identity"], "operation_tag": view["operation_tag"], "value": view[field]}
                    for view in views if field in view and view[field]]
        if selected:
            observations[observation_name] = available({"schema_source": counts.SCHEMA_SOURCE,
                         "scope": "original absolute/per-call values; no gauge/peak/timer subtraction", "snapshots": selected},
                         "original numeric schema sections 28/29/30; source field order decoded")
    names = {1: "owner_work", 8: "store_work", 9: "reader_work", 10: "cache_work", 16: "storage_diagnostics",
             11: "dispatch_work", 12: "native_work", 13: "global_sql_work", 15: "construction_work", 26: "opcode_work",
             25: "mount_work", 28: "resource_diagnostics", 29: "reader_storage_raw"}
    for section, name in names.items():
        rows = [{"call": call, "operation": group[0]["values"][0], "index": row["index"], "values": row["values"]}
                for call, group in groups.items() for row in group if row["section"] == section and row["available"]]
        if rows:
            observations[name] = available({"scope": "original cumulative/per-call arrays; schema pinned to product source; not fabricated phase deltas", "snapshots": rows}, "daemon.logs.stdout/stderr: numeric schema v1 section " + str(section))
    content = [{"identity":counts.identity(group),"section":row["section"],"index":row["index"],
                "available":row["available"],"values":row["values"]} for group in groups.values() for row in group if 17 <= row["section"] <= 23]
    if content:
        observations["content_diagnostics"] = available(content,"original source-pinned Content diagnostics sections17-23; unavailable arrays remain explicit")
    if observations.get("storage_diagnostics",{}).get("status") != "AVAILABLE" and observations.get("reader_storage_diagnostics",{}).get("status") == "AVAILABLE":
        observations["storage_diagnostics"] = observations["reader_storage_diagnostics"]
    sql = [{"call": call, "section": row["section"], "index": row["index"], "values": row["values"]}
           for call, group in groups.items() for row in group if row["section"] in {2, 3} and row["available"]]
    if sql:
        observations["statement_work"] = available(sql, "original overlay engine numeric statement families, foreground/maintenance distinct")
    global_sql = [{"identity":counts.identity(group),"section":row["section"],"index":row["index"],
                   "available":row["available"],"values":row["values"]} for group in groups.values() for row in group if row["section"] in {13,14}]
    if any(item["available"] for item in global_sql):
        observations["global_sql_work"] = available(global_sql,"original global writer and per-reader SQL snapshots; unavailable reader rows retained, never filled")
    owners = [row for group in groups.values() for row in group if row["section"] == 1 and row["available"]]
    if owners:
        last = owners[-1]["values"]
        for name, index in (("scheduler_bytes", 4), ("peak_credited_bytes", 2)):
            if len(last) > index:
                observations[name] = available(last[index], "original owner scalar order; cumulative observation, not phase peak")
    if groups:
        allowances = {rows[0]["values"][2] for rows in groups.values()}
        if len(allowances) == 1:
            observations["immutable_cache_allowance_bytes"] = available(next(iter(allowances)), "source-configured logical cache allowance; no resident-bound claim")
    if cache.get("class") == "B":
        terminal = [(call, group) for call, group in groups.items() if group[0]["values"][0] == 4]
        status = [(call, group) for call, group in groups.items() if group[0]["values"][0] == 3]
        if terminal and status:
            warm = terminal[0][1]
            measured = status[0][1]
            before = next((row for row in warm if row["section"] == 8 and row["available"]), None)
            after = next((row for row in measured if row["section"] == 8 and row["available"]), None)
            if before and after and after["values"][0] >= before["values"][0]:
                cache["object_demands"] = after["values"][0] - before["values"][0]


def snapshot_observations(runtime, observations):
    found = [row for row in runtime.rows if row.get("event") == "external_snapshot"]
    if not found:
        return
    path = found[-1].get("fields", {}).get("stdout")
    if not path:
        return
    values = {}
    backing = {}
    with Path(path).open() as stream:
        for line in stream:
            if line.startswith("VmHWM:"):
                values["daemon_vmhwm_bytes"] = int(line.split()[1]) * 1024
            parts = line.split()
            if len(parts) == 4 and parts[0].startswith(("/layerfs-store/global/store.sqlite", "/layerfs-local/overlay/overlay.sqlite")):
                backing[parts[0]] = (int(parts[1]), int(parts[2]) * int(parts[3]))
            elif len(parts) == 2 and parts[1] == "ABSENT":
                backing[parts[0]] = (0, 0)
    for prefix, base in (("store", "/layerfs-store/global/store.sqlite"), ("overlay", "/layerfs-local/overlay/overlay.sqlite")):
        paths = [base + suffix for suffix in ("", "-wal", "-shm", "-journal")]
        if all(path in backing for path in paths):
            values[prefix + "_logical_bytes"] = sum(backing[path][0] for path in paths)
            values[prefix + "_allocated_bytes"] = sum(backing[path][1] for path in paths)
    for name, value in values.items():
        observations[name] = {"status": "AVAILABLE", "value": value, "provenance": str(found[-1]["fields"]["stdout"]) + "; original external observation; VmHWM lifetime only; Store/overlay include explicitly observed sidecars"}


def diagnostic_completion(row):
    """Availability, never a numerical or resource-admission verdict."""
    if row["row_status"] in {"FAIL","INELIGIBLE","NOT_RUN"}:
        return
    cache = row["cache"]
    missing = []
    try:
        receipts.validate_cache(cache,row["source_arm"],row["attempted_operation_count"])
    except ValueError as error:
        missing.append("cache qualification: " + str(error))
    if cache["class"] == "A" and row["source_arm"] == "L":
        missing.append("post-Mount command cache state: immutable metadata and pooled reader metadata may be warm; SQLite pager/prepared state and kernel metadata residency are unavailable")
    if cache["class"] == "B":
        demands = cache.get("object_demands")
        if type(demands) is not int or demands < 0:
            missing.append("measured class B object demands")
        elif demands:
            row.update(row_status="INELIGIBLE",reason="measured class B object demands are nonzero")
            return
    if cache["class"] == "C":
        gap = cache.get("warmup_gap_ns")
        if type(gap) is not int or gap < 0:
            missing.append("same-clock class C warmup gap")
        elif gap >= 60_000_000_000:
            row.update(row_status="INELIGIBLE",reason="class C warmup lifetime exceeded")
            return
    if row.get("verification_status") != "PASS":
        missing.append("selected functional/scoped verifier")
    if row.get("completed_operation_count") != 1 or row.get("custody_status") != "KNOWN_STOP":
        missing.append("original complete operation/known stop")
    required = registry.COUNTERS if row["source_arm"] == "L" else []
    missing.extend(name for name in required if row["observations"][name]["status"] == "UNAVAILABLE")
    if row["source_arm"] == "L" and row.get("cleanup_status") != "Gone":
        missing.append("original physical cleanup Gone")
    if row["source_arm"] == "L" and row.get("numeric_correlation_status") != "PASS":
        missing.append("original numeric framing/control-call correlation")
    if row["source_arm"] == "P":
        missing.append("exact kernel batch/default opcode counts unavailable")
    if row["source_arm"] in {"N","P"}:
        missing.append("native command/process and backing resource scope not instrumented; idle LayerFS daemon gauges are separate")
    if any(item.get("status") == "UNAVAILABLE" for item in row.get("phase_counts",[])):
        missing.append("declared phase count interval unavailable")
    row["completion_gaps"] = missing
    row.update(row_status="INCOMPLETE" if missing else "DIAGNOSTIC",
               reason="; ".join(missing) if missing else "complete selected exploratory observations; no admission/performance/resource PASS",
               resource_status="OBSERVED_CURRENT_AND_LIFETIME" if all(row["observations"][name]["status"] == "AVAILABLE" for name in
                   ("daemon_vmhwm_bytes","store_logical_bytes","store_allocated_bytes","overlay_logical_bytes","overlay_allocated_bytes")) else "INCOMPLETE")


def ownership_proof(row, runtime, groups, correlation):
    if row.get("scenario_id") != "E01" or row.get("completed_operation_count") != 1 or row.get("custody_status") != "KNOWN_STOP":
        return
    if any(event.get("fields",{}).get("exit_code") != "0" or event.get("fields",{}).get("registered_execs") != "0" for event in row.get("command_original_events",[])):
        return
    if row["source_arm"] == "N":
        row.update(verification_status="PASS",ownership_proof={"status":"PASS","scope":"native ordinary command, known exit/stream completion and explicit owned controller stop; filesystem mount/unmount not applicable"})
        return
    if row["source_arm"] != "L" or correlation is None:
        return
    decoded = {counts.identity(group):counts.decode(group) for group in groups.values()}
    by_sequence = {item["event_sequence"]:item for item in correlation}
    terminal = row.get("terminal_unmount_events",[])
    if not terminal:
        return
    proofs = []
    for event in terminal:
        actual = decoded[tuple(by_sequence[event["sequence"]]["identities"][-1])]
        native, loops, work = actual.get("native",{}),actual.get("loops",{}),actual.get("mount",{})
        if native.get("detached") != 1 or [loops.get(key) for key in ("configured","created","entered","exited","joined")] != [2,2,2,2,2] or any(work.get(key) != 0 for key in ("received","admitted","queued","running","parked","retained","owned_input_bytes","future_bytes")):
            return
        key = event["fields"]["key"]
        if not any(item.get("fields",{}).get("key") == key and item["fields"].get("state") == "Gone" for item in row.get("cleanup_observations",[])):
            return
        proofs.append({"key":key,"identity":actual["identity"],"detached":native["detached"],"loops":loops,"terminal_work":work,"cleanup":"Gone"})
    row.update(verification_status="PASS",ownership_proof={"status":"PASS","scope":"original typed Ready operation, complete ordinary command, actual detached/joined/drained native session and original-token Gone; no tree oracle selected","terminal":proofs})


def one(config, selection, output, claims):
    config = dict(config)
    output = Path(output).absolute()
    output.mkdir(parents=True, exist_ok=False)
    row = {"schema": receipts.SCHEMA, "family_id": "r7-optimization", "mode": "exploratory",
           "admission_eligible": False, "registry_identity": registry.registry_identity(),
           "selection_id": selection["selection_id"], "row_status": "NOT_RUN", "reason": selection["reason"],
           "attempted_operation_count": 0, "completed_operation_count": 0, "sample_count": 0}
    if config.get("oracle_variant") == registry_variant.NAME and selection["case_id"] in registry_variant.GIT_TREE_CASES:
        row.update(oracle_variant=registry_variant.NAME,oracle_contract=registry_variant.CONTRACT,registry_identity=registry_variant.identity())
        if selection["case_id"] == "E18" and selection["cache_class"] == "C":
            selection = dict(selection, reason=registry_variant.E18_C_REASON)
            row["reason"] = selection["reason"]
    if selection["reason"] != "prospectively registered; not attempted":
        write_new(output / "receipt.json", receipts.validate_receipt(row))
        return row
    try:
        source_verification = preflight(config, selection)
    except (ValueError, KeyError, OSError) as error:
        row["reason"] = str(error)
        write_new(output / "receipt.json", receipts.validate_receipt(row))
        return row
    claims = Path(claims)
    claims.mkdir(parents=True, exist_ok=True)
    try:
        effective = effective_treatment(config, selection, source_verification)
    except (ValueError, KeyError, OSError) as error:
        row["reason"] = "verified execution claim unavailable: " + str(error)
        write_new(output / "receipt.json", receipts.validate_receipt(row))
        return row
    key = claim_key(selection, config["identities"], effective=effective)
    row["effective_treatment"] = effective
    claim = claims / key
    try:
        with claim.open("x") as stream:
            stream.write(str(output.resolve()) + "\n")
    except FileExistsError:
        row["reason"] = "unchanged treatment already claimed; no replay/resample"
        write_new(output / "receipt.json", receipts.validate_receipt(row))
        return row
    case = next(case for case in registry.cases() if case["case_id"] == selection["case_id"])
    row["source_inventory_verification"] = source_verification
    oracle_plan = verification_module.load(config, selection)
    budget = case["complete_command_wall_stop_ns"] / 1e9
    environment = output / "environment.txt"
    environment.write_text("".join(name + "=" + value + "\n" for name, value in workloads.ENV.items()))
    argv = [config["runtime_binary"], "serve", "--socket", config["socket"], "--volume", config["volume"],
            "--daemon", config["daemon_binary"], "--manifest", config["manifest"], "--receipt", str(output / "runtime.events.jsonl"),
            "--uid", str(config["uid"]), "--gid", str(config["gid"]), "--environment-file", str(environment),
            "--observation-scope", secrets.token_hex(32)]
    write_new(output / "selection.json", {"selection": selection, "case": case, "configuration": config, "argv": argv})
    runtime = None
    mounts = {}
    phases = {name: receipts.unavailable("phase not reached or not selected") for name in registry.PHASES}
    measured_events = []
    cache = {"class": selection["cache_class"], "scopes": dict.fromkeys(registry.PHASES,
             "original phase cache declarations retained in selection config; unavailable exact metadata residency remains explicit")}
    if selection["cache_class"] is None:
        cache["reason"] = config.get("commit_cache_declaration", "own-write and source/store residency not numerically qualified; captured frontier and original phase observations retained")
    status = "INCOMPLETE"
    original_error = None
    start = None
    performance = PerformanceClock()
    passthroughs = []
    peers = control_peers(config, selection)
    closed_keys = []
    endpoint_bindings = {}
    def bind_endpoint(label,event,position="last"):
        endpoint_bindings[label] = {"event_sequence":event["sequence"],"event":event["event"],"position":position}
    def verify_selected(directory, selected, original_command=None):
        verifier_start = time.monotonic_ns()
        performance.begin_verifier(verifier_start)
        proof_deadline = verifier_start / 1e9 + case["verifier_wall_stop_ns"] / 1e9
        attempt = {"start_ns": verifier_start, "wall_stop_ns": case["verifier_wall_stop_ns"],
                   "directory": directory, "expected_label": selected["label"] if selected else None,
                   "scope": "complete host comparison and selected original SDK verifier/count fence", "status": "ORIGINAL_PENDING"}
        row.setdefault("verification_attempts", []).append(attempt)
        try:
            proof = {}
            if selected is not None:
                host_script = verification_module.member(oracle_plan["root"], Path(selected["verifier"]["path"]).name)
                verified = runtime.send("verify", directory, host_script, deadline=proof_deadline)
                phases["verifier"] = phase_observation(verified, "original separate comparison interval")
                proof["tree"] = verification_module.compare_event(oracle_plan, selected, verified)
            if original_command is not None:
                ordinal = len(row.get("verification_events", []))
                proof["stdout"] = verification_module.compare_stdout(oracle_plan, original_command,
                    output / ("host-stdout-verifier-" + str(ordinal)), deadline=proof_deadline)
            if case["case_id"].startswith("K") and selected is not None:
                runtime.send("snapshot", directory, deadline=proof_deadline)
                boundary = next(row for row in reversed(runtime.rows) if row.get("event") == "workspace_status")
                boundary_label = "after_verifier:" + selected["label"]
                if boundary_label in endpoint_bindings:
                    boundary_label = "fresh_" + boundary_label
                bind_endpoint(boundary_label, boundary)
                proof["count_boundary"] = boundary
            verifier_end = time.monotonic_ns()
            if verifier_end - verifier_start > case["verifier_wall_stop_ns"]:
                raise TimeoutError("complete independent verifier wall stop exceeded")
            attempt["status"] = "PASS"
            proof["complete_controller_interval"] = {"start_ns": verifier_start, "end_ns": verifier_end,
                "duration_ns": verifier_end - verifier_start, "clock_domain": "host-monotonic"}
            row.setdefault("verification_events", []).append(proof)
            return proof
        except Exception as error:
            attempt.update(status="FAILED", original_error=str(error), original_error_type=type(error).__name__,
                           original_phase=getattr(error, "original_phase", None),
                           independent_close_failures=getattr(error, "independent_close_failures", []))
            raise
        finally:
            ended = time.monotonic_ns()
            attempt.update(end_ns=ended, duration_ns=ended-verifier_start)
            performance.end_verifier(ended)
            phases["verifier"] = phase_observation({"event": "independent_verifier",
                "span": {"clock_id": "monotonic", "start_ns": verifier_start, "end_ns": ended,
                         "duration_ns": ended-verifier_start}}, "verification_attempts; complete controller interval, including original comparison and count fence")
            if attempt["status"] == "PASS":
                proof["complete_controller_interval"].update(end_ns=ended, duration_ns=ended-verifier_start)
                if ended-verifier_start > case["verifier_wall_stop_ns"]:
                    attempt.update(status="FAILED", original_error="complete independent verifier wall stop exceeded",
                                   original_error_type="TimeoutError")
                    raise TimeoutError(attempt["original_error"])
    try:
        runtime = EventProcess(argv, output, "runtime")
        runtime.event("protocol_ready", time.monotonic() + config.get("startup_wall_seconds", 15))
        if not runtime.container:
            raise OriginalFailure("original acknowledged container identity missing")
        row["setup_attempted_operation_count"] = 1
        row["container_setup_receipt"] = "setup/receipt.json"
        staged = deployment.stage(config,selection,runtime.container,output/"setup")
        row["execution_asset_binding"] = bind_execution_assets(config, selection, staged)
        if oracle_plan:
            row["oracle_deployment_binding"] = verification_module.bind_deployment(oracle_plan, config, selection, staged)
        if config.get("oracle_variant") and staged["helper_sha256"].get("git_index_oracle.py") != config["git_index_oracle"]["helper_sha256"]:
            raise OriginalFailure("actual staged Git index oracle helper differs from prospective seal")
        phases["setup"] = phase_observation({"event":"container_setup","span":{"clock_id":"monotonic",
               "start_ns":staged["setup_start_ns"],"end_ns":staged["setup_end_ns"],"duration_ns":staged["setup_ns"]}},
               "setup/receipt.json; untimed actual asset/copy/metadata/full-byte validation")
        if selection["arm"] != "L":
            actual_manifest = staged["native_manifests"][0]
            config.update(native_file_manifest_inside=actual_manifest["manifest"],native_file_manifest_sha256=actual_manifest["sha256"],
                          declared_native_aliases=actual_manifest.get("declared_aliases",0) > 0)
        write_new(output/"effective-staged-configuration.json",config)
        declared_external = external_inputs(case["case_id"])
        cache["declared_external_inputs"] = declared_external
        cache["helper_code_residency"] = "common sealed interpreter/helper executables setup-warm in every arm; separate from fixture/semantic input data"
        cache["external_input_warmth"] = "cold components independently hinted/measured before A" if selection["cache_class"] == "A" else "setup/full-byte-verification warmth and identical-call warmth explicitly retained; no cold source claim"
        command = case["workload"].get("argv")
        if case["case_id"].startswith(("K", "W")):
            command = None
        body = command[-1] if command else ""
        warm_end = None
        def mount(deadline=None):
            mount_key = secrets.token_hex(32)
            event = runtime.send("mount", mount_key, deadline=deadline or time.monotonic() + budget)
            mounts[mount_key] = event["fields"]["directory"]
            return mount_key, event
        if selection["arm"] == "L" and selection["cache_class"] == "B":
            warm_key, _ = mount()
            runtime.send("command", warm_key, script(output, "warmup.sh", body), deadline=time.monotonic() + budget)
            warm_unmount = runtime.send("unmount", warm_key, deadline=time.monotonic() + budget)
            bind_endpoint("warmup_unmount",warm_unmount)
            mounts.pop(warm_key)
            cache.update(warmup_terminal_unmount=True, history_read_residency="per-file observer receipt before measured mount")
        if peers:
            lines = ["set -e"]
            for index, (root, _) in enumerate(peers):
                lines.extend(["test -d " + shlex.quote(root), "test -w " + shlex.quote(root),
                              "printf 'R7_PEER " + str(index) + " '; realpath -- " + shlex.quote(root),
                              "stat -Lc 'R7_PEER_ID %d:%i' -- " + shlex.quote(root)])
            prepared = runtime.send("command", "native:/", script(output, "peer-preconditions.sh", "\n".join(lines)),
                                    deadline=time.monotonic() + budget)
            with Path(prepared["fields"]["stdout"]).open() as stream:
                observed = list(stream)
            resolved = [line.split(" ",2)[2].strip() for line in observed if line.startswith("R7_PEER ")]
            inodes = [line.split()[1] for line in observed if line.startswith("R7_PEER_ID ")]
            if len(resolved) != len(peers) or len(inodes) != len(peers) or len(set(inodes)) != len(peers) or any(left == right or Path(left) in Path(right).parents for index,left in enumerate(resolved) for other,right in enumerate(resolved) if index != other):
                raise OriginalFailure("prepared peer directories alias or overlap in actual native container")
            row["peer_preconditions"] = {"preparation_identity": config["peer_preparation_identity"],
                                         "resolved_roots": resolved, "directory_identities": inodes,
                                         "scope": "untimed metadata/permission validation; independent writable byte-copy preparation supplied by lead"}
        def start_passthrough(root, target, index, until):
            pbody = "exec " + shlex.quote(config["passthrough_inside"]) + " " + shlex.quote(root) + " " + shlex.quote(target)
            pargv = ["docker", "--host", "unix://" + config["socket"], "exec", "--user", "0:0", runtime.container,
                     "/bin/bash", "-o", "pipefail", "-c", pbody]
            process = EventProcess(pargv, output, "passthrough-" + str(index))
            process.container = runtime.container
            passthroughs.append((process, target))
            ready = process.event("ready", min(until, time.monotonic() + 5))
            profile = passthrough_ready(process, ready)
            row.setdefault("passthrough_sessions", []).append({"root": root, "mountpoint": target, "ready": ready, "profile": profile})
            return process
        if selection["arm"] == "P" and selection["cache_class"] == "C":
            for index, (root,target) in enumerate(peers or [(config["native_root"],config["passthrough_mount"])]):
                start_passthrough(root, target, index, time.monotonic() + budget)
        active_key = None
        if selection["arm"] == "L" and selection["cache_class"] == "C":
            active_key, retained_mount = mount()
            bind_endpoint("retained_mount_attach_before_warmup",retained_mount)
        directory = "native:" + (config["passthrough_mount"] if selection["arm"] == "P" else config["native_root"])
        if active_key:
            directory = active_key
        concurrent_peer = None
        if selection["cache_class"] == "C" and case["case_id"].startswith("W"):
            if selection["arm"] == "L":
                concurrent_peer = active_key
                if case["case_id"] == "W01":
                    concurrent_peer, _ = mount()
                peer_directories = [mounts[active_key], mounts[concurrent_peer]]
            else:
                peer_directories = [target for _,target in peers]
            children = case["workload"]["commands"]
            if case["case_id"] == "W02":
                runtime.send("command", directory, script(output, "warmup-sequential.sh", children[0]["argv"][-1]), deadline=time.monotonic() + budget)
                children = children[1:]
            warm_body = shell_parallel([(peer_directories[index] if case["case_id"] == "W01" else peer_directories[0], child["argv"][-1]) for index, child in enumerate(children)])
            warm = runtime.send("command", directory, script(output, "warmup.sh", warm_body), deadline=time.monotonic() + budget)
            row["warmup_child_results"] = child_results(warm)
            warm_end = warm["span"]["end_ns"]
            cache.update(same_mount=True, warmup_receipt="runtime.events.jsonl: identical concurrent schedule")
        if selection["cache_class"] == "C" and body:
            warm = runtime.send("command", directory, script(output, "warmup.sh", body), deadline=time.monotonic() + budget)
            warm_end = warm["span"]["end_ns"]
            cache.update(same_mount=True, warmup_receipt="runtime.events.jsonl")
        if selection["arm"] == "L" and selection["cache_class"] == "C":
            runtime.send("snapshot",active_key,deadline=time.monotonic()+budget)
            warm_status = next(event for event in reversed(runtime.rows) if event.get("event") == "workspace_status")
            bind_endpoint("warmup_end_status",warm_status)
            row["warmup_observer_scope"] = "declared Status after identical warmup, before C predicate/start; outside performance; actual State/Resources jobs retained; no A/B post-predicate baseline"
        cache_body = ["python3", config["cache_observer_inside"], "--class", selection["cache_class"] or "B"]
        if selection["arm"] == "L":
            cache_body += ["--store", "/layerfs-store/global/store.sqlite", "--overlay", "/layerfs-local/overlay/overlay.sqlite"]
        elif selection["cache_class"] == "A":
            inventory_inside = "/tmp/r7-residency-" + secrets.token_hex(16) + ".jsonl"
            cache_body += ["--file-manifest", config["native_file_manifest_inside"], "--file-manifest-sha256", config["native_file_manifest_sha256"],
                           "--root", config["native_root"], "--inventory-output", inventory_inside]
            if config.get("declared_native_aliases"):
                cache_body.append("--allow-declared-aliases")
        # Cache observer runs with root through the ordinary external executor;
        # SDK admission is not weakened. Its nonzero result means zero attempts.
        cache_argv = ["docker", "--host", "unix://" + config["socket"], "exec", "--user", "0:0", runtime.container, *cache_body]
        write_new(output / "cache.invocation.json", {"argv": cache_argv, "container": runtime.container,
                   "exec_identity": "UNAVAILABLE through Docker CLI; no guessed identity", "phase": "pre-attempt cache observer"})
        if selection["arm"] != "L" and selection["cache_class"] == "C":
            residency = {"cache_class": "C", "status": "PENDING_WARMUP_BOUNDARY", "attempts": 0,
                         "files": [], "reason": "native class C proven by identical call/same mounted view and exact runtime clock gap; no cold-data claim"}
            write_new(output / "cache.stdout", residency)
            result_code = 0
        else:
            with (output / "cache.stdout").open("xb") as out, (output / "cache.stderr").open("xb") as err:
                result = subprocess.run(cache_argv, stdout=out, stderr=err, timeout=config.get("cache_wall_seconds", 15))
            residency = json.loads((output / "cache.stdout").read_text())
            result_code = result.returncode
        if selection["arm"] != "L" and selection["cache_class"] == "A" and residency.get("schema") == "r7-residency-stream-v1" and residency.get("inventory", {}).get("created"):
            destination = output / "cache.files.jsonl"
            copied = subprocess.run(["docker", "--host", "unix://" + config["socket"], "cp", runtime.container + ":" + inventory_inside,
                                     str(destination)], timeout=config.get("cache_wall_seconds", 15))
            if copied.returncode:
                raise OriginalFailure("original streamed cache inventory transfer failed")
            cache["stream_inventory_artifact"] = "cache.files.jsonl"
            cache["stream_inventory_sha256"] = sha(destination)
        cache.update(residency=residency, required_residency_paths=[item["path"] for item in (residency.get("files") or [])],
                     store_path="/layerfs-store/global/store.sqlite", overlay_path="/layerfs-local/overlay/overlay.sqlite",
                     fresh_daemon_cache=selection["cache_class"] == "A", fresh_kernel_connection=selection["cache_class"] != "C")
        if selection["cache_class"] == "A" and selection["arm"] == "L":
            cache["initial_cache_scope"] = "CanonicalCache and StorageFetch start empty; startup SQL opens fresh connections but reads profile/schema/history metadata and prepares statements"
            cache["command_cache_scope"] = "Mount reads canonical root/inode/directory metadata and retains immutable and pooled reader metadata; no post-Mount internal eviction or complete residency observation"
            cache["cold_command_qualification"] = "INCOMPLETE; backing-file predicate before Mount is insufficient"
        if result_code or residency["status"] in {"INELIGIBLE", "UNAVAILABLE"}:
            raise OriginalFailure("pre-attempt cache predicate " + residency["status"] + ": " + residency.get("error", "original observation retained"))
        if selection["cache_class"] == "A":
            auxiliary = []
            cache["auxiliary_residencies"] = auxiliary
            def attest_auxiliary(label,arguments):
                argv = ["docker","--host","unix://"+config["socket"],"exec","--user","0:0",runtime.container,
                        "python3",config["cache_observer_inside"],"--class","A",*arguments]
                write_new(output/("cache-"+label+".invocation.json"),{"argv":argv,"container":runtime.container,"attempts":1,
                          "phase":"additional pre-attempt cold input predicate","engine_exec_identity":"UNAVAILABLE through Docker CLI"})
                with (output/("cache-"+label+".stdout")).open("xb") as out, (output/("cache-"+label+".stderr")).open("xb") as err:
                    result = subprocess.run(argv,stdout=out,stderr=err,timeout=config.get("cache_wall_seconds",15))
                return result.returncode,json.loads((output/("cache-"+label+".stdout")).read_text())
            for index,root in enumerate(declared_external["replay_roots"]):
                actual = next((item for item in staged["auxiliary_manifests"] if item["root"] == root),None)
                if actual is None:
                    raise OriginalFailure("declared external replay cold manifest unavailable; zero attempts")
                inventory = "/tmp/r7-aux-residency-"+secrets.token_hex(16)+".jsonl"
                arguments = ["--file-manifest",actual["manifest"],"--file-manifest-sha256",actual["sha256"],"--root",root,"--inventory-output",inventory]
                if actual.get("declared_aliases",0):
                    arguments.append("--allow-declared-aliases")
                code,checked = attest_auxiliary("replay-"+str(index),arguments)
                component = {"role":"replay","root":root,"residency":checked}
                auxiliary.append(component)
                if checked.get("inventory",{}).get("created"):
                    destination = output/("cache-replay-"+str(index)+".files.jsonl")
                    subprocess.run(["docker","--host","unix://"+config["socket"],"cp",runtime.container+":"+inventory,str(destination)],
                                   check=True,timeout=config.get("cache_wall_seconds",15))
                    component.update(stream_inventory_artifact=destination.name,stream_inventory_sha256=sha(destination))
                if code or checked.get("status") in {"INELIGIBLE","UNAVAILABLE"}:
                    raise OriginalFailure("additional replay cold predicate " + checked.get("status","UNAVAILABLE") + "; zero attempts")
            if declared_external["semantic_files"]:
                code,checked = attest_auxiliary("semantic",[argument for path in declared_external["semantic_files"] for argument in ("--file",path)])
                auxiliary.append({"role":"semantic","required_residency_paths":declared_external["semantic_files"],"residency":checked})
                if code or checked.get("status") in {"INELIGIBLE","UNAVAILABLE"}:
                    raise OriginalFailure("additional semantic input cold predicate " + checked.get("status","UNAVAILABLE") + "; zero attempts")
        start = time.monotonic_ns()
        performance.begin(start)
        deadline = time.monotonic() + budget
        row.update(attempted_operation_count=1, sample_count=1)
        if selection["arm"] == "P" and not passthroughs:
            pstart = time.monotonic_ns()
            start_passthrough(config["native_root"], config["passthrough_mount"], 0, deadline)
            pend = time.monotonic_ns()
            phases["mount"] = phase_observation({"event": "passthrough_mount", "span": {"start_ns": pstart,
                 "end_ns": pend, "duration_ns": pend - pstart, "clock_id": "monotonic"}}, "passthrough.stdout; host monotonic readiness boundary")
        if selection["arm"] == "L" and not active_key:
            active_key, event = mount(deadline)
            bind_endpoint("measured_mount_bind",event,"first")
            bind_endpoint("measured_mount_attach",event)
            phases["mount"] = phase_observation(event, "runtime.events.jsonl")
            directory = active_key
        if case["case_id"].startswith("K"):
            scenario = case["workload"]
            for index, item in enumerate(scenario["commands"]):
                deadline = time.monotonic() + max(0, budget - performance.observe(time.monotonic_ns())["performance_ns"] / 1e9)
                text = item[-1] if isinstance(item, list) else item
                event = runtime.send("command", directory, script(output, f"command-{index}.sh", text), deadline=deadline)
                measured_events.append(event)
                runtime.send("snapshot", active_key, deadline=deadline)
                command_boundary = next(row for row in reversed(runtime.rows) if row.get("event") == "workspace_status")
                bind_endpoint("command:" + str(index), command_boundary)
                committed = runtime.send("commit", active_key, deadline=deadline)
                row.setdefault("checkpoint_commit_original_events", []).append(committed)
                require_changed_commit(committed)
                bind_endpoint("commit:"+str(index),committed)
                phases["commit"] = phase_observation(committed, "runtime.events.jsonl; individual Commit spans retained")
                if oracle_plan:
                    verify_selected(directory, verification_module.expected(oracle_plan, label="checkpoint-" + str(index + 1)),
                                    event if index == len(scenario["commands"]) - 1 else None)
        elif case["case_id"].startswith("W"):
            if selection["arm"] == "L":
                peer_key = concurrent_peer or active_key
                if case["case_id"] == "W01" and not concurrent_peer:
                    peer_key, _ = mount(deadline)
                    concurrent_peer = peer_key
                peer_directories = [mounts[active_key], mounts[peer_key]]
            else:
                peer_directories = [target for _,target in peers]
            children = case["workload"]["commands"]
            if case["case_id"] == "W02":
                measured_events.append(runtime.send("command", directory, script(output, "sequential.sh", children[0]["argv"][-1]), deadline=deadline))
                children = children[1:]
            parallel = shell_parallel([(peer_directories[index] if case["case_id"] == "W01" else peer_directories[0], child["argv"][-1]) for index, child in enumerate(children)])
            measured_events.append(runtime.send("command", directory, script(output, "parallel.sh", parallel), deadline=deadline))
            row["measured_child_results"] = child_results(measured_events[-1])
            row["concurrency_process_count"] = {"ordinary_parent": 1, "concurrent_bash_children": 2,
                                                 "sequential_commands": 1 if case["case_id"] == "W02" else 0}
        else:
            measured_events.append(runtime.send("command", directory, script(output, "command.sh", body), deadline=deadline))
        row["command_original_events"] = measured_events
        if case["case_id"].startswith("K"):
            row["checkpoint_verifier_count_scope"] = "Cumulative daemon counts include independent checkpoint verifier filesystem work. Isolated Commit selectors must use command:i -> commit:i; cross-checkpoint deltas are not Commit-only ratios. after_verifier:checkpoint-N records the original fence for subsequent command work."
        phases["command"] = phase_observation(measured_events[-1], "runtime.events.jsonl; last command span; every measured command retained in command_original_events, no interleaved Commit folded into command time")
        stream_events = [event for event in runtime.rows if event.get("event") == "streams"]
        if stream_events:
            phases["streams"] = phase_observation(stream_events[-1], "runtime.events.jsonl; transport completion only")
        if warm_end is not None:
            cache["warmup_gap_ns"] = measured_events[0]["span"]["start_ns"] - warm_end
        deadline = time.monotonic() + max(0, budget - performance.observe(time.monotonic_ns())["performance_ns"] / 1e9)
        if active_key:
            runtime.send("snapshot", active_key, deadline=deadline)
            command_status = next(event for event in reversed(runtime.rows) if event.get("event") == "workspace_status")
            bind_endpoint("command_end_status",command_status)
        else:
            runtime.send("observe", deadline=deadline)
        before_verifier_ns = performance.observe(time.monotonic_ns())["performance_ns"]
        # Verifier is a separate protocol command/budget, before uncommitted
        # state disappears. It is not supplied to the timed mutation path.
        verification = "UNAVAILABLE"
        if oracle_plan:
            if case["case_id"].startswith("K"):
                verification = "PASS" if len(row.get("verification_events", [])) == len(case["workload"]["commands"]) else "UNAVAILABLE"
            elif case["case_id"] == "W01":
                destinations = [active_key, concurrent_peer] if selection["arm"] == "L" else ["native:" + target for _,target in peers]
                verify_selected(destinations[0], verification_module.expected(oracle_plan, role="scan"), measured_events[-1])
                verify_selected(destinations[1], verification_module.expected(oracle_plan, role="writer"))
                verification = "PASS"
            else:
                selected = verification_module.expected(oracle_plan) if oracle_plan["value"]["expected"] else None
                verify_selected(directory, selected, measured_events[-1])
                verification = "PASS"
        resumed = time.monotonic_ns()
        terminal_deadline = time.monotonic() + max(0, budget - before_verifier_ns / 1e9)
        for key in list(mounts):
            event = runtime.send("unmount", key, deadline=terminal_deadline)
            phases["unmount"] = phase_observation(event, "runtime.events.jsonl")
            mounts.pop(key)
            closed_keys.append(key)
            row.setdefault("terminal_unmount_events", []).append(event)
            bind_endpoint("unmount:"+str(len(closed_keys)-1),event)
        for passthrough, target in passthroughs:
            detach = ["docker", "--host", "unix://" + config["socket"], "exec", "--user", "0:0", runtime.container, "umount", target]
            write_new(output / (passthrough.label + ".detach-attempt.json"), {"argv": detach, "container": runtime.container,
                      "mountpoint": target, "attempt": 1, "custody": "plain detach only; failure retains original process/mount"})
            detached = time.monotonic_ns()
            subprocess.run(detach, check=True, timeout=max(0.001, terminal_deadline - time.monotonic()))
            drain = passthrough.event("drain", terminal_deadline)
            joined = passthrough.event("joined", terminal_deadline)
            snapshot = joined.get("snapshot", {})
            if joined.get("clean") is not True or [snapshot.get(key) for key in ("configured", "created", "entered", "exited", "joined")] != [2,2,2,2,2] or drain.get("held_handles") != 0:
                raise OriginalFailure("original P plain detach did not establish clean two-loop join/handle drain")
            if passthrough.process.wait(timeout=max(0.001,terminal_deadline-time.monotonic())) != 0:
                raise OriginalFailure("original P process exited nonzero after join")
            ended = time.monotonic_ns()
            phases["unmount"] = phase_observation({"event": "passthrough_unmount", "span": {"start_ns": detached,
                 "end_ns": ended, "duration_ns": ended - detached, "clock_id": "monotonic"}}, "passthrough.stdout; original plain detach and drain")
            row.setdefault("passthrough_terminal", []).append({"mountpoint": target, "joined": joined, "drain": drain,
                            "span": phases["unmount"], "kernel_opcode_completeness": "UNAVAILABLE: batch/default callbacks are not exact kernel request counts"})
        row["complete_command_ns"] = before_verifier_ns + time.monotonic_ns() - resumed
        row["performance_segments"] = "Mount/command/observer interval plus explicit unmount interval; separate verifier excluded and retained"
        if case["workload"].get("fresh_mount_oracle"):
            if not oracle_plan:
                raise OriginalFailure("selected Commit fresh-mount oracle missing; no survival inference")
            fresh_key, fresh_event = mount(terminal_deadline)
            row["fresh_mount_original_span"] = fresh_event["span"]
            final_expected = verification_module.expected(oracle_plan, label="checkpoint-" + str(len(case["workload"]["commands"])))
            fresh_proof = verify_selected(fresh_key, final_expected)
            row["fresh_mount_verifier_original_span"] = fresh_proof["tree"]["original_event"]["span"]
            closed = runtime.send("unmount", fresh_key, deadline=time.monotonic() + max(0, budget - performance.observe(time.monotonic_ns())["performance_ns"] / 1e9))
            mounts.pop(fresh_key)
            closed_keys.append(fresh_key)
            row["fresh_mount_unmount_original_span"] = closed["span"]
            row["complete_command_ns"] += fresh_event["span"]["duration_ns"] + closed["span"]["duration_ns"]
        performance.finish(time.monotonic_ns())
        row["performance_clock"] = performance.observe(time.monotonic_ns())
        row["complete_command_ns"] = row["performance_clock"]["performance_ns"]
        if row["complete_command_ns"] > case["complete_command_wall_stop_ns"]:
            raise TimeoutError("registered complete performance command wall stop exceeded")
        cleanup_start = time.monotonic_ns()
        cleanup_deadline = time.monotonic() + config.get("cleanup_wall_seconds", 5)
        for key in closed_keys:
            while True:
                observed = runtime.send("cleanup", key, deadline=cleanup_deadline)
                row.setdefault("cleanup_observations", []).append(observed)
                if observed.get("fields", {}).get("state") == "Gone":
                    break
                if time.monotonic() >= cleanup_deadline:
                    raise TimeoutError("bounded readiness observation: physical cleanup not Gone")
                # Distinct read-only observations of ongoing automatic cleanup,
                # not replay of Unmount or invocation of a maintenance turn.
                time.sleep(min(0.05, max(0, cleanup_deadline - time.monotonic())))
        cleanup_end = time.monotonic_ns()
        if closed_keys:
            phases["cleanup"] = phase_observation({"event": "cleanup_to_Gone", "span": {"start_ns": cleanup_start,
                "end_ns": cleanup_end, "duration_ns": cleanup_end - cleanup_start, "clock_id": "monotonic"}}, "original closed-token readiness observations; separately reported after Unmounted")
        runtime.send("observe", deadline=time.monotonic() + config.get("observer_wall_seconds", 15))
        runtime.send("stop", deadline=time.monotonic() + budget)
        runtime.event("finished", time.monotonic() + budget)
        row["completed_operation_count"] = 1
        status = "INCOMPLETE"
        row["verification_status"] = verification
    except (OriginalFailure, TimeoutError, subprocess.TimeoutExpired, subprocess.CalledProcessError, OSError, ValueError, KeyError) as error:
        original_error = str(error)
        row["original_failure"] = {"cause": original_error, "type": type(error).__name__,
            "phase": getattr(error, "original_phase", None),
            "independent_close_failures": getattr(error, "independent_close_failures", [])}
        failed_at = time.monotonic_ns()
        if start is not None:
            row["performance_clock"] = performance.observe(failed_at)
            row["complete_command_ns"] = row["performance_clock"]["performance_ns"]
        if runtime is None:
            runtime = getattr(error, "event_process", None)
        if runtime is not None:
            row["retained_host_custody"] = runtime.retain(error)
        for passthrough, _ in passthroughs:
            passthrough.retain(error)
        if start is not None:
            status = "FAIL"
    observations = {name: receipts.unavailable("original instrument not yet ingested; never inferred zero") for name in registry.COUNTERS}
    groups, correlation = {}, None
    if runtime is not None:
        snapshot_observations(runtime, observations)
    if runtime is not None and runtime.container:
        logs_argv = ["docker", "--host", "unix://" + config["socket"], "logs", "--timestamps=false", runtime.container]
        try:
            with (output / "daemon.logs.stdout").open("xb") as out, (output / "daemon.logs.stderr").open("xb") as err:
                logs = subprocess.run(logs_argv, stdout=out, stderr=err, timeout=config.get("observer_wall_seconds", 15))
            if logs.returncode:
                raise ValueError("original daemon log observation failed")
            groups = {}
            for log in (output / "daemon.logs.stdout", output / "daemon.logs.stderr"):
                current = telemetry(log)
                if set(groups) & set(current):
                    raise ValueError("duplicate numeric call across original log streams")
                groups.update(current)
            words = lambda value: [int(value[index:index+16], 16) for index in range(0,64,16)]
            for group in groups.values():
                if not runtime.daemon_instance or not runtime.scope or group[0]["daemon"] != words(runtime.daemon_instance) or group[0]["scope"] != words(runtime.scope):
                    raise ValueError("numeric record daemon/scope differs from original authenticated runtime")
            ingest_observations(groups, observations, cache)
            correlation = None
            if selection["arm"] == "L":
                # A later verifier failure does not erase earlier typed control
                # ranges. The correlator still refuses every unmatched group.
                correlation = counts.correlate(runtime.rows, groups)
                row["numeric_correlation_status"] = "PASS"
                write_new(output / "control-call-correlation.json", {"schema": "r7-original-control-correlation-v1", "calls": correlation,
                          "scope": "successful original numeric send ranges; failed calls are never inferred successful"})
            endpoints = config.get("phase_count_endpoints", [])
            if endpoints:
                row["phase_counts"] = [counts.phase_counts(groups, item["before_identity"], item["end_identity"],
                        exclusive_observer_scope=config.get("exclusive_observer_scope") is True,
                        label=item["label"], control_correlation=correlation) for item in endpoints]
            if config.get("phase_count_selectors"):
                selected_endpoints = counts.resolve_selectors(config["phase_count_selectors"],endpoint_bindings,correlation or [])
                row["phase_selector_resolution"] = selected_endpoints
                row.setdefault("phase_counts",[]).extend(counts.phase_counts(groups,item["before_identity"],item["end_identity"],
                        exclusive_observer_scope=config.get("exclusive_observer_scope") is True,label=item["label"],control_correlation=correlation)
                        if item["status"] == "RESOLVED" else item for item in selected_endpoints)
            row["original_endpoint_bindings"] = endpoint_bindings
            write_new(output / "numeric-observation-validation.json", {"status": "PASS", "calls": len(groups), "sections": groups})
        except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
            row["numeric_correlation_status"] = "UNAVAILABLE"
            write_new(output / "numeric-observation-unavailable.json", {"status": "UNAVAILABLE", "reason": str(error)})
    cold_ineligible = cache.get("class") == "A" and (cache.get("residency", {}).get("status") == "INELIGIBLE" or
                       any(item.get("residency",{}).get("status") == "INELIGIBLE" for item in cache.get("auxiliary_residencies",[])))
    if row["attempted_operation_count"] == 0 and not cold_ineligible:
        row.update(row_status="NOT_RUN", reason=original_error or "pre-attempt preparation failed")
    else:
        row.setdefault("complete_command_ns", 0)
        row.update(row_status="INELIGIBLE" if cold_ineligible else status, reason=original_error or "original counters/cleanup not fully ingested", identities=config["identities"],
                   build_profile="release", dirty=False, construction_workers=1, store_profile="Disposable/WAL/OFF",
                   overlay_profile="MEMORY/OFF/EXCLUSIVE", exec_registration_count=0, command_launcher="ordinary external bash",
                   uid=config["uid"], gid=config["gid"], setup_method=config["setup_method"], benchmark_verification_count_in_performance=0,
                   declared_interference=config["declared_interference"], exact_command=case["workload"],
                   performance_status="PRICED_DIAGNOSTIC" if case["case_id"] in registry.DIAGNOSTIC else "DIAGNOSTIC",
                   verification_status=row.get("verification_status", "UNAVAILABLE"), resource_status="INCOMPLETE", cleanup_status="Gone" if closed_keys and not original_error else "UNAVAILABLE",
                   custody_status="RETAINED" if original_error else "KNOWN_STOP", failure_class=original_error or "missing instrument ingestion",
                   cache=cache, phases=phases, observations=observations, complete_command_wall_stop_ns=case["complete_command_wall_stop_ns"],
                   oracle_scope=case["oracle_scope"], source_arm=selection["arm"], scenario_id=case["case_id"], scenario_version=1,
                   seed_or_repetition=1, contract_commit=config.get("contract_commit", config["identities"]["source_commit"]),
                   treatment=config.get("treatment", config["identities"]["product"]), public_api_call_count=len([event for event in runtime.rows if event.get("event") in {"mount", "commit", "unmount", "workspace_status"}]),
                   forbidden_route_count=0, fallback_count=0)
        row.update({key: value for key, value in {
            "operation_contract_id": "r7-public-workspace-posix-v1", "operation_surface": "ordinary POSIX/FUSE and public SDK lifecycle",
            "operation_entrypoint": "ProjectApi/WorkspaceApi/Sandbox runtime", "orchestration_executor": "sealed R7 Python/SDK harness",
            "mutation_executor": "ordinary external Bash plus canonical Commit when selected", "implementation_route_status": "AUTHENTIC_ORIGINAL_ROUTE",
            "timing_boundary_id": "r7-priced-phases-v1", "clock_id": "monotonic; each original clock domain retained",
            "start_event": "selected measured lifecycle begins", "end_event": "original terminal unmount reply"}.items()})
        row["fixture_kind"] = config["fixture_kind"]
        row["reduced_fixture_cut_identity"] = config.get("reduced_fixture_cut_identity")
        if not case["workload"].get("commit_count"):
            for name in ("construction_work",):
                if observations[name]["status"] == "UNAVAILABLE":
                    observations[name] = receipts.unavailable("no canonical LayerFS Commit/construction selected; ordinary Git commands do not invoke it",not_applicable=True)
        if selection["arm"] != "L":
            for name in registry.COUNTERS:
                if observations[name]["status"] == "UNAVAILABLE" and not (selection["arm"] == "P" and name in {"opcode_work","native_work","mount_work"}):
                    observations[name] = receipts.unavailable("no selected LayerFS namespace/Store operation in native control",not_applicable=True)
            row["observation_domains"] = "Store/Overlay and PID1 gauges describe idle shared SDK topology only; native filesystem/command resource scopes remain unavailable"
        try:
            ownership_proof(row,runtime,groups,correlation)
        except (ValueError,KeyError,TypeError,IndexError) as error:
            row["ownership_proof"] = {"status":"UNAVAILABLE","reason":str(error)}
        diagnostic_completion(row)
    row["raw_artifacts"] = {path.relative_to(output).as_posix(): sha(path) for path in output.rglob("*") if path.is_file()}
    try:
        receipts.validate_receipt(row)
        row["receipt_validation"] = "PASS"
    except ValueError as error:
        row["receipt_validation"] = "INCOMPLETE: " + str(error)
        if row["row_status"] not in {"INELIGIBLE", "FAIL"}:
            row["row_status"] = "INCOMPLETE" if row["attempted_operation_count"] else "NOT_RUN"
    write_new(output / "receipt.json", row)
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--selection")
    group.add_argument("--all", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--claims", type=Path, required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    selected = registry.selections() if args.all else [item for item in registry.selections() if item["selection_id"] == args.selection]
    if not selected:
        parser.error("unknown prospective selection")
    if args.all:
        args.output.mkdir(parents=True, exist_ok=False)
    for selection in selected:
        cell = config.get("selections", {}).get(selection["selection_id"], config if not args.all else {})
        one(cell, selection, args.output / selection["selection_id"].replace(":", "-") if args.all else args.output, args.claims)


if __name__ == "__main__":
    main()

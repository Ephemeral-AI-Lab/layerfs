#!/usr/bin/env python3
"""External history-route driver: real service, real daemon, real frames.

This is a functional deployment proof, not a benchmark. It starts the production
`layerfs-server` with a configured history catalog, starts the production
`layerfs-daemon` as the client (on the host by default, in Linux Docker with
`--image`), and drives the daemon's own stdin/stdout frames. Nothing here
re-implements a service body, a codec or a history transition: the frames are the
production protocol and the replies are the production replies.

Profile 2, opcodes 6 (HistoryQuery) and 7 (HistoryCommand) are the history
surface; profile 1 uses generic SaveFile opcode 20. A grant mask of 31 must
grant neither history opcode.
"""
import argparse, hashlib, json, os, select, shutil, struct, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "core/target"))
PROFILE = os.environ.get("LAYERFS_PROOF_PROFILE", "debug")
if PROFILE not in ("debug", "release"):
    raise ValueError("unsupported proof build profile")
BIN = TARGET / PROFILE
# Optional daemon-client binary, for a run whose daemon is a Linux build while
# the host binaries stay native. The default is the profile directory above.
DAEMON_BINARY = Path(os.environ.get("LAYERFS_DAEMON_BINARY", BIN / "layerfs-daemon"))

FRAME_BYTES = 16384
CURSOR_BYTES = 160
HISTORY_PROFILE = 2
QUERY_OPCODE, COMMAND_OPCODE = 6, 7


def frame(kind, identity, data=b""):
    return b"LFB1" + struct.pack(">BBHQI", kind, 0, 0, identity, len(data)) + data


def begin(identity, opcode, payload, store=1, profile=1, deadline_ms=60000):
    response_bytes = 0 if opcode == 20 else 64 * 1024 * 1024
    return frame(2, identity, struct.pack(">QIHIQB", 1, store, profile, deadline_ms, response_bytes, opcode) + payload)


def read_exact(fd, count, end):
    out = bytearray()
    while len(out) < count:
        if not select.select([fd], [], [], max(0, end - time.monotonic()))[0]:
            raise TimeoutError("terminal response missing")
        part = os.read(fd, count - len(out))
        if not part:
            raise EOFError("terminal response missing")
        out.extend(part)
    return bytes(out)


def receive(process, timeout=30):
    end = time.monotonic() + timeout
    while True:
        header = read_exact(process.stdout.fileno(), 20, end)
        assert header[:4] == b"LFB1", header
        kind, flags, reserved, identity, size = struct.unpack(">BBHQI", header[4:])
        assert flags == 0 and reserved == 0 and size <= 32768, header
        body = read_exact(process.stdout.fileno(), size, end)
        if kind == 5:
            continue
        assert kind in (6, 7), kind
        return kind, body


def exchange(process, identity, opcode, payload, profile=1, body=b""):
    process.stdin.write(begin(identity, opcode, payload, profile=profile))
    for offset in range(0, len(body), FRAME_BYTES):
        process.stdin.write(frame(3, identity, body[offset:offset + FRAME_BYTES]))
    process.stdin.write(frame(4, identity, struct.pack(">Q", len(body))))
    process.stdin.flush()
    return receive(process)


def blob(value):
    return struct.pack(">H", len(value)) + value


def optional(value):
    return b"\0" if value is None else b"\1" + value


def save_file_metadata(length):
    return b"\0" + struct.pack(">QQQQ", 0, length, int(length != 0), length)


def save_file_body(data):
    return (struct.pack(">QQQ", 1, 0, len(data)) + data) if data else b""


class Reader:
    def __init__(self, data):
        self.data, self.at = data, 0

    def take(self, n):
        value = self.data[self.at:self.at + n]
        assert len(value) == n, (n, len(self.data) - self.at)
        self.at += n
        return value

    def u8(self):
        return self.take(1)[0]

    def u16(self):
        return struct.unpack(">H", self.take(2))[0]

    def u64(self):
        return struct.unpack(">Q", self.take(8))[0]

    def blob(self):
        return self.take(self.u16())

    def optional(self, width):
        return self.take(width) if self.u8() else None

    def done(self):
        assert self.at == len(self.data), (self.at, len(self.data))


def stack_record(r):
    return {"stack": r.take(17), "name": r.blob(), "scope": r.take(32), "profile": r.take(32), "head_layer": r.take(33)}


def branch_record(r):
    return {"branch": r.take(17), "stack": r.take(17), "name": r.blob(), "base_layer": r.take(33),
            "head_commit": r.optional(33)}


def commit_record(r):
    return {"commit": r.take(33), "stack": r.take(17), "root": r.take(32), "parent": r.optional(33),
            "base_layer": r.take(33)}


def layer_record(r):
    return {"layer": r.take(33), "stack": r.take(17), "parent": r.optional(33), "root": r.take(32),
            "source_branch": r.optional(17), "source_commit": r.optional(33)}


def stage_record(r):
    return {"workspace": r.take(32), "token": r.u64(), "stack": r.take(17), "branch": r.take(17),
            "expected_head": r.optional(33), "expected_base": r.take(33), "expected_root": r.take(32),
            "construction_base_root": r.take(32), "intended_commit_base": r.take(33),
            "candidate_root": r.take(32), "profile": r.take(32), "scope": r.take(32), "generation": r.u64()}


def history(body):
    r = Reader(body)
    assert r.u8() == 8, "history result tag"
    tag = r.u8()
    if tag == 12:
        record = stack_record(r)
        record.update(root=r.take(32), root_serial=r.u64())
        value = ("StackCreated", record)
    elif tag == 3:
        value = ("BranchSnapshot", {"branch": branch_record(r), "head_root": r.optional(32),
                                    "base_root": r.take(32), "effective_root": r.take(32),
                                    "scope": r.take(32), "profile": r.take(32), "root_serial": r.optional(8)})
    elif tag == 10:
        value = ("Stage", stage_record(r))
    elif tag == 13:
        if r.u8() == 0:
            value = ("Committed", commit_record(r))
        else:
            value = ("UpToDate", {"head": r.optional(33), "root": r.take(32)})
    elif tag == 14:
        inner = r.u8()
        if inner == 0:
            value = ("Added", layer_record(r))
        elif inner == 1:
            value = ("PublishedUpToDate", r.take(33))
        else:
            value = ("NoChanges", r.take(33))
    elif tag == 15:
        value = ("Discarded", r.u8())
    elif tag == 16:
        value = ("Reservation", {"scope": r.take(32), "start": r.u64(), "count": r.u64()})
    elif tag == 8:
        value = ("Layer", layer_record(r))
    elif tag == 6:
        value = ("Commit", commit_record(r))
    elif tag == 9:
        continuation = r.blob()
        records = [layer_record(r) for _ in range(r.u16())]
        value = ("Layers", {"continuation": continuation, "records": records})
    elif tag == 7:
        continuation = r.blob()
        records = [commit_record(r) for _ in range(r.u16())]
        value = ("Commits", {"continuation": continuation, "records": records})
    elif tag == 4:
        continuation = r.blob()
        records = [branch_record(r) for _ in range(r.u16())]
        value = ("Branches", {"continuation": continuation, "records": records})
    elif tag == 1:
        value = ("Stack", stack_record(r))
    else:
        raise AssertionError(f"unexpected history tag {tag}")
    r.done()
    return value


def public_key(private):
    env = os.environ.copy()
    env["LAYERFS_PRIVATE_KEY"] = private
    return subprocess.check_output([BIN / "examples/public_key"], env=env, text=True).strip()


def provision_store(path):
    """Provision the C2 store the service opens.

    `Store::open` never creates a database, and the service has no create-store
    operation, so an operator provisions the content store first. This uses the
    existing C2 fixture executable for that one step only; the history namespace
    this driver then exercises is built by production service code.
    """
    subprocess.check_output([BIN / "examples/prepare_store", str(path)], text=True)
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def native_directory(temp, name="source"):
    """Create one host directory the Service is configured to import.

    The native-directory importer is the only namespace-initialization route: it
    reads a real operator-configured directory, so a fixture that needs namespace
    content writes it here before asking the Service to import it.
    """
    source = Path(temp) / name
    source.mkdir(parents=True, exist_ok=True)
    return source


def start_service(temp, port, server_key, peers, import_root):
    env = os.environ.copy()
    env.update(LAYERFS_IMPORT_ROOT=str(import_root),
               LAYERFS_PRIVATE_KEY=server_key, LAYERFS_PEERS=peers,
               LAYERFS_STORE=str(Path(temp) / "store.sqlite"),
               LAYERFS_LISTEN=f"127.0.0.1:{port}", LAYERFS_TELEMETRY="off",
               LAYERFS_HISTORY_CATALOG=str(Path(temp) / "history.sqlite"),
               LAYERFS_HISTORY_BINDING="layerfs-history-route", LAYERFS_HISTORY_CREATE="1",
               LAYERFS_HISTORY_INCARNATION="1", LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex())
    child = subprocess.Popen([BIN / "layerfs-server"], env=env, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    line = child.stderr.readline()
    assert b"ready" in line, line
    return child, line.decode().strip()


def start_daemon(temp, port, client_key, server_public, selector, image):
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=client_key, LAYERFS_SERVER_KEY=server_public,
               LAYERFS_ENDPOINT=f"127.0.0.1:{port}", LAYERFS_SELECTOR=str(selector),
               LAYERFS_TELEMETRY="off")
    if image is None:
        return subprocess.Popen([DAEMON_BINARY], env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE), "host"
    name = f"layerfs-history-route-{os.getpid()}"
    command = ["docker", "run", "--name", name, "--rm", "--log-driver=none", "--read-only",
               "--cap-drop=ALL", "--security-opt=no-new-privileges", "--cpus=1", "--memory=128m",
               "--memory-swap=128m", "--pids-limit=16", "-i", "--add-host=host.docker.internal:host-gateway"]
    for key in ("LAYERFS_PRIVATE_KEY", "LAYERFS_SERVER_KEY", "LAYERFS_SELECTOR", "LAYERFS_TELEMETRY"):
        command += ["-e", key]
    env["LAYERFS_ENDPOINT"] = f"host.docker.internal:{port}"
    command += ["-e", "LAYERFS_ENDPOINT"]
    if Path(DAEMON_BINARY).parent != BIN:
        command += ["-v", f"{Path(DAEMON_BINARY).parent}:/linux-runner:ro"]
        command += [image, "/linux-runner/" + Path(DAEMON_BINARY).name]
        return subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE), name
    command += [image]
    return subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE), name


def run_cases(daemon, evidence, source):
    stack_body, branch_body, workspace = b"\x51" * 16, b"\x61" * 16, b"\x71" * 32
    seed = bytes(range(32))
    payload = b"history route payload"
    kind, body = exchange(daemon, 1, 20, save_file_metadata(len(payload)), body=save_file_body(payload))
    assert kind == 6 and body[0] == 2, body
    file_root = body[1:33]
    second = b"history route payload, second version"
    kind, body = exchange(daemon, 2, 20, save_file_metadata(len(second)), body=save_file_body(second))
    assert kind == 6 and body[0] == 2, body
    second_root = body[1:33]
    evidence.append({"id": "R01", "case": "generic file save through profile 1", "status": "PASS"})

    # The namespace file holds the exact bytes the earlier generic save stored,
    # so the published file root must be that root again.
    (source / "a").write_bytes(payload)
    init = b"\x09" + stack_body + blob(b"main") + seed
    kind, body = exchange(daemon, 3, COMMAND_OPCODE, init, HISTORY_PROFILE)
    assert kind == 6, body
    tag, record = history(body)
    assert tag == "StackCreated", tag
    stack, genesis = record["stack"], record["head_layer"]
    assert stack[0] == 0x31 and genesis[0] == 0x32
    # R03 below reads the genesis Layer's filesystem root back through the same
    # query surface and checks it against the fork's effective root; R09 then
    # compares the published file root with the generic save's exact root.
    evidence.append({"id": "R02", "case": "native-directory import through the daemon", "status": "PASS"})

    fork = b"\x02" + stack + branch_body + blob(b"work") + b"\x01" + genesis
    kind, body = exchange(daemon, 4, COMMAND_OPCODE, fork, HISTORY_PROFILE)
    assert kind == 6, body
    tag, snapshot = history(body)
    assert tag == "BranchSnapshot", tag
    branch = snapshot["branch"]["branch"]
    scope, profile = snapshot["scope"], snapshot["profile"]
    assert snapshot["head_root"] is None
    assert snapshot["effective_root"] == snapshot["base_root"]
    kind, body = exchange(daemon, 5, QUERY_OPCODE, b"\x07" + genesis, HISTORY_PROFILE)
    assert kind == 6, body
    tag, genesis_layer = history(body)
    assert tag == "Layer" and genesis_layer["root"] == snapshot["effective_root"]
    assert genesis_layer["parent"] is None and genesis_layer["source_branch"] is None
    evidence.append({"id": "R03", "case": "fork from a Layer shares its root", "status": "PASS"})

    staged_payload = (b"\x03" + workspace + branch + optional(None) + genesis
                      + struct.pack(">Q", 1) + file_root + scope + struct.pack(">Q", 1)
                      + struct.pack(">H", 1) + struct.pack(">Q", 1) + struct.pack(">H", 0)
                      + struct.pack(">H", 1) + struct.pack(">QB", 2, 1) + file_root + file_root)
    # The metadata root of the existing inode is read back through the production
    # stat surface rather than guessed.
    kind, body = exchange(daemon, 6, 2, genesis_layer["root"] + b"\x01" + blob(b"a"))
    assert kind == 6 and body[0] == 4, body
    metadata_root = body[50:82]
    staged_payload = (b"\x03" + workspace + branch + optional(None) + genesis
                      + struct.pack(">Q", 1) + genesis_layer["root"] + scope + struct.pack(">Q", 1)
                      + struct.pack(">H", 1) + struct.pack(">Q", 1) + struct.pack(">H", 0)
                      + struct.pack(">H", 1) + struct.pack(">QB", 2, 1) + second_root + metadata_root)
    kind, body = exchange(daemon, 7, COMMAND_OPCODE, staged_payload, HISTORY_PROFILE)
    assert kind == 6, body
    tag, stage = history(body)
    assert tag == "Stage" and stage["token"] == 1, (tag, stage)
    assert stage["expected_root"] == genesis_layer["root"]
    evidence.append({"id": "R04", "case": "stage_changes freezes exact context", "status": "PASS"})

    kind, body = exchange(daemon, 8, COMMAND_OPCODE,
                          b"\x04" + workspace + struct.pack(">Q", stage["token"]), HISTORY_PROFILE)
    assert kind == 6, body
    tag, commit = history(body)
    assert tag == "Committed", (tag, commit)
    assert commit["parent"] is None and commit["base_layer"] == genesis
    evidence.append({"id": "R05", "case": "commit_staged advances the Branch", "status": "PASS"})

    kind, body = exchange(daemon, 9, COMMAND_OPCODE,
                          b"\x06" + stack + branch + commit["commit"] + genesis + genesis, HISTORY_PROFILE)
    assert kind == 6, body
    tag, layer = history(body)
    assert tag == "Added", (tag, layer)
    assert layer["root"] == commit["root"] and layer["parent"] == genesis
    assert layer["source_branch"] == branch and layer["source_commit"] == commit["commit"]
    evidence.append({"id": "R06", "case": "add_layer publishes and advances the stack", "status": "PASS"})

    kind, body = exchange(daemon, 10, COMMAND_OPCODE,
                          b"\x06" + stack + branch + commit["commit"] + genesis + genesis, HISTORY_PROFILE)
    tag, again = history(body)
    assert kind == 6 and tag == "PublishedUpToDate" and again == layer["layer"], (tag, again)
    evidence.append({"id": "R07", "case": "repeat publication is idempotent", "status": "PASS"})

    kind, body = exchange(daemon, 11, QUERY_OPCODE, b"\x08" + stack + optional(None) + blob(b"") + struct.pack(">H", 8),
                          HISTORY_PROFILE)
    tag, page = history(body)
    assert kind == 6 and tag == "Layers" and len(page["records"]) == 2, (tag, page)
    assert page["records"][1]["layer"] == genesis
    evidence.append({"id": "R08", "case": "layer_history returns the chain", "status": "PASS"})

    kind, body = exchange(daemon, 12, 2, layer["root"] + b"\x01" + blob(b"a"))
    assert kind == 6 and body[0] == 4 and body[18:50] == second_root, body[:60]
    evidence.append({"id": "R09", "case": "logical readback of the published root", "status": "PASS"})

    # The Commit consumed the exact stage, so the same token removes nothing.
    kind, body = exchange(daemon, 13, COMMAND_OPCODE, b"\x07" + workspace + struct.pack(">Q", 1), HISTORY_PROFILE)
    assert kind == 6, body
    tag, discarded = history(body)
    assert tag == "Discarded" and discarded == 0, (tag, discarded)
    evidence.append({"id": "R10", "case": "a committed token removes nothing", "status": "PASS"})

    # A delayed token must not consume a replacement stage.
    replacement_workspace = b"\x72" * 32
    stage_payload = (b"\x03" + replacement_workspace + branch + optional(commit["commit"]) + genesis
                     + struct.pack(">Q", 2) + layer["root"] + scope + struct.pack(">Q", 1)
                     + struct.pack(">H", 1) + struct.pack(">Q", 1) + struct.pack(">H", 0)
                     + struct.pack(">H", 1) + struct.pack(">QB", 2, 1) + file_root + metadata_root)
    kind, body = exchange(daemon, 14, COMMAND_OPCODE, stage_payload, HISTORY_PROFILE)
    assert kind == 6, body
    tag, first = history(body)
    assert tag == "Stage", (tag, first)
    kind, body = exchange(daemon, 15, COMMAND_OPCODE,
                          b"\x07" + replacement_workspace + struct.pack(">Q", first["token"]), HISTORY_PROFILE)
    tag, removed = history(body)
    assert kind == 6 and tag == "Discarded" and removed == 1, (tag, removed)
    kind, body = exchange(daemon, 16, COMMAND_OPCODE, stage_payload, HISTORY_PROFILE)
    assert kind == 6, body
    tag, replacement = history(body)
    assert tag == "Stage" and replacement["token"] != first["token"], (tag, replacement)
    kind, body = exchange(daemon, 17, COMMAND_OPCODE,
                          b"\x07" + replacement_workspace + struct.pack(">Q", first["token"]), HISTORY_PROFILE)
    assert kind == 7 and body[0] == 16, body
    # A refusal ends this daemon session; the retained stage is verified on the
    # next connection, which is also what proves the stage is not session state.
    return {"stack": stack.hex(), "branch": branch.hex(), "genesis": genesis.hex(),
            "commit": commit["commit"].hex(), "layer": layer["layer"].hex(),
            "root": layer["root"].hex(),
            "replacement_workspace": replacement_workspace.hex(),
            "replacement_token": replacement["token"]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", default=None, help="Linux Docker image for the daemon client")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    evidence = {"kind": "functional-deployment", "performance": "NOT_RUN",
                "client": "linux-docker" if args.image else "host",
                "profile": PROFILE, "cases": [], "cleanup": "INCOMPLETE"}
    if args.image:
        evidence["image_id"] = json.loads(subprocess.check_output(
            ["docker", "image", "inspect", args.image], text=True))[0]["Id"]
    service = daemon = None
    try:
        with tempfile.TemporaryDirectory(prefix="layerfs-history-route-") as temp:
            server_key, allowed_key, legacy_key = (os.urandom(32).hex() for _ in range(3))
            server_public = public_key(server_key)
            allowed_public, legacy_public = public_key(allowed_key), public_key(legacy_key)
            with __import__("socket").socket() as probe:
                probe.bind(("127.0.0.1", 0))
                port = probe.getsockname()[1]
            expires = int(time.time()) + 3600
            peers = f"1,{allowed_public},{expires},127;2,{legacy_public},{expires},31"
            evidence["store_provisioning"] = {
                "method": "existing C2 fixture executable (Store::open never creates)",
                "store_sha256": provision_store(Path(temp) / "store.sqlite"),
            }
            source = native_directory(temp)
            service, ready = start_service(temp, port, server_key, peers, source)
            evidence["service_ready"] = ready
            evidence["binaries"] = {str(BIN / "layerfs-server"): hashlib.sha256((BIN / "layerfs-server").read_bytes()).hexdigest()}

            # A legacy grant mask of 31 must not reach a history opcode.
            legacy, name = start_daemon(temp, port, legacy_key, server_public, 2, args.image)
            daemon = legacy
            kind, body = exchange(legacy, 1, QUERY_OPCODE, b"\x01" + b"\x31" * 17, HISTORY_PROFILE)
            assert kind == 7 and body[0] == 3, body
            evidence["cases"].append({"id": "R00", "case": "legacy mask 31 grants no history", "status": "PASS",
                                      "client_name": name})
            legacy.stdin.close()
            legacy.wait(timeout=10)

            client, name = start_daemon(temp, port, allowed_key, server_public, 1, args.image)
            daemon = client
            evidence["client_name"] = name
            identities = run_cases(client, evidence["cases"], source)
            evidence["identities"] = identities
            evidence["cases"].append({"id": "R11", "case": "delayed token cannot consume a replacement stage",
                                      "status": "PASS"})
            client.stdin.close()
            client.wait(timeout=10)
            daemon = None

            # A new connection, same continuing authority: the replacement stage
            # survived the refused discard and is visible without any local state.
            witness, name = start_daemon(temp, port, allowed_key, server_public, 1, args.image)
            daemon = witness
            kind, body = exchange(witness, 1, QUERY_OPCODE,
                                  b"\x09" + bytes.fromhex(identities["replacement_workspace"]), HISTORY_PROFILE)
            assert kind == 6, body
            tag, retained = history(body)
            assert tag == "Stage" and retained["token"] == identities["replacement_token"], (tag, retained)
            evidence["cases"].append({"id": "R12", "case": "stage survives on a new connection", "status": "PASS"})
            kind, body = exchange(witness, 2, QUERY_OPCODE,
                                  b"\x03" + bytes.fromhex(identities["branch"]), HISTORY_PROFILE)
            assert kind == 6, body
            tag, descriptor = history(body)
            assert tag == "BranchSnapshot" and descriptor["root_serial"] == struct.pack(">Q", 1)
            assert descriptor["effective_root"].hex() == identities["root"]
            evidence["cases"].append({"id": "R13", "case": "GetBranch returns validated root serial", "status": "PASS"})
            witness.stdin.close()
            witness.wait(timeout=10)
            daemon = None
            evidence["cleanup"] = "PASS"
    finally:
        for child in (daemon, service):
            if child is not None:
                try:
                    child.kill()
                    child.wait(timeout=10)
                except Exception:
                    pass
        for label, child in (("daemon", daemon), ("service", service)):
            if child is not None and child.stderr is not None:
                try:
                    (args.output / f"{label}.stderr").write_bytes(child.stderr.read())
                except Exception:
                    pass
    evidence["status"] = "PASS" if all(case["status"] == "PASS" for case in evidence["cases"]) else "FAIL"
    (args.output / "history-route.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"status": evidence["status"], "cases": len(evidence["cases"]),
                      "client": evidence["client"]}, indent=2))
    return 0 if evidence["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())

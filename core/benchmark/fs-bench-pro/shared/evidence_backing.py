"""Retained E04 v2 guest-volume custody, never host/guest physical equivalence.

The owning validator supplies its bounded readers and exact-field checks. This
module never executes Docker, probes a running guest, or imports the caller.
"""
from pathlib import Path, PurePosixPath
import hashlib
import re
import stat


class Validator:
    def __init__(self, require, exact, artifact, read_json, integer):
        self.require, self.exact = require, exact
        self.artifact, self.read_json, self.integer = artifact, read_json, integer

    def read(self, descriptor, root):
        return self.read_json(self.artifact(descriptor, root))[0]

    def receipt(self, descriptor, root, command=None, exit_code=0):
        value = self.read(descriptor, root)
        self.require(isinstance(value, dict) and isinstance(value.get("command"), list)
            and all(isinstance(arg, str) for arg in value["command"])
            and (command is None or value["command"] == command)
            and type(value.get("exit_code")) is int
            and (0 <= value["exit_code"] <= 255 if exit_code is None else value["exit_code"] == exit_code)
            and value.get("timed_out") is False, "native backing original command failed or substituted")
        self.require(all(self.integer(value.get(key)) for key in
            ("started_monotonic_ns", "finished_monotonic_ns", "wall_ns"))
            and value["finished_monotonic_ns"] >= value["started_monotonic_ns"]
            and value["wall_ns"] == value["finished_monotonic_ns"] - value["started_monotonic_ns"],
            "native backing command causal boundaries missing")
        return value

    def follows(self, before, after):
        self.require(before["finished_monotonic_ns"] <= after["started_monotonic_ns"],
                     "native backing setup/consumer/observer command order changed")

    def raw_output(self, descriptor, receipt_descriptor, suffix, root):
        path = self.artifact(descriptor, root)
        self.require(path == self.artifact(receipt_descriptor, root).with_suffix(suffix),
                     "native backing stdout/stderr belongs to another original command")
        return path

    def container(self, group, root, image, backing, host_root, argv, writable, expected_command=None, exit_code=0, version=2):
        cid = group["container_id"]
        self.require(isinstance(cid, str) and re.fullmatch(r"[0-9a-f]{64}", cid),
                     "native backing container identity missing")
        outer = self.receipt(group["command_receipt"], root, expected_command, exit_code)
        inspected = self.receipt(group["inspect_receipt"], root, ["docker", "inspect", cid])
        self.follows(outer, inspected)
        data = self.read_json(self.raw_output(group["inspect_stdout"], group["inspect_receipt"], ".stdout", root))[0]
        self.require(isinstance(data, list) and len(data) == 1 and isinstance(data[0], dict)
            and data[0].get("Id") == cid and data[0].get("Image") == image,
            "native backing raw inspect identity differs")
        info = data[0]
        state, configuration = info.get("State"), info.get("HostConfig")
        self.require(isinstance(state, dict) and isinstance(configuration, dict)
            and state.get("Running") is False and type(state.get("ExitCode")) is int
            and state["ExitCode"] == outer["exit_code"] and configuration.get("AutoRemove") is False,
            "native backing original container is live, removed or has another exit")
        cid_path = self.artifact(group["cid_file"], root)
        with cid_path.open("rb") as source:
            cid_bytes = source.read(129)
        self.require(len(cid_bytes) <= 128 and cid_bytes.strip() == cid.encode(),
                     "native backing original CID file differs")
        self.direct_command(outer["command"], argv, image, cid_path, info, backing, host_root, writable)
        mounts = info.get("Mounts")
        self.require(isinstance(mounts, list) and len(mounts) == 2
            and all(isinstance(mount, dict) for mount in mounts), "native backing mounts absent or overlaid")
        by_path = {mount.get("Destination"): mount for mount in mounts}
        self.require(set(by_path) == {"/work", "/state"}, "native backing mounts absent or overlaid")
        bind, volume = by_path["/work"], by_path["/state"]
        if version == 3:
            self.require(configuration.get("ContainerIDFile") == str(cid_path)
                and info.get("Config", {}).get("Volumes") in (None, {}), "v3 original CID/inherited volumes differ")
        bind_source = self.bind_source(configuration, backing, writable) if version == 3 else str(host_root)
        self.require(bind.get("Type") == "bind" and bind.get("Source") == bind_source and bind.get("RW") is True,
                     "native backing actual repository bind differs")
        self.require(volume.get("Type") == "volume" and volume.get("Name") == backing["name"]
            and volume.get("Driver") == backing["driver"] and volume.get("Source") == backing["mountpoint"]
            and volume.get("RW") is writable, "native backing original named volume substituted")
        if "container_name" in group:
            self.require(info.get("Name") == "/" + group["container_name"], "native backing helper container name differs")
        return outer, inspected

    def bind_source(self, configuration, backing, writable):
        binds = configuration.get("Binds")
        self.require(isinstance(binds, list) and len(binds) == 2
            and configuration.get("VolumesFrom") in (None, []) and configuration.get("Mounts") in (None, [])
            and configuration.get("Tmpfs") in (None, {}),
            "v3 Docker bind inventory/VolumesFrom is ambiguous")
        actual = {}
        for item in binds:
            self.require(isinstance(item, str), "v3 Docker bind descriptor absent")
            parts = item.split(":")
            self.require(len(parts) in (2, 3) and parts[1] not in actual,
                         "v3 Docker bind descriptor repeated or ambiguous")
            actual[parts[1]] = (parts[0], parts[2] if len(parts) == 3 else "rw")
        self.require(set(actual) == {"/work", "/state"} and actual["/work"][1] == "rw"
            and PurePosixPath(actual["/work"][0]).is_absolute()
            and ".." not in PurePosixPath(actual["/work"][0]).parts
            and actual["/state"] == (backing["name"], "rw" if writable else "ro"),
            "v3 Docker actual bind destinations/access differ")
        return actual["/work"][0]

    def direct_command(self, command, argv, image, cid_path, info, backing, host_root, writable):
        self.require(command[:2] == ["docker", "run"], "native backing outer command is not direct Docker run")
        aliases = {"--cidfile": "cidfile", "--name": "name", "--workdir": "workdir", "-w": "workdir",
            "--env": "env", "-e": "env", "--volume": "volume", "-v": "volume", "--label": "label", "-l": "label"}
        options, environment, labels, volumes = {}, {}, {}, []
        index = 2
        while index < len(command) and command[index].startswith("-"):
            flag = command[index]
            key, separator, value = flag.partition("=") if flag.startswith("--") else (flag, "", "")
            self.require(key in aliases, "native backing outer option unregistered (including removal or wrapper)")
            if not separator:
                index += 1
                self.require(index < len(command), "native backing outer option has no value")
                value = command[index]
            self.require(bool(value), "native backing outer option is empty")
            name = aliases[key]
            if name in ("env", "label"):
                variable, equals, setting = value.partition("=")
                target = environment if name == "env" else labels
                self.require(bool(equals) and bool(variable) and variable not in target,
                             "native backing environment/label is inherited or ambiguous")
                target[variable] = setting
            elif name == "volume":
                volumes.append(value)
            else:
                self.require(name not in options, "native backing repeated outer option")
                options[name] = value
            index += 1
        self.require(command[index:] == [image, *argv], "native backing image/effective argv substituted")
        self.require(options.get("cidfile") == str(cid_path) and options.get("workdir") == "/work"
            and environment.get("LAYERFS_CONSTRUCTION_WORKERS") == "1", "native backing CID/workdir/construction environment differs")
        expected_volume = backing["name"] + ":/state" + ("" if writable else ":ro")
        volume_options = [expected_volume, expected_volume + ":rw"] if writable else [expected_volume]
        self.require(len(volumes) == 2 and volumes[0] in (str(host_root) + ":/work", str(host_root) + ":/work:rw")
            and volumes[1] in volume_options,
            "native backing original two-mount command differs")
        role = "consumer" if writable else argv[3]
        self.require(labels == {**backing["labels"], "layerfs.e04.role": role},
                     "native backing container ownership labels differ")
        config = info.get("Config")
        self.require(isinstance(config, dict) and config.get("Entrypoint") in (None, [])
            and "Entrypoint" in config and config.get("Cmd") == argv
            and info.get("Path") == argv[0] and info.get("Args") == argv[1:]
            and config.get("WorkingDir") == "/work", "native backing inspected effective process differs")
        actual_env = config.get("Env")
        self.require(isinstance(actual_env, list) and all(isinstance(item, str) and "=" in item for item in actual_env),
                     "native backing inspected environment unavailable")
        actual = dict(item.split("=", 1) for item in actual_env)
        self.require(len(actual) == len(actual_env) and all(actual.get(key) == value for key, value in environment.items()),
                     "native backing declared runtime environment differs")
        self.require(isinstance(config.get("Labels"), dict)
            and all(config["Labels"].get(key) == value for key, value in labels.items()),
            "native backing inspected ownership labels differ")
        if "name" in options:
            self.require(info.get("Name") == "/" + options["name"], "native backing original container name differs")

    def volume(self, backing, root, receipt_id):
        self.exact(backing, ("kind", "name", "driver", "mountpoint", "database_path", "container_root",
            "receipt_id", "owner_label", "labels", "retention", "volume_absence_receipt", "volume_absence_stdout",
            "volume_absence_stderr", "volume_create_receipt", "volume_inspect_receipt", "volume_inspect_stdout"),
            "native backing volume selection")
        self.require(backing["kind"] == "docker-volume" and backing["driver"] == "local" and backing["container_root"] == "/state"
            and backing["database_path"] == "/state/overlay.sqlite" and backing["receipt_id"] == receipt_id
            and backing["retention"] == "RETAIN_ORIGINAL_VOLUME_ON_SUCCESS_OR_FAILURE"
            and all(isinstance(backing[key], str) and backing[key] for key in ("name", "driver", "mountpoint", "owner_label"))
            and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", backing["name"])
            and PurePosixPath(backing["mountpoint"]).is_absolute(), "native backing ownership/placement/retention missing")
        self.require(backing["labels"] == {"layerfs.e04.receipt": receipt_id, "layerfs.e04.owner": backing["owner_label"]},
                     "native backing selected ownership labels differ")
        absence = self.receipt(backing["volume_absence_receipt"], root, ["docker", "volume", "inspect", backing["name"]], 1)
        stdout = self.raw_output(backing["volume_absence_stdout"], backing["volume_absence_receipt"], ".stdout", root)
        stderr = self.raw_output(backing["volume_absence_stderr"], backing["volume_absence_receipt"], ".stderr", root)
        self.require(self.read_json(stdout)[0] == [], "native backing volume existed before original creation")
        with stderr.open("rb") as source:
            error = source.read(4097)
        self.require(len(error) <= 4096 and error.strip() ==
            f"Error response from daemon: get {backing['name']}: no such volume".encode(),
            "native backing absence is not an original no-such-volume refusal")
        create_command = ["docker", "volume", "create", "--label", "layerfs.e04.receipt=" + receipt_id,
            "--label", "layerfs.e04.owner=" + backing["owner_label"], backing["name"]]
        created = self.receipt(backing["volume_create_receipt"], root, create_command)
        create_stdout = self.raw_output(created.get("stdout"), backing["volume_create_receipt"], ".stdout", root)
        with create_stdout.open("rb") as source:
            created_name = source.read(4097)
        self.require(len(created_name) <= 4096 and created_name.strip() == backing["name"].encode(),
                     "native backing original create stdout identity differs")
        inspected = self.receipt(backing["volume_inspect_receipt"], root, ["docker", "volume", "inspect", backing["name"]])
        self.follows(absence, created)
        self.follows(created, inspected)
        value = self.read_json(self.raw_output(backing["volume_inspect_stdout"], backing["volume_inspect_receipt"], ".stdout", root))[0]
        self.require(isinstance(value, list) and len(value) == 1 and isinstance(value[0], dict),
                     "native backing original volume inspect absent")
        actual = value[0]
        self.require(actual.get("Name") == backing["name"] and actual.get("Driver") == backing["driver"]
            and actual.get("Mountpoint") == backing["mountpoint"] and actual.get("Labels") == backing["labels"]
            and actual.get("Scope") == "local" and actual.get("Options") in (None, {}),
            "native backing original volume identity/options differ")
        return inspected

    def observation(self, group, root, phase, backing, image, host_root, sources, version=2):
        self.exact(group, ("container_id", "container_name", "command_receipt", "inspect_receipt", "inspect_stdout",
            "cid_file", "observation_stdout", "probe_source", "probe_argv"), "native backing " + phase + " witness")
        probe = self.artifact(group["probe_source"], root)
        self.require(probe in sources and probe.is_relative_to(host_root), "native backing probe absent from frozen source inventory")
        argv = ["python3", "-B", "/work/" + probe.relative_to(host_root).as_posix(), phase,
                backing["receipt_id"], backing["owner_label"], backing["name"]]
        self.require(group["probe_argv"] == argv, "native backing probe source/argv substituted")
        outer, inspected = self.container(group, root, image, backing, host_root, argv, False, version=version)
        value = self.read_json(self.raw_output(group["observation_stdout"], group["command_receipt"], ".stdout", root))[0]
        fields = {"schema", "phase", "receipt_id", "owner_label", "volume_name", "root_path", "database_path",
            "filesystem", "root_identity", "database"}
        if version == 3:
            fields.add("probe_source")
        self.exact(value, fields, "native backing actual guest observation")
        self.require(value["schema"] == ("e04-native-backing-observation-v2" if version == 3 else "e04-native-backing-observation-v1") and value["phase"] == phase
            and value["receipt_id"] == backing["receipt_id"] and value["owner_label"] == backing["owner_label"]
            and value["volume_name"] == backing["name"] and value["root_path"] == "/state"
            and value["database_path"] == backing["database_path"], "native backing observation belongs to another original")
        if version == 3:
            source = self.exact(value["probe_source"], ("path", "bytes", "sha256"), "original executing probe source")
            self.require(source["path"] == argv[2] and self.integer(source["bytes"])
                and source["bytes"] == probe.stat().st_size and source["sha256"] == group["probe_source"]["sha256"],
                "v3 original executing probe bytes differ from sealed source")
        filesystem = self.exact(value["filesystem"], ("type", "magic", "mountinfo"), "observed native filesystem")
        self.require(isinstance(filesystem["type"], str) and filesystem["type"] and self.integer(filesystem["magic"])
            and filesystem["magic"] <= 0xFFFFFFFF and isinstance(filesystem["mountinfo"], str),
            "native backing actual filesystem observation unavailable")
        identity = self.exact(value["root_identity"], ("device", "inode", "mode", "uid", "gid"), "native volume root identity")
        self.require(all(self.integer(item) for item in identity.values()) and stat.S_ISDIR(identity["mode"]),
                     "native backing observed root is not a directory")
        mount = filesystem["mountinfo"].split()
        self.require(mount.count("-") == 1 and len(mount) >= 10, "native backing actual mount context unavailable")
        separator = mount.index("-")
        # st_dev is Linux's encoded device number, even when this retained
        # validator runs on Darwin. Host os.major/minor would change domains.
        device = identity["device"]
        major = ((device >> 8) & 0xFFF) | ((device >> 32) & 0xFFFFF000)
        minor = (device & 0xFF) | ((device >> 12) & 0xFFFFFF00)
        self.require(separator >= 6 and len(mount) == separator + 4 and mount[4] == "/state"
            and mount[2] == f"{major}:{minor}"
            and mount[separator + 1] == filesystem["type"] and "ro" in mount[5].split(","),
            "native backing filesystem/mount/device context differs")
        value["mount_identity"] = (mount[2], mount[3], mount[separator + 1], mount[separator + 2])
        database = value["database"]
        if phase == "pre-start":
            self.exact(database, ("exists",), "native backing initial database absence")
            self.require(database["exists"] is False, "native backing reused a preexisting mutable database")
        else:
            self.exact(database, ("exists", "device", "inode", "logical_bytes", "allocated_bytes", "links", "mode", "uid", "gid",
                "sha256", "hash_read_bytes", "hash_window_bytes", "stable_during_observation"), "original guest database observation")
            self.require(database["exists"] is True and all(self.integer(database[key]) for key in
                ("device", "inode", "logical_bytes", "allocated_bytes", "links", "mode", "uid", "gid", "hash_read_bytes", "hash_window_bytes"))
                and database["device"] == identity["device"] and database["links"] == 1 and stat.S_ISREG(database["mode"])
                and isinstance(database["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", database["sha256"])
                and database["hash_read_bytes"] == database["logical_bytes"] and database["hash_window_bytes"] == 65_536
                and database["stable_during_observation"] is True, "native backing original guest file/hash identity unavailable")
        return value, outer, inspected

    def mapping(self, descriptor, argv, command, root, identity, version=2, original_inputs=None):
        self.require(descriptor is not None, "Docker native backing mapping UNAVAILABLE")
        value = self.read(descriptor, root)
        fields = {"schema", "status", "scope", "source", "container_id", "image_id", "host_root", "container_root",
            "collector_argv", "host_argv", "artifacts", "backing", "pre_start", "post_exit", "logical_export"}
        if version == 3:
            fields.add("application_disposal")
        self.exact(value, fields, "external Docker native path mapping")
        self.require(version in (2, 3) and value["schema"] == f"cluster-two-e2-docker-path-mapping-v{version}" and value["status"] == "OBSERVED"
            and value["scope"] == "diagnostic-input-path-binding" and isinstance(value["source"], str) and value["source"]
            and value["image_id"] == command.get("image_id") and value["collector_argv"] == argv,
            "Docker native mapping version/image/original invocation differs")
        host_root = Path(value["host_root"])
        self.require(host_root.is_absolute() and host_root == host_root.resolve() == Path(root).resolve()
            and value["container_root"] == "/work", "native backing repository root differs")
        backing = value["backing"]
        volume_receipt = self.volume(backing, root, identity["receipt_id"])
        self.require(argv[5] == backing["database_path"], "native backing original database argument substituted")
        host = value["host_argv"]
        self.require(isinstance(host, list) and len(host) == (9 if version == 3 else 8) and host[5] is None and host[1] == argv[1],
                     "native backing guest database acquired a fabricated host path")
        for index in ((0, 2, 3, 4, 6, 7, 8) if version == 3 else (0, 2, 3, 4, 6, 7)):
            local = PurePosixPath(argv[index])
            self.require(local.is_absolute() and local.is_relative_to("/work") and ".." not in local.parts
                and isinstance(host[index], str) and Path(host[index]).is_absolute()
                and host_root / local.relative_to("/work") == Path(host[index]) == Path(host[index]).resolve(),
                "native backing actual bind input mapping differs")
        self.exact(value["artifacts"], ("command_receipt", "inspect_receipt", "inspect_stdout", "cid_file"),
                   "native backing original consumer artifacts")
        group = {**value["artifacts"], "container_id": value["container_id"]}
        outer, inspected = self.container(group, root, value["image_id"], backing, host_root, argv, True,
                                          command["external_argv"], exit_code=None, version=version)
        product = self.read_json(self.artifact(identity["artifacts"]["product"], root), 1 << 20)[0]
        sources = {self.artifact(item, root) for item in product["sources"]}
        pre, pre_command, pre_inspect = self.observation(value["pre_start"], root, "pre-start", backing, value["image_id"], host_root, sources, version)
        post, post_command, _ = self.observation(value["post_exit"], root, "post-exit", backing, value["image_id"], host_root, sources, version)
        self.require(len({value["container_id"], value["pre_start"]["container_id"], value["post_exit"]["container_id"]}) == 3,
                     "native backing consumer/preparer/observer container identities reused")
        self.follows(volume_receipt, pre_command)
        self.follows(pre_inspect, outer)
        self.follows(inspected, post_command)
        self.require(pre["root_identity"] == post["root_identity"] and pre["mount_identity"] == post["mount_identity"]
            and all(pre["filesystem"][key] == post["filesystem"][key] for key in ("type", "magic")),
            "native backing pre/post filesystem/root identity changed")
        # This vehicle deliberately has no exported database. An added copy
        # cannot acquire authority for the original guest's inode/allocation.
        self.require(value["logical_export"] is None, "native backing unregistered logical copy cannot replace original guest artifact")
        native = {"filesystem_magic": post["filesystem"]["magic"], "database_path": backing["database_path"],
                  "database": post["database"], "consumer_exit_code": outer["exit_code"]}
        if version == 3:
            self.original_inputs(original_inputs, identity, argv, host, root)
            native.update(application_disposal=value["application_disposal"], consumer_command=outer,
                pre_start_command=pre_command, image_id=value["image_id"], probe_source=value["pre_start"]["probe_source"])
            self.require(value["pre_start"]["probe_source"] == value["post_exit"]["probe_source"],
                         "v3 pre/post original probe source changed")
        return host, native

    def original_inputs(self, value, identity, argv, host, root):
        names = ("binary", "assignment", "acquisition", "base", "replacements", "identity", "control")
        self.exact(value, ("scope", *names), "v3 original pre-start inputs")
        self.require(value["scope"] == "original-pre-start-logical-input-bytes", "v3 original input read scope differs")
        bundle = self.read(identity["artifacts"]["fixture"], root)
        frozen = {"binary": identity["artifacts"]["binary"], "assignment": bundle["inputs"]["assignment"],
            "acquisition": bundle["inputs"]["acquisition"], "base": bundle["inputs"]["base_input"],
            "replacements": bundle["inputs"]["replacements"], "control": identity["control"]}
        locations = {"binary": (argv[0], host[0]), "assignment": (argv[2], host[2]),
            "acquisition": (str(PurePosixPath(argv[2]).with_suffix(".fixture")), str(Path(host[2]).with_suffix(".fixture"))),
            "base": (argv[3], host[3]), "replacements": (argv[4], host[4]), "identity": (argv[7], host[7]), "control": (argv[8], host[8])}
        for name in names:
            observed = self.exact(value[name], ("path", "bytes", "sha256"), "v3 original " + name + " input")
            guest, host_path = locations[name]
            self.require(observed["path"] == guest and self.integer(observed["bytes"]), "v3 original input path/length differs")
            if name == "identity":
                supplied, raw = self.read_json(host_path, 16_384)
                self.require(supplied == identity and observed["bytes"] == len(raw)
                    and observed["sha256"] == hashlib.sha256(raw).hexdigest(), "v3 original identity input bytes differ")
            else:
                path = self.artifact(frozen[name], root)
                self.require(path == Path(host_path) and observed["sha256"] == frozen[name]["sha256"]
                    and observed["bytes"] == path.stat().st_size
                    and ("bytes" not in frozen[name] or frozen[name]["bytes"] == observed["bytes"]),
                    "v3 original input bytes differ from independent pre-start seal: " + name)

    def original_artifact(self, state, native):
        self.require(state["path"] == native["database_path"] and state["status"] == "OBSERVED",
                     "native backing original guest database path/status substituted")
        for key in ("device", "inode", "logical_bytes", "allocated_bytes", "links"):
            self.require(self.integer(state[key]) and state[key] == native["database"][key],
                         "native backing same-guest original database " + key + " changed")

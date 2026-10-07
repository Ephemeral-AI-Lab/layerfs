"""Source-shaped retained v2 witnesses; tests never invoke a guest or Docker."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_jobs as jobs


class NativeFixture:
    """Independent synthetic native custody, also usable by full-window tests."""
    def __init__(self, root, argv=None, receipt_id="3" * 64, product_sources=()):
        self.root = Path(root).resolve()
        self.argv = argv or ["/work/binary", "endpoint:1234", "/work/assignment", "/work/base",
            "/work/replacements", "/state/overlay.sqlite", "/work/output", "/work/identity"]
        self.image = "sha256:" + "8" * 64
        self.owner = "e04-native-synthetic"
        self.name = "e04-volume-synthetic"
        self.receipt_id = receipt_id
        self.labels = {"layerfs.e04.receipt": receipt_id, "layerfs.e04.owner": self.owner}
        self.mountpoint = "/var/lib/docker/volumes/" + self.name + "/_data"
        self.probe = self.artifact("e04_native_backing_observation.py", b"# independent source-shaped observer fixture\n")
        product = self.artifact("native-product.json", {"sources": [*product_sources, self.probe]})
        self.identity = {"receipt_id": receipt_id, "artifacts": {"product": product}}
        self.absence = self.receipt(["docker", "volume", "inspect", self.name], 1, 1)
        self.created = self.receipt(["docker", "volume", "create", "--label", "layerfs.e04.receipt=" + receipt_id,
            "--label", "layerfs.e04.owner=" + self.owner, self.name], 3)
        self.volume_inspected = self.receipt(["docker", "volume", "inspect", self.name], 5)
        self.volume_info = [{"Name": self.name, "Driver": "local", "Mountpoint": self.mountpoint,
            "Labels": self.labels.copy(), "Scope": "local", "Options": None}]
        self.backing = {"kind": "docker-volume", "name": self.name, "driver": "local", "mountpoint": self.mountpoint,
            "database_path": "/state/overlay.sqlite", "container_root": "/state", "receipt_id": receipt_id,
            "owner_label": self.owner, "labels": self.labels.copy(), "retention": "RETAIN_ORIGINAL_VOLUME_ON_SUCCESS_OR_FAILURE"}
        self.groups = {}
        for role, cid, tick in (("pre-start", "6" * 64, 10), ("consumer", "7" * 64, 20), ("post-exit", "9" * 64, 30)):
            self.groups[role] = self.group(role, cid, tick)
        self.command = {"image_id": self.image, "external_argv": self.groups["consumer"]["outer"]["command"], "argv": self.argv}
        root_identity = {"device": 2049, "inode": 71, "mode": 0o40755, "uid": 0, "gid": 0}
        common = {"schema": "e04-native-backing-observation-v1", "receipt_id": receipt_id,
            "owner_label": self.owner, "volume_name": self.name, "root_path": "/state", "database_path": "/state/overlay.sqlite",
            "filesystem": {"type": "ext4", "magic": 0xEF53,
                "mountinfo": "123 90 8:1 /volumes/e04 /state ro,relatime - ext4 /dev/vda1 rw"},
            "root_identity": root_identity}
        self.pre = {**copy.deepcopy(common), "phase": "pre-start", "database": {"exists": False}}
        self.post = {**copy.deepcopy(common), "phase": "post-exit", "database": {"exists": True,
            "device": 2049, "inode": 91, "logical_bytes": 23, "allocated_bytes": 268_435_456,
            "links": 1, "mode": 0o100600, "uid": 0, "gid": 0, "sha256": "a" * 64,
            "hash_read_bytes": 23, "hash_window_bytes": 65_536, "stable_during_observation": True}}
        self.host = [None if index == 5 else (arg if index == 1 else str(self.root / Path(arg).relative_to("/work")))
                     for index, arg in enumerate(self.argv)]
        self.value = {"schema": "cluster-two-e2-docker-path-mapping-v2", "status": "OBSERVED",
            "scope": "diagnostic-input-path-binding", "source": "independent source-shaped native witness",
            "container_id": "7" * 64, "image_id": self.image, "host_root": str(self.root), "container_root": "/work",
            "collector_argv": self.argv, "host_argv": self.host, "backing": self.backing, "logical_export": None}

    def artifact(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = value if isinstance(value, bytes) else json.dumps(value).encode()
        path.write_bytes(raw)
        return {"path": name, "sha256": jobs.hash_bytes(raw)}

    @staticmethod
    def receipt(command, tick, exit_code=0):
        return {"command": command, "exit_code": exit_code, "timed_out": False,
            "started_monotonic_ns": tick, "finished_monotonic_ns": tick + 1, "wall_ns": 1}

    def group(self, role, cid, tick):
        consumer = role == "consumer"
        name = self.owner if consumer else self.owner + "-" + role
        argv = self.argv if consumer else ["python3", "-B", "/work/" + self.probe["path"], role,
            self.receipt_id, self.owner, self.name]
        labels = {**self.labels, "layerfs.e04.role": role}
        command = ["docker", "run", "--name", name, "--cidfile", str(self.root / (role + ".cid"))]
        for key, value in labels.items():
            command.extend(["--label", key + "=" + value])
        command.extend(["-e", "LAYERFS_CONSTRUCTION_WORKERS=1", "-v", str(self.root) + ":/work",
            "-v", self.name + ":/state" + ("" if consumer else ":ro"), "-w", "/work", self.image, *argv])
        info = {"Id": cid, "Image": self.image, "Name": "/" + name, "Path": argv[0], "Args": argv[1:],
            "State": {"Running": False, "ExitCode": 0}, "HostConfig": {"AutoRemove": False},
            "Config": {"Entrypoint": None, "Cmd": argv.copy(), "WorkingDir": "/work", "Env": ["LAYERFS_CONSTRUCTION_WORKERS=1"], "Labels": labels},
            "Mounts": [{"Type": "bind", "Source": str(self.root), "Destination": "/work", "RW": True},
                {"Type": "volume", "Name": self.name, "Driver": "local", "Source": self.mountpoint,
                 "Destination": "/state", "RW": consumer}]}
        return {"cid": cid, "name": name, "argv": argv, "outer": self.receipt(command, tick),
            "inspected": self.receipt(["docker", "inspect", cid], tick + 2), "info": [info]}

    def seal_group(self, role):
        group = self.groups[role]
        result = {"container_id": group["cid"], "container_name": group["name"],
            "command_receipt": self.artifact(role + ".json", group["outer"]),
            "inspect_receipt": self.artifact(role + "-inspect.json", group["inspected"]),
            "inspect_stdout": self.artifact(role + "-inspect.stdout", group["info"]),
            "cid_file": self.artifact(role + ".cid", (group["cid"] + "\n").encode())}
        if role != "consumer":
            result.update(observation_stdout=self.artifact(role + ".stdout", self.pre if role == "pre-start" else self.post),
                probe_source=self.probe, probe_argv=group["argv"])
        return result

    def seal(self):
        self.created["stdout"] = self.artifact("create.stdout", (self.name + "\n").encode())
        self.backing.update(volume_absence_receipt=self.artifact("absence.json", self.absence),
            volume_absence_stdout=self.artifact("absence.stdout", []),
            volume_absence_stderr=self.artifact("absence.stderr", f"Error response from daemon: get {self.name}: no such volume\n".encode()),
            volume_create_receipt=self.artifact("create.json", self.created),
            volume_inspect_receipt=self.artifact("volume-inspect.json", self.volume_inspected),
            volume_inspect_stdout=self.artifact("volume-inspect.stdout", self.volume_info))
        consumer = self.seal_group("consumer")
        self.value.update(artifacts={key: value for key, value in consumer.items() if key not in ("container_id", "container_name")},
            pre_start=self.seal_group("pre-start"), post_exit=self.seal_group("post-exit"))
        return self.artifact("mapping-v2.json", self.value)

    def checked(self):
        return jobs.native_backing().mapping(self.seal(), self.argv, self.command, self.root, self.identity)


class E04NativeBacking(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.fixture = NativeFixture(temp.name)

    def refused(self):
        with self.assertRaises(ValueError):
            self.fixture.checked()

    def test_complete_witness_maps_inputs_but_has_no_host_database(self):
        host, native = self.fixture.checked()
        self.assertEqual(host, self.fixture.host)
        self.assertIsNone(host[5])
        self.assertEqual(native["filesystem_magic"], 0xEF53)

    def test_observed_other_filesystem_is_not_inferred_ext4(self):
        for value in (self.fixture.pre, self.fixture.post):
            value["filesystem"].update(type="overlay", magic=0x794C7630,
                mountinfo="123 90 8:1 /volumes/e04 /state ro,relatime - overlay overlay rw")
        self.assertEqual(self.fixture.checked()[1]["filesystem_magic"], 0x794C7630)

    def test_v1_mapping_cannot_supply_v2_custody(self):
        self.fixture.value["schema"] = "cluster-two-e2-docker-path-mapping-v1"
        self.refused()

    def test_guest_database_has_no_fabricated_host_alias(self):
        self.fixture.host[5] = str(self.fixture.root / "copied.sqlite")
        self.refused()

    def test_original_database_argument_cannot_use_repository_bind(self):
        self.fixture.argv[5] = "/work/overlay.sqlite"
        self.refused()

    def test_changed_original_container_or_image_refused(self):
        for key in ("Id", "Image"):
            value = self.fixture.groups["consumer"]["info"][0]
            original = value[key]
            value[key] = "f" * 64
            self.refused()
            value[key] = original

    def test_wrong_volume_source_or_readonly_consumer_refused(self):
        volume = self.fixture.groups["consumer"]["info"][0]["Mounts"][1]
        for key, value in (("Name", "other"), ("Source", "/other"), ("RW", False), ("Type", "bind")):
            original = volume[key]
            volume[key] = value
            self.refused()
            volume[key] = original

    def test_input_or_state_submount_refused(self):
        mounts = self.fixture.groups["consumer"]["info"][0]["Mounts"]
        for path in ("/work/base", "/state/overlay.sqlite"):
            mounts.append({"Type": "bind", "Source": "/other", "Destination": path, "RW": True})
            self.refused()
            mounts.pop()

    def test_reused_database_refused(self):
        self.fixture.pre["database"] = {"exists": True}
        self.refused()

    def test_volume_absence_must_precede_create(self):
        self.fixture.absence.update(started_monotonic_ns=7, finished_monotonic_ns=8)
        self.refused()

    def test_wrong_volume_ownership_or_options_refused(self):
        volume = self.fixture.volume_info[0]
        for key, value in (("Labels", {}), ("Options", {"type": "nfs"}), ("Driver", "unregistered")):
            original = volume[key]
            volume[key] = value
            self.refused()
            volume[key] = original

    def test_original_preparer_and_post_observer_order_required(self):
        self.fixture.groups["post-exit"]["outer"].update(started_monotonic_ns=19, finished_monotonic_ns=20)
        self.refused()

    def test_mount_filesystem_and_device_cannot_be_guessed(self):
        self.fixture.post["filesystem"]["mountinfo"] = "123 90 9:1 /volumes/e04 /state ro - ext4 /dev/vda1 rw"
        self.refused()

    def test_changed_filesystem_or_root_identity_refused(self):
        self.fixture.post["filesystem"]["magic"] += 1
        self.refused()

    def test_same_volume_name_cannot_replace_original_root_inode(self):
        self.fixture.post["root_identity"]["inode"] += 1
        self.refused()

    def test_wrong_effective_process_and_environment_refused(self):
        config = self.fixture.groups["consumer"]["info"][0]["Config"]
        config["Entrypoint"] = ["/bin/sh", "-c"]
        self.refused()
        config["Entrypoint"] = None
        config["Env"] = ["LAYERFS_CONSTRUCTION_WORKERS=4"]
        self.refused()

    def test_original_container_cannot_auto_remove(self):
        self.fixture.groups["consumer"]["outer"]["command"].insert(2, "--rm")
        self.refused()

    def test_probe_source_hash_must_remain_sealed(self):
        (self.fixture.root / self.fixture.probe["path"]).write_bytes(b"substituted observer")
        self.refused()

    def test_failed_or_partial_post_hash_does_not_attest_original(self):
        self.fixture.post["database"]["stable_during_observation"] = False
        self.refused()

    def test_host_copy_cannot_acquire_original_guest_identity(self):
        self.fixture.value["logical_export"] = {"path": "copied.sqlite", "device": 2049, "inode": 91}
        self.refused()

    def test_same_guest_collector_stat_must_match_original_observer(self):
        _, native = self.fixture.checked()
        state = {"path": "/state/overlay.sqlite", "status": "OBSERVED", **{key: native["database"][key]
            for key in ("device", "inode", "logical_bytes", "allocated_bytes", "links")}}
        jobs.native_backing().original_artifact(state, native)
        for key in ("device", "inode", "logical_bytes", "allocated_bytes", "links"):
            changed = {**state, key: state[key] + 1}
            with self.assertRaisesRegex(ValueError, "same-guest"):
                jobs.native_backing().original_artifact(changed, native)


if __name__ == "__main__":
    unittest.main()

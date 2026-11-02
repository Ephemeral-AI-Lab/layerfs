"""The sealed Linux bundle builder (#533 O16): per-package steps, the source check
before sealing, bundle.json and placement. Docker and Cargo are stubbed."""

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import linux_bundle as bundle
from cleanup_state import Refused


class LinuxBundleTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.worktree = self.root / "checkout"
        self.worktree.mkdir()
        subprocess.run(["git", "init", "-q", str(self.worktree)], check=True)
        (self.worktree / "Cargo.toml").write_text('[workspace.package]\nversion = "9.8.7"\n')
        self.state = self.root / "state"
        (self.state / "leases").mkdir(parents=True)
        (self.state / "builds").mkdir()
        self.record = {
            "state": str(self.state),
            "worktree": str(self.worktree),
            "images": {"build": "sha256:b", "sandbox": "sha256:s"},
        }
        self.source = {"head": "abc", "sha256": "src1", "files": {}}
        self.commands = []
        patches = [
            patch.object(bundle, "inputs", lambda record: dict(self.source)),
            patch.object(bundle, "docker_build", lambda record, command, extra=(): ["docker-run", *extra, *command]),
            patch.object(bundle, "execute", self.execute),
            patch.object(bundle, "arch", lambda image: "aarch64"),
        ]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def execute(self, command, cwd, env=None, timeout=None, log=None):
        self.commands.append((command, timeout))
        if command[:1] == ["docker-run"] and "sh" in command:
            export = Path(command[command.index("--mount") + 1].split("src=")[1].split(",")[0])
            for name in bundle.BINARIES:
                (export / name).write_bytes(name.encode())
        if command[:2] == ["docker", "save"]:
            Path(command[3]).write_bytes(b"image")
            Path(command[3]).chmod(0o600)

    def test_each_package_is_its_own_bounded_step(self):
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)
        cargo = [c for c, _ in self.commands]
        self.assertEqual([c[c.index("-p") + 1] for c in cargo], ["layerfs-daemon", "layerfs-cli", "layerfs-sandbox"])
        self.assertIn("layerfs-daemon/postgresql", cargo[0])
        self.assertIn("--no-default-features", cargo[1])
        self.assertTrue(all(t == 120 for _, t in self.commands))
        self.assertTrue(all(c[c.index("-j") + 1] == "6" for c in cargo))
        steps = json.loads((self.state / "linux-bundle/debug.json").read_text())
        self.assertEqual(steps["worker"]["source"], {"head": "abc", "sha256": "src1"})

    def test_quiet_in_the_main_checkout_means_two_jobs(self):
        (self.worktree / "QUIET").write_text("")
        self.assertEqual(bundle.jobs(self.worktree), 2)
        (self.worktree / "QUIET").unlink()
        self.assertEqual(bundle.jobs(self.worktree), 6)

    def test_seal_refuses_missing_or_stale_steps(self):
        dest = self.root / "cache"
        bundle.build(self.record, "daemon", "debug", 120)
        with self.assertRaisesRegex(Refused, "worker, helper"):
            bundle.seal(self.record, "debug", None, dest, 120)
        bundle.build(self.record, "worker", "debug", 120)
        bundle.build(self.record, "helper", "debug", 120)
        self.source = {"head": "abc", "sha256": "src2", "files": {}}
        with self.assertRaisesRegex(Refused, "daemon, worker, helper"):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.assertFalse(dest.exists() and any(dest.rglob("*")), "nothing placed")

    def test_seal_places_the_bundle_and_replaces_an_old_one(self):
        dest = self.root / "cache"
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)
        old = dest / "9.8.7/linux-aarch64"
        old.mkdir(parents=True)
        (old / "stale").write_text("x")
        final = bundle.seal(self.record, "debug", None, dest, 120)
        self.assertEqual(final, old)
        names = sorted(p.name for p in final.iterdir())
        self.assertEqual(names, sorted([*bundle.BINARIES, bundle.ARCHIVE, "bundle.json"]))
        manifest = json.loads((final / "bundle.json").read_text())
        files = {n: hashlib.sha256(n.encode()).hexdigest() for n in bundle.BINARIES}
        files[bundle.ARCHIVE] = hashlib.sha256(b"image").hexdigest()
        self.assertEqual(manifest["files"], files)
        self.assertEqual(manifest["hash"], hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest())
        self.assertEqual((manifest["image"], manifest["platform"]), ("sha256:s", "linux-aarch64"))
        self.assertEqual(sorted(p.name for p in final.parent.iterdir()), ["linux-aarch64"], "no partial or old left")
        self.assertEqual((final / bundle.ARCHIVE).stat().st_mode & 0o777, 0o644)
        save = [c for c, _ in self.commands if c[:2] == ["docker", "save"]]
        self.assertEqual(save[0][-1], "sha256:s")

    def test_a_failed_seal_leaves_no_partial_folder(self):
        dest = self.root / "cache"
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)

        def failing(command, cwd, env=None, timeout=None, log=None):
            if command[:2] == ["docker", "save"]:
                raise Refused("Command exited 1")
            self.execute(command, cwd, env, timeout, log)

        with patch.object(bundle, "execute", failing), self.assertRaises(Refused):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.assertEqual(list((dest / "9.8.7").iterdir()), [])

    def test_a_step_s_time_limit_is_bounded(self):
        with patch.object(bundle, "load", lambda path: self.record):
            self.assertEqual(bundle.main(["daemon", "--resources", "r", "--timeout", "301"]), 2)
            self.assertEqual(bundle.main(["daemon", "--resources", "r", "--timeout", "0"]), 2)
        self.assertEqual(self.commands, [])


if __name__ == "__main__":
    unittest.main()

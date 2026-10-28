"""Isolated resource, artifact and cancellation tests; no product Store sessions."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from cleanup_state import Refused, git
import worktree_build as builds
import worktree_resources as resources
import worktree_run as runs


class WorktreeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.repo = self.root / "checkout"
        self.repo.mkdir()
        for folder in (
            "core/crates/example/src",
            "engine/vendor",
            "engine/crates/example/src",
            "engine/crates/layerfs-api/src",
            ".cargo",
            "api/agent/src",
            "api/agent/test",
            "api/backend/src",
            "api/runtime/src",
            "engine/crates/layerfs-node/src",
        ):
            (self.repo / folder).mkdir(parents=True)
        for name in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml", "api/package.json"):
            (self.repo / name).write_text("{}\n")
        (self.repo / "api/agent/src/index.js").write_text("export const value = 1;\n")
        (self.repo / "api/agent/package.json").write_text(
            '{"name": "@layerfs/agent", "exports": {".": "./src/index.js", "./package.json": "./package.json"}}\n'
        )
        for name in ("backend", "runtime"):
            (self.repo / f"api/{name}/src/index.js").write_text('export * from "@layerfs/agent";\n')
            (self.repo / f"api/{name}/package.json").write_text(
                f'{{"name": "@layerfs/{name}", "exports": {{".": "./src/index.js"}}}}\n'
            )
        (self.repo / "api/cli/src").mkdir(parents=True)
        (self.repo / "api/cli/src/layerfs.js").write_text('import "@layerfs/agent";\n')
        (self.repo / "api/cli/package.json").write_text('{"name": "@layerfs/cli"}\n')
        git(self.repo, "init", "-q")
        git(self.repo, "config", "user.name", "Fixture")
        git(self.repo, "config", "user.email", "fixture@example.invalid")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-qm", "fixture")
        args = SimpleNamespace(
            worktree=str(self.repo),
            root=str(self.root / "state"),
            owner="fixture",
            build_image="build",
            runtime_image="runtime",
            sandbox_image="sandbox",
        )
        with patch.object(resources, "image_id", side_effect=lambda image: "sha256:" + image):
            with contextlib.redirect_stdout(io.StringIO()):
                resources.register(args)
        self.path = self.root / "state" / resources.identity(self.repo) / "resources.json"
        self.record = resources.load(self.path)

    def test_build_identity_covers_each_product_layer(self):
        before = resources.inputs(self.record)
        for relative in (
            "core/crates/example/src/lib.rs",
            "engine/crates/example/src/lib.rs",
            "engine/crates/layerfs-api/src/lib.rs",
        ):
            path = self.repo / relative
            path.write_text("pub fn layer() {}\n")
            after = resources.inputs(self.record)
            self.assertIn(relative, after["files"])
            self.assertNotEqual(before["sha256"], after["sha256"])
            before = after

    def test_checkout_path_controls_identity_not_basename(self):
        self.assertNotEqual(resources.identity(self.root / "a/checkout"), resources.identity(self.root / "b/checkout"))
        self.assertEqual(resources.identity(self.repo), resources.identity(self.repo / "."))

    def test_lease_refuses_same_worktree_without_wait(self):
        with resources.lease(self.record, "linux"):
            with self.assertRaises(Refused):
                with resources.lease(self.record, "linux"):
                    pass
            with resources.lease(self.record, "macos"):
                pass

    def test_linux_mounts_do_not_use_host_target_or_shared_home(self):
        with patch.object(resources, "volume"), patch.dict(os.environ, {}, clear=True):
            command = resources.docker_build(self.record, ["cargo", "+1.85.1", "build"])
        self.assertIn("type=bind,src=" + str(self.repo) + ",dst=/work,readonly", command)
        self.assertIn("CARGO_TARGET_DIR=/work/target", command)
        self.assertIn("CARGO_HOME=/cargo-home", command)
        self.assertIn(
            "type=volume,src=" + self.record["linux_target_volume"] + ",dst=/work/target,volume-nocopy", command
        )
        self.assertIn("sha256:build", command)

    def test_explicit_flags_cannot_drop_repository_profile(self):
        with patch.dict(os.environ, {"RUSTFLAGS": "-C opt-level=3"}):
            with self.assertRaises(Refused):
                resources.environment(self.record, "linux")

    def test_container_failure_cannot_be_hidden_by_attach_exit_zero(self):
        calls = []

        def run(cmd, *args, **kwargs):
            calls.append(cmd)
            content = (
                b"id\n" if cmd[1] == "create" else json.dumps([{"State": {"Running": False, "ExitCode": 7}}]).encode()
            )
            return SimpleNamespace(stdout=content)

        with patch.object(resources, "run", side_effect=run), patch.object(resources.subprocess, "Popen") as popen:
            popen.return_value.wait.return_value = 0
            with self.assertRaises(Refused):
                resources.execute(
                    ["docker", "run", "--rm", "image", "false"], self.root, log=self.root / "container.log"
                )
        self.assertFalse(any(cmd[1] == "rm" for cmd in calls))

    def test_cargo_scope_and_frozen_harness_guards(self):
        command = builds.cargo_arguments(["--", "test", "-p", "layerfs-storage", "--no-run"])
        self.assertIn("--offline", command)
        self.assertIn("--locked", command)
        for args in (
            ["build", "--workspace"],
            ["build", "-p", "fs-bench-pro"],
            ["build", "-p", "layerfs-node", "--target-dir=/shared"],
            ["test"],
        ):
            with self.assertRaises(Refused):
                builds.cargo_arguments(args)

    def test_source_identity_includes_new_source_but_excludes_target(self):
        before = resources.inputs(self.record)
        target = self.repo / "target"
        target.mkdir()
        (target / "generated").write_text("cache")
        self.assertEqual(before, resources.inputs(self.record))
        (self.repo / "core/crates/example/src/new.rs").write_text("pub fn new() {}")
        self.assertNotEqual(before["sha256"], resources.inputs(self.record)["sha256"])

    def test_source_directory_symlink_refused(self):
        (self.repo / "core/crates/link").symlink_to(self.repo / "api", target_is_directory=True)
        with self.assertRaises(Refused):
            resources.inputs(self.record)

    def build_bundle(self, mutate=False):
        target = self.repo / "target/debug"
        target.mkdir(parents=True)
        (target / "layerfs-daemon").write_bytes(b"daemon")
        (target / "layerfs-daemon").chmod(0o755)
        (target / "liblayerfs_node.dylib").write_bytes(b"addon")
        args = SimpleNamespace(resources=str(self.path), platform="macos", profile="debug", jobs=2)

        def execute(*a, **kw):
            if mutate:
                (self.repo / "api/agent/src/index.js").write_text("changed\n")

        with patch.object(builds, "execute", side_effect=execute), patch.object(builds, "environment", return_value={}):
            with contextlib.redirect_stdout(io.StringIO()):
                builds.build(args)
        return next((Path(self.record["state"]) / "artifacts").iterdir())

    def test_bundle_pins_sdk_and_binary_and_detects_mutation(self):
        bundle = self.build_bundle()
        manifest = builds.verify_bundle(bundle)
        # the bundle mirrors the repository layout under api/
        for name in (
            "api/package.json",
            "api/agent/src/index.js",
            "api/backend/src/index.js",
            "api/runtime/src/index.js",
        ):
            self.assertIn(name, manifest["files"])
        self.assertFalse(any(name.startswith("engine/crates/layerfs-node/") for name in manifest["files"]))
        # each package resolves by name through a shim, not a symlink
        self.assertEqual(
            (bundle / "api/node_modules/@layerfs/agent/index.js").read_text(),
            'export * from "../../../agent/src/index.js";\n',
        )
        self.assertEqual(
            json.loads((bundle / "api/node_modules/@layerfs/agent/package.json").read_text())["exports"],
            {".": "./index.js"},
        )
        self.assertEqual(
            (bundle / "api/node_modules/@layerfs/backend/index.js").read_text(),
            'export * from "../../../backend/src/index.js";\n',
        )
        # the layerfs command, which reaches the agent through its own shim
        self.assertIn("api/cli/src/layerfs.js", manifest["files"])
        self.assertEqual(
            (bundle / "api/cli/node_modules/@layerfs/agent/index.js").read_text(),
            'export * from "../../../../agent/src/index.js";\n',
        )
        (self.repo / "api/agent/src/index.js").write_text("later edit\n")
        self.assertEqual((bundle / "api/agent/src/index.js").read_text(), "export const value = 1;\n")
        file = bundle / "layerfs.node"
        file.chmod(0o600)
        file.write_bytes(b"corrupt")
        with self.assertRaises(Refused):
            builds.verify_bundle(bundle)

    def test_changed_build_source_cannot_publish_sealed_bundle(self):
        with self.assertRaises(Refused):
            self.build_bundle(mutate=True)
        self.assertFalse(any((Path(self.record["state"]) / "artifacts").glob("*/manifest.json")))

    def test_teardown_never_removes_other_names(self):
        record = {"prefix": "owned-unique-", "client_id": "id", "store_volume": "owned-volume"}
        commands = []

        def bounded(command, *args, **kwargs):
            commands.append(command)
            return b"id\n" if command[1] == "ps" else json.dumps([{"Id": "id", "Name": "/other-owner"}]).encode()

        with patch.object(runs, "bounded", side_effect=bounded):
            with self.assertRaises(Refused):
                runs.finish(record, True, time.monotonic() + 20)
        self.assertFalse(any(c[1] in {"rm", "kill", "volume"} for c in commands))

    def test_docker_short_and_full_ids_do_not_duplicate_teardown(self):
        full = "a" * 64

        def bounded(cmd, *args):
            if cmd[1] == "ps":
                return ((full if "--no-trunc" in cmd else full[:12]) + "\n").encode()
            records = [{"Id": full, "Name": "/mine-client"} for _ in cmd[2:]]
            return json.dumps(records).encode()

        with patch.object(runs, "bounded", side_effect=bounded):
            result = runs.inspect_owned("mine-", full, time.monotonic() + 20)
        self.assertEqual(len(result), 1)

    def test_failed_run_stops_own_container_but_retains_state(self):
        record = {"prefix": "mine-", "client_id": "id", "store_volume": "mine-store"}
        calls = []
        containers = [{"Id": "id", "Name": "/mine-client", "State": {"Running": True}}]
        with (
            patch.object(runs, "inspect_owned", return_value=containers),
            patch.object(runs, "bounded", side_effect=lambda cmd, *a: calls.append(cmd)),
        ):
            runs.finish(record, False, time.monotonic() + 20)
        self.assertEqual(calls, [["docker", "kill", "id"]])
        self.assertFalse(record["released"])

    def test_successful_run_removes_only_acknowledged_resources(self):
        record = {"prefix": "mine-", "client_id": "id", "store_volume": "mine-store"}
        calls = []
        containers = [{"Id": "id", "Name": "/mine-client", "State": {"Running": False}}]

        def bounded(cmd, *a):
            calls.append(cmd)
            return b""

        with (
            patch.object(runs, "inspect_owned", return_value=containers),
            patch.object(runs, "bounded", side_effect=bounded),
            patch.object(runs, "check_store"),
        ):
            runs.finish(record, True, time.monotonic() + 20)
        self.assertEqual(calls[0], ["docker", "rm", "id"])
        self.assertEqual(calls[-1], ["docker", "volume", "rm", "mine-store"])
        self.assertTrue(record["released"])

    def test_recreated_or_foreign_store_is_preserved(self):
        state = {
            "prefix": "mine-token-",
            "run_id": "token",
            "worktree_id": "worktree",
            "store_volume": "store",
            "volume_created_at": "old",
        }
        for info in [
            {"Labels": {}},
            {"Labels": {resources.LABEL: "worktree", "layerfs.dev.run": "token"}, "CreatedAt": "new"},
        ]:
            with patch.object(runs, "bounded", return_value=json.dumps([info]).encode()):
                with self.assertRaises(Refused):
                    runs.check_store(state, time.monotonic() + 20)

    def test_host_timeout_cancels_owned_process_group(self):
        pidfile = self.root / "child.pid"
        script = (
            'import subprocess,time,pathlib; p=subprocess.Popen(["'
            + sys.executable
            + '","-c","import time; time.sleep(60)"]); '
            "pathlib.Path(" + repr(str(pidfile)) + ").write_text(str(p.pid)); time.sleep(60)"
        )
        with self.assertRaises(subprocess.TimeoutExpired):
            resources.execute([sys.executable, "-c", script], self.root, timeout=0.5, log=self.root / "timeout.log")
        child = int(pidfile.read_text())
        # A killed child can briefly be a zombie; it must not remain executing.
        status = subprocess.run(["ps", "-o", "stat=", "-p", str(child)], capture_output=True, text=True).stdout.strip()
        self.assertTrue(not status or status.startswith("Z"), status)

    def test_disposed_failure_is_not_replayed(self):
        folder = Path(self.record["state"]) / "runs/fixture"
        folder.mkdir()
        receipt = folder / "receipt.json"
        resources.write(receipt, {"worktree_id": self.record["id"], "status": "failed", "released": False})
        resources.write(folder / "disposal-closed.json", {"released": True})
        args = SimpleNamespace(resources=str(self.path), run_receipt=str(receipt), release_failed=True, reason="closed")
        with patch.object(runs, "finish") as finish, contextlib.redirect_stdout(io.StringIO()):
            runs.teardown(args)
        finish.assert_not_called()

    def test_cleanup_scope_protects_pins_and_live_failures(self):
        folder = Path(self.record["state"]) / "runs/fixture"
        folder.mkdir()
        resources.write(folder / "receipt.json", {"released": False, "store_volume": "failed-store"})
        args = SimpleNamespace(resources=str(self.path), release_caches=False, out=str(self.root / "scope.json"))
        with contextlib.redirect_stdout(io.StringIO()):
            runs.cleanup_scope(args)
        scope = json.loads(Path(args.out).read_text())
        self.assertEqual(scope["actions"], [])
        self.assertIn("failed-store", scope["protected_volumes"])
        self.assertIn(self.record["state"], scope["protected_paths"])


if __name__ == "__main__":
    unittest.main()

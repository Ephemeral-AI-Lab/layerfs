"""Destructive cleanup regressions, confined to newly created temporary fixtures."""

import contextlib
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import cleanup
from cleanup_discover import discover
from cleanup_docker import guard as guard_container
from cleanup_retirement import retirement_open_files
from cleanup_archive import archive
from cleanup_state import Refused, ensure_idle, ensure_volume_idle, git, inventory, same_volume


class CleanupTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "Cleanup Fixture")
        (self.repo / "tracked").write_text("original\n")
        self.git("add", "tracked")
        self.git("commit", "-qm", "fixture")
        self.output = self.root / "pass"
        self.output.mkdir()

    def git(self, *args):
        return git(self.repo, *args)

    def action(self, path, kind="archive-tree"):
        return {
            "kind": kind,
            "path": str(path),
            "owner": "fixture",
            "reason": "completed disposable fixture",
            "released": True,
        }

    def apply(self, action, protected=None):
        path = Path(action["path"])
        action["snapshot"] = inventory(path, action["kind"] == "worktree")[1]
        if action["kind"] == "worktree":
            action["git_head"] = git(path, "rev-parse", "HEAD").decode().strip()
            action["staged_sha256"] = hashlib.sha256(
                git(path, "diff", "--cached", "--binary", "--full-index", "--no-ext-diff", "--no-textconv")
            ).hexdigest()
        plan = {"pass_id": "fixture", "scope": {"protected_paths": protected or []}}
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            return cleanup.apply_action(self.repo, self.output, plan, action)

    def test_raw_archive_roundtrip_including_symlink_and_executable(self):
        source = self.root / "copy"
        source.mkdir()
        (source / "file with space").write_bytes(b"a\r\nb\0c")
        (source / "run").write_bytes(b"#!/bin/sh\n")
        (source / "run").chmod(0o755)
        (source / "link").symlink_to("file with space")
        (source / "empty").mkdir()
        result = self.apply(self.action(source))
        self.assertFalse(source.exists())
        manifest = json.loads(gzip.open(result["manifest"], "rb").read())
        self.assertIn("empty", [r["path"] for r in manifest["rows"]])
        self.assertEqual(self.git("show", result["tree"] + ":file with space"), b"a\r\nb\0c")
        self.assertEqual(self.git("show", result["tree"] + ":link"), b"file with space")
        self.assertIn(b"100755 blob", self.git("ls-tree", result["tree"], "run"))

    def test_single_log_archived_before_removal(self):
        log = self.root / "closed.log"
        log.write_bytes(b"retained failed outcome\n")
        result = self.apply(self.action(log, "archive-file"))
        self.assertEqual(self.git("show", result["tree"] + ":closed.log"), b"retained failed outcome\n")
        self.assertFalse(log.exists())

    def test_protected_descendant_and_container_parent_bind_refused(self):
        source = self.root / "copy"
        source.mkdir()
        pin = source / "pinned"
        pin.write_text("proof")
        with self.assertRaises(Refused):
            ensure_idle([source], [pin], [], [])
        container = {"name": "active", "mounts": [{"Type": "bind", "Source": str(self.root)}]}
        with self.assertRaises(Refused):
            ensure_idle([source], [], [container], [])
        with self.assertRaises(Refused):
            ensure_idle([source], [], [], [(os.getpid() + 10000, str(pin))])
        self.assertTrue(pin.exists())

    def test_symlink_root_and_nested_git_refused(self):
        target = self.root / "real"
        target.mkdir()
        link = self.root / "alias"
        link.symlink_to(target, target_is_directory=True)
        with self.assertRaises(Refused):
            ensure_idle([link], [], [], [])
        (target / ".git").mkdir()
        with self.assertRaises(Refused):
            inventory(target)

    def test_changed_plan_preserves_original(self):
        source = self.root / "copy"
        source.mkdir()
        file = source / "file"
        file.write_text("before")
        action = self.action(source)
        action["snapshot"] = inventory(source)[1]
        file.write_text("after")
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            with self.assertRaises(Refused):
                cleanup.apply_action(self.repo, self.output, {"scope": {}, "pass_id": "fixture"}, action)
        self.assertEqual(file.read_text(), "after")

    def test_archive_verification_failure_never_deletes(self):
        source = self.root / "copy"
        source.mkdir()
        (source / "file").write_text("keep")
        with patch.object(cleanup, "archive", side_effect=Refused("verification failed")):
            with self.assertRaises(Refused):
                self.apply(self.action(source))
        self.assertEqual((source / "file").read_text(), "keep")

    def test_cargo_removal_preserves_direct_binary_and_helpers(self):
        target = self.repo / "target"
        profile = target / "debug"
        cache = profile / "deps"
        cache.mkdir(parents=True)
        (cache / "generated").write_text("cache")
        (profile / "daemon").write_text("pinned binary")
        (profile / "helper.py").write_text("helper")
        (target / ".rustc_info.json").write_text("{}")
        action = self.action(cache, "cargo-cache")
        action.update(guard_path=str(profile), target_root=str(target))
        self.apply(action)
        self.assertFalse(cache.exists())
        self.assertEqual((profile / "daemon").read_text(), "pinned binary")
        self.assertTrue((profile / "helper.py").exists())

    def test_dirty_worktree_preserves_staged_and_working_versions(self):
        tree = self.root / "linked"
        self.git("worktree", "add", "--detach", str(tree), "HEAD")
        file = tree / "tracked"
        file.write_text("staged\n")
        git(tree, "add", "tracked")
        file.write_text("working\n")
        (tree / "ignored-helper").write_text("helper")
        result = self.apply(self.action(tree, "worktree"))
        self.assertFalse(tree.exists())
        self.assertEqual(self.git("show", result["tree"] + ":tracked"), b"working\n")
        self.assertEqual(self.git("show", result["tree"] + ":ignored-helper"), b"helper")
        with gzip.open(result["patches"]["staged"]["path"], "rb") as stream:
            self.assertIn(b"+staged", stream.read())
        self.assertEqual(self.git("rev-parse", result["head_ref"]).decode().strip(), result["head"])

    def test_volume_consumer_even_stopped_is_protected(self):
        stopped = {"name": "stopped", "running": False, "mounts": [{"Type": "volume", "Name": "cache"}]}
        with self.assertRaises(Refused):
            ensure_volume_idle("cache", [], [stopped])
        with self.assertRaises(Refused):
            ensure_volume_idle("cache", ["cache"], [])
        self.assertFalse(same_volume({"Name": "cache", "CreatedAt": "old"}, {"Name": "cache", "CreatedAt": "new"}))

    def test_apply_is_single_attempt_and_stops_after_failure(self):
        plan = {
            "version": 1,
            "pass_directory": str(self.output),
            "repository": str(self.repo),
            "scope": {},
            "actions": [{"kind": "archive-file"}, {"kind": "archive-tree"}],
            "protected_presence": {},
            "blocked": [],
        }
        path = self.output / "plan.json"
        path.write_text(json.dumps(plan))
        args = type("Args", (), {"plan": str(path)})()
        with patch.object(cleanup, "apply_action", side_effect=Refused("busy")) as operation:
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(cleanup.apply_plan(args), 1)
            self.assertEqual(operation.call_count, 1)
        report = json.loads((self.output / "report.json").read_text())
        self.assertEqual(len(report["not_attempted"]), 1)
        with self.assertRaises(FileExistsError):
            cleanup.apply_plan(args)

    def test_default_plan_has_no_actions(self):
        args = type(
            "Args", (), {"repo": str(self.repo), "scope": None, "reclaim_gb": 0, "out": str(self.root / "plan-only")}
        )()
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(cleanup.make_plan(args), 0)
        plan = json.loads((Path(args.out) / "plan.json").read_text())
        self.assertEqual(plan["actions"], [])
        self.assertTrue((self.repo / "tracked").exists())

    def test_low_disk_threshold_uses_decimal_gb(self):
        args = type("Args", (), {"repo": str(self.repo), "min_free_gb": 100, "reclaim_gb": 300})()
        for available, expected in [(99_999_999_999, 2), (100_000_000_000, 0)]:
            with patch.object(cleanup, "free_space", return_value={"free_bytes": available}):
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(cleanup.check_space(args), expected)
        self.assertEqual(cleanup.byte_target(300), 300_000_000_000)

    def test_recovery_shortfall_returns_two_and_reports_actual_delta(self):
        plan = {
            "version": 1,
            "pass_directory": str(self.output),
            "repository": str(self.repo),
            "scope": {},
            "actions": [],
            "reclaim_target_bytes": 300_000_000_000,
            "protected_presence": {},
            "blocked": [],
        }
        path = self.output / "plan.json"
        path.write_text(json.dumps(plan))
        args = type("Args", (), {"plan": str(path)})()
        with patch.object(cleanup, "free_space", side_effect=[{"free_bytes": 10}, {"free_bytes": 20}]):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(cleanup.apply_plan(args), 2)
        report = json.loads((self.output / "report.json").read_text())
        self.assertEqual(report["net_free_bytes_change"], 10)
        self.assertFalse(report["reclaim_target_met"])

    def test_changed_volume_identity_refused_before_remove(self):
        action = {
            "kind": "docker-volume",
            "name": "exact-fixture",
            "released": True,
            "owner": "fixture",
            "reason": "closed",
            "identity": {"CreatedAt": "old"},
        }
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "volume_identity", return_value={"CreatedAt": "new"}),
        ):
            with patch.object(cleanup, "run") as operation:
                with self.assertRaises(Refused):
                    cleanup.apply_action(self.repo, self.output, {"scope": {}}, action)
                operation.assert_not_called()

    def test_no_space_target_does_not_fail_for_concurrent_writes(self):
        plan = {
            "version": 1,
            "pass_directory": str(self.output),
            "repository": str(self.repo),
            "scope": {},
            "actions": [],
            "reclaim_target_bytes": 0,
            "protected_presence": {},
            "blocked": [],
        }
        path = self.output / "plan.json"
        path.write_text(json.dumps(plan))
        args = type("Args", (), {"plan": str(path)})()
        with patch.object(cleanup, "free_space", side_effect=[{"free_bytes": 20}, {"free_bytes": 10}]):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(cleanup.apply_plan(args), 0)

    def test_authorized_discovery_selects_idle_dirty_worktree_but_protects_active(self):
        idle = self.root / "idle"
        active = self.root / "active"
        self.git("worktree", "add", "--detach", str(idle), "HEAD")
        self.git("worktree", "add", "--detach", str(active), "HEAD")
        (idle / "uncommitted").write_text("recover me")
        registered = {str(p): {"worktree": str(p)} for p in (self.repo, idle, active)}
        scope, skipped = discover(
            self.repo, {}, registered, [], [(os.getpid() + 10000, str(active / "tracked"))], "user cleanup"
        )
        self.assertEqual([a["path"] for a in scope["actions"]], [str(idle)])
        self.assertEqual(skipped[0]["path"], str(active))
        with self.assertRaises(Refused):
            discover(self.repo, {}, registered, [], [], "")

    def test_discovery_keeps_unrelated_and_running_containers(self):
        def container(name, running, mounts):
            return {
                "id": name,
                "name": "/" + name,
                "running": running,
                "status": "running" if running else "exited",
                "mounts": mounts,
            }

        store = [{"Type": "volume", "Destination": "/layerfs-store"}]
        containers = [
            container("closed", False, store),
            container("live", True, store),
            container("unrelated", False, []),
        ]
        scope, _ = discover(self.repo, {}, {}, containers, [], "authorized")
        self.assertEqual([a["name"] for a in scope["actions"]], ["closed"])

    def test_running_container_retirement_refuses_exec_descendant(self):
        info = {"Id": "exact", "Name": "/daemon", "Mounts": [], "State": {"Running": True}}
        with self.assertRaises(Refused):
            guard_container(info, {}, {})
        with patch("cleanup_docker.run") as operation:
            operation.return_value.stdout = b"PID COMMAND\n42 layerfs-daemon --config setup\n43 sh active-task\n"
            with self.assertRaises(Refused):
                guard_container(info, {"retire_running": True}, {})

    def test_daemon_inspection_includes_docker_required_pid_column(self):
        info = {"Id": "exact", "Name": "/daemon", "Mounts": [], "State": {"Running": True}}
        with patch("cleanup_docker.run") as operation:
            operation.return_value.stdout = b"PID COMMAND\n42 /usr/local/bin/layerfs-daemon --config setup\n"
            guard_container(info, {"retire_running": True}, {})
            operation.assert_called_once_with(["docker", "top", "exact", "-eo", "pid,args"])

    def test_container_using_pinned_volume_cannot_be_retired(self):
        info = {
            "Id": "exact",
            "Name": "/daemon",
            "Mounts": [{"Name": "pinned"}],
            "State": {"Running": False, "Status": "exited"},
        }
        with self.assertRaises(Refused):
            guard_container(info, {}, {"protected_volumes": ["pinned"]})

    def test_caches_discovery_keeps_helpers_and_active_profile(self):
        tree = self.root / "linked"
        target = tree / "target/linux"
        profile = target / "debug"
        (profile / "deps").mkdir(parents=True)
        (target / ".rustc_info.json").write_text("{}")
        (profile / "helper").write_text("preserve")
        registered = {str(tree): {"worktree": str(tree)}}
        scope, _ = discover(self.repo, {}, registered, [], [], "cleanup", caches_only=True)
        self.assertEqual(scope["actions"][0]["path"], str(profile))
        scope, skipped = discover(
            self.repo,
            {},
            registered,
            [],
            [(os.getpid() + 10000, str(profile / "deps/file"))],
            "cleanup",
            caches_only=True,
        )
        self.assertEqual(scope["actions"], [])
        self.assertEqual(skipped[0]["kind"], "cargo-cache")
        self.assertEqual((profile / "helper").read_text(), "preserve")

    def test_cargo_hardlinked_siblings_are_checked_before_either_is_removed(self):
        target = self.repo / "target"
        profile = target / "debug"
        first, second = profile / "incremental", profile / "deps"
        first.mkdir(parents=True)
        second.mkdir()
        (target / ".rustc_info.json").write_text("{}")
        (first / "object.o").write_bytes(b"compiler output")
        os.link(first / "object.o", second / "object.o")
        (profile / "helper").write_bytes(b"keep")
        action = self.action(profile, "cargo-cache")
        action.update(
            target_root=str(target),
            guard_path=str(profile),
            cache_snapshots={str(p): inventory(p)[1] for p in (first, second)},
        )
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            cleanup.apply_action(self.repo, self.output, {"scope": {}}, action)
        self.assertFalse(first.exists())
        self.assertFalse(second.exists())
        self.assertEqual((profile / "helper").read_bytes(), b"keep")

    def retired_cargo_fixture(self):
        target = self.repo / "target"
        profile = target / "debug"
        cache = profile / "deps"
        cache.mkdir(parents=True)
        (cache / "object.o").write_bytes(b"generated")
        (profile / "daemon").write_bytes(b"pinned")
        (target / ".rustc_info.json").write_text("{}")
        action = self.action(profile, "cargo-cache")
        action.update(
            target_root=str(target),
            guard_path=str(profile),
            retire_idle_sessions=True,
            cache_snapshots={str(cache): inventory(cache)[1]},
        )
        return action, profile, cache

    def test_retired_cargo_profile_filters_cached_vm_handles_at_plan_and_apply(self):
        action, profile, cache = self.retired_cargo_fixture()
        scope = self.root / "scope.json"
        scope.write_text(json.dumps({"version": 1, "actions": [action]}))
        self.output.rmdir()
        args = type(
            "Args", (), {"repo": str(self.repo), "scope": str(scope), "out": str(self.output), "reclaim_gb": 0}
        )()
        disk = self.root / "Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw"
        opened = [(4242, str(cache / "object.o")), (4242, str(disk))]
        with (
            patch("cleanup_retirement.Path.home", return_value=self.root),
            patch("cleanup_retirement.run") as process,
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=opened),
        ):
            process.return_value.stdout = b"4242 1 S /System/com.apple.Virtualization.VirtualMachine\n"
            with contextlib.redirect_stdout(io.StringIO()):
                cleanup.make_plan(args)
            plan = json.loads((self.output / "plan.json").read_text())
            self.assertEqual(plan["blocked"], [])
            self.assertEqual(len(plan["actions"]), 1)
            cleanup.apply_action(self.repo, self.output, plan, plan["actions"][0])
        self.assertFalse(cache.exists())
        self.assertEqual((profile / "daemon").read_bytes(), b"pinned")

    def test_retired_cargo_cache_keeps_live_consumers_and_requires_opt_in(self):
        action, profile, cache = self.retired_cargo_fixture()
        disk = self.root / "Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw"
        vm = [(4242, str(cache / "object.o")), (4242, str(disk))]
        ordinary = [(4343, str(cache / "object.o"))]
        bound = {"name": "live", "mounts": [{"Type": "bind", "Source": str(self.repo)}]}
        cases = [
            (False, vm, [[], []]),
            (True, ordinary, [[], []]),
            (True, vm, [[bound], [bound]]),
            (True, vm, [[], [bound]]),
        ]
        with patch("cleanup_retirement.Path.home", return_value=self.root), patch("cleanup_retirement.run") as process:
            process.return_value.stdout = (
                b"4242 1 S /System/com.apple.Virtualization.VirtualMachine\n4343 1 S /usr/bin/active-command\n"
            )
            for enabled, opened, containers in cases:
                with (
                    self.subTest(enabled=enabled, opened=opened, containers=containers),
                    patch.object(cleanup, "docker_state", side_effect=containers),
                    patch.object(cleanup, "open_files", return_value=opened),
                ):
                    action["retire_idle_sessions"] = enabled
                    with self.assertRaises(Refused):
                        cleanup.apply_action(self.repo, self.output, {"scope": {}}, action)
                    self.assertTrue((cache / "object.o").exists())

    def test_nested_linked_checkout_is_removable_without_removing_invoking_checkout(self):
        tree = self.repo / ".claude/worktrees/completed"
        self.git("worktree", "add", "--detach", str(tree), "HEAD")
        with patch.object(cleanup.Path, "cwd", return_value=self.repo):
            result = self.apply(self.action(tree, "worktree"))
        self.assertFalse(tree.exists())
        self.assertTrue((self.repo / "tracked").exists())
        self.assertEqual(self.git("show", result["head_ref"] + ":tracked"), b"original\n")

    def test_nested_git_metadata_is_preserved_in_worktree_manifest_mapping(self):
        tree = self.root / "linked"
        self.git("worktree", "add", "--detach", str(tree), "HEAD")
        nested = tree / "dependency/.git"
        nested.mkdir(parents=True)
        (nested / "config").write_text("nested repository recovery\n")
        result = self.apply(self.action(tree, "worktree"))
        manifest = json.loads(gzip.open(result["manifest"], "rb").read())
        row = next(r for r in manifest["rows"] if r["path"] == "dependency/.git/config")
        self.assertEqual(self.git("show", result["tree"] + ":" + row["archive_path"]), b"nested repository recovery\n")

    def test_retirement_allows_idle_agent_cwd_but_preserves_busy_agent_and_open_files(self):
        sessions = self.root / ".claude/sessions"
        sessions.mkdir(parents=True)
        record = sessions / "4242.json"
        record.write_text(json.dumps({"pid": 4242, "status": "idle"}))
        opened = [(4242, str(self.repo)), (4242, str(self.repo / "active-file"))]
        with patch("cleanup_retirement.Path.home", return_value=self.root), patch("cleanup_retirement.run") as process:
            process.return_value.stdout = b"4242 1 S /usr/bin/claude\n"
            self.assertEqual(retirement_open_files(self.repo, opened, True), opened)
            self.assertEqual(retirement_open_files(self.repo, opened[:1], True), [])
            record.write_text(json.dumps({"pid": 4242, "status": "busy"}))
            self.assertEqual(retirement_open_files(self.repo, opened[:1], True), opened[:1])

    def test_retirement_drops_only_exited_consumers_from_old_lsof_snapshot(self):
        sessions = self.root / ".claude/sessions"
        sessions.mkdir(parents=True)
        opened = [(4242, str(self.repo)), (4343, str(self.repo / "live-file"))]
        with patch("cleanup_retirement.Path.home", return_value=self.root), patch("cleanup_retirement.run") as process:
            process.return_value.stdout = b"4242 1 Z /usr/bin/git\n4343 1 S /usr/bin/active-command\n"
            self.assertEqual(retirement_open_files(self.repo, opened, True), opened[1:])

    def test_retirement_keeps_handles_of_virtual_machines_without_docker_backing(self):
        opened = [(4242, str(self.repo / "in-use"))]
        with patch("cleanup_retirement.Path.home", return_value=self.root), patch("cleanup_retirement.run") as process:
            process.return_value.stdout = b"4242 1 S /System/com.apple.Virtualization.VirtualMachine\n"
            self.assertEqual(retirement_open_files(self.repo, opened, True), opened)

    def test_worktree_captures_current_edits_under_the_same_planned_root(self):
        tree = self.root / "linked"
        self.git("worktree", "add", "--detach", str(tree), "HEAD")
        info = tree.stat()
        action = self.action(tree, "worktree")
        action.update(
            snapshot={"device": info.st_dev, "inode": info.st_ino},
            git_head=git(tree, "rev-parse", "HEAD").decode().strip(),
            staged_sha256=hashlib.sha256(b"").hexdigest(),
        )
        (tree / "new-helper").write_text("latest uncommitted contents\n")
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            result = cleanup.apply_action(self.repo, self.output, {"scope": {}, "pass_id": "fixture"}, action)
        self.assertEqual(self.git("show", result["tree"] + ":new-helper"), b"latest uncommitted contents\n")
        self.assertFalse(tree.exists())

    def test_replaced_worktree_root_is_preserved(self):
        tree = self.root / "linked"
        self.git("worktree", "add", "--detach", str(tree), "HEAD")
        info = tree.stat()
        action = self.action(tree, "worktree")
        action["snapshot"] = {"device": info.st_dev, "inode": info.st_ino + 1}
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            with self.assertRaisesRegex(Refused, "root was replaced"):
                cleanup.apply_action(self.repo, self.output, {"scope": {}}, action)
        self.assertTrue((tree / "tracked").exists())

    def test_archive_preserves_extended_attributes(self):
        source = self.root / "attributed"
        source.mkdir()
        file = source / "file"
        file.write_bytes(b"source bytes")
        key = "com.layerfs.cleanup-fixture" if os.uname().sysname == "Darwin" else "user.cleanup-fixture"
        if hasattr(os, "setxattr"):
            os.setxattr(file, key, b"attribute bytes")
        elif os.uname().sysname == "Darwin":
            subprocess.run(["xattr", "-w", key, "attribute bytes", str(file)], check=True, timeout=5)
        else:
            self.skipTest("No extended attribute support")
        result = self.apply(self.action(source))
        manifest = json.loads(gzip.open(result["manifest"], "rb").read())
        import base64

        row = next(row for row in manifest["rows"] if row["path"] == "file")
        self.assertEqual(base64.b64decode(row["xattrs_base64"][key]), b"attribute bytes")

    def test_docker_desktop_aliases_protect_native_host_paths(self):
        source = self.root / "copy"
        source.mkdir()
        for prefix in ("/host_mnt", "/run/desktop/mnt/host"):
            mount = {"Type": "bind", "Source": prefix + str(self.root)}
            container = {"name": "active", "mounts": [mount]}
            with patch("cleanup_state.sys.platform", "darwin"):
                with self.assertRaisesRegex(Refused, "bind overlapping"):
                    ensure_idle([source], [], [container], [])
                info = {
                    "Id": "fixture",
                    "Name": "/active",
                    "Mounts": [mount],
                    "State": {"Running": False, "Status": "exited"},
                }
                with self.assertRaisesRegex(Refused, "protected path"):
                    guard_container(info, {}, {"protected_paths": [str(source)]})

    def test_explicit_cache_discovery_includes_primary_and_limits_scope(self):
        other = self.root / "linked"
        self.git("worktree", "add", "--detach", str(other), "HEAD")
        for tree in (self.repo, other):
            target = tree / "target"
            (target / "debug/deps").mkdir(parents=True)
            (target / ".rustc_info.json").write_text("{}")
        registered = {str(p): {} for p in (self.repo, other)}
        scope, _ = discover(
            self.repo,
            {},
            registered,
            [],
            [],
            "user authorized caches",
            caches_only=True,
            cache_worktrees=[str(self.repo)],
        )
        self.assertEqual([a["path"] for a in scope["actions"]], [str(self.repo / "target/debug")])
        default, _ = discover(self.repo, {}, registered, [], [], "user authorized caches", caches_only=True)
        self.assertEqual([a["path"] for a in default["actions"]], [str(other / "target/debug")])

    def test_archive_across_hash_batches_preserves_every_file(self):
        source = self.root / "many-files"
        source.mkdir()
        for index in range(4097):
            (source / str(index)).write_text(str(index))
        with patch("cleanup_archive.HASH_BATCH_BYTES", 8192):
            result = self.apply(self.action(source))
        manifest = json.loads(gzip.open(result["manifest"], "rb").read())
        self.assertEqual(sum(row.get("target") is None and row["path"] != "." for row in manifest["rows"]), 4097)
        self.assertEqual(self.git("show", result["tree"] + ":0"), b"0")
        self.assertEqual(self.git("show", result["tree"] + ":4096"), b"4096")
        self.assertFalse(source.exists())

    def test_explicit_generated_tree_discard_keeps_guards_and_skips_archive(self):
        source = self.root / "staged-installer"
        source.mkdir()
        (source / "package").write_text("download can be recreated")
        action = self.action(source, "discard-tree")
        action["disposable_generated"] = True
        with patch.object(cleanup, "archive") as capture:
            self.apply(action)
            capture.assert_not_called()
        self.assertFalse(source.exists())

    def test_generated_tree_discard_requires_explicit_payload_release(self):
        source = self.root / "keep"
        source.mkdir()
        (source / "record").write_text("keep")
        with self.assertRaisesRegex(Refused, "disposable_generated"):
            self.apply(self.action(source, "discard-tree"))
        self.assertTrue((source / "record").exists())

    def test_discard_tree_refuses_file_payload(self):
        source = self.root / "file"
        source.write_text("keep")
        action = self.action(source, "discard-tree")
        action["disposable_generated"] = True
        with self.assertRaisesRegex(Refused, "must be a directory"):
            self.apply(action)
        self.assertEqual(source.read_text(), "keep")

    def test_parallel_archives_keep_independent_recovery_indexes(self):
        actions = []
        for name in ("one", "two"):
            source = self.root / name
            source.mkdir()
            (source / "payload").write_text(name)
            action = self.action(source)
            action["snapshot"] = inventory(source)[1]
            actions.append(action)
        plan = {
            "version": 1,
            "pass_directory": str(self.output),
            "repository": str(self.repo),
            "scope": {},
            "actions": actions,
            "protected_presence": {},
            "blocked": [],
        }
        path = self.output / "plan.json"
        path.write_text(json.dumps(plan))
        args = type("Args", (), {"plan": str(path), "jobs": 2})()
        with (
            patch.object(cleanup, "docker_state", return_value=[]),
            patch.object(cleanup, "open_files", return_value=[]),
        ):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(cleanup.apply_plan(args), 0)
        report = json.loads((self.output / "report.json").read_text())
        self.assertEqual(len(report["results"]), 2)
        for row in report["results"]:
            self.assertEqual(row["state"], "removed")
            expected = Path(row["action"]["path"]).name.encode()
            self.assertEqual(self.git("show", row["recovery"]["tree"] + ":payload"), expected)
        self.assertEqual(report["uncertain_actions"], [])

    def test_interrupted_attempt_is_uncertain_and_never_requeued(self):
        plan = {
            "version": 1,
            "pass_directory": str(self.output),
            "repository": str(self.repo),
            "scope": {},
            "actions": [{"kind": "archive-file"}, {"kind": "archive-tree"}],
            "protected_presence": {},
            "blocked": [],
        }
        path = self.output / "plan.json"
        path.write_text(json.dumps(plan))
        args = type("Args", (), {"plan": str(path)})()
        with patch.object(cleanup, "apply_action", side_effect=KeyboardInterrupt) as operation:
            with self.assertRaises(KeyboardInterrupt):
                cleanup.apply_plan(args)
            self.assertEqual(operation.call_count, 1)
        report = json.loads((self.output / "report.json").read_text())
        self.assertTrue(report["interrupted"])
        self.assertEqual(report["uncertain_actions"], [plan["actions"][0]])
        self.assertEqual(report["not_attempted"], [plan["actions"][1]])

    def test_protected_cargo_volume_never_starts_helper(self):
        import cleanup_volume_cache

        action = {"kind": "docker-cargo-cache", "name": "pinned"}
        with patch.object(cleanup_volume_cache, "helper") as helper:
            with self.assertRaisesRegex(Refused, "Protected volume"):
                cleanup_volume_cache.prepare(action, {"protected_volumes": ["pinned"]}, [], self.output)
            helper.assert_not_called()

    def test_changed_cargo_volume_preserved_before_helper(self):
        import cleanup_volume_cache

        action = {
            "kind": "docker-cargo-cache",
            "name": "cache",
            "identity": {"CreatedAt": "old"},
            "helper_sha256": hashlib.sha256(cleanup_volume_cache.SCRIPT.read_bytes()).hexdigest(),
        }
        with (
            patch.object(cleanup_volume_cache, "docker_state", return_value=[]),
            patch.object(cleanup_volume_cache, "volume_identity", return_value={"CreatedAt": "new"}),
            patch.object(cleanup_volume_cache, "helper") as helper,
        ):
            with self.assertRaisesRegex(Refused, "identity changed"):
                cleanup_volume_cache.clear(action, {}, self.output, lambda *a, **k: None)
            helper.assert_not_called()

    def test_nested_cargo_target_mount_stays_inside_selected_directory(self):
        import cleanup_volume_cache
        from types import SimpleNamespace

        for mode in ("plan", "apply"):
            calls = []

            def run(args, **kwargs):
                calls.append(args)
                data = (
                    b'[{"State":{"Running":false,"ExitCode":0}}]'
                    if args[1] == "inspect"
                    else b'{"version":1}'
                    if args[1] == "start"
                    else b"fixture"
                )
                return SimpleNamespace(stdout=data)

            with (
                patch.object(cleanup_volume_cache, "run", side_effect=run),
                patch.object(cleanup_volume_cache, "docker_state", return_value=[]),
            ):
                result = cleanup_volume_cache.helper(
                    "cache",
                    "pinned-image",
                    self.output,
                    mode,
                    cache={"version": 1} if mode == "apply" else None,
                    target_subdir="plain",
                )
            self.assertEqual(result, {"version": 1})
            create = calls[0]
            mounts = [create[i + 1] for i, x in enumerate(create) if x == "--mount"]
            self.assertIn(
                "type=volume,src=cache,dst=/target,volume-subpath=plain" + (",readonly" if mode == "plan" else ""),
                mounts,
            )
            self.assertFalse(any(x == "type=volume,src=cache,dst=/target" for x in mounts))

    def test_nested_cargo_target_rejects_escape_before_container_creation(self):
        import cleanup_volume_cache

        with patch.object(cleanup_volume_cache, "run") as run:
            for path in ("", "..", ".", "../other", "/plain", "plain/child", "plain,readonly", 1):
                with self.assertRaisesRegex(Refused, "one literal directory name"):
                    cleanup_volume_cache.helper("cache", "image", self.output, "plan", target_subdir=path)
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()

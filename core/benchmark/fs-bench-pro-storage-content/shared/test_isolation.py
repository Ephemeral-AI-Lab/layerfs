#!/usr/bin/env python3
"""The isolation contract: one worktree, one mutable namespace, no silent sharing.

    python3 core/benchmark/fs-bench-pro-storage-content/shared/test_isolation.py
"""

from __future__ import annotations

import fcntl
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import history_corpus  # noqa: E402
import isolation  # noqa: E402


def space(name: str) -> isolation.Namespace:
    return isolation.namespace(f"/tmp/isolation-test/{name}/core/benchmark/fs-bench-pro-storage-content")


class NamespaceTest(unittest.TestCase):
    def test_two_worktrees_never_share_a_mutable_root(self) -> None:
        one, two = space("one"), space("two")
        self.assertNotEqual(one.lock_path, two.lock_path)
        self.assertNotEqual(one.artifact_root, two.artifact_root)
        self.assertNotEqual(one.label, two.label)

    def test_the_lock_is_inside_the_worktree_that_owns_it(self) -> None:
        one = space("one")
        self.assertEqual(one.worktree_root, one.lock_path.parents[3])
        self.assertTrue(str(one.lock_path).startswith(str(one.worktree_root)))

    def test_a_sibling_worktree_is_refused_as_owned(self) -> None:
        with self.assertRaises(isolation.IsolationError):
            space("one").assert_owned(space("two").artifact_root, "another worktree's masters")

    def test_a_path_outside_every_worktree_is_refused(self) -> None:
        for foreign in (Path("/tmp"), Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")):
            with self.assertRaises(isolation.IsolationError):
                space("one").assert_owned(foreign, "a foreign root")

    def test_own_roots_are_accepted(self) -> None:
        one = space("one")
        self.assertEqual(one.assert_owned(one.artifact_root, "masters"), one.artifact_root.resolve())

class SharedInputTest(unittest.TestCase):
    def test_the_corpus_is_declared_read_only(self) -> None:
        self.assertIn(history_corpus.DEFAULT_ROOT.as_posix(), isolation.SHARED_READ_ONLY)

    def test_the_corpus_lives_outside_every_worktree(self) -> None:
        one = space("one")
        corpus = history_corpus.DEFAULT_ROOT.resolve()
        self.assertNotEqual(corpus, one.worktree_root)
        self.assertNotIn(one.worktree_root.resolve(), corpus.parents)

    def test_the_declaration_carries_no_writable_root(self) -> None:
        for entry in isolation.SHARED_READ_ONLY:
            self.assertTrue(Path(entry).is_absolute(), entry)


class TargetDirectoryTest(unittest.TestCase):
    def test_a_target_inside_the_worktree_is_accepted(self) -> None:
        one = space("one")
        with mock.patch.object(isolation, "effective_target_directory", return_value=one.build_target):
            self.assertEqual(isolation.assert_target_owned(one, "Cargo.toml"), one.build_target)

    def test_a_target_in_another_worktree_is_refused(self) -> None:
        one, two = space("one"), space("two")
        with mock.patch.object(isolation, "effective_target_directory", return_value=two.build_target):
            with self.assertRaises(isolation.IsolationError):
                isolation.assert_target_owned(one, "Cargo.toml")

    def test_a_machine_level_target_is_refused(self) -> None:
        one = space("one")
        with mock.patch.object(isolation, "effective_target_directory", return_value=Path("/tmp/target")):
            with self.assertRaises(isolation.IsolationError):
                isolation.assert_target_owned(one, "Cargo.toml")


class ObservationTest(unittest.TestCase):
    def test_a_build_is_classified_as_a_build(self) -> None:
        table = [(1, 0, "/bin/zsh"), (4242, 1, "cargo +1.85.1 build --locked")]
        with mock.patch.object(isolation, "_process_table", return_value=table):
            with mock.patch.object(isolation, "_ancestry", return_value={1}):
                self.assertEqual(isolation.concurrent_work(), [{"pid": 4242, "kind": "build",
                                                                 "command": "cargo +1.85.1 build --locked"}])

    def test_another_worktrees_measurement_is_recognised(self) -> None:
        table = [(1, 0, "/bin/zsh"),
                 (9, 1, "python3 /tmp/other/core/benchmark/fs-bench-pro-storage-content/runner.py perf")]
        with mock.patch.object(isolation, "_process_table", return_value=table):
            with mock.patch.object(isolation, "_ancestry", return_value={1}):
                self.assertEqual([entry["kind"] for entry in isolation.concurrent_work()], ["measurement"])

    def test_an_unrelated_process_is_not_reported(self) -> None:
        table = [(1, 0, "/bin/zsh"), (7, 1, "/usr/bin/tail -f /var/log/system.log")]
        with mock.patch.object(isolation, "_process_table", return_value=table):
            with mock.patch.object(isolation, "_ancestry", return_value={1}):
                self.assertEqual(isolation.concurrent_work(), [])

    def test_the_observation_declares_itself_a_snapshot(self) -> None:
        observed = isolation.observe(space("one"))
        self.assertIn("snapshot", str(observed["observation"]))
        self.assertEqual(observed["scope"], "per-worktree")
        self.assertIn("other worktrees run freely", str(observed["measurement_excludes"]))

    def test_this_process_and_its_parent_are_excluded(self) -> None:
        # The walk must terminate at launchd rather than looping on a parent whose
        # pid is lower than its child's — which is the ordinary case, not the edge.
        lineage = isolation._ancestry()
        self.assertIn(os.getpid(), lineage)
        self.assertIn(os.getppid(), lineage)
        self.assertNotIn(4242, lineage)


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_passes(self) -> None:
        self.assertEqual(isolation.self_check(), [])


class LiveLockTest(unittest.TestCase):
    """The semantics the owner direction asks for, held by a real `flock`.

    Synthetic worktree roots, so the test never touches a live measurement's lock
    file and can never be failed by one that is running.
    """

    def _holder(self, lock_path: Path) -> subprocess.Popen:
        return subprocess.Popen(
            [sys.executable, "-c", textwrap.dedent(f"""
                import fcntl, time
                handle = open({str(lock_path)!r}, "a")
                fcntl.flock(handle, fcntl.LOCK_EX)
                print("held", flush=True)
                time.sleep(20)
            """)],
            stdout=subprocess.PIPE, text=True,
        )

    @staticmethod
    def _probe(lock_path: Path) -> bool:
        """True when this process can take the lock non-blockingly."""
        handle = open(lock_path, "a")
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return True
        except BlockingIOError:
            return False
        finally:
            handle.close()

    def test_another_worktree_is_never_blocked_and_the_same_one_still_is(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            one = isolation.namespace(f"{directory}/one/core/benchmark/fs-bench-pro-storage-content")
            two = isolation.namespace(f"{directory}/two/core/benchmark/fs-bench-pro-storage-content")
            one.lock_path.parent.mkdir(parents=True, exist_ok=True)
            two.lock_path.parent.mkdir(parents=True, exist_ok=True)
            holder = self._holder(one.lock_path)
            try:
                self.assertEqual(holder.stdout.readline().strip(), "held")
                self.assertFalse(self._probe(one.lock_path), "this worktree stopped excluding itself")
                self.assertTrue(self._probe(two.lock_path), "another worktree was still blocked")
            finally:
                holder.kill()
                holder.wait()
                holder.stdout.close()


class NoMachineGlobalLockTest(unittest.TestCase):
    """A sealed scan: the retired machine-global lock must not come back.

    A regression guard in the shape the rest of the harness uses — source text,
    scanned, with the exemptions named rather than implied.
    """

    HARNESSES = (
        Path(__file__).resolve().parents[3] / "benchmark" / "fs-bench-pro",
        Path(__file__).resolve().parents[1],
    )

    #: The modules that name the retired lock on purpose: they are the record of
    #: its retirement, and a docstring is not an acquisition.
    EXEMPT = {"isolation.py", "test_isolation.py"}

    def test_no_harness_source_derives_a_lock_from_tmpdir(self) -> None:
        offenders: list[str] = []
        for root in self.HARNESSES:
            for path in sorted(root.rglob("*.py")):
                if path.name in self.EXEMPT or "__pycache__" in path.parts:
                    continue
                for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                    if "TMPDIR" in line and "layerfs-infra-measurement" in line:
                        offenders.append(f"{path}:{number}: {line.strip()}")
        self.assertEqual(offenders, [], "a machine-global measurement lock came back")


if __name__ == "__main__":
    unittest.main(verbosity=2)


if __name__ == "__main__":
    unittest.main(verbosity=2)

"""Offline checks for qualified Cargo reuse, immutable copies and owned retention."""
import fcntl
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import runner


class BuildReuseTests(unittest.TestCase):
    def test_unbuilt_workspace_binary_sources_need_not_enter_linux_build(self):
        dockerfile = (runner.BENCH / 'Dockerfile.layerfs').read_text()
        self.assertNotIn('COPY benchmark/fs-bench-pro/src ', dockerfile)
        manifest = (runner.BENCH / 'Cargo.toml').read_text()
        binary = '[[bin]]' + manifest.split('[[bin]]', 1)[1]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            selected = root / 'crates/selected'
            (selected / 'src').mkdir(parents=True)
            (selected / 'Cargo.toml').write_text(
                '[package]\nname="selected"\nversion="0.0.0"\nedition="2021"\n')
            (selected / 'src/main.rs').write_text('fn main() { println!("selected"); }\n')
            benchmark = root / 'benchmark/fs-bench-pro'
            benchmark.mkdir(parents=True)
            (benchmark / 'Cargo.toml').write_text(
                '[package]\nname="fs-benchmark-pro"\nversion="0.0.0"\nedition="2021"\n' + binary)
            (root / 'Cargo.toml').write_text(
                '[workspace]\nmembers=["crates/selected","benchmark/fs-bench-pro"]\nresolver="2"\n')
            for args in (['generate-lockfile', '--offline'],
                         ['build', '--locked', '--offline', '-p', 'selected']):
                runner.runtime.run(['cargo', '+1.85.1', *args], cwd=root,
                    deadline=runner.runtime.Deadline.after(30))
            result = runner.runtime.run([str(root / 'target/debug/selected')],
                deadline=runner.runtime.Deadline.after(5))
            self.assertEqual(result.stdout.strip(), b'selected')
            self.assertFalse((benchmark / 'src').exists())

    def test_docker_owned_source_refresh_rebuilds_backdated_content(self):
        dockerfile = (runner.BENCH / 'Dockerfile.layerfs').read_text()
        refresh = next(line.strip()[3:].removesuffix(' \\') for line in dockerfile.splitlines()
                       if line.strip().startswith('&& find Cargo.toml Cargo.lock '))
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / 'crates/cache-probe/src/main.rs'
            source.parent.mkdir(parents=True)
            (root / 'tools').mkdir()
            (root / 'benchmark/fs-bench-pro').mkdir(parents=True)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/cache-probe"]\nresolver="2"\n')
            (source.parent.parent / 'Cargo.toml').write_text(
                '[package]\nname="cache-probe"\nversion="0.0.0"\nedition="2021"\n')
            (root / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "cache-probe"\nversion = "0.0.0"\n')
            source.write_text('fn main() { println!("old"); }\n')
            stamp = source.stat().st_mtime_ns
            target = root / 'shared-target'
            command = ['cargo', '+1.85.1', 'build', '--locked', '--offline', '--target-dir', str(target)]
            def build():
                runner.runtime.run(command, cwd=root, deadline=runner.runtime.Deadline.after(30))
                return runner.runtime.run([str(target / 'debug/cache-probe')],
                    deadline=runner.runtime.Deadline.after(5)).stdout.strip()
            self.assertEqual(build(), b'old')
            source.write_text('fn main() { println!("new"); }\n')
            os.utime(source, ns=(stamp, stamp))
            self.assertEqual(build(), b'old', 'reproduce Cargo false-fresh shared-cache boundary')
            sentinel = target / 'retained-dependency-cache'
            sentinel.write_bytes(b'preserve')
            runner.runtime.run(['sh', '-c', refresh], cwd=root,
                deadline=runner.runtime.Deadline.after(5))
            self.assertEqual(build(), b'new')
            self.assertEqual(sentinel.read_bytes(), b'preserve')

    def test_host_jobs_are_bounded_and_recorded_in_compatibility(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(os, 'cpu_count', return_value=14):
            self.assertEqual(runner.host_build_jobs(), 8)
            with patch.object(os, 'cpu_count', return_value=4):
                self.assertEqual(runner.host_build_jobs(), 4)
            with patch.dict(os.environ, {'CARGO_BUILD_JOBS': '2'}):
                self.assertEqual(runner.host_build_jobs(), 2)
            for value in ('0', '9', '-1', 'invalid'):
                with patch.dict(os.environ, {'CARGO_BUILD_JOBS': value}), self.assertRaises(ValueError):
                    runner.host_build_jobs()

    def test_a_build_takes_no_measurement_lock(self):
        """Owner direction, 2026-09-21: a build never excludes another worktree.

        The retired machine-global file stays free while a build runs, and the
        build does not take this worktree's lock either: only pruning does, because
        pruning deletes state a running lane in this worktree may be about to read.
        """
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'TMPDIR': folder}):
            worktree_lock = Path(folder) / 'worktree-measurement.lock'

            def archive(tag):
                for path in (Path(folder) / 'layerfs-infra-measurement.lock', worktree_lock):
                    with path.open('a') as handle:
                        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with patch.object(runner, 'source_build_args', return_value={'LAYERFS_SOURCE_SEAL': 'a' * 64}), \
                    patch.object(runner.isolation, 'worktree_lock_path', return_value=worktree_lock), \
                    patch.object(runner.runtime, 'build_image', return_value=SimpleNamespace(returncode=0)), \
                    patch.object(runner, 'archive_image', side_effect=archive) as archived:
                self.assertEqual(runner.main(['--build-image']), 0)
                archived.assert_called_once()

    def test_pruning_keeps_the_worktree_lock(self):
        """Pruning still excludes a concurrent run *in this worktree*."""
        with tempfile.TemporaryDirectory() as folder:
            worktree_lock = Path(folder) / 'worktree-measurement.lock'

            def prune(keep, apply):
                with worktree_lock.open('a') as handle:
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return {'kept': [], 'removed': []}
            with patch.object(runner.isolation, 'worktree_lock_path', return_value=worktree_lock), \
                    patch.object(runner, 'prune_build_caches', side_effect=prune):
                self.assertEqual(runner.main(['--prune-builds', '2']), 0)

    def test_changed_source_cannot_publish_a_qualified_image(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, {'TMPDIR': folder}), \
                patch.object(runner, 'source_build_args', side_effect=[
                    {'LAYERFS_SOURCE_SEAL': 'a' * 64}, {'LAYERFS_SOURCE_SEAL': 'b' * 64}]), \
                patch.object(runner.runtime, 'build_image', return_value=SimpleNamespace(returncode=0)), \
                patch.object(runner, 'archive_image') as archive:
            with self.assertRaisesRegex(ValueError, 'source changed'):
                runner.main(['--build-image'])
            archive.assert_not_called()

    def test_compilation_and_dependency_invalidation(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            bench = root / 'benchmark/fs-bench-pro'
            paths = ['Cargo.toml', 'Cargo.lock', '.dockerignore', 'crates/store/src/lib.rs',
                     'crates/store/build.rs', 'crates/store/sql/schema.sql',
                     'benchmark/fs-bench-pro/Cargo.toml',
                     'benchmark/fs-bench-pro/Dockerfile.layerfs',
                     'benchmark/fs-bench-pro/src/main.rs',
                     'benchmark/fs-bench-pro/families/example.rs',
                     'benchmark/fs-bench-pro/workload/main.rs',
                     'benchmark/fs-bench-pro/build.rs', '.cargo/config.toml']
            for name in paths:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            with patch.object(runner, 'REPO', root), patch.object(runner, 'BENCH', bench), patch.object(
                    runner.runtime, 'run', return_value=SimpleNamespace(stdout=b'toolchain-1')) as command:
                def seals():
                    return runner.compilation_seals()
                baseline = seals()
                for name in paths:
                    path = root / name
                    original = path.read_bytes()
                    path.write_bytes(original + b' changed')
                    changed = seals()
                    self.assertNotEqual(changed[0], baseline[0], name)
                    benchmark_source = name in ('benchmark/fs-bench-pro/src/main.rs',
                                                'benchmark/fs-bench-pro/families/example.rs')
                    self.assertEqual(changed[1] == baseline[1], benchmark_source, name)
                    path.write_bytes(original)
                for name in ('docs.md', 'benchmark/fs-bench-pro/families/test.py'):
                    (root / name).write_text('non-native change')
                    self.assertEqual(seals(), baseline)
                for env in ('RUSTFLAGS', 'CARGO_BUILD_TARGET', 'CARGO_PROFILE_RELEASE_LTO',
                            'SDKROOT', 'CC', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_JOBS'):
                    with patch.dict(os.environ, {env: '2' if env == 'CARGO_BUILD_JOBS' else 'changed'}):
                        self.assertTrue(all(a != b for a, b in zip(seals(), baseline)), env)
                command.return_value = SimpleNamespace(stdout=b'toolchain-2')
                self.assertTrue(all(a != b for a, b in zip(seals(), baseline)))

    def test_seed_is_independent_excludes_benchmark_and_fails_closed(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(runner, 'HOST_ROOT', Path(folder)):
            root = Path(folder)
            binary = root / 'fs-benchmark-pro'
            binary.write_bytes(b'qualified')
            source = root / 'builds/native-old'
            for name in ('release/.fingerprint/fs-benchmark-pro-123/bin',
                         'release/deps/fs_benchmark_pro-123.d', 'release/fs-benchmark-pro',
                         'release/deps/libstore.rlib', 'release/.fingerprint/store-123/lib'):
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'compiled')
            previous = {'LAYERFS_COMPILATION_SEAL': 'old', 'LAYERFS_DEPENDENCY_SEAL': 'deps',
                        'build_target': str(source), 'binary_sha256': runner.runtime.file_sha256(binary)}
            Path(str(binary) + '.identity.json').write_text(json.dumps(previous))
            target = root / 'builds/native-new'
            self.assertIsNone(runner.seed_host_dependencies(target, {'LAYERFS_DEPENDENCY_SEAL': 'other'}, binary))
            self.assertFalse(target.exists())
            values = {'LAYERFS_DEPENDENCY_SEAL': 'deps'}
            receipt = runner.seed_host_dependencies(target, values, binary)
            self.assertTrue(receipt['independent_copy'])
            self.assertFalse(any('fs_benchmark_pro' in str(p) or 'fs-benchmark-pro' in str(p)
                                 for p in target.rglob('*')))
            copied = target / 'release/deps/libstore.rlib'
            copied.write_bytes(b'new candidate')
            self.assertEqual((source / 'release/deps/libstore.rlib').read_bytes(), b'compiled')
            self.assertIsNone(runner.seed_host_dependencies(target, values, binary))
            binary.write_bytes(b'corrupt')
            with self.assertRaisesRegex(ValueError, 'producer binary identity'):
                runner.seed_host_dependencies(root / 'builds/native-third', values, binary)

    def test_build_mode_is_validated_and_defaults_to_the_owned_incremental_cache(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(runner.build_mode(), 'incremental')
        with patch.dict(os.environ, {'LAYERFS_BUILD_ISOLATION': 'sealed'}):
            self.assertEqual(runner.build_mode(), 'sealed')
        for value in ('', 'shared', 'per-seal', 'INCREMENTAL'):
            with patch.dict(os.environ, {'LAYERFS_BUILD_ISOLATION': value}), self.assertRaises(ValueError):
                runner.build_mode()
        with tempfile.TemporaryDirectory() as folder:
            with patch.object(runner, 'HOST_ROOT', Path(folder)):
                self.assertEqual(runner.incremental_build_target(),
                                 Path(folder) / 'builds' / runner.INCREMENTAL_TARGET_NAME)

    def test_recompiled_packages_names_every_cargo_unit(self):
        result = SimpleNamespace(
            stdout=b'   Compiling layerfs-workspace v0.1.4 (/repo/crates/layerfs-workspace)\n'
                   b'   Compiling layerfs-sdk v0.1.4 (/repo/crates/layerfs-sdk)\n'
                   b'   Compiling layerfs-sdk v0.1.4 (/repo/crates/layerfs-sdk)\n',
            stderr=b'    Finished `release` profile [optimized] target(s) in 19.17s\n'
                   b'   Compiling fs-benchmark-pro v0.1.4 (/repo/benchmark/fs-bench-pro)\n')
        self.assertEqual(runner.recompiled_packages(result),
                         ['layerfs-workspace', 'layerfs-sdk', 'fs-benchmark-pro'])
        self.assertEqual(runner.recompiled_packages(SimpleNamespace(stdout=b'', stderr=b'')), [])

    def test_prune_retention_bounds_writable_build_caches_only(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            builds = root / 'builds'
            names = ['native-' + str(i) * 64 for i in range(3)] + [runner.INCREMENTAL_TARGET_NAME]
            for index, name in enumerate(names):
                path = builds / name / 'release'
                path.mkdir(parents=True)
                (path / 'payload.bin').write_bytes(b'x' * (10 + index))
                (builds / name / 'CACHEDIR.TAG').write_text('Signature: 8a477f597d28d172789f06886806bc55\n')
                os.utime(builds / name, (1_000 + index, 1_000 + index))
            protected = {}
            for name in ('fixtures', 'prepared', 'samples', 'binary-archive', 'image-archive'):
                path = root / name
                path.mkdir()
                (path / 'keep.bin').write_bytes(b'keep')
                protected[path] = (path / 'keep.bin').read_bytes()
            with patch.object(runner, 'HOST_ROOT', root):
                receipt = runner.prune_build_caches(keep=2, apply=False)
                self.assertEqual(receipt['removed'], [])
                self.assertEqual(len(receipt['candidates']), 1)
                self.assertTrue((builds / names[0]).is_dir())
                receipt = runner.prune_build_caches(keep=2, apply=True)
                removed = sorted(Path(entry['path']).name for entry in receipt['removed'])
                self.assertEqual(removed, names[:1])
                self.assertEqual(receipt['reclaimed_bytes'], sum(entry['bytes'] for entry in receipt['removed']))
                self.assertEqual(sorted(Path(p).name for p in receipt['retained']),
                                 names[1:3])
                self.assertTrue((builds / runner.INCREMENTAL_TARGET_NAME).is_dir())
                self.assertTrue(receipt['policy']['incremental_cache'] == runner.INCREMENTAL_TARGET_NAME)
                self.assertEqual(receipt['policy']['protected'],
                                 ['fixtures', 'prepared', 'samples', 'binary-archive', 'image-archive'])
                for path, content in protected.items():
                    self.assertEqual((path / 'keep.bin').read_bytes(), content)
                again = runner.prune_build_caches(keep=2, apply=True)
                self.assertEqual(again['removed'], [])
                self.assertEqual(again['reclaimed_bytes'], 0)

    def test_prune_fails_closed_before_removing_anything(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(runner, 'HOST_ROOT', Path(folder)):
            root = Path(folder)
            for keep in (-1, True, 0.5):
                with self.assertRaisesRegex(ValueError, 'nonnegative'):
                    runner.prune_build_caches(keep, apply=True)
            target = root / ('builds/native-' + 'a' * 64)
            target.mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, 'unrecognized'):
                runner.prune_build_caches(0, apply=True)
            self.assertTrue(target.exists())
            target.rmdir()
            target.symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'unrecognized'):
                runner.prune_build_caches(0, apply=True)
            target.unlink()
            (root / 'builds').rmdir()
            (root / 'builds').symlink_to(root, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'independently owned'):
                runner.prune_build_caches(0, apply=True)

    def test_publishing_breaks_existing_links_and_archive_is_immutable(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(runner, 'HOST_ROOT', Path(folder)):
            root = Path(folder)
            control, active, built = (root / name for name in ('control', 'active', 'built'))
            control.write_bytes(b'control')
            active.hardlink_to(control)
            built.write_bytes(b'candidate')
            runner.copy_executable(built, active)
            self.assertEqual(control.read_bytes(), b'control')
            self.assertEqual(active.read_bytes(), b'candidate')
            self.assertNotEqual(active.stat().st_ino, built.stat().st_ino)
            identity = Path(str(active) + '.identity.json')
            sha = runner.runtime.file_sha256(active)
            identity.write_text(json.dumps({'binary_sha256': sha}))
            runner.archive_binary(active)
            archived = root / 'binary-archive' / sha / active.name
            self.assertEqual(archived.stat().st_nlink, 1)
            self.assertEqual(archived.stat().st_mode & 0o222, 0)
            self.assertEqual(Path(str(archived) + '.identity.json').stat().st_mode & 0o222, 0)
            active.write_bytes(b'next')
            self.assertEqual(archived.read_bytes(), b'candidate')
            archived.chmod(0o755)
            archived.write_bytes(b'corrupt')
            active.write_bytes(b'candidate')
            with self.assertRaisesRegex(ValueError, 'custody mismatch'):
                runner.archive_binary(active)


if __name__ == '__main__':
    unittest.main()

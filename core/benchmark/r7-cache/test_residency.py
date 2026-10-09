"""External harness contract tests; the real Linux primitive is separately gated."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import sys

SPEC = importlib.util.spec_from_file_location('r7_residency', Path(__file__).with_name('residency.py'))
RESIDENCY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RESIDENCY)
STREAM_SPEC = importlib.util.spec_from_file_location('stream_manifest', Path(__file__).with_name('stream_manifest.py'))
STREAM = importlib.util.module_from_spec(STREAM_SPEC)
STREAM_SPEC.loader.exec_module(STREAM)
GEN_SPEC = importlib.util.spec_from_file_location('generate_manifest', Path(__file__).with_name('generate_manifest.py'))
GENERATOR = importlib.util.module_from_spec(GEN_SPEC)
with patch.dict(sys.modules, {'residency': RESIDENCY, 'stream_manifest': STREAM}):
    GEN_SPEC.loader.exec_module(GENERATOR)


class ClassContract(unittest.TestCase):
    def test_nonzero_cold_is_ineligible(self):
        self.assertEqual(RESIDENCY.class_status('A', {'resident_pages': 1}), 'INELIGIBLE')
        self.assertEqual(RESIDENCY.class_status('A', {'resident_pages': 0}), 'ELIGIBLE')

    def test_fresh_warm_requires_measured_phase_demands(self):
        row = {'resident_pages': 10}
        self.assertEqual(RESIDENCY.class_status('B', row), 'PENDING_PHASE_COUNTERS')
        self.assertEqual(RESIDENCY.class_status('B', row, object_demands=0), 'ELIGIBLE')
        self.assertEqual(RESIDENCY.class_status('B', row, object_demands=1), 'INELIGIBLE')

    def test_same_mount_lifetime_is_strict(self):
        row = {'resident_pages': 10}
        for ns in (-1, 60_000_000_000, 61_000_000_000):
            self.assertEqual(RESIDENCY.class_status('C', row, warm_interval_ns=ns), 'INELIGIBLE')
        self.assertEqual(RESIDENCY.class_status('C', row, warm_interval_ns=59_999_999_999), 'ELIGIBLE')

    def test_protected_paths_are_refused_before_open(self):
        with self.assertRaises(ValueError):
            RESIDENCY.validate_path('/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness/.git/index')

    def test_absent_optional_sidecar_does_not_hide_missing_store(self):
        with tempfile.TemporaryDirectory(prefix='r7-cache-contract-') as directory:
            path = Path(directory) / 'missing-wal'
            with patch.object(RESIDENCY, 'inspect_file', side_effect=FileNotFoundError):
                row = RESIDENCY.attest([path], optional=[path])
                self.assertFalse(row['files'][0]['present'])
                with self.assertRaises(FileNotFoundError):
                    RESIDENCY.attest([path])

    @unittest.skipUnless(RESIDENCY.platform.system() == 'Linux', 'Linux mincore required')
    def test_real_observer_does_not_read_payload_and_records_empty_file(self):
        with tempfile.TemporaryDirectory(prefix='r7-cache-native-') as directory:
            path = Path(directory) / 'empty'
            path.touch()
            row = RESIDENCY.attest([path], evict=True)
            self.assertEqual(row['payload_bytes_read'], 0)
            self.assertEqual(row['resident_pages'], 0)
            self.assertEqual(row['files'][0]['mmap_calls'], 0)
            self.assertEqual(row['files'][0]['eviction_hint_attempts'], 1)


class StreamedManifestContract(unittest.TestCase):
    @staticmethod
    def observe(path, evict=False, *, open_scope=None, expected_identity=None):
        fd, parent, leaf = open_scope.open(path)
        try:
            actual = STREAM.pinned_identity(os.fstat(fd))
            if actual != expected_identity:
                raise ValueError('independent identity oracle differs from manifest')
            return dict(path=str(path), present=True, **actual, resident_pages=0,
                        total_pages=0, eviction_hint_attempts=int(evict),
                        payload_bytes_read=0)
        finally:
            os.close(fd)
            os.close(parent)

    def test_generator_and_stream_preserve_two_files_and_artifact_hash(self):
        with tempfile.TemporaryDirectory(prefix='r7-manifest-contract-') as directory:
            directory = Path(directory).resolve()
            root = directory / 'native'
            root.mkdir()
            (root / 'first').write_bytes(b'one')
            (root / 'second').write_bytes(b'two')
            manifest = directory / 'input.jsonl'
            sealed = GENERATOR.generate(root, manifest)
            output = directory / 'observed.jsonl'
            calls = []
            def observe(*args, **kwargs):
                calls.append(str(args[0]))
                return self.observe(*args, **kwargs)
            row = STREAM.attest_manifest(manifest, sealed['sha256'], root, output,
                                         inspect_file=observe, validate_path=RESIDENCY.validate_path,
                                         evict=True)
            observations = [json.loads(line) for line in output.read_text().splitlines()]
            self.assertEqual({item['path'] for item in observations}, {str(root / 'first'), str(root / 'second')})
            self.assertEqual(len(calls), 2)
            self.assertEqual(row['inventory']['rows'], 2)
            self.assertEqual(row['inventory']['physical_files'], 2)
            self.assertTrue(row['inventory']['complete'])
            self.assertIsNone(row['files'])
            self.assertEqual(row['eviction_hint_attempts'], 2)
            self.assertEqual(row['payload_bytes_read'], 0)
            import hashlib
            self.assertEqual(row['inventory']['sha256'], hashlib.sha256(output.read_bytes()).hexdigest())

    def test_identity_tamper_is_detected_before_eviction_or_mapping(self):
        with tempfile.TemporaryDirectory(prefix='r7-manifest-pin-') as directory:
            directory = Path(directory).resolve()
            root = directory / 'native'
            root.mkdir()
            (root / 'first').write_bytes(b'one')
            (root / 'second').write_bytes(b'two')
            manifest = directory / 'input.jsonl'
            sealed = GENERATOR.generate(root, manifest)
            first = json.loads(manifest.read_text().splitlines()[1])
            (root / first['path']).write_bytes(b'changed after seal')
            with patch.object(RESIDENCY.platform, 'system', return_value='Linux'), \
                 patch.object(RESIDENCY.os, 'posix_fadvise', create=True) as hint, \
                 patch.object(RESIDENCY.ctypes, 'CDLL') as mapping:
                with self.assertRaises(STREAM.ManifestFailure) as stopped:
                    STREAM.attest_manifest(manifest, sealed['sha256'], root, directory / 'out.jsonl',
                                           inspect_file=RESIDENCY.inspect_file,
                                           validate_path=RESIDENCY.validate_path, evict=True)
                self.assertIn('before eviction', str(stopped.exception))
                hint.assert_not_called()
                mapping.assert_not_called()
                self.assertFalse(stopped.exception.receipt['inventory']['complete'])
                self.assertEqual(stopped.exception.receipt['inventory']['rows'], 0)

    def test_manifest_seal_tamper_has_no_file_attempt_or_inventory_creation(self):
        with tempfile.TemporaryDirectory(prefix='r7-manifest-seal-') as directory:
            directory = Path(directory).resolve()
            root = directory / 'native'
            root.mkdir()
            (root / 'file').touch()
            manifest = directory / 'input.jsonl'
            sealed = GENERATOR.generate(root, manifest)
            with manifest.open('ab') as stream:
                stream.write(b'{}\n')
            with self.assertRaises(STREAM.ManifestFailure) as stopped:
                STREAM.attest_manifest(manifest, sealed['sha256'], root, directory / 'out.jsonl',
                                       inspect_file=lambda *_args, **_kw: self.fail('file attempt before seal'),
                                       validate_path=RESIDENCY.validate_path, evict=True)
            self.assertIn('SHA-256', str(stopped.exception))
            self.assertFalse(stopped.exception.receipt['inventory']['created'])

    def test_parent_symlink_escape_is_refused_by_anchored_open(self):
        with tempfile.TemporaryDirectory(prefix='r7-manifest-escape-') as directory:
            directory = Path(directory).resolve()
            root = directory / 'native'
            root.mkdir()
            outside = directory / 'outside'
            outside.mkdir()
            (outside / 'file').touch()
            (root / 'escape').symlink_to(outside, target_is_directory=True)
            scope = STREAM.RootedScope(root, RESIDENCY.validate_path)
            try:
                with self.assertRaises(OSError):
                    scope.open(root / 'escape' / 'file')
                with self.assertRaises(ValueError):
                    scope.relative('../outside/file')
            finally:
                scope.close()

    def test_declared_alias_is_explicit_and_not_evicted_again(self):
        with tempfile.TemporaryDirectory(prefix='r7-manifest-alias-') as directory:
            directory = Path(directory).resolve()
            root = directory / 'native'
            root.mkdir()
            (root / 'first').write_bytes(b'one')
            os.link(root / 'first', root / 'second')
            manifest = directory / 'input.jsonl'
            sealed = GENERATOR.generate(root, manifest)
            with self.assertRaises(STREAM.ManifestFailure):
                STREAM.attest_manifest(manifest, sealed['sha256'], root, directory / 'rejected.jsonl',
                                       inspect_file=self.observe, validate_path=RESIDENCY.validate_path, evict=True)
            row = STREAM.attest_manifest(manifest, sealed['sha256'], root, directory / 'allowed.jsonl',
                                         inspect_file=self.observe, validate_path=RESIDENCY.validate_path,
                                         evict=True, allow_aliases=True)
            self.assertEqual(row['inventory']['rows'], 2)
            self.assertEqual(row['inventory']['physical_files'], 1)
            self.assertEqual(row['inventory']['declared_aliases'], 1)
            self.assertEqual(row['eviction_hint_attempts'], 1)
            raw = [json.loads(line) for line in (directory / 'allowed.jsonl').read_text().splitlines()]
            self.assertEqual(raw[1]['observation_kind'], 'declared_alias')
            self.assertFalse(raw[1]['included_in_aggregate'])
            self.assertEqual(row['resident_pages'], 0)

    @unittest.skipUnless(RESIDENCY.platform.system() == 'Linux', 'Linux mincore required')
    def test_real_nonempty_mincore_detects_read_resident_page_without_eviction(self):
        with tempfile.TemporaryDirectory(prefix='r7-cache-hot-page-') as directory:
            path = Path(directory) / 'one-page'
            page_size = os.sysconf('SC_PAGE_SIZE')
            path.write_bytes(bytes(page_size))
            # Independent test preconditioning, not observer payload access.
            with path.open('rb', buffering=0) as stream:
                self.assertEqual(len(stream.read(page_size)), page_size)
            row = RESIDENCY.attest([path], evict=False)
            item = row['files'][0]
            self.assertEqual(item['total_pages'], 1)
            self.assertEqual(item['mmap_calls'], 1)
            self.assertEqual(item['mincore_calls'], 1)
            self.assertEqual(item['munmap_calls'], 1)
            self.assertEqual(item['resident_pages'], 1)
            self.assertEqual(item['eviction_hint_attempts'], 0)
            self.assertEqual(row['payload_bytes_read'], 0)


if __name__ == '__main__':
    unittest.main()

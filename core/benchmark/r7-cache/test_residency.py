"""External harness contract tests; the real Linux primitive is separately gated."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('r7_residency', Path(__file__).with_name('residency.py'))
RESIDENCY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RESIDENCY)


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


if __name__ == '__main__':
    unittest.main()

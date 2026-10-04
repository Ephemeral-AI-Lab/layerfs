"""Profile-aware history facade and fail-closed independent proof inputs."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'core/benchmark/fs-bench-pro/diagnostics'))
from history_reference_vehicle import generate, generate_verifier, BASE

class HistoryVehicle(unittest.TestCase):
    def test_reference_facade_uses_unmodified_memory_engine_and_same_state_stages(self):
        source, seals = generate(ROOT)
        self.assertEqual(len(seals), 7)
        self.assertNotIn('handles.', source)
        self.assertNotIn('SqlitePersistenceProfile', source)
        self.assertIn('phase4.5-memory-off', source)
        self.assertIn('HISTORY_STATE_WORK', source)
        self.assertIn('StoreProvider::new(&storage)', source)

    def test_complete_proof_rejects_missing_pins_bad_roots_profile_and_census_before_io(self):
        binary = ROOT / 'core/target/release/examples/verify_history'
        self.assertTrue(binary.is_file(), 'build the needed release/locked verifier first')
        child = dict(selected_states=17, states=17, roots=['01' * 32] * 17,
                     profile_identity='sqlite-memory-off-macos-v1',
                     canonical_objects=51689, canonical_bytes=380559460)
        pins = dict(kind='matched-phase4.5-root-pins-v1', source_commit=BASE,
                    row='history-stride10', roots=child['roots'])
        with tempfile.TemporaryDirectory(prefix='layerfs-history-proof-input-') as tmp:
            folder = Path(tmp)
            receipt = folder / 'receipt.json'
            pinfile = folder / 'pins.json'
            cases = [
                ('complete proof requires independent root pins', child, None, 'disposable'),
                ('independent retained root mismatch', child,
                 {**pins, 'roots': ['02' * 32] * 17}, 'disposable'),
                ('producer effective profile identity mismatch', child, None, 'durable'),
                ('complete canonical census mismatch', {**child, 'canonical_objects': 1}, pins, 'disposable'),
                ('independent reference provenance missing', child,
                 {**pins, 'source_commit': 'unqualified'}, 'disposable'),
            ]
            for message, value, expected, profile in cases:
                receipt.write_text(json.dumps({'run': {'child': copy.deepcopy(value)}}))
                command = [str(binary), str(folder/'absent-corpus'), str(folder/'absent-db'),
                           str(receipt), 'history-stride10', 'complete', profile]
                if expected is not None:
                    pinfile.write_text(json.dumps(expected)); command.append(str(pinfile))
                result = subprocess.run(command, capture_output=True, text=True, timeout=9.5)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertFalse((folder/'absent-db').exists())

    def test_reference_proof_facade_seals_readback_and_removes_candidate_ports(self):
        source, helper, seals = generate_verifier(ROOT, Path('/sealed/reference-helper.rs'))
        self.assertEqual(len(seals), 6)
        self.assertNotIn('handles.', source)
        self.assertNotIn('layerfs_persistence', source)
        self.assertNotIn('PackPersistence', helper)
        self.assertIn('closed reference census disagrees with producer', source)
        self.assertIn('sqlite::open_read_only', source)
        self.assertIn('StoreProvider::new(&storage)', source)
        self.assertIn('self.persistence.get(id)', helper)

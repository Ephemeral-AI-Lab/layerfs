"""Closed-file census and evidence provenance guards; synthetic unit fixtures only."""
import copy
import hashlib
import json
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import phase7_history_proof as p


class HistoryProof(unittest.TestCase):
    def fixture(self, folder):
        path = folder / 'fixture.sqlite'
        with sqlite3.connect(path) as db:
            db.executescript('PRAGMA application_id=1279677264; PRAGMA user_version=1;')
            for table in sorted(p.COMBINED):
                columns = 'object_id BLOB, role INTEGER, canonical_length INTEGER' if table == 'object_location' else ('root_id BLOB' if table == 'layer' else ('body BLOB' if table == 'pack' else 'id INTEGER'))
                db.execute(f'CREATE TABLE "{table}" ({columns})')
            for table, count in {'history_meta': 1, 'layer_stack': 1, 'branch': 17, 'commit': 16}.items():
                db.executemany(f'INSERT INTO "{table}" VALUES (?)', [(i,) for i in range(count)])
            db.executemany('INSERT INTO layer VALUES (?)', [(bytes([i]) * 32,) for i in range(1, 18)])
            db.execute('INSERT INTO object_location VALUES (?,1,7)', (b'x' * 32,))
        inventory = hashlib.sha256(b'x' * 32 + b'\x01' + (7).to_bytes(8, 'little')).hexdigest()
        child = {'states': 17, 'selected_states': 17, 'roots': [f'{i:02x}' * 32 for i in range(1, 18)],
                 'canonical_objects': 1, 'canonical_bytes': 7, 'canonical_inventory_sha256': inventory}
        return path, child

    def test_closed_census_preserves_owner_and_rejects_stale_roots_and_id_inventory(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(p.CANONICAL, {'history-stride10': (1, 7)}):
            path, child = self.fixture(Path(tmp)); before = p.digest(path)
            result = p.collect(path, 'candidate', child, 'history-stride10')
            self.assertEqual(result['status'], 'CHECKED'); self.assertEqual(result['canonical_bytes'], 7)
            self.assertEqual(p.digest(path), before)
            self.assertEqual(result['owners'][0]['sha256_before'], result['owners'][0]['sha256_after'])
            bad = {**child, 'roots': ['ff' * 32] * 17}
            with self.assertRaisesRegex(ValueError, 'retained roots'): p.collect(path, 'candidate', bad, 'history-stride10')
            bad = {**child, 'canonical_inventory_sha256': 'bad'}
            with self.assertRaisesRegex(ValueError, 'inventory'): p.collect(path, 'candidate', bad, 'history-stride10')
            Path(str(path) + '-wal').touch()
            with self.assertRaisesRegex(ValueError, 'sidecars'): p.collect(path, 'candidate', child, 'history-stride10')

    def test_independent_pins_rederived_from_sealed_evidence_and_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            child = {'status': 'COMPLETE', 'profile_identity': 'phase4.5-memory-off', 'states': 17,
                     'roots': ['01' * 32] * 17}
            receipt = {'arm': 'baseline', 'measured_source_commit': p.BASE, 'status': 'COMPLETE',
                       'cache_status': 'PASS', 'cleanup': {'status': 'PASS'}, 'sample_count': 1,
                       'verification_status': 'PASS', 'command_wall_ns': 1, 'command_budget_ns': 60_000_000_000,
                       'verification_wall_ns': 1, 'performance': {'child': child}, 'workload_row': 'history-stride10',
                       'identity': {'harness_seal': 'unit-fixture'}, 'build': {'binaries': {'driver': 'unit-fixture'}}}
            census = {'status': 'CHECKED', 'canonical_objects': 51689, 'canonical_bytes': 380559460}
            proof = {'status': 'CHECKED', 'states': 17, 'custody_states': 17}
            paths = [folder / f'{name}.json' for name in ('receipt', 'census', 'proof')]
            for path, value in zip(paths, (receipt, census, proof)): path.write_text(json.dumps(value))
            pins = p.root_pins(receipt, census, proof, *paths)
            self.assertEqual(p.validate_pins(pins, receipt['identity'])['proof'], proof)
            bad = copy.deepcopy(pins); bad['roots'][0] = '02' * 32
            with self.assertRaisesRegex(ValueError, 'do not match'): p.validate_pins(bad, receipt['identity'])
            bad_receipt = {**receipt, 'verification_wall_ns': 9_500_000_001}
            with self.assertRaisesRegex(ValueError, 'budget'): p.root_pins(bad_receipt, census, proof, *paths)
            bad_receipt = {**receipt, 'cache_status': 'INELIGIBLE'}
            with self.assertRaisesRegex(ValueError, 'cold'): p.root_pins(bad_receipt, census, proof, *paths)
            paths[2].write_text('{}')
            with self.assertRaisesRegex(ValueError, 'hash'): p.validate_pins(pins, receipt['identity'])

    def test_reference_completed_save_rows_are_not_unfinished(self):
        # Original native schema retains publication rows after active_slot clears.
        with sqlite3.connect(':memory:') as db:
            db.execute('CREATE TABLE saves(active_slot INTEGER, publication INTEGER)')
            db.executemany('INSERT INTO saves VALUES(NULL,?)', [(n,) for n in range(1,18)])
            self.assertEqual(p.unfinished_saves(db),0)
            db.execute('INSERT INTO saves VALUES(1,NULL)')
            self.assertEqual(p.unfinished_saves(db),1)

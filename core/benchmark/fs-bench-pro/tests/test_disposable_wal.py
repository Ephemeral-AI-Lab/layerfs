"""WAL matrix selection, retained evidence and execution policy checks."""
import copy
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import phase7_sqlite, phase7_history
from shared import disposable_wal as d


class DisposableWalTests(unittest.TestCase):
    def test_exact_order_workloads_and_existing_limits(self):
        cases = [phase7_sqlite.CASES[name] for name, row in d.ROWS.items() if 'supersedes' not in row]
        self.assertEqual(len(cases), 7)
        self.assertEqual([phase7_sqlite.init.CASES[c.fixture].files for c in cases[:4]],
                         [100, 1000, 10000, 100000])
        self.assertEqual([c.states for c in cases[4:]], [17, 53, 157])
        self.assertEqual([c.command_budget_ns // 10**9 for c in cases], [30]*4+[60,170,300])
        self.assertEqual([c.verification_budget_ns // 10**9 for c in cases], [19]*4+[12,12,30])
        self.assertTrue(all(c.profile == 'disposable' for c in cases))

    def test_cold_correction_preserves_original_workload_and_limits(self):
        old = 'phase7-sqlite-disposable-init-100000-owner-wal-v1'
        new = old.removesuffix('-v1') + '-v2'
        row = d.ROWS[new]
        self.assertEqual(row['supersedes'], old)
        self.assertEqual(row['prepared_root'], 'disposable-wal-prepared.noindex')
        self.assertEqual({k: v for k, v in row.items() if k not in ('id', 'prepared_root', 'supersedes')},
                         {k: v for k, v in d.ROWS[old].items() if k != 'id'})

    def test_durable_refused_before_any_work(self):
        c = phase7_sqlite.CASES['phase7-sqlite-history-stride10-group-rows-indexed-v2']
        with self.assertRaisesRegex(ValueError, 'disabled by owner'):
            phase7_sqlite.run(c.id, None, 'candidate', None, None)
        with self.assertRaisesRegex(ValueError, 'disabled by owner'):
            phase7_history.run(c, None, 'candidate', None, None)

    def test_new_baseline_arm_refused(self):
        for name in d.ROWS:
            with self.assertRaisesRegex(ValueError, 'historical references'):
                phase7_sqlite.run(name, None, 'baseline', None, None)

    def test_retained_receipts_and_independent_roots(self):
        for name, row in d.ROWS.items():
            ref = d.reference(name)
            self.assertEqual(ref['verification_status'], 'PASS')
            if row['states']:
                pins = d.root_pins(name)
                self.assertEqual(pins['kind'], d.PINS_KIND)
                self.assertEqual(len(pins['roots']), row['states'])
                d.validate_pins(pins, name)

    def test_changed_reference_hash_is_refused(self):
        name = next(iter(d.ROWS))
        bad = copy.deepcopy(d.ROWS[name])
        bad['reference']['sha256'] = '0'*64
        with patch.dict(d.ROWS, {name: bad}), self.assertRaisesRegex(ValueError, 'hash mismatch'):
            d.reference(name)

    def test_changed_root_or_provenance_is_refused(self):
        name = list(d.ROWS)[4]
        for field in ('roots', 'historical_identity', 'retained_receipt'):
            pins = d.root_pins(name)
            pins[field] = None
            with self.assertRaisesRegex(ValueError, 'differ'):
                d.validate_pins(pins, name)

    def test_comparison_keeps_historical_arithmetic_out_of_admission(self):
        name = list(d.ROWS)[4]
        ref = d.reference(name)
        result = d.comparison({'comparison_ns': ref['comparison_ns'],
                               'storage_bytes': d.ROWS[name]['storage_ceiling']}, name)
        self.assertFalse(result['admission_eligible'])
        self.assertEqual(result['historical_speed_arithmetic'], 'PASS')
        self.assertEqual(result['allocation_gate'], 'PASS')
        self.assertIn('incumbent', result)


if __name__ == '__main__':
    unittest.main()

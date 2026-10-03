import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import phase7_storage as family
from shared import phase7 as contract
import runner


class Phase7(unittest.TestCase):
    def test_registered_seven_cases_and_exact_bounds(self):
        cases = list(family.CASES.values())
        self.assertEqual(len(cases), 7)
        self.assertEqual([runner.init.CASES[c.fixture].logical_bytes for c in cases[:4]],
                         [5_000_000, 20_000_000, 300_000_000, 500_000_000])
        self.assertEqual([c.states for c in cases[4:]], [17, 53, 157])
        self.assertEqual([c.storage_ceiling for c in cases[4:]], [54_278_964, 70_427_034, 92_342_273])
        self.assertTrue(all(c.storage_ceiling is None for c in cases[:4]))
        self.assertEqual([c.command_budget_ns // 10**9 for c in cases], [15]*4+[60,170,170])
        self.assertEqual([c.verification_budget_ns for c in cases[4:]], [10_000_000_000,20_000_000_000,30_000_000_000])

    def row(self, time=10, storage=99):
        return dict(cache_status='PASS', status='COMPLETE', verification_status='PASS',
                    command_wall_ns=10, command_budget_ns=15, verification_wall_ns=1,
                    verification_budget_ns=2, comparison_ns=time, storage_bytes=storage)

    def test_strict_time_storage_and_missing_init_gate(self):
        baseline = self.row(10)
        self.assertEqual(contract.strict_gate(self.row(9),baseline,100), 'PASS')
        self.assertEqual(contract.strict_gate(self.row(10),baseline,100), 'FAIL')
        self.assertEqual(contract.strict_gate(self.row(11),baseline,100), 'FAIL')
        self.assertEqual(contract.strict_gate(self.row(9,100),baseline,100), 'FAIL')
        self.assertEqual(contract.strict_gate(self.row(9),baseline,None), 'INCOMPLETE')
        for key, value, expected in [('cache_status','FAIL','INELIGIBLE'),
                                    ('verification_status','FAIL','INCOMPLETE'),
                                    ('storage_bytes',None,'INCOMPLETE'),
                                    ('comparison_ns',None,'INCOMPLETE'),
                                    ('command_wall_ns',16,'FAIL'),
                                    ('verification_wall_ns',3,'FAIL')]:
            row = self.row(9); row[key] = value
            self.assertEqual(contract.strict_gate(row, baseline,100),expected)

    def test_whole_input_residency_recheck_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory); (path/'a').write_bytes(b'abc'); (path/'b').write_bytes(b'')
            with patch.object(contract.residency,'de_warm',return_value=SimpleNamespace(resident_first=1,invalidated=True)) as invalidation, patch.object(contract.residency,'residency',return_value=SimpleNamespace(resident_pages=1,total_pages=1,length_bytes=3)) as check:
                result = contract.dewarm_tree(path)
            self.assertEqual(result['status'],'INELIGIBLE')
            self.assertEqual(result['resident_after'],2)
            self.assertEqual(invalidation.call_count,2)
            self.assertEqual(check.call_count,2)
            (path/'symlink').symlink_to(path/'a')
            with self.assertRaisesRegex(ValueError,'symlink'):
                contract.dewarm_tree(path)

    def test_timer_contains_engine_connections_and_creation(self):
        text = (runner.CORE/'crates/layerfs-project/examples/benchmark_init.rs').read_text()
        start = text.index('let start = Instant::now()')
        end = text.index('let operation_ns = start.elapsed()')
        for call in ('PgMetadata::open(', 'PgHistory::open_writable(', 'S3Objects::connect_parallel(', 'Storage::new(', '\n        init('):
            self.assertTrue(start < text.index(call) < end)
        self.assertIn('Duration::from_secs(15)',text)
        self.assertNotIn('Duration::from_secs(600)',text)

    def test_cache_contract_does_not_accept_existing_data(self):
        with patch.object(contract.services,'postgres_sql',return_value='1\n'):
            with self.assertRaisesRegex(ValueError,'populated'):
                contract.empty_services()

"""Product-free admission checks for the one prospective Init decision."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import phase7_sqlite, serverless_init


class ServerlessInitTests(unittest.TestCase):
    def test_registration_keeps_the_frozen_limits_and_one_disposable_case(self):
        case = phase7_sqlite.CASES[phase7_sqlite.SERVERLESS_WAL_CASE]
        self.assertEqual(case.fixture, 'namespace-1000-compact-v3')
        self.assertEqual(case.profile, 'disposable')
        self.assertEqual((case.command_budget_ns, case.verification_budget_ns),
                         (30_000_000_000, 19_000_000_000))
        self.assertEqual(sum('serverless-wal' in key for key in phase7_sqlite.CASES), 1)
        self.assertEqual(phase7_sqlite.PROFILE_IDS['disposable'], 'sqlite-memory-off-macos-v1')
        prior = 'phase7-sqlite-disposable-init-1000-incumbent-restored-v1'
        self.assertIn(prior, phase7_sqlite.RETIRED_ALLOCATION_INIT)
        with self.assertRaisesRegex(ValueError, 'mechanism removed'):
            phase7_sqlite.run(prior, None, 'candidate', None, None)

    def test_only_exact_owner_named_noninputs_are_exempt_and_dirtiness_is_preserved(self):
        identity = {'source_dirty': True, 'dirty_paths': ['?? ' + p for p in serverless_init.NONINPUTS]}
        original = copy.deepcopy(identity)
        serverless_init.check_scope(identity)
        self.assertEqual(identity, original)
        for line in [' M core/crates/a/src/lib.rs', '?? core/crates/a/src/lib.rs',
                     ' M core/docs/issues/307/HANDOFF-PRE-S8-SERVERLESS-20261007.md',
                     '?? unexpected.md']:
            with self.assertRaises(ValueError):
                serverless_init.check_scope({'dirty_paths': [line]})

    def test_control_identity_is_checked_from_retained_bytes(self):
        root = Path(__file__).resolve().parents[4]
        retained = serverless_init.control(root)
        self.assertEqual(retained['comparison_ns'], 155_291_459)
        self.assertEqual(retained['fixture']['files'], 1000)

    def test_decision_keeps_import_route_speed_and_retired_storage_gate_distinct(self):
        reference = {'comparison_ns': 100, 'storage_bytes': 1000}
        row = {'status': 'COMPLETE', 'cache_status': 'PASS', 'verification_status': 'PASS',
               'cleanup_status': 'PASS', 'comparison_ns': 110, 'storage_bytes': 950,
               'command_wall_ns': 1, 'command_budget_ns': 30,
               'verification_wall_ns': 1, 'verification_budget_ns': 19}
        result = serverless_init.decision(row, reference)
        self.assertEqual(result['speed_gate'], 'PASS')
        self.assertEqual(result['import_route'], 'memory-import-wal-seal-required')
        self.assertEqual(result['storage_delta_percent'], -5)
        self.assertEqual(result['strict_allocation_status'], 'NOT_RUN — mechanism removed')
        row['comparison_ns'] = 111
        self.assertEqual(serverless_init.decision(row, reference)['speed_gate'], 'FAIL')
        row['comparison_ns'] = 100
        self.assertEqual(serverless_init.decision(row, reference)['import_route'], 'wal-throughout')
        for key in ['cache_status', 'verification_status', 'cleanup_status']:
            refused = {**row, key: 'INELIGIBLE'}
            self.assertIsNone(serverless_init.decision(refused, reference)['import_route'])
        row['command_wall_ns'] = 31
        self.assertIsNone(serverless_init.decision(row, reference)['import_route'])


if __name__ == '__main__':
    unittest.main()

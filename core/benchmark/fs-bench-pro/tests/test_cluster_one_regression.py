"""Reference identity, retired format and full same-profile regression selection."""
import sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from families import phase7_sqlite as f
from shared import cluster_one_control as c
ROOT=Path(__file__).resolve().parents[4]
class ClusterOneRegression(unittest.TestCase):
    def test_all_eight_cases_keep_workload_profile_and_caps(self):
        self.assertEqual(len(f.REGRESSION_CASES),8)
        for profile,names in f.REGRESSION_CASES_BY_PROFILE.items():
            self.assertEqual([f.init.CASES[f.CASES[name].fixture].files for name in names],[100,1000,10000,100000])
            for name in names:
                case=f.CASES[name]
                self.assertEqual((case.profile,case.pack_layout,case.command_budget_ns,case.verification_budget_ns),(profile,'monolithic',30_000_000_000,19_000_000_000))
    def test_original_candidate_is_new_control_and_old_verdict_is_retained(self):
        for profile,names in f.REGRESSION_CASES_BY_PROFILE.items():
            for name in names:
                ref=c.reference(ROOT,profile,f.CASES[name].fixture)
                self.assertEqual(ref['source_commit'],f.CLUSTER_ONE_END)
                self.assertEqual(ref['original_arm'],'candidate')
                self.assertEqual(ref['original_pair_verdict'],'FAIL' if profile=='durable' else 'PASS')
                self.assertNotEqual(ref['source_commit'],ref['old_competitive_reference']['source'])
                self.assertEqual(ref['effective_profile']['fullfsync'],1 if profile=='durable' else 0)
    def test_retired_payload_vehicle_and_wrong_reference_execution_are_refused(self):
        with self.assertRaisesRegex(ValueError,'withdrawn'):f.run(f.PAYLOAD_SEGMENT_CASE,None,'candidate',None,None)
        with self.assertRaisesRegex(ValueError,'never the old Service'):f.run(f.REGRESSION_CASES[0],None,'baseline',None,None)

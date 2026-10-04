"""Prospective schema2 selection preserves cold/proof/storage budgets."""
import sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from families.phase7_sqlite import CASES,GROUP_ROW_CASES,REQUIRED_BY_PROFILE,LITE_PROOF_POLICY
class GroupRows(unittest.TestCase):
    def test_explicit_group_layout_new_identity_preserves_existing_gates(self):
        self.assertEqual(len(GROUP_ROW_CASES),8)
        for name in GROUP_ROW_CASES:
            c=CASES[name]
            old=next(CASES[n] for n in REQUIRED_BY_PROFILE[c.profile] if CASES[n].states==c.states)
            self.assertEqual(c.pack_layout,'group-rows');self.assertEqual(old.pack_layout,'monolithic')
            self.assertNotEqual(c.id,old.id)
            for field in ('storage_ceiling','command_budget_ns','proof_policy'):
                self.assertEqual(getattr(c,field),getattr(old,field))
            self.assertEqual(c.proof_policy,LITE_PROOF_POLICY)
            self.assertEqual(c.verification_budget_ns,15_000_000_000 if c.proof_envelope=='owner-stride1-proof15-v2' else 12_000_000_000)
            if c.proof_envelope=='owner-stride1-proof15-v2':self.assertEqual(c.states,157)
if __name__=='__main__':unittest.main()

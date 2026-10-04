"""History boundary and preflight validation; no product speed sample."""
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from families.phase7_history import boundaries


class HistoryRunner(unittest.TestCase):
    def fixture(self):
        rows=[{'before_state':n,'wall_ns':10,'attestation':{'files':2,'resident_after':0}} for n in range(2,19)]
        return rows
    def text(self,rows):
        return '\n'.join(['HISTORY_COLD_BOUNDARY '+json.dumps(r) for r in rows]+['HISTORY_COLD_TOTAL '+json.dumps({'checks':17,'wall_ns':170})])
    def test_full_boundaries_and_total_are_required(self):
        rows=self.fixture();self.assertEqual(boundaries(self.text(rows),17,'baseline')['status'],'PASS')
        with self.assertRaisesRegex(ValueError,'missing or out of order'):boundaries(self.text(rows[:-1]),17,'baseline')
        rows[0]['attestation']['resident_after']=1
        with self.assertRaisesRegex(ValueError,'INELIGIBLE'):boundaries(self.text(rows),17,'baseline')
    def test_split_reference_owners_and_consistent_totals(self):
        rows=self.fixture();rows[0]['attestation']['files']=1
        with self.assertRaisesRegex(ValueError,'residency'):boundaries(self.text(rows),17,'baseline')
        self.assertEqual(boundaries(self.text(rows),17,'candidate')['status'],'PASS')
        rows[0]['wall_ns']=11
        with self.assertRaisesRegex(ValueError,'total mismatch'):boundaries(self.text(rows),17,'candidate')

    def test_indexed_selection_keeps_the_approved_stride1_envelope(self):
        from families.phase7_sqlite import CASES, INDEXED_GROUP_ROW_CASES, LITE_PROOF_POLICY
        self.assertEqual(len(INDEXED_GROUP_ROW_CASES),2)
        for name in INDEXED_GROUP_ROW_CASES:
            c=CASES[name]
            self.assertEqual(c.pack_layout,'group-rows-indexed')
            self.assertEqual(c.states,157)
            self.assertEqual(c.command_budget_ns,300_000_000_000)
            self.assertEqual(c.verification_budget_ns,30_000_000_000)
            self.assertEqual(c.storage_ceiling,92_342_273)
            self.assertEqual(c.proof_policy,LITE_PROOF_POLICY)

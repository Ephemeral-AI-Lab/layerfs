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

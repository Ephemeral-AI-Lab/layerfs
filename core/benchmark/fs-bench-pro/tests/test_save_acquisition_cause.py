"""Counter scope/coverage guards; no performance samples."""
import json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from diagnostics.run_save_acquisition_cause import phase_counts
class SaveAcquisitionCounts(unittest.TestCase):
    def rows(self):
        rows=[]
        for state in (1,2):
            for stage in ('acquisition','construction','filesystem','save_custody'):
                values=[0]*19;values[11]=state*100;values[15]=state
                rows.append('HISTORY_ENGINE_WORK '+json.dumps(dict(state=state,stage=stage,available=True,values=values)))
        return rows
    def test_disjoint_deltas_sum_once_and_ignore_provider_snapshots(self):
        rows=self.rows()+['HISTORY_PROVIDER_WORK cumulative=999999']
        result=phase_counts('\n'.join(rows),2)
        self.assertEqual(result['save_custody'],dict(states=2,vfs_requested_bytes=300,pack_acquisitions=3))
    def test_missing_duplicate_and_unavailable_phases_fail_closed(self):
        for rows in (self.rows()[:-1],self.rows()+[self.rows()[0]],
                     [r.replace('"available": true','"available": false') for r in self.rows()]):
            with self.assertRaises(ValueError):phase_counts('\n'.join(rows),2)
if __name__=='__main__':unittest.main()

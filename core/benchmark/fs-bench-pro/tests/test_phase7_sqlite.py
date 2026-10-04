"""Exact acceptance arithmetic, registry/limits and actual child accounting."""
import sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from families import phase7_sqlite as f
from shared import sqlite_contract as c
class SqliteStep10(unittest.TestCase):
    def row(self,n,storage=100):
        return dict(cache_status='PASS',status='COMPLETE',verification_status='PASS',cleanup={'status':'PASS'},command_wall_ns=1,command_budget_ns=15,verification_wall_ns=1,verification_budget_ns=2,comparison_ns=n,storage_bytes=storage)
    def test_seven_required_selections_and_original_history_gates(self):
        cases=[f.CASES[x] for x in f.REQUIRED];self.assertEqual(len(cases),7)
        self.assertEqual([f.init.CASES[x.fixture].files for x in cases[:4]],[100,1000,10000,100000])
        self.assertEqual([x.states for x in cases[4:]],[17,53,157])
        self.assertEqual([x.storage_ceiling for x in cases[4:]],[54278964,70427034,92342273])
        self.assertEqual([x.command_budget_ns for x in cases[4:]],[60000000000,170000000000,300000000000])
        self.assertIn("owner-2026-10-04",f.HISTORY_BUDGET_RULE)
        self.assertTrue(all(x.verification_budget_ns==9500000000 for x in cases));self.assertEqual(f.REQUIRED_BY_PROFILE["durable"],f.REQUIRED)
        disposable=[f.CASES[x] for x in f.REQUIRED_BY_PROFILE["disposable"]]
        self.assertEqual(len(disposable),7)
        self.assertTrue(all(x.profile=="disposable" for x in disposable))
        self.assertEqual([(x.fixture,x.states,x.storage_ceiling,x.command_budget_ns) for x in disposable],[(x.fixture,x.states,x.storage_ceiling,x.command_budget_ns) for x in cases])
        self.assertEqual(len(f.CASES),24)
        self.assertEqual(len(f.LEGACY_HISTORY),10)
        self.assertTrue(all(f.CASES[name].states for name in f.LEGACY_HISTORY))
    def test_stride1_increase_is_prospective_and_does_not_change_proof_or_old_cases(self):
        self.assertEqual(len(f.RETIRED_STRIDE1),2)
        for old in f.RETIRED_STRIDE1:
            new=old.rsplit('-v',1)[0]+'-v3'
            self.assertEqual(f.CASES[old].command_budget_ns,170000000000)
            self.assertEqual(f.CASES[new].command_budget_ns,190000000000)
            self.assertEqual(f.CASES[new].verification_budget_ns,9500000000)
            self.assertEqual(f.CASES[old].storage_ceiling,f.CASES[new].storage_ceiling)
            self.assertIn(new,f.LEGACY_HISTORY)
            with self.assertRaisesRegex(ValueError,'historical history selection'):
                f.run(old,None,'baseline',None,None)
    def test_300_seconds_is_prospective_symmetric_and_preserves_190_seconds(self):
        self.assertEqual(len(f.RETIRED_STRIDE1_V3),2)
        for old in f.RETIRED_STRIDE1_V3:
            new=old.rsplit('-v',1)[0]+'-v4'
            self.assertEqual(f.CASES[old].command_budget_ns,190000000000)
            self.assertEqual(f.CASES[new].command_budget_ns,300000000000)
            self.assertEqual(f.CASES[new].verification_budget_ns,9500000000)
            self.assertEqual(f.CASES[old].storage_ceiling,f.CASES[new].storage_ceiling)
            self.assertIn(new,f.REQUIRED_BY_PROFILE[f.CASES[new].profile])
            with self.assertRaisesRegex(ValueError,'historical history selection'):
                f.run(old,None,'baseline',None,None)
    def test_margin_is_integer_total_bound_and_joint_gate_cannot_waive_missing_proof(self):
        self.assertEqual(c.gate(self.row(110),self.row(100),100),'PASS')
        self.assertEqual(c.gate(self.row(111),self.row(100),100),'FAIL')
        self.assertEqual(c.gate(self.row(110,101),self.row(100),100),'FAIL')
        self.assertEqual(c.gate(self.row(110),self.row(100),100,exclusive_storage=True),'FAIL')
        self.assertEqual(c.gate(self.row(110),self.row(100),None),'INCOMPLETE')
        row=self.row(99);row['cache_status']='INELIGIBLE';self.assertEqual(c.gate(row,self.row(100),100),'INELIGIBLE')
        row=self.row(99);row['verification_status']='FAIL';self.assertEqual(c.gate(row,self.row(100),100),'INCOMPLETE')
    def test_unresolved_contract_refuses_before_build_setup_or_sample(self):
        with patch.object(f,'INIT_ALLOCATION_RULE',None):
            with self.assertRaisesRegex(ValueError,'allocation'):f.run(f.REQUIRED[0],None,'candidate',None,None)
        with self.assertRaisesRegex(ValueError,'qualified reference pins'):f.run(f.REQUIRED[4],None,'candidate',None,None)
    def test_wait4_is_per_child_and_watchdog_keeps_failed_output(self):
        with tempfile.TemporaryDirectory() as d:
            out=Path(d)
            r=f.invoke([sys.executable,'-c','import json; print(json.dumps({"status":"COMPLETE"}))'],out,'small',2000000000,{},d)
            self.assertEqual(r['exit_code'],0);self.assertFalse(r['timed_out']);self.assertEqual(r['child']['status'],'COMPLETE');self.assertGreater(r['wall_ns'],0);self.assertGreater(r['peak_rss_bytes'],0)
            r=f.invoke([sys.executable,'-c','import time; time.sleep(1)'],out,'timeout',50000000,{},d)
            self.assertTrue(r['timed_out']);self.assertNotEqual(r['exit_code'],0);self.assertTrue((out/'timeout.stderr').exists())

    def test_watchdog_terminates_descendant_observers(self):
        import time
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp); marker=out/'orphan-wrote'
            child_code='import time; from pathlib import Path; time.sleep(0.2); Path('+repr(str(marker))+').touch()'
            parent_code='import subprocess,time; subprocess.Popen(['+repr(sys.executable)+',"-c",'+repr(child_code)+']); time.sleep(2)'
            result=f.invoke([sys.executable,'-c',parent_code],out,'tree-timeout',100000000,{},tmp)
            self.assertTrue(result['timed_out']); time.sleep(0.3)
            self.assertFalse(marker.exists(),'descendant survived measured-command timeout')

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
        self.assertTrue(all(x.verification_budget_ns==9500000000 for x in cases[:4]));self.assertTrue(all(x.verification_budget_ns==12000000000 for x in cases[4:]));self.assertEqual(f.REQUIRED_BY_PROFILE["durable"],f.REQUIRED)
        disposable=[f.CASES[x] for x in f.REQUIRED_BY_PROFILE["disposable"]]
        self.assertEqual(len(disposable),7)
        self.assertTrue(all(x.profile=="disposable" for x in disposable))
        self.assertEqual([(x.fixture,x.states,x.storage_ceiling,x.command_budget_ns) for x in disposable],[(x.fixture,x.states,x.storage_ceiling,x.command_budget_ns) for x in cases])
        self.assertGreaterEqual(len(f.CASES),30)
        self.assertEqual(len(f.LEGACY_HISTORY),16)
        self.assertTrue(all(f.CASES[name].states for name in f.LEGACY_HISTORY))
    def test_owner_closure_caps_are_exact_doubles_and_history_is_preserved(self):
        for profile,names in f.OWNER_CLOSURE_CASES_BY_PROFILE.items():
            self.assertEqual(len(names),7 if profile=='durable' else 4)
            for name in names:
                c=f.CASES[name]
                if c.states is None:
                    self.assertEqual((c.command_budget_ns,c.verification_budget_ns),(30_000_000_000,19_000_000_000))
                else:
                    old=f.CASES[name.rsplit('-v',1)[0]+'-v1']
                    self.assertEqual(c.command_budget_ns,2*old.command_budget_ns)
                    self.assertEqual(c.verification_budget_ns,2*old.verification_budget_ns)
                    self.assertEqual(c.storage_ceiling,old.storage_ceiling)
                    self.assertEqual(c.proof_policy,old.proof_policy)
                    self.assertEqual(c.proof_envelope,'owner-double-caps-20261005-v2')
    def test_rejected_wal_reservation_cannot_be_replayed_after_withdrawal(self):
        self.assertEqual(len(f.RETIRED_WAL_RESERVATION_CASES),8)
        for name in f.RETIRED_WAL_RESERVATION_CASES:
            self.assertIn(name,f.CASES)
            with self.assertRaisesRegex(ValueError,'rejected WAL reservation selection retired'):
                f.run(name,None,'candidate',None,None)
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
            self.assertIn(new,f.LEGACY_HISTORY)
            with self.assertRaisesRegex(ValueError,'historical history selection'):
                f.run(old,None,'baseline',None,None)
    def test_lite_scope_is_new_and_symmetric_with_old_proofs_preserved(self):
        self.assertEqual(len(f.RETIRED_FULL_CONTENT),6)
        for old in f.RETIRED_FULL_CONTENT:
            new=old.rsplit('-v',1)[0]+f'-v{int(old.rsplit("-v",1)[1])+1}'
            self.assertEqual(f.CASES[old].verification_budget_ns,9500000000)
            self.assertEqual(f.CASES[new].verification_budget_ns,12000000000)
            self.assertEqual(f.CASES[new].proof_policy,f.LITE_PROOF_POLICY)
            self.assertEqual(f.CASES[old].command_budget_ns,f.CASES[new].command_budget_ns)
            self.assertIn(new,f.REQUIRED_BY_PROFILE[f.CASES[new].profile])
            with self.assertRaisesRegex(ValueError,'historical history selection'):
                f.run(old,None,'baseline',None,None)
    def test_acquisition_vehicle_is_a_new_identity_and_run_backed_init_is_retired(self):
        self.assertEqual(len(f.RETIRED_RUN_BACKED_INIT),32)
        self.assertTrue(all(f.CASES[name].fixture for name in f.RETIRED_RUN_BACKED_INIT))
        for profile,names in f.ACQUISITION_V1_CASES_BY_PROFILE.items():
            cases=[f.CASES[name] for name in names];self.assertEqual(len(cases),4)
            self.assertEqual([f.init.CASES[x.fixture].files for x in cases],[100,1000,10000,100000])
            self.assertTrue(all(x.profile==profile and x.states is None and x.storage_ceiling is None for x in cases))
            self.assertTrue(all((x.command_budget_ns,x.verification_budget_ns)==(15_000_000_000,9_500_000_000) for x in cases))
            self.assertTrue(all(name.endswith('-acquisition-v1') and name not in f.RETIRED_RUN_BACKED_INIT for name in names))
        self.assertEqual(len(set(sum(f.ACQUISITION_V1_CASES_BY_PROFILE.values(),[]))),8)
        self.assertNotIn('scratch',f.ACQUISITION_VEHICLE)
        for old in f.RETIRED_RUN_BACKED_INIT:
            for arm in ('candidate','baseline'):
                with self.assertRaisesRegex(ValueError,'retired'):
                    f.run(old,None,arm,None,None)

    def test_acquisition_cap_lift_preserves_v1_and_all_other_gates(self):
        self.assertEqual(len(set(sum(f.ACQUISITION_CASES_BY_PROFILE.values(),[]))),8)
        for profile,old_names in f.ACQUISITION_V1_CASES_BY_PROFILE.items():
            new_names=f.ACQUISITION_CASES_BY_PROFILE[profile]
            for old,new in zip(old_names,new_names):
                before,after=f.CASES[old],f.CASES[new]
                self.assertEqual(new,old.rsplit('-v',1)[0]+'-v2')
                self.assertEqual((before.command_budget_ns,before.verification_budget_ns),
                                 (15_000_000_000,9_500_000_000))
                self.assertEqual((after.command_budget_ns,after.verification_budget_ns),
                                 (30_000_000_000,19_000_000_000))
                self.assertEqual((before.fixture,before.states,before.storage_ceiling,
                                  before.profile,before.proof_policy,before.pack_layout),
                                 (after.fixture,after.states,after.storage_ceiling,
                                  after.profile,after.proof_policy,after.pack_layout))
                self.assertEqual(after.proof_envelope,'owner-init-caps-30-19-20261006-v2')
    def test_candidate_driver_takes_no_scratch_and_creates_the_acquisition_schema(self):
        text=(Path(__file__).resolve().parents[3]/'crates/layerfs-project/examples/benchmark_init.rs').read_text()
        self.assertIn('with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)',text)
        self.assertIn('acquisition: &handles.acquisition',text)
        self.assertIn('HistoryName::new(&args[3])',text)
        self.assertIn('args.get(4)',text)
        self.assertNotIn('scratch',text)
    def test_work_reduction_is_new_identity_with_unchanged_frozen_inputs_and_gates(self):
        self.assertEqual(len(f.WORK_REDUCTION_CASES),8)
        self.assertEqual(len(set(f.WORK_REDUCTION_CASES)),8)
        for old,new in zip(f.SPACE_SCALING_CASES,f.WORK_REDUCTION_CASES):
            self.assertEqual(new,old.replace('-space-scaling-v2','-work-reduction-v1'))
            before,after=f.CASES[old],f.CASES[new]
            self.assertEqual((before.fixture,before.command_budget_ns,before.verification_budget_ns,
                              before.profile,before.proof_policy,before.pack_layout,before.proof_envelope),
                             (after.fixture,after.command_budget_ns,after.verification_budget_ns,
                              after.profile,after.proof_policy,after.pack_layout,after.proof_envelope))
            self.assertEqual((after.command_budget_ns,after.verification_budget_ns),
                             (30_000_000_000,19_000_000_000))
    def test_streaming_has_fresh_identity_with_unchanged_inputs_and_gates(self):
        self.assertEqual(len(f.STREAMING_CASES),8)
        self.assertEqual(len(set(f.STREAMING_CASES)),8)
        for old,new in zip(f.WORK_REDUCTION_CASES,f.STREAMING_CASES):
            self.assertEqual(new,old.replace('-work-reduction-v1','-streaming-v1'))
            before,after=f.CASES[old],f.CASES[new]
            self.assertEqual((before.fixture,before.command_budget_ns,before.verification_budget_ns,
                              before.profile,before.proof_policy,before.pack_layout,before.proof_envelope),
                             (after.fixture,after.command_budget_ns,after.verification_budget_ns,
                              after.profile,after.proof_policy,after.pack_layout,after.proof_envelope))
    def test_batched_streaming_preserves_all_frozen_gates_at_new_identity(self):
        self.assertEqual(len(f.STREAMING_BATCHED_CASES),8)
        self.assertEqual(len(set(f.STREAMING_BATCHED_CASES)),8)
        for old,new in zip(f.STREAMING_CASES,f.STREAMING_BATCHED_CASES):
            self.assertEqual(new,old.rsplit('-v',1)[0]+'-v2')
            before,after=f.CASES[old],f.CASES[new]
            self.assertEqual((before.fixture,before.command_budget_ns,before.verification_budget_ns,
                              before.profile,before.proof_policy,before.pack_layout,before.proof_envelope),
                             (after.fixture,after.command_budget_ns,after.verification_budget_ns,
                              after.profile,after.proof_policy,after.pack_layout,after.proof_envelope))
    def test_admission_refills_have_new_identity_with_all_gates_preserved(self):
        self.assertEqual(len(f.STREAMING_REFILL_CASES),8)
        self.assertEqual(len(set(f.STREAMING_REFILL_CASES)),8)
        for old,new in zip(f.STREAMING_BATCHED_CASES,f.STREAMING_REFILL_CASES):
            self.assertEqual(new,old.rsplit('-v',1)[0]+'-v3')
            before,after=f.CASES[old],f.CASES[new]
            self.assertEqual((before.fixture,before.command_budget_ns,before.verification_budget_ns,
                              before.profile,before.proof_policy,before.pack_layout,before.proof_envelope),
                             (after.fixture,after.command_budget_ns,after.verification_budget_ns,
                              after.profile,after.proof_policy,after.pack_layout,after.proof_envelope))
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

"""Owner proof15 ruling does not widen scope or fabricate a performance sample."""
import json,sys,tempfile,unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from shared import phase7_history_proof as p
class Proof15(unittest.TestCase):
    def test_retained_input_uses_existing_directory_guard_not_fresh_output_guard(self):
        from diagnostics import reprove_reference as r
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp);owned=base/'existing';owned.mkdir()
            with patch.object(r.runner,'RESULTS',base):
                self.assertEqual(r.reference_run_dir(owned),owned.resolve())
                with self.assertRaises(ValueError):r.reference_run_dir(base/'missing')
                link=base/'link';link.symlink_to(owned,target_is_directory=True)
                with self.assertRaises(ValueError):r.reference_run_dir(link)
    def records(self):
        receipt={'arm':'baseline','measured_source_commit':p.BASE,'status':'COMPLETE','cache_status':'PASS','cleanup':{'status':'PASS'},'sample_count':1,'verification_status':'PASS','proof_policy':p.LITE_POLICY,'proof_envelope':'owner-stride1-proof15-v2','verification_budget_ns':15_000_000_000,'verification_wall_ns':14_000_000_000,'command_wall_ns':200_000_000_000,'command_budget_ns':300_000_000_000,'performance':{'child':{'status':'COMPLETE','profile_identity':'phase4.5-memory-off','states':157,'roots':['01'*32]*157}},'identity':{'product_seal':'same','cargo_lock_sha256':'same'},'build':{'compilation_seal':'native-same','binaries':{'driver':'same'}},'workload_row':'history-stride1'}
        census={'status':'CHECKED','canonical_objects':104618,'canonical_bytes':871337620}
        proof={'status':'CHECKED','states':157,'custody_states':157,'sample_policy':p.LITE_POLICY,'authenticated_bytes':1,'acquired_content_bytes':1}
        return receipt,census,proof
    def pin(self,folder,r,c,v):
        paths=[folder/f'{n}.json' for n in ('receipt','census','proof')]
        for path,value in zip(paths,(r,c,v)):path.write_text(json.dumps(value))
        return p.root_pins(r,c,v,*paths)
    def test_authorized15_passes_but12_history_and_unrecognized_scope_do_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            r,c,v=self.records();folder=Path(tmp);self.pin(folder,r,c,v)
            self.pin(folder,{**r,'proof_envelope':'owner-stride1-proof30-v3','verification_budget_ns':30_000_000_000,'verification_wall_ns':29_000_000_000},c,v)
            for changes in ({'proof_envelope':'lite12-v1'},{'workload_row':'history-stride3'},{'verification_wall_ns':15_000_000_001}):
                with self.assertRaises(ValueError):self.pin(folder,{**r,**changes},c,v)
            for changes in ({'sample_policy':'weakened'},{'authenticated_bytes':8*1024*1024+1},{'acquired_content_bytes':32*1024*1024+1}):
                with self.assertRaises(ValueError):self.pin(folder,r,c,{**v,**changes})
    def test_shared_performance_hash_scope_and_exact_observations_are_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder=Path(tmp);r,c,v=self.records();original={**r,'verification_status':'FAIL','verification_budget_ns':12_000_000_000};path=folder/'original.json';path.write_text(json.dumps(original))
            reuse={'receipt':str(path),'sha256':p.digest(path),'new_performance_samples':0,'matched_compilation_seal':'native-same'}
            shared={**r,'performance_reuse':reuse};self.pin(folder,shared,c,v)
            with self.assertRaises(ValueError):self.pin(folder,{**shared,'command_wall_ns':201_000_000_000},c,v)
            with self.assertRaises(ValueError):self.pin(folder,{**shared,'performance_reuse':{**reuse,'new_performance_samples':1}},c,v)
            path.write_text('{}')
            with self.assertRaises(ValueError):self.pin(folder,shared,c,v)
if __name__=='__main__':unittest.main()

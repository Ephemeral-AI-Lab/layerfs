import importlib.util,sys,json,tempfile,contextlib,io
from pathlib import Path
from unittest.mock import patch
p=Path("benchmark/fs-bench-pro/workspace-runner.py");spec=importlib.util.spec_from_file_location("fail_fast_model",p);m=importlib.util.module_from_spec(spec);sys.modules[spec.name]=m;spec.loader.exec_module(m)
assets={"revision":"source","harness_seal":"h","product_seal":"p","image_id":"i","environment_identity":"e"}
case={"scenario_id":"case","family_id":"family","tier":1,"operation":"op"}
def row(seed,status="pass"):
    return {"harness_identity":"h","product_identity":"p","image_id":"i","environment_identity":"e","scenario_id":"case","seed":seed,"mode":"performance","source_arm":"baseline","coverage_status":"executed" if status!="preparation-fail" else "unexecuted","product_status":"pass" if status=="pass" else "not-run" if status=="preparation-fail" else "fail","harness_status":"pending-validation" if status=="pass" else "fail","supervisor_cleanup_status":"pass"}
checks=[]
with tempfile.TemporaryDirectory() as d:
    root=Path(d)
    for failure in ["preparation-fail","product-fail","retained-fail"]:
        campaign=root/failure;campaign.mkdir();calls=[];ledger={}
        def retain(value):
            attempt=campaign/"attempts"/str(value["seed"]);attempt.mkdir(parents=True,exist_ok=True);value={**value,"evidence_path":str(attempt)}
            m.custody.write_json(attempt/"outcome.json",value);m.custody.seal(attempt);return value
        prior=retain(row(1));ledger[m.slot_key(prior)]=prior
        if failure=="retained-fail":
            prior_fail=retain(row(2,"product-fail"));ledger[m.slot_key(prior_fail)]=prior_fail
        m.atomic_json(campaign/"slots.json",ledger)
        def sample(case,seed,*args):calls.append(seed);return retain(row(seed,failure))
        argv=["runner","--assets",str(root),"--output",str(campaign),"--family","family","--all"]
        with patch.object(sys,"argv",argv),patch.object(m,"source_validation",return_value=assets),patch.object(m,"command",return_value=json.dumps(case)),patch.object(m,"select_preparation",return_value=assets),patch.object(m,"sample",side_effect=sample),contextlib.redirect_stdout(io.StringIO()):
            assert m.main()==1
        assert calls==([] if failure=="retained-fail" else [2]),calls
        after=m.read_json(campaign/"slots.json");assert after[m.slot_key(prior)]==prior
        assert {r["seed"] for r in after.values()}=={1,2}
        for value in after.values():m.custody.verify_manifest(Path(value["evidence_path"]))
        invocation=m.read_json(next((campaign/"invocations").glob("*.json")));assert invocation["status"]=="failed-outcomes" and invocation["invocation_wall_ns"]>=0
        assert m.ledger_action(after[m.slot_key(prior)],None)=="reuse-recorded-outcome"
        checks.append({"failure":failure,"executed_seeds":calls,"retained_seeds":[1,2],"later_seed_executed":False})
print(json.dumps({"status":"pass","checks":checks},indent=2))

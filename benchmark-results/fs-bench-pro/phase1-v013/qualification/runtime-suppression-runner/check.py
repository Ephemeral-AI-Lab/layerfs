import importlib.util,sys,json,tempfile,contextlib,io,copy
from pathlib import Path
from unittest.mock import patch
p=Path("benchmark/fs-bench-pro/workspace-runner.py");spec=importlib.util.spec_from_file_location("suppression_runner_model",p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
assets={"revision":"source","image_id":"image","harness_seal":"h","product_seal":"p","environment_identity":"env"}
def case(name,proof=False):return {"scenario_id":name,"family_id":"family","tier":1,"operation":"op","input_mode":"store","proof_only":proof}
def execute(campaign,registry,argv,sample,prep):
    with patch.object(sys,"argv",["runner","--assets",str(campaign),"--output",str(campaign),*argv]),patch.object(m,"source_validation",return_value=assets),patch.object(m,"command",return_value="\n".join(json.dumps(r) for r in registry)),patch.object(m,"select_preparation",side_effect=prep),patch.object(m,"sample",side_effect=sample),contextlib.redirect_stdout(io.StringIO()):return m.main()
def result(c,seed,args,status="pass"):
    return {"scenario_id":c["scenario_id"],"family_id":c["family_id"],"seed":seed,"mode":args.mode,"source_arm":"baseline","source_revision":assets["revision"],"harness_identity":assets["harness_seal"],"product_identity":assets["product_seal"],"image_id":assets["image_id"],"environment_identity":assets["environment_identity"],"coverage_status":"executed","product_status":status,"harness_status":"pending-validation" if status=="pass" else "needs-review","supervisor_cleanup_status":"pass","sample_complete":{"pure_call_sum_ns":1000} if status=="pass" else None,"evidence_path":"model-"+c["scenario_id"]+str(seed)}
with tempfile.TemporaryDirectory() as directory:
    root=Path(directory);initial=root/"initial";initial.mkdir()
    fail=lambda *a:(_ for _ in ()).throw(AssertionError("suppressed work prepared/executed"))
    assert execute(initial,[case("git-tool-1")],["--case","git-tool-1","--seed","1"],fail,fail)==0
    ledger=m.load_suppressions(initial);assert len(ledger["cases"])==14
    assert not m.is_suppressed(case("git-tool-1",True),ledger)
    invocation=m.read_json(next((initial/"invocations").glob("*.json")));assert invocation["status"]==m.SUPPRESSION_STATUS and invocation["preparation_producer"] is None
    assert execute(initial,[case("git-tool-1")],["--case","git-tool-1","--seed","3","--mode","verify"],fail,fail)==0
    dynamic=root/"dynamic";dynamic.mkdir();calls=[];registry=[case("active-a"),case("active-b")]
    event={"kind":"product-time-budget-exceeded","limit_ns":15_000_000_000,"cumulative_ns":15_000_000_001,"completed_product_ns":8_000_000_000,"active_phase_ns":7_000_000_001,"phase":"commit","measurement":"active-pure-call-sum"}
    assert m.product_budget_observation(event)==15_000_000_001
    for bad in [{**event,"cumulative_ns":15_000_000_000},{**event,"active_phase_ns":1}]:
        try:m.product_budget_observation(bad);raise AssertionError("invalid budget trigger admitted")
        except ValueError:pass
    def sample(c,seed,args,*_):
        calls.append((c["scenario_id"],seed));row=result(c,seed,args)
        if c["scenario_id"]=="active-a":
            decision=m.record_suppression(dynamic,c,assets["revision"],seed,row["evidence_path"],{**event,"scenario_id":c["scenario_id"],"observed_product_ns":m.product_budget_observation(event)})
            assert c["scenario_id"] in m.read_json(dynamic/"phase1-runtime-suppressions.json")["cases"],"decision not immediately persistent"
            row.update(product_status="fail",sample_complete=None,harness_status="needs-review",supervisor_failure="product-time-budget",phase1_status=m.SUPPRESSION_STATUS,phase1_suppression=decision)
        return row
    assert execute(dynamic,registry,["--family","family","--all"],sample,lambda *a:assets)==0
    assert calls==[("active-a",1),("active-b",1),("active-b",2),("active-b",3)],calls
    rows=m.read_json(dynamic/"slots.json");assert len(rows)==4 and next(r for r in rows.values() if r["scenario_id"]=="active-a")["product_status"]=="fail"
    assert execute(dynamic,registry,["--family","family","--all"],fail,lambda *a:assets)==0,"passing active slots not reused"
    assets["revision"]="future-source";assets["harness_seal"]="future-h"
    assert execute(dynamic,[case("active-a")],["--case","active-a","--seed","2"],fail,fail)==0
    assert not m.budget_suppression_can_continue({**next(r for r in rows.values() if r["scenario_id"]=="active-a"),"other_product_failure":True})
    # Existing bounded supervisor receives a complete authoritative event even
    # when its synthetic child exits before the first polling interval.
    out=root/"out";err=root/"err";seen=[]
    command=[sys.executable,"-c","import json; print("+repr(json.dumps(event))+", flush=True)"]
    observed=m.bounded_run(command,out,err,2,dict(m.os.environ),on_budget=lambda e:seen.append(m.product_budget_observation(e)))
    assert seen==[15_000_000_001] and observed["supervisor_failure"]=="product-time-budget" and observed["exit_code"]==124
    assert m.completed_product_time(case("normal"),{"mode":"performance","sample_complete":{"pure_call_sum_ns":1},"preparation_ns":10**12})==1
    assert m.completed_product_time(case("proof",True),{"mode":"verify","sample_complete":{"pure_call_sum_ns":10**12}}) is None
print(json.dumps({"status":"pass","initial_suppressed":14,"suppression_precedes_preparation":True,"associated_verify_skipped":True,"proof_only_exempt":True,"dynamic_persist_then_skip_other_seeds":True,"future_source_persistence":True,"other_active_cases_continue":True,"passing_rows_reused":True,"authoritative_event_and_cumulative_equation":True}))

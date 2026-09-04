import importlib.util,json,copy
from pathlib import Path
from unittest.mock import patch
p=Path("benchmark/fs-bench-pro/workspace-runner.py");s=importlib.util.spec_from_file_location("prep_capture_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");runtime=m.custody.source_identity("8278d817");runtime["workspace_preparation_compatibility"]=m.custody.workspace_preparation_digest(runtime)
proofs={};producers={}
for name,count in [("assets-34224330",2),("assets-a40b17e0",1)]:
    producer=m.read_json(base/name/"evidence/build.json");producer["workspace_preparation_compatibility"]=m.custody.workspace_preparation_digest(producer);producers[name]=producer
    result=m.preparation_source_compatibility(producer,runtime);assert len(result["changed_inputs"])==count
    assert result["producer_compatibility"]!=result["runtime_compatibility"]
    proofs[name]=result
producer=producers["assets-a40b17e0"];old=m.preparation_inputs(producer["revision"]);new=m.preparation_inputs(runtime["revision"])
changed=copy.deepcopy(new);path="crates/layerfs-sdk/src/lib.rs";mode,body=changed[path];changed[path]=(mode,body+b"unreviewed")
with patch.object(m,"preparation_inputs",side_effect=lambda rev:old if rev==producer["revision"] else changed):
    try:m.preparation_source_compatibility(producer,runtime);raise AssertionError("unrelated preparation change accepted")
    except ValueError:pass
try:m.preparation_source_compatibility({**producer,"revision":"unreviewed"},runtime);raise AssertionError("unknown producer accepted")
except ValueError:pass
with patch.object(m,"sealed_build",return_value=producers["assets-34224330"]):
    try:m.select_preparation(Path("new"),runtime,Path("old"),[],[{"scenario_id":"namespace-subtree-relocate-delete-500"}]);raise AssertionError("old namespace500 producer accepted")
    except ValueError as e:assert "namespace500" in str(e)
with patch.object(m,"command",side_effect=[json.dumps({"input_plan_sha256":"old"}),json.dumps({"input_plan_sha256":"changed"})]):
    try:m.acquire({"scenario_id":"case"},1,"producer",Path("cache"),Path("run"),Path("build"),{},producer,runtime_binary="runtime");raise AssertionError("different fixture identity accepted")
    except ValueError as e:assert "disagree" in str(e)
(base/"qualification/sql-capture-preparation-bridge/proofs.json").write_text(json.dumps(proofs,sort_keys=True,indent=2)+"\n")
print(json.dumps({"status":"pass","source_revision":runtime["revision"],"342_changed_inputs":2,"a40_changed_inputs":1,"namespace500_old_producer_rejected":True,"unrelated_source_and_plan_changes_rejected":True}))

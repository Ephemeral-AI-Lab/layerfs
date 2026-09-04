# Run once at the coordinator's quiet checkpoint; no product workloads.
import importlib.util,json,copy
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("rebase_bridge_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");config=m.read(base/"evidence-builds.json");previous=m.read(base/"qualification/source-bridge-before-d6fdf964.json");build=m.read(base/"assets-d6fdf964/evidence/build.json")
proof=m.content_frontier_source_proof(m.REBASE_LIFETIME_PARENT,build["revision"])
assert proof["prior_content_frontier_proof"] is None
assert proof["new_rebase_allocation_and_instruction_cost"]=="not-measured-by-retained-observations"
prior={b["old_revision"]:b for b in previous["product_compatibility"]}
for b in config["product_compatibility"]:
    expected_prior=None if b["old_revision"]==m.REBASE_LIFETIME_PARENT else prior[b["old_revision"]]["source_proof"]
    assert b["source_proof"]=={**proof,"prior_content_frontier_proof":expected_prior}
original=m.REBASE_LIFETIME_SOURCE_HASHES;m.REBASE_LIFETIME_SOURCE_HASHES=(original[0],"0"*64)
try:m.content_frontier_source_proof(m.REBASE_LIFETIME_PARENT,build["revision"]);raise AssertionError("different rebase source admitted")
except ValueError:pass
finally:m.REBASE_LIFETIME_SOURCE_HASHES=original
selected={"default":build}
for selector,choice in config["selections"].items():selected[selector]=m.read(base/choice["assets"]/"evidence/build.json")
case={"scenario_id":"workspace-dense-rewrite-500","family_id":"workspace_change_locality","operation":"workspace-dense-rewrite"}
for seed in [1,2,3]:assert m.selected_build(selected,case,"verify",seed)["product_seal"]==build["product_seal"]
assert m.selected_build(selected,case,"performance",1)["revision"]==m.REBASE_LIFETIME_PARENT
for seed in [2,3]:assert m.selected_build(selected,case,"performance",seed)["revision"]==build["revision"]
slots=m.read(base/"slots.json");retained=[]
for row in slots.values():
    if row.get("product_status")!="pass" or row["mode"]!="performance" or row["source_revision"]==build["revision"]:continue
    c={"scenario_id":row["scenario_id"],"family_id":row["family_id"]}
    if m.selected_build(selected,c,"performance",row["seed"])["revision"]==row["source_revision"]:retained.append(row)
assert len(retained)==190,len(retained)
for seed in [1,2]:
    row=next(r for r in slots.values() if r["scenario_id"]==case["scenario_id"] and r["seed"]==seed and r["mode"]=="performance" and r["source_revision"]==m.REBASE_LIFETIME_PARENT)
    issues=[];m.validate_content_frontier_records(m.raw(Path(row["evidence_path"])/"raw.jsonl"),case,{"source_proof":proof},issues)
    assert bool(issues)==(seed==2),issues
print(json.dumps({"status":"pass","retained_old_performance":190,"dense500_all_verifiers":"repaired product","dense500_old_seed2_rejected":True,"prior_source_chains":"unchanged nested proofs","new_pair":"exact lifecycle only"}))

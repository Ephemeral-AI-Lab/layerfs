# One combined product-free bridge qualification; run only at coordinator pause.
import importlib.util,json,copy
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("frontier_bridge_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");config=json.loads((base/"evidence-builds.json").read_text());build=json.loads((base/"assets-d1325d7f/evidence/build.json").read_text())
cases={cid:{"scenario_id":cid,"operation":cid.rsplit("-",1)[0]} for b in config["product_compatibility"] for cid in b["case_ids"]}
bridges=m.configured_product_bridges(config,build,cases);assert len(bridges)==4
assert all(b["source_proof"]["new_planner_instruction_cost"]=="not-measured-by-retained-observations" for b in bridges)
for old,invalid in [(m.CONTENT_FRONTIER_PARENT,"workspace-dense-rewrite-100"),(m.SPILL_INDEX_PARENT,"namespace-subtree-relocate-delete-500")]:
    direct=copy.deepcopy(next(b for b in config["product_compatibility"] if b["old_revision"]==old));direct["case_ids"].append(invalid)
    try:m.configured_product_bridges({"product_compatibility":[direct]},build,{**cases,invalid:{"operation":invalid.rsplit("-",1)[0]}});raise AssertionError("old failed scope admitted")
    except ValueError:pass
original=m.product_tree
def altered(rev):
    tree=original(rev)
    if rev==build["revision"]:tree["crates/extra.rs"]="100644 blob unreviewed"
    return tree
m.product_tree=altered
try:m.content_frontier_source_proof(m.CONTENT_FRONTIER_PARENT,build["revision"]);raise AssertionError("additional product change accepted")
except ValueError:pass
finally:m.product_tree=original
for name,parent,new in [("CONTENT_FRONTIER_SOURCE_HASHES",m.CONTENT_FRONTIER_PARENT,build["revision"]),("SPILL_INDEX_SOURCE_HASHES",m.SPILL_INDEX_PARENT,m.CONTENT_FRONTIER_PARENT)]:
    value=getattr(m,name);setattr(m,name,(value[0],"0"*64))
    try:
        function=m.content_frontier_source_proof if name.startswith("CONTENT") else m.spill_index_source_proof
        function(parent,new);raise AssertionError("unreviewed source hash accepted")
    except ValueError:pass
    finally:setattr(m,name,value)
selected={"default":build}
for selector,choice in config["selections"].items():selected[selector]=json.loads((base/choice["assets"]/"evidence/build.json").read_text())
slots=json.loads((base/"slots.json").read_text());retained=[]
for r in slots.values():
    if r.get("product_status")!="pass" or r["mode"]!="performance" or r["source_revision"]==build["revision"]:continue
    case={"scenario_id":r["scenario_id"],"family_id":r["family_id"]}
    if m.selected_build(selected,case,"performance",r["seed"])["revision"]==r["source_revision"]:retained.append(r)
assert len(retained)==186,len(retained)
for family,cid in [("payload_create_read","payload-create-10m"),("tiny_file_churn","tiny-unlink-500"),("directory_construction_traversal","directory-construct-500"),("git_tool_workflow","git-tool-500"),("namespace_mutation","namespace-subtree-relocate-delete-100")]:
    assert m.selected_build(selected,{"scenario_id":cid,"family_id":family},"verify",1)["revision"]==m.SPILL_INDEX_PARENT
for family,cid in [("namespace_mutation","namespace-subtree-relocate-delete-500"),("workspace_change_locality","workspace-dense-rewrite-10")]:
    assert m.selected_build(selected,{"scenario_id":cid,"family_id":family},"verify",1)["revision"]==build["revision"]
for bridge in config["verification_compatibility"]:
    assert set(bridge["unchanged_paths"])==m.bridge_dependency_paths(bridge["family_id"])
    for filename,expected in bridge["unchanged_paths"].items():m.validate_bridge_path(filename,expected,[bridge["performance_revision"],bridge["verification_revision"]])
print(json.dumps({"status":"pass","product_bridges":4,"retained_performance_slots":186,"proof_producer_selectors":"earlier e784/prep342; namespace500 and locality d132/prep a40","failed_scope_and_unreviewed_source_rejection":True}))

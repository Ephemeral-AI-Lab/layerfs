# Product-free checkpoint check; do not run during performance collection.
import importlib.util,json,copy
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("spill_bridge_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");config=json.loads((base/"evidence-builds.json").read_text());build=json.loads((base/"assets-a40b17e0/evidence/build.json").read_text())
cases={cid:{"scenario_id":cid,"operation":cid.rsplit("-",1)[0]} for b in config["product_compatibility"] for cid in b["case_ids"]}
bridges=m.configured_product_bridges(config,build,cases);assert len(bridges)==3
assert sum(b["source_proof"]["prior_predicate_proof"] is None for b in bridges)==1
assert all(b["source_proof"]["new_index_resource_and_instruction_cost"]=="not-measured-by-retained-observations" for b in bridges)
mutated=copy.deepcopy(config);direct=next(b for b in mutated["product_compatibility"] if b["old_revision"]==m.SPILL_INDEX_PARENT);direct["case_ids"].append("namespace-subtree-relocate-delete-500");mutated["product_compatibility"]=[direct]
try:m.configured_product_bridges(mutated,build,{**cases,"namespace-subtree-relocate-delete-500":{"operation":"namespace-subtree-relocate-delete"}});raise AssertionError("previous failed tier admitted")
except ValueError:pass
original=m.product_tree
def altered(rev):
    tree=original(rev)
    if rev==build["revision"]:tree["crates/extra.rs"]="100644 blob unreviewed"
    return tree
m.product_tree=altered
try:m.spill_index_source_proof(m.SPILL_INDEX_PARENT,build["revision"]);raise AssertionError("additional product change accepted")
except ValueError:pass
finally:m.product_tree=original
original_hashes=m.SPILL_INDEX_SOURCE_HASHES;m.SPILL_INDEX_SOURCE_HASHES=(original_hashes[0],"0"*64)
try:m.spill_index_source_proof(m.SPILL_INDEX_PARENT,build["revision"]);raise AssertionError("different index implementation accepted")
except ValueError:pass
finally:m.SPILL_INDEX_SOURCE_HASHES=original_hashes
print(json.dumps({"status":"pass","product_bridges":3,"source_chain":"legacy predicate then exact derived-index pair","namespace500_old_scope_rejected":True,"additional_product_change_rejected":True,"different_index_source_rejected":True}))

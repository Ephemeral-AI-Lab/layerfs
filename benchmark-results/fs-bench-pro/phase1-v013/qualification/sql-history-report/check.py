# Product-free report model; execute at a coordinator-approved quiet checkpoint.
import importlib.util,json,subprocess
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("sql_history_report_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");manifest=m.read(base/"qualification/sql-history-invalidation/selected-performance.json");config=m.read(base/"evidence-builds.json");build=m.read(base/"assets-6c54f8d7/evidence/build.json")
assert manifest["count"]==len(manifest["slots"])==191
assert all("diagnostic-only" in m.sql_history_status(rev) for rev in {r["source_revision"] for r in manifest["slots"]})
assert m.sql_history_status(build["revision"])=="explicit-opt-in; default capture disabled"
assert len(config["selections"])==2 and all(key.endswith(":1:verify") for key in config["selections"])
registry=[json.loads(line) for line in subprocess.check_output([str(base/"assets-6c54f8d7/fs-benchmark-pro"),"workspace-registry"],text=True).splitlines()];cases={r["scenario_id"]:r for r in registry}
assert len(m.configured_product_bridges(config,build,cases))==2
old=next(r for r in manifest["slots"] if r["scenario_id"]=="payload-create-1m" and r["seed"]==1 and r["source_revision"].startswith("fbf"));outcome=m.read(Path(old["evidence_path"])/"outcome.json");oldbuild=m.read(base/"assets-fbf32e84/evidence/build.json")
result=m.validate_attempt(outcome,{},cases[old["scenario_id"]],oldbuild)
assert any("performance timer/resource contamination" in issue for issue in result["issues"])
assert outcome["product_status"]==result["product_status"]=="pass", "raw logical pass incorrectly relabelled"
assert result["verification_pass"] is False
invalidations=[json.loads(line) for line in (base/"invalidations.jsonl").read_text().splitlines() if line]
assert {r["evidence_path"] for r in manifest["slots"]} <= {r["previous_evidence"] for r in invalidations if r.get("category")=="unrequested-sql-history"}
print(json.dumps({"status":"pass","invalidated_selected_performance":191,"old_performance_selectors":0,"retained_proofs":2,"old_raw_pass_preserved_but_timer_rejected":True}))

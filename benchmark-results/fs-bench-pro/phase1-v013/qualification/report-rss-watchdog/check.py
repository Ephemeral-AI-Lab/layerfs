import importlib.util,json,subprocess
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("rss_report_model",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013");a=base/"attempts/workspace-dense-rewrite-500-s2-performance-7eed48854a32";outcome=m.read(a/"outcome.json");records=m.raw(a/"raw.jsonl")
assert m.host_rss_termination(records,outcome)==2150727680
for rows,o in [([{ "kind":"resource-failure","host_rss_bytes":2*m.GIB}],outcome),(records,{**outcome,"exit_code":0}),(records+[{"kind":"resource-failure","host_rss_bytes":2150727680}],outcome)]:
    try:m.host_rss_termination(rows,o);raise AssertionError("invalid termination accepted")
    except ValueError:pass
assets=base/"assets-d1325d7f";build=m.read(assets/"evidence/build.json");registry=[json.loads(line) for line in subprocess.check_output([str(assets/"fs-benchmark-pro"),"workspace-registry"],text=True).splitlines()];case=next(r for r in registry if r["scenario_id"]==outcome["scenario_id"])
classification=m.read(base/"classifications.json").get(a.name,{})
result=m.validate_attempt(outcome,classification,case,build)
assert result["product_status"]=="fail" and result["verification_pass"] is False
assert "host RSS exceeds frozen 2 GiB; watchdog terminated process" in result["violations"]
assert "failed performance has no reached failure boundary" not in result["issues"]
assert "missing/extra reached product phase boundaries" in result["issues"],"unflushed public-operation evidence gap was concealed"
assert result["metrics"]["host_watchdog.observed_rss_bytes.max"]==2150727680
q=base/"qualification/report-rss-watchdog";(q/"selected-validation.json").write_text(json.dumps(result,sort_keys=True,indent=2)+"\n")
print(json.dumps({"status":"pass","actual_rss_bytes":2150727680,"product_status":result["product_status"],"violations":result["violations"],"issues":result["issues"]}))

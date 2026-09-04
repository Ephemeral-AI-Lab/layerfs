import importlib.util,json,re
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("report_git_check",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
base=Path("benchmark-results/fs-bench-pro/phase1-v013/attempts")
git=base/"git-tool-10-s1-performance-5abd0cdea1ba"
canceled=base/"git-tool-10-s2-performance-e39620441956"
records=m.raw(git/"raw.jsonl");case={"scenario_id":"git-tool-10","family_id":"git_tool_workflow","operation":"git-tool","tier":10}
issues=[];elapsed=m.failed_execution(records,case,issues);assert not issues,issues;assert elapsed==19314780583
failed=json.loads((git/"outcome.json").read_text());pending=json.loads((canceled/"outcome.json").read_text())
assert m.derived_product_status(failed,[])=="fail"
assert m.derived_product_status(pending,[])=="not-run"
assert sum(m.derived_product_status(item,[])=="fail" for item in [failed,pending])==1
assert m.terminal_status([], ["unexecuted"], [], [])=="NO_GO"
text=next(row["original_error"] for row in records if row["kind"]=="recovery")
output=bytearray()
for block in re.findall(r"OutputChunk \{[^{}]*stream: Stderr,[^{}]*bytes: \[([0-9, ]*)\]",text):output.extend(int(value.strip())for value in block.split(",")if value.strip())
stderr=output.decode();partial=m.receipt("\n".join(line.removeprefix("partial_")for line in stderr.splitlines()if line.startswith("partial_")))
try:m.failed_git_command(stderr,{**partial,"git_process_count":3},case,[]);raise AssertionError("wrong Git attempt count accepted")
except ValueError:pass
for old in ["fbf32e84662d00993c033515e113437965395494","b8c2ad4bf4fa0415fd49d57abea15729b33a4284"]:
    proof=m.unlink_source_proof(old,"3422433020a678a77f88e8a110492ca293c05e30")
    assert proof["changed_path"]==m.UNLINK_SOURCE_PATH
    assert len(set(proof["unlink_body_sha256"].values()))==2
issues=[];m.validate_no_unlink_records(records,issues);assert issues,"Git mutation incorrectly accepted by no-call bridge"
for outcome in base.glob("payload-create-1m-s1-performance-*/outcome.json"):
    if json.loads(outcome.read_text())["source_revision"]=="fbf32e84662d00993c033515e113437965395494":
        issues=[];m.validate_no_unlink_records(m.raw(outcome.parent/"raw.jsonl"),issues);assert not issues,issues;break
else:raise AssertionError("retained fbf no-unlink fixture not found")
print("git_failure_not_run_and_exact_unlink_bridge_check=pass")

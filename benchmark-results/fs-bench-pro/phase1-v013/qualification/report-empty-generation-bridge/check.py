import importlib.util,json,copy
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py");s=importlib.util.spec_from_file_location("report_predicate_check",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
new="101626e773c2045875ec49fe48ac5416e4844feb";old="fbf32e84662d00993c033515e113437965395494";b8="b8c2ad4bf4fa0415fd49d57abea15729b33a4284"
proofs={rev:m.empty_generation_source_proof(rev,new) for rev in [old,b8]}
assert all(p["new_guard_instruction_cost"]=="not-measured-by-retained-timings" for p in proofs.values())
try:m.unlink_source_proof(old,new);raise AssertionError("old unlink-only bridge accepted additional product change")
except ValueError:pass
original=m.subprocess.check_output
for needle,replacement in [(b"let emptied = old.len() != 0 && next.len() == 0;",b"let emptied = next.len() == 0;"),(b"*current_edits = if emptied { 0 } else { edits };",b"*current_edits = 0;")]:
    def altered(args,**kwargs):
        data=original(args,**kwargs)
        return data.replace(needle,replacement) if args==["git","show",f"{new}:{m.INSTALL_EDIT_PATH}"] else data
    m.subprocess.check_output=altered
    try:m.empty_generation_source_proof(old,new);raise AssertionError("unreviewed installer transform accepted")
    except ValueError:pass
    finally:m.subprocess.check_output=original
base=Path("benchmark-results/fs-bench-pro/phase1-v013")
slots=json.loads((base/"slots.json").read_text());checked=[]
for operation in sorted(m.NO_UNLINK_OPERATIONS):
    outcomes=[o for o in slots.values() if o.get("source_revision")==old and o.get("mode")=="performance" and o.get("seed")==2 and o["scenario_id"] in {operation+"-1",operation+"-1m"}]
    assert len(outcomes)==1,(operation,len(outcomes))
    o=outcomes[0];case={"scenario_id":o["scenario_id"],"operation":operation,"tier":1}
    records=m.raw(Path(o["evidence_path"])/"raw.jsonl");issues=[];scope=m.validate_empty_generation_records(records,case,issues);assert not issues,(operation,issues)
    checked.append({"attempt":Path(o["evidence_path"]).name,"scope":scope})
    # A truncation counter invalidates even a nominally approved scenario.
    changed=copy.deepcopy(records);phase=next(r for r in changed if r["kind"]=="phase" and r.get("phase")=="exec")
    phase["workload_receipt"]=phase["workload_receipt"].replace("workload_ftruncate_call_count=0","workload_ftruncate_call_count=1")
    issues=[];m.validate_empty_generation_records(changed,case,issues);assert issues,"destructive observation accepted"
    if operation=="payload-create":
        issues=[];m.validate_empty_generation_records(records,{**case,"operation":"git-tool"},issues);assert issues,"Git accepted"
for o in slots.values():
    if o.get("source_revision")==old and o.get("mode")=="verify" and o["scenario_id"]=="payload-create-1m" and o["seed"]==1:
        issues=[];scope=m.validate_empty_generation_records(m.raw(Path(o["evidence_path"])/"raw.jsonl"),{"scenario_id":o["scenario_id"],"operation":"payload-create","tier":1},issues);assert not issues,issues
        checked.append({"attempt":Path(o["evidence_path"]).name,"scope":scope});break
else:raise AssertionError("old payload proof missing")
print(json.dumps({"status":"pass","actual_representative_attempts":checked,"source_proofs":proofs},indent=2))

import importlib.util
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py")
s=importlib.util.spec_from_file_location("report_slot_check",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
case={"scenario_id":"tiny-stat-1","family_id":"tiny_file_churn"}
builds={"default":"new","family:tiny_file_churn:performance":"old","slot:tiny-stat-1:1:performance":"new"}
assert m.selected_build(builds,case,"performance",1)=="new"
assert m.selected_build(builds,case,"performance",2)=="old"
assert m.selected_build(builds,{**case,"seed":1},"performance")=="new"
assert m.selected_build(builds,case,"verify",1)=="new"
source=b"fn timed() { work(); }\n\nfn sample_resources() -> Result<()> {\n    old_row();\n}\nfn after() { unchanged(); }\n"
changed=source.replace(b"old_row();",b"atomic_row();")
a,old=m.sampler_source_parts(source);b,new=m.sampler_source_parts(changed)
assert a==b and old!=new
assert m.sampler_source_parts(source.replace(b"work();",b"different_work();"))[0]!=a
assert m.sampler_source_parts(source.replace(b"unchanged();",b"changed_after();"))[0]!=a
for invalid in (source+source,source.replace(b"fn sample_resources()",b"fn another_sampler()")):
    try:m.sampler_source_parts(invalid);raise AssertionError("unrecognized sampler boundary accepted")
    except ValueError:pass
assert "benchmark/fs-bench-pro/workload.rs" in m.bridge_dependency_paths("tiny_file_churn")
assert "benchmark/fs-bench-pro/workspace_registry.rs" in m.bridge_dependency_paths("tiny_file_churn")
print("exact_slot_and_sampler_only_source_bridge_check=pass")

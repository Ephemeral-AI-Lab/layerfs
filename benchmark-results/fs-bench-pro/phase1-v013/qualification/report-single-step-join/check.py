import importlib.util
from pathlib import Path
p=Path("benchmark/fs-bench-pro/generate-workspace-report.py")
s=importlib.util.spec_from_file_location("report_join_check",p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
def state(step,size,phase):return {"kind":"store-observation","step":step,"phase":phase,**{key:size for key in m.STORE_GAUGES}}
records=[state(0,10,"before"),{"kind":"phase","phase":"initialize","elapsed_ns":7},state(0,30,"after-initialize")]
x=m.observation_data(records,{}, {}, {})
assert x["steps"][0]["step"]==0 and x["steps"][0]["store"]["file_bytes"]==10
assert x["steps"][1]["step"]==1 and x["steps"][1]["timings"]=={"initialize_ns":7}
assert x["steps"][1]["store_growth_this_step"]["file_bytes"]==20
x=m.observation_data([{"kind":"phase","phase":"sdk-edit","elapsed_ns":3},{"kind":"phase","phase":"commit","elapsed_ns":5}],{}, {}, {})
assert x["steps"]==[{"step":1,"timings":{"sdk-edit_ns":3,"commit_ns":5},"diagnostics":{}}]
print("single_step_import_and_capped_join_check=pass")

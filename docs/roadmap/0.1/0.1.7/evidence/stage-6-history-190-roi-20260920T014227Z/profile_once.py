#!/usr/bin/env python3
"""One declared native stack profile; no product or compiler changes."""
import importlib.util,json,subprocess,time
from pathlib import Path
p=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location("collector",p.parent/"stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py")
collector=importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)
collector.CAMPAIGN=p/"profile"
identity=json.loads((p/"profile/baseline-identity.json").read_text())
binary=identity["binary"]
original_run=subprocess.run

def profiled_run(command, **kwargs):
    if not command or command[0]!=binary:
        return original_run(command, **kwargs)
    timeout=kwargs.pop("timeout")
    start=time.monotonic_ns()
    child=subprocess.Popen(command, **kwargs)
    sample_command=["/usr/bin/sample",str(child.pid),str(timeout),"5","-mayDie","-file",str(p/"profile/sample.txt")]
    document={"kind":"profiled diagnostic; not a paired timing arm","child_command":command,"child_pid":child.pid,"sample_command":sample_command,"start_monotonic_ns":start,"actual_first_sample_gap_ns":None,"cache_admission":"INELIGIBLE","deadline_seconds":timeout}
    with (p/"profile/sample-stdout.txt").open("x") as out, (p/"profile/sample-stderr.txt").open("x") as err:
        sampler=subprocess.Popen(sample_command,stdout=out,stderr=err)
        document["profiler_launch_gap_ns"]=time.monotonic_ns()-start
        try:
            code=child.wait(timeout=max(0.1,timeout-(time.monotonic_ns()-start)/1e9))
            document["workload_wall_ns"]=time.monotonic_ns()-start
            try:
                sampler.wait(timeout=max(0.1,timeout-(time.monotonic_ns()-start)/1e9))
            except subprocess.TimeoutExpired:
                sampler.terminate();sampler.wait();document["sampler_timed_out"]=True
            return subprocess.CompletedProcess(command,code)
        except subprocess.TimeoutExpired:
            child.kill();child.wait();sampler.terminate();sampler.wait();document["workload_timed_out"]=True
            raise
        finally:
            document.update(child_exit_code=child.returncode,sampler_exit_code=sampler.returncode,complete_wall_ns=time.monotonic_ns()-start)
            with (p/"profile/profile-receipt.json").open("x") as f:json.dump(document,f,indent=2)
subprocess.run=profiled_run
collector.main()

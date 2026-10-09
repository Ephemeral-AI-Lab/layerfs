"""Once-only SDK-container input staging proof; setup, never performance.

The lead supplies an independent installed Store clone and sealed deployment.
The fixed staging stop is 300 seconds; startup and explicit stop each get 15.
Failure fences only this host controller and retains the original container.
"""
import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "fs-bench-pro"))
from r7 import deployment
from r7.runner import EventProcess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    selection = dict(arm="N", case_id="E01", cache_class="A")
    # Validate every closed input before creating a container. These are setup
    # pins, not a sealed performance-arm or cache eligibility claim.
    deployment.load_plan(config, selection)
    for name in ("runtime", "daemon"):
        deployment.require(deployment.digest(config[name + "_binary"]) == config[name + "_sha256"],
                           "original " + name + " binary changed")
    deployment.require(deployment.digest(config["manifest"]) == config["manifest_sha256"],
                       "original installed manifest changed")
    args.output.mkdir(exist_ok=False)
    argv = [config["runtime_binary"], "serve", "--socket", config["socket"],
            "--volume", config["volume"], "--daemon", config["daemon_binary"],
            "--manifest", config["manifest"], "--receipt", str(args.output / "runtime.events.jsonl"),
            "--uid", str(config["uid"]), "--gid", str(config["gid"])]
    deployment.write_new(args.output / "prospective-inputs.json", dict(
        config=config, config_sha256=deployment.digest(args.config), argv=argv,
        implementation_sha256=deployment.digest(__file__), overall_setup_stop_seconds=360,
        admission_eligible=False, performance_samples=0, cache_treatments=0,
        stage_role="actual transfer/setup verification through acknowledged fresh SDK container"))
    runtime = EventProcess(argv, args.output, "runtime")
    result = dict(status="INCOMPLETE", admission_eligible=False, performance_samples=0,
                  original_error=None, container=None, container_stop="NOT_ATTEMPTED")
    try:
        ready = runtime.event("protocol_ready", time.monotonic() + 15)
        deployment.require(runtime.container is not None, "SDK container acknowledgement missing")
        result["ready"] = ready
        result["container"] = runtime.container
        result["staging"] = deployment.stage(config, selection, runtime.container, args.output / "setup")
        # Ordinary nonroot execution proves helper/native permissions without
        # any Workspace registration or a performance/cold-cache claim.
        script = args.output / "ordinary-native-check.sh"
        script.write_text("set -euo pipefail\ntest -r /code/workload.py\ntest -r .git/HEAD\ntest -d node_modules\ntrue\n")
        result["ordinary_native_check"] = runtime.send("command", "native:" + config["native_root"],
                                                       script, deadline=time.monotonic() + 9.5)
        result["backing_snapshot"] = runtime.send("observe", deadline=time.monotonic() + 9.5)
        result["stop"] = runtime.send("stop", deadline=time.monotonic() + 15)
        runtime.event("finished", time.monotonic() + 5)
        code = runtime.process.wait(timeout=5)
        deployment.require(code == 0, "original runtime exit nonzero")
        result.update(status="PASS_SETUP_ONLY", runtime_exit=code, container_stop="KNOWN_STOP",
                      filesystem_mounts=0, verifier_scope="complete deployed regular bytes and supported metadata/aliases")
    except Exception as error:
        result.update(original_error=str(error), original_error_type=type(error).__name__,
                      container=runtime.container, engine_exec_ids=runtime.exec_ids,
                      failure_custody="exact container retained; lead explicit disposition; no replay or inferred drain")
        runtime.retain(error)
    deployment.write_new(args.output / "result.json", result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS_SETUP_ONLY" else 1


if __name__ == "__main__":
    raise SystemExit(main())

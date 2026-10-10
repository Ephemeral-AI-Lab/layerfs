"""Exploratory bounded count probe over one owned full-fixture Store copy.

Drives the registered R7 runtime (real SDK/Sandbox/daemon) once: mount, four
bounded read-only probe commands with a daemon snapshot around each, explicit
terminal unmount, Gone observation and stop. No oracle, no verdict, no timing
admission; natural caches with setup-copy warmth declared. One attempt per
operation; a failure retains the original custody and replays nothing.
"""
import argparse
import json
from pathlib import Path
import secrets
import shlex
import sys
import time

ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(ROOT / "core/benchmark/r8-tools"))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

PHASES = (
    ("tree-serial", 7, []),
    ("tree-parallel", 7, ["8"]),
    ("large-serial", 5, [
        "node_modules/.pnpm/@openai+codex@0.153.4-darwin-arm64/node_modules/@openai/codex/vendor/aarch64-apple-darwin/bin/codex"]),
    ("large-parallel", 5, [
        "node_modules/.pnpm/electron@44.0.0/node_modules/electron/dist/Electron.app/Contents/Frameworks/Electron Framework.framework/Versions/A/Electron Framework",
        "node_modules/.pnpm/@anthropic-ai+claude-agent-sdk-darwin-arm64@0.3.263/node_modules/@anthropic-ai/claude-agent-sdk-darwin-arm64/claude",
        ".git/objects/pack/pack-75ee84df94d3288bce321399037da9c969686a22.pack",
        "node_modules/.pnpm/@deepseek-ai+libreoffice-kit-darwin-arm64@0.1.1/node_modules/@deepseek-ai/libreoffice-kit-darwin-arm64/bin/libreoffice-kit"]),
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.inputs.read_text())
    probe = Path(__file__).with_name("probe.py")
    result = dict(schema="r8b-full-mount-count-probe-controller-v1", status="INCOMPLETE",
                  eligible=False, verdict="NONE: exploratory count diagnostic",
                  global_profile="Disposable/WAL/OFF", construction_workers=1,
                  cache="natural; setup copy warmth declared; no cold claim",
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("probe", probe),
                          ("controller", Path(__file__)), ("inputs", args.inputs))},
                  volume=config["volume"], phases=[], probes=[], operation_attempts=1)
    runtime = None
    deadline = time.monotonic() + 80
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"), "--uid", "501", "--gid", "20"]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        if not runtime.container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", runtime.container, "mkdir", "-m", "755", "/code"])
        checked(["docker", "cp", str(probe), runtime.container + ":/code/probe.py"])
        key = secrets.token_hex(32)
        lifecycle.attempt(runtime, result, "mount", key, None, 5, deadline, controls=2)
        lifecycle.attempt(runtime, result, "snapshot", key, None, 5, deadline)
        for phase, seconds, arguments in PHASES:
            body = args.output / (phase + ".sh")
            body.write_text("set -euo pipefail\nexec timeout --kill-after=1s " + str(seconds + 6)
                            + "s python3 -B /code/probe.py " + phase + " " + str(seconds) + " "
                            + " ".join(shlex.quote(value) for value in arguments) + "\n")
            event = lifecycle.attempt(runtime, result, "command", key, body, seconds + 8, deadline)
            fields = event.get("fields", {})
            if fields.get("registered_execs") != "0":
                raise OriginalFailure("probe lacks original zero Exec-registration evidence")
            result["probes"].append(json.loads(Path(fields["stdout"]).read_text()))
            lifecycle.attempt(runtime, result, "snapshot", key, None, 5, deadline)
        lifecycle.attempt(runtime, result, "unmount", key, None, 5, deadline, controls=1)
        lifecycle.cleanup(runtime, result, key, deadline)
        lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
        runtime.process.stdin.close()
        code = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
        if code:
            raise OriginalFailure("runtime exit differs: " + str(code))
        result.update(status="COMPLETE", container_stop="ACKNOWLEDGED", host_exit=code,
                      terminal="Unmounted, Gone observed, stop acknowledged")
    except Exception as original:
        result.update(status="FAILED", original_failure_type=type(original).__name__,
                      original_failure=str(original))
        if runtime is None:
            runtime = getattr(original, "event_process", None)
        if runtime is not None:
            result["custody"] = runtime.retain(original)
    finally:
        if runtime is not None:
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
        write(args.output / "probe.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "phases"}, sort_keys=True))
    if result["status"] != "COMPLETE":
        raise SystemExit(1)


if __name__ == "__main__":
    main()

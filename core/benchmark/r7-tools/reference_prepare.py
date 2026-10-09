"""Once-only native reference setup; source-dirty functional evidence, never a sample.

The lead invokes this controller with an outer wall stop of 660 seconds. Stage
and author each retain their distinct 300-second allowance; other actions have
15 seconds and share the outer deadline. No Init, Workspace mount, cache claim,
fixture reuse as N/P control, retry, or failed-path container Stop occurs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import secrets
import stat
import subprocess
import sys
import time
from types import SimpleNamespace

import oracle_prepare as author

sys.dont_write_bytecode = True
REPOSITORY = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPOSITORY / "core/benchmark/fs-bench-pro"))
from r7 import deployment, registry, verification, workloads
from r7.runner import EventProcess

SCHEMA = "r7-native-reference-controller-v1"
WALL_SECONDS = 660
require, sha, read_json, write_json = author.require, author.sha, author.read_json, author.write_new


def before(deadline, phase):
    if time.monotonic() >= deadline:
        error = TimeoutError("reference setup wall stop: " + phase)
        error.original_phase = phase
        raise error


def validate(config, case, cache):
    selected = next(row for row in registry.cases() if row["case_id"] == case)
    require(cache in selected["classes"], "reference cache class is not registered")
    require(not author.not_run(case, cache), author.not_run(case, cache) or "oracle unavailable")
    require(config["identities"]["image_id"] == registry.IMAGE and (config["uid"], config["gid"]) == (501, 20),
            "pinned image/ordinary workload identity required")
    require(config.get("store_profile") == "Disposable/WAL/OFF" and
            config.get("overlay_profile") == "MEMORY/OFF/EXCLUSIVE" and config.get("volume_clone_identity"),
            "explicit independent Disposable Store clone and overlay profile required")
    for name in ("runtime", "daemon"):
        require(sha(config[name + "_binary"]) == config[name + "_sha256"], "sealed " + name + " binary differs")
    require(sha(config["manifest"]) == config["manifest_sha256"], "sealed installed manifest differs")
    store = deployment.owned_input(config["prepared_store_file"])
    base = config["base_fixture_inventory"]
    verification.tree_inventory(deployment.owned_input(base["path"]), base["sha256"], base["content_metadata_set_sha256"])
    selection = dict(arm="N", case_id=case, cache_class=cache)
    plan = deployment.load_plan(config, selection)
    require(len(plan["native"]) == (2 if case == "W01" else 1), "reference root population differs")
    require(all(item["content_metadata_set_sha256"] == base["content_metadata_set_sha256"] for item in plan["native"]),
            "independent reference roots differ from declared complete fixture/cut")
    require([item["target"] for item in plan["assets"] if item["target"] != "/code"] in ([], ["/replay"]),
            "only sealed replay may be an external reference input")
    return selection, plan, dict(prepared_store_sha256=sha(store), installed_manifest_sha256=sha(config["manifest"]),
                                 base_content_metadata_set_sha256=base["content_metadata_set_sha256"],
                                 fixture_kind=config.get("fixture_kind"),
                                 reduced_fixture_cut_identity=config.get("reduced_fixture_cut_identity"))


def input_declaration(config, selection, plan, staging, fixture):
    require(staging.get("schema") == "r7-container-staging-v1" and staging.get("status") == "PASS" and
            staging.get("deployment_manifest_sha256") == config["container_setup"]["manifest_sha256"] and
            staging.get("implementation_sha256") == config["container_setup"]["implementation_sha256"],
            "original successful staging identity required")
    container, items = staging["container"], plan["assets"] + plan["native"]
    roles = ["scan", "writer"] if selection["case_id"] == "W01" else ["primary"]
    result = dict(fixture_identity=fixture, roots=[], external=[], code_assets={})
    for position, item in enumerate(items):
        calls = [row for row in staging["operations"] if row.get("label") == "inventory-" + str(position)]
        original = [row for row in staging["verifications"] if row.get("root") == item["target"]]
        require(len(calls) == len(original) == 1, "unique original inventory transfer/verification missing")
        call, verified = calls[0], original[0]
        expected = ["docker", "--host", "unix://" + config["socket"], "cp", str(deployment.owned_input(item["inventory"]))]
        require(call.get("attempts") == 1 and call.get("exit_code") == 0 and call["argv"][:-1] == expected and
                call["argv"][-1].startswith(container + ":"), "original acknowledged inventory transfer differs")
        inside = call["argv"][-1][len(container) + 1:]
        path = Path(inside)
        require(path.is_absolute() and ".." not in path.parts and len(path.parts) == 4 and
                path.parts[2].startswith("layerfs-r7-setup-") and path.name == "expected-" + str(position) + ".jsonl",
                "original inside inventory target is not owned setup evidence")
        require(verified.get("status") == "PASS" and verified.get("inventory_sha256") == item["inventory_sha256"] and
                verified.get("content_metadata_set_sha256") == item["content_metadata_set_sha256"],
                "original inventory verification seals differ")
        row = dict(path=item["target"], inventory=inside, inventory_sha256=item["inventory_sha256"],
                   content_metadata_set_sha256=item["content_metadata_set_sha256"])
        if item in plan["native"]:
            row["role"] = roles[len(result["roots"])]
            result["roots"].append(row)
        elif item["target"] != "/code":
            result["external"].append(row)
    case = selection["case_id"]
    names = {"largest-path"} if case == "E15" else {"node-roots.json"} if case in {"E12", "E13", "E14", "K02", "K03"} else set()
    if case == "E11":
        names.add("tracked-paths.json")
    if case in author.GIT_CASES:
        names.add("git-default-policy.json")
        result["git_default_policy"] = dict(path="/code/git-default-policy.json",
                                                sha256=staging["helper_sha256"]["git-default-policy.json"])
    for name in names:
        require(name in staging["helper_sha256"], "actual semantic code asset missing: " + name)
        result["code_assets"][name] = staging["helper_sha256"][name]
    return result


def cli_call(config, runtime, output, result, label, arguments, seconds, deadline):
    before(deadline, "before_" + label)
    stop = min(deadline, time.monotonic() + seconds)
    folder = output / "operations"
    argv = ["docker", "--host", "unix://" + config["socket"], *arguments]
    record = dict(label=label, argv=argv, container=runtime.container, attempts=1, pid=None,
                  status="ORIGINAL_PENDING", wall_stop_seconds=seconds,
                  engine_operation_identity="UNAVAILABLE through Docker CLI; no guessed Exec ID",
                  stdout=str(folder / (label + ".stdout")), stderr=str(folder / (label + ".stderr")))
    result["operations"].append(record)
    write_json(folder / (label + ".attempt.json"), record)
    phase = label + "_stream_open"
    try:
        with author.OutputFile(record["stdout"], phase) as stdout, author.OutputFile(record["stderr"], phase) as stderr:
            try:
                before(stop, "before_" + label + "_launch")
                phase = label + "_launch"
                child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True)
                record["pid"] = child.pid
                phase = label + "_acknowledgement_output"
                write_json(folder / (label + ".ack.json"), record)
                before(stop, "before_" + label + "_wait")
                phase = label + "_wait"
                code = child.wait(timeout=stop - time.monotonic())
                record.update(exit_code=code, status="ORIGINAL_COMPLETED")
                phase = label + "_exit"
                require(code == 0, "original Docker setup action failed: " + label)
            except BaseException as original:
                if not hasattr(original, "original_phase"):
                    original.original_phase = phase
                raise
        phase = label + "_evidence"
        record.update(stdout_sha256=sha(record["stdout"]), stderr_sha256=sha(record["stderr"]))
        write_json(folder / (label + ".completion.json"), record)
        before(stop, "complete_" + label)
        return record
    except BaseException as original:
        if not hasattr(original, "original_phase"):
            original.original_phase = phase
        record.update(original_failure_type=type(original).__name__, original_failure=str(original),
                      child_cancellation="NOT_ATTEMPTED; actual original host CLI PID retained")
        raise


def author_result(record):
    require(record.get("status") == "ORIGINAL_COMPLETED" and record.get("exit_code") == 0 and
            sha(record["stdout"]) == record["stdout_sha256"], "original known-zero author output identity differs")
    last = None
    with author.ReadFile(record["stdout"], "original_author_result_read") as stream:
        while True:
            line = stream.readline(2 * 1024 * 1024 + 1)
            if not line:
                break
            require(len(line) <= 2 * 1024 * 1024 and line.endswith(b"\n"), "original author result exceeds closed 2 MiB setup frame or is partial")
            last = line
    require(last is not None, "original author final result missing")
    return json.loads(last)


def check_bundle(bundle, spec, staging, asset, config, original_author):
    closed_path = deployment.owned_input(bundle / "closed.json")
    closed = read_json(closed_path)
    require(closed == author_result(original_author), "copied closed output differs from original author final stdout")
    require(closed.get("schema") == "r7-closed-oracle-author-v1" and closed.get("status") == "CLOSED_EXPECTED_SETUP_ONLY" and
            closed.get("spec_sha256") == spec["sha256"] and closed.get("staging_receipt_sha256") == staging["sha256"] and
            closed.get("container") == staging["value"]["container"] and
            closed.get("asset_root") == asset and closed.get("inputs") == spec["value"]["inputs"] and
            closed.get("helpers") == spec["value"]["helpers"] == verification.canonical_helpers(closed["case_id"]),
            "copied closed author/spec/input/helper identity differs")
    declaration = spec["value"]
    require(all(closed.get(key) == declaration[key] for key in ("case_id", "cache_class", "environment", "image_id", "uid", "gid",
                                                              "registry_sha256", "workload_source_sha256")) and
            closed.get("selected_workload") == declaration["case"]["workload"] and
            closed.get("fixture_identity") == declaration["inputs"]["fixture_identity"] and
            closed.get("native_verifications") == staging["value"]["verifications"], "copied original author declaration differs")
    rows = closed["expected"]
    require([(row["label"], row["role"]) for row in rows] == verification.coverage(declaration["case"]), "closed expected role/checkpoint coverage differs")
    git = verification.git_bundle(bundle, closed, config) if closed["case_id"] in author.GIT_CASES else None
    for row in rows:
        require(row["manifest"] == row["label"] + ".jsonl", "copied expected manifest checkpoint differs")
        require(sha(verification.member(bundle, row["manifest"])) == row["sha256"] and
                row["observer"] == author.observer_for(closed["case_id"], row["role"]), "closed selected expected observer differs")
        script = verification.member(bundle, Path(row["verifier"]["path"]).name)
        require(row["verifier"].get("path") == asset + "/" + row["label"] + ".verify.sh" and
                row["verifier"].get("performs_comparison") is True and row["verifier"].get("wall_stop_ns") == 9_500_000_000 and
                row["verifier"].get("root") == "original selected mounted cwd" and row["verifier"].get("expected") == row["manifest"],
                "copied verifier original root/path/budget differs")
        with author.ReadFile(script, "copied_verifier_read") as stream:
            require(stream.read() == author.verifier_body(asset, row["observer"], row["label"], row.get("git_index")).encode(),
                    "copied actual verifier body differs")
        require(sha(script) == row["verifier"]["sha256"], "copied actual verifier SHA differs")
        if git is not None:
            operand, context = row.get("git_index", {}), closed["git_context"]
            require(operand.get("manifest") == row["label"] + ".index.json" and operand.get("tree_sha256") == row["sha256"] and
                    all(operand.get(name) == context[name] for name in ("pin", "queries", "policy")) and
                    operand.get("pin_sha256") == context["pin_file_sha256"] and
                    all(operand.get(name + "_sha256") == context[name + "_sha256"] for name in ("queries", "policy")),
                    "copied expected Git row differs from paired context/tree")
            if closed["case_id"] == "E11":
                require(operand.get("tracked") == "/code/tracked-paths.json" and
                        operand.get("tracked_sha256") == declaration["inputs"]["code_assets"].get("tracked-paths.json"),
                        "copied selected tracked-path operand differs")
            else:
                require("tracked" not in operand and "tracked_sha256" not in operand,
                        "unselected copied tracked-path operand")
            for operand in ("manifest", "pin", "queries", "policy"):
                require(sha(verification.member(bundle, row["git_index"][operand])) == row["git_index"][operand + "_sha256"],
                        "copied expected Git operand differs")
            observation = verification.git_index_oracle.load_sealed(verification.member(bundle, row["git_index"]["manifest"]), row["git_index"]["manifest_sha256"])
            require(observation.get("pin") == git["pin"] and observation.get("pin_sha256") ==
                    verification.git_index_oracle.sha256(verification.git_index_oracle.canonical(git["pin"])), "copied index comparison pin differs")
            require(verification.git_index_oracle.compare_tree(verification.member(bundle, row["manifest"]), verification.member(bundle, row["manifest"]),
                    closed["case_id"], observation, observation)["status"] == "PASS", "copied expected index/tree raw metadata binding differs")
    stdout = closed["host_stdout_recipe"]
    require(sha(verification.member(bundle, stdout["expected_bundle_relative"])) == stdout["expected_sha256"], "copied expected stdout differs")
    return dict(closed=str(closed_path), closed_sha256=sha(closed_path), status=closed["status"], asset_root=asset)


def close_runtime(runtime, original=None, only=None):
    """Each actual host owner closes once, retaining the first cause separately."""
    closed = getattr(runtime, "_reference_closed_owners", set())
    runtime._reference_closed_owners = closed
    failures = []
    for name, owner in (("stdin", getattr(runtime.process, "stdin", None)), ("selector", runtime.selector),
                        ("raw", runtime.raw), ("stderr", runtime.stderr), ("stdout", getattr(runtime.process, "stdout", None))):
        if owner is None or name in closed or only is not None and name not in only:
            continue
        closed.add(name)
        try:
            owner.close()
        except BaseException as error:
            if not hasattr(error, "original_phase"):
                error.original_phase = "original_runtime_" + name + "_close"
            failures.append(error)
    if failures:
        if original is None:
            original = failures[0]
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), *(str(error) for error in failures[1:])]
            raise original
        original.independent_close_failures = [*getattr(original, "independent_close_failures", []), *(str(error) for error in failures)]


def seal_bundle(bundle, output, deadline):
    """Retain all copied witnesses with bounded payload reads and directory stack."""
    require(not bundle.is_symlink() and bundle.is_dir(), "copied bundle is not an actual directory")
    entries, payload, digest = 0, 0, hashlib.sha256()
    stack, original = [], None
    with author.OutputFile(output, "copied_bundle_inventory_write") as sink:
        try:
            stack.append((bundle, os.scandir(bundle)))
            while stack:
                before(deadline, "copied_bundle_inventory")
                directory, iterator = stack[-1]
                try:
                    path = directory / next(iterator).name
                except StopIteration:
                    stack.pop()
                    iterator.close()
                    continue
                info = path.lstat()
                require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), "copied witness bundle contains unsupported alias/kind")
                row = dict(path=path.relative_to(bundle).as_posix(), mode=stat.S_IMODE(info.st_mode), uid=info.st_uid,
                           gid=info.st_gid, mtime_ns=info.st_mtime_ns, kind="directory" if path.is_dir() else "file")
                if row["kind"] == "file":
                    row.update(bytes=info.st_size, sha256=deployment.file_hash(path)[0])
                    payload += info.st_size
                else:
                    stack.append((path, os.scandir(path)))
                raw = (json.dumps(row, sort_keys=True) + "\n").encode()
                require(sink.write(raw) == len(raw), "short original bundle inventory write; no resend")
                digest.update(raw)
                entries += 1
        except BaseException as error:
            original = error
            raise
        finally:
            failures = []
            for _, iterator in reversed(stack):
                try:
                    iterator.close()
                except BaseException as closing:
                    failures.append(closing)
            if failures:
                if original is None:
                    original = failures[0]
                    original.independent_close_failures = [*getattr(original, "independent_close_failures", []), *(str(error) for error in failures[1:])]
                    raise original
                original.independent_close_failures = [*getattr(original, "independent_close_failures", []), *(str(error) for error in failures)]
    return dict(inventory=str(output), sha256=digest.hexdigest(), entries=entries, regular_bytes=payload,
                read_window_bytes=65536, resident_state="directory-depth iterator stack plus one row/window; no payload-sized buffer")


def run(config_path, case, cache, output):
    started = time.monotonic()
    deadline = started + WALL_SECONDS
    config_path = deployment.owned_input(config_path)
    config = read_json(config_path)
    selection, plan, fixture = validate(config, case, cache)
    before(deadline, "after_reference_admission")
    output = author.temporary(output)
    require(not os.path.lexists(output), "exclusive controller output already exists")
    output.mkdir()
    (output / "operations").mkdir()
    environment = output / "environment.txt"
    with author.OutputFile(environment, "reference_environment_write") as stream:
        raw = "".join(name + "=" + value + "\n" for name, value in sorted(workloads.ENV.items())).encode()
        require(stream.write(raw) == len(raw), "short original environment write; no resend")
    nonce = secrets.token_hex(32)
    argv = [config["runtime_binary"], "serve", "--socket", config["socket"], "--volume", config["volume"],
            "--daemon", config["daemon_binary"], "--manifest", config["manifest"], "--receipt", str(output / "runtime.events.jsonl"),
            "--uid", "501", "--gid", "20", "--environment-file", str(environment), "--observation-scope", nonce]
    result = dict(schema=SCHEMA, status="INCOMPLETE", selection=selection, config_sha256=sha(config_path), controller_sha256=sha(__file__),
                  operations=[], performance_samples=0, admission_eligible=False, outer_wall_stop_seconds=WALL_SECONDS,
                  stage_wall_stop_seconds=300, author_wall_stop_seconds=300, other_action_wall_stop_seconds=15,
                  source_dirty=True, qualification="FUNCTIONAL_SETUP_ONLY; no clean-source or performance qualification",
                  global_profile="Disposable/WAL/OFF", overlay_profile="MEMORY/OFF/EXCLUSIVE", construction_workers=1,
                  reference_reuse="NEVER a mutated N/P sample/control", volume=config["volume"], volume_clone_identity=config["volume_clone_identity"],
                  container_stop="NOT_ATTEMPTED", setup_helper_uid=0, original_workload_uid=501, original_workload_gid=20)
    write_json(output / "prospective.json", result)
    runtime, runtime_constructed = None, False
    try:
        before(deadline, "before_original_runtime_launch")
        result["current_phase"] = "original_runtime_launch"
        runtime = EventProcess(argv, output, "runtime")
        runtime_constructed = True
        result["current_phase"] = "original_protocol_ready"
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 15))
        require(runtime.container, "original SDK container acknowledgement missing")
        result["current_phase"] = "original_staging_admission"
        require(deadline - time.monotonic() >= 300, "full original staging allowance cannot fit outer setup wall stop; no launch")
        staged = deployment.stage(config, selection, runtime.container, output / "staging")
        require(staged["container"] == runtime.container, "original staging container acknowledgement differs")
        before(deadline, "after_original_staging")
        result["current_phase"] = "original_reference_input_declaration"
        inputs = input_declaration(config, selection, plan, staged, fixture)
        inputs_path, spec_path = output / "inputs.json", output / "spec.json"
        write_json(inputs_path, inputs)
        spec_receipt = author.emit_spec(SimpleNamespace(case=case, cache_class=cache or "NONE", inputs=inputs_path,
                                                       inputs_sha256=sha(inputs_path), output=spec_path))
        require(spec_receipt["status"] == "DECLARED_SETUP_ONLY", "original reference spec is unavailable")
        spec = dict(value=read_json(spec_path), sha256=sha(spec_path))
        staged_path = output / "staging/receipt.json"
        staging = dict(value=staged, sha256=sha(staged_path))
        inside = "/tmp/layerfs-r7-reference-" + nonce
        inside_spec, inside_staging, inside_bundle = inside + "/spec.json", inside + "/staging.json", inside + "/bundle"
        prepare = "import os,sys; p=sys.argv[1]; os.mkdir(p,0o755)"
        cli_call(config, runtime, output, result, "prepare-author-inputs", ["exec", "--user", "0:0", runtime.container, "python3", "-B", "-c", prepare, inside], 15, deadline)
        cli_call(config, runtime, output, result, "copy-spec", ["cp", str(spec_path), runtime.container + ":" + inside_spec], 15, deadline)
        cli_call(config, runtime, output, result, "copy-staging", ["cp", str(staged_path), runtime.container + ":" + inside_staging], 15, deadline)
        asset = "/code/oracles/" + case + "-" + (cache or "NONE")
        command = ["exec", "--user", "0:0", runtime.container, "python3", "-B", "/code/oracle_prepare.py", "author",
                   "--spec", inside_spec, "--spec-sha256", spec["sha256"], "--staging-receipt", inside_staging,
                   "--staging-receipt-sha256", staging["sha256"], "--container", runtime.container,
                   "--output", inside_bundle, "--asset-root", asset, "--setup-wall-seconds", "300"]
        result["current_phase"] = "original_author_admission"
        require(deadline - time.monotonic() >= 300, "full original author allowance cannot fit outer setup wall stop; no launch")
        result["original_author"] = cli_call(config, runtime, output, result, "author", command, 300, deadline)
        bundle = output / "bundle"
        cli_call(config, runtime, output, result, "copy-bundle", ["cp", "-a", runtime.container + ":" + inside_bundle, str(bundle)], 15, deadline)
        result["current_phase"] = "copied_bundle_validation"
        result["closed_bundle"] = check_bundle(bundle, spec, staging, asset, config, result["original_author"])
        result["complete_bundle_inventory"] = seal_bundle(bundle, output / "bundle.inventory.jsonl", deadline)
        before(deadline, "before_normal_EndSession")
        result["container_stop"] = "ORIGINAL_STOP_SENT_OUTCOME_PENDING"
        result["current_phase"] = "normal_EndSession_stop"
        result["stop_event"] = runtime.send("stop", deadline=min(deadline, time.monotonic() + 15))
        result["container_stop"] = "KNOWN_STOP"
        result["current_phase"] = "original_runtime_finished"
        runtime.event("finished", min(deadline, time.monotonic() + 15))
        close_runtime(runtime, only={"stdin"})
        before(deadline, "before_original_runtime_exit")
        result["current_phase"] = "original_runtime_exit"
        code = runtime.process.wait(timeout=min(15, deadline - time.monotonic()))
        result["known_host_exit"] = code
        require(code == 0, "original SDK runtime exit nonzero")
        close_runtime(runtime)
        before(deadline, "complete_reference_controller")
        result.update(status="CLOSED_EXPECTED_FUNCTIONAL_SETUP_ONLY", EndSession="original acknowledged SessionEnded and container stop")
    except BaseException as original:
        result.update(status="INCOMPLETE", original_failure_type=type(original).__name__, original_failure=str(original),
                      original_phase=getattr(original, "original_phase", result.get("current_phase")), independent_close_failures=getattr(original, "independent_close_failures", []),
                      independent_output_failures=getattr(original, "independent_output_failures", []),
                      staging_receipt=getattr(original, "staging_receipt", None), staging_output=getattr(original, "staging_output", None))
        if runtime is None:
            runtime = getattr(original, "event_process", None)
        if runtime is not None:
            try:
                result["retained_custody"] = runtime.retain(original)
            except BaseException as secondary:
                result["independent_host_fence_failure"] = dict(type=type(secondary).__name__, cause=str(secondary))
            if runtime_constructed:
                close_runtime(runtime, original)
            result["independent_close_failures"] = getattr(original, "independent_close_failures", [])
    if runtime is not None:
        result.update(container=runtime.container, exec_ids=runtime.exec_ids, daemon_instance=runtime.daemon_instance,
                      child_event_receipts=runtime.child_event_receipts)
    result.update(setup_elapsed_ns=int((time.monotonic() - started) * 1e9), setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  memory_scope="controller lifetime high-water in native platform units; setup maps/files outside product and phase bounds")
    try:
        write_json(output / "result.json", result)
    except BaseException as secondary:
        result.update(status="INCOMPLETE", independent_result_output_failure=dict(type=type(secondary).__name__, cause=str(secondary)))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--case", required=True)
    parser.add_argument("--cache-class", choices=["A", "B", "C", "NONE"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = run(args.config, args.case, None if args.cache_class == "NONE" else args.cache_class, args.output)
    except BaseException as original:
        result = dict(schema=SCHEMA, status="INCOMPLETE", original_failure_type=type(original).__name__, original_failure=str(original),
                      original_phase=getattr(original, "original_phase", "pre_container_admission"),
                      independent_close_failures=getattr(original, "independent_close_failures", []),
                      container="NOT_CREATED_BY_CONTROLLER", partial_output_retained=os.path.lexists(args.output))
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "CLOSED_EXPECTED_FUNCTIONAL_SETUP_ONLY" else 1


if __name__ == "__main__":
    raise SystemExit(main())

"""Closed untimed native reference oracles; lead-run setup, never an N sample.

`spec` runs on the host and freezes the actual registry declaration. `author`
runs in the already acknowledged/staged reference container. No clone, Init,
network, image build, reset, failed-operation replay or implicit container stop.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import resource
import shlex
import signal
import stat
import subprocess
import sys
import time

sys.dont_write_bytecode = True

IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"
WINDOW = 65536
SETUP_SECONDS = 300
VERIFIER_SECONDS = 9.5
SUPPORTED = {"E02", "E04", "E05", "E06", "E10", "E11", "E12", "E13", "E14", "E15", "E16", "E17", "E18", "C12",
             "K01", "K02", "K03", "K04", "K05", "W01", "W02"} | {"C%02d" % i for i in range(1, 12)}
PENDING = {"E01": "ownership oracle belongs to the runner", "E03": "exact timed tar stream unavailable",
           "E07": "Node prerequisite absent", "E08": "owner excluded", "E09": "no normalized oracle",
           "E19": "sealed before/related-root identities, original 282-edit preparation custody, known mounted Commit lineage or two host-prepared related roots, and rederived original status stdout are not provided"}
GIT_CASES = {"E04", "E10", "E11", "E18", "C12"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def sha(path):
    digest = hashlib.sha256()
    with ReadFile(path, "payload_hash_read") as stream:
        for block in iter(lambda: stream.read(WINDOW), b""):
            digest.update(block)
    return digest.hexdigest()


class CustodyFile:
    """One unbuffered file owner; preserve first operation and close separately."""
    def __init__(self, path, mode, phase):
        self.phase = phase
        try:
            self.stream = Path(path).open(mode, buffering=0)
        except BaseException as original:
            if not hasattr(original, "original_phase"):
                original.original_phase = phase + "_open"
            raise

    def __enter__(self):
        return self.stream

    def __exit__(self, kind, original, traceback):
        if original is not None and not hasattr(original, "original_phase"):
            original.original_phase = self.phase
        try:
            self.stream.close()
        except OSError as closing:
            if original is None:
                if not hasattr(closing, "original_phase"):
                    closing.original_phase = self.phase + "_close"
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(closing)]
        return False


class OutputFile(CustodyFile):
    def __init__(self, path, phase="receipt_write"):
        super().__init__(path, "xb", phase)


class ReadFile(CustodyFile):
    def __init__(self, path, phase="original_file_read"):
        super().__init__(path, "rb", phase)


def read_json(path):
    with ReadFile(path, "closed_json_read") as stream:
        return json.load(stream)


def write_new(path, value):
    raw = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()
    with OutputFile(path) as stream:
        delivered = stream.write(raw)
        require(delivered == len(raw), f"short original receipt write: {delivered}/{len(raw)}; no resend")


def temporary(path, existing=False):
    path = Path(path).absolute()
    root = Path("/tmp").resolve()
    require(".." not in path.parts, "temporary path has parent traversal")
    require(path.parts[:2] == ("/", "tmp") or root in path.parents, "output/input evidence must be temporary")
    relative = path.relative_to("/tmp") if path.parts[:2] == ("/", "tmp") else path.relative_to(root)
    actual = root
    for index, part in enumerate(relative.parts):
        actual /= part
        try:
            info = actual.lstat()
        except FileNotFoundError:
            require(not existing and index == len(relative.parts)-1, "only final output may be absent")
            break
        require(not stat.S_ISLNK(info.st_mode), "temporary evidence path must not follow symlinks")
    require(any(part.startswith("layerfs-r7-") for part in relative.parts), "owned temporary evidence required")
    return actual


def load_helper(name):
    path = Path("/code") / (name + ".py")
    module_spec = importlib.util.spec_from_file_location("r7_closed_" + name, path)
    module = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(module)
    return module


def native(path):
    path = Path(path)
    require(str(path) == path.as_posix() and len(path.parts) == 2 and
            (str(path) == "/native" or str(path).startswith("/native-")), "declared native reference root required")
    require(path.resolve(strict=True) == path and stat.S_ISDIR(path.lstat().st_mode), "reference root must not alias another path")
    return path


def inventory_path(path):
    path = Path(path)
    require(path.is_absolute() and ".." not in path.parts and
            (str(path).startswith("/code/") or str(path).startswith("/tmp/layerfs-r7-")), "staged inventory path required")
    current = Path("/")
    for part in path.parts[1:]:
        current /= part
        require(not stat.S_ISLNK(current.lstat().st_mode), "inventory path must not follow symlinks")
    return path


def not_run(case, cache):
    if case in PENDING:
        return PENDING[case]
    if case not in SUPPORTED:
        return "oracle scope not assigned to this author"
    if case == "E13" and cache == "C":
        return "identical retained warmup creates .experiment-store; second original mkdir is nonrepeatable"
    if case == "E18" and cache == "C":
        return "identical retained status warmup can refresh the index; authorized unrefreshed E18 cannot claim a refreshed fast path"
    if case == "C12" and cache == "C":
        return "canonical init/commit body on the retained initialized root is nonrepeatable; no reset authorized"
    return None


def emit_spec(args):
    repository = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(repository / "core/benchmark/fs-bench-pro"))
    from r7 import registry, workloads
    case = next(item for item in registry.cases() if item["case_id"] == args.case)
    cache = None if args.cache_class == "NONE" else args.cache_class
    require(cache in case["classes"], "cache class is not registered for this case")
    inputs_path = temporary(args.inputs, True)
    require(sha(inputs_path) == args.inputs_sha256, "closed reference-input declaration differs")
    inputs = read_json(inputs_path)
    require(isinstance(inputs.get("fixture_identity"), dict) and inputs["fixture_identity"], "fixture/cut identity required")
    roots = inputs["roots"]
    require(len(roots) == (2 if args.case == "W01" else 1), "exact reference peer population required")
    roles = ["scan", "writer"] if args.case == "W01" else ["primary"]
    require([row["role"] for row in roots] == roles, "reference root roles differ")
    for row in roots:
        path = Path(row["path"])
        require(len(path.parts) == 2 and (str(path) == "/native" or str(path).startswith("/native-")), "native input path required")
        require(row.get("inventory_sha256") and row.get("content_metadata_set_sha256"), "closed root inventory seals required")
    if args.case in {"C09", "C10", "C11"} and inputs.get("precondition_applied"):
        require(inputs.get("precondition_identity"), "reused big-file prerequisite needs its original identity")
    required_assets = {"largest-path"} if args.case == "E15" else {"tracked-paths.json"} if args.case == "E11" else {"node-roots.json"} if args.case in {"E12", "E13", "E14", "K02", "K03"} else set()
    require(required_assets <= set(inputs.get("code_assets", {})), "selected closed code-data assets missing")
    if args.case in {"E12", "E13", "K02", "K03"}:
        require(any(row.get("path") == "/replay" for row in inputs.get("external", [])), "closed replay inventory required")
    if args.case in GIT_CASES:
        policy = inputs.get("git_default_policy", {})
        digest = policy.get("sha256")
        require(isinstance(policy.get("path"), str) and Path(policy["path"]).parent == Path("/code") and
                isinstance(digest, str) and len(digest) == 64 and all(c in "0123456789abcdef" for c in digest) and
                inputs.get("code_assets", {}).get(Path(policy["path"]).name) == digest,
                "closed qualified Git policy asset identity required")
    if args.case == "C12":
        require(inputs["fixture_identity"].get("fixture_kind") == "empty", "C12 requires declared independently sealed empty fixture")
    sources = {"oracle.py": repository / "core/benchmark/fs-bench-pro/r7/oracle.py",
               "workload.py": repository / "core/benchmark/fs-bench-pro/r7/workload.py",
               "r7_deployment.py": repository / "core/benchmark/fs-bench-pro/r7/deployment.py",
               "oracle_prepare.py": Path(__file__).resolve()}
    if args.case in GIT_CASES:
        sources.update({name: repository / "core/benchmark/fs-bench-pro/r7" / name
                        for name in ("git_queries.py", "git_index_oracle.py", "workloads.py")})
    spec = dict(schema="r7-oracle-preparation-spec-v1", case_id=args.case, cache_class=cache,
                status="NOT_RUN" if not_run(args.case, cache) else "DECLARED_SETUP_ONLY",
                reason=not_run(args.case, cache), case=case, environment=workloads.ENV,
                image_id=registry.IMAGE, uid=501, gid=20, inputs=inputs,
                input_declaration=dict(path=str(inputs_path), sha256=args.inputs_sha256),
                helpers={name: sha(path) for name, path in sources.items()},
                registry_sha256=sha(registry.__file__), workload_source_sha256=sha(workloads.__file__),
                setup_wall_stop_seconds=SETUP_SECONDS, verifier_wall_stop_ns=9_500_000_000,
                resource_domain="untimed independent reference author; no N sample/control reuse")
    if args.case in GIT_CASES:
        from r7 import registry_variant
        spec.update(oracle_variant=registry_variant.NAME, registry_variant_identity=registry_variant.identity(),
                    registry_variant_source_sha256=sha(registry_variant.__file__))
    output = temporary(args.output)
    write_new(output, spec)
    return dict(schema=spec["schema"], status=spec["status"], spec=str(output), sha256=sha(output), reason=spec["reason"])


def parallel(roots, commands):
    require(len(roots) == len(commands) == 2, "exact registered parallel peer population required")
    lines = ["pids=(); failure=0"]
    for root, body in zip(roots, commands):
        lines.append(f"(cd -- {shlex.quote(str(root))} && /bin/bash -o pipefail -c {shlex.quote(body)}) & pids+=(\"$!\")")
    lines.append('for pid in "${pids[@]}"; do if wait "$pid"; then code=0; else code=$?; if test "$failure" -eq 0; then failure=$code; fi; fi; printf "R7_CHILD_RESULT pid=%s exit=%s\\n" "$pid" "$code" >&2; done; exit "$failure"')
    return "\n".join(lines)


def body_argv(command):
    """Preserve registered Bash argv; textual K bodies use the same launcher."""
    if isinstance(command, str):
        return ["/bin/bash", "-o", "pipefail", "-c", command]
    require(isinstance(command, list) and len(command) == 5 and
            command[:4] == ["/bin/bash", "-o", "pipefail", "-c"] and isinstance(command[4], str),
            "registered ordinary Bash argv required; no coercion or alternate launcher")
    return list(command)


def reference_schedule(case, cache, declaration, roots, body, observe):
    """One native reference schedule; callbacks own original I/O and custody.

    Checkpoints bind to the immediately preceding original body. Observers are
    separate operations, and an original failure prevents every later step.
    """
    require(len(roots) == (2 if case == "W01" else 1) and len(set(roots)) == len(roots),
            "exact independent reference peer roots required")
    require(not not_run(case, cache), "unsupported/nonrepeatable reference schedule")
    repeats = ["warmup", "expectation"] if cache == "C" else ["expectation"]
    result = {}
    if case.startswith("K"):
        commands = declaration["commands"]
        require(cache is None and len(commands) == declaration["commit_count"] > 0,
                "registered Commit checkpoint schedule required")
        for index, command in enumerate(commands, 1):
            label = "expectation-" + str(index)
            result["last"] = body(label, body_argv(command), roots[0])
            observe("checkpoint-" + str(index), roots[0], command_labels=[label], checkpoint=index)
        result["commit_custody"] = "NOT_APPLICABLE to native reference; each expected checkpoint corresponds to one actual product Commit"
    elif case.startswith("W"):
        commands = declaration["commands"]
        expected_roles = ["scan", "write"] if case == "W01" else ["shared", "shared", "shared"]
        require([row["workspace"] for row in commands] == expected_roles, "registered concurrency peer roles differ")
        if case == "W02":
            require([row["stage"] for row in commands] == [0, 1, 1], "registered shared-Workspace stages differ")
        for label in repeats:
            if case == "W02":
                body(label + "-sequential", body_argv(commands[0]["argv"]), roots[0])
                children, peers = commands[1:], [roots[0], roots[0]]
            else:
                children, peers = commands, roots
            text = parallel(peers, [body_argv(item["argv"])[-1] for item in children])
            result["last"] = body(label + "-parallel", body_argv(text), roots[0])
        if case == "W01":
            observe("scan", roots[0], "scan", command_labels=["expectation-parallel"])
            observe("writer", roots[1], "writer", command_labels=["expectation-parallel"])
        else:
            observe("shared", roots[0], command_labels=["expectation-sequential", "expectation-parallel"])
    else:
        for label in repeats:
            result["last"] = body(label, body_argv(declaration["argv"]), roots[0])
        if case not in {"E05", "E06"}:
            observe("expected", roots[0], command_labels=["expectation"])
    return result


def ordinary_identity():
    if os.geteuid() == 0:
        os.setgroups([])
        os.setgid(20)
        os.setuid(501)
    else:
        require((os.geteuid(), os.getegid()) == (501, 20), "reference command must run as 501:20")


def require_empty(root, fixture):
    require(fixture.get("fixture_kind") == "empty", "canonical Git init requires declared empty fixture")
    iterator, original = os.scandir(root), None
    try:
        require(next(iterator, None) is None, "canonical Git init requires actual verified root empty before original body")
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            iterator.close()
        except OSError as closing:
            if original is None:
                raise
            original.independent_close_failures = [*getattr(original,"independent_close_failures",[]),str(closing)]


class ReferenceFailure(RuntimeError):
    def __init__(self, original, custody, output_errors, origin_phase):
        super().__init__(str(original))
        self.original, self.custody, self.output_errors = original, custody, output_errors
        self.origin_phase = origin_phase


def invoke(argv, root, output, label, environment, deadline, allowance, steps):
    require(time.monotonic() < deadline, "declared setup stop expired before next operation")
    step_deadline = min(deadline, time.monotonic() + allowance)
    attempt = dict(label=label, argv=argv, directory=str(root), uid=501, gid=20,
                   attempts=1, stdout=str(output / (label + ".stdout")), stderr=str(output / (label + ".stderr")),
                   wall_stop_seconds=allowance, status="ORIGINAL_PENDING")
    steps.append(attempt)
    process, original, output_errors, origin_phase = None, None, [], "output_admission"
    try:
        with OutputFile(attempt["stdout"]) as stdout, OutputFile(attempt["stderr"]) as stderr:
            origin_phase = "command_launch"
            process = subprocess.Popen(argv, cwd=root, env=environment, stdin=subprocess.DEVNULL,
                                       stdout=stdout, stderr=stderr, start_new_session=True,
                                       preexec_fn=ordinary_identity)
            attempt.update(pid=process.pid, process_group=process.pid)
            origin_phase = "custody_output"
            print(json.dumps(dict(schema="r7-reference-process-custody-v1", event="started", **attempt)), flush=True)
            write_new(output / (label + ".attempt.json"), attempt)
            origin_phase = "command_wait"
            try:
                code = process.wait(timeout=max(0, step_deadline-time.monotonic()))
            except subprocess.TimeoutExpired as error:
                original = error
                attempt.update(status="TIMEOUT", cancellation="one SIGKILL to exact original reference group; no rollback/drain inferred")
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    attempt["cancel_result"] = "already absent; original exit not inferred"
                except OSError as cancellation_error:
                    attempt["independent_cancellation_failure"] = str(cancellation_error)
                try:
                    attempt["exit_after_cancel"] = process.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    attempt["exit_after_cancel"] = "UNAVAILABLE"
                raise original
            attempt.update(status="COMPLETED", exit_code=code)
            origin_phase = "command_exit"
            require(code == 0, "original reference operation failed: " + label + " exit=" + str(code))
    except Exception as error:
        if original is None:
            original = error
            if attempt.get("exit_code") == 0:
                origin_phase = "stream_owner_close"
        output_errors.extend(getattr(error, "independent_close_failures", []))
    try:
        write_new(output / (label + ".result.json"), attempt)
    except Exception as error:
        output_errors.append(str(error))
        if original is None:
            original = error
            origin_phase = "receipt_output"
    if original is not None:
        raise ReferenceFailure(original, attempt, output_errors, origin_phase)
    return attempt


def observe_known(root, selection, output):
    oracle = load_helper("oracle")
    root = Path(root).resolve(strict=True)
    output = Path(output)
    require(root not in output.resolve().parents, "oracle evidence must live outside reference view")
    if selection == "small":
        directory = root / "r7-small"
        require(stat.S_ISDIR(directory.lstat().st_mode) and
                set(os.listdir(directory)) == {"f" + str(i) for i in range(1, 25)}, "exact known small-change names required")
        for index in range(1, 25):
            path = directory / ("f" + str(index))
            require(stat.S_ISREG(path.lstat().st_mode), "known small-change payload must be regular")
            with ReadFile(path, "known_payload_read") as stream:
                require(stream.read(32) == ("r7-%s\n" % index).encode() and not stream.read(1), "known small-change bytes differ")
        return dict(oracle.observe(root / "r7-small", "C00", output), status="OBSERVED")
    if selection == "log":
        selected = [root / "experiment.log"]
    elif selection == "large":
        selected = [root / "experiment-large"]
    else:
        prefix = "r7-increment-" if selection == "increments" else "experiment-temp-"
        with os.scandir(root) as entries:
            selected = [root / entry.name for entry in entries if entry.name.startswith(prefix)]
        selected.sort()
    def selected_paths():
        for path in selected:
            yield path
            if stat.S_ISDIR(path.lstat().st_mode):
                stack = [os.scandir(path)]
                try:
                    while stack:
                        try:
                            entry = next(stack[-1])
                        except StopIteration:
                            stack.pop().close()
                            continue
                        child = Path(entry.path)
                        yield child
                        if entry.is_dir(follow_symlinks=False):
                            stack.append(os.scandir(child))
                finally:
                    for iterator in stack:
                        iterator.close()
    count = 0
    with OutputFile(output, "known_manifest_write") as stream:
        for path in selected_paths():
            info = path.lstat()
            kind = "regular" if stat.S_ISREG(info.st_mode) else "directory" if stat.S_ISDIR(info.st_mode) else "symlink" if stat.S_ISLNK(info.st_mode) else "unsupported"
            row = dict(path=path.relative_to(root).as_posix(), kind=kind, mode=stat.S_IMODE(info.st_mode),
                       uid=info.st_uid, gid=info.st_gid, size=info.st_size, nlink=info.st_nlink,
                       device=info.st_dev, inode=info.st_ino, mtime_ns=info.st_mtime_ns, ctime_ns=info.st_ctime_ns)
            if kind == "regular":
                row["sha256"] = sha(path)
                if selection == "log":
                    known = hashlib.sha256()
                    for _ in range(10000):
                        known.update(b"x" * 99 + b"\n")
                    require(info.st_size == 1000000 and row["sha256"] == known.hexdigest(), "known append log bytes differ")
            elif kind == "symlink":
                row["target"] = os.readlink(path)
            raw = (json.dumps(row, sort_keys=True) + "\n").encode()
            require(stream.write(raw) == len(raw), "short original known-manifest write; no resend")
            count += 1
    return dict(schema="r7-known-path-observation-v1", status="OBSERVED", selection=selection,
                manifest_sha256=sha(output), paths=count, scope="declared known paths/prefix only")


def observer_for(case, role="primary"):
    if case in GIT_CASES:
        return dict(kind="git", case=case, compare_case=case, oracle_schema="r7-git-index-scoped-v2")
    if case in {"K02", "K03"}:
        source_case = "E12" if case == "K02" else "E13"
        return dict(kind="port", case=source_case, compare_case=source_case)
    if case in {"K01", "K05"}:
        return dict(kind="known", selection="small", compare_case="C00")
    if case == "K04":
        return dict(kind="known", selection="increments", compare_case="C00")
    if case in {"E15", "E16", "E17"} or case.startswith("W") and role != "scan":
        return dict(kind="known", selection={"E15": "large", "E17": "temporary"}.get(case, "log"), compare_case=case if case.startswith("E") else "C00")
    return dict(kind="port", case="E02" if role == "scan" else case, compare_case="E02" if role == "scan" else case)


def observe_command(observer, root, output):
    if observer["kind"] == "known":
        return ["python3", "-B", "/code/oracle_prepare.py", "observe-known", "--root", str(root),
                "--selection", observer["selection"], "--output", str(output)]
    return ["python3", "-B", "/code/oracle.py", "observe", "--root", str(root),
            "--case", observer["case"], "--output", str(output)]


def verifier_body(asset, observer, expected, git_index=None):
    """Canonical comparison-performing script text for author and sealed loader."""
    if observer["kind"] == "git":
        require(isinstance(git_index, dict), "sealed Git comparison operands required")
        argv = ["python3", "-B", "/code/git_queries.py", "verify", "--root", "$PWD", "--case", observer["case"],
                "--output", "$r7_git_actual", "--expected-tree", asset + "/" + expected + ".jsonl",
                "--expected-tree-sha256", git_index["tree_sha256"]]
        for option, member in (("expected-index", "manifest"), ("pin", "pin"), ("policy", "policy")):
            argv.extend(["--" + option, asset + "/" + git_index[member], "--" + option + "-sha256", git_index[member + "_sha256"]])
        if observer["case"] == "E11":
            require(git_index.get("tracked") == "/code/tracked-paths.json" and git_index.get("tracked_sha256"), "sealed E11 tracked operand required")
            argv.extend(["--tracked", git_index["tracked"], "--tracked-sha256", git_index["tracked_sha256"]])
        rendered = " ".join('"' + value + '"' if value in {"$PWD", "$r7_git_actual"} else shlex.quote(value) for value in argv)
        nonce = 'import secrets; print("/tmp/layerfs-r7-git-actual-"+secrets.token_hex(16))'
        return "set -euo pipefail\nr7_git_actual=$(python3 -B -c " + shlex.quote(nonce) + ")\n" + rendered + "\n"
    actual = "$r7_oracle_actual"
    argv = observe_command(observer, "$PWD", actual)
    render = lambda values: " ".join('"' + value + '"' if value in {"$PWD", actual} else shlex.quote(value) for value in values)
    command = render(argv)
    compare = ["python3", "-B", "/code/oracle.py", "compare", "--case", observer["compare_case"],
               "--expected", asset + "/" + expected + ".jsonl", "--actual", actual]
    nonce = 'import secrets; print("/tmp/layerfs-r7-actual-"+secrets.token_hex(16)+".jsonl")'
    return "set -euo pipefail\nr7_oracle_actual=$(python3 -B -c " + shlex.quote(nonce) + ")\n" + command + " >/dev/null\n" + render(compare) + "\n"


def verifier_script(output, asset, observer, expected, git_index=None):
    name = expected + ".verify.sh"
    raw = verifier_body(asset, observer, expected, git_index).encode()
    with OutputFile(output / name, "verifier_script_write") as stream:
        require(stream.write(raw) == len(raw), "short original verifier-script write; no resend")
    return dict(path=asset + "/" + name, sha256=sha(output / name), performs_comparison=True,
                wall_stop_ns=9_500_000_000, root="original selected mounted cwd", expected=expected + ".jsonl")


def author(args):
    require(os.uname().sysname == "Linux" and os.geteuid() in {0, 501}, "author runs only inside declared native Linux reference container")
    spec_path, staging_path = temporary(args.spec, True), temporary(args.staging_receipt, True)
    require(sha(spec_path) == args.spec_sha256 and sha(staging_path) == args.staging_receipt_sha256, "closed author inputs differ")
    spec, staging = read_json(spec_path), read_json(staging_path)
    require(spec["schema"] == "r7-oracle-preparation-spec-v1" and spec["image_id"] == IMAGE and
            (spec["uid"], spec["gid"]) == (501, 20), "declared image/user/spec differs")
    require(staging.get("schema") == "r7-container-staging-v1" and staging.get("status") == "PASS" and
            staging.get("container") == args.container and staging.get("image_id") == IMAGE and
            (staging.get("uid"), staging.get("gid")) == (501, 20), "actual acknowledged staging identity incomplete")
    require(args.setup_wall_seconds == spec["setup_wall_stop_seconds"] == SETUP_SECONDS, "frozen setup wall stop differs")
    require(args.asset_root.startswith("/code/oracles/") and str(Path(args.asset_root)) == args.asset_root and
            ".." not in Path(args.asset_root).parts and not any(c in args.asset_root for c in "\n\r\t"), "closed /code/oracles asset path required")
    for name, digest in spec["helpers"].items():
        require(sha(Path("/code") / name) == digest and staging["helper_sha256"].get(name) == digest,
                "actual staged helper identity differs: " + name)
    for name, digest in spec["inputs"].get("code_assets", {}).items():
        require(Path(name).name == name and sha(Path("/code") / name) == digest and
                staging["helper_sha256"].get(name) == digest, "closed code-data asset differs")
    output = temporary(args.output)
    output.mkdir()
    if os.geteuid() == 0:
        os.chown(output, 501, 20, follow_symlinks=False)
    output.chmod(0o755)
    result = dict(schema="r7-closed-oracle-author-v1", status="INCOMPLETE", spec_sha256=args.spec_sha256,
                  staging_receipt_sha256=args.staging_receipt_sha256, container=args.container,
                  case_id=spec["case_id"], cache_class=spec["cache_class"], fixture_identity=spec["inputs"]["fixture_identity"],
                  environment=spec["environment"], image_id=IMAGE, uid=501, gid=20, steps=[], expected=[],
                  setup_wall_stop_seconds=SETUP_SECONDS, verifier_wall_stop_ns=9_500_000_000,
                  admission_eligible=False, performance_claim="NONE; reference author is untimed setup, never N control/sample")
    start, deadline = time.monotonic_ns(), time.monotonic() + SETUP_SECONDS - 1
    try:
        require(spec["status"] == "DECLARED_SETUP_ONLY" and not not_run(spec["case_id"], spec["cache_class"]), spec.get("reason") or "unassigned oracle scope")
        deployment = load_helper("r7_deployment")
        roots = []
        for row in spec["inputs"]["roots"]:
            root = native(row["path"])
            require((root.stat().st_uid, root.stat().st_gid) == (501, 20), "reference root owner differs")
            inventory = inventory_path(row["inventory"])
            require(sha(inventory) == row["inventory_sha256"], "closed native inventory hash differs")
            original = next((value for value in staging["verifications"] if value.get("root") == str(root)), None)
            require(original and original["inventory_sha256"] == row["inventory_sha256"] and
                    original["content_metadata_set_sha256"] == row["content_metadata_set_sha256"], "actual reference root not verified by original staging")
            require((root.stat().st_dev, root.stat().st_ino) == (original["root_device"], original["root_inode"]), "actual staged root physical identity changed")
            require(deployment.verify_tree(root, inventory)["status"] == "PASS", "fresh reference bytes/metadata differ")
            require(root not in output.parents, "output overlaps reference root")
            roots.append(root)
        require(len(set(roots)) == len(roots), "reference peer roots alias")
        for row in spec["inputs"].get("external", []):
            require(row["path"] == "/replay", "only closed replay is an external payload input")
            inventory = inventory_path(row["inventory"])
            require(sha(inventory) == row["inventory_sha256"], "external inventory seal differs")
            require(deployment.verify_tree(Path("/replay"), inventory)["content_metadata_set_sha256"] == row["content_metadata_set_sha256"], "closed replay contents differ")
        root_key = hashlib.sha256(json.dumps([args.container, [(str(root), root.stat().st_dev, root.stat().st_ino) for root in roots]]).encode()).hexdigest()
        write_new(Path("/tmp") / ("layerfs-r7-oracle-reference-" + root_key + ".claim"), dict(spec_sha256=args.spec_sha256, output=str(output), attempts=1))
        case, cache, declaration = spec["case_id"], spec["cache_class"], spec["case"]["workload"]
        if case == "C12":
            require_empty(roots[0], spec["inputs"]["fixture_identity"])
        allowance = min(120, spec["case"]["complete_command_wall_stop_ns"] / 1e9)
        env = spec["environment"]
        git = None
        def collect_git():
            result["phase"] = dict(kind="original_read_only_Git_queries")
            policy = spec["inputs"]["git_default_policy"]
            step = invoke(["python3", "-B", "/code/git_queries.py", "collect", "--root", str(roots[0]),
                           "--case", case,
                           "--policy", policy["path"], "--policy-sha256", policy["sha256"],
                           "--output", str(output / "git-queries")], roots[0], output, "git-context",
                          env, deadline, VERIFIER_SECONDS, result["steps"])
            git = read_json(step["stdout"])
            require(git.get("schema") == "r7-original-git-queries-v1" and git.get("status") == "OBSERVED",
                    "original actual Git context query receipt missing")
            with ReadFile(policy["path"], "qualified_policy_copy_read") as source, \
                    OutputFile(output / "git-default-policy.json", "qualified_policy_copy_write") as target:
                for raw in iter(lambda: source.read(WINDOW), b""):
                    require(target.write(raw) == len(raw), "short original policy asset copy; no resend")
            require(sha(output / "git-default-policy.json") == policy["sha256"], "original policy copy identity differs")
            result["git_context"] = dict(oracle_schema="r7-git-index-scoped-v2", actual_pin=git["pin"],
                                         pin="git-queries/pin.json", pin_file_sha256=git["pin_file_sha256"],
                                         queries="git-queries/queries.json", queries_sha256=git["queries_sha256"],
                                         policy="git-default-policy.json", policy_sha256=policy["sha256"],
                                         witness_scope="actual query receipts retained; same sealed comparison pin shared by matched arms")
            result["git_context"]["query_position"] = "after original canonical body" if case == "C12" else "before original canonical body"
            return git
        if case in GIT_CASES and case != "C12":
            git = collect_git()
        def body(label, command, root=roots[0]):
            result["phase"] = dict(kind="original_reference_body", label=label)
            return invoke(body_argv(command), root, output, label, env, deadline, allowance, result["steps"])
        def observe(label, root, role="primary", *, command_labels, checkpoint=None):
            nonlocal git
            if case == "C12":
                require(git is None and command_labels == ["expectation"], "canonical init Git queries follow exactly one original body")
                git = collect_git()
            result["phase"] = dict(kind="independent_expected_observer", label=label)
            observer = observer_for(case, role)
            git_operand = None
            if case in GIT_CASES:
                payload = "\n".join(["import json,sys,time", "sys.dont_write_bytecode=True", "sys.path.insert(0,'/code')",
                                     "import git_index_oracle as index,oracle", "deadline=time.monotonic()+9.5",
                                     "pin=index.load_sealed(" + repr(git["pin_path"]) + "," + repr(git["pin_file_sha256"]) + ")",
                                     "observed=index.observe(" + repr(str(root)) + ",pin)",
                                     "index.write_new(" + repr(str(output / (label + ".index.json"))) + ",observed)",
                                     "tracked=None" if case != "E11" else "import git_queries; tracked=git_queries.tracked_paths('/code/tracked-paths.json'," + repr(spec["inputs"]["code_assets"]["tracked-paths.json"]) + ")",
                                     "oracle.observe(" + repr(str(root)) + "," + repr(case) + "," + repr(str(output / (label + ".jsonl"))) + ",tracked=tracked)",
                                     "assert time.monotonic()<deadline,'complete expected observer wall stop'", "print(json.dumps({'status':'OBSERVED'}))"])
                invoke(["python3", "-B", "-c", payload], root, output, label + "-observer", env,
                       deadline, VERIFIER_SECONDS, result["steps"])
                git_operand = dict(manifest=label + ".index.json", manifest_sha256=sha(output / (label + ".index.json")),
                                   pin="git-queries/pin.json", pin_sha256=git["pin_file_sha256"],
                                   queries="git-queries/queries.json", queries_sha256=git["queries_sha256"],
                                   policy="git-default-policy.json", policy_sha256=spec["inputs"]["git_default_policy"]["sha256"],
                                   tree_sha256=sha(output / (label + ".jsonl")))
                if case == "E11":
                    git_operand.update(tracked="/code/tracked-paths.json", tracked_sha256=spec["inputs"]["code_assets"]["tracked-paths.json"])
            else:
                invoke(observe_command(observer, root, output / (label + ".jsonl")), root,
                       output, label + "-observer", env, deadline, VERIFIER_SECONDS, result["steps"])
            script = verifier_script(output, args.asset_root, observer, label, git_operand)
            entry = dict(label=label, role=role, reference_root=str(root),
                                           original_reference_commands=command_labels, checkpoint_index=checkpoint,
                         manifest=label + ".jsonl", sha256=sha(output / (label + ".jsonl")), observer=observer, verifier=script)
            if git_operand is not None:
                entry["git_index"] = git_operand
            result["expected"].append(entry)
        prep = declaration.get("preparation_command")
        if prep and not spec["inputs"].get("precondition_applied"):
            body("prerequisite", prep)
        elif prep:
            result["reused_prerequisite"] = spec["inputs"]["precondition_identity"]
        schedule = reference_schedule(case, cache, declaration, roots, body, observe)
        last = schedule.pop("last")
        result.update(schedule)
        stdout_expected = Path(last["stdout"])
        result["host_stdout_recipe"] = dict(kind="exact-file-sha256" if not case.startswith("W") else "unordered-complete-lines",
                                           expected_asset=args.asset_root + "/" + stdout_expected.name,
                                           expected_bundle_relative=stdout_expected.name,
                                           expected_sha256=sha(stdout_expected), original_host_input="original command event fields.stdout; runner binds actual host file",
                                           required=True, container_stdout_path="NOT_USED")
        result.update(status="CLOSED_EXPECTED_SETUP_ONLY", asset_root=args.asset_root,
                      inputs=spec["inputs"],
                      helpers=spec["helpers"], registry_sha256=spec["registry_sha256"], workload_source_sha256=spec["workload_source_sha256"],
                      selected_workload=spec["case"]["workload"], native_verifications=staging["verifications"],
                      source_mutations="only declared original native reference bodies", root_claim=str(Path("/tmp") / ("layerfs-r7-oracle-reference-" + root_key + ".claim")))
        if case in GIT_CASES:
            result.update(oracle_variant=spec["oracle_variant"], registry_variant_identity=spec["registry_variant_identity"],
                          registry_variant_source_sha256=spec["registry_variant_source_sha256"])
    except Exception as error:
        result.update(status="NOT_RUN" if spec.get("reason") else "INCOMPLETE", original_failure_type=type(error).__name__,
                      original_failure=str(error), original_phase=getattr(error, "original_phase", result.get("phase")),
                      independent_close_failures=getattr(error, "independent_close_failures", []),
                      custody="original reference processes/output/root claim retained; no retry/reset/container stop")
        if isinstance(error, ReferenceFailure):
            key = ("original_query_failure" if error.custody["label"] == "git-context" else
                   "original_observer_failure" if error.custody["label"].endswith("-observer") else "original_body_failure") if error.origin_phase in {"command_launch", "command_wait", "command_exit"} else "original_control_output_failure"
            result[key] = dict(type=type(error.original).__name__, cause=str(error.original), phase=error.origin_phase, custody=error.custody)
            result["independent_output_failures"] = error.output_errors
    result.update(setup_ns=time.monotonic_ns()-start, setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                  setup_children_ru_maxrss=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
                  memory_scope="reference verifier/maps only; KiB Linux lifetime high-water, never daemon/phase peak")
    try:
        for path in output.iterdir():
            if path.is_file():
                if os.geteuid() == 0:
                    os.chown(path, 501, 20, follow_symlinks=False)
                path.chmod(0o644)
        if os.geteuid() == 0:
            os.chown(output, 501, 20, follow_symlinks=False)
    except Exception as error:
        result["status"] = "INCOMPLETE"
        result.setdefault("independent_output_failures", []).append(str(error))
    if result["status"] == "CLOSED_EXPECTED_SETUP_ONLY":
        ordinary_identity()
    try:
        write_new(output / "closed.json", result)
    except Exception as error:
        result["status"] = "INCOMPLETE"
        result.setdefault("independent_output_failures", []).append(str(error))
    return result


def compare_stdout(args):
    closed = read_json(args.closed)
    require(closed["status"] == "CLOSED_EXPECTED_SETUP_ONLY", "expected oracle is not closed")
    recipe = closed["host_stdout_recipe"]
    require(recipe["kind"] in {"exact-file-sha256", "unordered-complete-lines"}, "unknown original stdout recipe")
    member = Path(recipe["expected_bundle_relative"])
    require(not member.is_absolute() and ".." not in member.parts, "expected stdout escapes closed bundle")
    bundle = Path(args.bundle).resolve(strict=True)
    expected = (bundle / member).resolve(strict=True)
    require(bundle in expected.parents and expected.is_file(), "expected stdout outside closed bundle")
    require(sha(expected) == recipe["expected_sha256"], "closed expected stdout bytes differ")
    event = read_json(args.actual_event)
    fields = event.get("fields", {})
    require(event.get("event") == "command" and fields.get("exit_code") == "0" and
            fields.get("registered_execs") == "0", "original unregistered command zero-exit event required")
    actual = Path(fields["stdout"])
    require(actual.is_absolute() and not str(actual).startswith("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness"), "original host output path required")
    require(stat.S_ISREG(actual.lstat().st_mode), "original host stdout must be a regular file, not a symlink")
    resolved = actual.resolve(strict=True)
    protected = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness")
    require(resolved != protected and protected not in resolved.parents, "protected original host stdout target refused")
    if recipe["kind"] == "exact-file-sha256":
        equal = sha(expected) == sha(actual)
    else:
        def lines(path):
            with ReadFile(path, "host_stdout_lines_read") as stream:
                data = stream.read(WINDOW + 1)
            require(len(data) <= WINDOW and data.endswith(b"\n"), "bounded complete parallel stdout lines required")
            rows = data.splitlines(keepends=True)
            require(len(rows) == 2, "both original concurrent stdout lines required")
            return sorted(rows)
        equal = lines(expected) == lines(actual)
    return dict(schema="r7-original-host-stdout-oracle-v1", status="PASS" if equal else "FAIL",
                kind=recipe["kind"], original_event=str(args.actual_event), actual=str(actual),
                actual_sha256=sha(actual), expected_sha256=recipe["expected_sha256"],
                scope="original known-zero host command stdout; no container-path inference")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    spec = commands.add_parser("spec")
    spec.add_argument("--case", required=True)
    spec.add_argument("--cache-class", choices=["A", "B", "C", "NONE"], required=True)
    spec.add_argument("--inputs", type=Path, required=True)
    spec.add_argument("--inputs-sha256", required=True)
    spec.add_argument("--output", type=Path, required=True)
    run = commands.add_parser("author")
    for name in ("spec", "staging-receipt", "output"):
        run.add_argument("--" + name, type=Path, required=True)
    for name in ("spec-sha256", "staging-receipt-sha256", "container", "asset-root"):
        run.add_argument("--" + name, required=True)
    run.add_argument("--setup-wall-seconds", type=int, choices=[SETUP_SECONDS], required=True)
    observed = commands.add_parser("observe-known")
    observed.add_argument("--root", type=Path, required=True)
    observed.add_argument("--selection", choices=["small", "increments", "log", "large", "temporary"], required=True)
    observed.add_argument("--output", type=Path, required=True)
    stdout = commands.add_parser("compare-stdout")
    for name in ("closed", "bundle", "actual-event"):
        stdout.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    try:
        result = emit_spec(args) if args.command == "spec" else author(args) if args.command == "author" else compare_stdout(args) if args.command == "compare-stdout" else observe_known(args.root, args.selection, args.output)
    except Exception as error:
        result = dict(schema="r7-oracle-preparation-boundary-v1", status="INCOMPLETE", command=args.command,
                      original_failure_type=type(error).__name__, original_failure=str(error),
                      original_phase=getattr(error, "original_phase", None),
                      independent_close_failures=getattr(error, "independent_close_failures", []), retry=False)
    print(json.dumps(result, sort_keys=True))
    return 0 if result.get("status") in {"DECLARED_SETUP_ONLY", "NOT_RUN", "CLOSED_EXPECTED_SETUP_ONLY", "OBSERVED", "PASS"} else 1


if __name__ == "__main__":
    raise SystemExit(main())

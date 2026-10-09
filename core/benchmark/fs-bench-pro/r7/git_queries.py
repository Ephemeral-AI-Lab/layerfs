"""One-attempt read-only Git context queries and reviewed scoped verification.

Git executes here, never in the pure index decoder. Query evidence and verifier
maps live outside the measured root. No index normalization, config write,
retry, child cancellation, network, Store operation or performance claim.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
if __package__:
    from . import git_index_oracle as index, oracle, workloads
else:
    import git_index_oracle as index
    import oracle
    import workloads

SCHEMA = "r7-original-git-queries-v1"
COMPARISON_SCHEMA = "r7-git-scoped-comparison-v1"
WALL_SECONDS = 9.5
CASES = {"E04", "E10", "E11", "E18", "C12"}


def query_override(case):
    require(case in CASES, "only reviewed Git cases supported")
    return ["-c", "core.fsmonitor=false"] if case in {"E04", "E18"} else []


DEFAULTS = {"core.repositoryformatversion": "0", "extensions.objectformat": "sha1",
            "extensions.worktreeconfig": "false", "core.filemode": "true", "core.ignorecase": "false",
            "index.skiphash": "false", "core.splitindex": "false", "core.sparsecheckout": "false",
            "core.untrackedcache": "keep", "core.fsmonitor": "false", "feature.manyfiles": "false"}
BOOLS = {"extensions.worktreeconfig", "core.filemode", "core.ignorecase", "index.skiphash",
         "core.splitindex", "core.sparsecheckout", "core.fsmonitor", "feature.manyfiles"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def before_stop(deadline, phase):
    if time.monotonic() >= deadline:
        error = TimeoutError("original Git query/verifier wall stop: " + phase)
        error.original_phase = phase
        raise error


def evidence_path(value, root):
    path = Path(value).absolute()
    temporary = Path("/tmp").resolve(strict=True)
    require(".." not in path.parts, "evidence path parent traversal")
    relative = path.relative_to("/tmp") if path.parts[:2] == ("/", "tmp") else path.relative_to(temporary)
    require(any(part.startswith("layerfs-r7-") for part in relative.parts), "owned temporary query evidence required")
    actual = temporary
    for position, part in enumerate(relative.parts):
        actual /= part
        if not os.path.lexists(actual):
            require(position == len(relative.parts) - 1, "only final evidence directory may be absent")
            break
        require(not actual.is_symlink(), "query evidence must not traverse symlinks")
    require(actual != root and root not in actual.parents and actual not in root.parents,
            "query evidence must be outside the measured root")
    require(not os.path.lexists(actual), "exclusive query evidence already exists")
    return actual


def digest_text(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def write_comparison(path, value):
    # Expected root labels belong to a different reference container. The
    # collector has already admitted this exclusive directory outside the
    # actual root; do not resolve foreign labels as local filesystem paths.
    with oracle.FileOwner(path, "xb", "comparison_evidence_write") as stream:
        for fragment in json.JSONEncoder(sort_keys=True, ensure_ascii=True).iterencode(value):
            raw = fragment.encode()
            for start in range(0, len(raw), 65536):
                part = raw[start:start + 65536]
                require(stream.write(part) == len(part), "short original comparison evidence write; no resend")
        require(stream.write(b"\n") == 1, "short original comparison evidence newline; no resend")


def policy_shape(policy):
    require(isinstance(policy, dict) and policy.get("schema") == "r7-git-default-policy-v1" and
            policy.get("defaults") == DEFAULTS and digest_text(policy.get("git_binary_sha256")) and
            isinstance(policy.get("version"), str) and digest_text(policy.get("review_receipt_sha256")),
            "qualified exact Git/default policy required; no guessed defaults")
    sources = policy.get("primary_sources")
    require(isinstance(sources, list) and sources and all(isinstance(row, dict) and
            row.get("url", "").startswith("https://raw.githubusercontent.com/git/git/") and
            digest_text(row.get("sha256")) for row in sources), "sealed primary-source policy evidence required")


class QueryFailure(RuntimeError):
    def __init__(self, original, evidence, queries):
        super().__init__(str(original))
        self.original, self.evidence, self.queries = original, str(evidence), queries
        self.original_phase = getattr(original, "original_phase", "original_git_query")
        self.independent_close_failures = getattr(original, "independent_close_failures", [])


def query(root, evidence, label, argv, environment, deadline, queries, *, missing=False):
    before_stop(deadline, "before_query_admission")
    prefix = evidence / label
    row = dict(label=label, argv=argv, cwd=str(root), attempts=1, pid=None,
               stdout=str(prefix.with_suffix(".stdout")), stderr=str(prefix.with_suffix(".stderr")),
               status="ORIGINAL_PENDING")
    queries.append(row)
    index.write_new(prefix.with_suffix(".attempt.json"), row)
    phase = "query_output_admission"
    try:
        with oracle.FileOwner(row["stdout"], "xb", "query_stdout_owner") as stdout, \
                oracle.FileOwner(row["stderr"], "xb", "query_stderr_owner") as stderr:
            try:
                before_stop(deadline, "before_query_launch")
                phase = "original_query_launch"
                child = subprocess.Popen(argv, cwd=root, env=environment, stdin=subprocess.DEVNULL,
                                         stdout=stdout, stderr=stderr)
                row["pid"] = child.pid
                phase = "original_query_custody_output"
                index.write_new(prefix.with_suffix(".ack.json"), row)
                before_stop(deadline, "before_query_wait")
                phase = "original_query_wait"
                code = child.wait(timeout=deadline-time.monotonic())
                row.update(exit_code=code, status="ORIGINAL_COMPLETED")
            except BaseException as original:
                if not hasattr(original, "original_phase"):
                    original.original_phase = phase
                raise
        phase = "original_query_evidence"
        row.update(stdout_sha256=index.file_digest(row["stdout"]), stderr_sha256=index.file_digest(row["stderr"]))
        index.write_new(prefix.with_suffix(".completion.json"), row)
        with oracle.FileOwner(row["stdout"], "rb", "query_stdout_read") as stream:
            data = stream.read(65537)
        with oracle.FileOwner(row["stderr"], "rb", "query_stderr_read") as stream:
            errors = stream.read(65537)
        require(len(data) <= 65536 and not errors, "unqualified original query output; full original files retained")
        phase = "original_query_output_qualification"
        require(code == 0 or missing and code == 1 and not data, "original query failed; no default substitution")
        before_stop(deadline, "after_query_evidence")
        return data.decode("utf-8").strip() if code == 0 else None
    except BaseException as original:
        if not hasattr(original, "original_phase"):
            original.original_phase = phase
        row.update(original_failure_type=type(original).__name__, original_failure=str(original),
                   child_cancellation="NOT_ATTEMPTED; original PID/evidence retained")
        raise


def collect(root, policy, output, environment=None, *, deadline=None, case="E04"):
    """Collect actual Git context once; absence keeps its qualified literal value."""
    deadline = time.monotonic() + WALL_SECONDS if deadline is None else deadline
    environment = dict(workloads.ENV) if environment is None else environment
    require(environment == workloads.ENV, "canonical original environment required")
    override = query_override(case)
    policy_shape(policy)
    root = index.checked_root(root)
    require((root / ".git").is_dir() and not (root / ".git").is_symlink() and
            not os.path.lexists(root / ".git/commondir"), "private ordinary Git directory required")
    evidence = evidence_path(output, root)
    before_stop(deadline, "query_setup")
    evidence.mkdir(mode=0o700)
    queries = []
    try:
        executable = shutil.which("git", path=environment["PATH"])
        require(executable is not None, "actual Git executable missing")
        executable = str(Path(executable).resolve(strict=True))
        binary_sha = index.file_digest(executable)
        require(binary_sha == policy["git_binary_sha256"], "actual Git binary differs from qualified policy")
        def original(label, argv, missing=False):
            return query(root, evidence, label, argv, environment, deadline, queries, missing=missing)
        version = original("version", [executable, "--version"])
        require(version == policy["version"], "actual Git version differs from qualified policy")
        actual_format = original("object-format", [executable, *override, "rev-parse", "--show-object-format"])
        require(actual_format == "sha1", "actual object format unsupported")
        effective, provenance = {}, {}
        for key in [*index.EFFECTIVE_KEYS, "feature.manyfiles"]:
            argv = [executable, *override, "config", "--get"]
            if key in BOOLS:
                argv.append("--type=bool")
            argv.append(key)
            value = original(key.replace(".", "-"), argv, True)
            provenance[key] = dict(original="ABSENT" if value is None else value, exit_code=1 if value is None else 0)
            if value is None:
                value = policy["defaults"][key]
                provenance[key]["qualified_literal_default"] = value
            effective[key] = value
        require(effective.pop("feature.manyfiles") == "false", "implicit manyFiles defaults unsupported")
        require(effective["extensions.objectformat"] == actual_format, "effective/actual object format differs")
        receipt = dict(schema=SCHEMA, status="OBSERVED", case_id=case, root=str(root), queries=queries,
                       uid=os.getuid(), gid=os.getgid(),
                       actual_git_binary=executable, actual_git_binary_sha256=binary_sha, actual_git_version=version,
                       actual_object_format=actual_format, effective_config=effective, original_values=provenance,
                       qualified_policy=policy, environment=environment, command_override={"core.fsmonitor": "false"} if override else {},
                       wall_stop_seconds=WALL_SECONDS, child_cancellation="NOT_ATTEMPTED")
        receipt_path = evidence / "queries.json"
        index.write_new(receipt_path, receipt)
        worktree = root / ".git/config.worktree"
        pin = dict(schema=index.PIN_SCHEMA, object_format=actual_format, index_versions=[2, 3],
                   git=dict(binary=executable, sha256=binary_sha, version=version,
                            version_receipt_sha256=queries[0]["stdout_sha256"]),
                   config_sha256=index.file_digest(root / ".git/config"),
                   worktree_config_sha256=index.file_digest(worktree) if os.path.lexists(worktree) else None,
                   effective_config=effective, effective_config_sha256=index.sha256(index.canonical(effective)),
                   effective_config_receipt_sha256=index.file_digest(receipt_path))
        index.validate_pin(pin)
        pin_path = evidence / "pin.json"
        index.write_new(pin_path, pin)
        result = dict(schema=SCHEMA, status="OBSERVED", pin=pin, pin_path=str(pin_path),
                      pin_file_sha256=index.file_digest(pin_path), queries_path=str(receipt_path),
                      queries_sha256=index.file_digest(receipt_path), queries=queries, policy_sha256=index.sha256(index.canonical(policy)))
        before_stop(deadline, "complete_original_query_job")
        return result
    except BaseException as original:
        raise QueryFailure(original, evidence, queries) from original


def same_context(expected, actual):
    """Receipt witnesses vary; every binary/config/effective semantic value must match."""
    index.validate_pin(expected)
    index.validate_pin(actual)
    def semantic(pin):
        return {**{key: value for key, value in pin.items() if key not in {"git", "effective_config_receipt_sha256"}},
                "git": {key: value for key, value in pin["git"].items() if key != "version_receipt_sha256"}}
    require(semantic(expected) == semantic(actual), "actual queried Git/config context differs from paired comparison pin")


def tracked_paths(path, digest):
    rows = index.load_sealed(path, digest)
    require(isinstance(rows, list) and all(isinstance(row, str) and row and not Path(row).is_absolute() and
            ".." not in Path(row).parts and str(Path(row)) == row for row in rows), "sealed tracked paths are not canonical relative names")
    selected = set(rows)
    require(len(selected) == len(rows), "sealed tracked paths contain duplicates")
    return selected


def verify(args):
    start = time.monotonic()
    deadline = start + WALL_SECONDS
    require(args.case in CASES, "only reviewed Git cases supported")
    require(args.case != "E11" or getattr(args, "tracked", None) and getattr(args, "tracked_sha256", None), "E11 requires sealed tracked operand")
    tracked = tracked_paths(args.tracked, args.tracked_sha256) if args.case == "E11" else None
    require(args.case == "E11" or not getattr(args, "tracked", None) and not getattr(args, "tracked_sha256", None), "unselected tracked operand forbidden")
    root = index.checked_root(args.root)
    expected_pin = index.load_sealed(args.pin, args.pin_sha256)
    expected_index = index.load_sealed(args.expected_index, args.expected_index_sha256)
    policy = index.load_sealed(args.policy, args.policy_sha256)
    require(index.file_digest(args.expected_tree) == args.expected_tree_sha256, "expected generic tree operand changed")
    actual = collect(root, policy, args.output, deadline=deadline, case=args.case)
    same_context(expected_pin, actual["pin"])
    # Actual queries qualify this same sealed comparison context. Both decoder
    # observations use it; actual independent receipt witnesses remain intact.
    observed = index.observe(root, expected_pin)
    evidence = Path(actual["pin_path"]).parent
    actual_index, actual_tree = evidence / "actual.index.json", evidence / "actual.tree.jsonl"
    index.write_new(actual_index, observed)
    oracle.observe(root, args.case, actual_tree, tracked=tracked)
    comparison = index.compare_tree(args.expected_tree, actual_tree, args.case, expected_index, observed)
    require(index.file_digest(args.expected_tree) == args.expected_tree_sha256,
            "declared expected generic tree operand changed during comparison")
    if tracked is not None:
        require(index.file_digest(args.tracked) == args.tracked_sha256, "declared tracked input changed during comparison")
    comparison_path = evidence / "comparison.json"
    write_comparison(comparison_path, comparison)
    result = dict(schema=COMPARISON_SCHEMA, status=comparison["status"], case_id=args.case, oracle_schema=index.SCHEMA,
                differences=[] if comparison["status"] == "PASS" else ["retained full tree/index comparison differs"],
                expected_operands=dict(tree_sha256=args.expected_tree_sha256, index_sha256=args.expected_index_sha256,
                                       pin_file_sha256=args.pin_sha256, policy_file_sha256=args.policy_sha256),
                actual_operands=dict(tree_sha256=index.file_digest(actual_tree), index_sha256=index.file_digest(actual_index),
                                     query_receipt_sha256=actual["queries_sha256"], actual_query_pin_sha256=actual["pin_file_sha256"],
                                     comparison_sha256=index.file_digest(comparison_path)),
                paired_comparison_pin_sha256=index.sha256(index.canonical(expected_pin)),
                comparison_artifact=str(comparison_path), actual_evidence=str(evidence),
                scope="unchanged selected generic tree bytes/metadata plus complete semantic index; only planned cached-stat index exclusions",
                wall_stop_seconds=WALL_SECONDS, elapsed_ns=int((time.monotonic()-start)*1e9))
    before_stop(deadline, "complete_actual_git_comparison")
    if tracked is not None:
        result["expected_operands"]["tracked_sha256"] = args.tracked_sha256
    result["elapsed_ns"] = int((time.monotonic()-start)*1e9)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("collect", "verify"):
        command = commands.add_parser(name)
        for option in ("root", "policy", "output"):
            command.add_argument("--" + option, type=Path, required=True)
        command.add_argument("--policy-sha256", required=True)
        command.add_argument("--case", choices=sorted(CASES), default="E04" if name == "collect" else None, required=name == "verify")
        if name == "verify":
            command.add_argument("--tracked", type=Path)
            command.add_argument("--tracked-sha256")
            for option in ("expected-tree", "expected-index", "pin"):
                command.add_argument("--" + option, type=Path, required=True)
                command.add_argument("--" + option + "-sha256", required=True)
    args = parser.parse_args()
    try:
        require((os.getuid(), os.getgid()) == (501, 20), "ordinary original Git query/verifier identity must be501:20")
        result = collect(args.root, index.load_sealed(args.policy, args.policy_sha256), args.output, case=args.case) if args.command == "collect" else verify(args)
    except Exception as error:
        original = error.original if isinstance(error, QueryFailure) else error
        result = dict(schema=SCHEMA, status="UNAVAILABLE", original_failure_type=type(original).__name__,
                      original_failure=str(original), original_phase=getattr(error, "original_phase", None),
                      independent_close_failures=getattr(error, "independent_close_failures", []), retries=0,
                      evidence=getattr(error, "evidence", str(args.output)), queries=getattr(error, "queries", []),
                      child_cancellation="NOT_ATTEMPTED; original query PID/streams retained")
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] in {"OBSERVED", "PASS"} else 1


if __name__ == "__main__":
    raise SystemExit(main())

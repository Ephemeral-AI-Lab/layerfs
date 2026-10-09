"""Independent streaming scoped observations; never run inside product timing.

The output records all observed times. Timing-row comparisons cover names,
kinds, mode, ownership, size, symlink target, selected full payload bytes and
hard-link equivalence. They do not claim full timestamp/identity qualification.
The full-byte final functional proof has a separately declared exception.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat


class FileOwner:
    """Selected oracle I/O; closing preserves the exact first original failure."""
    def __init__(self, path, mode, phase):
        self.phase = phase
        try:
            self.stream = Path(path).open(mode, buffering=0) if "b" in mode else Path(path).open(mode)
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


def digest_file(path):
    digest = hashlib.sha256()
    with FileOwner(path, "rb", "payload_read") as stream:
        for part in iter(lambda: stream.read(65536), b""):
            digest.update(part)
    return digest.hexdigest()


def selected(case, relative):
    path = Path(relative)
    if case.startswith("C"):
        return True
    if case in {"E04", "E18"}:
        return relative == ".git" or relative.startswith(".git/")
    if case == "E10":
        return relative == ".git" or relative.startswith(".git/") or relative == "packages/acp/acp/src/codec.ts"
    if case in {"E11", "E19"}:
        # Complete names/metadata, selected bytes need the sealed tracked list.
        return True
    if case in {"E12", "E13", "E14"}:
        return "node_modules" in path.parts or relative == ".experiment-store" or relative.startswith(".experiment-store/")
    if case in {"E15", "E16"}:
        return relative == ("experiment-large" if case == "E15" else "experiment.log")
    if case == "E17":
        return relative.startswith("experiment-temp-")
    if case == "E02":
        return True
    raise ValueError("tree observer unavailable for stdout/lifecycle/stream-only case " + case)


def entries(root):
    root = Path(root)
    yield ".", root
    def walk_error(original):
        if not hasattr(original, "original_phase"):
            original.original_phase = "namespace_walk"
        raise original
    for current, directories, files in os.walk(root, followlinks=False, onerror=walk_error):
        directories.sort()
        files.sort()
        for name in sorted(directories + files):
            path = Path(current) / name
            yield path.relative_to(root).as_posix(), path


def observe(root, case, output, tracked=None):
    root, output = Path(root).resolve(), Path(output).resolve()
    if output.is_relative_to(root):
        raise ValueError("independent oracle output must be outside verification root")
    if case in {"E11", "E19"} and tracked is None:
        raise ValueError("sealed tracked-path list required for exact content scope")
    paths = files = payload = 0
    # A live mount must remain mounted until this observer exits. Reading a
    # backing directory after terminal unmount cannot prove uncommitted state.
    with FileOwner(output, "xb", "manifest_write") as stream:
        for relative, path in entries(root):
            if not selected(case, relative):
                continue
            info = path.lstat()
            kind = ("regular" if stat.S_ISREG(info.st_mode) else "directory" if stat.S_ISDIR(info.st_mode)
                    else "symlink" if stat.S_ISLNK(info.st_mode) else "unsupported")
            row = {"path": relative, "kind": kind, "mode": stat.S_IMODE(info.st_mode),
                   "uid": info.st_uid, "gid": info.st_gid, "size": info.st_size,
                   "nlink": info.st_nlink, "device": info.st_dev, "inode": info.st_ino,
                   "mtime_ns": info.st_mtime_ns, "ctime_ns": info.st_ctime_ns}
            hash_content = case != "E02" and kind == "regular"
            if case in {"E11", "E19"}:
                hash_content = hash_content and (relative in tracked or relative.startswith(".git/"))
            if hash_content:
                row["sha256"] = digest_file(path)
                files += 1
                payload += info.st_size
            if kind == "symlink":
                row["target"] = os.readlink(path)
            raw = (json.dumps(row, sort_keys=True) + "\n").encode()
            if stream.write(raw) != len(raw):
                raise ValueError("short original oracle manifest write; no resend")
            paths += 1
    return {"schema": "r7-scoped-oracle-observation-v1", "case_id": case,
            "oracle_scope": "complete-declared-root" if case.startswith("C") or case == "E02" else "scoped",
            "paths": paths, "hashed_files": files, "hashed_bytes": payload,
            "manifest_sha256": digest_file(output),
            "limitations": "scoped timing oracle; exact cross-mount inode/time fidelity requires separate functional proof; verifier-only resources"}


COMPARE_KEYS = ["kind", "mode", "uid", "gid", "nlink", "target", "sha256"]


def compare(expected, actual, case):
    """Independent comparison of retained observations; no mutation/rerun."""
    def load(path):
        rows = {}
        with FileOwner(path, "r", "manifest_read") as stream:
            for line in stream:
                row = json.loads(line)
                if row["path"] in rows:
                    raise ValueError("duplicate oracle path")
                rows[row["path"]] = row
        return rows
    want, got = load(expected), load(actual)
    differences = []
    for path in sorted(set(want) | set(got)):
        left, right = want.get(path), got.get(path)
        if left is None or right is None:
            differences.append({"path": path, "reason": "name membership"})
            continue
        keys = ["kind"] if case == "E02" else list(COMPARE_KEYS)
        # Native directory block size is not a portable filesystem size.
        if left["kind"] in {"regular", "symlink"} and case != "E02":
            keys.append("size")
        changed = [key for key in keys if left.get(key) != right.get(key)]
        if changed:
            differences.append({"path": path, "fields": changed})
    if case == "E13" or case.startswith("C"):
        def aliases(rows):
            groups = {}
            for path, row in rows.items():
                if row["kind"] == "regular":
                    groups.setdefault((row["device"], row["inode"]), []).append(path)
            return sorted(sorted(paths) for paths in groups.values())
        if aliases(want) != aliases(got):
            differences.append({"reason": "hard-link equivalence classes"})
    return {"schema": "r7-scoped-oracle-comparison-v1", "status": "PASS" if not differences else "FAIL",
            "case_id": case, "differences": differences,
            "observation_memory_scope": "independent verifier only; two selected path maps, not product memory",
            "timestamp_identity_claim": "NOT_CLAIMED"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    observe_parser = commands.add_parser("observe")
    observe_parser.add_argument("--root", type=Path, required=True)
    observe_parser.add_argument("--case", required=True)
    observe_parser.add_argument("--output", type=Path, required=True)
    observe_parser.add_argument("--tracked", type=Path)
    compare_parser = commands.add_parser("compare")
    compare_parser.add_argument("--expected", type=Path, required=True)
    compare_parser.add_argument("--actual", type=Path, required=True)
    compare_parser.add_argument("--case", required=True)
    stdout_parser = commands.add_parser("stdout")
    stdout_parser.add_argument("--expected", type=Path, required=True)
    stdout_parser.add_argument("--actual", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "observe":
            tracked = None
            if args.tracked:
                with FileOwner(args.tracked, "r", "tracked_paths_read") as stream:
                    tracked = set(json.load(stream))
            row = observe(args.root, args.case, args.output, tracked)
        elif args.command == "compare":
            row = compare(args.expected, args.actual, args.case)
        else:
            row = {"schema": "r7-stdout-oracle-v1", "status": "PASS" if digest_file(args.expected) == digest_file(args.actual) else "FAIL",
                   "expected_sha256": digest_file(args.expected), "actual_sha256": digest_file(args.actual),
                   "scope": "stdout only; original exit status checked in independent envelope"}
    except Exception as original:
        failure = dict(schema="r7-scoped-oracle-failure-v1", status="INCOMPLETE", command=args.command,
                       original_failure_type=type(original).__name__, original_failure=str(original),
                       original_phase=getattr(original, "original_phase", None),
                       independent_close_failures=getattr(original, "independent_close_failures", []), retries=0)
        try:
            print(json.dumps(failure, sort_keys=True), flush=True)
        except Exception as output_error:
            original.independent_output_failures = [*getattr(original, "independent_output_failures", []), str(output_error)]
            raise original from output_error
        raise SystemExit(1) from original
    print(json.dumps(row, sort_keys=True))
    if row.get("status") == "FAIL":
        raise SystemExit(1)


if __name__ == "__main__":
    main()

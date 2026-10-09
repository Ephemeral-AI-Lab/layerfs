"""Once-only untimed author of owned R7 code/replay/native input roots.

No process or Git invocation, network, Store operation, source mutation or retry.
The lead supplies the frozen deployment helper and the owned-copy Git tracked
list, runs this setup under a wall stop, then independently seals each root.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import stat
import time

from seal_fixture import entries, metadata, stability

WINDOW = 65536
COMMIT = "639ed015397290b3745d163aafe02ffee4aa3f84"
SOURCE = "/tmp/layerfs-r7-full-fixture-20261009"
ROWS = "/tmp/layerfs-r7-full-fixture-seal-20261009.jsonl"
ROWS_SHA = "90cde1a4647d26ae3ad4971eb91c7bde0f68cda156025520a9fbc21fbc711840"
TRACKED = "/tmp/layerfs-r7-tracked-paths-20261009.json"
TRACKED_SHA = "181cfc8309081511265659215075d6fc8731fc2bd59b8708fc7f2834cddfa07c"
REPOSITORY = Path(__file__).resolve().parents[3]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(WINDOW), b""):
            digest.update(block)
    return digest.hexdigest()


def owned(path, existing=True):
    path = Path(path).absolute()
    temporary = Path("/tmp").resolve(strict=True)
    require(".." not in path.parts, "temporary path must not contain parent traversal")
    require(path.parts[:2] == ("/", "tmp") or temporary in path.parents,
            "input/output must be under owned temporary storage")
    relative = path.relative_to("/tmp") if path.parts[:2] == ("/", "tmp") else path.relative_to(temporary)
    actual = temporary
    for index, part in enumerate(relative.parts):
        actual = actual / part
        try:
            info = actual.lstat()
        except FileNotFoundError:
            require(not existing and index == len(relative.parts) - 1, "only final output component may be absent")
            break
        require(not stat.S_ISLNK(info.st_mode), "owned temporary path must not traverse symlinks")
    require(temporary in actual.parents and any(part.startswith("layerfs-r7-") for part in actual.parts),
            "input/output must have an owned layerfs-r7- path component")
    return actual


def relative_path(value):
    require(isinstance(value, str) and value and "\x00" not in value, "invalid relative name")
    path = Path(value)
    require(not path.is_absolute() and ".." not in path.parts and path.as_posix() == value,
            "noncanonical or escaping relative name")
    return path


def apply_metadata(path, row):
    info = path.lstat()
    if (info.st_uid, info.st_gid) != (row["uid"], row["gid"]):
        os.chown(path, row["uid"], row["gid"], follow_symlinks=False)
    if stat.S_IMODE(path.lstat().st_mode) != row["mode"]:
        if row["kind"] == "symlink" and hasattr(os, "lchmod"):
            os.lchmod(path, row["mode"])
        else:
            os.chmod(path, row["mode"], follow_symlinks=False)
    os.utime(path, ns=(row["mtime_ns"], row["mtime_ns"]), follow_symlinks=False)
    require(metadata(path.lstat()) == {key: row[key] for key in ("kind", "mode", "uid", "gid", "mtime_ns")},
            "prepared supported metadata differs: " + str(path))


def generated(path, uid, gid, kind="file", mode=0o644):
    apply_metadata(path, dict(kind=kind, mode=mode, uid=uid, gid=gid, mtime_ns=0))


def write_json(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, sort_keys=True, ensure_ascii=True)
        stream.write("\n")


def load_rows(source, path, expected_sha):
    require(sha(path) == expected_sha, "original raw fixture seal hash differs")
    records = {}
    with path.open("rb") as stream:
        header = json.loads(stream.readline(WINDOW + 1))
        require(header.get("schema") == "r7-fixture-seal-rows-v1", "raw fixture seal schema differs")
        # The protected original-source string is never used as a filesystem path.
        require(owned(header["copy"]) == source and header.get("copy_head") == COMMIT,
                "raw seal does not identify the declared owned copy at the pinned HEAD")
        completed = None
        while True:
            line = stream.readline(WINDOW + 1)
            if not line:
                break
            require(len(line) <= WINDOW, "raw seal record exceeds 64 KiB setup bound")
            row = json.loads(line)
            if row.get("event") == "completed":
                require(completed is None, "duplicate raw seal footer")
                completed = row
                require(not stream.read(1), "bytes after raw seal footer")
                break
            relative = row["path"]
            relative_path(relative)
            require(relative not in records and row.get("match") is True and row.get("copy") == row.get("source"),
                    "raw seal contains duplicate or mismatched entry")
            records[relative] = row["copy"]
    require(completed is not None and completed.get("status") == "PASS", "raw fixture seal did not complete PASS")
    require(completed["counts"]["source_entries"] == completed["counts"]["copy_entries"] == len(records),
            "raw seal entry cardinality differs")
    require(records.get(".", {}).get("kind") == "directory", "raw seal root is missing")
    for name, row in records.items():
        path = source if name == "." else source / name
        # Validate every named parent as an actual directory; no symlink traversal.
        for parent in relative_path(name).parents:
            parent_name = parent.as_posix()
            require(records.get(parent_name, {}).get("kind") == "directory", "non-directory sealed parent")
        actual = path.lstat()
        require(metadata(actual) == {key: row[key] for key in ("kind", "mode", "uid", "gid", "mtime_ns")},
                "owned source metadata changed after seal: " + name)
        if row["kind"] == "file":
            require(actual.st_size == row["size"], "owned source size changed: " + name)
        elif row["kind"] == "symlink":
            require(actual.st_nlink == 1, "hard-linked symlinks are unsupported by this preparation author")
            require(os.fsencode(os.readlink(path)).hex() == row["target_hex"], "owned source link target changed")
    count = 0
    for name, _ in entries(source):
        require(name in records, "unexpected owned source entry: " + name)
        count += 1
    require(count == len(records), "owned source namespace differs from full seal")
    return records, completed


def copy_regular(source, targets, expected):
    before = source.lstat()
    fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    streams = []
    digest = hashlib.sha256()
    total = 0
    try:
        require(stat.S_ISREG(before.st_mode) and stability(before) == stability(os.fstat(fd)),
                "original regular source identity changed")
        for target in targets:
            streams.append(target.open("xb"))
        while True:
            block = os.read(fd, WINDOW)
            if not block:
                break
            digest.update(block)
            total += len(block)
            for stream in streams:
                stream.write(block)
            del block
        require(stability(before) == stability(os.fstat(fd)) == stability(source.lstat()),
                "original source changed during copy")
        if expected is not None:
            require(total == expected["size"] and digest.hexdigest() == expected["content_sha256"],
                    "copied source payload differs from full seal")
    finally:
        for stream in streams:
            stream.close()
        os.close(fd)
    return total, digest.hexdigest()


def prepare(args):
    start = time.monotonic_ns()
    source = owned(args.source)
    raw, tracked_file = owned(args.fixture_rows), owned(args.tracked_paths)
    output = owned(args.output, existing=False)
    require(not output.exists() and not output.is_relative_to(source) and not source.is_relative_to(output),
            "exclusive output must be fresh and disjoint from owned fixture")
    require(stat.S_ISDIR(source.lstat().st_mode), "source must be the actual owned fixture directory")
    require((source.stat().st_uid, source.stat().st_gid) == (args.uid, args.gid), "fixture root owner differs")
    records, footer = load_rows(source, raw, args.fixture_rows_sha256)
    require(sha(tracked_file) == args.tracked_paths_sha256, "lead tracked-list seal differs")
    tracked = json.loads(tracked_file.read_text())
    require(isinstance(tracked, list) and len(tracked) == 14104 and tracked == sorted(set(tracked)),
            "exact sorted unique full tracked list of 14104 paths required")
    kinds = {"file": 0, "symlink": 0}
    for name in tracked:
        relative_path(name)
        require(name in records and records[name]["kind"] in kinds, "tracked entry absent or not regular/symlink")
        kinds[records[name]["kind"]] += 1
    require(kinds == {"file": 14090, "symlink": 14}, "tracked kind counts differ from lead Git classification")
    helper = args.deployment_helper.resolve(strict=True)
    require(helper == REPOSITORY / "core/benchmark/fs-bench-pro/r7/deployment.py", "use the frozen first-party deployment helper")
    require(sha(helper) == args.deployment_helper_sha256, "frozen deployment helper identity differs")
    helpers = {name: REPOSITORY / "core/benchmark/fs-bench-pro/r7" / name
               for name in ("workload.py", "oracle.py", "changes.py")}
    helpers.update({name: REPOSITORY / "core/benchmark/r7-cache" / name
                    for name in ("residency.py", "stream_manifest.py", "generate_manifest.py")})
    helpers["r7_deployment.py"] = helper
    if args.passthrough_binary is not None:
        binary = args.passthrough_binary.resolve(strict=True)
        require(binary.is_relative_to(REPOSITORY / "core/target"), "P binary must be the lead's built first-party artifact")
        require(sha(binary) == args.passthrough_sha256, "P binary seal differs")
        helpers["r7-passthrough"] = binary
    helper_rows = {}
    for name, path in helpers.items():
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode), "static helper must be an actual regular file")
        helper_rows[name] = dict(size=info.st_size, content_sha256=sha(path))
    require(helper_rows["r7_deployment.py"]["content_sha256"] == args.deployment_helper_sha256,
            "deployment helper changed before exclusive preparation")
    if args.passthrough_binary is not None:
        require(helper_rows["r7-passthrough"]["content_sha256"] == args.passthrough_sha256,
                "P binary changed before exclusive preparation")
    output.mkdir()
    args.created_output = output
    code, replay, minus = (output / name for name in ("code", "replay", "full-minus-dependencies"))
    for root in (code, replay, minus):
        root.mkdir()
    replay_root = replay / "F"
    replay_root.mkdir()
    node_roots, ancestors = set(), {"."}
    for name in records:
        parts = Path(name).parts
        if "node_modules" in parts:
            root = Path(*parts[:parts.index("node_modules") + 1])
            node_roots.add(root.as_posix())
            ancestors.update(parent.as_posix() for parent in root.parents)
    aliases = {replay_root: {}, minus: {}}
    directories, master = [], {}
    copied_bytes = payload_read = selected_entries = excluded_entries = linked_aliases = 0
    for name, expected in records.items():
        dependency = "node_modules" in Path(name).parts
        selected = dependency or name in ancestors
        selected_entries += selected
        excluded_entries += dependency
        src = source if name == "." else source / name
        info = src.lstat()
        master[name] = {**expected, "device": info.st_dev, "inode": info.st_ino, "nlink": info.st_nlink}
        if expected["kind"] == "symlink":
            master[name]["target"] = os.fsdecode(bytes.fromhex(expected["target_hex"]))
        targets = [(root, root if name == "." else root / name) for root, include in
                   ((replay_root, selected), (minus, not dependency)) if include]
        if expected["kind"] == "directory":
            for _, dst in targets:
                if name != ".":
                    dst.mkdir()
                directories.append((dst, expected))
        elif expected["kind"] == "symlink":
            for _, dst in targets:
                dst.symlink_to(master[name]["target"])
                apply_metadata(dst, expected)
        else:
            require(expected["kind"] == "file", "unsupported sealed kind")
            identity = (info.st_dev, info.st_ino)
            writes = []
            for root, dst in targets:
                if info.st_nlink > 1 and identity in aliases[root]:
                    os.link(aliases[root][identity], dst)
                    linked_aliases += 1
                else:
                    writes.append(dst)
                    if info.st_nlink > 1:
                        aliases[root][identity] = dst
            read, _ = copy_regular(src, writes, expected)
            payload_read += read
            copied_bytes += read * len(writes)
            for _, dst in targets:
                apply_metadata(dst, expected)
    for path, expected in reversed(directories):
        apply_metadata(path, expected)
    assets = {}
    for name, src in helpers.items():
        _, digest = copy_regular(src, [code / name], helper_rows[name])
        generated(code / name, args.uid, args.gid, mode=0o755 if name == "r7-passthrough" else 0o644)
        assets[name] = dict(source=str(src), sha256=digest)
    write_json(replay / "master.json", master)
    write_json(code / "node-roots.json", sorted(node_roots))
    write_json(code / "tracked-paths.json", tracked)
    largest = min((name for name, row in records.items() if row["kind"] == "file"),
                  key=lambda name: (-records[name]["size"], name))
    require(not any(character in largest for character in "\r\n\x00"), "largest path cannot be expressed by registered shell command")
    with (code / "largest-path").open("x") as stream:
        stream.write(largest + "\n")
    for path in (replay / "master.json", code / "node-roots.json", code / "tracked-paths.json", code / "largest-path"):
        generated(path, args.uid, args.gid)
    roots = {"code": str(code), "replay": str(replay), "full-minus-dependencies": str(minus)}
    for name in ("empty", "big"):
        if not getattr(args, name):
            continue
        root = output / name
        root.mkdir()
        if name == "big":
            with (root / "big").open("xb") as stream:
                block = bytes(WINDOW)
                for _ in range(64 * 1024 * 1024 // WINDOW):
                    stream.write(block)
            generated(root / "big", args.uid, args.gid)
        generated(root, args.uid, args.gid, "directory", 0o755)
        roots[name] = str(root)
    for root in (code, replay, output):
        generated(root, args.uid, args.gid, "directory", 0o755)
    require(sha(raw) == args.fixture_rows_sha256 and sha(tracked_file) == args.tracked_paths_sha256,
            "input evidence changed during once-only preparation")
    receipt = dict(schema="r7-prepared-inputs-v1", status="PREPARED_SETUP_ONLY", roots=roots,
                   fixture_source=str(source), fixture_head=COMMIT, fixture_rows=str(raw),
                   fixture_rows_sha256=args.fixture_rows_sha256, full_fixture_counts=footer["counts"],
                   tracked_list=dict(source=str(tracked_file), sha256=args.tracked_paths_sha256,
                                     paths=14104, regular_files=14090, symlinks=14,
                                     provenance="lead Git on owned copy; author validates sealed names/kinds, performs no Git"),
                   replay_cut="all exact node_modules-component entries plus ancestor directories; full master metadata retained; selected workload unchanged",
                   replay_entries=selected_entries, full_minus_excluded_entries=excluded_entries,
                   node_roots=sorted(node_roots), largest=dict(path=largest, **records[largest]), helpers=assets,
                   fixture_payload_bytes_read=payload_read, fixture_regular_bytes_written=copied_bytes,
                   copied_internal_hardlink_aliases=linked_aliases, read_window_bytes=WINDOW,
                   setup_metadata_rows_retained=len(records), setup_master_rows_retained=len(master),
                   setup_directory_restore_rows_retained=len(directories), setup_tracked_paths_retained=len(tracked),
                   setup_internal_alias_groups_retained=sum(len(groups) for groups in aliases.values()),
                   generated_big_bytes=64 * 1024 * 1024 if args.big else 0,
                   setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                   setup_ru_maxrss_units="bytes macOS; KiB Linux; process lifetime high-water",
                   setup_ns=time.monotonic_ns()-start, admission_eligible=False, source_written=False,
                   resource_scope="untimed input author only; resident maps/sorts/copy files are outside product and measured phases",
                   next_step="lead separately seal-tree code/replay/full-minus/empty/big and author deployment plan",
                   source_quiescence_required=True, native_snapshot=False,
                   supported_metadata=["mode", "uid", "gid", "mtime_ns", "symlink target", "internal hardlink equivalence"],
                   timestamp_identity_claim="prepared host supported metadata only; container copy independently verified",
                   construction_workers=1, store_execution="NONE", fsync_calls=0, git_invocations=0,
                   durable="NOT_RUN — disabled by owner until explicit reauthorization")
    write_json(output / "preparation-receipt.json", receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path(SOURCE))
    parser.add_argument("--fixture-rows", type=Path, default=Path(ROWS))
    parser.add_argument("--fixture-rows-sha256", default=ROWS_SHA)
    parser.add_argument("--tracked-paths", type=Path, default=Path(TRACKED))
    parser.add_argument("--tracked-paths-sha256", default=TRACKED_SHA)
    parser.add_argument("--deployment-helper", type=Path, default=REPOSITORY / "core/benchmark/fs-bench-pro/r7/deployment.py")
    parser.add_argument("--deployment-helper-sha256", required=True)
    parser.add_argument("--passthrough-binary", type=Path)
    parser.add_argument("--passthrough-sha256")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--uid", type=int, default=501)
    parser.add_argument("--gid", type=int, default=20)
    parser.add_argument("--empty", action="store_true")
    parser.add_argument("--big", action="store_true")
    args = parser.parse_args()
    require(args.uid == 501 and args.gid == 20, "this preparation preserves declared fixture owner 501:20")
    require((args.passthrough_binary is None) == (args.passthrough_sha256 is None), "P binary/path seal must be supplied together")
    try:
        result = prepare(args)
    except Exception as error:
        result = dict(schema="r7-prepared-inputs-v1", status="INCOMPLETE", output=str(args.output),
                      original_failure_type=type(error).__name__, original_failure=str(error),
                      retained_partial_outputs=True, source_written=False, git_invocations=0,
                      admission_eligible=False, resource_scope="untimed setup; no retry or removal")
        if getattr(args, "created_output", None) is not None:
            try:
                write_json(args.created_output / "preparation-failure.json", result)
            except OSError as output_error:
                result["independent_receipt_output_failure"] = str(output_error)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PREPARED_SETUP_ONLY" else 1


if __name__ == "__main__":
    raise SystemExit(main())

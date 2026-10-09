"""Prospective scoped Git-index oracle; never changes an index or invokes Git.

SHA-1 index versions 2/3 and the TREE extension are supported. All other forms
refuse. Stat caches remain observations; semantic entries and TREE meaning are
compared. The caller supplies sealed Git/effective-config evidence. This helper
checks the actual binary/config bytes, not the truth of a supplied version run.
Verifier maps and output belong outside product/performance resource domains.
Grammar: https://git-scm.com/docs/gitformat-index (v2/v3 SHA-1 and TREE).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat

SCHEMA = "r7-git-index-scoped-v2"
PIN_SCHEMA = "r7-git-index-pin-v1"
WINDOW = 65536
STAT_FIELDS = ("ctime_seconds", "ctime_nanoseconds", "mtime_seconds", "mtime_nanoseconds",
               "device", "inode", "uid", "gid", "file_size")
PROTECTED = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness")
EFFECTIVE_KEYS = ("core.repositoryformatversion", "extensions.objectformat", "extensions.worktreeconfig",
                  "core.filemode", "core.ignorecase", "index.skiphash", "core.splitindex",
                  "core.sparsecheckout", "core.untrackedcache", "core.fsmonitor")


class Refusal(ValueError):
    """Original malformed, unsupported or unpinned input; no PASS or fallback."""


def require(condition, reason):
    if not condition:
        raise Refusal(reason)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def sha256(value):
    return hashlib.sha256(value).hexdigest()


def stability(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


def checked_root(value):
    root = Path(value).absolute()
    before = root.lstat()
    require(stat.S_ISDIR(before.st_mode), "verification root is not a directory or is a symlink")
    resolved = root.resolve(strict=True)
    require(resolved != PROTECTED and PROTECTED not in resolved.parents, "protected original checkout")
    require(stability(before) == stability(resolved.lstat()) == stability(root.lstat()),
            "caller verification root changed during admission")
    return root


def file_digest(path, *, directory=None):
    path = Path(path)
    if directory is None:
        resolved = path.resolve(strict=True)
        require(resolved != PROTECTED and PROTECTED not in resolved.parents, "protected original file")
    before = os.stat(path, dir_fd=directory, follow_symlinks=False)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    original = None
    try:
        require(stat.S_ISREG(before.st_mode) and stability(before) == stability(os.fstat(fd)),
                "sealed file identity changed before read")
        value = hashlib.sha256()
        total = 0
        while True:
            block = os.read(fd, WINDOW)
            if not block:
                break
            value.update(block)
            total += len(block)
        require(total == before.st_size and stability(before) == stability(os.fstat(fd))
                == stability(os.stat(path, dir_fd=directory, follow_symlinks=False)),
                "sealed file changed during read")
        return value.hexdigest()
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            os.close(fd)
        except OSError as error:
            if original is None:
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(error)]


def validate_pin(pin):
    require(pin.get("schema") == PIN_SCHEMA, "index pin schema")
    require(pin.get("object_format") == "sha1", "unsupported/unpinned object format; SHA-256 not implemented")
    versions = pin.get("index_versions")
    require(isinstance(versions, list) and versions and len(set(versions)) == len(versions)
            and all(type(value) is int and value in (2, 3) for value in versions), "unsupported/unpinned index version")
    effective = pin.get("effective_config")
    require(isinstance(effective, dict) and all(key in effective for key in EFFECTIVE_KEYS),
            "complete selected effective-config observations required")
    require(sha256(canonical(effective)) == pin.get("effective_config_sha256"), "effective config seal differs")
    require(effective["core.repositoryformatversion"] in ("0", "1") and effective["extensions.objectformat"] == "sha1",
            "unsupported repository/object format")
    require(all(effective[key] == "false" for key in ("index.skiphash", "core.splitindex", "core.sparsecheckout", "core.fsmonitor")),
            "unsupported skipped checksum, split/sparse index or filesystem cache configuration")
    # Git preserves an existing untracked cache when the option is absent.
    # An actual UNTR extension still refuses in parse_index; keep is not false.
    require(effective["core.untrackedcache"] in ("false", "keep"), "untracked cache creation unsupported")
    require(all(effective[key] in ("true", "false") for key in
                ("extensions.worktreeconfig", "core.filemode", "core.ignorecase")), "unqualified effective boolean")
    git = pin.get("git", {})
    require(all(isinstance(git.get(key), str) and git[key] for key in
                ("binary", "sha256", "version", "version_receipt_sha256")), "sealed Git binary/version receipt required")
    require(isinstance(pin.get("config_sha256"), str) and "worktree_config_sha256" in pin,
            "actual config and explicit worktree-config presence/hash required")
    require(isinstance(pin.get("effective_config_receipt_sha256"), str) and pin["effective_config_receipt_sha256"],
            "original effective-config receipt seal required")
    return pin


class Reader:
    def __init__(self, stream, size):
        require(type(size) is int and size >= 32, "index smaller than header/checksum")
        self.stream, self.size, self.position = stream, size, 0
        self.raw, self.checksum = hashlib.sha256(), hashlib.sha1()

    def read(self, count, *, trailer=False):
        limit = self.size if trailer else self.size - 20
        require(count >= 0 and count <= limit - self.position, "index field exceeds structural bounds")
        value = self.stream.read(count)
        require(len(value) == count, "short original index read")
        self.position += count
        self.raw.update(value)
        if not trailer:
            self.checksum.update(value)
        return value

    def terminated(self, marker):
        value = bytearray()
        while True:
            byte = self.read(1)
            if byte == marker:
                return bytes(value)
            value.extend(byte)

    def number(self, count):
        return int.from_bytes(self.read(count), "big")


def path_valid(path):
    return bool(path) and not path.startswith(b"/") and all(
        part not in (b"", b".", b"..", b".git") for part in path.split(b"/"))


def tree_expectations(entries):
    """Construct canonical Git trees once per directory, O(entries * path depth)."""
    directories, counts, conflicts = {b"": {}}, {b"": 0}, set()
    for entry in entries:
        path = bytes.fromhex(entry["path_hex"])
        parts = path.split(b"/")
        parent = b""
        chain = [parent]
        for part in parts[:-1]:
            child = parent + (b"/" if parent else b"") + part
            old = directories[parent].setdefault(part, (0o40000, child))
            require(old == (0o40000, child), "index file/directory path collision")
            directories.setdefault(child, {})
            counts.setdefault(child, 0)
            parent = child
            chain.append(parent)
        for directory in chain:
            counts[directory] += 1
            if entry["stage"] or entry.get("intent_to_add"):
                conflicts.add(directory)
        if not entry["stage"]:
            require(parts[-1] not in directories[parent], "index duplicate/colliding stage-zero path")
            directories[parent][parts[-1]] = (entry["mode"], bytes.fromhex(entry["object_id"]))
    hashes = {}
    for path in sorted(directories, key=lambda value: value.count(b"/") + bool(value), reverse=True):
        if path in conflicts:
            continue
        rows = directories[path]
        body = bytearray()
        for name, (mode, value) in sorted(rows.items(), key=lambda item: item[0] + (b"/" if item[1][0] == 0o40000 else b"")):
            oid = hashes[value] if mode == 0o40000 else value
            body.extend(f"{mode:o} ".encode() + name + b"\0" + oid)
        hashes[path] = hashlib.sha1(b"tree " + str(len(body)).encode() + b"\0" + body).digest()
    children = {path: {name for name, (mode, _) in rows.items() if mode == 0o40000}
                for path, rows in directories.items()}
    return counts, hashes, children


def parse_tree(payload, entries):
    """Validate TREE grammar, hierarchy, entry counts and canonical object IDs."""
    cursor, nodes, stack, frames = 0, [], [], {}
    counts, hashes, children_by_path = tree_expectations(entries)
    while cursor < len(payload):
        end = payload.find(b"\0", cursor)
        require(end >= 0, "TREE missing path terminator")
        name, cursor = payload[cursor:end], end + 1
        end = payload.find(b"\n", cursor)
        require(end >= 0, "TREE missing numeric terminator")
        pieces = payload[cursor:end].split(b" ")
        require(len(pieces) == 2 and (pieces[0] == b"-1" or pieces[0].isdigit()) and pieces[1].isdigit(),
                "TREE malformed counts")
        count, children = map(int, pieces)
        cursor = end + 1
        while stack and stack[-1]["remaining"] == 0:
            stack.pop()
        if not nodes:
            require(name == b"", "TREE root name must be empty")
            path = b""
        else:
            require(stack and path_valid(name) and b"/" not in name, "TREE invalid hierarchy/name")
            parent = stack[-1]
            require(name not in parent["names"], "TREE duplicate sibling")
            parent["names"].add(name)
            parent["remaining"] -= 1
            path = parent["path"] + (b"/" if parent["path"] else b"") + name
        require(path in counts, "TREE node absent from index namespace")
        oid = None
        if count >= 0:
            require(cursor + 20 <= len(payload), "TREE truncated object ID")
            oid = payload[cursor:cursor + 20]
            cursor += 20
            require(count == counts[path] and hashes.get(path) == oid, "TREE count/object ID differs from index entries")
        nodes.append(dict(path_hex=path.hex(), entry_count=count, subtrees=children,
                          object_id=oid.hex() if oid is not None else None))
        frame = dict(path=path, remaining=children, names=set())
        frames[path] = frame
        stack.append(frame)
    require(nodes and all(frame["remaining"] == 0 for frame in stack), "TREE incomplete child hierarchy")
    for node in nodes:
        if node["entry_count"] >= 0:
            path = bytes.fromhex(node["path_hex"])
            require(frames[path]["names"] == children_by_path[path], "TREE valid node lacks complete directory children")
    return nodes


def parse_index(stream, size, *, versions=(2, 3), object_format="sha1"):
    require(object_format == "sha1", "unsupported object format")
    reader = Reader(stream, size)
    require(reader.read(4) == b"DIRC", "index signature")
    version, count = reader.number(4), reader.number(4)
    require(version in versions and version in (2, 3), "unsupported/unpinned index version")
    require(count <= (size - 32) // 64, "entry count cannot fit index")
    entries, caches, previous = [], [], None
    for _ in range(count):
        start = reader.position
        words = [reader.number(4) for _ in range(10)]
        require(words[1] < 1_000_000_000 and words[3] < 1_000_000_000, "index invalid nanosecond fraction")
        mode = words[6]
        require(mode in (0o100644, 0o100755, 0o120000, 0o160000), "unsupported/invalid staged object mode")
        oid, flags = reader.read(20).hex(), reader.number(2)
        extended = bool(flags & 0x4000)
        require(version >= 3 or not extended, "extended flags in version 2")
        extra = reader.number(2) if extended else 0
        require(extra & ~0x6000 == 0, "unknown/reserved persisted extended flags")
        name = reader.terminated(b"\0")
        require(path_valid(name), "index invalid raw pathname")
        require(flags & 0xFFF == min(len(name), 0xFFF), "index pathname-length flag differs")
        padding = (-(reader.position - start)) % 8
        require(reader.read(padding) == bytes(padding), "index nonzero entry padding")
        stage = (flags >> 12) & 3
        key = name, stage
        require(previous is None or previous < key, "index duplicate/unordered path-stage key")
        require(previous is None or previous[0] != name or previous[1] != 0, "mixed stage-zero/conflict entries")
        previous = key
        entries.append(dict(path_hex=name.hex(), mode=mode, object_id=oid, stage=stage,
                            assume_valid=bool(flags & 0x8000), extended=extended,
                            skip_worktree=bool(extra & 0x4000), intent_to_add=bool(extra & 0x2000)))
        caches.append(dict(path_hex=name.hex(), stage=stage, **dict(zip(STAT_FIELDS, words[:6] + words[7:]))))
    extensions = []
    while reader.position < size - 20:
        signature, length = reader.read(4), reader.number(4)
        require(signature == b"TREE" and not extensions, "unsupported/duplicate index extension")
        require(length <= size - 20 - reader.position, "index extension exceeds bounds")
        payload = reader.read(length)
        extensions.append(dict(signature="TREE", payload_sha256=sha256(payload), nodes=parse_tree(payload, entries)))
    trailer = reader.read(20, trailer=True)
    require(trailer != bytes(20), "zero/absent index checksum")
    require(trailer == reader.checksum.digest(), "index checksum mismatch")
    require(reader.position == size and stream.read(1) == b"", "index trailing bytes")
    return dict(version=version, object_format="sha1", entry_count=count, entries=entries,
                extensions=extensions, stat_cache=caches, raw_index_sha256=reader.raw.hexdigest(),
                raw_index_bytes=size, checksum=dict(algorithm="sha1", value=trailer.hex(), verified=True))


def observe(root, pin):
    validate_pin(pin)
    root = checked_root(root)
    require(file_digest(pin["git"]["binary"]) == pin["git"]["sha256"], "actual Git binary seal differs")
    root_before = root.lstat()
    root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    git_fd, fd, stream, original = None, None, None, None
    try:
        require(stability(root_before) == stability(os.fstat(root_fd)), "verification root changed before open")
        git_fd = os.open(".git", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=root_fd)
        git_before = os.fstat(git_fd)
        def present(name):
            try:
                os.stat(name, dir_fd=git_fd, follow_symlinks=False)
                return True
            except FileNotFoundError:
                return False
        require(not present("commondir"), "external common Git directory unsupported")
        def configs():
            require(file_digest("config", directory=git_fd) == pin["config_sha256"], "actual repository config seal differs")
            worktree = file_digest("config.worktree", directory=git_fd) if present("config.worktree") else None
            require(worktree == pin["worktree_config_sha256"], "actual worktree config presence/seal differs")
        configs()
        before = os.stat("index", dir_fd=git_fd, follow_symlinks=False)
        fd = os.open("index", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=git_fd)
        require(stat.S_ISREG(before.st_mode) and stability(before) == stability(os.fstat(fd)), "index opened identity differs")
        stream = os.fdopen(fd, "rb", buffering=WINDOW)
        fd = None
        parsed = parse_index(stream, before.st_size, versions=pin["index_versions"], object_format=pin["object_format"])
        require(stability(before) == stability(os.fstat(stream.fileno()))
                == stability(os.stat("index", dir_fd=git_fd, follow_symlinks=False)), "index changed during observation")
        configs()
        require(stability(git_before) == stability(os.fstat(git_fd))
                == stability(os.stat(".git", dir_fd=root_fd, follow_symlinks=False)), "Git directory changed during observation")
        require(stability(root_before) == stability(os.fstat(root_fd)) == stability(root.lstat()), "verification root changed during observation")
    except BaseException as error:
        original = error
        raise
    finally:
        closing = []
        for value, is_stream in ((stream, True), (fd, False), (git_fd, False), (root_fd, False)):
            if value is None:
                continue
            try:
                if is_stream:
                    value.close()
                else:
                    os.close(value)
            except OSError as error:
                closing.append(error)
        if closing:
            if original is None:
                closing[0].independent_close_failures = [str(error) for error in closing[1:]]
                raise closing[0]
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []),
                                                 *(str(error) for error in closing)]
    return dict(schema=SCHEMA, status="OBSERVED", root=str(root), pin_sha256=sha256(canonical(pin)), pin=pin,
                semantic={key: parsed[key] for key in ("version", "object_format", "entry_count", "entries", "extensions")},
                stat_cache=parsed["stat_cache"], excluded_cross_filesystem_fields=list(STAT_FIELDS),
                raw_index_sha256=parsed["raw_index_sha256"], raw_index_bytes=parsed["raw_index_bytes"], checksum=parsed["checksum"],
                filesystem_metadata=dict(kind="regular", mode=stat.S_IMODE(before.st_mode), uid=before.st_uid,
                                         gid=before.st_gid, size=before.st_size, nlink=before.st_nlink,
                                         device=before.st_dev, inode=before.st_ino, mtime_ns=before.st_mtime_ns, ctime_ns=before.st_ctime_ns),
                provenance="binary/config bytes checked; Git version/effective values supplied by caller's sealed original receipts; no Git invoked",
                resource_scope="independent verifier only; entry/stat/TREE maps outside product and performance",
                limitations="scoped semantic index comparison; raw native/L byte equality and stat-cache correctness NOT_CLAIMED")


def compare(expected, actual):
    differences = []
    for row in (expected, actual):
        require(row.get("schema") == SCHEMA and row.get("status") == "OBSERVED", "qualified index observation missing")
        require(row.get("checksum", {}).get("verified") is True, "index checksum proof missing")
        require(row.get("excluded_cross_filesystem_fields") == list(STAT_FIELDS), "index exclusion contract differs")
        validate_pin(row["pin"])
        require(sha256(canonical(row["pin"])) == row["pin_sha256"], "index observation pin differs")
    for key in ("pin_sha256", "semantic"):
        if expected[key] != actual[key]:
            differences.append(key)
    for key in ("kind", "mode", "uid", "gid", "size", "nlink"):
        if expected["filesystem_metadata"][key] != actual["filesystem_metadata"][key]:
            differences.append("filesystem_metadata." + key)
    return dict(schema=SCHEMA, status="FAIL" if differences else "PASS", differences=differences,
                verification_roots=[expected["root"], actual["root"]],
                expected_raw_sha256=expected["raw_index_sha256"], actual_raw_sha256=actual["raw_index_sha256"],
                excluded_cross_filesystem_fields=list(STAT_FIELDS),
                scope="complete semantic entries/TREE and actual index file metadata; stat-cache correctness/raw cross-arm index bytes NOT_CLAIMED")


def compare_tree(expected_tree, actual_tree, case, expected_index, actual_index):
    """Reuse the unchanged tree oracle; only index payload SHA gets this scope."""
    if __package__:
        from . import oracle
    else:
        import oracle
    seals = [file_digest(path) for path in (expected_tree, actual_tree)]
    def bound(path, observed):
        selected = []
        with Path(path).open() as stream:
            for line in stream:
                row = json.loads(line)
                if row.get("path") == ".git/index":
                    selected.append(row)
        if len(selected) != 1:
            return False
        row, metadata = selected[0], observed["filesystem_metadata"]
        return row.get("sha256") == observed["raw_index_sha256"] and all(
            row.get(key) == metadata[key] for key in ("kind", "mode", "uid", "gid", "size", "nlink"))
    bindings = all(bound(path, observed) for path, observed in
                   ((expected_tree, expected_index), (actual_tree, actual_index)))
    tree = oracle.compare(expected_tree, actual_tree, case)
    index = compare(expected_index, actual_index)
    kept = []
    for difference in tree["differences"]:
        if bindings and difference.get("path") == ".git/index" and "fields" in difference:
            fields = [name for name in difference["fields"] if name != "sha256"]
            if fields:
                kept.append({**difference, "fields": fields})
        else:
            kept.append(difference)
    require(seals == [file_digest(path) for path in (expected_tree, actual_tree)], "tree inputs changed during index-aware comparison")
    if not bindings:
        kept.append(dict(path=".git/index", reason="raw hash/metadata do not bind each tree row to its actual index observation"))
    tree = {**tree, "status": "FAIL" if kept else "PASS", "differences": kept}
    return dict(schema=SCHEMA, status="PASS" if tree["status"] == index["status"] == "PASS" else "FAIL",
                tree=tree, index=index, verification_roots=index["verification_roots"],
                scope="new explicit index payload scope; all other unchanged tree checks retained")


def write_new(path, value):
    path = Path(path)
    actual = path.parent.resolve(strict=True) / path.name
    require(actual != PROTECTED and PROTECTED not in actual.parents, "oracle output is in protected original checkout")
    roots = value.get("verification_roots", [])
    if value.get("status") == "OBSERVED":
        roots = [value["root"]]
    for root in roots:
        selected = Path(root).resolve(strict=True)
        require(actual != selected and selected not in actual.parents, "oracle output must be outside verification root")
    stream = path.open("xb", buffering=0)
    original = None
    try:
        for fragment in json.JSONEncoder(sort_keys=True, ensure_ascii=True).iterencode(value):
            raw = fragment.encode()
            for start in range(0, len(raw), WINDOW):
                part = raw[start:start + WINDOW]
                require(stream.write(part) == len(part), "short original oracle output write; no resend")
        require(stream.write(b"\n") == 1, "short original oracle output newline; no resend")
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            stream.close()
        except OSError as error:
            if original is None:
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(error)]


def load_sealed(path, digest):
    path = Path(path)
    resolved = path.resolve(strict=True)
    require(resolved != PROTECTED and PROTECTED not in resolved.parents, "protected original oracle input")
    before = path.lstat()
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    original = None
    try:
        require(stat.S_ISREG(before.st_mode) and stability(before) == stability(os.fstat(fd)), "oracle input opened identity differs")
        raw = bytearray()
        value = hashlib.sha256()
        while True:
            block = os.read(fd, WINDOW)
            if not block:
                break
            raw.extend(block)
            value.update(block)
        require(value.hexdigest() == digest, "oracle input SHA-256 differs")
        require(len(raw) == before.st_size and stability(before) == stability(os.fstat(fd)) == stability(path.lstat()),
                "oracle input changed during decode")
        # The whole sealed JSON artifact is verifier-only decoding storage.
        # The digest above covers the very bytes decoded, not a later reread.
        return json.loads(raw)
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            os.close(fd)
        except OSError as error:
            if original is None:
                raise
            original.independent_close_failures = [*getattr(original, "independent_close_failures", []), str(error)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    observer = commands.add_parser("observe")
    observer.add_argument("--root", required=True)
    observer.add_argument("--pin", required=True)
    observer.add_argument("--pin-sha256", required=True)
    observer.add_argument("--output", required=True)
    comparison = commands.add_parser("compare")
    comparison.add_argument("--expected", required=True)
    comparison.add_argument("--expected-sha256", required=True)
    comparison.add_argument("--actual", required=True)
    comparison.add_argument("--actual-sha256", required=True)
    comparison.add_argument("--output", required=True)
    args = parser.parse_args()
    phase = "sealed-input-decode"
    try:
        if args.command == "observe":
            pin = load_sealed(args.pin, args.pin_sha256)
            phase = "original-index-observation"
            result = observe(args.root, pin)
        else:
            expected = load_sealed(args.expected, args.expected_sha256)
            actual = load_sealed(args.actual, args.actual_sha256)
            phase = "expected-actual-comparison"
            result = compare(expected, actual)
        phase = "exclusive-oracle-output"
        write_new(args.output, result)
        print(json.dumps(dict(schema=SCHEMA, status=result["status"], output=args.output,
                              output_sha256=file_digest(args.output), resource_scope="independent verifier only"), sort_keys=True))
        return 1 if result["status"] == "FAIL" else 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps(dict(schema=SCHEMA, status="UNAVAILABLE", original_failure_type=type(error).__name__,
                              original_failure=str(error), phase=phase,
                              independent_close_failures=getattr(error, "independent_close_failures", []),
                              output=args.output, output_present=os.path.lexists(args.output), retries=0), sort_keys=True))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

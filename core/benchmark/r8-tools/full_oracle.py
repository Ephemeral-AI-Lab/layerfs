#!/usr/bin/env python3
"""Independent read-only full-byte proof of an installed LayerFS namespace.

Expected input is an externally sealed r7-deployment-tree-v1 JSONL inventory;
its SHA-256 is checked before the first mount observation. Expected rows and
alias classes are indexed in external SQLite scratch, never obtained from the
product's output. Every regular name is read to EOF in 65536-byte windows.

Observation and comparison are separate. A fixed number of walker threads
observe names, metadata, complete payload digests and symlink targets with one
unchanged system-call sequence per name; one thread compares every observed
row with the sealed expectation and owns the scratch index. The serial
arrangement of receipt 053 could not finish inside its registered stop (see
checks/r8b-requalification-20261010/006-full-oracle-timeout-diagnosis.md);
every assertion of that arrangement is retained here.

Portable projections, reviewed at 9e4de35fc:
* Project import/scan.rs:53-64 preserves supported regular/directory modes and
  mtime; symlink mode is 0777. Source uid/gid are not portable attributes.
* Fuse attributes.rs:38-79 projects configured uid/gid and ctime=mtime.
* Fuse attributes.rs:46-60 and architecture/77-native-mutation-coherence.md's
  R7 update (2026-10-09, 82c51a439) project directory nlink as 2 plus immediate
  child directories. That explicit update supersedes historical constant-2
  wording. Regular nlink is the number of stored names sharing one source
  device/inode, not the source inode number or the observed product grouping.

The caller owns mount, registration, wall stop, terminal unmount and custody.
This command makes no timing/cache/resource admission claim and never repairs
metadata. Output must be a fresh JSON file outside the observed namespace;
its sibling .artifacts directory retains expected/observed rows and scratch.
Timestamp nanoseconds and native inode identities use decimal TEXT in the
index so SQLite's signed-64-bit integer does not narrow their source formats.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import queue
import sqlite3
import stat
import sys
import threading

WINDOW = 65536
WALKERS = 8
ROWS = 1024
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
SCHEMA = "r8-full-mounted-oracle-v2"
INPUT_SCHEMA = "r7-deployment-tree-v1"
PROJECTION = "portable-init-fuse-r7-directory-links-v1"


class ProofError(ValueError):
    """An exact failed input or namespace assertion; no attempted repair."""


def require(condition, reason):
    if not condition:
        raise ProofError(reason)


@contextmanager
def closing(value, close):
    original = None
    try:
        yield value
    except BaseException as error:
        original = error
        raise
    finally:
        try:
            close(value)
        except BaseException as error:
            if original is None:
                raise
            original.independent_close_failures = [
                *getattr(original, "independent_close_failures", []), str(error)]


def sha256(path):
    digest = hashlib.sha256()
    with closing(Path(path).open("rb"), lambda stream: stream.close()) as stream:
        for block in iter(lambda: stream.read(WINDOW), b""):
            digest.update(block)
    return digest.hexdigest()


def digest_value(value, label):
    require(isinstance(value, str) and len(value) == 64 and
            all(char in "0123456789abcdef" for char in value), label + " is not SHA-256")
    return value


def integer(value, label, minimum=None):
    require(type(value) is int and (minimum is None or value >= minimum),
            label + " is not a valid integer")
    return value


def safe_relative(value):
    require(isinstance(value, str) and value and "\0" not in value,
            "invalid inventory path")
    path = PurePosixPath(value)
    require(value == "." or (not path.is_absolute() and value == path.as_posix()
            and all(part not in {".", ".."} for part in path.parts)),
            "unsafe inventory path: " + value)
    return value


def parent_of(relative):
    return None if relative == "." else str(PurePosixPath(relative).parent)


class SetDigest:
    """Exact R7 input-set fingerprint, checked in addition to the file SHA."""
    def __init__(self):
        self.count = self.total = self.xor = 0

    def add(self, relative, metadata):
        raw = json.dumps(dict(path=relative, **metadata), sort_keys=True,
                         separators=(",", ":")).encode()
        number = int.from_bytes(hashlib.sha256(b"r7-fixture-row-v1\0" + raw).digest(), "big")
        self.count += 1
        self.total = (self.total + number) % (1 << 256)
        self.xor ^= number

    def finish(self):
        raw = (b"r7-fixture-set-v1\0" + str(self.count).encode() + b"\0"
               + self.total.to_bytes(32, "big") + self.xor.to_bytes(32, "big"))
        return hashlib.sha256(raw).hexdigest()


def target_bytes(metadata):
    if "target_hex" in metadata:
        value = metadata["target_hex"]
        require(isinstance(value, str), "symlink target_hex is not text")
        try:
            target = bytes.fromhex(value)
        except ValueError as error:
            raise ProofError("invalid symlink target_hex") from error
        require(value == target.hex(), "noncanonical symlink target_hex")
        if "target" in metadata:
            require(isinstance(metadata["target"], str) and
                    os.fsencode(metadata["target"]) == target, "conflicting symlink targets")
    else:
        require(isinstance(metadata.get("target"), str), "symlink target missing")
        target = os.fsencode(metadata["target"])
    require(target and b"\0" not in target, "invalid symlink target bytes")
    return target


def projected_row(row, uid, gid):
    require(isinstance(row, dict), "inventory row is not an object")
    relative = safe_relative(row.get("path"))
    value, identity = row.get("metadata"), row.get("identity")
    require(isinstance(value, dict) and isinstance(identity, dict), "inventory metadata/identity missing")
    kind = value.get("kind")
    require(kind in {"file", "directory", "symlink"}, "unsupported inventory kind: " + relative)
    mode = integer(value.get("mode"), "source mode", 0)
    integer(value.get("uid"), "source uid", 0)
    integer(value.get("gid"), "source gid", 0)
    mtime = integer(value.get("mtime_ns"), "source mtime")
    source_device = integer(identity.get("device"), "source device", 0)
    source_inode = integer(identity.get("inode"), "source inode", 1)
    source_nlink = integer(identity.get("nlink"), "source nlink", 1)
    require(mode <= 0o7777, "source mode contains file type bits: " + relative)
    size = checksum = target = None
    if kind == "file":
        require(mode & ~0o777 == 0, "nonportable regular mode: " + relative)
        size = integer(value.get("size"), "regular size", 0)
        checksum = digest_value(value.get("content_sha256"), "regular content digest")
    elif kind == "directory":
        require(mode & ~0o1777 == 0, "nonportable directory mode: " + relative)
    else:
        mode = 0o777
        target = target_bytes(value).hex()
        size = len(bytes.fromhex(target))
    return (relative, parent_of(relative), kind, mode, uid, gid, str(mtime), size,
            checksum, target, str(source_device), str(source_inode), source_nlink)


def load_expected(database, inventory, uid, gid):
    database.executescript("""
        PRAGMA journal_mode=MEMORY;
        PRAGMA synchronous=OFF;
        PRAGMA cache_size=-2048;
        PRAGMA temp_store=FILE;
        CREATE TABLE expected (
            path TEXT PRIMARY KEY, parent TEXT, kind TEXT, mode INTEGER,
            uid INTEGER, gid INTEGER, mtime TEXT, size INTEGER,
            checksum TEXT, target TEXT, source_device TEXT, source_inode TEXT,
            source_nlink INTEGER, nlink INTEGER, seen INTEGER NOT NULL DEFAULT 0,
            observed_device TEXT, observed_inode TEXT, observed_nlink INTEGER
        );
        CREATE INDEX children ON expected(parent,kind);
        CREATE INDEX source_aliases ON expected(kind,source_device,source_inode);
        BEGIN;
    """)
    fingerprint = SetDigest()
    totals = dict(entries=0, regular_files=0, regular_bytes=0)
    footer = None
    with closing(Path(inventory).open("r", encoding="utf-8"), lambda stream: stream.close()) as stream:
        header = json.loads(next(stream))
        require(isinstance(header, dict) and header.get("schema") == INPUT_SCHEMA,
                "wrong inventory header schema")
        for line in stream:
            row = json.loads(line)
            require(isinstance(row, dict), "inventory row is not an object")
            require(footer is None, "inventory data after completion")
            if "event" in row:
                require(row.get("event") == "completed" and row.get("status") == "SEALED_SETUP_ONLY",
                        "inventory completion did not seal setup")
                footer = row
                continue
            projected = projected_row(row, uid, gid)
            try:
                database.execute("INSERT INTO expected(path,parent,kind,mode,uid,gid,mtime,size,"
                                 "checksum,target,source_device,source_inode,source_nlink) "
                                 "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)", projected)
            except sqlite3.IntegrityError as error:
                raise ProofError("duplicate inventory path: " + projected[0]) from error
            fingerprint.add(projected[0], row["metadata"])
            totals["entries"] += 1
            if projected[2] == "file":
                totals["regular_files"] += 1
                totals["regular_bytes"] += projected[7]
    require(footer is not None, "inventory missing sealed completion")
    for field, count in totals.items():
        require(type(footer.get(field)) is int and footer[field] == count,
                "inventory completion count mismatch: " + field)
    require(footer.get("content_metadata_set_sha256") == fingerprint.finish(),
            "inventory metadata/content set fingerprint mismatch")
    require(database.execute("SELECT kind FROM expected WHERE path='.'").fetchone() == ("directory",),
            "inventory root directory missing")
    require(database.execute("SELECT e.path FROM expected e LEFT JOIN expected p ON e.parent=p.path "
                             "WHERE e.path!='.' AND (p.path IS NULL OR p.kind!='directory') LIMIT 1").fetchone() is None,
            "inventory has missing or non-directory parent")
    invalid_alias = database.execute("SELECT source_device,source_inode FROM expected WHERE kind='file' "
        "GROUP BY source_device,source_inode HAVING MIN(source_nlink)!=COUNT(*) OR MAX(source_nlink)!=COUNT(*) "
        "OR MIN(mode)!=MAX(mode) OR MIN(mtime)!=MAX(mtime) OR MIN(size)!=MAX(size) "
        "OR MIN(checksum)!=MAX(checksum) LIMIT 1").fetchone()
    require(invalid_alias is None, "inventory regular alias class is inconsistent or has external names")
    database.execute("UPDATE expected SET nlink=2+(SELECT COUNT(*) FROM expected child "
                     "WHERE child.parent=expected.path AND child.kind='directory') WHERE kind='directory'")
    database.execute("UPDATE expected SET nlink=source_nlink WHERE kind='file'")
    database.execute("UPDATE expected SET nlink=1 WHERE kind='symlink'")
    database.execute("COMMIT")
    return totals


def emit(stream, row):
    stream.write(json.dumps(row, sort_keys=True) + "\n")


def write_expected(database, path):
    with closing(Path(path).open("x", encoding="utf-8"), lambda stream: stream.close()) as stream:
        emit(stream, dict(schema="r8-projected-expected-v1", projection=PROJECTION))
        for row in database.execute("SELECT path,kind,mode,uid,gid,mtime,size,checksum,target,nlink "
                                    "FROM expected ORDER BY path"):
            value = dict(zip(("path", "kind", "mode", "uid", "gid", "mtime_ns", "size",
                              "content_sha256", "target_hex", "nlink"), row))
            value["mtime_ns"] = int(value["mtime_ns"])
            emit(stream, value)


def stable(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


def observe(info):
    kind = ("file" if stat.S_ISREG(info.st_mode) else "directory"
            if stat.S_ISDIR(info.st_mode) else "symlink" if stat.S_ISLNK(info.st_mode) else "unsupported")
    return dict(kind=kind, mode=stat.S_IMODE(info.st_mode), uid=info.st_uid,
                gid=info.st_gid, mtime_ns=info.st_mtime_ns, ctime_ns=info.st_ctime_ns,
                size=info.st_size, nlink=info.st_nlink, device=info.st_dev, inode=info.st_ino)


def read_file(parent_fd, name, before):
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
    descriptor = os.open(name, flags, dir_fd=parent_fd)
    checksum, total = hashlib.sha256(), 0
    with closing(descriptor, os.close):
        require(stable(os.fstat(descriptor)) == stable(before), "regular identity changed before read: " + name)
        while True:
            block = os.read(descriptor, WINDOW)
            if not block:
                break
            total += len(block)
            checksum.update(block)
        require(total == before.st_size and stable(os.fstat(descriptor)) == stable(before)
                == stable(os.stat(name, dir_fd=parent_fd, follow_symlinks=False)),
                "regular file changed during full-byte read: " + name)
    return checksum.hexdigest(), total


class Traversal:
    """Bounded concurrent dirfd traversal; observation only, never comparison.

    WALKERS threads each own one directory stream per depth and never follow a
    symlink. A walker opens a child directory under its parent descriptor and
    hands it to an idle walker through a queue of at most 2*WALKERS entries, or
    descends itself when that queue is full, so open descriptors stay bounded
    by walkers times depth. Every name, its metadata and its complete payload
    digest or symlink target reach the single comparing thread as one row, in
    arrival order, through a queue of at most ROWS entries. A walker's first
    original failure travels through the same queue, after its earlier rows.
    """
    def __init__(self, root, walkers):
        self.root, self.walkers = root, walkers
        self.rows = queue.Queue(maxsize=ROWS)
        self.pending = queue.Queue(maxsize=2 * walkers)
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.outstanding = 0
        self.close_failures = []
        self.threads = [threading.Thread(target=self.walker, name="oracle-walker-%d" % index)
                        for index in range(walkers)]

    def release(self, value, close):
        try:
            close(value)
        except BaseException as error:
            with self.lock:
                self.close_failures.append(str(error))

    def finished(self):
        with self.lock:
            self.outstanding -= 1
            last = self.outstanding == 0
        if last:
            for _ in self.threads:
                self.pending.put(None)

    def offer(self, frame):
        with self.lock:
            self.outstanding += 1
        try:
            self.pending.put_nowait(frame)
            return True
        except queue.Full:
            with self.lock:
                self.outstanding -= 1
            return False

    def entry(self, relative, parent_fd, name, info):
        value = observe(info)
        if value["kind"] == "file":
            value["content_sha256"], value["read_bytes"] = read_file(parent_fd, name, info)
        elif value["kind"] == "symlink":
            value["target_hex"] = os.fsencode(os.readlink(name, dir_fd=parent_fd)).hex()
            require(stable(os.stat(name, dir_fd=parent_fd, follow_symlinks=False)) == stable(info),
                    "symlink changed during observation: " + relative)
        self.rows.put((relative, info, value))

    def directory(self, frame):
        stack = [(*frame, os.scandir(frame[1]))]
        try:
            while stack and not self.stop.is_set():
                relative, parent_fd, before, iterator = stack[-1]
                entry = next(iterator, None)
                if entry is None:
                    require(stable(before) == stable(os.fstat(parent_fd)), "directory changed: " + relative)
                    stack.pop()
                    iterator.close()
                    os.close(parent_fd)
                    continue
                child = entry.name if relative == "." else relative + "/" + entry.name
                safe_relative(child)
                info = os.stat(entry.name, dir_fd=parent_fd, follow_symlinks=False)
                self.entry(child, parent_fd, entry.name, info)
                if stat.S_ISDIR(info.st_mode):
                    child_fd = os.open(entry.name, DIRECTORY, dir_fd=parent_fd)
                    try:
                        require(stable(os.fstat(child_fd)) == stable(info), "directory changed before open: " + child)
                        if not self.offer((child, child_fd, info)):
                            stack.append((child, child_fd, info, os.scandir(child_fd)))
                    except BaseException:
                        self.release(child_fd, os.close)
                        raise
        finally:
            for _, parent_fd, _, iterator in reversed(stack):
                self.release(iterator, lambda value: value.close())
                self.release(parent_fd, os.close)

    def walker(self):
        try:
            while True:
                frame = self.pending.get()
                if frame is None:
                    return
                try:
                    if self.stop.is_set():
                        self.release(frame[1], os.close)
                    else:
                        self.directory(frame)
                except BaseException as error:
                    self.stop.set()
                    self.rows.put(error)
                finally:
                    self.finished()
        finally:
            self.rows.put(None)

    def observations(self):
        """Yield (relative, lstat, observation) rows; raise the first failure."""
        descriptor = os.open(self.root, DIRECTORY)
        initial = os.fstat(descriptor)
        try:
            require(stable(initial) == stable(self.root.lstat()), "root changed before traversal")
        except BaseException:
            self.release(descriptor, os.close)
            raise
        yield ".", initial, observe(initial)
        self.outstanding = 1
        self.pending.put((".", descriptor, initial))
        for thread in self.threads:
            thread.start()
        live = self.walkers
        try:
            while live:
                row = self.rows.get()
                if row is None:
                    live -= 1
                elif isinstance(row, BaseException):
                    raise row
                else:
                    yield row
            require(stable(initial) == stable(self.root.lstat()), "root changed during traversal")
        finally:
            # Stop at the next name boundary and receive every walker's exit,
            # so no thread, directory stream or descriptor outlives the proof.
            self.stop.set()
            while live:
                if self.rows.get() is None:
                    live -= 1
            for thread in self.threads:
                thread.join()
        require(not self.close_failures, "independent close failures: " + "; ".join(self.close_failures))


def check_namespace(database, root, output, totals, walkers=WALKERS):
    traversal = Traversal(root, walkers)
    with closing(Path(output).open("x", encoding="utf-8"), lambda stream: stream.close()) as stream:
        emit(stream, dict(schema="r8-full-byte-observations-v2", root=str(root), walkers=walkers,
                          order="arrival at the comparing thread"))
        observations = traversal.observations()
        try:
            for relative, info, value in observations:
                expected = database.execute("SELECT kind,mode,uid,gid,mtime,size,checksum,target,nlink,seen "
                                            "FROM expected WHERE path=?", (relative,)).fetchone()
                require(expected is not None, "extra namespace name: " + relative)
                expected = (*expected[:4], int(expected[4]), *expected[5:])
                require(expected[9] == 0, "duplicate observed name: " + relative)
                fields = ("kind", "mode", "uid", "gid", "mtime_ns")
                for index, field in enumerate(fields):
                    require(value[field] == expected[index], "metadata mismatch " + field + ": " + relative)
                require(value["ctime_ns"] == expected[4], "metadata mismatch ctime_ns: " + relative)
                require(value["nlink"] == expected[8], "metadata mismatch nlink: " + relative)
                if value["kind"] == "file":
                    require(value["size"] == expected[5], "regular size mismatch: " + relative)
                    totals["hashed_files"] += 1
                    totals["hashed_bytes"] += value.pop("read_bytes")
                    require(value["content_sha256"] == expected[6], "regular content mismatch: " + relative)
                elif value["kind"] == "symlink":
                    require(value["target_hex"] == expected[7] and value["size"] == expected[5],
                            "symlink target/size mismatch: " + relative)
                database.execute("UPDATE expected SET seen=1,observed_device=?,observed_inode=?,observed_nlink=? "
                                 "WHERE path=?", (str(info.st_dev), str(info.st_ino), value["nlink"], relative))
                totals["paths"] += 1
                emit(stream, dict(path=relative, match=True, observation=value))
        except BaseException as error:
            try:
                observations.close()
            except BaseException as closing_error:
                error.independent_close_failures = [
                    *getattr(error, "independent_close_failures", []), str(closing_error)]
            error.independent_close_failures = [
                *getattr(error, "independent_close_failures", []), *traversal.close_failures]
            raise
        missing = database.execute("SELECT path FROM expected WHERE seen=0 LIMIT 1").fetchone()
        require(missing is None, "missing namespace name: " + (missing[0] if missing else ""))
        alias = database.execute("SELECT source_device,source_inode FROM expected WHERE kind='file' "
            "GROUP BY source_device,source_inode HAVING COUNT(DISTINCT observed_device||':'||observed_inode)!=1 "
            "OR MIN(observed_nlink)!=COUNT(*) OR MAX(observed_nlink)!=COUNT(*) LIMIT 1").fetchone()
        require(alias is None, "regular alias class split or link count mismatch")
        merged = database.execute("SELECT observed_device,observed_inode FROM expected WHERE kind='file' "
            "GROUP BY observed_device,observed_inode HAVING COUNT(DISTINCT source_device||':'||source_inode)!=1 "
            "LIMIT 1").fetchone()
        require(merged is None, "unrelated regular alias classes merged")
        emit(stream, dict(event="completed", status="PASS", paths=totals["paths"],
                          hashed_files=totals["hashed_files"], hashed_bytes=totals["hashed_bytes"]))


def prove(root, inventory, expected_sha256, output, uid=501, gid=20, walkers=WALKERS):
    root, inventory, output = Path(root), Path(inventory), Path(output)
    require(not root.is_symlink(), "root must not be a symlink")
    root = root.resolve(strict=False)
    inventory = inventory.resolve(strict=False)
    output = output.parent.resolve(strict=True) / output.name
    require(output != root and root not in output.parents and inventory != root and root not in inventory.parents,
            "oracle inputs/output must be external to observed root")
    require(output != inventory and not output.exists() and not output.is_symlink(), "output must be fresh")
    artifacts = output.with_name(output.name + ".artifacts")
    require(not artifacts.exists() and not artifacts.is_symlink(), "artifact output must be fresh")
    artifacts.mkdir()
    result = dict(schema=SCHEMA, status="FAIL", root=str(root), inventory=str(inventory),
                  expected_inventory_sha256=expected_sha256, output=str(output),
                  artifacts=str(artifacts),
                  projection=PROJECTION, owner_uid=uid, owner_gid=gid, read_window_bytes=WINDOW,
                  walkers=walkers, paths=0, hashed_files=0, hashed_bytes=0, phase="argument_admission", differences=[])
    try:
        integer(uid, "configured uid", 0)
        integer(gid, "configured gid", 0)
        require(uid <= 0xffffffff and gid <= 0xffffffff, "configured ownership exceeds u32")
        require(1 <= integer(walkers, "walker count") <= 64, "walker count outside 1..64")
        require(root.is_dir() and not root.is_symlink(), "root must be an actual directory")
        result["oracle_source_sha256"] = sha256(__file__)
        result["phase"] = "input_seal"
        digest_value(expected_sha256, "expected inventory digest")
        result["inventory_sha256_before"] = sha256(inventory)
        require(result["inventory_sha256_before"] == expected_sha256, "inventory SHA-256 mismatch")
        database = sqlite3.connect(artifacts / "expected.sqlite", timeout=0, isolation_level=None)
        with closing(database, lambda connection: connection.close()):
            result["phase"] = "expected_validation"
            result["expected_totals"] = load_expected(database, inventory, uid, gid)
            write_expected(database, artifacts / "expected.jsonl")
            result["projected_expected_sha256"] = sha256(artifacts / "expected.jsonl")
            # Recheck the external seal before touching product names or payloads.
            require(sha256(inventory) == expected_sha256, "inventory changed during expectation preparation")
            result["phase"] = "namespace"
            check_namespace(database, root, artifacts / "observed.jsonl", result, walkers)
            result["phase"] = "input_seal_after"
            result["inventory_sha256_after"] = sha256(inventory)
            require(result["inventory_sha256_after"] == expected_sha256, "inventory changed during proof")
            require(result["paths"] == result["expected_totals"]["entries"]
                    and result["hashed_files"] == result["expected_totals"]["regular_files"]
                    and result["hashed_bytes"] == result["expected_totals"]["regular_bytes"],
                    "full coverage counters mismatch")
            result["regular_alias_groups"] = database.execute("SELECT COUNT(*) FROM (SELECT 1 FROM expected "
                "WHERE kind='file' GROUP BY source_device,source_inode HAVING COUNT(*)>1)").fetchone()[0]
        result.update(status="PASS", phase="complete")
    except Exception as error:
        result.update(original_error=str(error), original_error_type=type(error).__name__,
                      exactfailure=str(error), original_phase=result["phase"],
                      differences=[dict(phase=result["phase"], reason=str(error))],
                      independent_close_failures=getattr(error, "independent_close_failures", []))
    for name in ("expected.jsonl", "observed.jsonl"):
        if (artifacts / name).is_file():
            try:
                result[name.replace(".jsonl", "_sha256")] = sha256(artifacts / name)
            except Exception as error:
                if result["status"] == "PASS":
                    result.update(status="FAIL", exactfailure=str(error), original_error=str(error),
                                  original_error_type=type(error).__name__, original_phase="artifact_hash",
                                  differences=[dict(phase="artifact_hash", reason=str(error))])
                else:
                    result.setdefault("independent_output_failures", []).append(str(error))
    result.setdefault("original_phase", None)
    result.setdefault("exactfailure", None)
    with closing(output.open("x", encoding="utf-8"), lambda stream: stream.close()) as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
        stream.write("\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--inventory", required=True, type=Path)
    parser.add_argument("--inventory-sha256", "--expected-sha256", dest="expected_sha256", required=True)
    parser.add_argument("--uid", type=int, default=501)
    parser.add_argument("--gid", type=int, default=20)
    parser.add_argument("--walkers", type=int, default=WALKERS)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    try:
        result = prove(arguments.root, arguments.inventory, arguments.expected_sha256,
                       arguments.output, arguments.uid, arguments.gid, arguments.walkers)
    except Exception as error:
        result = dict(schema=SCHEMA, status="FAIL", original_error=str(error),
                      original_error_type=type(error).__name__, phase="argument_admission")
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())

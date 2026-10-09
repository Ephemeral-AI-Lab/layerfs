"""Actual-byte source seals over explicit L/N/P relevant input scopes.

Control scopes exclude unrelated LayerFS product edits. Their already sealed
SDK/daemon dependencies remain identified by compilation/dependency/binary
receipts; they are not rebuilt or inferred from changed live product source.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess

SCHEMA = "r7-source-inventory-v1"
POLICY = "r7-relevant-source-v1"
CONTROL = ("core/benchmark/fs-bench-pro/r7", "core/benchmark/r7-runtime", "core/benchmark/r7-cache", "core/benchmark/r7-tools", ".cargo")
P_EXTRA = ("core/benchmark/r7-passthrough", "core/vendor/fuser-0.18.0")
L_EXTRA = ("core/crates", "core/Cargo.toml", "core/Cargo.lock", "core/vendor/fuser-0.18.0")
IGNORED_DIRECTORIES = {"target", "__pycache__", ".git", "tests", "fixtures", "examples"}
IGNORED_SUFFIXES = {".md", ".png", ".jpg", ".svg", ".pyc", ".DS_Store"}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def roots(arm):
    require(arm in {"L","N","P"}, "source inventory arm must be L/N/P")
    return CONTROL + (L_EXTRA if arm == "L" else P_EXTRA if arm == "P" else ())


def relevant(relative, arm):
    path = Path(relative)
    if path.name == ".DS_Store":
        return False
    if any(part in IGNORED_DIRECTORIES for part in path.parts):
        return False
    selected = any(path == Path(root) or Path(root) in path.parents for root in roots(arm))
    if not selected:
        return False
    if path.parts[:2] == ("core","crates"):
        # Retain outside-src shipped input of every kind, not only Rust/SQL.
        # This intentionally includes some crate metadata rather than silently
        # dropping a compile-time include with an unfamiliar extension.
        return path.suffix != ".pyc"
    return not path.name.startswith("test_") and path.name != ".DS_Store" and path.suffix not in IGNORED_SUFFIXES


def files(root, arm):
    found = set()
    for prefix in roots(arm):
        source = root/prefix
        require(source.exists(), "required relevant source root missing: " + prefix)
        if source.is_file():
            found.add(prefix)
            continue
        require(not source.is_symlink(), "source input directory alias is not sealed: " + prefix)
        for directory, directories, names in os.walk(source,followlinks=False):
            directories[:] = [name for name in directories if name not in IGNORED_DIRECTORIES]
            for name in directories:
                require(not (Path(directory)/name).is_symlink(), "source input subtree alias is not sealed")
            for name in names:
                relative = (Path(directory)/name).relative_to(root).as_posix()
                if relevant(relative,arm):
                    found.add(relative)
    return sorted(found)


def git(root, *arguments):
    return subprocess.check_output(["git",*arguments],cwd=root).decode("utf-8",errors="surrogateescape")


def assert_clean(root, arm, selected):
    tracked = set(git(root,"ls-files","-z","--",*roots(arm)).split("\0"))
    untracked = sorted(set(selected)-tracked)
    require(not untracked, "untracked relevant source input: " + repr(untracked[:5]))
    changed = [name for name in git(root,"diff","--name-only","-z","HEAD","--",*roots(arm)).split("\0") if name and relevant(name,arm)]
    require(not changed, "dirty relevant source input: " + repr(changed[:5]))


def file_record(root, relative):
    path = root/relative
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode), "relevant source input must be regular: " + relative)
    value = hashlib.sha256()
    descriptor = os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    try:
        opened = os.fstat(descriptor)
        fields = lambda info:(info.st_dev,info.st_ino,info.st_size,info.st_mode,info.st_mtime_ns,info.st_ctime_ns)
        require(fields(before) == fields(opened), "source file identity changed before byte read")
        total = 0
        while True:
            block = os.read(descriptor,65536)
            if not block:
                break
            value.update(block)
            total += len(block)
        require(fields(opened) == fields(os.fstat(descriptor)) == fields(path.lstat()) and total == opened.st_size,
                "source file changed during complete byte inventory")
    finally:
        os.close(descriptor)
    return dict(path=relative,sha256=value.hexdigest(),bytes=total,mode=stat.S_IMODE(opened.st_mode))


def collect(root, arm):
    selected = files(root,arm)
    assert_clean(root,arm,selected)
    records = [file_record(root,relative) for relative in selected]
    require(selected == files(root,arm), "source membership changed during byte inventory")
    assert_clean(root,arm,selected)
    return records


def set_sha(records):
    return hashlib.sha256(json.dumps(records,sort_keys=True,separators=(",",":")).encode()).hexdigest()


def seal(root, arm, output):
    root = Path(root).resolve(strict=True)
    commit, tree = git(root,"rev-parse","HEAD").strip(),git(root,"rev-parse","HEAD^{tree}").strip()
    records = collect(root,arm)
    require(commit == git(root,"rev-parse","HEAD").strip() and tree == git(root,"rev-parse","HEAD^{tree}").strip(), "source commit changed during inventory")
    value = dict(schema=SCHEMA,scope_policy=POLICY,arm=arm,source_commit=commit,source_tree=tree,
                 scope_roots=list(roots(arm)),source_set_sha256=set_sha(records),files=records,
                 dirty=False,untracked_relevant_files=0,coverage="complete current relevant regular-file membership and actual bytes",
                 control_dependency_scope="immutable compiled dependency/binary seals; unrelated live product changes excluded for N/P")
    with Path(output).open("x") as stream:
        json.dump(value,stream,sort_keys=True,indent=2)
        stream.write("\n")
    return value


def verify(config, arm):
    root = Path(__file__).resolve().parents[4]
    specified = config.get("source_inventory",{})
    path = Path(specified["path"])
    from .deployment import digest
    require(digest(path) == specified.get("sha256"), "source inventory artifact SHA missing or changed")
    value = json.loads(path.read_text())
    require(value.get("schema") == SCHEMA and value.get("scope_policy") == POLICY and value.get("arm") == arm and value.get("scope_roots") == list(roots(arm)),
            "source inventory explicit arm/coverage policy differs")
    require(value.get("dirty") is False and value.get("untracked_relevant_files") == 0, "source inventory is not a clean seal")
    actual = collect(root,arm)
    require(value.get("files") == actual and value.get("source_set_sha256") == set_sha(actual), "actual source bytes/membership differ from sealed inventory")
    if arm == "L":
        require(value["source_commit"] == config["identities"]["source_commit"] == git(root,"rev-parse","HEAD").strip() and
                value["source_tree"] == config["identities"]["source_tree"] == git(root,"rev-parse","HEAD^{tree}").strip(),
                "actual L source commit/tree differs from sealed source inventory")
    return dict(status="PASS",arm=arm,source_set_sha256=value["source_set_sha256"],inventory_sha256=specified["sha256"],files=len(actual),scope_policy=POLICY)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arm",choices=["L","N","P"],required=True)
    parser.add_argument("--root",type=Path,default=Path(__file__).resolve().parents[4])
    parser.add_argument("--output",type=Path,required=True)
    args = parser.parse_args()
    value = seal(args.root,args.arm,args.output)
    from .deployment import digest
    print(json.dumps(dict(status="SEALED",path=str(args.output),sha256=digest(args.output),source_set_sha256=value["source_set_sha256"],arm=args.arm,files=len(value["files"])),sort_keys=True))


if __name__ == "__main__":
    main()

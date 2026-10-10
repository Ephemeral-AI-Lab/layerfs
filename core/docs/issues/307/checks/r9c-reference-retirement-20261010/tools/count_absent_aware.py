#!/usr/bin/env python3
"""Per-commit production LOC that states an absent scope instead of reporting 0.

Same method as the R8b helper: independent `git archive` extractions of the
first parent and of the counted tree, counted by the pinned
tools/production_loc.py. The difference is that a scope whose directory is not
in the tree is archived as absent and recorded as `present: false`, because
the pinned counter reads a missing directory as 0 silently.

Usage: count_absent_aware.py staged|head [output.json]
"""
from pathlib import Path
import hashlib
import importlib.util
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import tomllib

PINNED = "c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb"
SCOPES = {"core": "core/crates", "reference": "crates"}
counter = Path("tools/production_loc.py")
digest = hashlib.sha256(counter.read_bytes()).hexdigest()
assert digest == PINNED, digest
spec = importlib.util.spec_from_file_location("counter", counter)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def git(*arguments, check=True):
    done = subprocess.run(["git", *arguments], capture_output=True, text=True)
    if check and done.returncode:
        raise SystemExit(done.stderr)
    return done.stdout.strip() if done.returncode == 0 else None


def count(ref):
    trees = {scope: git("rev-parse", "--verify", "--quiet", f"{ref}:{path}", check=False)
             for scope, path in SCOPES.items()}
    assert trees["core"], "the active core scope must exist"
    paths = [path for scope, path in SCOPES.items() if trees[scope]] + ["core/Cargo.toml"]
    with tempfile.TemporaryDirectory(prefix="layerfs-r9c-loc-") as directory:
        root = Path(directory)
        data = subprocess.check_output(["git", "archive", ref, *paths])
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            archive.extractall(root, filter="data")
        totals = module.scan(root)
        members = tomllib.loads((root / "core/Cargo.toml").read_text())["workspace"]["members"]
        active = ["core/" + member + "/" for member in members]
        outside = [entry["path"] for entry in module.per_file(root)["files"]
                   if entry["scope"] == "core" and not any(entry["path"].startswith(p) for p in active)]
    assert not outside, outside
    scopes = {}
    for scope in SCOPES:
        present = trees[scope] is not None
        lines = totals["scopes"][scope]["lines"]
        assert present or lines == 0
        scopes[scope] = dict(present=present, tree=trees[scope], files=totals["scopes"][scope]["files"],
                             production_loc=lines if present else None)
    return dict(combined=totals["combined"], scopes=scopes)


mode = sys.argv[1]
if mode == "staged":
    parent, final = git("rev-parse", "HEAD"), git("write-tree")
else:
    parent, final = git("rev-parse", "HEAD^"), git("rev-parse", "HEAD^{tree}")
before, after = count(parent), count(final)
record = dict(
    first_parent=parent, counted_tree=final, mode=mode, counter_sha256=digest,
    method="Independent parent/final git archive extractions; pinned tools/production_loc.py scan; "
           "first-party Rust and shipped SQL, excluding comments, blanks, inline and transitive tests, "
           "docs, tools and third-party code; every core file checked to lie under an active Cargo member; "
           "a scope whose directory is not in the tree is recorded as absent, not as 0",
    before=before, after=after, delta=after["combined"] - before["combined"])
if len(sys.argv) > 2:
    Path(sys.argv[2]).write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record, indent=2))

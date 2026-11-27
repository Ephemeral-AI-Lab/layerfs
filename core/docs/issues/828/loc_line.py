"""Print the AGENTS.md production LOC line for the commit being prepared.

Usage (repository root): python3 -B core/docs/issues/828/loc_line.py [BEFORE] [AFTER]

BEFORE defaults to HEAD (the first parent of the next commit); AFTER defaults to
the staged index tree. Both snapshots are exported with `git archive` and counted
by one counter: tools/production_loc.py as it stands in the AFTER tree, so a
commit that teaches the counter a new crate name counts both sides the same way.
Unstaged and untracked work is excluded.
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

SCOPES = ("core", "engine", "control", "api")


def git(*args, cwd=None, **kw):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, **kw).stdout


def export(tree, into):
    data = git("archive", "--format=tar", tree, "--", "core", "engine", "control", "api", "tools/production_loc.py")
    subprocess.run(["tar", "-x", "-C", str(into)], input=data, check=True)


def count(counter, root):
    out = subprocess.run(
        [sys.executable, "-B", str(counter), "--root", str(root), "--json"], check=True, capture_output=True, text=True
    ).stdout
    result = json.loads(out)
    return {s: result["scopes"][s]["lines"] for s in SCOPES}, result["combined"]


def main(argv):
    before = argv[0] if len(argv) > 0 else "HEAD"
    after = argv[1] if len(argv) > 1 else git("write-tree").decode().strip()
    with tempfile.TemporaryDirectory() as tmp:
        b, a = Path(tmp, "before"), Path(tmp, "after")
        b.mkdir()
        a.mkdir()
        export(before, b)
        export(after, a)
        counter = a / "tools/production_loc.py"
        bs, bt = count(counter, b)
        as_, at = count(counter, a)
    print(f"Production LOC: {bt:,} -> {at:,} (delta {at - bt:+,})")
    for s in SCOPES:
        print(f"  {s}: {bs[s]:,} -> {as_[s]:,} (delta {as_[s] - bs[s]:+,})")
    print("  reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)")
    print(f"  Method: tools/production_loc.py of the after tree on `git archive` of {before} and the staged tree.")


if __name__ == "__main__":
    main(sys.argv[1:])

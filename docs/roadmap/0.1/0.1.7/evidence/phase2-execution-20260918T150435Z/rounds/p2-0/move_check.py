#!/usr/bin/env python3
"""P2-0 move check: the split moved code, it did not write any.

Compares every non-blank line of the parent `cas/owner.rs` (parent commit
`0a1d74742`, file blob 962 lines) against the after-tree's `cas/*.rs`, as a
multiset after stripping the one marker the split is allowed to add
(`pub(super) `). Reports:

  * parent lines missing from the after tree (code lost) - must be 0;
  * after lines not present in the parent (code written) - must be only the
    new module headers/import lines and the `impl MutationOwner {` lines.

Usage: python3 move_check.py <parent-owner.rs> <after-cas-dir>
"""
import collections
import pathlib
import re
import sys

MARKER = re.compile(r"^(?P<indent>\s*)pub\(super\) ")


def normalise(line: str) -> str:
    return MARKER.sub(lambda m: m.group("indent"), line.rstrip())


def lines_of(path: pathlib.Path) -> list[str]:
    return [normalise(line) for line in path.read_text().split("\n") if line.strip()]


def main() -> int:
    parent_path, after_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    parent = lines_of(parent_path)
    after_files = ["owner.rs", "pool_lane.rs", "placement.rs", "lifecycle.rs", "selection.rs"]
    after: list[str] = []
    for name in after_files:
        after.extend(lines_of(after_dir / name))
    parent_counts, after_counts = collections.Counter(parent), collections.Counter(after)
    lost = parent_counts - after_counts
    added = after_counts - parent_counts
    print(f"parent non-blank lines: {len(parent)}")
    print(f"after  non-blank lines: {len(after)} (delta {len(after) - len(parent):+d})")
    print(f"parent lines lost: {sum(lost.values())}")
    for line, count in lost.items():
        print(f"  LOST x{count}: {line}")
    print(f"after lines not in parent: {sum(added.values())}")
    classes = {"module doc": 0, "import": 0, "impl opening": 0, "brace": 0, "OTHER": 0}
    other = []
    for line, count in added.items():
        if line.startswith("//!"):
            classes["module doc"] += count
        elif line.startswith("use "):
            classes["import"] += count
        elif line == "impl MutationOwner {":
            classes["impl opening"] += count
        elif line == "}":
            classes["brace"] += count
        else:
            classes["OTHER"] += count
            other.append(f"x{count}: {line}")
    for name, count in classes.items():
        print(f"  {name}: {count}")
    for line in other:
        print(f"  OTHER {line}")
    return 1 if lost or classes["OTHER"] else 0


if __name__ == "__main__":
    raise SystemExit(main())

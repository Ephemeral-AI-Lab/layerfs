"""Prefix self-spawn test filters with their module path in joined test binaries (#828).

Usage (repository root): reexact.py [--dry] FILE...

A test that runs its own binary again names one of its own tests (`--exact
name`). After #828 joined test files into area binaries, libtest names that test
`module::name`, so the bare name matches nothing and the child runs no test.
For a FILE that is a module of a `tests/<binary>/main.rs` binary, every string
literal equal to one of the file's own `#[test]` function names becomes
`"<module path>::name"`. Single-file binaries are left alone.
"""

import os
import re
import sys


def module_path(f):
    d = os.path.dirname(f)
    while "/tests/" in d + "/":
        if os.path.exists(os.path.join(d, "main.rs")) and os.path.basename(os.path.dirname(d)) == "tests":
            rel = os.path.relpath(f, d)[:-3].split("/")
            if rel[-1] == "mod":
                rel = rel[:-1]
            return None if rel == ["main"] else "::".join(rel)
        d = os.path.dirname(d)
    return None


def main(argv):
    dry = argv[:1] == ["--dry"]
    for f in argv[1:] if dry else argv:
        mod = module_path(f)
        if not mod:
            continue
        s = open(f).read()
        names = set(re.findall(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?fn\s+(\w+)", s))
        hits = []

        def sub(m):
            if m.group(1) in names:
                hits.append(m.group(1))
                return f'"{mod}::{m.group(1)}"'
            return m.group(0)

        new = re.sub(r'"(\w+)"', sub, s)
        if hits:
            print(f"{f}: {mod}:: {sorted(set(hits))}")
            if not dry:
                open(f, "w").write(new)


if __name__ == "__main__":
    main(sys.argv[1:])

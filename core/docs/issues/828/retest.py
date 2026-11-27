"""Respell `--test <binary>` commands whose test binary #828 joined into another.

Usage (repository root): retest.py BASE_METADATA.json NOW_METADATA.json [--dry] FILE...

The metadata files are `cargo metadata --no-deps --format-version 1` of the base
(3203f6acbf) and of the current tree. A base test target that is gone maps,
through the old file's new path (repath.py's map), to the binary whose root
holds it now and the module path inside it: `--test old` becomes
`--test binary module`, and a test-name filter right after it becomes
`module::filter`. When the binary now lives in another package, `-p` on that
line follows it. The package comes from `-p <package>` on the same line, else
from the only base package with that binary name.
"""

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import repath  # noqa: E402


def targets(path):
    meta = json.load(open(path))
    root = meta["workspace_root"].rstrip("/") + "/"
    out = {}
    for p in meta["packages"]:
        for t in p["targets"]:
            if "test" in t["kind"]:
                out[(p["name"], t["name"])] = t["src_path"].replace(root, "")
    return out


def main(argv):
    base, now = targets(argv[0]), targets(argv[1])
    rest = argv[2:]
    dry = rest[:1] == ["--dry"]
    files = rest[1:] if dry else rest
    _, fmap, _ = repath.build()
    roots = {}  # binary root dir -> (package, binary), for tests/<b>/main.rs roots
    files_of = {}  # src path -> (package, binary), for single-file roots
    for (pkg, name), src in now.items():
        files_of[src] = (pkg, name)
        if src.endswith("/main.rs"):
            roots[os.path.dirname(src)] = (pkg, name)
    gone = {k: v for k, v in base.items() if k not in now}
    by_name = {}
    for pkg, name in gone:
        by_name.setdefault(name, []).append(pkg)

    def where(pkg, name):
        old = gone.get((pkg, name))
        if old is None:
            return None
        new = fmap.get(old, old)
        if new in files_of:
            return files_of[new][0], files_of[new][1], None
        d = os.path.dirname(new)
        while d.count("/") >= 3:
            if d in roots:
                rel = os.path.relpath(new, d)[:-3].split("/")
                if rel[-1] == "mod":
                    rel = rel[:-1]
                return roots[d][0], roots[d][1], "::".join(rel)
            d = os.path.dirname(d)
        return None

    pat = re.compile(r"--test([ =])([A-Za-z0-9_]+)((?: +(?!-)[A-Za-z0-9_:]+)?)")
    unresolved = set()
    count = 0
    for f in files:
        lines = open(f).read().split("\n")
        changed = False
        for i, line in enumerate(lines):
            pm = re.search(r"-p[ =]([A-Za-z0-9_\-]+)", line)

            def sub(m):
                nonlocal count
                name = m.group(2)
                pkgs = [pm.group(1)] if pm else by_name.get(name, [])
                if len(pkgs) != 1 or (pkgs[0], name) not in gone:
                    if name in by_name and not (pkgs and (pkgs[0], name) in now):
                        unresolved.add((f, i + 1, name))
                    return m.group(0)
                hit = where(pkgs[0], name)
                if hit is None:
                    unresolved.add((f, i + 1, name))
                    return m.group(0)
                newpkg, binary, module = hit
                if newpkg != pkgs[0]:
                    moved_pkg.append((pkgs[0], newpkg))
                filt = m.group(3).strip()
                if module and filt:
                    filt = f"{module}::{filt}"
                elif module:
                    filt = module
                count += 1
                return f"--test{m.group(1)}{binary}" + (f" {filt}" if filt else "")

            moved_pkg = []
            new = pat.sub(sub, line)
            if moved_pkg:
                if len(set(moved_pkg)) == 1 and pm:
                    old, newpkg = moved_pkg[0]
                    new = re.sub(r"(-p[ =])" + re.escape(old) + r"\b", r"\g<1>" + newpkg, new)
                else:
                    unresolved.add((f, i + 1, "package moved: " + repr(sorted(set(moved_pkg)))))
            if new != line:
                lines[i] = new
                changed = True
        if changed and not dry:
            open(f, "w").write("\n".join(lines))
    for f, n, name in sorted(unresolved):
        print(f"unresolved {f}:{n}: {name}")
    print(f"rewritten {count}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1:])

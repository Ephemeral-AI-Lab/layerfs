#!/usr/bin/env python3
"""#828: load each shared test support file once per test binary.

Usage (repository root): hoist_support.py [--check] [CRATE_DIR...]

An area test binary (`tests/<area>/main.rs` plus one module per topic) is the
joining of files that each loaded their support with `#[path = "..."] mod x;`.
Loaded by two modules of one binary, the file is compiled twice and Clippy
refuses it (`duplicate_mod`). For each support file that two or more modules
of one binary load, this declares it once in `main.rs` and turns every
module's declaration into `use crate::<name>;` (`as <their name>` when the
names differ), keeping the declaration's `#[cfg]` on the `use`. A module that
declared the file without using it is left with an unused `use`; delete the
lines the compiler names. A support module named like one of the binary's own
modules is declared under its file stem instead.
"""

import os
import re
import sys
from pathlib import Path

DECL = re.compile(
    r'(?P<attrs>(?:[ \t]*#\[(?!path\b)[^\n]*\]\n)*)[ \t]*#\[path\s*=\s*"(?P<path>[^"]+)"\]\n'
    r"(?P<more>(?:[ \t]*#\[[^\n]*\]\n)*)[ \t]*(?P<vis>pub(?:\([a-z]+\))?\s+)?mod\s+(?P<name>\w+);\n"
)
MOD = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+(\w+);", re.M)
CFG = re.compile(r"#\[cfg\([^\n]*\)\]")


def binaries(crate):
    tests = Path(crate) / "tests"
    if not tests.is_dir():
        return
    for main in sorted(tests.glob("*/main.rs")):
        yield main


def hoist(main, check):
    folder = main.parent
    text = main.read_text()
    modules = MOD.findall(text)
    loads = {}
    for name in modules:
        path = folder / f"{name}.rs"
        if not path.exists():
            path = folder / name / "mod.rs"
        if not path.exists():
            continue
        source = path.read_text()
        for m in DECL.finditer(source):
            target = os.path.normpath(path.parent / m["path"])
            loads.setdefault(target, []).append((path, m))
    changed = []
    edits = {}
    head = []
    for target, places in sorted(loads.items()):
        if len({str(p) for p, _ in places}) < 2:
            continue
        first = places[0][1]
        canonical = first["name"]
        if canonical in modules or canonical in {m for m in MOD.findall(text)}:
            canonical = Path(target).stem
        cfgs = {tuple(CFG.findall(m["attrs"] + m["more"])) for _, m in places}
        cfg = "".join(f"{c}\n" for c in next(iter(cfgs))) if len(cfgs) == 1 else ""
        rel = os.path.relpath(target, folder)
        own_allow = "#![allow(dead_code)]" in Path(target).read_text()
        allow = "" if own_allow else "#[allow(dead_code)]\n"
        head.append(f'{cfg}{allow}#[path = "{rel}"]\nmod {canonical};\n')
        for path, m in places:
            own = "".join(f"{c}\n" for c in CFG.findall(m["attrs"] + m["more"]))
            alias = "" if m["name"] == canonical else f" as {m['name']}"
            vis = m["vis"] or ""
            edits.setdefault(path, []).append((m.group(0), f"{own}{vis}use crate::{canonical}{alias};\n"))
        changed.append(f"{main}: {rel} -> mod {canonical}")
    if not changed:
        return []
    if not check:
        for path, pairs in edits.items():
            source = path.read_text()
            for old, new in pairs:
                source = source.replace(old, new, 1)
            path.write_text(source)
        lines = text.splitlines(keepends=True)
        at = 0
        while at < len(lines) and (lines[at].startswith("//!") or lines[at].startswith("#![")):
            at += 1
        main.write_text("".join(lines[:at]) + "".join(head) + "".join(lines[at:]))
    return changed


def main(argv):
    check = "--check" in argv
    crates = [a for a in argv if not a.startswith("--")]
    if not crates:
        crates = [str(p.parent) for p in Path(".").glob("*/crates/*/Cargo.toml")]
    found = []
    for crate in sorted(crates):
        for main_rs in binaries(crate):
            found += hoist(main_rs, check)
    for line in found:
        print(line)
    return 1 if check and found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

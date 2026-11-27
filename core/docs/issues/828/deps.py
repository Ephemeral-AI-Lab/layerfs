#!/usr/bin/env python3
"""#828: bring each package's layerfs-* dependencies in line with the code.

Usage (repository root): deps.py [--check] [PACKAGE_DIR...]

For every workspace package (or the named ones) it reads the `layerfs_*` crate
names the Rust code uses (comments and strings ignored), split into the library
and binaries (`[dependencies]`) and tests/examples/benches (`[dev-dependencies]`),
then edits Cargo.toml:

- a dependency on a crate that no longer exists is renamed to its successor
  (keeping its attributes) when the code uses the successor, else removed;
- a layerfs dependency the code no longer names is reported, not removed;
- a layerfs crate the code names that is missing is added with its path;
- a dev-dependency already present as a normal dependency is dropped.

Feature lists that name removed crates are reported, not rewritten.
"""

import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import importlib.util

_spec = importlib.util.spec_from_file_location("move828", Path(__file__).resolve().parent / "move.py")
mv = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mv)

SUCCESSOR = {
    "engine/crates/layerfs-bridge": "layerfs-protocol",
    "engine/crates/layerfs-workspace": "layerfs-vfs",
    "engine/crates/layerfs-mount-ports": "layerfs-vfs",
    "control/crates/layerfs-bucket": "layerfs-store",
    "control/crates/layerfs-platform": "layerfs-machine",
    "control/crates/layerfs-sandbox-backend": "layerfs-sandbox",
    "control/crates/layerfs-control": "layerfs-inspect",
}


def packages():
    out = {}
    for base in ("core/crates", "engine/crates", "control/crates", "api/crates"):
        for d in sorted(Path(base).glob("*")):
            m = d / "Cargo.toml"
            if m.exists():
                text = m.read_text()
                name = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M).group(1)
                lib = mv.toml_lib_name(text, str(d))
                out[name] = {"dir": str(d), "lib": lib, "text": text}
    return out


PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]|include!\(\s*"([^"]+)"')


def used_names(dirpath, sub):
    """layerfs_* names in the Rust files under dirpath/sub, following #[path] and
    include! to files outside it (other crates' test support)."""
    names = set()
    todo = []
    for root, _, files in os.walk(os.path.join(dirpath, sub)):
        todo += [os.path.join(root, f) for f in files if f.endswith(".rs")]
    seen = set()
    while todo:
        f = os.path.normpath(todo.pop())
        if f in seen or not os.path.exists(f):
            continue
        seen.add(f)
        text = open(f, encoding="utf-8").read()
        for t in mv.lex(text):
            if t.k == "id" and t.v.startswith("layerfs_"):
                names.add(t.v)
        for m in PATH_ATTR.finditer(text):
            rel = m.group(1) or m.group(2)
            todo.append(os.path.join(os.path.dirname(f), rel))
    return names


def section_spans(text, header):
    """[(start, end)] of every table with this exact header line."""
    spans = []
    for m in re.finditer(r"^\[" + re.escape(header) + r"\]\s*$", text, re.M):
        nxt = re.search(r"^\[", text[m.end():], re.M)
        spans.append((m.end(), m.end() + nxt.start() if nxt else len(text)))
    return spans


DEP_LINE = re.compile(r'^(layerfs-[a-z-]+)\s*=\s*\{([^}\n]*)\}\s*\n', re.M)


def main(argv):
    check = "--check" in argv
    only = [a.rstrip("/") for a in argv if not a.startswith("--")]
    pk = packages()
    by_lib = {v["lib"]: k for k, v in pk.items()}
    for name, info in pk.items():
        d = info["dir"]
        if only and d not in only:
            continue
        text = info["text"]
        orig = text
        lib_used = used_names(d, "src") | used_names(d, "build.rs") if os.path.exists(f"{d}/src") else set()
        dev_used = set()
        for sub in ("tests", "examples", "benches"):
            dev_used |= used_names(d, sub)
        own = info["lib"]
        want_normal = {by_lib[n] for n in lib_used if n in by_lib and n != own}
        want_dev = {by_lib[n] for n in dev_used if n in by_lib and n != own} - want_normal
        for header, want in (("dependencies", want_normal), ("dev-dependencies", want_dev)):
            spans = section_spans(text, header)
            if not spans:
                if want:
                    text = text.rstrip("\n") + f"\n\n[{header}]\n"
                    spans = section_spans(text, header)
                else:
                    continue
            s, e = spans[0]
            body = text[s:e]
            have = {}
            for m in DEP_LINE.finditer(body):
                have[m.group(1)] = m
            new_body = body
            renamed = set()
            for dep, m in have.items():
                attrs = m.group(2)
                pm = re.search(r'path\s*=\s*"([^"]+)"', attrs)
                target = os.path.normpath(os.path.join(d, pm.group(1))) if pm else None
                succ = SUCCESSOR.get(target) if target else None
                if succ:
                    if succ in want and succ not in have and succ not in renamed:
                        renamed.add(succ)
                        newpath = os.path.relpath(pk[succ]["dir"], d)
                        new_attrs = re.sub(r'path\s*=\s*"[^"]+"', 'path = "' + newpath + '"', attrs)
                        line = succ + " = {" + new_attrs + "}\n"
                        new_body = new_body.replace(m.group(0), line)
                        have_add = succ
                    else:
                        new_body = new_body.replace(m.group(0), "")
                    continue
                if dep not in want and dep in pk and dep not in (want_normal | want_dev):
                    print(f"{d}/Cargo.toml: {header} {dep} is not named by the code (left as is)")
            present = set(re.findall(r"^(layerfs-[a-z-]+)\s*=", new_body, re.M))
            if header == "dev-dependencies":
                for ns, ne in section_spans(text, "dependencies"):
                    present |= set(re.findall(r"^(layerfs-[a-z-]+)\s*=", text[ns:ne], re.M))
            for dep in sorted(want - present):
                path = os.path.relpath(pk[dep]["dir"], d)
                new_body = new_body.rstrip("\n") + f'\n{dep} = {{ path = "{path}" }}\n' + ("\n" if e < len(text) else "")
            text = text[:s] + new_body + text[e:]
        if text != orig:
            print(f"{d}/Cargo.toml: updated")
            if not check:
                open(f"{d}/Cargo.toml", "w").write(text)
        for f in re.findall(r'"(layerfs-(?:bridge|mount-ports|bucket|platform|sandbox-backend|control)/[^"]+)"', text):
            print(f"{d}/Cargo.toml: feature names a removed crate: {f}")


if __name__ == "__main__":
    main(sys.argv[1:])

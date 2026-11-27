"""Respell layerfs_home paths that went through the removed #828 compat modules.

Usage (repository root): rehome.py FILE...

Rewrites, per file:
- `use layerfs_home::...` statements: each leaf of the use tree is mapped and the
  statement is re-emitted (cargo fmt lays it out);
- qualified paths `layerfs_home::a::b` in code;
- `home::X` / `helpers::X` in code when the file imported the old module `home`
  or `helpers` from layerfs_home (as `self` or by name): `home::X` becomes
  `disk::X` when X lives in disk (and `disk` is imported), else the full path.
"""

import re
import sys

ROOT = "layerfs_home"
DISK = {
    "guest", "layout", "paths", "filesystem", "filesystem_available", "Filesystem", "log_line",
    "log_swept", "now_ms", "private_dir", "read_record", "traversable", "versioned", "with_field",
    "write_entity", "write_record", "RECORD_VERSION", "detect", "ensure", "open", "opened",
    "HomeFile", "Layout", "LAYOUT_VERSION",
}
TOP = {
    "cursor": "reply::cursor", "events": "reply::events", "outcome": "reply::outcome",
    "state": "reply::state", "time": "reply::time", "logs": "log::calls", "tail": "log::tail",
    "trace": "log::trace", "op": "record::intent", "process": "record::process",
    "secrets": "secret::keys", "wipe": "secret::wipe",
}
HOME = {
    "entity_lock": "lock::entity", "flock": "lock::flock", "lock": "lock::home",
    "fields": "record::fields", "generation": "record::generation", "observed": "record::observed",
    "secrets": "secret::saved", "host_socket": "socket",
}
LINES = {"log_line", "log_swept", "SWEPT", "UNCERTAIN"}


def map_path(segs):
    """Old path segments after `layerfs_home` -> new segments (or None: unchanged)."""
    if not segs:
        return None
    head, rest = segs[0], segs[1:]
    if head in TOP:
        return TOP[head].split("::") + rest
    if head == "helpers":
        if not rest:
            raise ValueError("bare helpers module")
        if rest[0] == "lock":
            return ["lock", "mutex", "lock"] + rest[1:]
        if rest[0] in ("ordered_token", "random"):
            return ["record", "token"] + rest
        return ["reply", "values"] + rest
    if head == "home":
        if not rest:
            raise ValueError("bare home module")
        x = rest[0]
        if x in HOME:
            return HOME[x].split("::") + rest[1:]
        if x == "records":
            if len(rest) < 2:
                raise ValueError("bare home::records module")
            y = rest[1]
            return (["log", "lines"] if y in LINES else ["record", "file"]) + rest[1:]
        if x in DISK:
            return ["disk"] + rest
        raise ValueError(f"home::{x} unknown")
    return None


# ---- use trees ---------------------------------------------------------------
TOK = re.compile(r"\s*(::|\{|\}|,|\*|[A-Za-z_][A-Za-z0-9_]*)")


def parse_tree(text):
    toks = []
    pos = 0
    while pos < len(text):
        m = TOK.match(text, pos)
        if not m:
            if text[pos:].strip() == "":
                break
            raise ValueError(f"cannot parse use tree: {text!r}")
        toks.append(m.group(1))
        pos = m.end()
    leaves = []

    def walk(i, prefix):
        path = list(prefix)
        while i < len(toks):
            t = toks[i]
            if t == "{":
                i += 1
                while toks[i] != "}":
                    i = walk(i, path)
                    if toks[i] == ",":
                        i += 1
                return i + 1
            if t == "::":
                i += 1
                continue
            if t in (",", "}"):
                return i
            if t == "as":
                alias = toks[i + 1]
                leaves.append((path, alias))
                return i + 2
            path = path + [t]
            if i + 1 >= len(toks) or toks[i + 1] in (",", "}"):
                leaves.append((path, None))
                return i + 1
            i += 1
        return i

    walk(0, [])
    return leaves


USE = re.compile(r"(?m)^(?P<indent>[ \t]*)(?P<vis>pub(?:\([a-z]+\))?\s+)?use\s+(?P<tree>" + ROOT + r"::[^;]*);")


def rewrite(text):
    modules = {}  # local name -> "home" | "helpers"
    disk_needed = False

    def emit_use(m):
        nonlocal disk_needed
        leaves = parse_tree(m["tree"])
        out = []
        changed = False
        for path, alias in leaves:
            assert path[0] == ROOT, path
            segs = path[1:]
            if segs and segs[-1] == "self":
                segs = segs[:-1]
            if segs in (["home"], ["helpers"]):
                modules[alias or segs[0]] = segs[0]
                changed = True
                continue
            if segs in (["home", "records"],):
                raise ValueError("home::records imported as a module")
            new = map_path(segs)
            if new is None:
                out.append((segs, alias))
                continue
            changed = True
            name = alias or segs[-1]
            out.append((new, None if new[-1] == name else name))
        if not changed:
            return m.group(0)
        if not out:
            return "\x00DROP\x00"
        items = ", ".join("::".join(s) + (f" as {a}" if a else "") for s, a in out)
        if len(out) == 1:
            return f"{m['indent']}{m['vis'] or ''}use {ROOT}::{items};"
        return f"{m['indent']}{m['vis'] or ''}use {ROOT}::{{{items}}};"

    text = USE.sub(emit_use, text)
    text = re.sub(r"(?m)^[ \t]*\x00DROP\x00\n?", "", text)

    # qualified paths in code: layerfs_home::a::b...
    def qual(m):
        segs = m.group(1).split("::")
        # stop at the first segment that is not a path segment we know
        for cut in range(len(segs), 0, -1):
            try:
                new = map_path(segs[:cut])
            except ValueError:
                continue
            if new is not None:
                return ROOT + "::" + "::".join(new + segs[cut:])
        return m.group(0)

    text = re.sub(ROOT + r"::((?:[A-Za-z_][A-Za-z0-9_]*)(?:::[A-Za-z_][A-Za-z0-9_]*)*)", qual, text)

    # old module bindings used in code: home::X / helpers::X
    for local, old in modules.items():
        def local_path(m):
            nonlocal disk_needed
            segs = [old] + m.group(1).split("::")
            for cut in range(len(segs), 1, -1):
                try:
                    new = map_path(segs[:cut])
                except ValueError:
                    continue
                if new is not None:
                    full = new + segs[cut:]
                    if old == "home" and full[0] == "disk":
                        disk_needed = True
                        return "::".join(full)
                    return ROOT + "::" + "::".join(full)
            raise ValueError(f"{local}::{m.group(1)} not mapped")

        text = re.sub(r"(?<![A-Za-z0-9_:])" + local + r"::((?:[A-Za-z_][A-Za-z0-9_]*)(?:::[A-Za-z_][A-Za-z0-9_]*)*)", local_path, text)
    if disk_needed and not re.search(r"use " + ROOT + r"::disk;|" + ROOT + r"::\{[^}]*\bdisk\b", text):
        m = USE.search(text) or re.search(r"(?m)^use ", text)
        at = m.start() if m else 0
        indent = m["indent"] if m and "indent" in m.groupdict() else ""
        text = text[:at] + f"{indent}use {ROOT}::disk;\n" + text[at:]
    return text


def main(files):
    bad = 0
    for f in files:
        src = open(f).read()
        try:
            new = rewrite(src)
        except (ValueError, IndexError, AssertionError) as e:
            print(f"{f}: {e}")
            bad += 1
            continue
        if new != src:
            open(f, "w").write(new)
            print(f"{f}: rewritten")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

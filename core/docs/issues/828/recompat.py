"""Respell paths that went through removed `#828 compat` re-exports, any crate.

Usage (repository root): recompat.py COMPAT_MAP.tsv FILE...

COMPAT_MAP.tsv is drop_compat.py's report: <file>\t<compat module or ''>\t<body>.
For every lib crate it rebuilds what each compat item meant:
  explicit: old path -> new path      (`pub use P as N;`, `pub use P::{a, b};`)
  glob:     old module -> [modules]   (`pub use P::*;` inside a compat module, or
                                       at a file's own module level)
and respells, per file, use statements, qualified paths and module bindings
(`use krate::old;` / `{self}` then `old::X` in code). A glob name is resolved
against the `pub` items of each target module's file.
"""

import os
import re
import sys
from functools import lru_cache

sys.path.insert(0, os.path.dirname(__file__))
from rehome import parse_tree  # noqa: E402

IDENT = r"[A-Za-z_][A-Za-z0-9_]*"


def crate_roots():
    roots = {}
    for base in ("core/crates", "engine/crates", "control/crates", "api/crates"):
        for d in sorted(os.listdir(base)):
            if os.path.exists(f"{base}/{d}/src/lib.rs"):
                roots[d.replace("-", "_")] = f"{base}/{d}/src"
    return roots


ROOTS = crate_roots()


def module_of(file):
    for lib, src in ROOTS.items():
        if file.startswith(src + "/"):
            rel = file[len(src) + 1 : -3].split("/")
            if rel[-1] in ("lib", "mod"):
                rel = rel[:-1]
            return lib, rel
    return None, None


def resolve(lib, here, path):
    """A use path written in module `here` of crate `lib` -> absolute segments."""
    segs = path.split("::")
    if segs[0] == "crate":
        return segs[1:]
    out = list(here)
    while segs and segs[0] in ("self", "super"):
        if segs[0] == "super":
            out = out[:-1]
        segs = segs[1:]
    return out + segs


EXPLICIT = {}  # (lib, tuple old) -> list new
GLOBS = {}  # (lib, tuple old module) -> [tuple target module]


def load(tsv):
    for line in open(tsv):
        file, mod, body = line.rstrip("\n").split("\t", 2)
        lib, here = module_of(file)
        if lib is None:
            continue
        body = body.replace("\\n", "\n")
        scope = tuple(here + ([mod] if mod else []))
        stack = [scope]
        for raw in body.split("\n"):
            s = raw.strip()
            m = re.match(r"pub mod (" + IDENT + r") \{", s)
            if m and s != f"pub mod {mod} {{":
                stack.append(stack[-1] + (m.group(1),))
                continue
            if s == "}" and len(stack) > 1:
                stack.pop()
                continue
            m = re.match(r"pub use ([^;]+);", s)
            if not m:
                continue
            cur = stack[-1]
            for leaf, alias in parse_tree(m.group(1)):
                if leaf[-1] == "*":
                    GLOBS.setdefault((lib, cur), []).append(tuple(resolve(lib, list(here), "::".join(leaf[:-1]))))
                    continue
                target = resolve(lib, list(here), "::".join(leaf if leaf[-1] != "self" else leaf[:-1]))
                name = alias or target[-1]
                EXPLICIT[(lib, cur + (name,))] = target


@lru_cache(None)
def module_file(lib, mod):
    src = ROOTS[lib]
    if not mod:
        return f"{src}/lib.rs"
    for cand in (f"{src}/{'/'.join(mod)}.rs", f"{src}/{'/'.join(mod)}/mod.rs"):
        if os.path.exists(cand):
            return cand
    return None


@lru_cache(None)
def defines(lib, mod, name):
    f = module_file(lib, mod)
    if f is None:
        # an item path (a type), or an inline module
        return False
    text = open(f).read()
    pat = (
        r"(?m)^\s*pub\s+(?:(?:async|unsafe|const|extern(?:\s+\"C\")?)\s+)*(?:fn|struct|enum|const|static|type|trait|mod|union)\s+"
        + re.escape(name)
        + r"\b"
    )
    if re.search(pat, text):
        return True
    for m in re.finditer(r"(?m)^\s*pub\s+use\s+([^;]+);", text):
        for leaf, alias in parse_tree(m.group(1)):
            if (alias or leaf[-1]) == name:
                return True
            if leaf[-1] == "*":
                sub = resolve(lib, list(mod), "::".join(leaf[:-1]))
                if sub and sub[0] in ROOTS:
                    continue
                if defines(lib, tuple(sub), name):
                    return True
    return False


def map_path(lib, segs):
    """Absolute old segments in crate lib -> new segments, or None."""
    segs = tuple(segs)
    for cut in range(len(segs), 0, -1):
        key = (lib, segs[:cut])
        if key in EXPLICIT:
            new = EXPLICIT[key] + list(segs[cut:])
            again = map_path(lib, new)
            return again if again is not None else new
        if cut < len(segs) and key in GLOBS:
            name = segs[cut]
            if module_file(lib, segs[:cut]) and (defines(lib, segs[:cut], name) or module_file(lib, segs[: cut + 1])):
                return None
            hits = [t for t in GLOBS[key] if defines(lib, t, name)]
            if hits:
                new = list(hits[0]) + list(segs[cut:])
                again = map_path(lib, new)
                return again if again is not None else new
            raise ValueError(f"{lib}::{'::'.join(segs)}: {name} not found in {GLOBS[key]}")
    return None


def is_old_module(lib, segs):
    key = (lib, tuple(segs))
    return key in GLOBS and key not in EXPLICIT and module_file(lib, tuple(segs)) is None


USE = re.compile(r"(?m)^(?P<indent>[ \t]*)(?P<vis>pub(?:\([a-z]+\))?\s+)?use\s+(?P<tree>(?P<root>" + IDENT + r")::[^;]*);")


def rewrite(text, here_lib, here_mod):
    bindings = {}  # local name -> (lib, old module segs)

    def root_lib(root):
        if root in ROOTS:
            return root, []
        if root == "crate" and here_lib:
            return here_lib, None
        return None, None

    def emit_use(m):
        lib, _ = root_lib(m["root"])
        if lib is None:
            return m.group(0)
        leaves = parse_tree(m["tree"])
        out = []
        changed = False
        for path, alias in leaves:
            segs = path[1:]
            selfish = segs and segs[-1] == "self"
            if selfish:
                segs = segs[:-1]
            if is_old_module(lib, segs):
                bindings[alias or segs[-1]] = (lib, segs)
                changed = True
                continue
            try:
                new = map_path(lib, segs)
            except ValueError:
                new = None
            if new is None:
                out.append((segs + (["self"] if selfish else []), alias))
                continue
            changed = True
            name = alias or segs[-1]
            out.append((new, None if new[-1] == name else name))
        if not changed:
            return m.group(0)
        if not out:
            return "\x00DROP\x00"
        groups = {}
        for s, a in out:
            if s[-1] == "self":
                groups.setdefault(tuple(s[:-1]), []).append("self" + (f" as {a}" if a else ""))
            else:
                groups.setdefault(tuple(s[:-1]), []).append(s[-1] + (f" as {a}" if a else ""))
        lines = []
        for mod, items in groups.items():
            items = sorted(set(items), key=lambda x: (not x.startswith("self"), x.lower()))
            base = "::".join((m["root"],) + mod)
            body = items[0] if len(items) == 1 and not items[0].startswith("self") else "{" + ", ".join(items) + "}"
            if items == ["self"]:
                lines.append(f"{m['indent']}{m['vis'] or ''}use {base};")
            else:
                lines.append(f"{m['indent']}{m['vis'] or ''}use {base}::{body};")
        return "\n".join(lines)

    text = USE.sub(emit_use, text)
    text = re.sub(r"(?m)^[ \t]*\x00DROP\x00\n?", "", text)

    def qual(m):
        root = m.group(1)
        lib, _ = root_lib(root)
        if lib is None:
            return m.group(0)
        segs = m.group(2).split("::")
        for cut in range(len(segs), 0, -1):
            try:
                new = map_path(lib, segs[:cut])
            except ValueError:
                continue
            if new is not None:
                return root + "::" + "::".join(new + segs[cut:])
        return m.group(0)

    roots = "|".join(sorted(ROOTS)) + "|crate"
    text = re.sub(r"(?<![A-Za-z0-9_:])(" + roots + r")::((?:" + IDENT + r")(?:::" + IDENT + r")*)", qual, text)

    for local, (lib, old) in bindings.items():
        prefix = lib if lib != here_lib else "crate"

        def local_path(m):
            segs = list(old) + m.group(1).split("::")
            for cut in range(len(segs), len(old), -1):
                try:
                    new = map_path(lib, segs[:cut])
                except ValueError:
                    continue
                if new is not None:
                    return prefix + "::" + "::".join(new + segs[cut:])
            raise ValueError(f"{local}::{m.group(1)} not mapped")

        text = re.sub(r"(?<![A-Za-z0-9_:.])" + local + r"::((?:" + IDENT + r")(?:::" + IDENT + r")*)", local_path, text)
    return text


def main(argv):
    load(argv[0])
    bad = 0
    for f in argv[1:]:
        src = open(f).read()
        lib, mod = module_of(f)
        if lib is None:
            # a test, example or bench: `crate` is its own binary
            lib = None
        try:
            new = rewrite(src, lib, mod)
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

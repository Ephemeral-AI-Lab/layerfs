"""Regroup `use layerfs_home::{a::x, a::y, b::z as w};` into one statement per module."""
import re, sys
sys.path.insert(0, __import__("os").path.dirname(__file__))
from rehome import parse_tree
USE = re.compile(r"(?m)^(?P<indent>[ \t]*)(?P<vis>pub(?:\([a-z]+\))?\s+)?use\s+(?P<tree>layerfs_home::\{[^;]*\});\n")
for f in sys.argv[1:]:
    text = open(f).read()
    def sub(m):
        leaves = parse_tree(m["tree"])
        groups = {}
        for path, alias in leaves:
            segs = path[1:]
            if segs[-1] == "self":
                mod, leaf = tuple(segs[:-1]), "self"
            else:
                mod, leaf = tuple(segs[:-1]), segs[-1]
            groups.setdefault(mod, []).append(leaf + (f" as {alias}" if alias else ""))
        out = []
        for mod, items in groups.items():
            base = "::".join(("layerfs_home",) + mod)
            if mod and mod[-1] == "disk" and "self" not in items and re.search(r"(?<![A-Za-z0-9_:])disk::", text):
                items = ["self"] + items
            items = sorted(set(items), key=lambda s: (s != "self", s.lower()))
            body = items[0] if len(items) == 1 and items[0] != "self" else "{" + ", ".join(items) + "}"
            if items == ["self"]:
                out.append(f"{m['indent']}{m['vis'] or ''}use {base};\n")
            else:
                out.append(f"{m['indent']}{m['vis'] or ''}use {base}::{body};\n")
        return "".join(out)
    new = USE.sub(sub, text)
    # a separately added `use layerfs_home::disk;` is redundant once a group binds `self`
    if re.search(r"use layerfs_home::disk::\{self", new):
        new = re.sub(r"(?m)^[ \t]*use layerfs_home::disk;\n", "", new)
    if new != text:
        open(f, "w").write(new)
        print(f)

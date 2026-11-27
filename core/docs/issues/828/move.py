#!/usr/bin/env python3
"""#828 move tool: apply mapping rows to the tree and rewrite what the moves break.

Usage (repository root):
  move.py apply  --crate DIR [--crate DIR...] [--cross] [--rewrite all|selected]
                 [--compat] [--dry-run] [--report FILE]
  move.py callers [--crate DIR...]      respell paths that pass through `#828 compat`
                                         re-exports, workspace-wide (wave 3)
  move.py status --crate DIR            pending rows for a crate

`apply` selects the pending rows (old file present) whose old or new path lies in
a selected crate directory. `--cross` selects instead every row whose old and new
crate differ (wave 1). For the selected rows it:

- runs `git mv` (move, rename, the first file of a split or merge), appends the
  other merge sources to the merge target (their `//!` become `//`), appends a
  removed mod.rs's non-declaration content to the module that receives its
  children, and creates the mod.rs / test main.rs of new folders;
- regenerates `mod` declarations from the new tree, keeping attributes, docs
  and visibility (a module that changes parent and was private becomes
  `pub(crate)`);
- rewrites module paths in Rust source (`crate::`, `super::`, `self::`, bare
  child names, extern crate names, `use` trees, `pub(in ...)`, `$crate`),
  keeping a spelling that still resolves and respelling only broken ones;
- recomputes `#[path]`, `include!`/`include_str!`/`include_bytes!` literals and
  `concat!(env!("CARGO_MANIFEST_DIR"), "...")` literals, and `[[test]]`/`[[bin]]`
  entries (a standalone test gets a `[[test]]` entry);
- with `--compat`, adds `#828 compat` re-exports so every old public module path
  of the selected lib crates keeps resolving;
- prints the hand work left (split targets, new files, refusals).

Nothing here is product code. Rust is read with a small tokenizer, not rustc: a
path the tool cannot resolve is left as written and the compiler reports it.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

PLAN = Path(__file__).resolve().parent
LAYERS = ("core", "engine", "control", "api")
MARK = "#828 compat"
KEYWORDS = {
    "as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "true",
    "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "Self", "mod", "union",
}

# --------------------------------------------------------------------------- lexer


class Tok:
    __slots__ = ("k", "v", "s", "e")

    def __init__(self, k, v, s, e):
        self.k, self.v, self.s, self.e = k, v, s, e

    def __repr__(self):
        return f"{self.k}:{self.v!r}@{self.s}"


ID_START = re.compile(r"[A-Za-z_]")
ID_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUM_RE = re.compile(r"[0-9][0-9A-Za-z_]*(\.[0-9][0-9A-Za-z_]*)?([eE][+-]?[0-9_]+)?")


def lex(text):
    toks = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            toks.append(Tok("com", text[i:j], i, j))
            i = j
            continue
        if text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            toks.append(Tok("com", text[i:j], i, j))
            i = j
            continue
        m = re.match(r'(b|c)?r(#*)"', text[i:i + 20])
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + m.group(2)
            j = text.find(close, i + m.end())
            j = n if j < 0 else j + len(close)
            toks.append(Tok("str", text[i:j], i, j))
            i = j
            continue
        if c == '"' or (c in "bc" and i + 1 < n and text[i + 1] == '"'
                        and not (i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_"))):
            j = i + (2 if c != '"' else 1)
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j += 1
            toks.append(Tok("str", text[i:j], i, j))
            i = j
            continue
        if c == "b" and i + 1 < n and text[i + 1] == "'" and not (i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_")):
            j = i + 2
            while j < n and text[j] != "'":
                j += 2 if text[j] == "\\" else 1
            toks.append(Tok("char", text[i:j + 1], i, j + 1))
            i = j + 1
            continue
        if c == "'":
            if i + 1 < n and text[i + 1] == "\\":
                j = i + 2
                while j < n and text[j] != "'":
                    j += 2 if text[j] == "\\" else 1
                toks.append(Tok("char", text[i:j + 1], i, j + 1))
                i = j + 1
                continue
            if i + 2 < n and text[i + 2] == "'":
                toks.append(Tok("char", text[i:i + 3], i, i + 3))
                i += 3
                continue
            m = ID_RE.match(text, i + 1)
            j = m.end() if m else i + 1
            toks.append(Tok("life", text[i:j], i, j))
            i = j
            continue
        if text.startswith("r#", i) and i + 2 < n and ID_START.match(text[i + 2]):
            m = ID_RE.match(text, i + 2)
            toks.append(Tok("id", text[i:m.end()], i, m.end()))
            i = m.end()
            continue
        if ID_START.match(c):
            m = ID_RE.match(text, i)
            toks.append(Tok("id", m.group(), i, m.end()))
            i = m.end()
            continue
        if c.isdigit():
            m = NUM_RE.match(text, i)
            toks.append(Tok("num", m.group(), i, m.end()))
            i = m.end()
            continue
        if text.startswith("::", i):
            toks.append(Tok("p", "::", i, i + 2))
            i += 2
            continue
        toks.append(Tok("p", c, i, i + 1))
        i += 1
    return toks


# ------------------------------------------------------------------ file parsing


class Decl:
    """A `mod name;` or `mod name { ... }` declaration."""

    def __init__(self):
        self.name = None
        self.inline = False
        self.span = None  # (start, end) of the whole declaration incl. attrs/docs
        self.vis = ""
        self.attrs = []  # attribute texts (not #[path])
        self.docs = []  # doc comment texts
        self.path_attr = None  # (value, str token)
        self.scope = ()  # inline chain where declared
        self.body = None  # (open index, close index) for inline modules
        self.compat = False


class UseLeaf:
    def __init__(self, segs, alias, glob, toks):
        self.segs, self.alias, self.glob, self.toks = segs, alias, glob, toks

    def local(self):
        if self.glob:
            return None
        if self.alias:
            return self.alias
        return self.segs[-1] if self.segs else None


class Use:
    def __init__(self):
        self.vis = ""
        self.span = None  # tree span (start offset, end offset) between `use` and `;`
        self.leaves = []
        self.scope = ()
        self.compat = False
        self.leading = False  # tree starts with `::`


class Occ:
    """A path occurrence outside `use`: token indexes of its segments."""

    def __init__(self, segs, toks, scope, dollar=False, leading=False):
        self.segs, self.toks, self.scope, self.dollar, self.leading = segs, toks, scope, dollar, leading


class Include:
    def __init__(self, kind, tok, scope, value):
        self.kind, self.tok, self.scope, self.value = kind, tok, scope, value


class Parsed:
    def __init__(self, path, text):
        self.path = path
        self.text = text
        self.toks = lex(text)
        self.sig = [i for i, t in enumerate(self.toks) if t.k != "com"]
        self.decls = []
        self.uses = []
        self.occs = []
        self.includes = []
        self.items = defaultdict(set)  # scope -> names defined at module level
        self.inner_attr_end = 0  # offset after leading //! docs and #![...] attrs
        self.problems = []
        self._parse()

    def _parse(self):
        toks, sig = self.toks, self.sig
        S = [toks[i] for i in sig]
        n = len(S)

        def match_close(k, open_, close):
            depth = 0
            while k < n:
                if S[k].v == open_ and S[k].k == "p":
                    depth += 1
                elif S[k].v == close and S[k].k == "p":
                    depth -= 1
                    if depth == 0:
                        return k
                k += 1
            return n - 1

        # Leading inner docs and attributes.
        k = 0
        end = 0
        for t in toks:
            if t.k == "com" and (t.v.startswith("//!") or t.v.startswith("/*!")):
                end = t.e
                continue
            if t.k == "com":
                continue
            break
        while k < n and S[k].v == "#" and k + 1 < n and S[k + 1].v == "!" and k + 2 < n and S[k + 2].v == "[":
            c = match_close(k + 2, "[", "]")
            end = S[c].e
            k = c + 1
        # docs after attrs
        for t in toks:
            if t.s >= end and t.k == "com" and t.v.startswith("//!"):
                end = t.e
            elif t.s >= end and t.k != "com":
                break
        self.inner_attr_end = end

        scope_stack = []  # (name, close index)
        depth_stack = []  # brace depth markers for inline mods
        brace = 0
        mod_at_depth = {}
        item_depth = 0
        k = 0
        # Track block depth at which module-level items live: inline mod bodies reset it.
        level_stack = [0]

        def scope():
            return tuple(s for s, _ in scope_stack)

        def attrs_before(k0):
            """Walk back from sig index k0 over attributes, visibility excluded."""
            j = k0
            start = S[k0].s
            attr_texts = []
            path_attr = None
            while True:
                if j - 1 >= 0 and S[j - 1].v == "]":
                    # find the matching '['
                    d, m = 0, j - 1
                    while m >= 0:
                        if S[m].v == "]":
                            d += 1
                        elif S[m].v == "[":
                            d -= 1
                            if d == 0:
                                break
                        m -= 1
                    if m - 1 >= 0 and S[m - 1].v == "#":
                        text = self.text[S[m - 1].s:S[j - 1].e]
                        pm = re.match(r'#\[\s*path\s*=\s*("(?:[^"\\]|\\.)*")\s*\]$', text)
                        if pm:
                            path_attr = (json.loads(pm.group(1)), sig[m + 3] if m + 3 < len(sig) else None)
                            # token index of the string literal
                            for q in range(m, j):
                                if S[q].k == "str":
                                    path_attr = (json.loads(S[q].v), sig[q])
                        else:
                            attr_texts.insert(0, text)
                        start = S[m - 1].s
                        j = m - 1
                        continue
                break
            # doc comments directly above
            ti = sig[j] - 1 if j < len(sig) else len(toks) - 1
            docs = []
            while ti >= 0 and toks[ti].k == "com" and (toks[ti].v.startswith("///") or toks[ti].v.startswith("/**")):
                docs.insert(0, toks[ti].v)
                start = toks[ti].s
                ti -= 1
            return start, attr_texts, path_attr, docs

        while k < n:
            t = S[k]
            if t.k == "p" and t.v == "{":
                brace += 1
                k += 1
                continue
            if t.k == "p" and t.v == "}":
                if scope_stack and scope_stack[-1][1] == k:
                    scope_stack.pop()
                    level_stack.pop()
                brace -= 1
                k += 1
                continue
            at_level = brace == level_stack[-1]
            # visibility
            vis_start = k
            vis = ""
            if t.k == "id" and t.v == "pub":
                if k + 1 < n and S[k + 1].v == "(":
                    c = match_close(k + 1, "(", ")")
                    vis = self.text[t.s:S[c].e]
                    k2 = c + 1
                else:
                    vis = "pub"
                    k2 = k + 1
            else:
                k2 = k
            if k2 < n and S[k2].k == "id" and S[k2].v == "mod" and k2 + 1 < n and S[k2 + 1].k == "id":
                d = Decl()
                d.name = S[k2 + 1].v
                d.vis = vis
                d.scope = scope()
                start, attrs, path_attr, docs = attrs_before(vis_start)
                d.attrs, d.path_attr, d.docs = attrs, path_attr, docs
                if k2 + 2 < n and S[k2 + 2].v == ";":
                    d.span = (start, S[k2 + 2].e)
                    line = self.text[S[k2 + 2].e:self.text.find("\n", S[k2 + 2].e) if self.text.find("\n", S[k2 + 2].e) >= 0 else len(self.text)]
                    d.compat = MARK in line
                    self.decls.append(d)
                    if at_level:
                        self.items[scope()].add(d.name)
                    k = k2 + 3
                    continue
                if k2 + 2 < n and S[k2 + 2].v == "{":
                    c = match_close(k2 + 2, "{", "}")
                    d.inline = True
                    d.span = (start, S[c].e)
                    d.body = (k2 + 2, c)
                    head = self.text[start:S[k2 + 2].e]
                    d.compat = MARK in head or MARK in self.text[S[k2 + 2].e:self.text.find("\n", S[k2 + 2].e)]
                    self.decls.append(d)
                    if at_level:
                        self.items[scope()].add(d.name)
                    scope_stack.append((d.name, c))
                    brace += 1
                    level_stack.append(brace)
                    k = k2 + 3
                    continue
            if k2 < n and S[k2].k == "id" and S[k2].v == "use" and not (k2 + 1 < n and S[k2 + 1].v == "<"):
                u = Use()
                u.vis = vis
                u.scope = scope()
                j = k2 + 1
                semi = j
                d = 0
                while semi < n:
                    if S[semi].v in "{(":
                        d += 1
                    elif S[semi].v in "})":
                        d -= 1
                    elif S[semi].v == ";" and d == 0:
                        break
                    semi += 1
                if semi >= n:
                    k = k2 + 1
                    continue
                u.span = (S[j].s, S[semi - 1].e)
                u.stmt = (attrs_before(vis_start)[0], S[semi].e)
                linestart = self.text.rfind("\n", 0, S[vis_start].s)
                prev_line_start = self.text.rfind("\n", 0, max(linestart, 0))
                lineend = self.text.find("\n", S[semi].e)
                around = self.text[max(prev_line_start, 0):lineend if lineend >= 0 else len(self.text)]
                u.compat = MARK in around
                if S[j].v == "::":
                    u.leading = True
                    j += 1
                try:
                    u.leaves = self._use_top(S, sig, j, semi)
                except Exception as exc:  # pragma: no cover - reported
                    self.problems.append(f"{self.path}: unparsed use at {S[k2].s}: {exc}")
                    u.leaves = []
                self.uses.append(u)
                k = semi + 1
                continue
            if at_level and t.k == "id" and t.v in ("fn", "struct", "enum", "trait", "type", "const", "static", "union") \
                    and k + 1 < n and S[k + 1].k == "id":
                self.items[scope()].add(S[k + 1].v)
            if at_level and t.k == "id" and t.v == "macro_rules" and k + 2 < n and S[k + 1].v == "!":
                self.items[scope()].add(S[k + 2].v)
            # include!-family literals
            if t.k == "id" and t.v in ("include", "include_str", "include_bytes") and k + 3 < n \
                    and S[k + 1].v == "!" and S[k + 2].v == "(" and S[k + 3].k == "str" and S[k + 3].v.startswith('"'):
                self.includes.append(Include(t.v, sig[k + 3], scope(), json.loads(S[k + 3].v)))
            if t.k == "id" and t.v == "env" and k + 4 < n and S[k + 1].v == "!" and S[k + 2].v == "(" \
                    and S[k + 3].v == '"CARGO_MANIFEST_DIR"' and S[k + 4].v == ")":
                if k + 6 < n and S[k + 5].v == "," and S[k + 6].k == "str" and S[k + 6].v.startswith('"'):
                    self.includes.append(Include("manifest", sig[k + 6], scope(), json.loads(S[k + 6].v)))
            # path occurrences
            if self._path_start(S, k):
                segs, idxs, kk, dollar, leading = self._read_path(S, sig, k)
                if len(segs) >= 2 or (len(segs) == 1 and segs[0] in ("crate", "super", "self")):
                    self.occs.append(Occ(segs, idxs, scope(), dollar, leading))
                k = max(kk, k + 1)
                continue
            k += 1

    def _path_start(self, S, k):
        t = S[k]
        prev = S[k - 1] if k > 0 else None
        if t.k == "p" and t.v == "$" and k + 2 < len(S) and S[k + 1].v == "crate" and S[k + 2].v == "::":
            return True
        if t.k == "p" and t.v == "::":
            if prev is not None and (prev.k == "id" or prev.v in (">", ")", "]")):
                return False
            return k + 1 < len(S) and S[k + 1].k == "id"
        if t.k != "id" or t.v in KEYWORDS and t.v not in ("crate", "self", "super"):
            return False
        if prev is not None and (prev.v == "::" or prev.v == "." or (prev.v == "$" and t.v == "crate")):
            return False
        if prev is not None and prev.k == "id" and prev.v in ("struct", "enum", "trait", "fn", "mod", "type", "union", "macro_rules"):
            return False
        return k + 1 < len(S) and S[k + 1].v == "::"

    def _read_path(self, S, sig, k):
        segs, idxs = [], []
        dollar = leading = False
        if S[k].v == "$":
            dollar = True
            k += 1
        elif S[k].v == "::":
            leading = True
            k += 1
        while k < len(S) and S[k].k == "id":
            segs.append(S[k].v)
            idxs.append(sig[k])
            if k + 2 < len(S) and S[k + 1].v == "::" and S[k + 2].k == "id":
                k += 2
                continue
            k += 1
            break
        return segs, idxs, k, dollar, leading

    def _use_tree(self, S, sig, j, end, prefix, prefix_toks):
        """Parse use tree tokens S[j:end] into leaves."""
        leaves = []
        segs, toks = list(prefix), list(prefix_toks)
        while j < end:
            t = S[j]
            if t.k == "id" and t.v != "as":
                segs.append(t.v)
                toks.append(sig[j])
                j += 1
                if j < end and S[j].v == "::":
                    j += 1
                    continue
                alias = None
                if j < end and S[j].v == "as":
                    alias = S[j + 1].v
                    j += 2
                if segs and segs[-1] == "self" and len(segs) > 1:
                    leaves.append(UseLeaf(segs[:-1], alias, False, toks[:-1]))
                else:
                    leaves.append(UseLeaf(segs, alias, False, toks))
                return leaves, j
            if t.v == "*":
                leaves.append(UseLeaf(segs, None, True, toks))
                return leaves, j + 1
            if t.v == "{":
                j += 1
                while j < end and S[j].v != "}":
                    sub, j = self._use_tree(S, sig, j, end, segs, toks)
                    leaves.extend(sub)
                    if j < end and S[j].v == ",":
                        j += 1
                return leaves, j + 1
            raise ValueError(f"token {t}")
        return leaves, j

    def _use_top(self, S, sig, j, end):
        leaves = []
        while j < end:
            sub, j = self._use_tree(S, sig, j, end, [], [])
            leaves.extend(sub)
            if j < end and S[j].v == ",":
                j += 1
        return leaves


# ------------------------------------------------------------------ workspace


def git(*args, check=True):
    return subprocess.run(["git", *args], check=check, capture_output=True, text=True).stdout


def crate_of(path, crate_dirs):
    parts = path.split("/")
    for k in range(len(parts) - 1, 0, -1):
        d = "/".join(parts[:k])
        if d in crate_dirs:
            return d
    return None


def toml_lib_name(text, crate_dir):
    m = re.search(r"^\[lib\][^\[]*?^name\s*=\s*\"([^\"]+)\"", text, re.M | re.S)
    if m:
        return m.group(1)
    m = re.search(r"^\[package\][^\[]*?^name\s*=\s*\"([^\"]+)\"", text, re.M | re.S)
    name = m.group(1) if m else crate_dir.rsplit("/", 1)[1]
    return name.replace("-", "_")


def toml_targets(text):
    """[[test]]/[[bin]]/[[example]]/[[bench]] tables as dicts with their spans."""
    out = []
    for m in re.finditer(r"^\[\[(test|bin|example|bench)\]\]\s*$", text, re.M):
        start = m.start()
        nxt = re.search(r"^\[", text[m.end():], re.M)
        end = m.end() + nxt.start() if nxt else len(text)
        body = text[m.end():end]
        fields = dict(re.findall(r'^(\w[\w-]*)\s*=\s*"?([^"\n]*)"?\s*$', body, re.M))
        out.append({"kind": m.group(1), "span": (start, end), "fields": fields, "body": body})
    return out


class Target:
    def __init__(self, tid, crate, kind, name, root):
        self.tid, self.crate, self.kind, self.name, self.root = tid, crate, kind, name, root


class World:
    """Packages, targets and module trees of one file set."""

    def __init__(self, files, read, crate_dirs, lib_names, cargo_texts):
        self.files = set(files)
        self.read = read
        self.crate_dirs = crate_dirs
        self.lib_names = lib_names  # crate dir -> lib name (only crates with src/lib.rs)
        self.cargo = cargo_texts  # crate dir -> Cargo.toml text
        self.targets = {}
        self.parsed = {}
        self.mods = {}  # (tid, path) -> (file, inline chain)
        self.children = defaultdict(dict)  # (tid, path) -> name -> (tid, path)
        self.decl_of = {}  # (tid, path) -> (parent file, Decl)
        self.incl = defaultdict(list)  # file -> [(tid, path)]
        self.kind = {}  # (tid, path) file inclusion kind: root|conv|path
        self.extern = {}  # lib name -> lib tid
        self._targets()
        for t in self.targets.values():
            self._walk(t, t.root, ())

    def parse(self, f):
        if f not in self.parsed:
            self.parsed[f] = Parsed(f, self.read(f))
        return self.parsed[f]

    def _targets(self):
        for crate in sorted(self.crate_dirs):
            text = self.cargo.get(crate, "")
            explicit = toml_targets(text)
            seen_roots = set()
            for e in explicit:
                p = e["fields"].get("path")
                if p:
                    root = f"{crate}/{p}"
                    name = e["fields"].get("name") or Path(p).stem
                    self._add(crate, e["kind"], name, root)
                    seen_roots.add(root)
            if f"{crate}/src/lib.rs" in self.files:
                t = self._add(crate, "lib", self.lib_names.get(crate) or crate.rsplit("/", 1)[1].replace("-", "_"),
                              f"{crate}/src/lib.rs")
                self.extern[t.name] = t.tid
            if f"{crate}/src/main.rs" in self.files and f"{crate}/src/main.rs" not in seen_roots:
                self._add(crate, "bin", crate.rsplit("/", 1)[1], f"{crate}/src/main.rs")
            auto = {"bin": "src/bin", "test": "tests", "example": "examples", "bench": "benches"}
            for kind, sub in auto.items():
                base = f"{crate}/{sub}/"
                for f in self.files:
                    if not f.startswith(base) or not f.endswith(".rs"):
                        continue
                    rest = f[len(base):]
                    if "/" not in rest:
                        root, name = f, rest[:-3]
                    elif rest.count("/") == 1 and rest.endswith("/main.rs"):
                        root, name = f, rest.split("/")[0]
                    else:
                        continue
                    if root in seen_roots:
                        continue
                    self._add(crate, kind, name, root)

    def _add(self, crate, kind, name, root):
        tid = f"{crate}::{kind}::{name}"
        t = Target(tid, crate, kind, name, root)
        self.targets[tid] = t
        return t

    def _walk(self, t, f, path, inline=()):
        if f not in self.files:
            return
        key = (t.tid, path)
        if inline == ():
            self.mods[key] = (f, ())
            self.incl[f].append(key)
        p = self.parse(f)
        is_modrs = f == t.root or f.endswith("/mod.rs") or f.endswith("/main.rs") and f == t.root
        base_dir = os.path.dirname(f)
        own_dir = base_dir if (f == t.root or f.endswith("/mod.rs")) else f[:-3]
        for d in p.decls:
            if d.scope != inline:
                continue
            ckey = (t.tid, path + (d.name,))
            self.children[key][d.name] = ckey
            self.decl_of[ckey] = (f, d)
            if d.inline:
                self.mods[ckey] = (f, inline + (d.name,))
                self._walk_inline(t, f, path + (d.name,), inline + (d.name,), own_dir)
                continue
            if d.path_attr:
                child = os.path.normpath(os.path.join(base_dir, d.path_attr[0]))
                self.kind[ckey] = "path"
            else:
                sub = own_dir
                cands = [f"{sub}/{d.name}.rs", f"{sub}/{d.name}/mod.rs"]
                child = next((c for c in cands if c in self.files), None)
                self.kind[ckey] = "conv"
            if child and child in self.files:
                self._walk(t, child, path + (d.name,))

    def _walk_inline(self, t, f, path, inline, own_dir):
        key = (t.tid, path)
        p = self.parse(f)
        for d in p.decls:
            if d.scope != inline:
                continue
            ckey = (t.tid, path + (d.name,))
            self.children[key][d.name] = ckey
            self.decl_of[ckey] = (f, d)
            if d.inline:
                self.mods[ckey] = (f, inline + (d.name,))
                self._walk_inline(t, f, path + (d.name,), inline + (d.name,), own_dir + "/" + d.name)
            elif d.path_attr:
                child = os.path.normpath(os.path.join(own_dir + "/" + "/".join(inline), d.path_attr[0]))
                self.kind[ckey] = "path"
                self._walk(t, child, path + (d.name,))
            else:
                sub = own_dir + "/" + "/".join(inline)
                cands = [f"{sub}/{d.name}.rs", f"{sub}/{d.name}/mod.rs"]
                child = next((c for c in cands if c in self.files), None)
                self.kind[ckey] = "conv"
                if child:
                    self._walk(t, child, path + (d.name,))

    # -- module info -------------------------------------------------------
    def uses_in(self, key):
        f, inline = self.mods.get(key, (None, ()))
        if f is None:
            return []
        return [u for u in self.parse(f).uses if u.scope == inline]

    def items_in(self, key):
        f, inline = self.mods.get(key, (None, ()))
        if f is None:
            return set()
        return self.parse(f).items.get(inline, set())

    def parent(self, key):
        tid, path = key
        return (tid, path[:-1]) if path else None


# --------------------------------------------------------------- resolution


class Resolver:
    def __init__(self, world, follow):
        self.w = world
        self.follow = follow  # callable(container key, Use) -> bool: follow item re-exports

    def lookup_import(self, key, name, depth=0):
        """Find (Use, leaf) importing `name` into module `key` (also through globs)."""
        if depth > 4:
            return None
        globs = []
        for u in self.w.uses_in(key):
            for leaf in u.leaves:
                if leaf.glob:
                    globs.append((u, leaf))
                elif leaf.local() == name:
                    return (key, u, leaf)
        for u, leaf in globs:
            r = self.resolve(key, leaf.segs, depth=depth + 1, in_use=True, leading=u.leading)
            if not r or r["rest"]:
                continue
            src = r["module"]
            if name in self.w.children.get(src, {}):
                return ("child", src, name)
            hit = self.lookup_import(src, name, depth + 1)
            if hit:
                return hit
        return None

    def resolve(self, scope, segs, depth=0, in_use=False, dollar=False, leading=False):
        """Resolve the module prefix of segs from module `scope`.

        Returns {module, consumed, tail, start, leaf} or None."""
        w = self.w
        if depth > 6 or not segs:
            return None
        tid = scope[0]
        first = segs[0]
        start = None
        leaf_hit = None
        if leading:
            if first in w.extern:
                cur, i, start = (w.extern[first], ()), 1, "extern"
            else:
                return None
        elif first == "crate":
            cur, i, start = (tid, ()), 1, "dollar" if dollar else "crate"
        elif first == "self":
            cur, i, start = scope, 1, "self"
        elif first == "super":
            cur, i = scope, 0
            while i < len(segs) and segs[i] == "super":
                cur = w.parent(cur)
                if cur is None:
                    return None
                i += 1
            start = "super"
        elif first in w.children.get(scope, {}):
            cur, i, start = w.children[scope][first], 1, "bare"
        else:
            hit = self.lookup_import(scope, first, depth)
            if hit and hit[0] == "child":
                cur, i, start = w.children[hit[1]][hit[2]], 1, "import"
                leaf_hit = None
            elif hit:
                ukey, u, leaf = hit
                r = self.resolve(ukey, leaf.segs, depth + 1, in_use=True, leading=u.leading)
                if not r or r["rest_len"]:
                    return None
                cur, i, start = r["module"], 1, "import"
                leaf_hit = (ukey, u, leaf)
            elif first in w.extern:
                cur, i, start = (w.extern[first], ()), 1, "extern"
            else:
                return None
        tail = []
        while i < len(segs):
            nxt = segs[i]
            ch = w.children.get(cur, {})
            if nxt in ch:
                cur = ch[nxt]
                i += 1
                continue
            hits = [(u, leaf) for u in w.uses_in(cur) for leaf in u.leaves
                    if not leaf.glob and leaf.local() == nxt]
            if len(hits) == 1:
                u, leaf = hits[0]
                r = self.resolve(cur, leaf.segs, depth + 1, in_use=True, leading=u.leading)
                if r and self.follow(cur, u, r["module"]):
                    cur = r["module"]
                    tail = list(leaf.segs[r["consumed"]:])
                    i += 1
                    if tail:
                        break
                    continue
            break
        return {"module": cur, "consumed": i, "tail": tail, "start": start, "leaf": leaf_hit,
                "rest": segs[i:], "rest_len": len(segs) - i}


# ---------------------------------------------------------------- the plan


def load_rows():
    rows = []
    for layer in LAYERS:
        data = json.load(open(PLAN / f"mapping-{layer}.json"))
        for r in data["rows"]:
            rows.append(r)
    return rows


def package_dirs(paths):
    return {p.rsplit("/", 1)[0] for p in paths if p.endswith("/Cargo.toml")}


class Plan:
    def __init__(self, rows, current, select):
        self.rows = rows
        self.current = current
        self.pending = [r for r in rows if select(r) and self._pending(r)]
        self.moves = {}  # old -> new (primary destinations)
        self.merge_into = defaultdict(list)  # new target -> [old sources in order]
        self.removed = []
        self.created = []  # new rows (paths) not produced by a move
        self.split_targets = []
        for r in self.pending:
            old, new, ch = r.get("old"), r.get("new"), r["change"]
            if ch in ("move", "rename", "split"):
                self.moves[old] = new
            elif ch == "merge":
                if old:
                    self.merge_into[new].append(old)
            elif ch == "remove":
                self.removed.append(old)
            elif ch == "new":
                self.created.append(r)
            elif ch == "keep":
                pass
        # A merge target that another row moves a file to, or that exists, is the
        # primary; otherwise the first source moves there.
        targets_of_moves = set(self.moves.values())
        self.primary_merge = {}
        for tgt, srcs in self.merge_into.items():
            if tgt in targets_of_moves or tgt in current:
                self.primary_merge[tgt] = None
            else:
                prim = next((s for s in srcs if s.rsplit("/", 1)[1] == tgt.rsplit("/", 1)[1]), srcs[0])
                self.primary_merge[tgt] = prim
                self.moves[prim] = tgt
        self.appended = {}  # old source -> target it is appended to
        for tgt, srcs in self.merge_into.items():
            for s in srcs:
                if s not in self.moves:
                    self.appended[s] = tgt

    def _pending(self, r):
        old, new, ch = r.get("old"), r.get("new"), r["change"]
        if ch == "keep":
            return False
        if ch == "new":
            return new not in self.current
        if not old:
            return False
        return old in self.current and old != new


# ---------------------------------------------------------------- applying


def relpath(target, from_dir):
    return os.path.relpath(target, from_dir or ".")


def default_target_name(crate, kind, rel):
    return rel


class Mover:
    def __init__(self, args):
        self.args = args
        self.notes = []
        self.hand = []
        tracked = git("ls-files").splitlines()
        self.current = set(f for f in tracked if os.path.exists(f))
        self.rows = load_rows()
        crate_sel = [c.rstrip("/") for c in (args.crate or [])]
        self.crate_sel = crate_sel

        def crate_of_row(p):
            return crate_of(p, self.all_crates) if p else None

        cur_crates = package_dirs(self.current)
        new_crates = package_dirs([r["new"] for r in self.rows if r.get("new")])
        self.all_crates = cur_crates | new_crates
        if args.cross:
            def select(r):
                o, n = r.get("old"), r.get("new")
                if r["change"] == "new":
                    return crate_of_row(n) not in cur_crates or any(
                        x.get("old") and crate_of_row(x["old"]) != crate_of_row(n) and x.get("new", "").startswith(os.path.dirname(n))
                        for x in [])
                return o and n and crate_of_row(o) != crate_of_row(n)
        else:
            def inside(p):
                return p and any(p == c or p.startswith(c + "/") for c in crate_sel)

            def select(r):
                return inside(r.get("old")) or inside(r.get("new"))
        self.plan = Plan(self.rows, self.current, select)

    # -- new file set ---------------------------------------------------------
    def new_files(self):
        p = self.plan
        files = set(self.current)
        for old, new in p.moves.items():
            files.discard(old)
            files.add(new)
        for s in p.appended:
            files.discard(s)
        for r in p.removed:
            files.discard(r)
        return files

    def run(self):
        p = self.plan
        cur = self.current
        old_crates = package_dirs(cur)
        read_old = lambda f: open(f, encoding="utf-8").read()
        cargo_old = {c: read_old(f"{c}/Cargo.toml") for c in old_crates if os.path.exists(f"{c}/Cargo.toml")}
        lib_old = {c: toml_lib_name(t, c) for c, t in cargo_old.items()}
        self.W = World(cur, read_old, old_crates, lib_old, cargo_old)
        newf = self.new_files()
        # new folders needing a mod.rs / main.rs
        self.make_new_world(newf)
        self.rewrite_all()
        return 0

    # -- new identities ----------------------------------------------------
    def make_new_world(self, newf):
        W, p = self.W, self.plan
        self.newf = set(newf)
        self.new_crates = package_dirs(self.newf) | package_dirs([r["new"] for r in p.created])
        for r in p.created:
            if r["new"].endswith("Cargo.toml"):
                self.new_crates.add(r["new"].rsplit("/", 1)[0])
        # lib names in the new world
        self.lib_new = {}
        for c in self.new_crates:
            old_c = None
            for oc in W.crate_dirs:
                f = f"{oc}/Cargo.toml"
                if p.moves.get(f) == f"{c}/Cargo.toml" or (oc == c and f not in p.moves and f not in p.appended):
                    old_c = oc
            name = c.rsplit("/", 1)[1].replace("-", "_")
            if old_c and old_c == c:
                name = W.lib_names.get(old_c, name)
            self.lib_new[c] = name
        # map old file -> new file
        self.fmap = {}
        for f in self.current:
            if f in p.moves:
                self.fmap[f] = p.moves[f]
            elif f in p.appended:
                self.fmap[f] = p.appended[f]
            elif f in p.removed:
                self.fmap[f] = None
            else:
                self.fmap[f] = f
        # created module files (mod.rs of new folders, test main.rs)
        created_rs = {r["new"] for r in p.created if r["new"].endswith(".rs")}
        self.create = {}
        for f in sorted(self.newf | created_rs):
            if not f.endswith(".rs"):
                continue
        self.newf |= {f for f in created_rs if self._is_entry(f)}
        self._fill_folder_entries()
        # new module key for every old module
        self.newkey_cache = {}

    def _is_entry(self, f):
        b = f.rsplit("/", 1)[1]
        return b in ("mod.rs", "main.rs", "lib.rs")

    def _fill_folder_entries(self):
        """Every folder holding conventional modules gets a mod.rs (src) or main.rs (test area)."""
        W = self.W
        conv_new = set()
        cur_dirs = {os.path.dirname(f) for f in self.current}
        for f in self.current:
            nf = self.fmap.get(f)
            if not nf or not nf.endswith(".rs") or nf == f:
                continue
            if any(W.kind.get(k) == "conv" for k in W.incl.get(f, [])) or any(k[1] == () for k in W.incl.get(f, [])):
                conv_new.add(nf)
        self.conv_new = conv_new
        added = []
        for f in sorted(conv_new):
            crate = crate_of(f, self.new_crates)
            if not crate:
                continue
            rel = f[len(crate) + 1:]
            parts = rel.split("/")
            if parts[0] == "src" and len(parts) > 2 and parts[1] != "bin":
                for k in range(2, len(parts)):
                    d = crate + "/" + "/".join(parts[:k])
                    if f"{d}/mod.rs" not in self.newf and f"{d}.rs" not in self.newf:
                        self.newf.add(f"{d}/mod.rs")
                        added.append(f"{d}/mod.rs")
            if parts[0] == "tests" and len(parts) > 2 and parts[1] not in ("support", "standalone", "common", "compile_fail"):
                d = f"{crate}/tests/{parts[1]}"
                if f"{d}/main.rs" not in self.newf and (d not in cur_dirs or f"{d}/main.rs" in self.current):
                    self.newf.add(f"{d}/main.rs")
                    added.append(f"{d}/main.rs")
        created_rows = {r["new"] for r in self.plan.created}
        for a in added:
            if a not in created_rows:
                self.notes.append(f"created {a} (no mapping row)")
        self.added_entries = set(added) | {r["new"] for r in self.plan.created
                                           if self._is_entry(r["new"]) and r["new"] not in self.current}

    def new_target_of(self, f, old_key):
        """(tid, path, kind) for new file f, given one of its old inclusions."""
        crate = crate_of(f, self.new_crates)
        rel = f[len(crate) + 1:]
        parts = rel.split("/")
        old_t = self.W.targets[old_key[0]]
        if parts[0] == "src":
            if parts[1] == "bin":
                name = parts[2][:-3] if len(parts) == 3 else parts[2]
                root = f"{crate}/src/bin/{parts[2]}" if len(parts) == 3 else f"{crate}/src/bin/{parts[2]}/main.rs"
                tid = f"{crate}::bin::{name}"
                sub = parts[3:] if len(parts) > 3 else []
                return tid, self._conv(sub), root == f
            has_lib = f"{crate}/src/lib.rs" in self.newf
            if old_t.kind == "bin" and f"{crate}/src/main.rs" in self.newf and (not has_lib or old_t.root.endswith("src/main.rs")):
                tid = f"{crate}::bin::{crate.rsplit('/', 1)[1]}"
                if rel == "src/main.rs":
                    return tid, (), True
                return tid, self._conv(parts[1:]), False
            tid = f"{crate}::lib::{self.lib_new.get(crate)}"
            if rel == "src/lib.rs":
                return tid, (), True
            return tid, self._conv(parts[1:]), False
        auto = {"tests": "test", "examples": "example", "benches": "bench"}
        if parts[0] in auto:
            kind = auto[parts[0]]
            if len(parts) == 2:
                return f"{crate}::{kind}::{parts[1][:-3]}", (), True
            if parts[1] == "standalone" and len(parts) == 3:
                return f"{crate}::{kind}::{parts[2][:-3]}", (), True
            if parts[1] in ("support", "common", "standalone", "compile_fail"):
                return None
            tid = f"{crate}::{kind}::{parts[1]}"
            if len(parts) == 3 and parts[2] == "main.rs":
                return tid, (), True
            return tid, self._conv(parts[2:]), False
        return None

    @staticmethod
    def _conv(parts):
        parts = list(parts)
        if not parts:
            return ()
        last = parts[-1]
        if last == "mod.rs":
            parts = parts[:-1]
        else:
            parts[-1] = last[:-3]
        return tuple(parts)

    def newkey(self, key):
        """New module key of old module key, or None."""
        if key in self.newkey_cache:
            return self.newkey_cache[key]
        self.newkey_cache[key] = None
        W = self.W
        res = None
        if key in W.mods:
            f, inline = W.mods[key]
            fkey = (key[0], key[1][:len(key[1]) - len(inline)]) if inline else key
            base = self._newkey_file(f, fkey)
            if base is not None:
                res = (base[0], base[1] + inline)
        self.newkey_cache[key] = res
        return res

    def _newkey_file(self, f, fkey):
        W = self.W
        nf = self.fmap.get(f, f)
        kind = W.kind.get(fkey, "root" if fkey[1] == () else "conv")
        if nf is None:
            # removed file: the lowest common ancestor of its children's new modules
            kids = [self.newkey(c) for c in W.children.get(fkey, {}).values()]
            kids = [k for k in kids if k]
            if kids:
                tid = kids[0][0]
                common = kids[0][1][:-1]
                for k in kids[1:]:
                    n = 0
                    while n < min(len(common), len(k[1])) and common[n] == k[1][n]:
                        n += 1
                    common = common[:n]
                return (tid, common)
            par = W.parent(fkey)
            return self.newkey(par) if par else None
        if f in self.plan.appended:
            # merged into another file: that file's new module
            tgt = self.plan.appended[f]
            for of, nf2 in self.fmap.items():
                if nf2 == tgt and of not in self.plan.appended and W.incl.get(of):
                    return self._newkey_file(of, W.incl[of][0])
            t = self.new_target_of(tgt, fkey)
            return (t[0], t[1]) if t else None
        if kind == "path" or (kind == "conv" and len(W.incl.get(f, [])) > 1):
            par = W.parent(fkey)
            pk = self.newkey(par)
            if pk is None:
                return None
            return (pk[0], pk[1] + (fkey[1][-1],))
        t = self.new_target_of(nf, fkey)
        if t is None:
            par = W.parent(fkey)
            pk = self.newkey(par) if par else None
            return (pk[0], pk[1] + (fkey[1][-1],)) if pk else None
        return (t[0], t[1])

    # -- spelling ----------------------------------------------------------
    def spell(self, target, scope, style, orig_segs):
        """Spell module `target` from module `scope` (both new keys) in a style."""
        ttid, tpath = target
        stid, spath = scope
        if ttid != stid:
            crate = ttid.split("::")[0]
            kind = ttid.split("::")[1]
            if kind != "lib":
                return None
            return [self.lib_new.get(crate, crate.rsplit("/", 1)[1].replace("-", "_"))] + list(tpath)
        if style in ("crate", "dollar", "extern"):
            return ["crate"] + list(tpath)
        k = 0
        while k < min(len(tpath), len(spath)) and tpath[k] == spath[k]:
            k += 1
        ups = len(spath) - k
        rest = list(tpath[k:])
        if style == "self" and ups == 0:
            return ["self"] + rest
        if style == "bare" and ups == 0 and rest:
            return rest
        if ups == 0 and not rest:
            return ["self"]
        if style in ("super", "self", "bare") and ups <= 2:
            if ups == 0:
                return ["self"] + rest
            return ["super"] * ups + rest
        return ["crate"] + list(tpath)

    # -- rewriting -----------------------------------------------------------
    def follow_reexport(self, key, use, target):
        """Follow a re-export to its source when the re-export cannot carry the path."""
        if use.compat:
            return True
        if self.args.mode == "callers":
            return False
        here, there = self.newkey(key), self.newkey(target)
        if here is None:
            return there is not None
        if there is None:
            return False
        return here[0].split("::")[0] != there[0].split("::")[0]

    def rewrite_all(self):
        W = self.W
        R = Resolver(W, self.follow_reexport)
        self.R = R
        scope_files = self.rewrite_scope()
        self.edits = defaultdict(list)  # old file -> [(start, end, text)]
        self.renamed_locals = defaultdict(dict)
        for f in sorted(scope_files):
            if not f.endswith(".rs") or f not in W.incl:
                continue
            self.rewrite_file(f)
        self.emit()

    def rewrite_scope(self):
        if self.args.rewrite == "all" or self.args.mode == "callers":
            return set(self.W.incl)
        sel = set()
        moved = set(self.plan.moves) | set(self.plan.appended) | set(self.plan.removed)
        for f in self.W.incl:
            if any(f.startswith(c + "/") for c in self.crate_sel) or f in moved:
                sel.add(f)
        return sel

    def inclusions(self, f):
        """[(old scope key of file module, new key)] for every inclusion of f."""
        out = []
        for k in self.W.incl.get(f, []):
            out.append((k, self.newkey(k)))
        return out

    def rewrite_file(self, f):
        W, R = self.W, self.R
        p = W.parse(f)
        incs = self.inclusions(f)
        if not incs:
            return
        toks = p.toks
        edits = self.edits[f]

        def scope_keys(inline):
            return [((k[0], k[1] + inline), (nk[0], nk[1] + inline) if nk else None) for k, nk in incs]

        # use statements
        renames = {}  # (scope inline, old local) -> new local
        for u in p.uses:
            if not u.leaves:
                continue
            changed = False
            new_leaves = []
            for leaf in u.leaves:
                segs = list(leaf.segs)
                if not segs:
                    new_leaves.append((segs, leaf.alias, leaf.glob))
                    continue
                spelled = None
                ok = True
                for ok_, nk in scope_keys(u.scope):
                    r = R.resolve(ok_, segs, in_use=True, leading=u.leading)
                    if not r or r["start"] == "import" and not self._import_renamed(r):
                        spelled = spelled or None
                        ok = ok and True
                        if r is None:
                            spelled = "keep"
                        else:
                            spelled = spelled if spelled not in (None,) else "keep"
                        continue
                    tm = self.newkey(r["module"])
                    if tm is None or nk is None:
                        spelled = "keep"
                        continue
                    if r["start"] == "import":
                        spelled = "keep"
                        continue
                    s = self.spell(tm, nk, r["start"], segs)
                    if s is None:
                        self.hand.append(f"{f}: cannot reach {tm} from {nk} ({'::'.join(segs)})")
                        spelled = "keep"
                        continue
                    full = s + r["tail"] + segs[r["consumed"]:]
                    if (not leaf.glob and r["consumed"] == len(segs) and not r["tail"] and tm[0] == nk[0]
                            and tm[1] == nk[1] + (leaf.local(),)):
                        full = ["<drop>"]
                    if spelled not in (None, "keep") and spelled != full:
                        self.hand.append(f"{f}: inclusions disagree on {'::'.join(segs)}")
                    spelled = full
                if spelled in (None, "keep"):
                    new_leaves.append((segs, leaf.alias, leaf.glob))
                    continue
                if u.leading and spelled and spelled[0] in ("crate", "self", "super"):
                    pass
                alias = leaf.alias
                if spelled != segs:
                    changed = True
                    if spelled == ["<drop>"]:
                        new_leaves.append((spelled, None, False))
                        continue
                    if not leaf.glob and alias is None and segs[-1] != spelled[-1] and segs[-1] not in ("self",):
                        # a renamed module import: keep the old local name where it is
                        # public, otherwise rename its uses in this file
                        if u.vis:
                            alias = segs[-1]
                        else:
                            renames[(u.scope, segs[-1])] = spelled[-1]
                new_leaves.append((spelled, alias, leaf.glob))
            if changed:
                kept = [x for x in new_leaves if x[0] != ["<drop>"]]
                if not kept:
                    edits.append((self._line_start(p.text, u.stmt[0]), self._line_end(p.text, u.stmt[1]), ""))
                    continue
                text = emit_use_tree(kept, leading=u.leading and kept[0][0][0] not in ("crate", "self", "super"))
                edits.append((u.span[0] - (2 if u.leading else 0), u.span[1], text))
        # check rename conflicts
        names_in = defaultdict(set)
        for u in p.uses:
            for leaf in u.leaves:
                if leaf.local():
                    names_in[u.scope].add(leaf.local())
        for scope_, names in p.items.items():
            names_in[scope_] |= names
        for (sc, old), new in list(renames.items()):
            if new in names_in[sc] and new != old:
                # conflict: alias instead
                del renames[(sc, old)]
                self._alias_instead(p, edits, sc, old, new)
        # path occurrences
        for o in p.occs:
            segs = o.segs
            spelled = "keep"
            for ok_, nk in scope_keys(o.scope):
                r = R.resolve(ok_, segs, dollar=o.dollar, leading=o.leading)
                if not r or nk is None:
                    continue
                if r["start"] == "import":
                    key = (o.scope, segs[0])
                    new = None
                    for (sc, old), nn in renames.items():
                        if old == segs[0] and (sc == o.scope[:len(sc)]):
                            new = nn
                    if new:
                        spelled = [new] + segs[1:]
                    continue
                tm = self.newkey(r["module"])
                if tm is None:
                    continue
                s = self.spell(tm, nk, r["start"], segs)
                if s is None:
                    self.hand.append(f"{f}: cannot reach {tm} from {nk} ({'::'.join(segs)})")
                    continue
                full = s + r["tail"] + segs[r["consumed"]:]
                if spelled != "keep" and spelled != full:
                    self.hand.append(f"{f}: inclusions disagree on {'::'.join(segs)}")
                spelled = full
            if spelled == "keep" or spelled == segs:
                continue
            start = toks[o.toks[0]].s
            if o.dollar:
                start -= 1
            if o.leading:
                start -= 2
            end = toks[o.toks[-1]].e
            text = "::".join(spelled)
            if o.dollar and spelled[0] == "crate":
                text = "$" + text
            elif o.dollar or (o.leading and spelled[0] not in ("crate", "self", "super")):
                text = "::" + text
            edits.append((start, end, text))
        # includes and manifest-relative literals
        nf = self.fmap.get(f, f) or self.plan.appended.get(f)
        for inc in p.includes:
            tok = toks[inc.tok]
            if inc.kind == "manifest":
                crate = crate_of(f, self.W.crate_dirs)
                old_abs = os.path.normpath(crate + "/" + inc.value.lstrip("/"))
                new_abs = self.map_path(old_abs)
                ncrate = crate_of(nf, self.new_crates) if nf else crate
                if new_abs and (new_abs != old_abs or ncrate != crate):
                    lit = "/" + os.path.relpath(new_abs, ncrate) if inc.value.startswith("/") else os.path.relpath(new_abs, ncrate)
                    if new_abs.endswith("/") or inc.value.endswith("/"):
                        lit = lit.rstrip("/") + "/"
                    if lit != inc.value:
                        edits.append((tok.s, tok.e, json.dumps(lit)))
                continue
            old_abs = os.path.normpath(os.path.join(os.path.dirname(f), inc.value))
            new_abs = self.map_path(old_abs)
            if not nf or not new_abs:
                continue
            lit = os.path.relpath(new_abs, os.path.dirname(nf))
            if lit != inc.value:
                edits.append((tok.s, tok.e, json.dumps(lit)))
        # #[path] declarations
        for d in p.decls:
            if not d.path_attr or d.path_attr[1] is None:
                continue
            if d.scope:
                continue
            tok = toks[d.path_attr[1]]
            old_abs = os.path.normpath(os.path.join(os.path.dirname(f), d.path_attr[0]))
            new_abs = self.map_path(old_abs)
            if not nf or not new_abs:
                continue
            lit = os.path.relpath(new_abs, os.path.dirname(nf))
            if lit != d.path_attr[0]:
                edits.append((tok.s, tok.e, json.dumps(lit)))

    def _import_renamed(self, r):
        return False

    def _alias_instead(self, p, edits, sc, old, new):
        # rewrite the emitted use edit to add `as old`
        for i, (s, e, t) in enumerate(edits):
            if re.search(rf"\b{re.escape(new)}\b", t):
                t2 = re.sub(rf"\b{re.escape(new)}\b(?!\s*::)(?!\s+as)", f"{new} as {old}", t, count=1)
                edits[i] = (s, e, t2)
                return

    def map_path(self, old_abs):
        if old_abs in self.fmap:
            return self.fmap[old_abs]
        # a directory: map by its files' common destination
        pre = old_abs.rstrip("/") + "/"
        dests = [(f, n) for f, n in self.fmap.items() if f.startswith(pre) and n]
        if not dests:
            return old_abs
        if all(n.startswith(pre) for f, n in dests if f == n) and all(f == n for f, n in dests):
            return old_abs
        # take the destination folder of the first moved file minus its relative part
        for f, n in dests:
            rel = f[len(pre):]
            if n.endswith(rel):
                return n[: len(n) - len(rel)].rstrip("/")
        return old_abs

    # -- declarations ------------------------------------------------------------
    def desired_children(self):
        """new parent key -> {name: (old key or None)} for conventional modules."""
        W = self.W
        want = defaultdict(dict)
        for key in W.mods:
            if W.mods[key][1]:
                continue  # inline modules travel with their file
            kind = W.kind.get(key)
            f = W.mods[key][0]
            if kind == "path" or (kind == "conv" and len(W.incl.get(f, [])) > 1):
                continue
            if f in self.plan.appended or self.fmap.get(f) is None:
                continue
            nk = self.newkey(key)
            if nk is None or not nk[1]:
                continue
            want[(nk[0], nk[1][:-1])].setdefault(nk[1][-1], key)
        # intermediate folders created by the tool
        for nk_parent in list(want):
            tid, path = nk_parent
            while path:
                up = (tid, path[:-1])
                want[up].setdefault(path[-1], None)
                path = path[:-1]
        return want

    def emit(self):
        """Compose new file contents and apply git operations."""
        W, p = self.W, self.plan
        want = self.desired_children()
        # new file -> [old pieces], primary first
        pieces = defaultdict(list)
        for f in sorted(self.current):
            nf = self.fmap.get(f)
            if nf and f not in p.appended:
                pieces[nf].insert(0, f)
        for tgt, srcs in p.merge_into.items():
            for s in srcs:
                if s in p.appended:
                    pieces[tgt].append(s)
        # removed mod.rs with content: append to the module receiving its children
        for r in p.removed:
            if r.endswith(".rs") and W.incl.get(r):
                nk = self.newkey(W.incl[r][0])
                host = self.file_of_newkey(nk) if nk else None
                body = self.non_decl_content(r)
                if host and body.strip():
                    pieces[host].append(r)
                    self.notes.append(f"{r}: non-declaration content appended to {host}")
                    k0 = W.incl[r][0]
                    if k0 in W.decl_of and W.decl_of[k0][1].attrs:
                        self.hand.append(f"{r}: its declaration carried {W.decl_of[k0][1].attrs}; apply them to the content appended to {host}")
        # key of each new module file
        file_key = {}
        for f in list(pieces) + sorted(self.added_entries):
            k = self.key_of_new_file(f, pieces.get(f))
            if k:
                file_key[f] = k
        affected = set()
        for key, (f, inline) in W.mods.items():
            if inline or not key[1]:
                continue
            if self.fmap.get(f) == f and f not in p.appended:
                continue
            par = W.parent(key)
            if par is not None:
                affected.add(self.newkey(par))
            nk = self.newkey(key)
            if nk and nk[1]:
                affected.add((nk[0], nk[1][:-1]))
            if nk:
                affected.add(nk)
        for nf in self.added_entries:
            k = file_key.get(nf)
            if k:
                affected.add(k)
                if k[1]:
                    affected.add((k[0], k[1][:-1]))
        self.affected = affected
        changes = {}
        for nf, srcs in pieces.items():
            if not nf.endswith(".rs"):
                continue
            touched = any(s != nf for s in srcs) or any(self.edits.get(s) for s in srcs)
            key = file_key.get(nf)
            if key not in affected:
                key = None
            kids = want.get(key, {}) if key else {}
            if not touched and key is None and len(srcs) == 1:
                continue
            changes[nf] = self.compose(nf, srcs, key, kids)
        for nf in sorted(self.added_entries):
            if nf in pieces:
                continue
            key = file_key.get(nf)
            kids = want.get(key, {}) if key else {}
            changes[nf] = self.compose(nf, [], key, kids)
        self.apply_git(changes, pieces)
        self.cargo_targets()
        self.report()

    def key_of_new_file(self, nf, srcs):
        W = self.W
        for s in srcs or []:
            if s in W.incl and s not in self.plan.appended and self.fmap.get(s) == nf:
                return self.newkey(W.incl[s][0])
        crate = crate_of(nf, self.new_crates)
        if not crate:
            return None
        rel = nf[len(crate) + 1:]
        parts = rel.split("/")
        if parts[0] == "src":
            tid = f"{crate}::lib::{self.lib_new.get(crate)}"
            if rel == "src/lib.rs":
                return (tid, ())
            return (tid, self._conv(parts[1:]))
        if parts[0] == "tests" and len(parts) == 3 and parts[2] == "main.rs":
            return (f"{crate}::test::{parts[1]}", ())
        return None

    def file_of_newkey(self, nk):
        tid, path = nk
        crate = tid.split("::")[0]
        kind = tid.split("::")[1]
        name = tid.split("::")[2]
        if kind == "lib":
            base = f"{crate}/src"
            root = f"{crate}/src/lib.rs"
        elif kind == "test":
            base = f"{crate}/tests/{name}"
            root = f"{crate}/tests/{name}/main.rs"
            if f"{crate}/tests/{name}.rs" in self.newf:
                return f"{crate}/tests/{name}.rs" if not path else None
        else:
            return None
        if not path:
            return root
        cands = [f"{base}/{'/'.join(path)}/mod.rs", f"{base}/{'/'.join(path)}.rs"]
        return next((c for c in cands if c in self.newf), cands[0])

    def non_decl_content(self, f):
        p = self.W.parse(f)
        text = p.text
        spans = [d.span for d in p.decls if not d.inline and not d.path_attr and not d.scope]
        out, pos = [], 0
        for s, e in sorted(spans):
            out.append(text[pos:s])
            pos = e
        out.append(text[pos:])
        body = "".join(out)
        body = re.sub(r"(?m)^\s*//!.*\n?", "", body)
        return body

    def decls_differ(self, primary, key, kids):
        W = self.W
        p = W.parse(primary) if primary in W.incl or os.path.exists(primary) else None
        if p is None:
            return bool(kids)
        have = {d.name for d in p.decls if not d.inline and not d.path_attr and not d.scope}
        return have != set(kids) or any(self.decl_text(n, k, key) is not None and self._decl_changed(p, n, k, key)
                                        for n, k in kids.items())

    def _decl_changed(self, p, name, oldkey, key):
        for d in p.decls:
            if d.name == name and not d.inline and not d.scope:
                return self.decl_text(name, oldkey, key) != self.text_of_decl(p, d)
        return True

    def text_of_decl(self, p, d):
        return p.text[d.span[0]:d.span[1]]

    def decl_text(self, name, oldkey, parent_key):
        W = self.W
        if oldkey is None:
            vis = "pub" if self.public_below(parent_key, name) else "pub(crate)"
            return f"{vis} mod {name};"
        if oldkey not in W.decl_of:
            return f"mod {name};"  # an old crate root that became a module (test topic)
        f, d = W.decl_of[oldkey]
        par = W.parent(oldkey)
        if par and self.newkey(par) == parent_key and d.name == name:
            vis = d.vis
        else:
            vis = d.vis if d.vis in ("pub", "pub(crate)") else "pub(crate)"
        attrs = list(d.attrs)
        # a removed parent folder's own attributes (cfg) now apply to its children
        while par and par in W.decl_of and W.mods.get(par, (None,))[0] in self.plan.removed:
            pattrs = [a for a in W.decl_of[par][1].attrs if a.startswith("#[cfg")]
            attrs = [a for a in pattrs if a not in attrs] + attrs
            par = W.parent(par)
        lines = list(d.docs) + attrs + [(vis + " " if vis else "") + f"mod {name};"]
        return "\n".join(lines)

    def public_below(self, parent_key, name):
        W = self.W
        for key in W.mods:
            nk = self.newkey(key)
            if nk and nk[0] == parent_key[0] and nk[1][:len(parent_key[1]) + 1] == parent_key[1] + (name,):
                if self.publicly_reachable(key):
                    return True
        return False

    def publicly_reachable(self, key):
        W = self.W
        tid, path = key
        for k in range(1, len(path) + 1):
            sub = (tid, path[:k])
            if sub not in W.decl_of:
                return False
            if W.decl_of[sub][1].vis != "pub":
                return False
        return W.targets[tid].kind == "lib"

    def compose(self, nf, srcs, key, kids):
        """Text of new file nf from its old pieces.

        In the primary piece each kept declaration stays where it is (renamed or
        re-attributed in place) and departed ones are deleted; arriving children
        are inserted after the last kept declaration. Other pieces lose their
        declarations and are appended."""
        W = self.W
        # which existing declaration of the primary stands for which new child
        by_old = {}
        for name, ok in kids.items():
            if ok is not None:
                by_old[ok] = name
        placed = set()
        parts = []
        for idx, s in enumerate(srcs):
            p = W.parse(s)
            text = p.text
            edits = list(self.edits.get(s, []))
            insert_at = None
            if key is not None:
                fkeys = W.incl.get(s, [])
                for d in p.decls:
                    if d.inline or d.path_attr or d.scope:
                        continue
                    st, en = self._line_start(text, d.span[0]), self._line_end(text, d.span[1])
                    child_name = None
                    if idx == 0:
                        for fk in fkeys:
                            ok = (fk[0], fk[1] + (d.name,))
                            if ok in by_old and by_old[ok] not in placed:
                                child_name = by_old[ok]
                                break
                    if child_name is not None:
                        placed.add(child_name)
                        new_text = self.decl_text(child_name, kids[child_name], key)
                        old_text = text[d.span[0]:d.span[1]]
                        if new_text != old_text:
                            edits.append((d.span[0], d.span[1], new_text))
                        insert_at = en
                    else:
                        edits.append((st, en, ""))
                        if idx == 0 and insert_at is None:
                            insert_at = ("first", st)
            body = apply_edits(text, edits)
            if idx == 0:
                # position in the edited body: map the original offset
                if isinstance(insert_at, tuple):
                    off = insert_at[1]
                elif insert_at is None:
                    off = None
                else:
                    off = insert_at
                if off is None:
                    anchor = self._after_inner(body)
                else:
                    anchor = len(apply_edits(text[:off], [e for e in edits if e[1] <= off]))
                parts.append((body, anchor))
            else:
                body = re.sub(r"(?m)^(\s*)//!", r"\1//", body)
                parts.append(("\n" + body.lstrip("\n"), None))
        block = ""
        if key is not None:
            rest = sorted(n for n in kids if n not in placed)
            block = "".join(self.decl_text(n, kids[n], key) + "\n" for n in rest)
        if not srcs:
            return block
        body, anchor = parts[0]
        if block:
            body = body[:anchor] + block + body[anchor:]
        return body + "".join(b for b, _ in parts[1:])

    def _drop_decls_text(self, body):
        return body

    @staticmethod
    def _line_start(text, i):
        j = text.rfind("\n", 0, i) + 1
        return j if text[j:i].strip() == "" else i

    @staticmethod
    def _line_end(text, i):
        j = text.find("\n", i)
        if j < 0:
            return len(text)
        return j + 1 if text[i:j].strip() == "" else i

    def _after_inner(self, body):
        p = Parsed("x", body)
        e = p.inner_attr_end
        if e == 0:
            return 0
        j = body.find("\n", e)
        return len(body) if j < 0 else j + 1

    # -- git -----------------------------------------------------------------
    def apply_git(self, changes, pieces):
        p = self.plan
        dry = self.args.dry_run
        ops = []
        for old, new in sorted(p.moves.items()):
            ops.append(("mv", old, new))
        for s in sorted(p.appended):
            ops.append(("rm", s))
        for r in sorted(p.removed):
            ops.append(("rm", r))
        self.ops = ops
        if dry:
            self.changes = changes
            return
        for op in ops:
            if op[0] == "mv":
                os.makedirs(os.path.dirname(op[2]), exist_ok=True)
                subprocess.run(["git", "mv", "-k", op[1], op[2]], check=True)
        for nf, text in changes.items():
            os.makedirs(os.path.dirname(nf), exist_ok=True)
            with open(nf, "w", encoding="utf-8") as fh:
                fh.write(text)
            subprocess.run(["git", "add", nf], check=True)
        for op in ops:
            if op[0] == "rm" and os.path.exists(op[1]):
                subprocess.run(["git", "rm", "-q", "-f", op[1]], check=True)
        # files only rewritten in place (callers)
        for f, ed in self.edits.items():
            if not ed:
                continue
            nf = self.fmap.get(f)
            if nf in changes or f in p.appended or nf is None:
                continue
            text = apply_edits(self.W.parse(f).text, ed)
            with open(nf, "w", encoding="utf-8") as fh:
                fh.write(text)
            subprocess.run(["git", "add", nf], check=True)

    def cargo_targets(self):
        """Fix [[test]] etc. paths; add entries for new standalone roots."""
        p = self.plan
        by_crate = defaultdict(list)
        for old, new in p.moves.items():
            if old.endswith(".rs"):
                by_crate[crate_of(new, self.new_crates)].append((old, new))
        for crate, moves in by_crate.items():
            if not crate:
                continue
            path = f"{crate}/Cargo.toml"
            src = path if os.path.exists(path) else None
            if src is None:
                continue
            text = open(src).read()
            orig = text
            entries = toml_targets(text)
            old_crates = {crate_of(o, self.W.crate_dirs) for o, _ in moves}
            for o, n in moves:
                ocrate = crate_of(o, self.W.crate_dirs)
                orel = o[len(ocrate) + 1:]
                nrel = n[len(crate) + 1:]
                oc_text = open(f"{ocrate}/Cargo.toml").read() if os.path.exists(f"{ocrate}/Cargo.toml") else ""
                oentries = toml_targets(oc_text if ocrate != crate else text)
                ent = next((e for e in oentries if e["fields"].get("path") == orel), None)
                nparts = nrel.split("/")
                is_root = (len(nparts) == 2 and nparts[0] in ("tests", "examples", "benches")) or \
                          (len(nparts) == 3 and nparts[1] == "standalone") or \
                          (len(nparts) == 3 and nparts[2] == "main.rs" and nparts[0] in ("tests", "examples", "benches"))
                if ent and ocrate == crate:
                    if is_root:
                        text = text.replace(f'path = "{orel}"', f'path = "{nrel}"')
                    else:
                        text = text.replace(ent["body"], "\n") if False else text
                        span_text = text[text.find(f'path = "{orel}"'):]
                        self.hand.append(f"{path}: [[{ent['kind']}]] {ent['fields'].get('name')} now a module ({nrel}); entry needs removal: {ent['fields']}")
                elif ent and ocrate != crate:
                    self.hand.append(f"{path}: carry [[{ent['kind']}]] {ent['fields']} from {ocrate} as {nrel}")
                if not ent and len(nparts) == 3 and nparts[1] == "standalone":
                    kind = {"tests": "test", "examples": "example", "benches": "bench"}[nparts[0]]
                    name = nparts[2][:-3]
                    text = text.rstrip("\n") + f'\n\n[[{kind}]]\nname = "{name}"\npath = "{nrel}"\n'
            if text != orig and not self.args.dry_run:
                open(path, "w").write(text)
                subprocess.run(["git", "add", path], check=True)

    def report(self):
        p = self.plan
        for r in p.pending:
            if r["change"] == "split":
                self.hand.append(f"split {r['old']} -> {r['new']}: {r['note']}")
            elif r["change"] == "new" and r["new"] not in self.added_entries:
                self.hand.append(f"new {r['new']}: {r['note']}")
            elif r["change"] == "merge":
                self.hand.append(f"merge {r['old']} -> {r['new']}: {r['note']}")
        for f, pr in ((f, pp) for f, pp in self.W.parsed.items()):
            for x in pr.problems:
                self.notes.append(x)
        out = {"moves": len(p.moves), "appended": sorted(p.appended), "removed": p.removed,
               "notes": self.notes, "hand": self.hand}
        if self.args.dry_run:
            out["ops"] = self.ops
            out["changes"] = {k: v for k, v in sorted(self.changes.items())} if self.args.show else sorted(self.changes)
        text = json.dumps(out, indent=1)
        if self.args.report:
            open(self.args.report, "w").write(text)
        print(f"moves {len(p.moves)}, appended {len(p.appended)}, removed {len(p.removed)}, "
              f"notes {len(self.notes)}, hand {len(self.hand)}")
        for h in self.hand[:200]:
            print("HAND", h)
        for n in self.notes[:100]:
            print("NOTE", n)


def apply_edits(text, edits):
    out, pos = [], 0
    for s, e, t in sorted(edits, key=lambda x: (x[0], x[1])):
        if s < pos or t is None:
            continue
        out.append(text[pos:s])
        out.append(t)
        pos = e
    out.append(text[pos:])
    return "".join(out)


def emit_use_tree(leaves, leading=False):
    """Emit a use tree (without `use` and `;`) from leaves [(segs, alias, glob)]."""
    root = {"kids": {}, "order": [], "term": []}

    def node():
        return {"kids": {}, "order": [], "term": []}

    for segs, alias, glob in leaves:
        cur = root
        for s in segs:
            if s not in cur["kids"]:
                cur["kids"][s] = node()
                cur["order"].append(s)
            cur = cur["kids"][s]
        cur["term"].append("*" if glob else ("self" + (f" as {alias}" if alias else "")))

    def items(n):
        out = []
        for t in n["term"]:
            out.append(t)
        for s in n["order"]:
            out.append(render(s, n["kids"][s]))
        return out

    def render(name, n):
        sub = items(n)
        if len(sub) == 1 and n["term"] == ["self"] and not n["order"]:
            return name
        if len(sub) == 1 and n["term"] and n["term"][0].startswith("self as"):
            return f"{name} as {n['term'][0][8:]}"
        if len(sub) == 1 and n["term"] == ["*"]:
            return f"{name}::*"
        if len(sub) == 1 and not n["term"]:
            return f"{name}::{sub[0]}"
        return f"{name}::{{{', '.join(sub)}}}"

    top = items(root)
    text = top[0] if len(top) == 1 else "{" + ", ".join(top) + "}"
    return ("::" if leading else "") + text


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("apply", "callers", "status"))
    ap.add_argument("--crate", action="append")
    ap.add_argument("--cross", action="store_true")
    ap.add_argument("--rewrite", choices=("all", "selected"), default="selected")
    ap.add_argument("--compat", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--show", action="store_true")
    ap.add_argument("--report")
    args = ap.parse_args(argv)
    m = Mover(args)
    if args.mode == "status":
        for r in m.plan.pending:
            print(r["change"], r.get("old"), "->", r.get("new"))
        return 0
    return m.run()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

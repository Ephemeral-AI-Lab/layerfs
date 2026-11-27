"""Respell repository paths that #828 moved, in text files (docs, tools, strings).

Usage (repository root): repath.py [--dry] FILE...

A path is rewritten only when it existed at the base (3203f6acbf) and is gone
now. Files map by the mapping rows (`mapping-*.json`; for a split, the row that
keeps the basename, else the first), then by Git rename detection. A directory
maps to the deepest directory holding all of its files that went to the
crate most of them went to. Longer prefixes win, so
`dir/file.rs` maps as a file and `dir/` as a directory. A Markdown link
label in backticks that was a suffix of the old target becomes the same
suffix of the new one.
"""

import collections
import json
import os
import re
import subprocess
import sys

BASE = "3203f6acbf"
HERE = os.path.dirname(os.path.abspath(__file__))


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def build():
    base = set(git("ls-tree", "-r", "--name-only", BASE).split())
    fmap = {}
    for layer in ("core", "engine", "control", "api"):
        rows = json.load(open(f"{HERE}/mapping-{layer}.json"))["rows"]
        by_old = collections.defaultdict(list)
        for r in rows:
            if r["old"] and r["new"] and os.path.exists(r["new"]):
                by_old[r["old"]].append(r["new"])
        for old, news in by_old.items():
            same = [n for n in news if os.path.basename(n) == os.path.basename(old)]
            fmap[old] = (same or news)[0]
    out = git("-c", "diff.renameLimit=20000", "diff", "-M50", "--name-status", BASE, "HEAD")
    for line in out.splitlines():
        parts = line.split("\t")
        if parts[0].startswith("R") and parts[1] not in fmap:
            fmap[parts[1]] = parts[2]
    fmap = {o: n for o, n in fmap.items() if o in base and not os.path.exists(o) and o != n}
    under = collections.defaultdict(list)
    for old, new in fmap.items():
        o = old.split("/")
        for k in range(2, len(o)):
            under["/".join(o[:k])].append(new)
    dmap = {}
    for d, news in under.items():
        if os.path.isdir(d):
            continue
        depth = 3 if d.split("/")[1:2] == ["crates"] else 2
        crates = collections.Counter("/".join(n.split("/")[:depth]) for n in news)
        crate, _ = crates.most_common(1)[0]
        parents = [n.split("/")[:-1] for n in news if n.startswith(crate + "/")]
        common = parents[0]
        for q in parents[1:]:
            i = 0
            while i < min(len(common), len(q)) and common[i] == q[i]:
                i += 1
            common = common[:i]
        dmap[d] = "/".join(common)
    return base, fmap, dmap


PAT = re.compile(r"(?<![A-Za-z0-9_\-])((?:core|engine|control|api)(?:/[A-Za-z0-9_\-\.]+)+)")


LINK = re.compile(r"\[`([^`\]]+)`\]\(([^)\s]+)\)")


def main(argv):
    dry = argv[:1] == ["--dry"]
    files = argv[1:] if dry else argv
    base, fmap, dmap = build()
    unresolved = collections.Counter()
    total = 0
    for f in files:
        text = open(f).read()

        def sub(m):
            nonlocal total
            raw = m.group(1)
            p = raw.rstrip(".")
            tail = raw[len(p):]
            if os.path.exists(p):
                return raw
            if p in fmap:
                total += 1
                return fmap[p] + tail
            segs = p.split("/")
            for k in range(len(segs) - 1, 1, -1):
                head = "/".join(segs[:k])
                if head in fmap or head in dmap:
                    new = (fmap.get(head) or dmap[head]) + "/" + "/".join(segs[k:])
                    if os.path.exists(new) or head in dmap and os.path.exists(dmap[head]):
                        total += 1
                        return new + tail
            if p in dmap:
                total += 1
                return dmap[p] + tail
            if p in base or any(b.startswith(p + "/") for b in ()):
                unresolved[(f, p)] += 1
            return raw

        def link(m):
            label, target = m.group(1), m.group(2)
            moved = PAT.sub(sub, target)
            if moved == target:
                return m.group(0)
            old = target.split("#")[0].rstrip("/")
            new_path = moved.split("#")[0].rstrip("/")
            bare = label.rstrip("/")
            if bare and (old == bare or old.endswith("/" + bare)):
                n = bare.count("/") + 1
                label = "/".join(new_path.split("/")[-n:]) + ("/" if label.endswith("/") else "")
            return f"[`{label}`]({moved})"

        new = LINK.sub(link, text)
        new = PAT.sub(sub, new)

        here = os.path.dirname(f)

        def relative(m):
            target = m.group(1)
            if "://" in target or target.startswith(("#", "mailto:")):
                return m.group(0)
            path, frag = (target.split("#", 1) + [""])[:2]
            full = os.path.normpath(os.path.join(here, path))
            moved = PAT.sub(sub, full)
            if moved == full or not os.path.exists(moved):
                return m.group(0)
            rel = os.path.relpath(moved, here or ".")
            return "](" + rel + ("#" + frag if frag else "") + ")"

        new = re.sub(r"\]\(([^)\s]+)\)", relative, new)
        if new != text and not dry:
            open(f, "w").write(new)
    for (f, p), n in sorted(unresolved.items()):
        print(f"unresolved {f}: {p}")
    print(f"rewritten {total}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1:])

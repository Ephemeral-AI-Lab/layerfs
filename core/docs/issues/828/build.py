"""Combine the four mapping files into page data and check them against the owner rules.

Usage: python3 -I build.py <nav dir>
Writes <nav dir>/site/layout.json (one record per line) and prints a check report.
"""

import csv
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

NAV = Path(sys.argv[1])
LAYERS = ["core", "engine", "control", "api"]
ENTRY = {"mod.rs", "lib.rs", "main.rs", "__init__.py", "index.js"}
CHANGES = {"keep", "move", "rename", "merge", "split", "remove", "new"}


def expand_braces(s):
    """Expand one level of {a,b} groups, as written in the reviewers' exception lists."""
    i = s.find("{")
    if i < 0:
        return [s]
    j = s.index("}", i)
    return [x for alt in s[i + 1 : j].split(",") for x in expand_braces(s[:i] + alt + s[j + 1 :])]


def package_root(path):
    parts = path.split("/")
    if len(parts) > 2 and parts[1] == "crates":
        return "/".join(parts[:3])
    return "/".join(parts[:2])


def check_tree(paths, exceptions):
    """Return {folder: verdict dict} for every folder below a package root."""
    files = defaultdict(list)
    subs = defaultdict(set)
    roots = set()
    for p in paths:
        root = package_root(p)
        roots.add(root)
        parent = p.rsplit("/", 1)[0]
        files[parent].append(p.rsplit("/", 1)[1])
        d = parent
        while len(d) > len(root):
            up = d.rsplit("/", 1)[0]
            subs[up].add(d)
            d = up
    result = {}
    for folder in set(files) | set(subs):
        root = package_root(folder + "/x")
        if len(folder) <= len(root):
            continue
        loose = [f for f in files.get(folder, []) if f not in ENTRY]
        n_sub = len(subs.get(folder, ()))
        depth = folder[len(root) + 1 :].count("/") + 1
        if n_sub:
            kind = "branch"
            ok = len(loose) <= 4
            issue = None if ok else f"branch with {len(loose)} loose files (max 4)"
        else:
            kind = "leaf"
            if folder.rsplit("/", 1)[1] in ("support", "standalone") and "/tests/" in folder + "/":
                ok, issue = True, None  # RULE7: the 3-12 rule does not bind support/ or standalone/
            elif len(loose) > 20:
                ok, issue = False, f"{len(loose)} files (max 20)"
            elif len(loose) > 12:
                ok, issue = False, f"{len(loose)} files (aim 12)"
            elif len(loose) < 3 and folder.rsplit("/", 1)[1] in ("support", "standalone") and "/tests/" in folder + "/":  # RULE7
                ok, issue = True, None
            elif len(loose) < 3:
                ok, issue = False, f"only {len(loose)} file(s) (min 3)"
            else:
                ok, issue = True, None
        if depth > 4:
            ok, issue = False, f"folder depth {depth} (max 4)"
        exc = exceptions.get(folder)
        result[folder] = {
            "kind": kind,
            "loose": len(loose),
            "subs": n_sub,
            "ok": ok,
            "issue": issue,
            "exception": exc,
        }
    return result


def main():
    records = []
    report = {}
    all_exceptions = {}
    for layer in LAYERS:
        cur = list(csv.DictReader(open(NAV / f"current-{layer}.csv")))
        kinds = {r["path"]: (r["kind"], int(r["prod_loc"]) if r["prod_loc"] else None) for r in cur}
        mp = NAV / f"mapping-{layer}.json"
        if not mp.exists():
            print(f"{layer}: mapping missing")
            continue
        data = json.load(open(mp))
        rows = data["rows"]
        exceptions = {}
        for e in data.get("exceptions", []):
            for folder in expand_braces(e["folder"].rstrip("/")):
                exceptions[folder] = e["reason"]
        all_exceptions[layer] = exceptions
        problems = []
        olds = Counter(r["old"] for r in rows if r.get("old"))
        missing = [p for p in kinds if p not in olds]
        extra = [p for p in olds if p not in kinds]
        dups = [p for p, n in olds.items() if n > 1]
        if missing:
            problems.append(f"{len(missing)} current files not mapped, e.g. {missing[:3]}")
        if extra:
            problems.append(f"{len(extra)} old paths not in the tree, e.g. {extra[:3]}")
        if dups:
            problems.append(f"{len(dups)} old paths mapped twice, e.g. {dups[:3]}")
        # Trees are built from every layer's rows: a file may move across layers.
        every = [r for ly in LAYERS if (NAV / f"mapping-{ly}.json").exists()
                 for r in json.load(open(NAV / f"mapping-{ly}.json"))["rows"]]
        mine = [r for r in every if (r.get("new") or "").startswith(layer + "/")]
        news = Counter(r["new"] for r in mine if r["change"] != "merge")
        merge_targets = {r["new"] for r in mine if r["change"] == "merge"}
        ndups = [p for p, n in news.items() if n > 1]
        if ndups:
            problems.append(f"{len(ndups)} new paths used twice, e.g. {ndups[:3]}")
        bad = [r for r in rows if r["change"] not in CHANGES]
        if bad:
            problems.append(f"{len(bad)} rows with unknown change type")
        new_paths = [p for p in news] + [p for p in merge_targets if p not in news]
        verdicts = check_tree(new_paths, exceptions)
        cur_verdicts = check_tree(list(kinds), {})
        failing = {f: v for f, v in verdicts.items() if not v["ok"] and not v["exception"]}
        excepted = {f: v for f, v in verdicts.items() if not v["ok"] and v["exception"]}
        report[layer] = {
            "rows": len(rows),
            "changes": dict(Counter(r["change"] for r in rows)),
            "problems": problems,
            "failing": failing,
            "excepted": len(excepted),
            "current_failing": sum(1 for v in cur_verdicts.values() if not v["ok"]),
        }
        for r in rows:
            old, new = r.get("old"), r.get("new")
            kind, loc = kinds.get(old, (None, None)) if old else (None, None)
            if kind is None and new:
                seg = new.split("/")
                kind = "test" if ("tests" in seg or "test" in seg) else "prod"
            rec = {"l": layer, "o": old, "n": new, "c": r["change"], "k": kind}
            if loc is not None:
                rec["loc"] = loc
            if r.get("note"):
                rec["why"] = r["note"]
            records.append(rec)
    site = NAV / "site"
    site.mkdir(exist_ok=True)
    with open(site / "layout.json", "w") as fh:
        fh.write("[\n")
        fh.write(",\n".join(json.dumps(r, separators=(",", ":")) for r in records))
        fh.write("\n]\n")
    with open(site / "exceptions.json", "w") as fh:
        json.dump(all_exceptions, fh, indent=1)
    for layer, rep in report.items():
        print(f"== {layer}: {rep['rows']} rows {rep['changes']}")
        print(f"   folders breaking a rule today: {rep['current_failing']}; after: {len(rep['failing'])} (+{rep['excepted']} accepted exceptions)")
        for p in rep["problems"]:
            print("   PROBLEM:", p)
        for f, v in sorted(rep["failing"].items())[:25]:
            print("   FAIL:", f, "-", v["issue"])


main()

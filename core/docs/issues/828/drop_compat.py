"""Delete every `#828 compat` re-export (lines and compat modules); print a map
of what each one re-exported: <file>\t<module path or ''>\t<use text>."""
import re, subprocess, sys
MARK = "#828 compat"
files = subprocess.run(["git", "grep", "-l", MARK, "--", "*.rs"], capture_output=True, text=True).stdout.split()
ATTR = re.compile(r"^\s*#\[(doc\(hidden\)|allow\([^)]*\)|cfg\(.*\))\]\s*$")
report = []
for f in files:
    lines = open(f).read().split("\n")
    drop = set()
    i = 0
    while i < len(lines):
        l = lines[i]
        m = re.match(r"^(\s*)pub mod (\w+) \{\s*$", l)
        if m:
            j = i + 1
            while j < len(lines) and not lines[j].strip():
                j += 1
            if j < len(lines) and MARK in lines[j]:
                depth = 0
                k = i
                while True:
                    depth += lines[k].count("{") - lines[k].count("}")
                    if depth == 0 and k > i:
                        break
                    k += 1
                body = "\n".join(lines[i : k + 1])
                report.append((f, m.group(2), body))
                drop.update(range(i, k + 1))
                a = i - 1
                while a >= 0 and (ATTR.match(lines[a]) or MARK in lines[a]):
                    drop.add(a)
                    a -= 1
                i = k + 1
                continue
        if MARK in l and i not in drop:
            start = i
            if l.strip().startswith("//"):
                drop.add(i)
                i += 1
                continue
            if "use " not in l:
                while start > 0 and "use " not in lines[start]:
                    start -= 1
            report.append((f, "", "\n".join(lines[start : i + 1])))
            drop.update(range(start, i + 1))
            a = start - 1
            while a >= 0 and (ATTR.match(lines[a]) or (MARK in lines[a] and lines[a].strip().startswith("//"))):
                drop.add(a)
                a -= 1
        i += 1
    out = [l for n, l in enumerate(lines) if n not in drop]
    open(f, "w").write("\n".join(out))
for f, mod, body in report:
    print(f + "\t" + mod + "\t" + body.replace("\n", "\\n"))

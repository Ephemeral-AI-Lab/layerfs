import io
import re
import sys

"""Add a Windows wildcard arm to every unix-only match in a file.

Usage: python add_win_arm.py <file.rs>
A match whose arms are all cfg-gated away on Windows (non-exhaustive
there) gains a cfg(windows) catch-all arm.
"""

NL = chr(10)
H = chr(35)
Q = chr(34)
MATCH_RE = re.compile(r"\s*match\s+\w+\s*\{\s*$")

def add_win_arms(path):
    s = io.open(path, encoding="utf-8").read()
    lines = s.split(NL)
    out = []
    i = 0
    added = 0
    while i < len(lines):
        l = lines[i]
        out.append(l)
        if MATCH_RE.match(l):
            indent = len(l) - len(l.lstrip())
            j = i + 1
            arm_lines = []
            has_windows = False
            depth = 1
            while j < len(lines):
                inner = lines[j]
                st = inner.strip()
                depth += st.count("{") - st.count("}")
                if depth == 0:
                    break
                if "cfg(windows)" in inner or "Windows(" in inner:
                    has_windows = True
                arm_lines.append(inner)
                j += 1
            if arm_lines and not has_windows:
                for al in arm_lines:
                    if al.strip():
                        first_indent = len(al) - len(al.lstrip())
                        break
                pad = " " * first_indent
                out.append(pad + H + "[cfg(windows)]")
                out.append(pad + "_ => unreachable!(" + Q + "the Windows stub face has no runtime arm" + Q + "),")
                added += 1
            out.extend(arm_lines)
            if j < len(lines):
                out.append(lines[j])
            i = j + 1
            continue
        i += 1
    io.open(path, "w", encoding="utf-8", newline="").write(NL.join(out))
    print(path.split("/")[-1] + ":", added, "windows arms added")

for p in sys.argv[1:]:
    add_win_arms(p)

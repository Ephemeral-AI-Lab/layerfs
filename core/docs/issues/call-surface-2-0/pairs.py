"""The short/long side-by-side section, rendered from the family rows' own
examples (`families/<family>.json`, `examples.short[0]` and `examples.long`).

There is no second copy of any example here. `build.py` loads this file and
calls `render`; the key-set checks (long holds every short key, no empty
value in a short form) are build.py's assertions. Run alone it reads the
family files unchecked and writes pairs.html:

    python3 -I pairs.py <spec-src>
"""
import html
import json
import pathlib
import re
import sys

ORDER = ["project", "file", "sandbox", "workspace", "branch", "commit", "home", "machine", "store", "backup"]


def unlabel(line):
    """A short example line without its optional `label: ` prefix: (label, json text)."""
    line = line.strip()
    if line[:1] in "{[":
        return "", line
    at = line.find("{")
    if at < 0:
        return "", line
    return line[:at].rstrip().rstrip(":").rstrip(), line[at:]


def size(value):
    """Bytes of the value as one line of compact JSON."""
    return len(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode())


def short_ids(text):
    """Long hex ids shortened for reading; sizes are measured before this."""
    text = re.sub(r"sha256:[0-9a-f]{64}", lambda m: m.group(0)[:15] + "…", text)
    return re.sub(r"\b[0-9a-f]{32,66}\b", lambda m: m.group(0)[:8] + "…", text)


def show(value):
    return html.escape(short_ids(json.dumps(value, separators=(",", ":"), ensure_ascii=False)), quote=False)


def pairs_of(families):
    """[(family, [(call, cli, short value, long value)])] for every call with both examples."""
    groups = []
    for fam in families:
        rows = []
        for row in fam.get("calls", []):
            ex = row.get("examples") or {}
            shorts, long = ex.get("short") or [], ex.get("long")
            if not shorts or not long:
                continue
            try:
                rows.append((row["call"], row.get("cli", ""), json.loads(unlabel(shorts[0])[1]), json.loads(long)))
            except (ValueError, TypeError):
                continue  # build.py names the example that does not parse
        if rows:
            groups.append((fam["family"], rows))
    return groups


def render(families):
    """(html, short total, long total, pair count)."""
    out, short_total, long_total, count = [], 0, 0, 0
    for family, rows in pairs_of(families):
        out.append(f'      <h3>{html.escape(family)}</h3>')
        for call, cli, short, long in rows:
            s, l = size(short), size(long)
            short_total += s
            long_total += l
            count += 1
            out.append(
                '      <div class="pair">\n'
                f'        <div class="pair-head"><span class="call">{html.escape(call)}</span><code>{html.escape(cli, quote=False)}</code></div>\n'
                f'        <div class="side"><div class="side-l"><span class="tag">Short · agents</span><span class="bytes">{s:,} B</span></div><pre>{show(short)}</pre></div>\n'
                f'        <div class="side long"><div class="side-l"><span class="tag">Long · UI · --detail</span><span class="bytes">{l:,} B</span></div><pre>{show(long)}</pre></div>\n'
                '      </div>')
    body = "\n".join(out) + ("\n" if out else "")
    return body, short_total, long_total, count


def document(families):
    """pairs.html: a totals line, then the pairs."""
    body, short_total, long_total, count = render(families)
    return f"<!-- totals short={short_total} long={long_total} pairs={count} -->\n" + body


if __name__ == "__main__":
    S = pathlib.Path(sys.argv[1])
    fams = [json.loads(p.read_text()) for p in (S / "families" / f"{f}.json" for f in ORDER) if p.exists()]
    text = document(fams)
    (S / "pairs.html").write_text(text)
    sys.stderr.write(text.split("\n", 1)[0] + "\n")

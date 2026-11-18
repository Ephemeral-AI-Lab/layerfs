"""Build the frozen LayerFS 2.0 call-surface spec (version 18).

  python3 -I build.py <spec-src>                  check families/*.json against SCHEMA.md and
                                                  DECISIONS.md; write calls.json, pairs.html and
                                                  layerfs-call-surface.html
  python3 -I build.py <spec-src> --today <repo>   re-extract today-table.json from the code's own
                                                  call table (api/crates/layerfs-api/src) and stop
  python3 -I build.py <spec-src> --partial        a family file may be missing (testing only;
                                                  calls.json is not written)
  python3 -I build.py <spec-src> --draft          write the page even though rules fail
                                                  (calls.json is not written)
  ... --families <dir> --out <dir>                read family files from, and write outputs to,
                                                  another folder (testing only)

Inputs: families/<family>.json (the call rows; SCHEMA.md is their format), today-table.json
(today's 84 calls), DECISIONS.md, status.html, pg.html, extra.css, pairs.py, and
layerfs-call-surface.v6.html (CSS, three kept sections and the filter script).
Every failed rule is printed as `<call>: [<rule>] <what>`; any failure exits 1.
"""
import html
import importlib.util
import json
import keyword
import pathlib
import re
import subprocess
import sys

VERSION = 18
FROZEN = "2026-11-18"
BASE = "caff58a43b"
ORDER = ["project", "file", "sandbox", "workspace", "branch", "commit", "home", "machine", "store", "backup"]
KINDS = ["Name", "Project", "Text", "Int", "Bool", "Json", "Image", "Content", "Size", "Function"]
BADGE = {"kept": "b-kept", "extended": "b-extended", "renamed": "b-renamed", "reshaped": "b-renamed",
         "merged": "b-merged", "new": "b-new", "moved": "b-renamed", "paging": "b-renamed"}
ROW_FIELDS = ["call", "badge", "note", "today", "help", "cli", "positionals", "options", "stdin", "with", "paging",
              "progress", "quiet", "profile", "short", "long", "item", "examples", "dropped", "new_work"]
HTML_ONLY = ["note"]  # row fields left out of calls.json
THINGS = ["home", "machine", "store", "project", "sandbox", "workspace", "bucket"]
E = html.escape

# --- the frame: states, refusals, exit codes, options, shapes ------------------------------

# state, companion keys, when, what it replaces today
STATES = [
    ("ready", [], "Usable now. The one word for every thing, a running machine and a running Sandbox included.",
     "running, ready, idle, in_use, mounted, Active, Available"),
    ("busy", ["op"], "An operation holds it. Calls that need it are refused <code>Busy</code>.",
     "creating, removing, closing, Busy; and starting, stopping, mounting, committing, which are not recorded today"),
    ("stopped", [], "Off on purpose. Machines only.", "stopped"),
    ("blocked", ["blocked_by"], "Fine itself; the thing it lives on is not ready.",
     "a Store <code>down</code> with a prose <code>down_reason</code>; a Sandbox on a stopped machine reading \"Running\""),
    ("broken", ["reason", "fix"], "Needs a person. Docker not running is <code>broken</code> with <code>reason: engine_down</code>.",
     "failed, missing, unresolved, lost, gone, unsupported; Docker not running; the UI's \"Not answering\""),
    ("unknown", ["observed_at"], "A live read was asked for (<code>--with live</code>) and could not be made.",
     "unknown; a failed read shown as Down, closed or Removed"),
]
STATE_NAMES = [s[0] for s in STATES]
COMPANIONS = {s[0]: s[1] for s in STATES}
# every reason code; the first two are a blocker's own state, the rest are why a thing is broken
REASONS = ["stopped", "busy", "engine_down", "not_answering", "outdated", "unreachable", "missing", "failed",
           "unresolved", "lost", "unsupported"]
BROKEN_REASONS = REASONS[2:]
COMPANION_SHAPES = {
    "op": {"keys": ["name", "phase", "started_at"], "is": "the recorded operation: its call word, its phase, when it began"},
    "blocked_by": {"keys": ["<machine|store|sandbox>", "reason"],
                   "is": "the thing at the root of the block, by its own word, with that thing's state (stopped, busy) or its broken reason"},
    "reason": {"one_of": BROKEN_REASONS, "is": "why it is broken, as a code; the sentence is the long form's message"},
    "fix": {"is": "one runnable shell line; two commands are joined with &&"},
    "observed_at": {"is": "when LayerFS last knew its state: the last live read that answered, else when its record was last written"},
}

# the closed list a caller can meet; all thirteen are codes the code uses today
REFUSAL_CODES = [
    ("NotFound", "the named thing, Commit or path is not there"),
    ("Exists", "the name is taken, or the same database or bucket is already connected"),
    ("InUse", "held by something else: Sandboxes on a machine, a mounted Workspace, a login a Store uses"),
    ("Busy", "an operation is running on it; the refusal names the <code>op</code>"),
    ("Unresolved", "an earlier operation on it ended with its outcome not known"),
    ("Conflict", "the head moved, or a merge or Commit conflicts; the refusal names the head as <code>commit</code>"),
    ("Dirty", "uncommitted changes would be lost; the refusal lists the paths"),
    ("Capacity", "a stated limit: a Sandbox's Workspaces, a disk without room"),
    ("InvalidInput", "the request is wrong for this name or kind; nothing happened"),
    ("Unsupported", "not possible on this platform or engine, or not built"),
    ("Unavailable", "a machine, worker, server or bucket cannot be reached, or refused the login"),
    ("Failed", "it was tried and did not happen; whatever exists is kept"),
    ("Unknown", "the outcome is not known; read the state before acting again"),
]
CODE_NAMES = [c for c, _ in REFUSAL_CODES]
# today's other codes (caff58a43b has no closed list; these are the strings its crates refuse with)
TODAY_CODES = {
    "NotFound": ["NoSuchStore", "NoSuchProject", "NoSuchBranch", "NoSuchCommit", "NoSuchPath", "NoSuchSandbox",
                 "NoStore", "NoCommit", "NotInHistory", "NotMounted", "Removed"],
    "Exists": ["NameTaken"],
    "InUse": ["MountsActive"],
    "Conflict": ["HeadMoved"],
    "InvalidInput": ["NotFile", "IsDirectory", "NotDirectory", "NotEmpty", "Binary", "ReservedName", "ReadOnly",
                     "NoMergeBase", "TooLarge"],
    "Unavailable": ["Denied", "Unauthorized", "StaleSandbox", "StaleStore"],
    "Failed": ["UnknownMethod"],
}
EXIT_CODES = {
    "0": "done; a doctor exits 0 once its report is made, whatever it found",
    "2": "wrong command line; nothing was called",
    "125": "refused or failed; the refusal is one line of JSON on stderr",
    "command": "workspace exec exits with the command's own code (0 when it has none, 1 for a code outside 0 to 255) "
               "and prints its result JSON on stdout; a refusal still exits 125 with {\"error\",...} on stderr and nothing on stdout",
}
WITH_ORDER = ["live", "stats", "space", "docker", "platforms", "tree", "patch"]  # DECISIONS.md, "Simplicity changes adopted"
WITH_GONE = {"write": "R5", "leftovers": "R6", "storage": "folded into stats", "usage": "folded into stats"}
GLOBAL_OPTIONS = [
    {"name": "detail", "kind": "Bool", "cli": "--detail", "node": "detail: true", "python": "detail=True",
     "help": "the long form"},
    {"name": "with", "kind": "Text", "cli": "--with live,stats", "node": 'with: ["live", "stats"]',
     "python": 'with_=["live", "stats"]', "help": "adds named sections; only on the calls whose row lists them"},
    {"name": "on_progress", "kind": "Function", "cli": "--progress", "node": "on_progress: fn", "python": "on_progress=fn",
     "help": "progress events; only on the calls whose row says progress"},
    {"name": "text", "kind": "Bool", "cli": "--text", "node": None, "python": None,
     "help": "a readable result instead of JSON; shell only, as today"},
    {"name": "from", "kind": "Text", "cli": "--from <file>", "node": None, "python": None,
     "help": "a content or JSON positional read from a file instead of stdin; shell only, as today"},
    {"name": "scope", "kind": "Text", "cli": "--scope <folder> --scope-project <project> --scope-branch <branch>",
     "node": "scope", "python": "scope", "help": "the folder scope, as today"},
]
LIST_SHAPE = {
    "in": ["limit", "cursor"],
    "long": ["items", "next_cursor"],
    "short": {"always": ["items"], "sometimes": ["next_cursor"]},
    "default_limit": {"list": 100, "log": 200, "line": 2000},
    "max_limit": 1000,
    "page_bytes": 65536,
    "rules": ["next_cursor is left out of the short form at the end and is null in the long form",
              "a page ends early at page_bytes with a cursor, never Capacity",
              "the cursor carries the pins and filters of the call that made it",
              "a log pages from the newest line back, with the same options and shape"],
}
REFUSAL_SHAPE = {
    "short": {"always": ["error", "message"], "sometimes": ["fix"]},
    "long": ["error", "message", "fix", "detail"],
    "extra": "the fields the next call needs, flat beside them: a Conflict names the head as commit, "
             "a Busy or Unresolved names its op, a Dirty lists its paths",
    "stream": "stderr",
    "exit": 125,
    "error": CODE_NAMES,
    "today": "{\"code\",\"message\",\"detail\"}",
    "today_codes": TODAY_CODES,
}
PROGRESS_EVENT = ["phase", "done", "total"]

# the doctor answer: one shape for the six doctors
STATUSES = ["ok", "notice", "warning", "error", "not_checked"]  # a thing's status, and the whole answer's
OUTCOMES = ["pass", "notice", "warning", "error", "not_checked"]  # one check: passed, its finding's severity, or not made
COVERAGE = ["none", "shallow", "full"]  # what was checked on a thing in this run
WORST_FIRST = ["error", "warning", "not_checked", "notice", "ok"]
SEVERITIES = ["error", "warning", "notice"]
NOT_CHECKED_REASONS = ["timeout", "busy", "lost"]
CHILDREN = {"home": ["machines", "stores", "buckets"], "machine": ["stores", "sandboxes"], "store": ["projects"],
            "sandbox": ["workspaces"], "project": [], "bucket": [], "workspace": []}
DOCTOR_THING = {"home.doctor": "home", "machine.doctor": "machine", "store.doctor": "store",
                "project.doctor": "project", "sandbox.doctor": "sandbox", "backup.doctor": "bucket"}
FULL_DOCTORS = ["project.doctor", "backup.doctor"]  # the two with a Full check (R5)
DOCTOR_LONG = ["ok", "findings", "not_checked", "status", "counts", "observed_at", "tree"]
NODE_KEYS = ["status", "coverage", "checks", "blocked_by", "reason"]  # after the thing's own key; then its child lists
CHECK_KEYS = ["check", "outcome", "from"]
PHASES = ["self", "machines", "stores", "projects", "sandboxes", "workspaces", "buckets"]


def doctor_shape(calls):
    return {
        "calls": calls,
        "things": THINGS,
        "short": {"always": [], "sometimes": ["ok", "findings", "not_checked"]},
        "ok": "{\"ok\":true} alone, only when every check was made and nothing was found",
        "long": DOCTOR_LONG,
        "status": STATUSES,
        "worst_first": WORST_FIRST,
        "outcomes": OUTCOMES,
        "coverage": COVERAGE,
        "full": FULL_DOCTORS,
        "finding": {"short": {"always": ["<thing>", "check", "severity", "message"], "sometimes": ["fix", "from"]},
                    "long": ["<thing>", "check", "severity", "message", "fix", "from", "evidence"]},
        "severity": SEVERITIES,
        "not_checked": {"short": {"always": ["<thing>"], "one_of": ["blocked_by", "reason"], "sometimes": ["check"]},
                        "long": ["<thing>", "check", "blocked_by", "reason"]},
        "not_checked_reason": NOT_CHECKED_REASONS,
        "blocked_by": ["<thing>", "reason"],
        "blocked_by_reason": REASONS,
        "counts": STATUSES,
        "check": CHECK_KEYS,
        "node": ["<thing>"] + NODE_KEYS + ["<child lists>"],
        "children": {k: v for k, v in CHILDREN.items() if k != "workspace"},
        "progress_event": PROGRESS_EVENT + ["node"],
        "phases": PHASES,
        "exit": 0,
    }


# --- today's call table, from the code ------------------------------------------------------

def _close(src, at):
    """Index just past the bracket that closes the one opening at src[at]; strings and // comments are skipped."""
    pairs = {"(": ")", "[": "]", "{": "}"}
    depth, i = 0, at
    while i < len(src):
        ch = src[i]
        if ch == '"':
            i += 1
            while src[i] != '"':
                i += 2 if src[i] == "\\" else 1
        elif src.startswith("//", i):
            i = src.index("\n", i)
        elif ch in pairs:
            depth += 1
        elif ch in pairs.values():
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise ValueError("unbalanced brackets")


def _split(expr):
    """Top-level comma-separated parts of a Rust expression list."""
    parts, depth, i, start = [], 0, 0, 0
    while i < len(expr):
        ch = expr[i]
        if ch == '"':
            i += 1
            while expr[i] != '"':
                i += 2 if expr[i] == "\\" else 1
        elif expr.startswith("//", i):
            end = expr.find("\n", i)
            end = len(expr) if end < 0 else end
            expr = expr[:i] + expr[end:]
            continue
        elif ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append(expr[start:i].strip())
            start = i + 1
        i += 1
    parts.append(expr[start:].strip())
    return [p for p in parts if p]


def _rust_string(token):
    """The text of a Rust string literal (escapes and line continuations resolved)."""
    assert token[0] == '"' and token[-1] == '"', token
    body = re.sub(r"\\\n\s*", "", token[1:-1])
    return body.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\")


def _args(expr, consts):
    """A positional or option list: `&[a(..), opt(..), CONST]`, `&CONST` or `[..]`."""
    expr = expr.strip().lstrip("&").strip()
    if not expr.startswith("["):
        if expr not in consts:
            raise ValueError(f"unknown argument list `{expr}`")
        return [dict(a) for a in consts[expr]]
    out = []
    for item in _split(expr[1:-1]):
        m = re.fullmatch(r'(a|opt)\(\s*"(\w+)"\s*,\s*(?:Kind::)?(\w+)\s*,?\s*\)', item, re.S)
        if m:
            assert m.group(3) in KINDS, item
            out.append({"name": m.group(2), "kind": m.group(3), "optional": m.group(1) == "opt"})
        elif item in consts:
            out.extend(dict(a) for a in consts[item])
        else:
            raise ValueError(f"unknown argument `{item}`")
    return out


def _consts(src, known):
    """`const NAME: [Arg; n] = [..];` and `const NAME: Arg = opt(..);` in a Rust file."""
    found = dict(known)
    for m in re.finditer(r"const\s+(\w+):\s*(\[Arg;\s*\d+\]|&\[Arg\]|Arg)\s*=\s*", src):
        end = src.index(";", m.end()) if src[m.end()] not in "[&" else _close(src, src.index("[", m.end()))
        value = src[m.end():end].strip()
        found[m.group(1)] = _args(value if value.lstrip("&").startswith("[") else f"[{value}]", found)
    return found


def today_table(repo):
    """Today's call table: every `call(..)` entry under api/crates/layerfs-api/src, in help order."""
    repo = pathlib.Path(repo)
    src = repo / "api/crates/layerfs-api/src"
    shared = _consts((src / "calls/table.rs").read_text(), {})
    lib = (src / "lib.rs").read_text()
    folders = {mod: folder for folder, mod in re.findall(r'#\[path = "(\w+)/mod\.rs"\]\s*mod (\w+);', lib)}
    families = re.findall(r'Family\s*\{\s*name:\s*"(\w+)",\s*list:\s*(\w+)::calls', lib)
    calls, counts = [], []
    for family, mod in families:
        folder = src / folders.get(mod, mod)
        listing = (folder / "mod.rs").read_text()
        body = listing[listing.index("fn calls()"):]
        body = body[body.index("&[") + 1:]
        order = re.findall(r"(\w+)::CALL", body[: _close(body, 0)])
        for name in order:
            text = (folder / f"{name}.rs").read_text()
            m = re.search(r"const CALL: Call = call\(", text)
            assert m, f"{folder.name}/{name}.rs has no CALL entry"
            end = _close(text, m.end() - 1)
            parts = _split(text[m.end():end - 1])
            assert len(parts) == 6, (folder.name, name, parts)
            local = _consts(text[: m.start()], shared)
            tail = re.sub(r"\s+", "", text[end: text.index(";", end)])
            assert re.fullmatch(r"(\.(quiet|unopened)\(\))*", tail), (name, tail)
            entry = {
                "family": _rust_string(parts[0]),
                "call": _rust_string(parts[1]),
                "positionals": _args(parts[2], local),
                "options": [{"name": o["name"], "kind": o["kind"]} for o in _args(parts[3], local)],
                "quiet": ".quiet()" in tail,
                "help": _rust_string(parts[4]),
            }
            assert entry["family"] == family, (family, name)
            calls.append(entry)
        counts.append({"family": family, "calls": len(order)})
    base = subprocess.run(["git", "-C", str(repo), "rev-parse", "--short=10", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    # the code's own count assert, as a cross-check
    test = (repo / "api/crates/layerfs-api/tests/calls_table.rs").read_text()
    asserted = re.search(r"assert_eq!\(counts, \[([\d, ]+)\]\)", test)
    if asserted:
        assert [c["calls"] for c in counts] == [int(n) for n in asserted.group(1).split(",")], counts
    assert len(calls) == 84 and len(counts) == 12, (len(calls), len(counts))
    assert len({(c["family"], c["call"]) for c in calls}) == 84
    # the shell commands outside the table (`layerfs doctor ...`), from the CLI's own list
    cli = (repo / "api/crates/layerfs-cli/src/capabilities.rs").read_text()
    off = []
    for m in re.finditer(r"command\(", cli[cli.index("let commands = vec!["):]):
        text = cli[cli.index("let commands = vec!["):]
        parts = _split(text[m.end(): _close(text, m.end() - 1) - 1])
        strings = lambda part: [_rust_string(x) for x in _split(part.strip().lstrip("&").strip()[1:-1])]
        off.append({"command": _rust_string(parts[0]), "positionals": strings(parts[1]), "options": strings(parts[2])})
    assert any(c["command"] == "doctor" for c in off), off
    return {"base": base, "source": "api/crates/layerfs-api/src/<family>/<call>.rs",
            "families": counts, "calls": calls,
            "off_table_source": "api/crates/layerfs-cli/src/capabilities.rs", "off_table": off}


# --- checks ---------------------------------------------------------------------------------

def unlabel(line):
    """A short example without its optional `label: ` prefix: (label, json text)."""
    line = line.strip()
    if line[:1] in "{[":
        return "", line
    at = line.find("{")
    return (line[:at].rstrip().rstrip(":").rstrip(), line[at:]) if at >= 0 else ("", line)


def parse_cli(cli):
    """What a `cli` line shows: family, call, positionals [(name, optional)], options, --with values, redirect."""
    main, sep, _ = cli.partition(" < ")
    words = main.split()
    if len(words) < 3 or words[0] != "layerfs":
        raise ValueError("does not start with `layerfs <family> <call>`")
    shown = {"family": words[1], "call": words[2], "positionals": [], "options": [], "bare": [], "with": None,
             "redirect": bool(sep)}
    rest = main.split(None, 3)[3] if len(words) > 3 else ""
    tokens = re.findall(r"\[[^\]]*\]|\S+", rest)
    i = 0

    def option(text, bare=False):
        parts = text.split(None, 1)
        flag = parts[0]
        if flag == "-m":
            flag = "--message"
        name = flag[2:]
        if not re.fullmatch(r"[a-z][a-z0-9-]*", name):
            raise ValueError(f"cannot read option `{text}`")
        if name == "with":
            shown["with"] = [v for v in re.split(r"[,|]", parts[1])] if len(parts) > 1 else []
        shown["options"].append(name.replace("-", "_"))
        if bare:
            shown["bare"].append(name.replace("-", "_"))

    while i < len(tokens):
        tok = tokens[i]
        if tok.startswith("["):
            inner = tok[1:-1].strip()
            if inner.startswith("-"):
                option(inner)
            elif re.fullmatch(r"[a-z][a-z0-9_-]*", inner):
                shown["positionals"].append((inner.replace("-", "_"), True))
            else:
                raise ValueError(f"cannot read `{tok}`")
        elif tok.startswith("<"):
            m = re.fullmatch(r"<([a-z][a-z0-9_-]*)>", tok)
            if not m:
                raise ValueError(f"cannot read `{tok}`")
            shown["positionals"].append((m.group(1).replace("-", "_"), False))
        elif tok.startswith("-"):
            value = ""
            if i + 1 < len(tokens) and tokens[i + 1][0] not in "<[-":
                value = " " + tokens[i + 1]
                i += 1
            option(tok + value, bare=True)
        else:
            raise ValueError(f"cannot read `{tok}`")
        i += 1
    return shown


def walk(value, where=""):
    """Every dict inside a JSON value, with its path."""
    if isinstance(value, dict):
        yield where, value
        for k, v in value.items():
            yield from walk(v, f"{where}.{k}" if where else k)
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from walk(v, f"{where}[{i}]")


def empties(value, where=""):
    """Paths of null, false or empty values (a short form carries none)."""
    if isinstance(value, dict):
        for k, v in value.items():
            path = f"{where}.{k}" if where else k
            if v is None or v is False or v == "" or v == [] or v == {}:
                yield path
            else:
                yield from empties(v, path)
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from empties(v, f"{where}[{i}]")


def thing_key(entry):
    keys = [k for k in entry if k in THINGS]
    return keys[0] if len(keys) == 1 else None


def check_blocked_by(value):
    if not isinstance(value, dict) or set(value) - set(THINGS) != {"reason"} or len(value) != 2:
        return "blocked_by is {<thing>, reason}"
    if value["reason"] not in REASONS:
        return f"blocked_by.reason `{value['reason']}` is not a reason code"
    return None


def check_states(value, short, say):
    """The state table (C11) on every object of an example that carries `state`."""
    for where, obj in walk(value):
        if "state" not in obj:
            continue
        state = obj["state"]
        at = where or "the answer"
        if state not in STATE_NAMES:
            say(f"{at}: state `{state}` is not one of {', '.join(STATE_NAMES)}")
            continue
        for key in COMPANIONS[state]:
            if obj.get(key) is None:
                say(f"{at}: state {state} comes with `{key}`")
        if state == "blocked" and obj.get("blocked_by") is not None:
            problem = check_blocked_by(obj["blocked_by"])
            if problem:
                say(f"{at}: {problem}")
        if state == "broken" and obj.get("reason") is not None and obj["reason"] not in BROKEN_REASONS:
            say(f"{at}: reason `{obj['reason']}` is not one of {', '.join(BROKEN_REASONS)}")
        if short:
            for other in ("op", "blocked_by", "fix"):
                if other in obj and other not in COMPANIONS[state]:
                    say(f"{at}: state {state} does not come with `{other}`")


def check_node(node, kind, say, children=True):
    """One node of a doctor's tree (or of a progress event)."""
    if not isinstance(node, dict) or kind not in node:
        say(f"a {kind} node names its `{kind}`")
        return
    want = [kind] + NODE_KEYS + (CHILDREN[kind] if children else [])
    if set(node) != set(want):
        say(f"a {kind} node has exactly {want}, not {sorted(node)}")
        return
    if node["status"] not in STATUSES:
        say(f"node status `{node['status']}` is not one of {STATUSES}")
    if node["coverage"] not in COVERAGE:
        say(f"node coverage `{node['coverage']}` is not one of {COVERAGE}")
    for check in node["checks"] if isinstance(node["checks"], list) else []:
        if not isinstance(check, dict) or set(check) != set(CHECK_KEYS) or check["outcome"] not in OUTCOMES:
            say(f"a check entry is {{{', '.join(CHECK_KEYS)}}} with an outcome of {OUTCOMES}: {check}")
    if node["status"] == "not_checked" and (node["blocked_by"] is None) == (node["reason"] is None):
        say("a not_checked node carries blocked_by or reason, one of them")
    if children:
        for name in CHILDREN[kind]:
            child_kind = name[:-1] if name != "sandboxes" else "sandbox"
            for child in node[name]:
                check_node(child, child_kind, say, children=False)


def check_doctor_short(value, say):
    if not isinstance(value, dict) or not value or set(value) - {"ok", "findings", "not_checked"}:
        say("a doctor's short form is {\"ok\":true}, or findings and not_checked")
        return
    if "ok" in value and value != {"ok": True}:
        say("`ok` stands alone and is true: {\"ok\":true}")
    for f in value.get("findings", []):
        thing = thing_key(f) if isinstance(f, dict) else None
        if not thing:
            say(f"a finding names exactly one thing by its own word ({', '.join(THINGS)})")
            continue
        extra = set(f) - {thing, "check", "severity", "message", "fix", "from"}
        missing = {"check", "severity", "message"} - set(f)
        if extra or missing:
            say(f"a short finding is {{<thing>, check, severity, message}} plus fix and from when set: {sorted(f)}")
        if f.get("severity") not in SEVERITIES:
            say(f"severity `{f.get('severity')}` is not one of {SEVERITIES}")
    for n in value.get("not_checked", []):
        thing = thing_key(n) if isinstance(n, dict) else None
        if not thing:
            say("a not_checked entry names exactly one thing by its own word")
            continue
        if set(n) - {thing, "check", "blocked_by", "reason"} or ("blocked_by" in n) == ("reason" in n):
            say(f"a not_checked entry is {{<thing>}} with blocked_by or reason, one of them (R3): {sorted(n)}")
        if "reason" in n and n["reason"] not in NOT_CHECKED_REASONS:
            say(f"not_checked reason `{n['reason']}` is not one of {NOT_CHECKED_REASONS}")
        if "blocked_by" in n and check_blocked_by(n["blocked_by"]):
            say(check_blocked_by(n["blocked_by"]))


def check_doctor_long(call, value, say):
    if not isinstance(value, dict) or set(value) != set(DOCTOR_LONG):
        say(f"a doctor's long form has exactly {DOCTOR_LONG}")
        return
    if value["status"] not in STATUSES:
        say(f"status `{value['status']}` is not one of {STATUSES}")
    if value["ok"] is not (not value["findings"] and not value["not_checked"]):
        say("`ok` is true exactly when findings and not_checked are both empty")
    if not isinstance(value["counts"], dict) or set(value["counts"]) != set(STATUSES):
        say(f"counts has exactly {STATUSES}")
    for f in value["findings"]:
        thing = thing_key(f) if isinstance(f, dict) else None
        if not thing or set(f) != {thing, "check", "severity", "message", "fix", "from", "evidence"}:
            say("a long finding is {<thing>, check, severity, message, fix, from, evidence}")
        elif f["severity"] not in SEVERITIES:
            say(f"severity `{f['severity']}` is not one of {SEVERITIES}")
    for n in value["not_checked"]:
        thing = thing_key(n) if isinstance(n, dict) else None
        if not thing or set(n) != {thing, "check", "blocked_by", "reason"}:
            say("a long not_checked entry is {<thing>, check, blocked_by, reason}")
        elif (n["blocked_by"] is None) == (n["reason"] is None):
            say("a not_checked entry carries blocked_by or reason, one of them (R3)")
    tree = value["tree"]
    check_node(tree, DOCTOR_THING[call], say)
    if isinstance(tree, dict) and isinstance(value["counts"], dict):
        nodes = [tree] + [n for name in CHILDREN[DOCTOR_THING[call]] for n in tree.get(name) or [] if isinstance(n, dict)]
        seen = [n.get("status") for n in nodes]
        counted = {k: seen.count(k) for k in STATUSES}
        if set(value["counts"]) == set(STATUSES) and value["counts"] != counted:
            say(f"counts counts the things in the tree by status: the tree has {counted}")
        found = seen + [f.get("severity") for f in value["findings"] if isinstance(f, dict)] \
            + (["not_checked"] if value["not_checked"] else [])
        worst = next((w for w in WORST_FIRST if w in found), "ok")
        if value["status"] in STATUSES and value["status"] != worst:
            say(f"status is the worst of its things and findings: `{worst}` here, not `{value['status']}`")


ISO = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?Z")
SINGULAR_S = {"status", "cpus", "class", "address", "progress", "access", "alias", "basis", "bus", "canvas", "ms",
              "exists", "holders"}
ENUM_KEYS = ["state", "outcome", "type", "engine", "change", "severity", "status", "via", "activity", "backend",
             "route", "phase", "profile", "platform"]


def naming(call, value, warn, top=True):
    """Field naming on an example (SCHEMA.md, last rule). Heuristics, so warnings."""
    for where, obj in walk(value):
        for k, v in obj.items():
            at = f"{where}.{k}" if where else k
            if k.endswith("_at") and v is not None and not (isinstance(v, str) and (ISO.fullmatch(v) or v == "…")):
                warn(f"{call}: [naming] {at} ends in _at and is not ISO-8601 UTC: {v!r}")
            if (k.endswith("_ms") or k.endswith("_bytes") or k.endswith("_count")) and v is not None and not (
                    isinstance(v, int) and not isinstance(v, bool)):
                warn(f"{call}: [naming] {at} is not a whole number: {v!r}")
            if k.endswith("s_count") and k[: -len("_count")] not in obj:
                warn(f"{call}: [naming] {at} stands beside no `{k[:-6]}` list")
            if k.endswith("s") and k not in SINGULAR_S and not k.endswith(("_ms", "_bytes")) and isinstance(v, (int, float)) \
                    and not isinstance(v, bool):
                warn(f"{call}: [naming] {at} is a plural noun with a number; a plural is a list")
            if k in ENUM_KEYS and isinstance(v, str) and v != v.lower():
                warn(f"{call}: [naming] {at} is an enum value and is not lowercase: {v!r}")
            if k == "name" and where.rsplit(".", 1)[-1].split("[")[0] not in ("op",):
                warn(f"{call}: [naming] {at}: a thing goes by its own word, never `name`")


def check_family(path, fam, today, state):
    """Every rule of SCHEMA.md on one family file. Appends to state['errors'] and state['warnings']."""
    errors, warnings = state["errors"], state["warnings"]
    stem = path.stem
    for key in ("family", "who", "old_count", "blurb", "reasons", "calls"):
        if key not in fam:
            errors.append(f"{stem}.json: [fields] the family lacks `{key}`")
    if fam.get("family") != stem:
        errors.append(f"{stem}.json: [fields] `family` is {fam.get('family')!r}, not the file's name")
    old = sum(1 for c in today.values() if c["family"] == stem)
    if fam.get("old_count") != old:
        errors.append(f"{stem}.json: [today] old_count is {fam.get('old_count')}; the code's table has {old} {stem} calls")
    if not isinstance(fam.get("reasons", []), list) or not isinstance(fam.get("calls", []), list):
        errors.append(f"{stem}.json: [fields] `reasons` and `calls` are lists")
        return
    for row in fam.get("calls", []):
        check_row(stem, row, today, state)


def check_row(family, row, today, state):
    call = row.get("call", f"{family}.?")
    errors, warnings = state["errors"], state["warnings"]

    def err(rule, what):
        errors.append(f"{call}: [{rule}] {what}")

    missing = [f for f in ROW_FIELDS if f not in row]
    if missing:
        err("fields", f"the row lacks {', '.join(missing)}")
    if not re.fullmatch(rf"{family}\.[a-z][a-z0-9_]*", call):
        err("fields", f"a {family} call is named `{family}.<snake_case>`")
    if call in state["calls"]:
        err("fields", "the call is listed twice")
    state["calls"][call] = row
    if row.get("badge") not in BADGE:
        err("fields", f"badge `{row.get('badge')}` is not one of {', '.join(BADGE)}")
    if row.get("profile") not in ("agent", "full"):
        err("fields", f"profile `{row.get('profile')}` is not agent or full")
    if row.get("stdin") not in ("content", "json", "secret", None):
        err("fields", f"stdin `{row.get('stdin')}` is not content, json, secret or null")
    if row.get("paging") not in ("cursor", "line", "byte", None):
        err("fields", f"paging `{row.get('paging')}` is not cursor, line, byte or null")
    for flag in ("progress", "quiet"):
        if not isinstance(row.get(flag), bool):
            err("fields", f"`{flag}` is true or false")
    help_line = row.get("help")
    if not isinstance(help_line, str) or not help_line.strip() or "\n" in help_line or re.search(r"</?[a-z]+>", help_line):
        err("fields", "`help` is one line of plain text")
    if not isinstance(row.get("note", ""), str):
        err("fields", "`note` is text")
    if row.get("new_work") is not None and not isinstance(row.get("new_work"), str):
        err("fields", "`new_work` is plain text or null")

    # kinds and names
    positionals = [p for p in row.get("positionals") or [] if isinstance(p, dict)]
    options = [o for o in row.get("options") or [] if isinstance(o, dict)]
    for arg in positionals + options:
        name = arg.get("name", "")
        if arg.get("kind") not in KINDS:
            err("kind", f"`{name}` has kind `{arg.get('kind')}`; the code's kinds are {', '.join(KINDS)}")
        if not re.fullmatch(r"[a-z][a-z0-9_]*", str(name)):
            err("kind", f"`{name}` is not snake_case")
    names = [a.get("name") for a in positionals + options]
    for name in sorted({n for n in names if names.count(n) > 1}):
        err("kind", f"`{name}` is listed twice among the positionals and options")
    optional_seen = False
    for p in positionals:
        if p.get("optional"):
            optional_seen = True
        elif optional_seen:
            err("cli", f"required positional `{p.get('name')}` follows an optional one")
    option_names = [o.get("name") for o in options]
    withs = row.get("with") if isinstance(row.get("with"), dict) else {}

    # cli holds exactly the row's positionals and options
    cli = row.get("cli", "")
    try:
        shown = parse_cli(cli)
    except ValueError as why:
        err("cli", f"{why}: {cli}")
        shown = None
    if shown:
        if shown["family"] != family or shown["call"].replace("-", "_") != call.split(".", 1)[-1] or "_" in shown["call"]:
            err("cli", f"the command is `layerfs {family} {call.split('.', 1)[-1].replace('_', '-')}`, not `layerfs {shown['family']} {shown['call']}`")
        piped = [p["name"] for p in positionals
                 if p.get("kind") == "Content" or (p.get("kind") == "Json" and row.get("stdin") == "json")]
        want = [(p.get("name"), bool(p.get("optional"))) for p in positionals]
        want_unpiped = [w for w in want if w[0] not in piped]
        if shown["positionals"] != want and not (shown["redirect"] and shown["positionals"] == want_unpiped):
            def spell(ps):
                return " ".join(f"[{n}]" if o else f"<{n}>" for n, o in ps) or "none"
            err("cli", f"positionals in cli are {spell(shown['positionals'])}; the row's are {spell(want)}")
        if shown["redirect"] and row.get("stdin") is None:
            err("cli", "cli reads stdin (`< …`) and `stdin` is null")
        allowed = set()
        if withs or "with" in option_names:
            allowed.add("with")
        if row.get("progress") is True:
            allowed.add("progress")
        required = {o["name"] for o in options if o.get("kind") != "Function"}
        every = required | {o["name"] for o in options}
        cli_options = set(shown["options"])
        for name in sorted(cli_options - every - allowed):
            err("cli", f"cli has --{name.replace('_', '-')}, which is not an option of the row")
        for name in sorted(required - cli_options):
            err("cli", f"option `{name}` is not in cli")
        for o in options:
            if o.get("required") is True and o.get("name") in cli_options and o["name"] not in shown["bare"]:
                err("cli", f"option `{o['name']}` is required, so cli shows it without brackets")
        if withs and shown["with"] is None:
            err("cli", f"the row takes --with {','.join(withs)} and cli does not show --with")
        if shown["with"] is not None:
            if not withs:
                err("cli", "cli shows --with and the row's `with` is empty")
            for value in shown["with"]:
                if value not in withs:
                    err("cli", f"cli shows --with {value}, which is not a section of the row")
        if re.search(r"--[a-z0-9-]*_", cli):
            err("cli", "a shell option is the name with `-` for `_`")

    # with sections
    for name, section in withs.items():
        if name in WITH_GONE:
            err("with", f"`{name}` is not a --with value ({WITH_GONE[name]})")
        elif name not in WITH_ORDER:
            err("with", f"`{name}` is not a --with value; DECISIONS.md lists {', '.join(WITH_ORDER)}")
        if not isinstance(section, dict) or not section.get("keys") or not all(isinstance(k, str) for k in section["keys"]):
            err("with", f"section `{name}` lists its keys")
            continue
        if section.get("where") not in (None, "own", "item", "top"):
            err("with", f"section `{name}`: where is `item`, `top` or left out (its own key)")
        if section.get("where") == "item" and not row.get("item"):
            err("with", f"section `{name}` adds keys to each item and the row has no `item`")
        state["with"].setdefault(name, []).append(call)
    if not isinstance(row.get("with"), dict):
        err("with", "`with` is an object; {} when none")

    # short, long, item
    def keyset(form, where):
        if not isinstance(form, dict) or not isinstance(form.get("always"), list) or not isinstance(form.get("sometimes"), list):
            err("short", f"{where} is {{always: [...], sometimes: [...]}}")
            return [], []
        if set(form["always"]) & set(form["sometimes"]):
            err("short", f"{where}: {sorted(set(form['always']) & set(form['sometimes']))} is in both always and sometimes")
        return form["always"], form["sometimes"]

    always, sometimes = keyset(row.get("short"), "short")
    long = row.get("long") if isinstance(row.get("long"), list) else []
    if not long:
        err("long", "`long` is the complete, ordered key list of the long form")
    if len(set(long)) != len(long):
        err("long", "a key is listed twice in `long`")
    for key in always + sometimes:
        if key not in long:
            err("long ⊇ short", f"short key `{key}` is not in `long`")
    if not always and not sometimes:
        err("short", "the short form has no key; an action with nothing to return has short.always == [\"ok\"]")
    if "ok" in always and (always != ["ok"] or sometimes):
        err("short", "`ok` in short.always stands alone, with nothing in sometimes: a call that never has news answers "
            "{\"ok\":true}; a call that sometimes has news lists `ok` in short.sometimes")
    item = row.get("item")
    item_always, item_sometimes, item_long, scalar = [], [], [], False
    if item is not None:
        if not isinstance(item, dict):
            err("item", "`item` is null, {scalar}, or {short, long}")
            item = None
        elif "scalar" in item:
            scalar = True
            if set(item) != {"scalar"} or not isinstance(item["scalar"], str):
                err("item", "a list of plain strings is {\"scalar\": \"<what each string is>\"}")
        else:
            item_always, item_sometimes = keyset(item.get("short"), "item.short")
            item_long = item.get("long") if isinstance(item.get("long"), list) else []
            if not item_long:
                err("item", "`item.long` lists each row's keys")
            for key in item_always + item_sometimes:
                if key not in item_long:
                    err("long ⊇ short", f"item short key `{key}` is not in `item.long`")
        if "items" not in long:
            err("item", "a call with `item` answers {\"items\":[…]}: `items` is not in `long`")
    elif "items" in long:
        err("item", "`items` is in `long` and the row has no `item`")
    own_sections = {n for n, s in withs.items() if isinstance(s, dict) and s.get("where") in (None, "own")}
    top_keys = {k for s in withs.values() if isinstance(s, dict) and s.get("where") == "top" for k in s.get("keys", [])}
    item_keys = {k for s in withs.values() if isinstance(s, dict) and s.get("where") == "item" for k in s.get("keys", [])}

    # examples
    examples = row.get("examples") if isinstance(row.get("examples"), dict) else {}
    shorts = examples.get("short")
    if not isinstance(shorts, list) or not shorts or not all(isinstance(s, str) for s in shorts):
        err("examples.short", "`examples.short` is a list of one or more JSON lines")
        shorts = []
    is_doctor = call in DOCTOR_THING
    for n, line in enumerate(shorts, 1):
        label, text = unlabel(line)
        tag = f"line {n}" + (f" ({label})" if label else "")
        try:
            value = json.loads(text)
        except ValueError as why:
            err("examples.short", f"{tag} does not parse as JSON: {why}")
            continue
        if not isinstance(value, dict):
            err("examples.short", f"{tag} is not an object")
            continue
        for key in value:
            if key not in always and key not in sometimes and key not in own_sections and key not in top_keys:
                err("examples.short", f"{tag} uses `{key}`, which is not a key of `short`")
        for key in always:
            if key not in value:
                err("examples.short", f"{tag} lacks `{key}`, which is in short.always")
        if not value:
            err("examples.short", f"{tag} is empty; an answer with no news is {{\"ok\":true}}")
        if "ok" in value and value != {"ok": True}:
            err("examples.short", f"{tag}: `ok` appears only alone, as {{\"ok\":true}}; it is dropped when anything else remains")
        for path in empties(value):
            err("examples.short", f"{tag}: `{path}` is null, false or empty; a short form leaves it out")
        if item is not None and isinstance(value.get("items"), list):
            for i, entry in enumerate(value["items"]):
                if scalar:
                    if not isinstance(entry, str):
                        err("examples.short", f"{tag}: items[{i}] is not a plain string")
                    continue
                if not isinstance(entry, dict):
                    err("examples.short", f"{tag}: items[{i}] is not an object")
                    continue
                for key in entry:
                    if key not in item_always and key not in item_sometimes and key not in item_keys:
                        err("examples.short", f"{tag}: items[{i}] uses `{key}`, which is not a key of `item.short`")
                for key in item_always:
                    if key not in entry:
                        err("examples.short", f"{tag}: items[{i}] lacks `{key}`, which is in item.short.always")
        check_states(value, True, lambda what: err("state", f"short {tag}: {what}"))
        if is_doctor:
            check_doctor_short(value, lambda what: err("doctor", f"short {tag}: {what}"))
        naming(call, value, warnings.append)
    long_text = examples.get("long")
    if not isinstance(long_text, str):
        err("examples.long", "`examples.long` is one JSON object as text")
    else:
        try:
            value = json.loads(long_text)
        except ValueError as why:
            err("examples.long", f"does not parse as JSON: {why}")
            value = None
        if value is not None and not isinstance(value, dict):
            err("examples.long", "is not an object")
        elif value is not None:
            if set(value) != set(long):
                more, less = sorted(set(value) - set(long)), sorted(set(long) - set(value))
                err("examples.long", "its keys are not exactly `long`:"
                    + (f" extra {', '.join(more)};" if more else "") + (f" missing {', '.join(less)}" if less else ""))
            if item is not None and isinstance(value.get("items"), list):
                for i, entry in enumerate(value["items"]):
                    if scalar:
                        if not isinstance(entry, str):
                            err("examples.long", f"items[{i}] is not a plain string")
                    elif not isinstance(entry, dict) or set(entry) != set(item_long):
                        got = set(entry) if isinstance(entry, dict) else set()
                        more, less = sorted(got - set(item_long)), sorted(set(item_long) - got)
                        err("examples.long", f"items[{i}] does not have exactly `item.long`:"
                            + (f" extra {', '.join(more)};" if more else "") + (f" missing {', '.join(less)}" if less else ""))
            check_states(value, False, lambda what: err("state", f"long: {what}"))
            if "ok" in value and not is_doctor and value["ok"] is not True:
                err("examples.long", "`ok` is always true in the long form of a call that is not a doctor")
            if is_doctor:
                check_doctor_long(call, value, lambda what: err("doctor", f"long: {what}"))
            naming(call, value, warnings.append)
    for key in long + (item_long if item else []):
        if key == "name":
            warnings.append(f"{call}: [naming] `name` is a key; a thing goes by its own word")

    # today and dropped
    todays = row.get("today") if isinstance(row.get("today"), list) else []
    if not isinstance(row.get("today"), list):
        err("today", "`today` is a list of today's call names; [] when new")
    if (row.get("badge") == "new") != (not todays):
        err("today", "badge `new` and an empty `today` go together")
    dropped = row.get("dropped") if isinstance(row.get("dropped"), list) else []
    if not isinstance(row.get("dropped"), list):
        err("dropped", "`dropped` is a list; [] when nothing was dropped")
    # each dropped entry: tokens joined by " · ", each `--opt`, `<pos>` or `[pos]`, with an optional ` (old.call, …)`
    drops = []  # (token, the old calls it is about or None for all of the row's)
    for d in dropped:
        if not isinstance(d, dict) or not isinstance(d.get("today"), str) or not d.get("today").strip() or not d.get("why") \
                or "now" not in d:
            err("dropped", f"a dropped entry is {{today, now, why}}: {d}")
            continue
        for part in re.split(r" · (?![^()]*\))", d["today"]):
            m = re.fullmatch(r"(--[a-z][a-z0-9-]*|<[a-z][a-z0-9_]*>|\[[a-z][a-z0-9_]*\])(?: \(([^()]+)\))?", part.strip())
            if not m:
                err("dropped", f"`{part}` is not `--opt`, `<pos>` or `[pos]`, with an optional ` (old.call, …)`")
                continue
            if m.group(2) and m.group(2).startswith("layerfs "):
                # an option of a shell command outside the call table (`layerfs doctor ...`)
                known = {o for c in state["off_table"] if c["command"].startswith("doctor") for o in c["options"]}
                if not m.group(2).startswith("layerfs doctor"):
                    err("dropped", f"`{part}`: the only shell command outside the table with options is `layerfs doctor`")
                elif m.group(1).startswith("--") and m.group(1) not in known:
                    err("dropped", f"`{part}`: `layerfs doctor` takes {', '.join(sorted(known))} today, not {m.group(1)} (C3)")
                continue
            scope = [c.strip() for c in m.group(2).split(",")] if m.group(2) else None
            for named in scope or []:
                if named not in todays:
                    err("dropped", f"`{part}` names {named}, which is not in the row's `today`")
            drops.append((m.group(1), scope))

    def token(arg, is_option):
        if is_option:
            return "--" + arg["name"].replace("_", "-")
        return f"[{arg['name']}]" if arg.get("optional") else f"<{arg['name']}>"

    new_positionals = {p.get("name") for p in positionals}
    new_options = set(option_names)
    old_tokens = {}
    for old in todays:
        if old not in today:
            err("today", f"`{old}` is not a call of today's table")
            continue
        state["covered"].setdefault(old, []).append(call)
        entry = today[old]
        for is_option, arg in [(False, a) for a in entry["positionals"]] + [(True, a) for a in entry["options"]]:
            tok = token(arg, is_option)
            old_tokens.setdefault(old, set()).add(tok)
            if arg["name"] == "on_progress":
                if row.get("progress") is not True and not any(t == tok for t, _ in drops):
                    err("dropped", f"today's {old} reports progress (`on_progress`); the row's `progress` is false and it is not in `dropped`")
                continue
            listed = any(t == tok and (scope is None or old in scope) for t, scope in drops)
            kept = arg["name"] in (new_options if is_option else new_positionals)
            moved = arg["name"] in (new_positionals if is_option else new_options)
            if kept or listed:
                continue
            if moved:
                err("dropped", f"today's {old} takes `{tok}`; the row has `{arg['name']}` as "
                    f"{'a positional' if is_option else 'an option'}, so `dropped` lists `{tok}`")
            else:
                err("dropped", f"today's {old} takes `{tok}`; it is neither in the row nor in `dropped`")
    for tok, scope in drops:
        about = scope or todays
        if not any(tok in old_tokens.get(old, ()) for old in about):
            err("dropped", f"`{tok}` is not an argument of {', '.join(about) or 'any call'} in the code's table (C3)")

    # paging
    paging = row.get("paging")
    limit = next((o for o in options if o.get("name") == "limit"), None)
    if "before" in option_names:
        err("paging", "no call takes --before; a log pages with --cursor")
    if "offline" in option_names:
        err("decision R4", "no doctor takes --offline")
    if paging == "cursor":
        if "limit" not in option_names or "cursor" not in option_names:
            err("paging", "a cursor-paged call takes --limit and --cursor")
        if "next_cursor" not in long:
            err("paging", "a cursor-paged call answers `next_cursor` in the long form")
        if "next_cursor" in always:
            err("paging", "`next_cursor` is left out at the end, so it is in short.sometimes")
        if item is None:
            err("paging", "a cursor-paged call has an `item`")
        want = 200 if call.endswith(".log") else 100
        if limit is not None and limit.get("default") != want:
            err("paging", f"--limit defaults to {want} here (100 for lists, 200 for logs), not {limit.get('default')}")
    elif paging == "line":
        if "line" not in option_names or "limit" not in option_names:
            err("paging", "a line window takes --line and --limit")
        if limit is not None and limit.get("default") != 2000:
            err("paging", f"--limit defaults to 2000 lines for a text window, not {limit.get('default')}")
        if "next_line" not in long:
            err("paging", "a line window answers `next_line`")
    elif paging is None:
        if "cursor" in option_names:
            err("paging", "the row takes --cursor and `paging` is null")
        if item is not None and not str(row.get("note", "")).strip():
            err("paging", "an unpaged list says in its note why it is bounded")
    if call.endswith(".log") and paging != "cursor":
        err("paging", "a log call pages with --limit and --cursor")
    if call in ("file.glob", "file.grep") and paging != "cursor":
        err("paging", f"{call} gains --cursor")

    # a thing's get carries its state; a list row leaves `ready` out
    if call in ("machine.get", "store.get", "project.get", "sandbox.get") and "state" not in always:
        err("state", "a `get` always answers `state`: it is not in short.always (C11)")
    if item is not None and "state" in item_always:
        err("state", "a list row leaves `state` out when ready: it belongs in item.short.sometimes")

    if is_doctor:
        if always or set(sometimes) != {"ok", "findings", "not_checked"}:
            err("doctor", "a doctor's short is {always: [], sometimes: [ok, findings, not_checked]}")
        if set(long) != set(DOCTOR_LONG):
            err("doctor", f"a doctor's long is {DOCTOR_LONG}")
        if ("full" in option_names) != (call in FULL_DOCTORS):
            err("decision R5", "only `project doctor` and `backup doctor` take --full")
        if call not in FULL_DOCTORS and row.get("progress") is not True:
            err("doctor", "a doctor that covers more than one thing reports progress, one event per finished thing")

    # quiet, progress and profile lists for the rules
    for flag, value in (("quiet", row.get("quiet")), ("progress", row.get("progress")), ("agent", row.get("profile") == "agent")):
        if value is True:
            state[flag].append(call)

    for text in (row.get("note"), row.get("help"), row.get("new_work"), cli):
        if isinstance(text, str) and re.search(r"LAYERFS_(PROJECT|BRANCH|WORKSPACE)", text):
            err("decision", "no context environment variables")


def has(state, call, kind, names, rule, what):
    """A decision check on one row: its positionals or options hold `names`."""
    row = state["calls"].get(call)
    if row is None:
        return
    held = [a.get("name") for a in row.get(kind) or [] if isinstance(a, dict)]
    lacking = [n for n in names if n not in held]
    if lacking:
        state["errors"].append(f"{call}: [decision {rule}] {what}: {kind} lack {', '.join(lacking)}")


def check_decisions(state, partial, totals):
    """The rulings and corrections of DECISIONS.md that name a call."""
    errors, calls = state["errors"], state["calls"]

    def need(cond, call, rule, what):
        if call in calls and not cond(calls[call]):
            errors.append(f"{call}: [decision {rule}] {what}")

    def names(row, kind):
        return [a.get("name") for a in row.get(kind) or [] if isinstance(a, dict)]

    has(state, "workspace.unmount", "options", ["discard", "force"], "C1", "keeps --discard and --force")
    for call in ("workspace.commit", "sandbox.remove", "machine.stop"):
        has(state, call, "options", ["force"], "C1", "keeps --force")
    for call in ("file.write", "file.edit", "file.rm", "file.mkdir", "file.mv", "file.cp", "file.apply", "workspace.commit"):
        has(state, call, "options", ["message"], "C2", "-m on every call that makes a Commit")
    for call in ("file.write", "file.mkdir"):
        has(state, call, "options", ["mode"], "C2", "keeps mode")
    need(lambda r: [(p.get("name"), bool(p.get("optional"))) for p in r.get("positionals") or []] == [("machine", False), ("image", True)]
         and "unused" in names(r, "options"), "machine.prune", "C4", "is `machine prune <machine> [image] [--unused]`")
    has(state, "machine.update", "options", ["cpus", "memory", "image", "docker", "wsl", "daemon"], "C5", "keeps its six options")
    need(lambda r: "disk" not in names(r, "options"), "machine.update", "C5", "takes no --disk; resizing is not in 2.0")
    need(lambda r: names(r, "positionals") == ["machine", "image"], "machine.load_image", "C6",
         "is `machine load-image <machine> <image>`")
    need(lambda r: r.get("new_work"), "branch.merge", "C8", "merging into an existing Branch is new work: `new_work` says so")
    has(state, "branch.publish", "options", ["expected"], "C8", "--expected guards main's head")
    has(state, "backup.doctor", "options", ["full"], "R5", "is `backup doctor <bucket> --full`")
    has(state, "project.doctor", "options", ["full"], "R5", "takes --full")
    need(lambda r: names(r, "positionals") == ["leftover"], "home.prune", "R6", "is `home prune <leftover>`")
    has(state, "home.leftovers", "options", ["machine"], "R6", "is `home leftovers [--machine M]`")
    need(lambda r: names(r, "positionals") == ["workspace"] and "exec" in names(r, "options"), "workspace.cancel", "R7",
         "is `workspace cancel <workspace> [--exec E]`")
    need(lambda r: r.get("profile") == "agent", "workspace.list", "R7", "joins the agent profile")
    has(state, "store.doctor", "options", ["machine"], "R9", "--machine M stays for the command line")
    need(lambda r: r.get("with") == {}, "project.get", "simplicity", "takes no --with")
    need(lambda r: "stats" in (r.get("with") or {}), "project.list", "simplicity", "takes --with stats")
    need(lambda r: "stats" in (r.get("with") or {}), "sandbox.get", "simplicity", "takes --with stats")
    need(lambda r: "space" in (r.get("with") or {}), "machine.stats", "simplicity", "keeps --with space")
    has(state, "store.connect", "options", ["auth", "machine"], "simplicity", "takes --auth (first connect) and --machine")
    need(lambda r: not r.get("positionals") and "bucket" in names(r, "options"), "backup.list", "simplicity",
         "is `backup list [--bucket B]`")
    need(lambda r: names(r, "positionals") == ["project", "ref"] and "from" in names(r, "options"), "commit.diff", "simplicity",
         "is `commit diff <project> <ref> [--from R]`")
    need(lambda r: "generation" in (r.get("long") or []) and "changes" not in (r.get("long") or []), "home.get", "simplicity",
         "answers `generation`, not `changes`")
    if "sandbox.close" in calls:
        errors.append("sandbox.close: [decision simplicity] is renamed sandbox.remove")
    if partial or state["missing"]:
        return  # a missing family file is already a failure; what it would hold is not reported again
    for call, rule in (("home.leftovers", "R6"), ("sandbox.remove", "simplicity"), ("workspace.list", "R7"),
                       ("home.doctor", "R1"), ("project.doctor", "R8")):
        if call not in calls:
            errors.append(f"{call}: [decision {rule}] the call is not in the table")
    for call in DOCTOR_THING:
        if call not in calls:
            errors.append(f"{call}: [doctor] the six doctors are {', '.join(DOCTOR_THING)}")
    used = [w for w in WITH_ORDER if w in state["with"]]
    if used != WITH_ORDER:
        errors.append(f"--with: [with] DECISIONS.md lists {', '.join(WITH_ORDER)}; no row takes "
                      f"{', '.join(w for w in WITH_ORDER if w not in used)}")
    counts = {f: sum(1 for c in calls if c.startswith(f + ".")) for f in ORDER}
    if totals and (counts != totals["families"] or len(calls) != totals["calls"]):
        got = ", ".join(f"{f} {n}" for f, n in counts.items())
        errors.append(f"totals: [decision totals] DECISIONS.md says {totals['calls']} calls ({totals['text']}); "
                      f"the rows have {len(calls)} ({got})")


def decision_totals(md):
    m = re.search(r"(\d+) calls in (\d+) families:\s*([^.]+)\.", md)
    if not m:
        return None
    families = {name: int(n) for name, n in re.findall(r"(\w+) (\d+)", m.group(3))}
    return {"calls": int(m.group(1)), "families": families, "text": re.sub(r"\s+", " ", m.group(3))}


# --- the frame's own examples (checked like the rows') -------------------------------------

T = "2026-11-18T09:16:59Z"
OUTDATED_FIX = "layerfs machine stop demo-build && layerfs machine start demo-build"


def _check(name, outcome="pass", source=None):
    return {"check": name, "outcome": outcome, "from": source}


def _node(kind, handle, status="ok", checks=(), blocked_by=None, reason=None, **children):
    node = {kind: handle, "status": status, "coverage": "shallow" if checks else "none", "checks": list(checks),
            "blocked_by": blocked_by, "reason": reason}
    node.update(children)
    return node


BLOCK = {"machine": "demo-build", "reason": "outdated"}
DOCTOR_EXAMPLES = {
    "clean": ("machine.doctor", "layerfs machine doctor dev", {"ok": True}, {
        "ok": True, "findings": [], "not_checked": [], "status": "ok",
        "counts": {"ok": 3, "notice": 0, "warning": 0, "error": 0, "not_checked": 0}, "observed_at": T,
        "tree": _node("machine", "dev", checks=[_check("machine.state"), _check("machine.worker.ping"),
                                                _check("machine.worker.compatibility"), _check("machine.capacity")],
                      stores=[_node("store", "shop", checks=[_check("store.record"), _check("store.schema")])],
                      sandboxes=[_node("sandbox", "tests", checks=[_check("sandbox.daemon")])])}),
    "problem": ("machine.doctor", "layerfs machine doctor demo-build", {
        "findings": [{"machine": "demo-build", "check": "machine.worker.compatibility", "severity": "error",
                      "message": "worker is from another build", "fix": OUTDATED_FIX}],
        "not_checked": [{"store": "demo-build-cache", "blocked_by": BLOCK}, {"sandbox": "build-1", "blocked_by": BLOCK}]}, {
        "ok": False,
        "findings": [{"machine": "demo-build", "check": "machine.worker.compatibility", "severity": "error",
                      "message": "worker is from another build", "fix": OUTDATED_FIX, "from": None,
                      "evidence": {"protocol": 3}}],
        "not_checked": [{"store": "demo-build-cache", "check": None, "blocked_by": BLOCK, "reason": None},
                        {"sandbox": "build-1", "check": None, "blocked_by": BLOCK, "reason": None}],
        "status": "error", "counts": {"ok": 0, "notice": 0, "warning": 0, "error": 1, "not_checked": 2}, "observed_at": T,
        "tree": _node("machine", "demo-build", "error",
                      [_check("machine.state"), _check("machine.worker.ping"), _check("machine.worker.compatibility", "error")],
                      stores=[_node("store", "demo-build-cache", "not_checked", blocked_by=BLOCK)],
                      sandboxes=[_node("sandbox", "build-1", "not_checked", blocked_by=BLOCK)])}),
    "could_not": ("store.doctor", "layerfs store doctor qv2-store", {
        "findings": [{"project": "qv2-store/qv2-probe", "check": "project.listed", "severity": "notice",
                      "message": "on record, but the Store does not list it",
                      "fix": "layerfs project delete qv2-store/qv2-probe"}],
        "not_checked": [{"project": "qv2-store/qv2-strata", "check": "project.listed", "reason": "timeout"}]}, None),
}
PROGRESS_EXAMPLE = [
    {"phase": "self", "done": 1, "total": 3,
     "node": _node("machine", "native", checks=[_check("machine.state"), _check("machine.worker.ping"), _check("machine.capacity")])},
    {"phase": "stores", "done": 2, "total": 3,
     "node": _node("store", "qv2-store", checks=[_check("store.record"), _check("store.schema")])},
    {"phase": "sandboxes", "done": 3, "total": 3, "node": _node("sandbox", "tests", "not_checked", reason="timeout")},
]


def check_frame():
    """The frame's own doctor examples obey the doctor shape."""
    problems = []
    for name, (call, _, short, long) in DOCTOR_EXAMPLES.items():
        check_doctor_short(short, lambda what: problems.append(f"{name} short: {what}"))
        problems += [f"{name} short: {p} is empty" for p in empties(short)]
        if long is not None:
            check_doctor_long(call, long, lambda what: problems.append(f"{name} long: {what}"))
    for event in PROGRESS_EXAMPLE:
        kind = thing_key(event["node"])
        check_node(event["node"], kind, lambda what: problems.append(f"progress: {what}"), children=False)
        assert list(event) == PROGRESS_EVENT + ["node"] and event["phase"] in PHASES
    assert not problems, problems
    assert all(code in CODE_NAMES for code in TODAY_CODES) and set(COMPANIONS) == set(STATE_NAMES)


# --- rendering ------------------------------------------------------------------------------

def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def code_list(names, sep=", "):
    return sep.join(f"<code>{E(n)}</code>" for n in names)


def state_table(replaces):
    """The one state table (C11): state, companion, when."""
    head = "<th>State</th><th>Comes with</th><th>When</th>" + ("<th>Replaces today's</th>" if replaces else "")
    rows = []
    for name, companions, when, old in STATES:
        shaped = []
        for key in companions:
            keys = COMPANION_SHAPES[key].get("keys")
            shaped.append(f"{key} {{{', '.join(keys)}}}" if keys else key)
        rows.append(f'          <tr><td class="call">{name}</td><td class="cli" style="width:auto">{E(", ".join(shaped)) or "—"}</td>'
                    f'<td class="plain">{when}</td>' + (f'<td class="plain">{old}</td>' if replaces else "") + "</tr>")
    width = 760 if replaces else 520
    return (f'<div class="tbl"><table class="{"" if replaces else "mini"}" style="min-width:{width}px">\n        <thead><tr>{head}</tr></thead>\n'
            "        <tbody>\n" + "\n".join(rows) + "\n        </tbody>\n      </table></div>")


def doctor_keys_table():
    rows = [
        ("ok", "short, long", "<code>{\"ok\":true}</code> alone in the short form, only when every check was made and nothing was found. In the long form, true or false."),
        ("findings", "short when any, long", "What was found. Each entry: the thing by its own word and handle (" + code_list(THINGS)
         + "), <code>check</code> (the check's id), <code>severity</code> (" + code_list(SEVERITIES)
         + "), <code>message</code> (one sentence), <code>fix</code> (one runnable shell line, or null), and <code>from</code> "
           "(the machine a probe ran from, on a PostgreSQL Store's reach check; else null). The long form adds <code>evidence</code> "
           "(an object, or null). The short form leaves a null <code>fix</code> and <code>from</code> out."),
        ("not_checked", "short when any, long", "What could not be checked (R3). Each entry names its thing and carries exactly one of "
         "<code>blocked_by</code> (<code>{&lt;thing&gt;, reason}</code>, the same object the status calls answer) or <code>reason</code>: "
         + code_list(NOT_CHECKED_REASONS) + ". <code>check</code> is the first check that was not made; it is left out (null in the long form) "
         "when none of the thing's checks was made. An entry has no severity and is never a finding. A check that does not apply is left out of the report."),
        ("status", "long", "The worst among the things in the tree and the findings, one of " + code_list(STATUSES) + ". Worst first: " + code_list(WORST_FIRST, " &gt; ") + "."),
        ("counts", "long", "Things by status: <code>{" + ", ".join(STATUSES) + "}</code>. Each thing in the tree is counted once, by its own status; every key is always present."),
        ("observed_at", "long", "When the doctor finished."),
        ("tree", "long",
         "The thing's node and, under it, one list per kind of thing directly on it. A node: its thing by its own word, <code>status</code>, "
         "<code>coverage</code> (" + code_list(COVERAGE) + ": what this run checked on it; <code>full</code> only on a project or a bucket whose Full check ran), "
         "<code>checks</code> (each <code>{" + ", ".join(CHECK_KEYS) + "}</code>, in the order they ran; an outcome is " + code_list(OUTCOMES)
         + ": passed, its finding's severity, or not made), and <code>blocked_by</code> and <code>reason</code> (one of them set when its status is "
         "<code>not_checked</code>, both null otherwise). Child lists: " + "; ".join(f"{k}: {', '.join(v)}" for k, v in CHILDREN.items() if v)
         + ". A child node has no lists of its own. A project's and a bucket's tree is their one node."),
    ]
    body = "\n".join(f'          <tr><td class="call">{k}</td><td class="plain">{f}</td><td class="plain">{d}</td></tr>' for k, f, d in rows)
    return ('<div class="tbl"><table style="min-width:760px">\n        <thead><tr><th style="width:12%">Key</th><th style="width:16%">Form</th>'
            "<th>Holds</th></tr></thead>\n        <tbody>\n" + body + "\n        </tbody>\n      </table></div>")


def doctor_example(name, long=False):
    call, cli, short, full = DOCTOR_EXAMPLES[name]
    value = full if long else short
    return f"$ {E(cli)}{' --detail' if long else ''}\n{E(compact(value), quote=False)}"


def md_inline(text):
    text = E(text, quote=False)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    return re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", text)


def decisions_html(md):
    """DECISIONS.md as the "Decided at the freeze" section: headings, paragraphs and one card per decision."""
    out, blocks, current = [], [], None
    for line in md.splitlines():
        if line.startswith("# "):
            continue
        if line.startswith("## "):
            blocks.append(("h", line[3:].strip()))
            current = None
        elif line.startswith("- "):
            current = ["li", line[2:].strip()]
            blocks.append(current)
        elif not line.strip():
            current = None
        elif current is not None:
            current[1] += " " + line.strip()
        else:
            current = ["p", line.strip()]
            blocks.append(current)

    def card(label, text):
        bold = f"<b>{md_inline(label)}</b>" if label else ""
        return f'        <div class="decision">{bold}<span>{md_inline(text)}</span></div>'

    open_cards = False

    def cards(on):
        nonlocal open_cards
        if on and not open_cards:
            out.append('      <div class="decisions">')
        if not on and open_cards:
            out.append("      </div>")
        open_cards = on

    for kind, text in blocks:
        if kind == "h":
            cards(False)
            out.append(f"      <h3>{md_inline(text)}</h3>")
        elif kind == "li":
            cards(True)
            m = re.match(r"\*\*([^*]+)\*\*\s*(.*)", text, re.S)
            out.append(card(m.group(1), m.group(2)) if m else card("", text))
        elif re.search(r"In short: C1 ", text):
            lead, rest = text.split("In short: ", 1)
            cards(False)
            out.append(f"      <p>{md_inline(lead.strip())}</p>")
            cards(True)
            for piece in re.split(r";\s+(?=C\d+ )", rest):
                m = re.match(r"(C\d+) (.*)", piece.strip().rstrip("."), re.S)
                out.append(card(m.group(1), m.group(2)[:1].upper() + m.group(2)[1:] + ".") if m else card("", piece))
        else:
            cards(False)
            out.append(f"      <p>{md_inline(text)}</p>")
    cards(False)
    return "\n".join(out)


def long_adds(row):
    """What --detail adds on top of the short form, and what each --with section adds."""
    short = set(row["short"]["always"]) | set(row["short"]["sometimes"])
    parts = []
    adds = [k for k in row["long"] if k not in short]
    if adds:
        parts.append(E(", ".join(adds)))
    item = row.get("item")
    if item and "long" in item:
        ishort = set(item["short"]["always"]) | set(item["short"]["sometimes"])
        adds = [k for k in item["long"] if k not in ishort]
        if adds:
            parts.append("per item: " + E(", ".join(adds)))
    for name, section in row["with"].items():
        where = {"item": " (per item)", "top": " (top level)"}.get(section.get("where"), "")
        parts.append(f"<code>--with {E(name)}</code>{where}: " + E(", ".join(section["keys"])))
    return "; ".join(parts) or "—"


def render(S, fams, today, state, pairs_mod, draft):
    old = (S / "layerfs-call-surface.v6.html").read_text()
    head = old[: old.index('<div class="wrap">')].replace("</style>", (S / "extra.css").read_text() + "</style>")

    def section(sid):
        a = old.index(f'<section id="{sid}">')
        return old[a: old.index("</section>", a) + len("</section>")]

    kept = {k: section(k) for k in ("learned", "backups", "cache")}
    kept["learned"] = kept["learned"].replace(
        "Commits pinned with a flag (<code>--at</code>), not a trailing positional",
        "Commits pinned with a flag (<code>--at</code>), not a trailing positional; reads that don't answer a revision")
    kept["cache"] = kept["cache"].replace("one home-wide change counter", "one home-wide counter (<code>generation</code> on <code>home.get</code>)")
    script = old[old.index("<script>"):]

    rows = [r for f in fams for r in f["calls"]]
    by_name = {r["call"]: r for r in rows}
    total_new, total_old = len(rows), len(today["calls"])
    old_families = [f["family"] for f in today["families"]]
    gone_families = [f for f in old_families if f not in ORDER]
    now_of = {old_call: new for old_call, news in state["covered"].items() for new in news[:1]}
    with_values = [w for w in WITH_ORDER if w in state["with"]] + sorted(w for w in state["with"] if w not in WITH_ORDER)
    pairs_body, short_total, long_total, npairs = pairs_mod.render(fams)

    def cmd(call):
        return f"<code>{E(call)}</code>"

    def example(call, long=False):
        row = by_name.get(call)
        if not row:
            return None
        ex = row["examples"]
        return ex["long"] if long else unlabel(ex["short"][0])[1]

    # nav
    nav = ['  <nav class="index" aria-label="Contents">']
    for href, label in [("forms", "Short and long form"), ("side", "Side by side"), ("why", "Why this shape"),
                        ("rules", "Rules for every call"), ("agent", "Agent profile")]:
        nav.append(f'    <a href="#{href}">{label}</a>')
    nav.append('    <div class="sep"></div>')
    for f in fams:
        nav.append(f'    <a href="#f-{f["family"]}">{f["family"]} <span class="n">{f["old_count"]}→{len(f["calls"])}</span></a>')
    gone_count = sum(1 for c in today["calls"] if c["family"] in gone_families)
    nav.append(f'    <a href="#f-gone">{", ".join(gone_families)} <span class="n">{gone_count}→0</span></a>')
    nav.append('    <div class="sep"></div>')
    for href, label in [("postgres", "PostgreSQL Stores"), ("status", "Status in the API"), ("states", "Status in the UI"),
                        ("doctor", "Doctor"), ("learned", "What we took from others"), ("backups", "Backup status"),
                        ("cache", "Cache and freshness")]:
        nav.append(f'    <a href="#{href}">{label}</a>')
    nav += ['    <div class="sep"></div>', '    <a href="#map">Old → new map</a>', '    <a href="#decided">Decided at the freeze</a>', '  </nav>']

    option_names = sorted({o["name"] for r in rows for o in r["options"]})
    eyebrow = f"Frozen spec · version {VERSION} · {FROZEN}" + (f" · DRAFT, {len(state['errors'])} rule failures" if draft else "")
    percent = f"{short_total * 100 // long_total}%" if long_total else "—"
    header = f'''    <header class="top">
      <div class="eyebrow">{eyebrow}</div>
      <h1>LayerFS 2.0 Call Surface</h1>
      <p>The call table for <code>layerfs_v2_0_0</code>, frozen for implementation. Every call answers a <b>short form</b> by default, made for agents, and a <b>long form</b> with <code>--detail</code>, made for the UI. Each family shows the shell command, the short JSON line it prints, what the long form adds, and what changed. The same rows are in <code>calls.json</code>, which the implementation's contract test reads.</p>
      <div class="meta">
        <div>LayerFS <span>layerfs_v2_0_0 @ {BASE}</span></div>
        <div>dsh-layerfs <span>layerfs_v2_0_0 @ 176324d</span></div>
        <div>Frozen <span>{FROZEN}</span></div>
      </div>
      <div class="counts">
        <div class="count"><div class="v"><s>{total_old}</s> {total_new}</div><div class="l">calls in the table</div></div>
        <div class="count"><div class="v"><s>{len(old_families)}</s> {len(fams)}</div><div class="l">families</div></div>
        <div class="count"><div class="v">{len(with_values)}</div><div class="l">values of the one <code>--with</code></div></div>
        <div class="count"><div class="v">{len(option_names)}</div><div class="l">option names in all</div></div>
        <div class="count"><div class="v">{len(STATES)}</div><div class="l">states, on every thing</div></div>
        <div class="count"><div class="v">{len(REFUSAL_CODES)}</div><div class="l">refusal codes</div></div>
        <div class="count"><div class="v">{percent}</div><div class="l">of the long forms' bytes are in the short forms, over {npairs} examples</div></div>
      </div>
      <p class="small">Frozen from version 17 with rulings R1 to R9, corrections C1 to C14 and the simplicity changes applied; they are listed under <a href="#decided">Decided at the freeze</a>. Today's column is extracted from the code's own call table at <code>{BASE}</code> ({total_old} calls in {len(old_families)} families). Owner, 2026-11-18: no environment variables for context; project, Branch and Workspace stay explicit on every call.</p>
    </header>'''

    def pre(lines):
        lines = [l for l in lines if l]
        return "<pre>" + "\n".join(lines) + "</pre>" if lines else ""

    def run_example(call, args, long=False):
        text = example(call, long)
        if text is None:
            return ""
        return f"$ layerfs {call.replace('.', ' ').replace('_', '-')} {args}{' --detail' if long else ''}\n" + E(pairs_mod.short_ids(text), quote=False)

    with_calls = sum(1 for r in rows if r["with"])
    preferred = [by_name[c] for c in ("store.get", "machine.list", "commit.diff") if c in by_name and by_name[c]["with"]]
    with_examples = [E(r["cli"]) for r in (preferred or [r for r in rows if r["with"]])[:3]]
    forms = f'''    <section id="forms">
      <h2>Short form and long form</h2>
      <div class="forms">
        <div class="form">
          <span class="tag">Short form · default · agents</span>
          <p>Only what the call produced: the content asked for, the id or handle the next call takes, and anything that changed. No result tag, no echoed inputs, no revision on reads. Fields that are null, false or empty are left out. An action with nothing to return answers <code>{{"ok":true}}</code>, and <code>ok</code> never appears beside another key.</p>
          {pre([run_example("file.write", "shop/api agent /src/app.ts &lt; app.ts"), run_example("file.stat", "shop/api agent /src/app.ts")])}
        </div>
        <div class="form">
          <span class="tag">Long form · --detail · UI</span>
          <p>Every short key, with the same name and type, plus flat extra keys. Every key of the row's long list is always present, nulls and empty lists included, so a UI reads one stable shape. It adds no I/O; it shows what was already read.</p>
          {pre([run_example("file.stat", "shop/api agent /src/app.ts", True)])}
        </div>
        <div class="form">
          <span class="tag">Sections · --with</span>
          <p><code>--with</code> adds one named section to an answer, in either form. It never changes what a call does. The values are {code_list(with_values)}; {with_calls} calls take one or more, and each row lists its own. Anything that probes a machine, walks a disk or reads more of the Store is behind it, so neither form is slow by surprise.</p>
          {pre(with_examples)}
        </div>
      </div>
      <p class="small">In Node the long form is <code>{{ detail: true }}</code> and a section is <code>{{ with: ["live"] }}</code>. In Python they are <code>detail=True</code> and <code>with_=["live"]</code>: a name that is a Python keyword takes a trailing underscore. A UI asks for <code>--detail</code> once, in its client; agents get the short form without asking.</p>
    </section>'''

    side = f'''    <section id="side">
      <h2>Short and long, side by side</h2>
      <p>{npairs} calls, each with its first short example (agents, the default) and its long example (UI, <code>--detail</code>), taken from the family rows. Byte counts are measured on the examples as written, as one line of compact JSON; ids of 32 or more hex characters are shortened here for reading, after measuring. The build checks every example against its row: a short example uses only short keys and carries no empty field, and a long example has exactly the row's long keys. Across all {npairs}, the short forms total <b>{short_total:,} B</b> and the long forms <b>{long_total:,} B</b>: an agent reads {percent} of what the UI does.</p>
      <div class="pairs">
{pairs_body}      </div>
    </section>'''

    WHY = [
        ("Agents pay for every byte", "An agent reads every result into its context, on every call. The short form carries one id for a write, and exit code and output for a command.",
         "mirage, drive9 and cloudflare/computer all answer writes with the path or one id, and command runs with exit code and output."),
        ("A read answers what was asked", "<code>file stat</code> answers type, size and mode, not the Commit it read. The revision is the caller's to choose (<code>--at</code>) or the long form's to show. A write answers its new Commit, because that is new information.",
         "Carrying the revision on every read invited guards nobody set."),
        ("A result feeds the next call", "The id a call returns is named like the argument the next call takes: <code>commit</code> out, <code>--at</code> and <code>--expected</code> in; <code>workspace</code> from mount, <code>workspace</code> into exec.",
         "Today <code>workspace.mount</code> answers a 32-hex token in <code>workspace</code>, which exec does not accept; the handle is in <code>ws</code>."),
        ("The UI reads one stable shape", "The long form keeps every short key and type and always sends every key, nulls included, so a UI can render without guessing. Agents never pay for it.",
         "Results are untyped maps today; each row's <code>long</code> list is the schema."),
        ("A screen gets what it needs in one call", "Every UI call is a CLI process. List rows carry what a row shows, and probes are named in <code>--with</code>.",
         "Today the UI calls <code>sandbox.get</code> for every Sandbox only to show Workspace names."),
        ("Simple, but not by dropping history", "LayerFS keeps Branches, Commits, shared Stores and several machines. It hides them behind defaults, short ids and the long form, not by removing them.",
         "Automatic retries, idle shutdown and output files inside the workspace break LayerFS rules and are not copied."),
    ]
    why = ['    <section id="why">', '      <h2>Why this shape</h2>', '      <div class="why-grid">']
    for title, text, evidence in WHY:
        why.append(f'        <div class="why">\n          <h3>{title}</h3>\n          <p>{text}</p>\n          <p class="ev">{evidence}</p>\n        </div>')
    why += ['      </div>', '    </section>']

    # computed parts of the rules
    def calls_where(test):
        return [r["call"] for r in rows if test(r)]

    def has_option(name):
        return calls_where(lambda r: any(o["name"] == name for o in r["options"]))

    optional = [f'{cmd(r["call"])} <code>[{p["name"]}]</code>' for r in rows for p in r["positionals"] if p.get("optional")]
    cursor_calls = calls_where(lambda r: r["paging"] == "cursor")
    log_calls = [c for c in cursor_calls if c.endswith(".log")]
    line_calls = calls_where(lambda r: r["paging"] == "line")
    byte_calls = calls_where(lambda r: r["paging"] == "byte")
    unpaged = calls_where(lambda r: r["item"] is not None and r["paging"] is None)
    top_sections = sorted({f'{n} on {r["call"]}' for r in rows for n, s in r["with"].items() if s.get("where") in ("item", "top")})
    fold = "; ".join(f"{code_list(olds)} → <code>{new}</code>" for new, olds in TODAY_CODES.items())
    front = "".join(
        f'<tr><td class="cli" style="width:auto">{E(o["cli"])}</td><td class="cli" style="width:auto">{E(o["node"] or "—")}</td>'
        f'<td class="cli" style="width:auto">{E(o["python"] or "—")}</td><td class="plain">{E(o["help"])}</td></tr>' for o in GLOBAL_OPTIONS)
    stdin_secret = calls_where(lambda r: r["stdin"] == "secret")
    py_keywords = sorted({n for n in ["with"] + [a["name"] for r in rows for a in r["positionals"] + r["options"]] if keyword.iskeyword(n)})
    RULES = [
        ("Call form", "<code>layerfs &lt;family&gt; &lt;call&gt; &lt;positionals&gt; [--options]</code>. Two levels only. Positionals go from the outside in: owner, Branch, path or new name. Anything optional is an option"
         + (f", except {len(optional)} trailing positionals with an obvious default: {', '.join(optional)}" if optional else "")
         + ". A shell option is the option's name with <code>-</code> for <code>_</code>. No old call or option name keeps an alias (R1)."),
        ("Short form", "The default, for agents. No echo of inputs, no result tag, no revision on reads, no null, false or empty field. A boolean is named so that <code>true</code> is the news. Each row lists the keys that are always there and the keys that appear only when they are news. <code>ok</code> appears only alone: <code>{\"ok\":true}</code> is the whole answer when there is nothing else to say, and <code>ok</code> is dropped when any other key remains. The short form is the long form projected onto the row's short keys, with null, false and empty values dropped, then <code>ok</code> dropped if anything is left beside it."),
        ("Long form", "<code>--detail</code>, for the UI. Every short key with the same name and type, plus flat extra keys. Every key of the row's <code>long</code> list is always present. Where a row has <code>ok</code>, the long form keeps it and it is true; only a doctor's <code>ok</code> can be false, when it has findings or things it could not check. It adds no I/O."),
        ("Sections", f"<code>--with</code> adds one named section to an answer and nothing else. Values: {code_list(with_values)}. A section answers under its own key (<code>\"live\": {{…}}</code>)"
         + (f", except where a row adds its keys flat to each item or to the top level ({E('; '.join(top_sections))})" if top_sections else "") + "."),
        ("Commits in", f"<code>--at C</code> pins a read: a full id or a prefix of 8 or more characters, resolved by the Store ({len(has_option('at'))} calls). <code>--expected C</code> guards a write: <code>Conflict</code> if the head moved ({len(has_option('expected'))} calls). <code>-m</code> gives the message on every call that makes a Commit ({len(has_option('message'))} calls)."),
        ("Commits out", "A write answers the new <code>commit</code>, a plain id, the same word <code>--at</code> and <code>--expected</code> take. Reads answer none in the short form. History calls answer Commits because Commits are what they list."),
        ("Paging", f"One convention. In: <code>--limit N --cursor C</code>; the default limit is {LIST_SHAPE['default_limit']['list']} for a list and {LIST_SHAPE['default_limit']['log']} for a log, at most {LIST_SHAPE['max_limit']}. Out: <code>{{\"items\":[…],\"next_cursor\":\"…\"}}</code>; <code>next_cursor</code> is left out at the end. A page ends early at 64 KiB with a cursor, never <code>Capacity</code>. The cursor carries the pins and filters. {len(cursor_calls)} calls page this way"
         + (f", the log calls among them ({code_list(log_calls)}), newest first" if log_calls else "")
         + (f". {len(unpaged)} list calls take no cursor because they are bounded, and each row says why: {code_list(unpaged)}" if unpaged else "") + "."),
        ("Windows", f"Text by lines: <code>--line N</code> (1-based) and <code>--limit N</code> (default {LIST_SHAPE['default_limit']['line']}), answering <code>next_line</code> when more remains"
         + (f" ({code_list(line_calls)})" if line_calls else "") + "."
         + (f" Bytes, for UI file views only: {code_list(byte_calls)}." if byte_calls else "")),
        ("Inline lists", "A list inside a result (conflicts, overwritten paths, changes) shows its first 20 entries; only then does <code>&lt;list&gt;_count</code> appear. The rest comes from the call that owns the list. A field never changes type when cut."),
        ("Names", "Each thing by its own word (<code>project</code>, <code>branch</code>, <code>workspace</code>, <code>sandbox</code>, <code>machine</code>, <code>store</code>, <code>bucket</code>, <code>commit</code>), never <code>name</code> except for what a call creates. <code>engine</code> is sqlite or postgresql; <code>type</code> is file, dir or symlink only. Enum values are lowercase."),
        ("Fields", "Byte totals end in <code>_bytes</code>; one object's length is <code>size</code>. <code>*_at</code> is ISO-8601 UTC; <code>*_ms</code> is a duration. A plural noun is always a list, never a number. A count is <code>&lt;thing&gt;_count</code> (<code>file_count</code>); the count of a list that was cut is <code>&lt;list&gt;_count</code> and appears only beside that list."),
        ("State", "One of six words on every machine, Store, project, Sandbox and Workspace, each with the companion below and no other. A <code>get</code> always answers <code>state</code>; a list row leaves it out when it is <code>ready</code>. The reasons and rules are under <a href=\"#status\">Status</a>.</span>"
         + state_table(False) + "<span>"),
        ("Refusals", f"<code>{{\"error\",\"message\",\"fix\"}}</code> on stderr, plus the fields the next call needs (a <code>Conflict</code> names the head as <code>commit</code>). <code>fix</code> is a runnable command and is left out when there is none. <code>detail</code> only in the long form. {len(REFUSAL_CODES)} codes: {code_list(CODE_NAMES)}. Today's refusal is <code>{{\"code\",\"message\",\"detail\"}}</code> and the code has no closed list; its other codes fold into these at the boundary: {fold}."),
        ("Exit codes", f"<code>0</code>: {E(EXIT_CODES['0'])}. <code>2</code>: {E(EXIT_CODES['2'])}. <code>125</code>: {E(EXIT_CODES['125'])}. As today. <code>workspace exec</code> exits with the command's own code and prints its result JSON on stdout; a refusal prints <code>{{\"error\",…}}</code> on stderr, nothing on stdout, and exits 125."),
        ("Global options", "The same on every call, in each front end:</span>"
         f'<div class="tbl"><table class="mini" style="min-width:520px"><thead><tr><th>Shell</th><th>Node</th><th>Python</th><th>Does</th></tr></thead><tbody>{front}</tbody></table></div><span>'
         + "Python spells a name that is a Python keyword with a trailing underscore"
         + (f": {code_list([k + '_=' for k in py_keywords])}" if py_keywords else "") + ". "
         + (f"Secrets come only on stdin ({code_list(stdin_secret)})." if stdin_secret else "Secrets come only on stdin.")),
        ("Progress", f"<code>--progress</code> prints one JSON line per event on stderr, <code>{{{', '.join(chr(34) + k + chr(34) for k in PROGRESS_EVENT)}}}</code>; Node and Python pass <code>on_progress</code>. {len(state['progress'])} calls report it: {code_list(state['progress'])}. A doctor's event also carries <code>node</code>, the thing that just finished."),
        ("Call log", f"Every call writes one line to the home's call log, except the calls whose row says quiet, which a UI polls: {code_list(state['quiet']) or 'none'}."),
        ("One attempt", "Every call is one attempt, with no retry and no replay of a lost reply. The in-between states are stated (C13): a <code>sandbox create --mount</code> whose mount is refused keeps the Sandbox and names it in the refusal; <code>store move</code> goes to or from this computer only; a lost reply on <code>machine start</code> or <code>machine stop</code> is settled by <code>machine get --with live</code>."),
    ]
    rules = ['    <section id="rules">', '      <h2>Rules for every call</h2>', '      <div class="rules">']
    for k, d in RULES:
        rules.append(f'        <div class="rule"><div class="k">{k}</div><div class="d"><span>{d}</span></div></div>')
    rules += ['      </div>', '    </section>']

    agent_calls = state["agent"]
    branch_tools = [c for c in agent_calls if c.split(".")[0] in ("project", "file", "branch", "commit")]
    workspace_tools = [c for c in agent_calls if c.split(".")[0] in ("sandbox", "workspace")]
    other_tools = [c for c in agent_calls if c not in branch_tools and c not in workspace_tools]
    hidden = [f["family"] for f in fams if not any(r["profile"] == "agent" for r in f["calls"])]
    agent = f'''    <section id="agent">
      <h2>Agent profile</h2>
      <p>Agents get {len(agent_calls)} of the {total_new} calls as tools, with short forms. The UI and operators keep the full table and ask for <code>--detail</code>. Every call names its project and Branch, or its Workspace, explicitly: LayerFS adds no environment variables or bound context.</p>
      <div class="rules">
        <div class="rule"><div class="k">Branch tools</div><div class="d"><span>{code_list(branch_tools) or "—"}. Each write is one Commit on the named Branch. For small changes without a Sandbox.</span></div></div>
        <div class="rule"><div class="k">Workspace tools</div><div class="d"><span>{code_list(workspace_tools) or "—"}. Files are read and written through the mount by the commands the agent runs. <code>workspace.list</code> is here so an agent can find its Workspaces and their running commands, and <code>workspace cancel</code> stops every running command of a Workspace unless <code>--exec</code> names one (R7).</span></div></div>
        {f'<div class="rule"><div class="k">Other tools</div><div class="d"><span>{code_list(other_tools)}.</span></div></div>' if other_tools else ""}
        <div class="rule"><div class="k">Hidden by default</div><div class="d"><span>{", ".join(hidden) or "no whole family"}{" and" if hidden else ";"} every call whose row says <code>full</code>, behind the MCP server's full profile. Write tools are hidden when the caller cannot write; a write to <code>main</code> is refused with a <code>fix</code> naming <code>branch fork</code>.</span></div></div>
      </div>
    </section>'''

    tools = '''    <section aria-label="Families">
      <div class="tools">
        <input type="search" id="filter" placeholder="Filter calls, e.g. exec or cursor" aria-label="Filter calls">
        <label><input type="checkbox" id="changed"> Changed only</label>
        <div class="legend">
          <span class="badge b-kept">kept</span>
          <span class="badge b-extended">extended</span>
          <span class="badge b-renamed">renamed / reshaped / moved / paging</span>
          <span class="badge b-merged">merged</span>
          <span class="badge b-new">new</span>
        </div>
      </div>
      <p class="small">The third column is the short form, one line of JSON on stdout; a labelled line is another case of the same call. The fourth lists what <code>--detail</code> adds on top, and what each <code>--with</code> section adds. Example values are illustrative; the key lists are the contract and are in <code>calls.json</code>.</p>
    </section>'''

    fam_html = []
    for f in fams:
        who = f'<span class="who">{E(f["who"])}</span>' if f.get("who") else ""
        fam_html.append(f'''    <section class="fam" id="f-{f["family"]}">
      <div class="fam-head"><h2>{f["family"]}</h2>{who}<span class="c">{f["old_count"]} → {len(f["calls"])}</span><p>{f["blurb"]}</p></div>
      <div class="tbl"><table>
        <thead><tr><th>Call</th><th>Input (CLI)</th><th>Short form (agents)</th><th>Long form adds (UI)</th><th>Change</th></tr></thead>
        <tbody>''')
        for r in f["calls"]:
            changed = "" if r["badge"] == "kept" else " data-changed"
            note = f'<span class="note">{r["note"]}</span>' if r["note"] else ""
            work = (f'<details class="work"><summary>New work</summary><span>{md_inline(r["new_work"])}</span></details>'
                    if r.get("new_work") else "")
            fam_html.append(
                f'          <tr{changed}><td class="call">{r["call"]}<span class="help">{E(r["help"])}</span></td><td class="cli">{E(r["cli"])}</td>'
                f'<td class="out">{E(pairs_mod.short_ids(chr(10).join(r["examples"]["short"])), quote=False)}</td><td class="long">{long_adds(r)}</td>'
                f'<td class="chg"><span class="badge {BADGE[r["badge"]]}">{r["badge"]}</span>{note}{work}</td></tr>')
        fam_html.append('        </tbody>\n      </table></div>')
        if f["reasons"]:
            fam_html.append('      <ul class="fam-why">\n' + "\n".join(f"        <li>{r}</li>" for r in f["reasons"]) + '\n      </ul>')
        fam_html.append('    </section>\n')
    fam_html.append(f'''    <section class="fam" id="f-gone">
      <div class="fam-head"><h2>{", ".join(gone_families)}</h2><span class="c">{gone_count} → 0</span><p>These families are gone. Each call moved to the family whose thing it reads or fixes. The off-table <code>layerfs doctor</code> report is replaced by the six doctor calls, with no alias (R1).</p></div>
      <div class="tbl"><table>
        <thead><tr><th>Today</th><th>Now</th></tr></thead>
        <tbody>''')
    for c in today["calls"]:
        if c["family"] in gone_families:
            name = f'{c["family"]}.{c["call"]}'
            fam_html.append(f'          <tr data-changed><td class="call">{name}</td><td class="call">{now_of.get(name, "—")}</td></tr>')
    fam_html.append('        </tbody>\n      </table></div>\n    </section>')

    # old → new map, from the rows' `today` lists
    mp = ['    <section id="map">', '      <h2>Old → new map</h2>',
          f'      <p>Every one of today\'s {total_old} calls, in the code\'s own table order, with the row that replaces it. Generated from the rows\' <code>today</code> lists; the build fails unless each call is covered exactly once.</p>',
          '      <div class="tbl"><table style="min-width:640px">',
          '        <thead><tr><th>Today</th><th>Now</th><th>How to call it now</th></tr></thead>', '        <tbody>']
    for c in today["calls"]:
        name = f'{c["family"]}.{c["call"]}'
        new = now_of.get(name)
        same = ' class="same"' if new == name else ""
        cli = E(by_name[new]["cli"]) if new in by_name else "—"
        mp.append(f'          <tr{same}><td class="call">{name}</td><td class="call">{new or "—"}</td><td class="cli" style="width:auto">{cli}</td></tr>')
    mp += ['        </tbody>', '      </table></div>']
    new_calls = [r for r in rows if not r["today"]]
    if new_calls:
        mp.append(f'      <p class="small">New, with no call behind them today ({len(new_calls)}): {code_list([r["call"] for r in new_calls])}.</p>')
    # argument table, from the rows' `dropped` entries (C3)
    grouped, order = {}, []
    for r in rows:
        for d in r["dropped"]:
            key = (d["today"], d.get("now") or "", d.get("why") or "")
            if key not in grouped:
                grouped[key] = []
                order.append(key)
            grouped[key].append(r["call"])
    mp += ['      <h3>Argument renames and removals</h3>',
           f'      <p class="small">Every positional and option that exists in the code\'s table today and is not in its new row under the same name: {len(order)} entries, generated from the rows\' <code>dropped</code> lists. The build fails if an argument of today\'s table is in neither.</p>',
           '      <div class="tbl"><table style="min-width:720px">',
           '        <thead><tr><th style="width:20%">Today</th><th style="width:20%">Now</th><th style="width:22%">Where</th><th>Why</th></tr></thead>', '        <tbody>']
    for key in order:
        old_arg, now, reason = key
        mp.append(f'          <tr><td class="cli" style="width:auto">{E(old_arg)}</td><td class="cli" style="width:auto">{E(now) or "(dropped)"}</td>'
                  f'<td class="plain">{E(", ".join(grouped[key]))}</td><td class="plain">{md_inline(reason)}</td></tr>')
    mp += ['        </tbody>', '      </table></div>', '    </section>']

    decided = ['    <section id="decided">', '      <h2>Decided at the freeze</h2>', decisions_html((S / "DECISIONS.md").read_text()),
               '      <p class="small">Not changed on purpose: two-level names, the single <code>file.*</code> writes, the <code>backup</code> family, <code>project.delete</code>\'s verb, the separate <code>images</code>, <code>load_image</code> and <code>prune</code>, explicit Commit and unmount, one attempt per call, no context environment variables. Sources: the call table in <code>api/crates/layerfs-api/src</code> at <code>' + BASE + '</code>, <code>docs/versioned/2.0.0/calls.md</code>, dsh-layerfs <code>src/host</code> and <code>src/client</code>, and the review in the rollout page.</p>',
               '    </section>']

    # the frame's own sections, with their generated parts
    status = (S / "status.html").read_text()
    fills = {
        "STATE_TABLE": state_table(True),
        "REASON_CODES": code_list(REASONS),
        "BROKEN_REASONS": code_list(BROKEN_REASONS),
        "DOCTOR_KEYS": doctor_keys_table(),
        "DOCTOR_EX_CLEAN": doctor_example("clean"),
        "DOCTOR_EX_PROBLEM": doctor_example("problem"),
        "DOCTOR_EX_PROBLEM_LONG": doctor_example("problem", True),
        "DOCTOR_EX_COULD_NOT": doctor_example("could_not"),
        "DOCTOR_EX_PROGRESS": "$ layerfs machine doctor native --progress\n" + "\n".join(E(compact(e), quote=False) for e in PROGRESS_EXAMPLE),
        "STATUSES": code_list(STATUSES),
        "OUTCOMES": code_list(OUTCOMES),
        "PHASES": code_list(PHASES),
    }
    for key, value in fills.items():
        assert f"<!--{key}-->" in status, f"status.html lacks <!--{key}-->"
        status = status.replace(f"<!--{key}-->", value)
    assert not re.search(r"<!--[A-Z_]+-->", status), re.findall(r"<!--[A-Z_]+-->", status)
    pg = (S / "pg.html").read_text()

    page = (head + '<div class="wrap">\n' + "\n".join(nav) + "\n\n  <main>\n" + header + "\n\n" + forms + "\n\n" + side + "\n\n"
            + "\n".join(why) + "\n\n" + "\n".join(rules) + "\n\n" + agent + "\n\n" + tools + "\n\n" + "\n".join(fam_html) + "\n\n    "
            + kept["learned"] + "\n\n" + pg + "\n" + status + "\n    " + kept["backups"] + "\n\n    " + kept["cache"] + "\n\n"
            + "\n".join(mp) + "\n\n" + "\n".join(decided) + "\n  </main>\n</div>\n\n" + script)
    for name in ("LAYERFS_PROJECT", "LAYERFS_BRANCH", "LAYERFS_WORKSPACE"):
        assert name not in page, f"the page mentions {name} (owner rule: no context environment variables)"
    return page, (short_total, long_total, npairs)


def calls_json(fams, state):
    rows = [r["call"] for f in fams for r in f["calls"]]
    return {
        "version": VERSION,
        "frozen": FROZEN,
        "base": BASE,
        "states": STATE_NAMES,
        "state_companions": {name: {key: COMPANION_SHAPES[key] for key in COMPANIONS[name]} for name in STATE_NAMES},
        "refusal_codes": CODE_NAMES,
        "exit_codes": EXIT_CODES,
        "with_values": [w for w in WITH_ORDER if w in state["with"]],
        "global_options": GLOBAL_OPTIONS,
        "doctor": doctor_shape([c for c in rows if c in DOCTOR_THING]),
        "list_shape": LIST_SHAPE,
        "refusal_shape": REFUSAL_SHAPE,
        "progress_event": PROGRESS_EVENT,
        "families": [{"family": f["family"], "who": f.get("who"), "old_count": f.get("old_count"),
                      "calls": [{k: v for k, v in r.items() if k not in HTML_ONLY} for r in f["calls"]]} for f in fams],
    }


def main(argv):
    flags = {"--partial": False, "--draft": False}
    values = {"--today": None, "--families": None, "--out": None}
    args = []
    i = 1
    while i < len(argv):
        if argv[i] in flags:
            flags[argv[i]] = True
        elif argv[i] in values:
            values[argv[i]] = argv[i + 1]
            i += 1
        else:
            args.append(argv[i])
        i += 1
    if len(args) != 1:
        sys.exit(__doc__)
    S = pathlib.Path(args[0])
    out = pathlib.Path(values["--out"] or S)
    if values["--today"]:
        table = today_table(values["--today"])
        (S / "today-table.json").write_text(json.dumps(table, indent=1) + "\n")
        print(f"today-table.json: {len(table['calls'])} calls in {len(table['families'])} families at {table['base']}: "
              + ", ".join(f"{f['family']} {f['calls']}" for f in table["families"]))
        return 0
    partial = flags["--partial"]
    check_frame()
    table = json.loads((S / "today-table.json").read_text())
    assert table["base"] == BASE and len(table["calls"]) == 84, "today-table.json is not the table at " + BASE
    today = {f'{c["family"]}.{c["call"]}': c for c in table["calls"]}
    state = {"errors": [], "warnings": [], "calls": {}, "covered": {}, "with": {}, "quiet": [], "progress": [], "agent": [],
             "missing": [], "off_table": table.get("off_table", [])}
    folder = pathlib.Path(values["--families"] or S / "families")
    fams = []
    for name in ORDER:
        path = folder / f"{name}.json"
        if not path.exists():
            state["missing"].append(name)
            if not partial:
                state["errors"].append(f"{name}.json: [fields] the family file does not exist")
            continue
        try:
            fam = json.loads(path.read_text())
        except ValueError as why:
            state["errors"].append(f"{name}.json: [fields] does not parse as JSON: {why}")
            continue
        check_family(path, fam, today, state)
        fams.append(fam)
    for old, news in state["covered"].items():
        if len(news) > 1:
            state["errors"].append(f"{old}: [today] today's call is covered by {len(news)} rows: {', '.join(news)}")
    if not partial and not state["missing"]:
        for old in today:
            if old not in state["covered"]:
                state["errors"].append(f"{old}: [today] today's call is in no row's `today` list")
        counted = sum(f.get("old_count") or 0 for f in fams if isinstance(f.get("old_count"), int))
        gone = sum(1 for c in table["calls"] if c["family"] not in ORDER)
        if counted + gone != len(today):
            state["errors"].append(f"totals: [today] the old_count values add up to {counted}; with {gone} calls in the gone families that is not {len(today)}")
    check_decisions(state, partial, decision_totals((S / "DECISIONS.md").read_text()))
    state["warnings"] = list(dict.fromkeys(state["warnings"]))
    for line in state["warnings"]:
        print("warning:", line)
    for line in state["errors"]:
        print("FAIL", line)
    failed = bool(state["errors"])
    if failed and not flags["--draft"]:
        print(f"{len(state['errors'])} rule failures in {len(state['calls'])} calls; nothing written")
        return 1
    spec = importlib.util.spec_from_file_location("pairs", S / "pairs.py")
    pairs_mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pairs_mod)
    try:
        page, (short_total, long_total, npairs) = render(S, fams, table, state, pairs_mod, failed)
    except (KeyError, TypeError, AttributeError) as why:
        print(f"the page cannot be rendered from rows that fail the rules: {why!r}")
        return 1
    (out / "layerfs-call-surface.html").write_text(page)
    (out / "pairs.html").write_text(pairs_mod.document(fams))
    wrote = "layerfs-call-surface.html, pairs.html"
    if not failed and not partial:
        (out / "calls.json").write_text(json.dumps(calls_json(fams, state), indent=1, ensure_ascii=False) + "\n")
        wrote += ", calls.json"
    per_family = ", ".join(f"{f['family']} {len(f['calls'])}" for f in fams)
    print(f"calls {len(today)}->{len(state['calls'])} in {len(fams)} families ({per_family}); page {len(page)} B; "
          f"short {short_total} B, long {long_total} B over {npairs} pairs; wrote {wrote}"
          + (f"; {len(state['errors'])} RULE FAILURES (draft)" if failed else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

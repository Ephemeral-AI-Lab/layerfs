"""Premise analysis v2 over dsh session event logs (~/.dsh/sessions/**/session*.jsonl.zstd).

Read-only; prints aggregates only (no prompts, contents, paths or command text).
Excludes sessions whose model provider is the LayerFS mock model ("layerfs-mock").

Per step (event order: step/start, assistant/message, tool/call*, tool/result*, step/end):
  model_ms = assistant/message.time - step/start.time
  tool_ms  = union of [tool/call.time, tool/result.time] intervals within the step
  rest_ms  = step_ms - model_ms - tool_ms (harness/bookkeeping)
Idle time between turns (waiting for the user) is excluded.

Tool categories (top-level calls, and sub-calls dispatched from run_code via tool/ptc-dispatch):
  file   : read, write, edit, glob, grep, read_image, read_before_edit_status
  shell  : bash, exec_command, write_stdin, job_output, job_list, job_kill, cmd
  agent  : send_message, subagent, list_agents, interrupt_agent, ask_user_question, report, present,
           todo_write, *session*, loop_*, *goal*, skill
  web    : web_search, web_fetch
  code   : run_code (code-mode wrapper; its sub-calls are counted separately)
  other  : anything else
Shell commands are bucketed by first program word (see classify) — buckets only.
"""
import sys, json, glob, os, subprocess, shlex, statistics, re
from collections import Counter, defaultdict

ROOT = os.path.expanduser("~/.dsh/sessions")
files = sorted(glob.glob(f"{ROOT}/*/*/session*.jsonl.zstd"))

FILE = {"read", "write", "edit", "glob", "grep", "read_image", "read_before_edit_status"}
SHELL = {"bash", "exec_command", "write_stdin", "job_output", "job_list", "job_kill", "cmd"}
WEB = {"web_search", "web_fetch"}


def category(name):
    n = name or ""
    if n in FILE:
        return "file"
    if n in SHELL:
        return "shell"
    if n in WEB:
        return "web"
    if n == "run_code":
        return "code"
    if n in ("send_message", "subagent", "list_agents", "interrupt_agent", "ask_user_question", "report", "present",
             "todo_write", "skill") or "session" in n or n.startswith("loop_") or "goal" in n:
        return "agent"
    return "other"


READ_CMDS = {"cat", "sed", "head", "tail", "rg", "grep", "ls", "find", "fd", "wc", "nl", "less", "awk", "tree", "stat",
             "file", "du", "pwd", "diff", "jq", "bat", "realpath", "readlink", "sort", "uniq", "cut", "echo", "printf",
             "which", "test", "[", "xxd", "od", "cmp", "md5", "shasum", "sha256sum", "column"}
GIT_READ = {"show", "diff", "log", "status", "blame", "grep", "ls-files", "rev-parse", "branch", "describe", "cat-file",
            "merge-base", "remote", "for-each-ref", "worktree", "shortlog", "reflog", "fetch", "-C"}
BUILD = {"cargo", "npm", "npx", "pnpm", "yarn", "node", "make", "cmake", "pytest", "python", "python3", "go", "rustc",
         "tsc", "bun", "deno", "pip", "pip3", "uv", "just", "swift", "xcodebuild", "rustup", "timeout", "gtimeout"}
OPS = {"docker", "gh", "curl", "limactl", "container", "colima", "psql", "sqlite3", "layerfs", "env", "time", "nohup",
       "sleep", "kill", "pkill", "ps", "lsof", "open", "zstd", "tar", "ssh", "brew", "launchctl", "df", "top", "pgrep"}
EDIT = {"apply_patch", "patch", "mv", "cp", "rm", "mkdir", "touch", "chmod", "ln", "tee", "perl", "truncate"}


def first_word(c):
    for p in re.split(r"\s*(?:&&|;|\|\||\n)\s*", c.strip()):
        try:
            toks = shlex.split(p, posix=True)
        except Exception:
            toks = p.split()
        while toks and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", toks[0]):
            toks = toks[1:]
        if not toks:
            continue
        w = os.path.basename(toks[0])
        if w in ("cd", "set", "export", "source", ".", "#"):
            continue
        return w, (toks[1] if len(toks) > 1 else ""), toks
    return "", "", []


def classify(c):
    if isinstance(c, list):
        if len(c) >= 3 and os.path.basename(str(c[0])) in ("bash", "zsh", "sh") and c[1] in ("-lc", "-c"):
            c = c[2]
        else:
            c = " ".join(map(str, c))
    if not isinstance(c, str) or not c.strip():
        return "unknown"
    w, sub, toks = first_word(c)
    if w in ("bash", "zsh", "sh") and len(toks) >= 3 and toks[1] in ("-lc", "-c"):
        w, sub, toks = first_word(toks[2])
    if w == "git":
        if sub == "-C" and len(toks) > 3:
            sub = toks[3]
        return "git-read" if sub in GIT_READ else "git-write"
    if w in ("python", "python3") and sub == "-":
        return "build/test/run"
    if w in READ_CMDS:
        return "read/search"
    if w in EDIT:
        return "edit/fs-mutate"
    if w in BUILD:
        return "build/test/run"
    if w in OPS:
        return "ops/infra"
    return "other"


def shell_cmd(name, args):
    if name == "bash":
        return args.get("command")
    if name == "exec_command":
        return args.get("cmd", args.get("command"))
    return None


def load(path):
    out = subprocess.run(["zstd", "-dc", path], capture_output=True, timeout=100).stdout
    for line in out.splitlines():
        try:
            yield json.loads(line)
        except Exception:
            continue


def pct(v, q):
    v = sorted(v)
    return v[min(len(v) - 1, int(q * (len(v) - 1)))] if v else None


SESS = defaultdict(list)
G = defaultdict(lambda: {"sessions": 0, "turns": 0, "steps": 0, "model_ms": 0, "tool_ms": 0, "step_ms": 0,
                         "top": Counter(), "cat": Counter(), "sub": Counter(), "subcat": Counter(),
                         "shell_cls": Counter(), "call_ms": defaultdict(list), "model_step_ms": [],
                         "gap_ms": [], "turn_shell": [], "turn_calls": [], "turn_file": [],
                         "steps_with_shell": 0, "steps_with_tools": 0})
excluded = Counter()

for f in files:
    evs = list(load(f))
    prov = None; model = ""
    for o in evs:
        if o.get("type") == "assistant/message":
            src = ((o.get("data") or {}).get("message") or {}).get("source") or {}
            if src.get("provider"):
                prov = src.get("provider"); model = src.get("model") or ""
                break
    if prov is None:
        for o in evs:
            if o.get("type") == "model/selection":
                prov = (o.get("data") or {}).get("provider")
    if prov is None or prov == "layerfs-mock":
        excluded[prov or "no-model"] += 1
        continue
    pm = (prov + "/" + model).lower()
    family = "deepseek" if "deepseek" in pm else ("openai-codex" if "codex" in pm or "gpt" in pm else ("glm" if "glm" in pm else "xai/other"))
    for key in ("ALL", family):
        g = G[key]
        g["sessions"] += 1
    _m0 = G[family]["model_ms"]; _s0 = G[family]["step_ms"]
    st = None; msg_t = None; calls = {}; intervals = []; last_res = None
    tshell = tcalls = tfile = 0; in_turn = False
    step_has_shell = False

    def add(field, val, fam=family):
        for key in ("ALL", fam):
            G[key][field] += val

    for o in evs:
        t = o.get("type"); ts = o.get("time"); d = o.get("data") or {}
        if t == "turn/start":
            in_turn = True; tshell = tcalls = tfile = 0; last_res = None
            add("turns", 1)
        elif t == "turn/end" and in_turn:
            for key in ("ALL", family):
                G[key]["turn_shell"].append(tshell); G[key]["turn_calls"].append(tcalls); G[key]["turn_file"].append(tfile)
            in_turn = False
        elif t == "step/start":
            st = ts; msg_t = None; calls = {}; intervals = []; step_has_shell = False
        elif t == "assistant/message" and st is not None:
            msg_t = ts
        elif t == "tool/call":
            name = d.get("name") or ""
            calls[d.get("callId")] = (ts, name)
            c = category(name)
            tcalls += 1
            if c == "shell":
                tshell += 1; step_has_shell = True
            if c == "file":
                tfile += 1
            if last_res is not None and ts >= last_res:
                for key in ("ALL", family):
                    G[key]["gap_ms"].append(ts - last_res)
            for key in ("ALL", family):
                G[key]["top"][name if len(name) < 40 else "other"] += 1
                G[key]["cat"][c] += 1
            if c == "shell":
                try:
                    a = json.loads(d.get("arguments") or "{}")
                    cl = classify(shell_cmd(name, a)) if name in ("bash", "exec_command") else "stdin/job-poll"
                except Exception:
                    cl = "unparsed"
                for key in ("ALL", family):
                    G[key]["shell_cls"][cl] += 1
        elif t == "tool/ptc-dispatch-start":
            name = d.get("name") or ""
            c = category(name)
            for key in ("ALL", family):
                G[key]["sub"][name] += 1
                G[key]["subcat"][c] += 1
            if c == "shell":
                tshell += 1; step_has_shell = True
                a = d.get("arguments") or {}
                cl = classify(a.get("command")) if isinstance(a, dict) else "unparsed"
                for key in ("ALL", family):
                    G[key]["shell_cls"]["ptc:" + cl] += 1
            if c == "file":
                tfile += 1
        elif t == "tool/result":
            msg = d.get("message") or {}
            cid = msg.get("toolCallId") or (msg.get("source") or {}).get("callId")
            if cid in calls:
                c0, name = calls[cid]
                intervals.append((c0, ts))
                for key in ("ALL", family):
                    G[key]["call_ms"][category(name)].append(ts - c0)
            last_res = ts
        elif t == "step/end" and st is not None and msg_t is not None:
            iv = sorted(intervals); u = 0; cs = ce = None
            for a, b in iv:
                if cs is None:
                    cs, ce = a, b
                elif a <= ce:
                    ce = max(ce, b)
                else:
                    u += ce - cs; cs, ce = a, b
            if cs is not None:
                u += ce - cs
            for key in ("ALL", family):
                g = G[key]
                g["steps"] += 1; g["model_ms"] += msg_t - st; g["tool_ms"] += u; g["step_ms"] += ts - st
                g["model_step_ms"].append(msg_t - st)
                if calls:
                    g["steps_with_tools"] += 1
                if step_has_shell:
                    g["steps_with_shell"] += 1
            st = None
    _dm = G[family]["model_ms"] - _m0; _ds = G[family]["step_ms"] - _s0
    if _ds > 60000:
        SESS["ALL"].append(_dm / _ds); SESS[family].append(_dm / _ds)

out = {"files": len(files), "excluded": dict(excluded), "groups": {}}
for key, g in G.items():
    calls_top = sum(g["cat"].values())
    allcalls = Counter(g["cat"]); allcalls.update(g["subcat"]); allcalls.pop("code", None)
    out["groups"][key] = {
        "sessions": g["sessions"], "turns": g["turns"], "steps": g["steps"],
        "steps_with_tools": g["steps_with_tools"], "steps_with_shell": g["steps_with_shell"],
        "hours_in_steps": round(g["step_ms"] / 3.6e6, 2),
        "per_session_frac_model (sessions with >60 s of steps)": {"n": len(SESS[key]), "p10": pct(SESS[key], .1), "median": pct(SESS[key], .5), "p90": pct(SESS[key], .9)},
        "frac_model": round(g["model_ms"] / g["step_ms"], 4) if g["step_ms"] else None,
        "frac_tool": round(g["tool_ms"] / g["step_ms"], 4) if g["step_ms"] else None,
        "frac_rest": round(1 - (g["model_ms"] + g["tool_ms"]) / g["step_ms"], 4) if g["step_ms"] else None,
        "model_step_ms": {"median": pct(g["model_step_ms"], .5), "p10": pct(g["model_step_ms"], .1), "p90": pct(g["model_step_ms"], .9)},
        "top_level_calls": calls_top, "top_level_by_category": dict(g["cat"]),
        "top_names": dict(g["top"].most_common(25)),
        "run_code_subcalls": dict(g["sub"].most_common(20)), "run_code_subcalls_by_category": dict(g["subcat"]),
        "effective_calls_by_category (top-level minus run_code, plus run_code sub-calls)": dict(allcalls),
        "shell_command_buckets": dict(g["shell_cls"]),
        "shell_buckets_merged": (lambda c: {b: sum(v for k, v in c.items() if k.split(":")[-1] == b) for b in set(k.split(":")[-1] for k in c)})(g["shell_cls"]),
        "call_ms_by_category": {k: {"n": len(v), "median": pct(v, .5), "p90": pct(v, .9), "sum_h": round(sum(v) / 3.6e6, 2)} for k, v in g["call_ms"].items()},
        "gap_result_to_next_call_ms": {"n": len(g["gap_ms"]), "p10": pct(g["gap_ms"], .1), "median": pct(g["gap_ms"], .5), "p90": pct(g["gap_ms"], .9)},
        "per_turn": {"n": len(g["turn_calls"]),
                     "calls_median": pct(g["turn_calls"], .5), "calls_mean": round(statistics.mean(g["turn_calls"]), 2) if g["turn_calls"] else None,
                     "shell_median": pct(g["turn_shell"], .5), "shell_mean": round(statistics.mean(g["turn_shell"]), 2) if g["turn_shell"] else None,
                     "file_mean": round(statistics.mean(g["turn_file"]), 2) if g["turn_file"] else None,
                     "turns_with_zero_shell": sum(1 for x in g["turn_shell"] if x == 0),
                     "turns_with_tools_and_zero_shell": sum(1 for x, y in zip(g["turn_shell"], g["turn_calls"]) if x == 0 and y > 0)},
    }
json.dump(out, sys.stdout, indent=1, default=str)

"""Premise analysis over dsh session event logs (~/.dsh/sessions/**/session*.jsonl.zstd).

Read-only. Decompresses with the zstd CLI. Prints ONLY aggregate counts and timings; never prints
prompts, file contents, paths or command arguments. For exec_command calls it classifies the
first program word of the command into coarse buckets and reports bucket counts only.

Timing model per step (dsh event order is: step/start, assistant/message, tool/call..., tool/result..., step/end):
  model_ms = assistant/message.time - step/start.time   (request + generation, incl. reasoning)
  ttft_ms  = first stream chunk time - step/start.time  (when chunk times exist)
  tool_ms  = union of [tool/call.time, tool/result.time] intervals in the step
  other_ms = step/end - step/start - model_ms - tool_ms  (harness overhead)
User idle time between turns is excluded (only step intervals are summed).
"""
import sys, json, glob, os, subprocess, shlex, statistics, re
from collections import Counter, defaultdict

ROOT = os.path.expanduser("~/.dsh/sessions")
files = sorted(glob.glob(f"{ROOT}/*/*/session*.jsonl.zstd"))

READ_CMDS = {"cat", "sed", "head", "tail", "rg", "grep", "ls", "find", "fd", "wc", "nl", "less", "awk", "tree", "stat", "file", "du", "pwd", "diff", "jq", "bat", "realpath", "readlink", "sort", "uniq", "cut", "echo", "printf", "which", "test", "["}
GIT_READ = {"show", "diff", "log", "status", "blame", "grep", "ls-files", "rev-parse", "branch", "describe", "cat-file", "merge-base", "remote", "config", "for-each-ref", "worktree", "shortlog", "reflog", "fetch"}
BUILD = {"cargo", "npm", "npx", "pnpm", "yarn", "node", "make", "cmake", "pytest", "python", "python3", "go", "rustc", "tsc", "bun", "deno", "docker", "pip", "pip3", "uv", "bash", "sh", "zsh", "just", "swift", "xcodebuild", "rustup", "gh", "curl", "limactl", "container", "colima", "psql", "sqlite3", "layerfs", "timeout", "gtimeout", "env", "time", "nohup", "sleep", "kill", "pkill", "ps", "lsof", "open"}
EDIT = {"apply_patch", "patch", "mv", "cp", "rm", "mkdir", "touch", "chmod", "ln", "tee", "perl", "truncate"}


def first_word(cmdstr):
    s = cmdstr.strip()
    # strip leading "cd X &&" sequences and env assignments
    parts = re.split(r"\s*(?:&&|;|\|\|)\s*", s)
    for p in parts:
        try:
            toks = shlex.split(p, posix=True)
        except Exception:
            toks = p.split()
        while toks and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", toks[0]):
            toks = toks[1:]
        if not toks:
            continue
        w = os.path.basename(toks[0])
        if w in ("cd", "set", "export", "source", "."):
            continue
        sub = toks[1] if len(toks) > 1 else ""
        return w, sub
    return "", ""


def classify(cmd_arg):
    # exec_command arguments: {"cmd": "..."} or {"cmd": [...]} ; shell wrappers like bash -lc "..."
    c = cmd_arg
    if isinstance(c, list):
        if len(c) >= 3 and os.path.basename(c[0]) in ("bash", "zsh", "sh") and c[1] in ("-lc", "-c"):
            c = c[2]
        else:
            c = " ".join(map(str, c))
    if not isinstance(c, str):
        return "unknown"
    w, sub = first_word(c)
    if w in ("bash", "zsh", "sh"):
        m = re.search(r"-l?c\s+(['\"])(.*)\1", c, re.S)
        if m:
            w, sub = first_word(m.group(2))
    if w == "git":
        return "git-read" if sub in GIT_READ else "git-write"
    if w in READ_CMDS:
        return "read/search"
    if w in EDIT:
        return "edit/fs-mutate"
    if w in BUILD:
        return "build/test/run"
    return "other" if w else "unknown"


def load(path):
    p = subprocess.run(["zstd", "-dc", path], capture_output=True, timeout=100)
    for line in p.stdout.splitlines():
        try:
            yield json.loads(line)
        except Exception:
            continue


per_session = []
tool_names = Counter()
exec_classes = Counter()
call_durs = defaultdict(list)
gaps_result_to_next_call = []  # ms between a tool result and the next tool call (same turn)
providers = Counter()

for f in files:
    cwdkey = f.split("/")[-3]
    sess = {"cwdkey": cwdkey, "steps": 0, "model_ms": 0, "tool_ms": 0, "step_ms": 0, "ttft": [], "model_steps": [],
            "calls": 0, "exec_calls": 0, "file_calls": 0, "turns": 0, "provider": None, "model": None,
            "tool_names": Counter()}
    st = None
    calls = {}
    intervals = []
    msg_t = None
    first_chunk = None
    last_result_t = None
    last_turn = None
    for o in load(f):
        t = o.get("type")
        ts = o.get("time")
        d = o.get("data") or {}
        if t == "model/selection":
            sess["provider"] = d.get("provider"); sess["model"] = d.get("model")
        elif t == "turn/start":
            sess["turns"] += 1
            last_result_t = None
        elif t == "step/start":
            st = ts; calls = {}; intervals = []; msg_t = None; first_chunk = None
        elif t == "assistant/message" and st is not None:
            msg_t = ts
            src = (d.get("message") or {}).get("source") or {}
            if src.get("provider"):
                sess["provider"] = src.get("provider"); sess["model"] = src.get("model")
            for s in d.get("stream") or []:
                if isinstance(s, dict):
                    tt = s.get("time") or s.get("time0")
                    if isinstance(tt, int):
                        first_chunk = tt if first_chunk is None else min(first_chunk, tt)
        elif t == "tool/call":
            name = d.get("name")
            calls[d.get("callId")] = (ts, name)
            tool_names[name] += 1
            sess["tool_names"][name] += 1
            sess["calls"] += 1
            if last_result_t is not None:
                gaps_result_to_next_call.append(ts - last_result_t)
            if name == "exec_command":
                try:
                    a = json.loads(d.get("arguments") or "{}")
                    exec_classes[classify(a.get("cmd", a.get("command")))] += 1
                except Exception:
                    exec_classes["unparsed"] += 1
        elif t == "tool/result":
            msg = d.get("message") or {}
            cid = msg.get("toolCallId") or (msg.get("source") or {}).get("callId")
            if cid in calls:
                c0, name = calls[cid]
                intervals.append((c0, ts))
                call_durs[name].append(ts - c0)
            last_result_t = ts
        elif t == "step/end" and st is not None and msg_t is not None:
            total = ts - st
            model = msg_t - st
            # union of tool intervals
            iv = sorted(intervals)
            u = 0; cs = ce = None
            for a, b in iv:
                if cs is None:
                    cs, ce = a, b
                elif a <= ce:
                    ce = max(ce, b)
                else:
                    u += ce - cs; cs, ce = a, b
            if cs is not None:
                u += ce - cs
            sess["steps"] += 1
            sess["model_ms"] += model
            sess["tool_ms"] += u
            sess["step_ms"] += total
            sess["model_steps"].append(model)
            if first_chunk is not None and first_chunk >= st:
                sess["ttft"].append(first_chunk - st)
            st = None
    if sess["steps"]:
        per_session.append(sess)
        providers[(sess["provider"], sess["model"])] += 1

json.dump({
    "files": len(files),
    "sessions": [{k: (dict(v) if isinstance(v, Counter) else v) for k, v in s.items() if k not in ("ttft", "model_steps")}
                 | {"ttft_med": statistics.median(s["ttft"]) if s["ttft"] else None,
                    "model_step_med": statistics.median(s["model_steps"])}
                 for s in per_session],
    "tool_names": tool_names,
    "exec_classes": exec_classes,
    "call_durs": {k: {"n": len(v), "median": statistics.median(v), "p90": sorted(v)[int(0.9 * (len(v) - 1))],
                      "sum": sum(v)} for k, v in call_durs.items() if v},
    "gaps": {"n": len(gaps_result_to_next_call),
             "median": statistics.median(gaps_result_to_next_call) if gaps_result_to_next_call else None,
             "p10": sorted(gaps_result_to_next_call)[len(gaps_result_to_next_call) // 10] if gaps_result_to_next_call else None,
             "p90": sorted(gaps_result_to_next_call)[int(0.9 * (len(gaps_result_to_next_call) - 1))] if gaps_result_to_next_call else None},
    "providers": {f"{k[0]}/{k[1]}": v for k, v in providers.items()},
}, sys.stdout, default=str)

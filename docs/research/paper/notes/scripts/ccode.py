"""Premise analysis over local Claude Code transcripts (~/.claude/projects/**/*.jsonl). Read-only.
Prints aggregates only: no prompts, file contents, paths or command text.

Timeline method (per transcript file, records in file order, ISO timestamps):
  Each gap between consecutive timestamped conversation records is attributed to what ENDS the gap:
    - next record is an assistant record          -> model time
    - next record is a user record with tool_result -> tool time
    - next record is a human prompt (user text, not tool_result, not meta) -> idle (excluded)
  Gaps longer than IDLE_CAP_S are counted as idle regardless (excluded).
Tool calls are counted from assistant tool_use blocks; per-call duration = tool_result ts - tool_use ts.
Bash commands are bucketed by first program word only.
"""
import sys, os, json, glob, statistics, re
from datetime import datetime
from collections import Counter, defaultdict
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shellcls import classify  # noqa: E402

IDLE_CAP_S = 900
FILE = {"Read", "Write", "Edit", "MultiEdit", "NotebookEdit", "Glob", "Grep", "LS", "NotebookRead"}
SHELL = {"Bash", "BashOutput", "KillShell", "KillBash", "Monitor", "PowerShell"}
WEB = {"WebFetch", "WebSearch"}


def cat(name):
    if name in FILE:
        return "file"
    if name in SHELL:
        return "shell"
    if name in WEB:
        return "web"
    if name.startswith("mcp__"):
        return "mcp"
    if name in ("Agent", "Task", "TodoWrite", "TaskCreate", "TaskUpdate", "TaskList", "TaskGet", "TaskStop", "SendMessage",
                "Skill", "AskUserQuestion", "ExitPlanMode", "EnterPlanMode", "ToolSearch", "TaskOutput"):
        return "agent/meta"
    return "other"


def ts(s):
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
    except Exception:
        return None


def one(path):
    r = {"model_s": 0.0, "tool_s": 0.0, "idle_s": 0.0, "calls": Counter(), "cats": Counter(), "bash_cls": Counter(),
         "call_s": defaultdict(list), "prompts": 0, "prompt_shell": [], "prompt_calls": [], "assistant_msgs": 0,
         "gaps_tool_to_next_call": []}
    prev_t = None
    pending = {}
    pshell = pcalls = 0
    started = False
    last_result_t = None
    try:
        fh = open(path, "rb")
    except Exception:
        return r
    for raw in fh:
        if b'"timestamp"' not in raw:
            continue
        if not (raw.startswith(b'{"parentUuid"') or b'"type":"user"' in raw[:400] or b'"type":"assistant"' in raw[:400]):
            # quick skip for non-conversation records
            if b'"role"' not in raw[:2000]:
                continue
        try:
            o = json.loads(raw)
        except Exception:
            continue
        typ = o.get("type")
        if typ not in ("user", "assistant"):
            continue
        t = ts(o.get("timestamp", ""))
        if t is None:
            continue
        msg = o.get("message") or {}
        content = msg.get("content")
        kind = None
        if typ == "assistant":
            kind = "assistant"
        else:
            if isinstance(content, list) and any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content):
                kind = "tool_result"
            elif o.get("isMeta") or o.get("isCompactSummary"):
                kind = "meta"
            else:
                kind = "prompt"
        if prev_t is not None and t >= prev_t:
            gap = t - prev_t
            if gap > IDLE_CAP_S or kind in ("prompt", "meta"):
                r["idle_s"] += gap
            elif kind == "assistant":
                r["model_s"] += gap
            elif kind == "tool_result":
                r["tool_s"] += gap
        prev_t = t
        if kind == "prompt":
            if started:
                r["prompt_shell"].append(pshell); r["prompt_calls"].append(pcalls)
            started = True; pshell = pcalls = 0; r["prompts"] += 1; last_result_t = None
        elif kind == "assistant":
            r["assistant_msgs"] += 1
            if isinstance(content, list):
                for b in content:
                    if isinstance(b, dict) and b.get("type") == "tool_use":
                        name = b.get("name") or ""
                        c = cat(name)
                        r["calls"][name if not name.startswith("mcp__") else "mcp__*"] += 1
                        r["cats"][c] += 1
                        pcalls += 1
                        if c == "shell":
                            pshell += 1
                        if name == "Bash":
                            inp = b.get("input") or {}
                            r["bash_cls"][classify(inp.get("command"))] += 1
                        pending[b.get("id")] = (t, c)
                        if last_result_t is not None:
                            r["gaps_tool_to_next_call"].append(t - last_result_t)
                            last_result_t = None
        elif kind == "tool_result":
            for b in content:
                if isinstance(b, dict) and b.get("type") == "tool_result":
                    p = pending.pop(b.get("tool_use_id"), None)
                    if p:
                        r["call_s"][p[1]].append(t - p[0])
            last_result_t = t
    if started:
        r["prompt_shell"].append(pshell); r["prompt_calls"].append(pcalls)
    return r


def pct(v, q):
    v = sorted(v)
    return v[min(len(v) - 1, int(q * (len(v) - 1)))] if v else None


if __name__ == "__main__":
    files = sorted(glob.glob(os.path.expanduser("~/.claude/projects/**/*.jsonl"), recursive=True))
    with Pool(8) as p:
        res = p.map(one, files, chunksize=4)
    A = {"model_s": 0.0, "tool_s": 0.0, "idle_s": 0.0}
    calls = Counter(); cats = Counter(); bash = Counter(); call_s = defaultdict(list)
    ps = []; pc = []; prompts = 0; msgs = 0; gaps = []; per_file_frac = []
    for r in res:
        for k in A:
            A[k] += r[k]
        calls.update(r["calls"]); cats.update(r["cats"]); bash.update(r["bash_cls"])
        for k, v in r["call_s"].items():
            call_s[k].extend(v)
        ps += r["prompt_shell"]; pc += r["prompt_calls"]; prompts += r["prompts"]; msgs += r["assistant_msgs"]
        gaps += r["gaps_tool_to_next_call"]
        act = r["model_s"] + r["tool_s"]
        if act > 60:
            per_file_frac.append(r["model_s"] / act)
    act = A["model_s"] + A["tool_s"]
    out = {
        "files": len(files), "files_with_activity_gt_60s": len(per_file_frac), "human_prompts": prompts,
        "assistant_records": msgs,
        "active_hours": round(act / 3600, 1), "idle_hours_excluded": round(A["idle_s"] / 3600, 1),
        "frac_model": round(A["model_s"] / act, 4), "frac_tool": round(A["tool_s"] / act, 4),
        "per_file_frac_model": {"p10": pct(per_file_frac, .1), "median": pct(per_file_frac, .5), "p90": pct(per_file_frac, .9)},
        "calls_by_category": dict(cats), "top_tools": dict(calls.most_common(30)),
        "bash_buckets": dict(bash),
        "call_seconds_by_category": {k: {"n": len(v), "median": round(pct(v, .5), 3), "p90": round(pct(v, .9), 3), "sum_h": round(sum(v) / 3600, 2)} for k, v in call_s.items() if v},
        "gap_result_to_next_tool_call_s": {"n": len(gaps), "p10": round(pct(gaps, .1), 2), "median": round(pct(gaps, .5), 2), "p90": round(pct(gaps, .9), 2)},
        "per_prompt": {"n": len(pc), "calls_median": pct(pc, .5), "calls_mean": round(statistics.mean(pc), 2),
                       "shell_median": pct(ps, .5), "shell_mean": round(statistics.mean(ps), 2),
                       "prompts_with_tools_and_zero_shell": sum(1 for a, b in zip(ps, pc) if a == 0 and b > 0),
                       "prompts_with_zero_tools": sum(1 for b in pc if b == 0)},
    }
    json.dump(out, sys.stdout, indent=1)

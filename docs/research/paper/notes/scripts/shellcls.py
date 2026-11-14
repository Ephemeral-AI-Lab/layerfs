"""Shell command bucketing shared by premise2.py and ccode.py (first program word only)."""
import os, re, shlex

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



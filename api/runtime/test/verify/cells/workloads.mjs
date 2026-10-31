// The twelve R7 cells (#516, #527): bodies, setup and one verifier per cell.
// The bodies are byte-exact copies of `CF` in
// benchmark/cluster-two/cells/workloads.py at 196c714b0b^ (MK1000, MKTREE, DD);
// each runs as `/bin/bash -o pipefail -c "{ <body>; } >/dev/null"` like that
// file's COMMANDS. A verifier is one bash script run by `workspace.exec` in a
// fresh mount of the committed Branch; it prints `VERIFY OK` and exits 0, or
// prints `VERIFY FAIL <reason>` and exits 1. It uses only tools whose options
// GNU (the Linux image), BSD (macOS native runs commands on the Mac) and
// busybox share: find, sort, cksum, wc, awk, cmp -n, seq, sed, tr.
import { createHash } from "node:crypto";

const MK1000 = "for i in $(seq 1 1000); do echo $i > f$i; done";
const MKTREE = "for a in $(seq 1 10); do for b in $(seq 1 10); do mkdir -p $a/$b; for c in $(seq 1 10); do touch $a/$b/$c; done; done; done";
const DD = "dd if=/dev/zero of=big bs=1M count=64 status=none";
const BIG = 67108864;

/** cell -> [body, setup body ("" when the base is empty)] */
export const CF = {
  C01: [MK1000, ""],
  C02: [MK1000 + "; for i in $(seq 1 1000); do stat f$i; done", ""],
  C03: [MK1000 + "; rm f*", ""],
  C04: [MKTREE, ""],
  C05: [MKTREE + "; find . -type f | wc -l", ""],
  C06: [DD, ""],
  C07: [DD + "; cp big big2", ""],
  C08: [DD + "; cat big > /dev/null", ""],
  C09: ["cat big > /dev/null", DD],
  C10: ["cp big big2", DD],
  C11: [DD + " conv=notrunc", DD],
  C12: ["git init -q; for i in $(seq 1 100); do echo $i > f$i; done; git add -A; git -c user.email=a@b -c user.name=a commit -qm init", ""],
};

export const sha256 = (text) => createHash("sha256").update(text).digest("hex");
/** The command the cell runs, exactly as workloads.py's COMMANDS has it. */
export const command = (cell) => `{ ${CF[cell][0]}; } >/dev/null`;
const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;

/**
 * Settings put in place before the clock starts, outside the hashed command:
 * a C locale and no system or user Git configuration (on macOS native the
 * command runs as this user, whose ~/.gitconfig could add filters or signing),
 * the fixed dates of workloads.py's ENV.
 */
const PRELUDE = "export LANG=C LC_ALL=C GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_DATE=2026-10-03T00:00:00Z GIT_COMMITTER_DATE=2026-10-03T00:00:00Z";

/**
 * The exec line: the command timed on the Sandbox's own clock. Its own output
 * goes to /dev/null inside; the only line printed is
 * `LFS527 <exit code> <start ns> <end ns>`.
 */
export function timedLine(text) {
  return `${PRELUDE}; s=$(date +%s%N); /bin/bash -o pipefail -c ${quote(text)}; rc=$?; e=$(date +%s%N); echo "LFS527 $rc $s $e"`;
}

/** { rc, ms } from a timedLine's stdout; ms is null unless both stamps are nanoseconds since the
 * epoch (at least 16 digits): busybox `date` has no `%N` and prints whole seconds. */
export function parseTimed(stdout) {
  const found = /^LFS527 (\S+) (\S+) (\S+)$/m.exec(stdout ?? "");
  if (!found) return { rc: null, ms: null };
  const rc = /^\d+$/.test(found[1]) ? Number(found[1]) : null;
  const ns = (text) => /^\d{16,}$/.test(text);
  const ms = ns(found[2]) && ns(found[3]) ? Number(BigInt(found[3]) - BigInt(found[2])) / 1e6 : null;
  return { rc, ms };
}

// ---- verifiers ------------------------------------------------------------

const HEAD = `${PRELUDE}
fail() { echo "VERIFY FAIL $*"; exit 1; }
count() { wc -l | tr -d ' '; }
# exact names: the sorted list of every path under the root, compared by cksum
names() { find . -mindepth 1 "$@" | LC_ALL=C sort | cksum; }`;

/** f<i> holds "<i>\\n" and nothing else, for i in 1..n */
const numbered = (n) => `
[ "$(find . -mindepth 1 -path ./.git -prune -o ! -type f -print | count)" = 0 ] || fail "an entry other than a regular file"
[ "$(cat $(seq 1 ${n} | sed 's/^/f/') | cksum)" = "$(seq 1 ${n} | cksum)" ] || fail "f1..f${n} contents"
wc -c $(seq 1 ${n} | sed 's/^/f/') | awk '$2 != "total" && $1 != length(substr($2, 2)) + 1 { print "size", $2; bad = 1 } END { exit bad }' || fail "a file of the wrong size"`;

const bigs = (files) => files.map((name) => `
[ -f ${name} ] || fail "${name} missing"
[ "$(wc -c < ${name} | tr -d ' ')" = ${BIG} ] || fail "${name} size $(wc -c < ${name})"
cmp -n ${BIG} ${name} /dev/zero >/dev/null || fail "${name} is not ${BIG} zero bytes"`).join("");

const exactNames = (list, ...findArgs) => `
[ "$(names ${findArgs.join(" ")})" = "$({ ${list}; } | LC_ALL=C sort | cksum)" ] || fail "names: $(find . -mindepth 1 ${findArgs.join(" ")} | count) entries; unexpected: $(find . -mindepth 1 ${findArgs.join(" ")} | grep -v -E '^[.]/(f[0-9]+|[0-9]+(/[0-9]+)*|big2?)$' | head -5 | tr '\\n' ' ')"`;

const FILES = (n) => `for i in $(seq 1 ${n}); do echo ./f$i; done`;
const TREE = "for a in $(seq 1 10); do echo ./$a; for b in $(seq 1 10); do echo ./$a/$b; for c in $(seq 1 10); do echo ./$a/$b/$c; done; done; done";

const CHECKS = {
  numbered: exactNames(FILES(1000)) + numbered(1000),
  empty: `
[ "$(find . -mindepth 1 | count)" = 0 ] || fail "root is not empty: $(find . -mindepth 1 | count) entries"`,
  tree: exactNames(TREE) + `
[ "$(find . -mindepth 1 -type d | count)" = 110 ] || fail "directories"
[ "$(find . -type f | count)" = 1000 ] || fail "files"
[ "$(find . -type f -size +0c | count)" = 0 ] || fail "a file is not empty"`,
  big: exactNames("echo ./big") + bigs(["big"]),
  big2: exactNames("echo ./big; echo ./big2") + bigs(["big", "big2"]),
  git: exactNames(FILES(100), "-path ./.git -prune -o -print") + `
[ -d .git ] || fail ".git missing"` + numbered(100) + `
g() { git -c safe.directory='*' -c core.fsmonitor=false "$@"; }
[ "$(g rev-list --count HEAD)" = 1 ] || fail "commits: $(g rev-list --count HEAD 2>&1)"
[ "$(g ls-files | count)" = 100 ] || fail "tracked files"
[ "$(g log -1 --format=%s)" = init ] || fail "commit subject"
[ -z "$(g status --porcelain)" ] || fail "status not clean: $(g status --porcelain | head -3)"`,
};

const VERIFIER = {
  C01: "numbered", C02: "numbered", C03: "empty", C04: "tree", C05: "tree", C06: "big",
  C07: "big2", C08: "big", C09: "big", C10: "big2", C11: "big", C12: "git",
};

/** The verifier script of a cell. */
export const verifier = (cell) => `${HEAD}${CHECKS[VERIFIER[cell]]}
echo "VERIFY OK"`;

/** One untimed exec on the Sandbox's side: where it runs. */
export const IDENTITY = [
  "echo kernel=$(uname -sr)",
  "echo machine=$(uname -m)",
  "echo cpus=$(nproc 2>/dev/null || sysctl -n hw.ncpu)",
  "echo memKiB=$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo 2>/dev/null)",
  "echo memBytes=$(sysctl -n hw.memsize 2>/dev/null)",
  "echo cpuMax=$(cat /sys/fs/cgroup/cpu.max 2>/dev/null)",
  "echo id=$(id -u):$(id -g)",
].join("; ");

/** IDENTITY's stdout as an object. */
export const parseIdentity = (stdout) => Object.fromEntries((stdout ?? "").split("\n").filter((line) => line.includes("=")).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1) || null]));

export const CELLS = Object.keys(CF);

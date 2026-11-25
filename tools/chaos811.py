#!/usr/bin/env python3
"""#811 V2: one multi-host chaos run against one PostgreSQL Store.

One `run` is one command: it creates a private Docker network, one
`postgres:17` server, N Linux "host" containers (each running one node, the
daemon's in-process Store service built as the `chaos811` test binary of
layerfs-store-service), one admin container, and one network-fault sidecar
per host that shares that host's network namespace (`--network
container:<host>`, NET_ADMIN on the sidecar only). It runs the seeded
workload, injects the seeded faults, checks the seven invariants of #811 V2
from an admin connection and the node logs, writes `receipts.json` and
removes the run's containers. A failed run keeps its stopped containers for
diagnosis; nothing in a results directory is ever deleted.

  python3 tools/chaos811.py stage --resources <resources.json>
  python3 tools/chaos811.py run --seed 7

`stage` copies the built test binary out of the worktree's Linux target
volume into a pinned directory (by SHA-256). `run` must be wrapped in a
<=120 s wall timeout by the caller; it also bounds itself (RUN_BUDGET).

Nothing here is product code. Faults touch only containers this script
created (prefix `lfs-811-v2-`); no port is published.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

PREFIX = "lfs-811-v2-"
PG_IMAGE = "postgres:17"
HOST_IMAGE = "postgres:17-bookworm"  # glibc and libssl3 of the bookworm build image
NET_IMAGE = "lfs-811-v2-net:1"  # alpine 3.20 + iproute2, iptables (built locally)
STATE = Path.home() / ".cache" / "layerfs-811"
BIN_ROOT = STATE / "v2-bin"
RUNS = STATE / "v2-runs"
RUN_BUDGET = 112.0
PASSWORD = "chaos"
DEFINITE = {
    "HeadMoved",
    "Conflict",
    "Busy",
    "LaneMerge",
    "LiveConflict",
    "StaleStore",
    "MountsActive",
    "NoSuchBranch",
    "Removed",
    "UpToDate",
    "Conflicted",
    "Closed",
    "Unresolved",
}


class Budget(Exception):
    pass


def sh(args: list[str], timeout: float = 20.0, check: bool = True) -> subprocess.CompletedProcess:
    done = subprocess.run(args, check=False, capture_output=True, text=True, timeout=timeout)
    if check and done.returncode != 0:
        raise RuntimeError(f"{' '.join(args)} -> {done.returncode}: {done.stderr.strip()[:400]}")
    return done


def stage(resources: str) -> None:
    res = json.loads(Path(resources).read_text())
    volume = res["linux_target_volume"]
    tmp = BIN_ROOT / "incoming"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    script = (
        "f=$(ls -t /t/debug/deps/chaos811-* | grep -v '\\.d$' | head -1); "
        'echo "$f"; cp "$f" /o/chaos811 && chmod 0555 /o/chaos811'
    )
    out = sh(
        ["docker", "run", "--rm", "-v", f"{volume}:/t:ro", "-v", f"{tmp}:/o", "alpine:3", "sh", "-c", script],
        timeout=60,
    )
    digest = hashlib.sha256((tmp / "chaos811").read_bytes()).hexdigest()
    final = BIN_ROOT / digest[:16]
    if not final.exists():
        tmp.rename(final)
    (BIN_ROOT / "current").write_text(f"{final}\n{digest}\n{out.stdout.strip()}\n")
    print(json.dumps({"binary": str(final / "chaos811"), "sha256": digest, "source": out.stdout.strip()}))


class Run:
    def __init__(self, seed: int, nodes: int, workload: float):
        self.started = time.monotonic()
        self.seed = seed
        self.nodes = nodes
        self.workload = workload
        stamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
        self.dir = RUNS / f"{stamp}-seed{seed}"
        self.dir.mkdir(parents=True, exist_ok=False)
        self.p = f"{PREFIX}s{seed}-{stamp[9:15].lower()}"
        lines = (BIN_ROOT / "current").read_text().split("\n")
        self.bin_dir, self.bin_sha = Path(lines[0]), lines[1]
        self.rng = random.Random(seed)
        self.events: list[dict] = []
        self.incarnation = [0] * nodes
        self.containers: list[str] = []
        self.network = f"{self.p}-net"
        self.ips: dict[str, str] = {}

    # -- infrastructure -------------------------------------------------
    def left(self) -> float:
        return RUN_BUDGET - (time.monotonic() - self.started)

    def guard(self, need: float = 0.0) -> None:
        if self.left() < need:
            raise Budget(f"run budget exhausted ({self.left():.1f}s left, {need}s needed)")

    def note(self, kind: str, **fields) -> None:
        event = {"t": int(time.time() * 1000), "kind": kind, **fields}
        self.events.append(event)
        with open(self.dir / "driver.jsonl", "a") as f:
            f.write(json.dumps(event) + "\n")

    def url(self) -> str:
        return (
            f"host={self.p}-pg port=5432 user=postgres password={PASSWORD} dbname=postgres "
            "sslmode=disable connect_timeout=5 keepalives_idle=5 keepalives_interval=2 "
            "keepalives_retries=2 tcp_user_timeout=10"
        )

    def host(self, n: int) -> str:
        return f"{self.p}-h{n}"

    def admin(self) -> str:
        return f"{self.p}-adm"

    def start_containers(self) -> None:
        sh(["docker", "network", "create", "--internal", self.network])
        pg = f"{self.p}-pg"
        sh(
            [
                "docker",
                "run",
                "-d",
                "--name",
                pg,
                "--network",
                self.network,
                "-e",
                f"POSTGRES_PASSWORD={PASSWORD}",
                PG_IMAGE,
                "-c",
                "max_connections=300",
                "-c",
                "tcp_keepalives_idle=5",
                "-c",
                "tcp_keepalives_interval=2",
                "-c",
                "tcp_keepalives_count=2",
            ]
        )
        self.containers.append(pg)
        names = [self.host(n) for n in range(self.nodes)] + [self.admin()]
        for name in names:
            # `sleep 170`: a container this run leaves behind stops by itself.
            sh(
                [
                    "docker",
                    "run",
                    "-d",
                    "--name",
                    name,
                    "--network",
                    self.network,
                    "--entrypoint",
                    "sleep",
                    "-v",
                    f"{self.bin_dir}:/bin811:ro",
                    "-v",
                    f"{self.dir}:/out",
                    HOST_IMAGE,
                    "170",
                ]
            )
            self.containers.append(name)
        for n in range(self.nodes):
            side = f"{self.p}-net{n}"
            sh(
                [
                    "docker",
                    "run",
                    "-d",
                    "--name",
                    side,
                    "--network",
                    f"container:{self.host(n)}",
                    "--cap-add",
                    "NET_ADMIN",
                    NET_IMAGE,
                    "sleep",
                    "170",
                ]
            )
            self.containers.append(side)
        for name in names + [pg]:
            ip = sh(["docker", "inspect", "-f", "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}", name])
            self.ips[name] = ip.stdout.strip()
        self.note("containers", ips=self.ips)
        until = time.monotonic() + 25
        while True:
            ready = sh(
                ["docker", "exec", pg, "psql", "-h", "127.0.0.1", "-U", "postgres", "-tAc", "SELECT 1"],
                check=False,
            )
            if ready.returncode == 0:
                break
            if time.monotonic() > until:
                raise RuntimeError("postgres not ready in 25 s")
            time.sleep(0.3)
        self.note("pg_ready")

    def role_env(self, role: str, **extra) -> list[str]:
        env = {
            "CHAOS811_ROLE": role,
            "CHAOS811_URL": self.url(),
            "CHAOS811_OUT": "/out",
            "CHAOS811_RUN": f"s{self.seed}",
            "CHAOS811_SEED": str(self.seed),
            "CHAOS811_NODES": str(self.nodes),
            "RUST_BACKTRACE": "1",
            **{k: str(v) for k, v in extra.items()},
        }
        out = []
        for k, v in env.items():
            out += ["-e", f"{k}={v}"]
        return out

    def role(self, container: str, role: str, timeout: float, log: str, **extra) -> int:
        self.guard(timeout + 4)
        args = ["docker", "exec", *self.role_env(role, **extra), container, "sh", "-c"]
        args.append(f"exec /bin811/chaos811 --exact chaos811_{role} --nocapture --test-threads=1 > /out/{log} 2>&1")
        try:
            code = sh(args, timeout=timeout, check=False).returncode
        except subprocess.TimeoutExpired:
            sh(["docker", "exec", container, "sh", "-c", "pkill -9 -f chaos811 || true"], check=False)
            code = 124
        self.note("role", role=role, container=container, code=code, log=log)
        return code

    def start_node(self, n: int, deadline_ms: int, lull: str) -> None:
        inc = self.incarnation[n]
        (self.dir / f"n{n}").mkdir(exist_ok=True)
        extra = {
            "CHAOS811_NODE": n,
            "CHAOS811_INC": inc,
            "CHAOS811_DEADLINE_MS": deadline_ms,
            "CHAOS811_LULL_MS": lull,
            "CHAOS811_PROXY": "1" if n == 0 else "0",
        }
        cmd = (
            f"echo $$ > /out/n{n}/pid.{inc}; exec /bin811/chaos811 --exact chaos811_node --nocapture "
            f"--test-threads=1 > /out/n{n}/stdout.{inc} 2>&1"
        )
        sh(["docker", "exec", "-d", *self.role_env("node", **extra), self.host(n), "sh", "-c", cmd])
        self.note("node_start", node=n, inc=inc)

    def node_alive(self, n: int) -> bool:
        inc = self.incarnation[n]
        done = sh(
            ["docker", "exec", self.host(n), "sh", "-c", f"kill -0 $(cat /out/n{n}/pid.{inc}) 2>/dev/null"],
            check=False,
        )
        return done.returncode == 0

    def psql(self, sql: str) -> str:
        done = sh(
            [
                "docker",
                "exec",
                f"{self.p}-pg",
                "psql",
                "-h",
                "127.0.0.1",
                "-U",
                "postgres",
                "-tA",
                "-F",
                "|",
                "-c",
                sql,
            ],
            check=False,
        )
        return done.stdout.strip() if done.returncode == 0 else f"ERROR {done.stderr.strip()[:200]}"

    # -- faults ---------------------------------------------------------
    def schedule(self) -> list[tuple[float, str, dict]]:
        r = self.rng
        w = self.workload
        plan: list[tuple[float, str, dict]] = []
        for _ in range(r.choice([1, 2])):
            plan.append((r.uniform(3, w - 8), "kill9", {"node": r.randrange(self.nodes), "down": r.uniform(0.5, 2)}))
        for _ in range(3):
            plan.append((r.uniform(2, w - 3), "terminate", {"want": r.choice(["lane", "busy", "lane", "idle"])}))
        plan.append(
            (r.uniform(3, w - 10), "partition", {"node": r.randrange(self.nodes), "dur": round(r.uniform(1.5, 4), 2)})
        )
        plan.append(
            (
                r.uniform(3, w - 10),
                "loss",
                {"node": r.randrange(self.nodes), "pct": r.choice([10, 20, 30]), "dur": round(r.uniform(3, 6), 2)},
            )
        )
        plan.append((r.uniform(8, w - 12), "pg_restart", {}))
        return sorted(plan, key=lambda x: x[0])

    def net(self, n: int, script: str) -> int:
        return sh(["docker", "exec", f"{self.p}-net{n}", "sh", "-c", script], check=False).returncode

    def inject(self, kind: str, a: dict, later: list) -> None:
        now = time.monotonic()
        pg_ip = self.ips[f"{self.p}-pg"]
        if kind == "kill9":
            n = a["node"]
            inc = self.incarnation[n]
            code = sh(
                ["docker", "exec", self.host(n), "sh", "-c", f"kill -9 $(cat /out/n{n}/pid.{inc})"], check=False
            ).returncode
            self.note("kill9", node=n, inc=inc, code=code)
            later.append((now + a["down"], "restart", {"node": n}))
        elif kind == "terminate":
            excluded = self.ips[self.admin()]
            rows = self.psql(
                "SELECT a.pid, host(a.client_addr), CASE WHEN EXISTS (SELECT 1 FROM pg_locks l "
                "JOIN pg_class c ON c.oid = l.relation WHERE l.pid = a.pid AND c.relname = 'store_lane') "
                "THEN 'lane' WHEN a.state = 'active' OR a.state LIKE 'idle in transaction%' THEN 'busy' "
                "ELSE 'idle' END, a.state FROM pg_stat_activity a WHERE a.backend_type = 'client backend' "
                f"AND a.pid <> pg_backend_pid() AND a.client_addr IS NOT NULL AND host(a.client_addr) <> '{excluded}' "
                "ORDER BY a.pid"
            )
            cands = [line.split("|") for line in rows.splitlines() if line and not line.startswith("ERROR")]
            order = [a["want"]] + [c for c in ("lane", "busy", "idle") if c != a["want"]]
            pick = None
            for want in order:
                group = [c for c in cands if c[2] == want]
                if group:
                    pick = self.rng.choice(group)
                    break
            if pick:
                done = self.psql(f"SELECT pg_terminate_backend({pick[0]})")
                self.note("terminate", want=a["want"], pid=pick[0], addr=pick[1], cat=pick[2], state=pick[3], ok=done)
            else:
                self.note("terminate", want=a["want"], pid=None, note=rows[:200])
        elif kind == "partition":
            n = a["node"]
            code = self.net(n, f"iptables -I OUTPUT -d {pg_ip} -j DROP && iptables -I INPUT -s {pg_ip} -j DROP")
            self.note("partition", node=n, addr=self.ips[self.host(n)], dur=a["dur"], code=code)
            later.append((now + a["dur"], "heal_partition", {"node": n}))
        elif kind == "heal_partition":
            n = a["node"]
            code = self.net(n, f"iptables -D OUTPUT -d {pg_ip} -j DROP; iptables -D INPUT -s {pg_ip} -j DROP")
            self.note("heal_partition", node=n, code=code)
        elif kind == "loss":
            n = a["node"]
            code = self.net(n, f"tc qdisc add dev eth0 root netem loss {a['pct']}% delay 20ms")
            self.note("loss", node=n, addr=self.ips[self.host(n)], pct=a["pct"], dur=a["dur"], code=code)
            later.append((now + a["dur"], "heal_loss", {"node": n}))
        elif kind == "heal_loss":
            n = a["node"]
            code = self.net(n, "tc qdisc del dev eth0 root")
            self.note("heal_loss", node=n, code=code)
        elif kind == "pg_restart":
            t0 = time.time()
            code = sh(["docker", "restart", "-t", "2", f"{self.p}-pg"], timeout=30, check=False).returncode
            self.note("pg_restart", code=code, ms=int((time.time() - t0) * 1000))
        elif kind == "restart":
            n = a["node"]
            self.incarnation[n] += 1
            self.start_node(n, self.deadline_ms, self.lull)

    # -- the run --------------------------------------------------------
    def execute(self) -> dict:
        (self.dir / "seed").write_text(f"{self.seed}\n")
        self.note(
            "run",
            seed=self.seed,
            nodes=self.nodes,
            workload_s=self.workload,
            binary_sha256=self.bin_sha,
            git=sh(["git", "rev-parse", "HEAD"], check=False).stdout.strip(),
            reopen="not exercised: claude/writer-reopen is not merged; V2 runs layerfs_v2_0_0",
        )
        self.start_containers()
        if self.role(self.admin(), "setup", 30, "setup.out") != 0:
            raise RuntimeError("setup failed")
        sh(
            [
                "docker",
                "exec",
                "-d",
                *self.role_env("monitor"),
                self.admin(),
                "sh",
                "-c",
                "exec /bin811/chaos811 --exact chaos811_monitor --nocapture --test-threads=1 > /out/monitor.out 2>&1",
            ]
        )
        t0 = time.monotonic()
        wall0 = int(time.time() * 1000)
        self.deadline_ms = wall0 + int(self.workload * 1000)
        self.lull = f"{wall0 + int(self.workload * 550)}-{wall0 + int(self.workload * 680)}"
        self.note("workload", deadline_ms=self.deadline_ms, lull=self.lull)
        for n in range(self.nodes):
            self.start_node(n, self.deadline_ms, self.lull)
        pending = [(t0 + at, kind, a) for at, kind, a in self.schedule()]
        while pending or time.monotonic() < t0 + self.workload:
            self.guard(30)
            pending.sort(key=lambda x: x[0])
            if pending and pending[0][0] <= time.monotonic():
                _, kind, a = pending.pop(0)
                later: list = []
                self.inject(kind, a, later)
                pending.extend(later)
                continue
            nxt = pending[0][0] if pending else t0 + self.workload
            time.sleep(max(0.0, min(0.2, nxt - time.monotonic())))
        # Quiesce: every live node writes its state file and holds.
        until = time.monotonic() + 25
        missing = set(range(self.nodes))
        while missing and time.monotonic() < until:
            for n in list(missing):
                if (self.dir / f"n{n}" / f"state.{self.incarnation[n]}.json").exists():
                    missing.discard(n)
            time.sleep(0.2)
        self.note("quiesced", missing=sorted(missing))
        self.role(self.admin(), "probe", 15, "probe-quiesce.out", CHAOS811_LABEL="quiesce")
        for n in range(self.nodes):
            (self.dir / f"n{n}" / "release").write_text("release\n")
        until = time.monotonic() + 20
        alive = set(range(self.nodes))
        while alive and time.monotonic() < until:
            for n in list(alive):
                if not self.node_alive(n):
                    alive.discard(n)
            time.sleep(0.3)
        for n in alive:
            inc = self.incarnation[n]
            sh(["docker", "exec", self.host(n), "sh", "-c", f"kill -9 $(cat /out/n{n}/pid.{inc})"], check=False)
        self.note("exited", hung=sorted(alive))
        # Backends of the host containers are gone once their processes are.
        hosts = ",".join(f"'{self.ips[self.host(n)]}'" for n in range(self.nodes))
        until = time.monotonic() + 15
        while time.monotonic() < until:
            left = self.psql(f"SELECT count(*) FROM pg_stat_activity WHERE host(client_addr) IN ({hosts})")
            if left == "0":
                break
            time.sleep(0.3)
        self.note("backends_after_exit", left=left)
        self.role(self.admin(), "probe", 15, "probe-exit.out", CHAOS811_LABEL="exit")
        # The history rows before any compaction can collect a removed Branch.
        self.role(self.admin(), "rows", 15, "rows.out")
        # Settlement: first each host's own compaction admission (it adopts
        # the intents of owners this PID namespace proves gone), then, for
        # rows that remain, the documented operator path, then one more
        # compaction, which must run.
        for n in range(self.nodes):
            self.role(self.host(n), "settle", 25, f"settle-h{n}.out", CHAOS811_LABEL=f"h{n}")
        self.role(self.admin(), "probe", 15, "probe-settled.out", CHAOS811_LABEL="settled")
        settled = load(self.dir / "probe-settled.json") or {"intents": []}
        if [r for r in settled["intents"] if r["state"] != "done"]:
            self.role(self.admin(), "operator", 25, "operator.out")
        self.role(self.host(0), "settle", 25, "settle-final.out", CHAOS811_LABEL="final")
        self.role(self.admin(), "probe", 15, "probe-final.out", CHAOS811_LABEL="final")
        (self.dir / "stop-monitor").write_text("stop\n")
        code = self.role(self.admin(), "dump", 40, "dump.out")
        self.note("dump", code=code)
        time.sleep(0.3)
        return check(self)

    def teardown(self, keep: bool) -> None:
        if keep:
            for name in self.containers:
                sh(["docker", "stop", "-t", "1", name], timeout=20, check=False)
            self.note("kept", containers=self.containers, network=self.network)
            return
        for name in self.containers:
            sh(["docker", "rm", "-f", name], timeout=20, check=False)
        sh(["docker", "network", "rm", self.network], timeout=20, check=False)
        self.note("removed", containers=self.containers, network=self.network)


# -- checks -----------------------------------------------------------------
def jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out = []
    for line in path.read_text().splitlines():
        if line.strip():
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                out.append({"ev": "torn", "raw": line[:200]})
    return out


def load(path: Path):
    return json.loads(path.read_text()) if path.exists() else None


def check(run: Run) -> dict:
    d = run.dir
    dump = load(d / "dump.json")
    ops: list[dict] = []
    for n in range(run.nodes):
        ops += [dict(e, node=n) for e in jsonl(d / f"n{n}" / "ops.jsonl")]
    begins = {e["token"]: e for e in ops if e.get("ev") == "begin"}
    ends = {e["token"]: e for e in ops if e.get("ev") == "end"}
    unanswered = [b for t, b in begins.items() if t not in ends]
    checks: dict = {}
    if dump is None:
        return {"verdict": "FAIL", "checks": {"dump": "missing dump.json"}}
    # Checks 1, 2 and 7 read the rows taken before the last compaction,
    # which collects removed Branches' history; check 3 the dump after it.
    rows = load(d / "rows.json")
    if rows is None:
        return {"verdict": "FAIL", "checks": {"rows": "missing rows.json"}}
    commits = rows["commits"]
    by_id = {c["id"]: c for c in commits}
    branches = {b["name"]: b for b in rows["branches"]}
    name_of = {b["id"]: b["name"] for b in rows["branches"]}

    def first_parent(start: str | None) -> list[str]:
        chain, at, seen = [], start, set()
        while at and at in by_id and at not in seen:
            seen.add(at)
            chain.append(at)
            at = by_id[at]["parent"]
        return chain

    on_chain = {}
    for name, b in branches.items():
        for cid in first_parent(b["head"]):
            on_chain.setdefault(cid, set()).add(name)
    displaced_reach = {}
    for c in commits:
        if c["displaced"]:
            for cid in first_parent(c["displaced"]):
                displaced_reach.setdefault(cid, set()).add(c["id"])

    def placement(cid: str, branch: str) -> str:
        if branch in on_chain.get(cid, set()):
            return "chain"
        if cid in displaced_reach:
            return "displaced"
        b = branches.get(branch)
        if b and b["retired"] == "true":
            return "removed-branch"
        return "LOST"

    # 1. acknowledged Commits are on their chain or explicitly displaced.
    fails1, tally1 = [], {}
    for e in ends.values():
        if e["op"] != "commit":
            continue
        if e["outcome"] in ("Committed", "PublishedThenFailed") and e.get("commit"):
            row = by_id.get(e["commit"])
            if row is None:
                b = branches.get(e["branch"])
                if b and b["retired"] == "true":
                    tally1["removed-branch(row collected)"] = tally1.get("removed-branch(row collected)", 0) + 1
                    continue
                fails1.append(f"{e['token']}: acknowledged {e['commit']} has no row")
                continue
            if row["message"] != e["token"]:
                fails1.append(f"{e['token']}: row message {row['message']}")
            if name_of.get(row["branch"]) != e["branch"]:
                fails1.append(f"{e['token']}: row Branch {name_of.get(row['branch'])} != {e['branch']}")
            where = placement(e["commit"], e["branch"])
            tally1[where] = tally1.get(where, 0) + 1
            if where == "LOST":
                fails1.append(f"{e['token']}: {e['commit']} neither on {e['branch']} nor displaced")
            if where == "displaced":
                for by in displaced_reach[e["commit"]]:
                    displacer = ends.get(by_id[by]["message"] or "", {})
                    if displacer and not str(displacer.get("mode", "")).startswith("force"):
                        fails1.append(f"{e['token']}: displaced by non-forced {by_id[by]['message']}")
        elif e["outcome"] in DEFINITE:
            if any(c["message"] == e["token"] for c in commits):
                fails1.append(f"{e['token']}: refused {e['outcome']} but published")
    for c in commits:
        if c["displaced"]:
            e = ends.get(c["message"] or "")
            if e and not str(e.get("mode", "")).startswith("force"):
                fails1.append(f"{c['message']}: displaced {c['displaced']} without force")
    checks["1_acknowledged_on_chain"] = {"pass": not fails1, "tally": tally1, "failures": fails1[:50]}

    # 2. no attempt token twice (the product's attempt; the harness token).
    attempts = [c["attempt"] for c in commits if c["attempt"]]
    messages = [c["message"] for c in commits if (c["message"] or "").startswith("c811:")]
    dup_attempt = len(attempts) - len(set(attempts))
    dup_message = len(messages) - len(set(messages))
    phantom = [m for m in messages if m not in begins]
    no_attempt = [c["message"] for c in commits if (c["message"] or "").startswith("c811:") and not c["attempt"]]
    checks["2_attempt_once"] = {
        "pass": dup_attempt == 0 and dup_message == 0 and not phantom,
        "attempts": len(attempts),
        "duplicate_attempts": dup_attempt,
        "duplicate_tokens": dup_message,
        "published_without_a_logged_attempt": phantom[:20],
        "workspace_commits_without_attempt": len(no_attempt),
    }

    # 3. heads, roots and the Store audit.
    bad_heads = [h for h in dump["heads"] if h["snapshot"] != "ok" or not h["walk"].startswith("ok:")]
    after = {c["id"] for c in dump["commits"]}
    missing_head_rows = [b["name"] for b in dump["branches"] if b["head"] and b["head"] not in after]
    # The last compaction kept every live Branch's chain and each displaced chain.
    kept = set()
    for b in rows["branches"]:
        if b["retired"] != "true":
            kept.update(first_parent(b["head"]))
    for c in commits:
        if c["displaced"] and c["id"] in kept:
            kept.update(first_parent(c["displaced"]))
    lost_by_compaction = sorted(kept - after)
    moved_heads = [
        b["name"]
        for b in rows["branches"]
        if b["retired"] != "true"
        and b["head"] != next((a["head"] for a in dump["branches"] if a["name"] == b["name"]), "gone")
    ]
    checks["3_heads_and_audit"] = {
        "pass": not bad_heads
        and not missing_head_rows
        and not dump["audit"]
        and not dump["amcheck"]
        and not dump["allocation"]
        and not lost_by_compaction
        and not moved_heads,
        "lost_by_last_compaction": lost_by_compaction[:20],
        "heads_moved_by_last_compaction": moved_heads,
        "audit_ms": dump.get("audit_ms"),
        "branches_checked": len(dump["heads"]),
        "bad_heads": bad_heads,
        "head_rows_missing": missing_head_rows,
        "audit": dump["audit"],
        "amcheck": dump["amcheck"],
        "allocation": dump["allocation"],
    }

    # 4. gc_intent rows and advisory keys against live mounts.
    checks["4_gc_intents_and_keys"] = check_intents(run, ops)

    # 5. no compaction or removal succeeded while a mount of its scope lived.
    checks["5_gc_vs_live_mounts"] = check_scopes(run, ops)

    # 6. allocators.
    checks["6_allocators"] = check_allocators(d)

    # 7. Unknowns resolve to exactly one outcome.
    unknowns = [e for e in ends.values() if e["op"] == "commit" and e["outcome"] in ("Unknown", "Failed")]
    unknowns += [b for b in unanswered if b["op"] == "commit"]
    fails7, resolutions = [], []
    for e in unknowns:
        rows = [c for c in commits if c["message"] == e["token"]]
        if len(rows) > 1:
            fails7.append(f"{e['token']}: {len(rows)} rows")
            res = "MANY"
        elif rows:
            where = placement(rows[0]["id"], e["branch"])
            res = f"installed:{where}"
            if where == "LOST":
                fails7.append(f"{e['token']}: installed {rows[0]['id']} off its chain")
        else:
            res = "not-installed"
        resolutions.append(
            {
                "token": e["token"],
                "answer": e.get("outcome", "unanswered(killed)"),
                "resolution": res,
                "commit": rows[0]["id"] if rows else None,
            }
        )
    retired = {b["name"] for b in dump["branches"] if b["retired"] == "true"}
    quiesce = load(d / "probe-quiesce.json") or {"intents": []}
    exit_rows = load(d / "probe-exit.json") or {"intents": []}
    rows_ws = {r["workspace"] for r in quiesce["intents"] + exit_rows["intents"] if r["workspace"]}
    for e in list(ends.values()) + unanswered:
        answer = e.get("outcome", "unanswered(killed)")
        if answer not in ("Unknown", "Failed", "unanswered(killed)"):
            continue
        if e["op"] == "branch.remove":
            res = "removed" if e["branch"] in retired else "not-removed"
        elif e["op"] == "mount":
            res = "registered" if e.get("ws") in rows_ws else "not-registered"
        elif e["op"] == "unmount":
            res = "row-kept" if e.get("ws") in rows_ws else "released"
        elif e["op"] == "compact":
            res = "see gc_op/monitor incarnation"
        else:
            continue
        resolutions.append({"token": e["token"], "op": e["op"], "answer": answer, "resolution": res})
    with open(d / "resolutions.jsonl", "w") as f:
        f.writelines(json.dumps(r) + "\n" for r in resolutions)
    checks["7_unknowns_resolve"] = {
        "pass": not fails7,
        "unknowns": len(unknowns),
        "resolutions": resolutions,
        "failures": fails7,
    }

    tally: dict = {}
    for e in ends.values():
        key = f"{e['op']}:{e['outcome']}"
        tally[key] = tally.get(key, 0) + 1
    slow = [e for e in ops if e.get("ev") == "slow"]
    verdict = "PASS" if all(c["pass"] for c in checks.values()) else "FAIL"
    return {
        "verdict": verdict,
        "checks": checks,
        "ops": dict(sorted(tally.items())),
        "unanswered_ops": [{"token": b["token"], "op": b["op"]} for b in unanswered],
        "slow_ops": slow,
    }


def mount_intervals(run: Run, ops: list[dict]) -> list[dict]:
    """[m1, end] per answered mount: end is its unmount's start, or the
    death of its node's incarnation (kill -9, the last logged line)."""
    last: dict = {}
    for e in ops:
        key = (e["node"], e.get("inc"))
        last[key] = max(last.get(key, 0), e.get("t1") or e.get("t0") or e.get("t") or 0)
    kills = {(e["node"], e["inc"]): e["t"] for e in run.events if e["kind"] == "kill9"}
    unmounts = {}
    for e in ops:
        if e.get("ev") == "end" and e["op"] == "unmount":
            unmounts[e["ws"]] = e["t0"]
    out = []
    for e in ops:
        if e.get("ev") == "end" and e["op"] == "mount" and e["outcome"] == "Ok":
            key = (e["node"], e["inc"])
            end = unmounts.get(e["ws"]) or kills.get(key) or last.get(key)
            out.append({"ws": e["ws"], "branch": e["branch"], "node": e["node"], "from": e["t1"], "to": end})
    return out


def check_scopes(run: Run, ops: list[dict]) -> dict:
    mounts = mount_intervals(run, ops)
    fails, ok_compactions, ok_removals = [], [], []
    gc = [e for e in ops if e.get("ev") == "end" and e["op"] in ("compact", "branch.remove") and e["outcome"] == "Ok"]
    for s in jsonl(run.dir / "settle.jsonl"):
        if s["outcome"] == "Ok":
            gc.append({"op": "compact", "t0": s["t0"], "t1": s["t1"], "token": f"settle:{s['label']}", "branch": ""})
    for e in gc:
        if e["op"] == "compact":
            ok_compactions.append(e["token"])
            scope = mounts
        else:
            ok_removals.append(f"{e['branch']}")
            scope = [m for m in mounts if m["branch"] == e["branch"]]
        for m in scope:
            if m["from"] <= e["t0"] and m["to"] >= e["t1"]:
                fails.append(f"{e['token']} Ok [{e['t0']},{e['t1']}] under mount {m}")
        if e["op"] == "branch.remove":
            for later in ops:
                if (
                    later.get("ev") == "end"
                    and later["branch"] == e["branch"]
                    and later["t0"] >= e["t1"]
                    and (
                        (later["op"] == "mount" and later["outcome"] == "Ok")
                        or (later["op"] == "commit" and later["outcome"] == "Committed")
                    )
                ):
                    fails.append(f"{later['token']}: {later['op']} on {e['branch']} after its removal")
    return {
        "pass": not fails,
        "ok_compactions": ok_compactions,
        "ok_removals": ok_removals,
        "mount_intervals": len(mounts),
        "failures": fails[:50],
    }


def check_intents(run: Run, ops: list[dict]) -> dict:
    d = run.dir
    quiesce = load(d / "probe-quiesce.json")
    final = load(d / "probe-final.json")
    exit_probe = load(d / "probe-exit.json")
    fails, notes = [], []
    if quiesce is None or final is None:
        return {"pass": False, "failures": ["missing probe"]}
    states = {}
    for n in range(run.nodes):
        p = d / f"n{n}" / f"state.{run.incarnation[n]}.json"
        if p.exists():
            states[n] = json.loads(p.read_text())
        else:
            fails.append(f"node {n}: no quiesce state")
    rows = {r["workspace"]: r for r in quiesce["intents"] if r["kind"] == "mount" and r["workspace"]}
    locks_by_addr: dict = {}
    for lock in quiesce["locks"]:
        locks_by_addr.setdefault(lock["addr"], set()).add((lock["classid"], lock["objid"], lock["objsubid"]))
    faults_on = {}
    for e in run.events:
        if e["kind"] in ("terminate", "pg_restart", "partition", "loss"):
            faults_on.setdefault(e.get("addr") or "*", []).append(e["t"])
    live_ws = set()
    per_node = {}
    for n, s in states.items():
        addr = run.ips[run.host(n)]
        held = locks_by_addr.get(addr, set())
        branches = {m["branch"] for m in s["mounts"]}
        for m in s["mounts"]:
            live_ws.add(m["ws"])
            if m["ws"] not in rows:
                fails.append(f"node {n}: live mount {m['ws']} on {m['branch']} has no gc_intent row")
        expected = 0 if not s["mounts"] else 2 + len(branches)
        per_node[n] = {
            "mounts": len(s["mounts"]),
            "branches": sorted(branches),
            "keys_held": len(held),
            "keys_expected": expected,
            "healthy": s["healthy"],
        }
        if not s["mounts"] and held:
            fails.append(f"node {n}: holds {len(held)} advisory keys with no live mount")
        if s["mounts"] and len(held) != expected:
            mounted_at = min(
                e["t1"]
                for e in ops
                if e.get("ev") == "end" and e["op"] == "mount" and e.get("ws") in {m["ws"] for m in s["mounts"]}
            )
            hit = [t for t in faults_on.get(addr, []) + faults_on.get("*", []) if t >= mounted_at - 60_000]
            msg = f"node {n}: {len(held)} keys held for {len(s['mounts'])} mounts on {sorted(branches)}"
            if len(held) < expected and hit:
                notes.append(msg + " (writer connection hit by a fault; rows still hold the scope)")
            else:
                fails.append(msg)
    orphans = [r for ws, r in rows.items() if ws not in live_ws]
    settled = load(d / "probe-settled.json")
    operator = jsonl(d / "operator.jsonl")
    final_settle = [s for s in jsonl(d / "settle.jsonl") if s["label"] == "final"]
    if not final_settle or final_settle[-1]["outcome"] != "Ok":
        fails.append(f"the last compaction did not run: {final_settle[-1:]}")
    bad_operator = [o for o in operator if not o["released"].startswith("Ok")]
    if bad_operator:
        fails.append(f"operator release refused: {bad_operator[:3]}")
    leftover = [r for r in final["intents"] if r["state"] != "done"]
    if leftover:
        fails.append(f"{len(leftover)} gc_intent rows remain after every owner exited and each host settled")
    if final["locks"]:
        fails.append(f"{len(final['locks'])} advisory locks remain after every node exited")
    return {
        "pass": not fails,
        "per_node": per_node,
        "orphans_at_quiesce": len(orphans),
        "orphan_owners": sorted({r["owner_runtime"] or "" for r in orphans}),
        "rows_after_exit": len(exit_probe["intents"]) if exit_probe else None,
        "rows_after_host_adoption": len(settled["intents"]) if settled else None,
        "rows_removed_by_operator": len(operator),
        "adoption_alone_cleared": bool(settled) and not [r for r in settled["intents"] if r["state"] != "done"],
        "rows_final": len(final["intents"]),
        "leftover": leftover,
        "notes": notes,
        "failures": fails,
    }


def check_allocators(d: Path) -> dict:
    samples = [s for s in jsonl(d / "monitor.jsonl") if "error" not in s]
    errors = len(jsonl(d / "monitor.jsonl")) - len(samples)
    fails, rewinds = [], 0
    seen = {"pack": 0, "ordinal": 0, "serial": 0}
    prev = None
    high: dict = {}
    for s in samples:
        seen["pack"] = max(seen["pack"], s["max_pack_id"])
        seen["ordinal"] = max(seen["ordinal"], s["max_ordinal"])
        seen["serial"] = max(seen["serial"], s["max_history_serial"])
        if s["next_pack_id"] <= seen["pack"]:
            fails.append(f"t={s['t']}: next_pack_id {s['next_pack_id']} <= assigned {seen['pack']}")
        if s["next_ordinal"] <= seen["ordinal"]:
            fails.append(f"t={s['t']}: next_ordinal {s['next_ordinal']} <= assigned {seen['ordinal']}")
        if s["next_history_serial"] <= seen["serial"]:
            fails.append(f"t={s['t']}: next_history_serial {s['next_history_serial']} <= assigned {seen['serial']}")
        if prev:
            if s["next_history_serial"] < prev["next_history_serial"]:
                fails.append(f"t={s['t']}: next_history_serial went back")
            if s["next_pack_id"] < prev["next_pack_id"]:
                fails.append(f"t={s['t']}: next_pack_id went back {prev['next_pack_id']} -> {s['next_pack_id']}")
            if s["next_ordinal"] < prev["next_ordinal"]:
                rewinds += 1
        for part in filter(None, s["scopes"].split(",")):
            scope, value = part.split(":")
            if int(value) < high.get(scope, 0):
                fails.append(f"t={s['t']}: scope {scope[:12]} highwater went back")
            high[scope] = max(high.get(scope, 0), int(value))
        prev = s
    return {
        "pass": not fails and len(samples) > 10,
        "samples": len(samples),
        "sample_errors": errors,
        "ordinal_rewinds": rewinds,
        "final": prev,
        "failures": fails[:30],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("stage")
    s.add_argument("--resources", required=True)
    r = sub.add_parser("run")
    r.add_argument("--seed", type=int, required=True)
    r.add_argument("--nodes", type=int, default=3)
    r.add_argument("--workload", type=float, default=32.0)
    r.add_argument("--keep", action="store_true", help="keep containers even when the run passes")
    args = parser.parse_args()
    if args.cmd == "stage":
        stage(args.resources)
        return 0
    run = Run(args.seed, args.nodes, args.workload)
    receipts: dict = {"seed": args.seed, "dir": str(run.dir), "prefix": run.p}
    try:
        receipts.update(run.execute())
    except Exception as error:  # noqa: BLE001 - every failure is recorded
        receipts.update({"verdict": "FAIL", "error": f"{type(error).__name__}: {error}"})
    receipts["wall_s"] = round(time.monotonic() - run.started, 1)
    receipts["faults"] = [e for e in run.events if e["kind"] not in ("role", "containers", "node_start")]
    receipts["node_starts"] = [e for e in run.events if e["kind"] == "node_start"]
    receipts["binary_sha256"] = run.bin_sha
    receipts["reopen"] = "not exercised (claude/writer-reopen not merged)"
    keep = receipts["verdict"] != "PASS" or args.keep
    try:
        run.teardown(keep)
    except Exception as error:  # noqa: BLE001
        receipts["teardown_error"] = str(error)
    receipts["kept_containers"] = keep
    receipts["wall_s_total"] = round(time.monotonic() - run.started, 1)
    (run.dir / "receipts.json").write_text(json.dumps(receipts, indent=1) + "\n")
    summary = {k: receipts.get(k) for k in ("seed", "verdict", "wall_s_total", "dir", "error")}
    summary["checks"] = {k: v["pass"] for k, v in receipts.get("checks", {}).items()}
    print(json.dumps(summary))
    return 0 if receipts["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())

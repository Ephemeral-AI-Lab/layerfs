#!/usr/bin/env python3
"""#811 V3 driver: the servers and faults around `gap811_v3.rs`.

Every container, network and volume it makes is named `lfs-811-v3-*`; ports are
never published (the tests run in a container on the same private network).
Faults stay inside those containers: `docker network disconnect` partitions a
standby, `docker kill` ends a primary. Each subcommand is one bounded step (run
it under a wall timeout of at most 120 s); state passes between steps through
`state.json` in the run directory, and every command and its full output are
appended to `driver.log` there.

  up-repl RUN [--sync-names s1]   primary pg1 + streaming standby pg2 (app s1)
  failover RUN --sync local|on|remote_apply --mode partition|ack
  setup RUN                       after up-repl: a Store with Branches b1, b2 on pg1
  up-split RUN                    after up-repl: Store on pg1, then promote pg2
  multihost RUN --arm NAME --url TEMPLATE [--gate]
  up-pgbouncer RUN --pool-mode transaction|session [--pool-size N]
  pooled RUN --arm NAME [--pin]
  lane RUN --writers N --seconds S [--lock-ms MS] [--shared]
  lane-multi RUN --containers N --seconds S [--lock-ms MS] [--shared]   one writer per container
  lost-release RUN
  down                            remove every lfs-811-v3-* container and network
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

PREFIX = "lfs-811-v3-"
NET = PREFIX + "net"
PG_IMAGE = "postgres:17"
PGB_IMAGE = os.environ.get("GAP811_PGBOUNCER_IMAGE", "edoburu/pgbouncer:v1.26.1-p0")
USER, PASSWORD, DB = "lfs", "lfs811pw", "lfs"
RESOURCES = Path.home() / ".cache/layerfs-worktrees/811-v3-829c690b0161/resources.json"
WORKTREE = Path(__file__).resolve().parents[1]


def url(host, port=5432, extra=""):
    return f"host={host} port={port} user={USER} password={PASSWORD} dbname={DB} sslmode=disable {extra}".strip()


class Run:
    def __init__(self, path):
        self.dir = Path(path).expanduser()
        self.dir.mkdir(parents=True, exist_ok=True)
        self.state_path = self.dir / "state.json"
        self.state = json.loads(self.state_path.read_text()) if self.state_path.exists() else {}
        self.log = (self.dir / "driver.log").open("a")

    def save(self):
        self.state_path.write_text(json.dumps(self.state, indent=2))

    def sh(self, *args, timeout=60, check=True, quiet=False):
        started = time.time()
        result = subprocess.run(list(args), check=False, capture_output=True, text=True, timeout=timeout)
        self.log.write(
            f"\n$ {' '.join(args)}\n[exit {result.returncode}, {time.time() - started:.2f}s]\n"
            f"{result.stdout}{result.stderr}"
        )
        self.log.flush()
        if not quiet:
            print(f"$ {' '.join(args)} -> {result.returncode}", flush=True)
        if check and result.returncode != 0:
            sys.exit(f"failed: {' '.join(args)}\n{result.stdout}{result.stderr}")
        return result

    def note(self, text):
        print(text, flush=True)
        self.log.write(f"\n# {text}\n")
        self.log.flush()

    def psql(self, container, sql, check=True):
        return self.sh(
            "docker", "exec", "-e", f"PGPASSWORD={PASSWORD}", PREFIX + container,
            "psql", "-h", "127.0.0.1", "-U", USER, "-d", DB, "-X", "-A", "-t", "-v", "ON_ERROR_STOP=1", "-c", sql,
            check=check,
        )  # fmt: skip


def wait_ready(run, container, limit=40):
    until = time.time() + limit
    while time.time() < until:
        ok = run.sh("docker", "exec", PREFIX + container, "pg_isready", "-h", "127.0.0.1", check=False, quiet=True)
        if ok.returncode == 0 and run.psql(container, "SELECT 1", check=False).returncode == 0:
            return
        time.sleep(0.5)
    sys.exit(f"{container} not ready")


def network(run):
    if run.sh("docker", "network", "inspect", NET, check=False, quiet=True).returncode != 0:
        run.sh("docker", "network", "create", NET)


def primary(run, name):
    args = ["-c", "wal_level=replica", "-c", "max_wal_senders=10", "-c", "max_connections=300"]
    args += ["-c", "wal_keep_size=256MB"]
    run.sh(
        "docker", "run", "-d", "--name", PREFIX + name, "--network", NET, "--shm-size=256m",
        "-e", f"POSTGRES_USER={USER}", "-e", f"POSTGRES_PASSWORD={PASSWORD}", "-e", f"POSTGRES_DB={DB}",
        PG_IMAGE, *args,
    )  # fmt: skip
    wait_ready(run, name)
    run.sh(
        "docker", "exec", PREFIX + name, "bash", "-c",
        "echo 'host replication all all scram-sha-256' >> /var/lib/postgresql/data/pg_hba.conf",
    )  # fmt: skip
    run.psql(name, "SELECT pg_reload_conf()")
    run.psql(name, "CREATE EXTENSION IF NOT EXISTS amcheck")


def standby(run, name, of, app):
    script = (
        "set -e; mkdir -p /tmp/standby; chmod 700 /tmp/standby; "
        f"PGPASSWORD={PASSWORD} pg_basebackup -h {PREFIX + of} -U {USER} -D /tmp/standby -X stream -R; "
        f"echo \"primary_conninfo = 'host={PREFIX + of} user={USER} password={PASSWORD} application_name={app}'\" "
        ">> /tmp/standby/postgresql.auto.conf; exec postgres -D /tmp/standby -c max_connections=300"
    )
    run.sh(
        "docker", "run", "-d", "--name", PREFIX + name, "--network", NET, "--user", "postgres",
        "--entrypoint", "bash", PG_IMAGE, "-c", script,
    )  # fmt: skip
    wait_ready(run, name)


def caught_up(run, limit=30):
    until = time.time() + limit
    while time.time() < until:
        out = run.psql(
            "pg1",
            "SELECT coalesce(bool_and(replay_lsn = pg_current_wal_lsn()), false) FROM pg_stat_replication",
            check=False,
        )
        if out.stdout.strip() == "t":
            return
        time.sleep(0.2)
    sys.exit("standby never caught up")


def test_binary(run):
    if "binary" in run.state:
        return run.state["binary"]
    builds = sorted(
        (Path(json.loads(RESOURCES.read_text())["state"]) / "builds").glob("cargo-linux-*/output.log"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )
    for log in builds:
        for line in log.read_text(errors="replace").splitlines():
            if "tests/gap811_v3.rs" in line and "(" in line:
                binary = "/work/" + line.split("(")[1].rstrip(")").strip()
                run.state["binary"] = binary
                run.save()
                return binary
    sys.exit("no gap811_v3 test binary in the build logs")


def test(run, name, env, tag, timeout=110, background=False):
    record = json.loads(RESOURCES.read_text())
    out = run.dir / tag
    out.mkdir(exist_ok=True)
    env = {"GAP811_OUT": f"/out/{tag}/records.jsonl", "RUST_BACKTRACE": "1", **env}
    command = [
        "docker", "run", "--rm", "--name", f"{PREFIX}t-{tag}", "--network", NET,
        "--mount", f"type=bind,src={record['worktree']},dst=/work,readonly",
        "--mount", f"type=volume,src={record['linux_target_volume']},dst=/work/target,volume-nocopy",
        "--mount", "type=volume,dst=/tmp",
        "--mount", f"type=bind,src={run.dir},dst=/out",
        *[v for k, value in env.items() for v in ("-e", f"{k}={value}")],
        "-w", "/work/engine/crates/layerfs-store-service", "--entrypoint", test_binary(run),
        record["images"]["build"], "--exact", name, "--nocapture", "--test-threads=1",
    ]  # fmt: skip
    run.note(f"test {name} [{tag}] env {sorted(env)}")
    if background:
        stream = (out / "output.log").open("w")
        return subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT)
    started = time.time()
    result = subprocess.run(command, check=False, capture_output=True, text=True, timeout=timeout)
    (out / "output.log").write_text(result.stdout + result.stderr)
    run.note(f"test {name} [{tag}] exit {result.returncode} in {time.time() - started:.1f}s; {out}/output.log")
    return result


def records(run, tag):
    path = run.dir / tag / "records.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def store_env(run, host="pg1"):
    setup = run.state["setup"]
    return {
        "LAYERFS_TEST_POSTGRES_URL": url(PREFIX + host),
        "GAP811_SCHEMA": setup["schema"],
        "GAP811_BINDING": setup["binding"],
        "GAP811_STACK": setup["stack"],
        "GAP811_LAYER": setup["layer"],
    }


def setup_store(run, tag):
    result = test(run, "failover", {"GAP811_PHASE": "setup", "LAYERFS_TEST_POSTGRES_URL": url(PREFIX + "pg1")}, tag)
    if result.returncode != 0:
        sys.exit("setup failed")
    run.state["setup"] = records(run, tag)[-1]
    run.save()


def cmd_up_repl(run, args):
    network(run)
    primary(run, "pg1")
    standby(run, "pg2", "pg1", "s1")
    caught_up(run)
    if args.sync_names:
        # Set after the standby is attached: the image's init would wait for it.
        run.psql("pg1", f"ALTER SYSTEM SET synchronous_standby_names = '{args.sync_names}'")
        run.psql("pg1", "SELECT pg_reload_conf()")
        time.sleep(1)
    run.psql("pg1", "SELECT application_name, state, sync_state FROM pg_stat_replication")
    run.state["repl"] = {"sync_names": args.sync_names}
    run.save()


def cmd_failover(run, args):
    tag = f"failover-{args.sync}-{args.mode}"
    setup_store(run, tag + "-setup")
    caught_up(run)
    go = run.dir / tag / "go"
    (run.dir / tag).mkdir(exist_ok=True)
    env = {**store_env(run), "GAP811_PHASE": "commit", "GAP811_SYNC": args.sync, "GAP811_GO": f"/out/{tag}/go"}
    env["GAP811_BLOCK_S"] = str(args.block_s)
    process = test(run, "failover", env, tag, background=True)
    until = time.time() + 60
    while time.time() < until and not any(r.get("phase") == "ready" for r in records(run, tag)):
        if process.poll() is not None:
            sys.exit("commit phase ended early")
        time.sleep(0.2)
    caught_up(run)
    run.psql("pg1", "SELECT application_name, state, sync_state, replay_lsn FROM pg_stat_replication")
    if args.mode == "partition":
        run.sh("docker", "network", "disconnect", NET, PREFIX + "pg2")
    go.write_text("go")
    process.wait(timeout=args.block_s + 30)
    run.note(f"commit phase exit {process.returncode}")
    commit = [r for r in records(run, tag) if r.get("phase") == "commit"][-1]
    run.psql("pg1", "SELECT pg_current_wal_lsn(), application_name, state FROM pg_stat_replication", check=False)
    run.sh("docker", "kill", PREFIX + "pg1")
    if args.mode == "partition":
        run.sh("docker", "network", "connect", NET, PREFIX + "pg2")
    run.psql("pg2", "SELECT pg_promote(true, 30)")
    schema = run.state["setup"]["schema"]
    head_sql = f"SELECT coalesce(encode(head_commit_id, 'hex'), 'none') FROM {schema}.branch WHERE name = 'b1'"
    promoted = run.psql("pg2", head_sql).stdout.strip()
    answered = commit["outcome"].startswith("Committed(")
    acked = commit["outcome"][len("Committed(") : -1] if answered else None
    rows = acked and run.psql(
        "pg2", f"SELECT count(*) FROM {schema}.\"commit\" WHERE commit_id = decode('{acked}', 'hex')"
    )
    commits = run.psql("pg2", f'SELECT count(*) FROM {schema}."commit"').stdout.strip()
    run.sh("docker", "start", PREFIX + "pg1")
    wait_ready(run, "pg1")
    old = run.psql("pg1", head_sql).stdout.strip()
    verdict = {
        "sync": args.sync,
        "mode": args.mode,
        "commit": commit,
        "acked_commit": acked,
        "head_on_promoted_standby": promoted,
        "acked_commit_rows_on_promoted_standby": int(rows.stdout.strip()) if acked else None,
        "commit_rows_on_promoted_standby": int(commits),
        "head_on_restarted_old_primary": old,
        "acknowledged_commit_lost": bool(acked) and promoted != acked,
    }
    (run.dir / tag / "verdict.json").write_text(json.dumps(verdict, indent=2))
    run.note("VERDICT " + json.dumps(verdict))


def cmd_setup(run, args):
    setup_store(run, "setup")
    caught_up(run)


def cmd_up_split(run, args):
    setup_store(run, "split-setup")
    caught_up(run)
    run.psql("pg2", "SELECT pg_promote(true, 30)")
    run.note("pg1 and pg2 are both writable and both hold the Store (split brain)")


def cmd_multihost(run, args):
    env = {**store_env(run), "GAP811_ARM": args.arm, "GAP811_MH_URL": args.url}
    env["GAP811_ADMIN_A"] = url(PREFIX + args.a)
    env["GAP811_ADMIN_B"] = url(PREFIX + args.b)
    if args.gate:
        env["GAP811_GATE"] = f"{PREFIX}{args.a}:5432"
    if args.xid_align:
        env["GAP811_XID_ALIGN"] = "1"
    result = test(run, "multihost", env, f"multihost-{args.arm}")
    sys.exit(result.returncode)


def cmd_up_pgbouncer(run, args):
    network(run)
    if run.sh("docker", "inspect", PREFIX + "pg1", check=False, quiet=True).returncode != 0:
        primary(run, "pg1")
    name = PREFIX + "pgb-" + args.pool_mode
    run.sh("docker", "rm", "-f", name, check=False)
    run.sh(
        "docker", "run", "-d", "--name", name, "--network", NET,
        "-e", f"DB_HOST={PREFIX}pg1", "-e", f"DB_USER={USER}", "-e", f"DB_PASSWORD={PASSWORD}", "-e", f"DB_NAME={DB}",
        "-e", f"POOL_MODE={args.pool_mode}", "-e", "AUTH_TYPE=scram-sha-256",
        "-e", f"DEFAULT_POOL_SIZE={args.pool_size}", "-e", "MAX_CLIENT_CONN=500",
        "-e", f"MAX_PREPARED_STATEMENTS={args.max_prepared}", "-e", "IGNORE_STARTUP_PARAMETERS=extra_float_digits",
        PGB_IMAGE,
    )  # fmt: skip
    time.sleep(2)
    run.sh("docker", "logs", name, check=False)
    run.sh("docker", "exec", name, "pgbouncer", "--version", check=False)


def cmd_pooled(run, args):
    env = {
        "LAYERFS_TEST_POSTGRES_URL": url(PREFIX + "pg1"),
        "GAP811_POOLED_URL": url(PREFIX + "pgb-" + args.pool_mode),
        "GAP811_ARM": args.arm,
    }
    if args.pin:
        env["GAP811_PIN"] = "1"
    if args.steal:
        env["GAP811_STEAL"] = "1"
    result = test(run, "pooled", env, f"pooled-{args.arm}")
    sys.exit(result.returncode)


def cmd_lane(run, args):
    network(run)
    if run.sh("docker", "inspect", PREFIX + "pg1", check=False, quiet=True).returncode != 0:
        primary(run, "pg1")
    env = {
        "LAYERFS_TEST_POSTGRES_URL": url(PREFIX + "pg1"),
        "GAP811_WRITERS": str(args.writers),
        "GAP811_SECONDS": str(args.seconds),
        "GAP811_LOCK_TIMEOUT_MS": str(args.lock_ms),
        "GAP811_ARM": args.arm,
    }
    if args.shared:
        env["GAP811_SHARED"] = "1"
    result = test(run, "lane_contention", env, f"lane-{args.arm}")
    sys.exit(result.returncode)


def cmd_lane_multi(run, args):
    """One writer process per container, all on one Store: the Store and its
    Branches come from phase `setup`; container 0 samples `pg_locks`."""
    network(run)
    if run.sh("docker", "inspect", PREFIX + "pg1", check=False, quiet=True).returncode != 0:
        primary(run, "pg1")
    branches = 1 if args.shared else args.containers
    result = test(
        run,
        "failover",
        {"GAP811_PHASE": "setup", "GAP811_BRANCHES": str(branches), "LAYERFS_TEST_POSTGRES_URL": url(PREFIX + "pg1")},
        f"lane-multi-{args.arm}-setup",
    )
    if result.returncode != 0:
        sys.exit("setup failed")
    run.state["setup"] = records(run, f"lane-multi-{args.arm}-setup")[-1]
    run.save()
    processes = []
    for index in range(args.containers):
        env = {
            **store_env(run),
            "GAP811_WRITERS": "1",
            "GAP811_WRITER_BASE": str(index),
            "GAP811_SECONDS": str(args.seconds),
            "GAP811_LOCK_TIMEOUT_MS": str(args.lock_ms),
            "GAP811_ARM": f"{args.arm}-c{index}",
            "GAP811_SAMPLE": "1" if index == 0 else "0",
        }
        if args.shared:
            env["GAP811_SHARED"] = "1"
        processes.append(test(run, "lane_contention", env, f"lane-multi-{args.arm}-c{index}", background=True))
    deadline = time.time() + args.seconds + 40
    codes = []
    for process in processes:
        try:
            codes.append(process.wait(timeout=max(1, deadline - time.time())))
        except subprocess.TimeoutExpired:
            process.kill()
            codes.append("timeout")
    run.note(f"containers exit {codes}")
    rows = [r for i in range(args.containers) for r in records(run, f"lane-multi-{args.arm}-c{i}")]
    summary = {
        "arm": args.arm,
        "containers": args.containers,
        "exit": codes,
        "per_container_committed": [r["per_writer_committed"][0] for r in rows],
        "classes": [r["classes"] for r in rows],
        "commit_p50_p99_max": [(r["commit_ms"]["p50"], r["commit_ms"]["p99"], r["commit_ms"]["max"]) for r in rows],
        "busy_min_ms": [r["busy_min_ms"] for r in rows],
        "server_lock_waits_ms": rows[0]["server_lock_waits_ms"] if rows else None,
    }
    (run.dir / f"lane-multi-{args.arm}-summary.json").write_text(json.dumps(summary, indent=2))
    run.note("SUMMARY " + json.dumps(summary))
    sys.exit(0 if all(code == 0 for code in codes) else 1)


def cmd_lost_release(run, args):
    network(run)
    if run.sh("docker", "inspect", PREFIX + "pg1", check=False, quiet=True).returncode != 0:
        primary(run, "pg1")
    result = test(run, "lost_unmount_reply", {"LAYERFS_TEST_POSTGRES_URL": url(PREFIX + "pg1")}, "lost-release")
    sys.exit(result.returncode)


def cmd_down(run, args):
    names = run.sh("docker", "ps", "-a", "--format", "{{.Names}}", "--filter", f"name=^{PREFIX}").stdout.split()
    for name in names:
        run.sh("docker", "rm", "-f", "-v", name, check=False)
    if run.sh("docker", "network", "inspect", NET, check=False, quiet=True).returncode == 0:
        run.sh("docker", "network", "rm", NET, check=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("up-repl")
    p.add_argument("--sync-names")
    p = sub.add_parser("failover")
    p.add_argument("--sync", required=True)
    p.add_argument("--mode", choices=["partition", "ack"], required=True)
    p.add_argument("--block-s", type=int, default=20)
    sub.add_parser("setup")
    sub.add_parser("up-split")
    p = sub.add_parser("multihost")
    p.add_argument("--arm", required=True)
    p.add_argument("--url", required=True)
    p.add_argument("--a", default="pg1")
    p.add_argument("--b", default="pg2")
    p.add_argument("--gate", action="store_true")
    p.add_argument("--xid-align", action="store_true")
    p = sub.add_parser("up-pgbouncer")
    p.add_argument("--pool-mode", choices=["transaction", "session"], required=True)
    p.add_argument("--pool-size", type=int, default=20)
    p.add_argument("--max-prepared", type=int, default=200)
    p = sub.add_parser("pooled")
    p.add_argument("--arm", required=True)
    p.add_argument("--pool-mode", choices=["transaction", "session"], required=True)
    p.add_argument("--pin", action="store_true")
    p.add_argument("--steal", action="store_true")
    p = sub.add_parser("lane")
    p.add_argument("--arm", required=True)
    p.add_argument("--writers", type=int, default=8)
    p.add_argument("--seconds", type=int, default=60)
    p.add_argument("--lock-ms", type=int, default=30000)
    p.add_argument("--shared", action="store_true")
    p = sub.add_parser("lane-multi")
    p.add_argument("--arm", required=True)
    p.add_argument("--containers", type=int, default=8)
    p.add_argument("--seconds", type=int, default=45)
    p.add_argument("--lock-ms", type=int, default=30000)
    p.add_argument("--shared", action="store_true")
    sub.add_parser("lost-release")
    sub.add_parser("down")
    for name, sp in sub.choices.items():
        if name != "down":
            sp.add_argument("run")
    args = parser.parse_args()
    run = Run(getattr(args, "run", None) or Path.home() / ".cache/layerfs-811/v3/driver")
    globals()["cmd_" + args.command.replace("-", "_")](run, args)


if __name__ == "__main__":
    main()

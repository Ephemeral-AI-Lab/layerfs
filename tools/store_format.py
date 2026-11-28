#!/usr/bin/env python3
"""Capture and guard the frozen Store-format fixtures of `layerfs store upgrade` (#804).

A fixture is a Store written by a real old build, never one faked from a
current Store. `capture` runs one deterministic scenario with an old `layerfs`
command in a fresh scratch home under the temporary folder, saves the Store
(SQLite: the closed `store.sqlite`; PostgreSQL: a plain `pg_dump` of the
Store's schema) with a `manifest.json` of what the old build answered, and
refreshes `fixtures/store-formats/SHA256SUMS`. With `--create-cli` an older
build creates the (native) Store and writes the first half of the history; the
`--cli` build then opens it, which runs its open-time migrations, and writes the
rest. `verify-hashes` checks that
list; a fixture is never edited once committed. `new-step` freezes the current
schema text before a format change and captures its fixture with the parent
build.

Usage:
  store_format.py capture --engine sqlite --cli OLD/bin/layerfs --out fixtures/store-formats/sqlite/v2
  store_format.py capture --engine postgresql --cli OLD/bin/layerfs --out fixtures/store-formats/postgresql/v3
  store_format.py capture --engine sqlite --create-cli OLDER/layerfs --cli OLD/bin/layerfs \
      --out fixtures/store-formats/sqlite/v2/d-appended
  store_format.py verify-hashes
  store_format.py new-step --engine sqlite --version 3 --cli PARENT/bin/layerfs
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures" / "store-formats"
SUMS = FIXTURES / "SHA256SUMS"
SEED = 804
LARGE_SIZE = 1536 * 1024
SCRATCH_SIZE = 640 * 1024
CALL_TIMEOUT = 110
PG_CONTAINER = "lfs-804-cap-pg"
SQLITE_SQL = REPO / "engine/crates/layerfs-persistence/sql/sqlite"
PG_SQL = REPO / "engine/crates/layerfs-persistence/sql/postgresql"
# The SQLite scripts in the order schema.rs `scripts` composes them for the
# product selection (group rows indexed, acquisition tables).
SQLITE_SCRIPTS = [
    "objects_units.sql",
    "objects_units_index.sql",
    "metadata.sql",
    "history.sql",
    "lineage.sql",
    "merge_scratch.sql",
    "file_digest.sql",
    "gc.sql",
    "acquisition/schema.sql",
]
PG_SCRIPTS = ["schema.sql", "gc.sql"]


def log(msg: str) -> None:
    print(f"[store_format] {msg}", file=sys.stderr, flush=True)


class Cli:
    """One old `layerfs` command against one scratch home; one attempt per call."""

    def __init__(self, path: Path, home: Path, bundle: Path | None, flags: tuple = ("--detail", "--json")) -> None:
        self.path = path
        self.flags = flags
        self.env = dict(os.environ, LAYERFS_ROOT=str(home))
        if bundle is not None:
            self.env["LAYERFS_BUNDLE"] = str(bundle)

    def __call__(self, *args: str, stdin: bytes | None = None, ok: bool = True) -> dict:
        cmd = [str(self.path), *args, *self.flags]
        p = subprocess.run(cmd, input=stdin, capture_output=True, env=self.env, timeout=CALL_TIMEOUT, check=False)
        if ok and p.returncode != 0:
            raise SystemExit(f"layerfs {' '.join(args)} failed ({p.returncode}): {p.stderr.decode(errors='replace')}")
        out = p.stdout.decode().strip()
        answer = json.loads(out) if out else {}
        answer["_exit"] = p.returncode
        return answer

    def pages(self, *args: str) -> list[dict]:
        items: list[dict] = []
        cursor: list[str] = []
        while True:
            a = self(*args, "--limit", "1000", *cursor)
            items += a.get("items") or []
            if not a.get("next_cursor"):
                return items
            cursor = ["--cursor", a["next_cursor"]]


# ---------------------------------------------------------------- content


def stream(label: str, size: int, seed: int = SEED) -> bytes:
    """Deterministic non-repeating bytes: SHA-256 in counter mode."""
    out = bytearray()
    n = 0
    while len(out) < size:
        out += hashlib.sha256(f"{seed}:{label}:{n}".encode()).digest()
        n += 1
    return bytes(out[:size])


def config_text(version: int, seed: int = SEED, lines: int = 240) -> bytes:
    """A text file whose versions differ in a few lines, so they share delta chains."""
    rnd = random.Random(seed)
    rows = [f'  "key_{i:03d}": "{rnd.getrandbits(64):016x}",' for i in range(lines)]
    edit = random.Random(seed * 1000 + version)
    for _ in range(version * 3):
        i = edit.randrange(lines)
        rows[i] = f'  "key_{i:03d}": "v{version}-{edit.getrandbits(48):012x}",'
    return ("{\n" + "\n".join(rows) + '\n  "end": true\n}\n').encode()


def source_text(name: str, version: int, seed: int = SEED) -> bytes:
    rnd = random.Random(f"{seed}:{name}")
    body = [f"def f{i}(x):\n    return x * {rnd.randrange(1000)} + {i}\n" for i in range(60)]
    body[version % 60] = f"def f{version % 60}(x):\n    return x - {version}  # version {version}\n"
    return (f"# {name} version {version}\n" + "\n".join(body)).encode()


def write(path: Path, data: bytes, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    os.chmod(path, mode)


def make_web(root: Path, seed: int) -> None:
    write(root / "README.md", b"# web\n\nA LayerFS Store-format fixture project (#804).\n")
    write(root / "src/app.py", source_text("app", 1, seed))
    write(root / "src/util.py", source_text("util", 1, seed))
    os.link(root / "src/app.py", root / "src/app-hardlink.py")
    write(root / "bin/run.sh", b"#!/bin/sh\necho run\n", 0o755)
    write(root / "secret.key", stream("secret", 64, seed).hex().encode() + b"\n", 0o600)
    write(root / "data/large.bin", stream("large", LARGE_SIZE, seed))
    write(root / "notes/config.json", config_text(1, seed))
    write(root / "docs/ünïcødé-名前.txt", "Unicode name: ünïcødé 名前 ✓\n".encode())
    write(root / "données/été.md", b"accents\n")
    (root / "empty").mkdir()
    os.symlink("README.md", root / "link-to-readme")
    os.symlink("../src", root / "docs/src-dir-link")
    os.symlink("missing/target", root / "dangling-link")
    write(root / ".gitignore", b"*.log\nbuild/\n")
    write(root / "debug.log", b"ignored log line\n")
    write(root / "build/out.o", stream("build", 512, seed))
    git = root / ".git"
    write(git / "HEAD", b"ref: refs/heads/main\n")
    write(git / "config", b"[core]\n\trepositoryformatversion = 0\n\tbare = false\n")
    write(git / "refs/heads/main", b"0123456789abcdef0123456789abcdef01234567\n")
    write(git / "objects/01/23456789abcdef0123456789abcdef01234567", stream("gitobj", 300, seed))
    (git / "objects/pack").mkdir(parents=True)


def make_lib(root: Path, seed: int) -> None:
    write(root / "lib.c", source_text("lib", 1, seed))
    write(root / "include/lib.h", b"int lib(int);\n")
    write(root / "tests/test_lib.py", source_text("test_lib", 1, seed))
    write(root / "tools/gen.sh", b"#!/bin/sh\necho gen\n", 0o755)


# ---------------------------------------------------------------- scenario


def sh_quote(s: str) -> str:
    return "'" + s.replace("'", "'\\''") + "'"


def commit_id(answer: dict) -> str:
    """A Commit's id from either build: `commit` (a dict in the older one) or an init's `layer`."""
    c = answer.get("commit") or answer["layer"]
    return c["id"] if isinstance(c, dict) else c


def scenario(cli: Cli, store: str, machine: str, work: Path, seed: int, first: Cli | None = None) -> dict:
    """Run the history; returns notable Commits by name. `first`, an older build, writes up to the merge."""
    ev: dict = {}
    web, lib = f"{store}/web", f"{store}/lib"
    make_web(work / "web", seed)
    make_lib(work / "lib", seed)
    a = first or cli
    ev["web_init"] = commit_id(a("project", "init", web, str(work / "web")))
    ev["lib_init"] = commit_id(a("project", "init", lib, str(work / "lib")))

    def fw(project: str, branch: str, path: str, data: bytes, msg: str, mode: str | None = None, by: Cli = cli) -> str:
        extra = ["--mode", mode] if mode else []
        return commit_id(by("file", "write", project, branch, path, *extra, "-m", msg, stdin=data))

    def fork(project: str, name: str) -> None:
        a("branch", "fork", project, name, *(["main"] if first else []))

    # web: feature with small-file versions, a move, a mkdir and deletes
    fork(web, "feature")
    for v in (2, 3, 4):
        fw(web, "feature", "/notes/config.json", config_text(v, seed), f"config v{v}", by=a)
    fw(web, "feature", "/src/app.py", source_text("app", 2, seed), "app v2", by=a)
    a("file", "mv", web, "feature", "/src/util.py", "/src/helpers.py", "-m", "rename util")
    a("file", "mkdir", web, "feature", "/feature-empty", "-m", "empty folder")
    a("file", "rm", web, "feature", "/debug.log", "-m", "drop ignored log")
    recursive = [] if first else ["--recursive"]  # the older build removes a folder without the flag
    ev["feature_rm_dir"] = commit_id(a("file", "rm", web, "feature", "/build", *recursive, "-m", "drop build"))
    # web: other diverges, then merges into feature (two parents)
    fork(web, "other")
    fw(web, "other", "/other.txt", b"from the other Branch\n", "other file", by=a)
    fw(web, "other", "/notes/other-config.json", config_text(5, seed), "other config", by=a)
    # from here on `cli` writes; its first call opens the older build's Store and migrates it
    merge = cli("branch", "merge", web, "feature", "other", "-m", "merge other into feature")
    ev["merge"] = merge["commit"]
    ev["merge_second_parent"] = merge.get("second_parent")
    if first:
        cli("store", "move", store, machine)  # its Sandbox needs the Store on the machine
    ev["publish_feature"] = cli("branch", "publish", web, "feature")["commit"]
    # web: a forced publish displaces main's head
    init = ev["web_init"]
    cli("branch", "fork", web, "hotfix", "--at", init)
    fw(web, "hotfix", "/HOTFIX", b"hotfix\n", "hotfix")
    forced = cli("branch", "publish", web, "hotfix", "--force")
    ev["publish_forced"] = forced["commit"]
    ev["publish_forced_previous"] = forced.get("previous")
    # web: a removed Branch
    cli("branch", "fork", web, "scratch")
    # more than a tenth of the packs, so the compaction below rebuilds
    fw(web, "scratch", "/scratch.bin", stream("scratch", SCRATCH_SIZE, seed), "scratch data")
    cli("branch", "remove", web, "scratch")
    # compaction before the Workspace Commits: a rebuild empties binding_hint_block
    compact = cli("store", "compact", store)
    ev["compact"] = {k: compact.get(k) for k in ("outcome", "freed_bytes", "generation_before", "generation_after")}
    if compact.get("generation_after") == compact.get("generation_before"):
        raise SystemExit(f"store compact rebuilt nothing: {compact}")
    # web: Workspace Commits through a mounted Workspace in a Sandbox
    cli("branch", "fork", web, "ws")
    cli("sandbox", "create", web, "box", "--machine", machine, "--mount", "ws")
    steps = [
        "printf 'workspace edit\\n' >> README.md",
        "ln src/app.py src/app-link2.py",
        "ln -s ../README.md docs/readme-link",
        "mkdir -p ws-empty && chmod 0700 ws-empty",
        "printf '#!/bin/sh\\necho tool\\n' > bin/tool.sh && chmod 0755 bin/tool.sh",
        "printf 'private\\n' > private.txt && chmod 0600 private.txt",
        "rm -f secret.key",
        "printf 'ünïcødé from a Workspace\\n' > 'docs/ワークスペース.txt'",
        "printf 'new.log\\n' > new.log",
        "printf '[remote]\\n' >> .git/config",
    ]

    def ex(cmd: str) -> None:
        a = cli("workspace", "exec", "box:ws", cmd)
        if a.get("exit_code") != 0:
            raise SystemExit(f"workspace exec {cmd!r} answered {a}")

    ex(" && ".join(steps))
    ev["ws_commit_1"] = cli("workspace", "commit", "box:ws", "-m", "workspace edits")["commit"]
    for v in (6, 7):
        ex(f"printf %s {sh_quote(config_text(v, seed).decode())} > notes/config.json")
        ev[f"ws_commit_config_v{v}"] = cli("workspace", "commit", "box:ws", "-m", f"workspace config v{v}")["commit"]
    # a forced Workspace Commit over a head that moved since the mount
    ev["ws_moved_head"] = fw(web, "ws", "/moved.txt", b"written beside the Workspace\n", "moved head")
    ex("printf 'forced\\n' > forced.txt && rm -f notes/other-config.json")
    forced_ws = cli("workspace", "commit", "box:ws", "-m", "forced Workspace Commit", "--force")
    ev["ws_forced"] = forced_ws["commit"]
    ev["ws_forced_displaced"] = forced_ws.get("displaced")
    cli("workspace", "unmount", "box:ws")
    cli("sandbox", "remove", "box")
    # a Branch removed after the compaction keeps its name taken in the fixture
    cli("branch", "fork", web, "dropped", "feature")
    fw(web, "dropped", "/dropped.txt", b"on a removed Branch\n", "dropped data")
    cli("branch", "remove", web, "dropped")
    # lib: a second project with its own Branches and versions
    cli("branch", "fork", lib, "dev")
    for v in (2, 3, 4, 5):
        fw(lib, "dev", "/lib.c", source_text("lib", v, seed), f"lib v{v}")
    fw(lib, "dev", "/tools/gen.sh", b"#!/bin/sh\necho gen v2\n", "gen v2", mode="0755")
    cli("file", "rm", lib, "dev", "/tests/test_lib.py", "-m", "drop test")
    ev["lib_publish"] = cli("branch", "publish", lib, "dev")["commit"]
    cli("branch", "fork", lib, "old", "--at", ev["lib_init"])
    fw(lib, "old", "/include/lib.h", b"int lib(int);\nint old(void);\n", "old api")
    return ev


# ---------------------------------------------------------------- manifest


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def tree(cli: Cli, project: str, branch: str, out: Path) -> dict:
    """Every path at a Branch head: content and links from `project export`, mode and inode from `file stat`."""
    out.parent.mkdir(parents=True, exist_ok=True)
    cli("project", "export", project, str(out), "--branch", branch)
    files: dict = {}
    for dirpath, dirnames, filenames in os.walk(out):
        for name in sorted(dirnames + filenames):
            p = Path(dirpath) / name
            rel = "/" + p.relative_to(out).as_posix()
            st = cli("file", "stat", project, branch, rel)
            entry: dict = {"kind": st["type"], "mode": st["mode"], "inode": st["inode"]}
            if p.is_symlink():
                entry["target"] = os.readlink(p)
            elif p.is_file():
                entry |= {"sha256": sha256_file(p), "size": p.stat().st_size}
                if st["type"] == "file" and st["size"] != entry["size"]:
                    raise SystemExit(f"{project} {branch} {rel}: stat size {st['size']} != export {entry['size']}")
            files[rel] = entry
    # hard links: paths whose stat shares one inode
    by_inode: dict = {}
    for rel, e in files.items():
        if e["kind"] == "file":
            by_inode.setdefault(e["inode"], []).append(rel)
    for group in by_inode.values():
        if len(group) > 1:
            for rel in group:
                files[rel]["hard_links"] = sorted(group)
    for e in files.values():
        e.pop("inode")
    return dict(sorted(files.items()))


def describe(cli: Cli, store: str, work: Path) -> dict:
    projects: dict = {}
    for name in ("lib", "web"):
        project = f"{store}/{name}"
        branches: dict = {}
        commits: set = set()
        for b in cli.pages("branch", "list", project):
            ids = [c["commit"] for c in cli.pages("commit", "list", project, b["branch"])][::-1]
            commits |= set(ids)
            files = tree(cli, project, b["branch"], work / "export" / name / b["branch"])
            branches[b["branch"]] = {"head": b["commit"], "commits": ids, "files": files}
        projects[name] = {"branches": branches, "commit_count": len(commits)}
    return projects


# ---------------------------------------------------------------- engines


def docker(*args: str, stdin: bytes | None = None, check: bool = True) -> subprocess.CompletedProcess:
    p = subprocess.run(["docker", *args], input=stdin, capture_output=True, timeout=CALL_TIMEOUT, check=False)
    if check and p.returncode != 0:
        raise SystemExit(f"docker {args[0]} failed: {p.stderr.decode(errors='replace')}")
    return p


def psql(sql: str, db: str = "postgres") -> str:
    p = docker(
        "exec", "-i", PG_CONTAINER, "psql", "-U", "postgres", "-d", db, "-At", "-v", "ON_ERROR_STOP=1", "-c", sql
    )
    return p.stdout.decode().strip()


def store_schema() -> str:
    rows = psql("SELECT table_schema FROM information_schema.tables WHERE table_name='history_meta'").split()
    if len(rows) != 1:
        raise SystemExit(f"expected one Store schema, found {rows}")
    return rows[0]


def old_build(cli_path: Path) -> dict:
    runtime = cli_path.resolve().parent.parent
    found = sorted(runtime.glob("bundle/*/bundle.json"))
    info = json.loads(found[0].read_text()) if found else {}
    return {
        "runtime": runtime.name,
        "commit": (info.get("source") or {}).get("head"),
        "version": info.get("version"),
        "bundle": found[0].parent.name if found else None,
    }


def created_by(cli_path: Path) -> dict:
    """The older build of a two-build capture: a cargo build inside its source checkout."""
    p = cli_path.expanduser().resolve()
    head = subprocess.run(["git", "-C", str(p.parent), "rev-parse", "HEAD"], capture_output=True, check=False)
    version = subprocess.run([str(p), "--version"], capture_output=True, check=False).stdout.decode().split()
    return {"commit": head.stdout.decode().strip() or None, "version": version[-1] if version else None}


def release(cli: Cli, home: Path) -> list[str]:
    """Stop the scratch home's machine and remove the Docker resources named with its id, and only those."""
    prefix = "layerfs-" + json.loads((home / "layerfs.json").read_text())["id"][:8] + "-"
    cli("machine", "stop", "docker", ok=False)
    removed = []
    for kind, fmt in (("container", "{{.Names}}"), ("volume", "{{.Name}}")):
        listing = ["ps", "-a"] if kind == "container" else ["volume", "ls"]
        names = docker(*listing, "--format", fmt).stdout.decode().split()
        for name in (n for n in names if n.startswith(prefix)):
            docker(*(["rm", "-f"] if kind == "container" else ["volume", "rm"]), name)
            removed.append(f"{kind} {name}")
    log(f"removed {removed}")
    return removed


def capture_host_machine(cli: Cli) -> str:
    rows = cli("machine", "list")["items"]
    hosts = [
        row["machine"]
        for row in rows
        if (row.get("platform"), row.get("machine"))
        in {("native", "native"), ("host", "macos-host"), ("host", "linux-host")}
    ]
    if len(hosts) != 1:
        raise SystemExit("the selected build must list exactly one local host machine for SQLite capture")
    return hosts[0]


def capture(args: argparse.Namespace) -> None:
    cli_path = Path(args.cli).expanduser().resolve()
    out = Path(args.out).resolve()
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f"{out} is not empty; a fixture is frozen once written")
    scratch = Path(tempfile.mkdtemp(prefix="lfs-store-format-"))
    home = scratch / "home"
    bundle = (
        Path(args.bundle) if args.bundle else next(iter(sorted(cli_path.parent.parent.glob("bundle/linux-*"))), None)
    )
    cli = Cli(cli_path, home, bundle)
    log(f"scratch home {home}")
    cli("home", "init")
    machine = args.machine
    cli("machine", "start", machine)
    store = "fx"
    notes = [
        (
            "the old build's `project init` imports hard-linked host files as separate files (distinct inodes); "
            "the fixture's hard links come from `ln` in a mounted Workspace (web ws)"
        ),
        (
            "scenario.events names notable Commits: the merge, the forced publish and the forced Workspace Commit "
            "with the heads they displaced"
        ),
    ]
    first = None
    if args.create_cli:
        # the older build writes into the home the newer one made: it names machines as the newer one reads
        first = Cli(Path(args.create_cli).expanduser().resolve(), home, None, flags=())
        if args.engine != "sqlite":
            raise SystemExit("--create-cli captures a sqlite Store only")
        first("store", "create", store)
        notes += [
            (
                "the Store was created and written up to the merge by the older build in created_by, its only "
                "Store kind a sqlite Store on native; old_build opened it (its open-time migrations append the "
                "columns the older source lacks), moved it to the machine and wrote the rest"
            ),
        ]
    elif args.engine == "sqlite":
        cli("store", "create", store, "--machine", machine)
    else:
        if not args.pg_url:
            raise SystemExit("--pg-url is needed for postgresql")
        cli("store", "connect", store, args.pg_url, "--user", "postgres", "--password-stdin", stdin=b"trust\n")
        cli("store", "attach", store, "--machine", machine)
    t0 = time.time()
    events = scenario(cli, store, machine, scratch / "work", args.seed, first)
    log(f"scenario done in {time.time() - t0:.0f}s")
    projects = describe(cli, store, scratch)
    out.mkdir(parents=True, exist_ok=True)
    manifest: dict = {"engine": args.engine}
    if args.engine == "sqlite":
        cli("store", "move", store, capture_host_machine(cli))
        db = home / "store" / store / "store.sqlite"
        leftovers = [p.name for p in db.parent.iterdir() if p.name.startswith("store.sqlite-")]
        if leftovers:
            raise SystemExit(f"the moved Store is not closed: {leftovers}")
        shutil.copyfile(db, out / "store.sqlite")
        con = sqlite3.connect(f"file:{out / 'store.sqlite'}?immutable=1", uri=True)
        version = con.execute("PRAGMA user_version").fetchone()[0]
        journal = con.execute("PRAGMA journal_mode").fetchone()[0]
        source, definition = con.execute("SELECT schema_source, schema_definition FROM history_meta").fetchone()
        con.close()
        hashes = {k: hashlib.sha256(v.encode()).hexdigest() for k, v in (("source", source), ("definition", definition))}
        log(f"schema_source sha256 {hashes['source']}, schema_definition sha256 {hashes['definition']}")
        if first:
            notes.append(
                f"history_meta: sha256(schema_source) {hashes['source']}, sha256(schema_definition) "
                f"{hashes['definition']}"
            )
        manifest |= {"format": "sqlite-schema-2", "user_version": version, "journal_mode": journal}
        notes.append(
            f"store.sqlite is the file the old build's `store move {store} native` left: closed, no -wal or -shm, "
            f"journal_mode {journal} in its header"
        )
        if not (36 <= version <= 38 or 42 <= version <= 44):
            raise SystemExit(f"user_version {version} is not a schema-2 Store")
    else:
        schema = store_schema()
        version = int(psql(f"SELECT schema_version FROM {schema}.history_meta"))
        dump = docker(
            "exec", PG_CONTAINER, "pg_dump", "-U", "postgres", "-d", "postgres",
            f"--schema={schema}", "--no-owner", "--no-privileges",
        ).stdout  # fmt: skip
        (out / "store.sql").write_bytes(dump)
        psql("DROP DATABASE IF EXISTS restore_check")
        psql("CREATE DATABASE restore_check")
        docker("exec", "-i", PG_CONTAINER, "psql", "-U", "postgres", "-d", "restore_check", "-q",
               "-v", "ON_ERROR_STOP=1", stdin=dump)  # fmt: skip
        restored = int(psql(f"SELECT schema_version FROM {schema}.history_meta", db="restore_check"))
        psql("DROP DATABASE restore_check")
        if restored != version:
            raise SystemExit("the dump did not restore")
        manifest |= {"format": f"postgresql-schema-{version}", "schema_version": version, "schema": schema}
        manifest["restore"] = {
            "command": "psql -v ON_ERROR_STOP=1 -d <fresh database> -f store.sql",
            "roles": "none: dumped with --no-owner --no-privileges; any role that may create a schema restores it",
            "extensions": "none beyond plpgsql, which every database has",
            "psql": "a psql that knows \\restrict (17.6 or later in the 17 series), as pg_dump 17.6+ writes it",
            "search_path": f"the Store lives in schema {schema}; open it with that schema as the product does",
            "server": psql("SHOW server_version"),
        }
        if version != 3:
            raise SystemExit(f"schema_version {version} is not PostgreSQL schema 3")
    manifest |= {
        "old_build": old_build(cli_path),
        **({"created_by": created_by(Path(args.create_cli))} if args.create_cli else {}),
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "machine": machine,
        "scenario": {"seed": args.seed, "large_file_bytes": LARGE_SIZE, "store": store, "events": events},
        "projects": projects,
        "removed_branches": {
            "web": {"scratch": "removed before the compaction, which freed it", "dropped": "removed after it"}
        },
        "notes": notes
        + [
            (
                "files: content, size and symlink targets from the old build's `project export` of each Branch head; "
                "kind and mode from its `file stat`; hard_links lists paths whose `file stat` shares one inode"
            ),
        ],
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    write_sums()
    if not args.keep:
        release(cli, home)
        shutil.rmtree(scratch, ignore_errors=True)
    log(f"fixture written to {out}")


# ---------------------------------------------------------------- hashes


def fixture_files() -> list[Path]:
    return sorted(p for p in FIXTURES.rglob("*") if p.is_file() and p != SUMS)


def sums_text() -> str:
    return "".join(f"{sha256_file(p)}  {p.relative_to(FIXTURES).as_posix()}\n" for p in fixture_files())


def write_sums() -> None:
    SUMS.write_text(sums_text())


def verify_hashes(_: argparse.Namespace) -> None:
    want = SUMS.read_text() if SUMS.exists() else ""
    got = sums_text()
    if want != got:
        a, b = set(want.splitlines()), set(got.splitlines())
        for line in sorted(a - b):
            print(f"expected {line}")
        for line in sorted(b - a):
            print(f"found    {line}")
        raise SystemExit("fixtures/store-formats differ from SHA256SUMS; frozen fixtures are never edited")
    print(f"ok: {len(got.splitlines())} fixture files match SHA256SUMS")


# ---------------------------------------------------------------- new step


def current_schema(engine: str) -> str:
    if engine == "sqlite":
        return "".join((SQLITE_SQL / s).read_text() for s in SQLITE_SCRIPTS)
    return "".join((PG_SQL / s).read_text() for s in PG_SCRIPTS)


STEP = """//! Store upgrade step {engine} v{n} -> v{m} (#804).
//! Frozen source of v{n}: fixtures/store-formats/{engine}/v{n}/schema.sql;
//! its fixture is beside it. Describe what the step changes, then implement it
//! as one transaction that the frozen-fixture test checks.
"""


def new_step(args: argparse.Namespace) -> None:
    out = FIXTURES / args.engine / f"v{args.version}"
    step = REPO / args.step_dir / f"{args.engine}_v{args.version}.rs"
    if args.cli:
        args.out = str(out)
        capture(args)
    out.mkdir(parents=True, exist_ok=True)
    frozen = out / "schema.sql"
    if frozen.exists():
        raise SystemExit(f"{frozen} exists; it is frozen")
    frozen.write_text(current_schema(args.engine))
    if not step.exists():
        step.parent.mkdir(parents=True, exist_ok=True)
        step.write_text(STEP.format(engine=args.engine, n=args.version, m=args.version + 1))
    write_sums()
    log(f"froze {frozen.relative_to(REPO)}; step skeleton {step.relative_to(REPO)}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    def capture_options(p: argparse.ArgumentParser, cli_required: bool) -> None:
        p.add_argument("--engine", choices=("sqlite", "postgresql"), required=True)
        p.add_argument("--cli", required=cli_required, help="the old build's layerfs command")
        p.add_argument("--create-cli", help="an older sqlite build that creates the Store and writes up to the merge")
        p.add_argument("--bundle", help="its Linux bundle; beside the command's runtime when left out")
        p.add_argument("--machine", default="docker", help="the machine that runs the Store or its Sandbox")
        p.add_argument(
            "--pg-url", help="postgresql://HOST:PORT/postgres?sslmode=disable (no user; trust auth, role postgres)"
        )
        p.add_argument("--seed", type=int, default=SEED)
        p.add_argument("--keep", action="store_true", help="keep the scratch home and machine")

    c = sub.add_parser("capture", help="run the scenario with an old build and write a fixture")
    capture_options(c, True)
    c.add_argument("--out", required=True)
    c.set_defaults(func=capture)
    n = sub.add_parser("new-step", help="freeze the current schema and capture its fixture before a format change")
    capture_options(n, False)
    n.add_argument("--version", type=int, required=True, help="the format version being frozen")
    n.add_argument("--step-dir", default="engine/crates/layerfs-persistence/src/upgrade")
    n.set_defaults(func=new_step)
    v = sub.add_parser("verify-hashes", help="check fixtures/store-formats against SHA256SUMS")
    v.set_defaults(func=verify_hashes)
    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()

# AgentFS architecture overview

> **Status:** Research; informative and not a product contract.

Upstream: <https://github.com/tursodatabase/agentfs> (Turso; Rust core +
multi-language SDKs; MIT; beta). Pinned local snapshot:
`/Users/yifanxu/Ephemeral-AI-Lab/study/agentfs` — not a git checkout, pinned by
release **0.6.4** (`CHANGELOG.md` head, 2026-03-25; matches the `sdk/rust` and
`cli` crate versions). Written 2026-09-16 for issue
[#159](https://github.com/Ephemeral-AI-Lab/layerfs/issues/159), one of three
external studies feeding the v0.1.7 architecture refactor (#155).

Method: lead agent with 8 parallel research subagents (store model/schema; core
VFS access-durability-concurrency; overlay and host filesystems; audit trail;
snapshot/portability/sync/encryption; mount layer; SDK surfaces; sandbox and
agent story). The lead spot-checked ~40 load-bearing citations against the
source and reconciled the reports; no contradiction between reports survived
verification. Citations are `path:line` relative to the snapshot root for code
and `doc#section` for `SPEC.md`/`MANUAL.md`. Nothing was built, run, or
measured; performance is never claimed. Core diagrams are embedded at their
anchor sections (§2, §3, §4.1, §4.4, §4.7), added 2026-09-16 at owner request.

## 1. The system in one paragraph

AgentFS is "the filesystem for agents": an agent's entire runtime — file tree,
key-value state, and a tool-call log — lives in **one SQLite database file**
(`README.md:40-42`). It exposes three interfaces over that file: a POSIX-like
virtual filesystem (inode/dentry/chunk tables), a KV store, and an insert-only
tool-call audit table (`SPEC.md#tool-calls`, `SPEC.md#virtual-filesystem`,
`SPEC.md#key-value-data`). The engine is not SQLite-the-library but **turso** —
Turso's in-process, SQLite-compatible database written in Rust — pinned at
0.4.4 with the `sync` feature (`sdk/rust/Cargo.toml:9`), which is also what
enables embedded replication, WASM-in-browser, and at-rest encryption. The
product's framing: agents need *auditability* (query history with SQL),
*reproducibility* (snapshot = copy the file), and *portability* (one file moves
between machines) (`README.md:38-42`); isolation comes from a copy-on-write
overlay that puts every mutation of a host directory into the database while
the host stays untouched, exposed through FUSE on Linux, NFS on macOS, an MCP
tool server, and four language SDKs.

## 2. Component map

```text
agentfs/
├── SPEC.md / MANUAL.md / README.md / CHANGELOG.md   Spec v0.4; CLI reference; framing; history.
├── sdk/rust/                Reference core (agentfs-sdk 0.6.4, engine turso 0.4.4):
│   ├── lib.rs               AgentFS facade, AgentFSOptions, sync/encryption, /proc/mounts, overlay introspection.
│   ├── connection_pool.rs   "Pool" hard-limited to MAX_CONNECTIONS = 1; semaphore; 30 s timeout.
│   ├── schema.rs            SchemaVersion "0.4", detected by fs_inode column introspection.
│   ├── toolcalls.rs         tool_calls DDL; start/success/error (UPDATE) and record (insert-only).
│   ├── kvstore.rs           kv_store upsert/get/delete/keys (JSON in TEXT).
│   └── filesystem/          mod.rs (inode-based FileSystem/File traits, FsError→errno); agentfs.rs
│                            (SQLite VFS: chunked fs_data, BEGIN IMMEDIATE writes, dentry LRU, fsync
│                            dance); overlayfs.rs (delta-over-base COW); hostfs_{darwin,linux}.rs
│                            (host passthrough: path-based / fd-O_PATH-based).
├── sdk/typescript/          Node (@tursodatabase/database 0.4.0-pre.18) + browser (WASM) + serverless
│                            adapter + Cloudflare Durable Objects integration (a 5th implementation).
├── sdk/python/              pyturso==0.4.4, async.
├── sdk/go/                  modernc.org/sqlite v1.29.1 (pure-Go SQLite — no turso); largest surface
│                            (overlay, io/fs, streaming, SPEC conformance tests).
├── cli/                     The agentfs binary (0.6.4): cmd/ verbs (init, exec, run, mount, ps,
│   ├── src/fuser/           serve mcp, serve nfs, sync, migrate, fs, diff, timeline);
│   ├── src/nfsserve/        vendored pure-Rust FUSE library (no libfuse, Linux);
│   ├── src/{fuse,nfs}.rs    vendored NFSv3 server (mount proto + faked portmap, one port);
│   ├── src/mount/           FUSE/NFS adapters onto FileSystem; unified mount API (RAII handle);
│   ├── src/daemon.rs        double-fork daemonize() helper (FUSE background mounts);
│   └── src/sandbox/         run() guts: Linux user+mount namespaces; macOS sandbox-exec profiles;
│                            linux_ptrace (experimental).
├── sandbox/                 Crate agentfs-sandbox: Vfs trait, BindVfs/MountTable/FdTable, SqliteVfs;
│                            reverie-based ptrace syscall interception (Linux-only deps).
├── examples/                mastra, claude-agent, openai-agents (SDK-as-cache), ai-sdk-just-bash
│                            (SDK as bash-sandbox fs), cloudflare (Durable Objects), firecracker
│                            (NFSv3 as VM root filesystem).
└── dist-workspace.toml      Distributes only the cli binary.
```

Deployment shape: a single agent is one `.agentfs/<id>.db` file
(`sdk/rust/src/lib.rs:130-151`); every opener embeds the engine (no
client/server Store). Mounts add a server process each (FUSE daemon or NFS
listener); `agentfs run` forks a child into namespaces (Linux) or
`sandbox-exec` (macOS) with a per-session delta database at
`~/.agentfs/run/<id>/delta.db`.

The repository's own only diagram (`.github/assets/agentfs-arch.svg`, embedded
at `README.md:169`) presents just the SDK over the three interfaces over one
SQLite file (its text labels: `AgentFS SDK`, `Filesystem`, `Key-Value`,
`Toolcalls`, `SQLite Database File`, `B-Tree`, `Page`) — it omits mounts,
overlay, sandbox, and sync entirely.

```text
Deployment and openers                          .agentfs/<id>.db
                                                 (+ .db-wal / .db-shm sidecars;
                                                  + <db>-info when synced)
                                                         ▲
         ┌──────────────────────┬──────────────────────┬─┴──────────────┐
         │                      │                      │                │
    SDK embeddings         mounts (one server     run sessions       CLI one-shots
    Rust (turso 0.4.4)     process per mount):    ~/.agentfs/run/    (init/exec/fs/
    TS (turso 0.4.0-pre,   FUSE daemon (Linux)    <id>/delta.db      diff/timeline/
    WASM, serverless)      NFSv3 listener        + mnt/ + child      sync/migrate)
    Python (pyturso)       (macOS; serve nfs)    in namespaces /     — also embed
    Go (modernc sqlite)    MCP server (stdio)    sandbox-exec        the engine
```

Every path into the database embeds the engine in-process; there is no
client/server Store process anywhere in the design.

## 3. Authority and data flow

```text
Layering and the write path — one trait, many adapters, one serialized lane

 kernel FUSE            NFS client            SDK calls           MCP tools/call
     │                     │                    │                     │
 cli/src/fuse.rs       cli/src/nfs.rs        path-based API       cli/src/cmd/
     │                 (AgentNFS)                │                mcp_server.rs
     └───────┐             └───────┐             │             ┌──────┘
             ▼                     ▼             ▼             ▼
      FileSystem trait (inode-based: lookup / readdir_plus / open→File / rename / …)
             │
      ┌──────┴────────┐
      ▼               ▼
  OverlayFS        AgentFS (delta = the .db)
  (base = HostFS       │
  over a host dir)     ▼
      │         ConnectionPool — MAX_CONNECTIONS = 1, semaphore, 30 s timeout
      └───────────────►│
                      ▼
                turso engine → PRAGMA synchronous = OFF at open;
                fsync() escalates FULL → empty BEGIN/COMMIT → OFF;
                BEGIN IMMEDIATE on data writes / create_file / rename;
                metadata ops (mkdir, unlink, …) autocommit statement-by-statement
```

**State.** All durable state is the one database: mutable
`fs_inode`/`fs_dentry`/`fs_data`/`fs_symlink` rows; overlay metadata in
`fs_whiteout`/`fs_origin` plus the Rust-only, not-in-SPEC `fs_overlay_config`
that embeds the base directory's absolute canonical path
(`sdk/rust/src/filesystem/overlayfs.rs:95-126`); KV in `kv_store`; audit in
`tool_calls`. A synced database has an engine-managed `{db}-info` sidecar
marker (`sdk/rust/src/lib.rs:310,329-336`), and ordinary use leaves engine WAL
sidecars (`.db-wal`, `.db-shm`) next to the file (`sdk/rust/src/lib.rs:669,847-851`).

**Who may write.** Anyone who can open the file — no file locking, lockfile, or
ownership check in the core. Within one `AgentFS` handle all operations
serialize through a pool of exactly one connection
(`sdk/rust/src/connection_pool.rs:14`); cross-handle/cross-process safety is
delegated to engine locking plus `PRAGMA busy_timeout = 5000`
(`sdk/rust/src/filesystem/agentfs.rs:439-441`). The FUSE and NFS adapters
further serialize the whole filesystem behind one `Arc<Mutex<dyn FileSystem>>`
(`cli/src/nfs.rs:54-64`; `cli/src/cmd/run_darwin.rs:90`).

**Write path.** Mutations take the single pooled connection. File *data*
writes, truncates, `create_file`, and `rename` each run in one
`TransactionBehavior::Immediate` transaction
(`agentfs.rs:192,246,1249,1410,1615,2309,3096,3611`); metadata ops (`mkdir`,
`mknod`, `symlink`, `link`, `unlink`, `rmdir`) autocommit statement-by-statement
with no transaction, so intermediate states — orphan inodes, chunks leaked by a
crash mid-`unlink` — are possible (`agentfs.rs:2977-3065,3341-3513`).
Connections open with `PRAGMA synchronous = OFF` (`agentfs.rs:436-437`);
`fsync()` escalates to `PRAGMA synchronous = FULL`, runs an empty
`BEGIN; COMMIT` "to force a checkpoint", and restores OFF
(`agentfs.rs:2495-2507`) — durability is opt-in per `fsync` call.

**Read path.** Path resolution walks `fs_dentry` from root ino 1 through a
10 000-entry dentry LRU cache with no TTL or cross-validation, so a second
process's renames/unlinks are invisible to it until eviction
(`agentfs.rs:20,27-65,845-892`). Reads select only the needed
`(ino, chunk_index)` range and zero-fill missing chunks (`agentfs.rs:88-184`).
No read path runs in a transaction; a size read and a chunk read can straddle
another process's commit.

**Mount lifecycle.** `agentfs mount`: open the database first (schema errors
surface to the user), rebuild the overlay if `fs_overlay_config.base_path` is
set (`cli/src/cmd/mount.rs:88-183`), then mount — Linux FUSE via the vendored
library (direct `mount()` syscall, setuid `fusermount` fallback), optionally
daemonized by the double-fork helper (`cli/src/fuser/mnt/fuse_pure.rs:101-129`;
`cli/src/daemon.rs:24-110`); macOS via the in-tree NFSv3 server on
`127.0.0.1:<port>` (default 11111), where "background" mode just blocks forever
on the server task — no fork (`cli/src/cmd/mount.rs:283-312`). `exec`/`init -c`
mount to a temp dir, run a command, auto-unmount (`cli/src/cmd/exec.rs:21-121`).
Unmount: `fusermount -u` / `umount` with lazy/force fallbacks
(`cli/src/mount/nfs.rs:19-77`).

**Concurrency and audit.** One serialized lane per handle; one writer at a time
per database at the engine level. Kernel-visible caches are made coherent by
construction: FUSE negotiates `FUSE_WRITEBACK_CACHE` with entry/attr TTL of
`Duration::MAX` plus explicit deferred `FUSE_NOTIFY_INVAL_ENTRY` after
mutations, "safe because we are the only writer to the filesystem"
(`cli/src/fuse.rs:71-74,124-133`); NFS relies on `wcc_data` with nanosecond
timestamps (`cli/src/nfs.rs:66-109`; `SPEC.md#revision-history`). The audit
trail is produced on none of these paths — only explicit
`tools.record`/`start`/`success`/`error` calls insert `tool_calls` rows
(`sdk/rust/src/toolcalls.rs:129-265`).

## 4. Implementation details

### 4.1 Store model and schema (SPEC v0.4 vs code)

```text
One file, four subsystems, no transactional coupling between them

.agentfs/<id>.db
├── VFS (mutable in place)   fs_inode ←── fs_dentry (UNIQUE parent_ino + name)
│                            fs_data (4 KiB chunks, keyed (ino, chunk_index))
│                            fs_symlink (ino → target)
├── Overlay metadata         fs_whiteout (path → deleted-from-base)
│                            fs_origin (delta_ino → base_ino)
│                            fs_overlay_config (base_path — absolute host path)
├── KV                       kv_store (key, JSON-in-TEXT value, created/updated_at)
└── Audit                    tool_calls (insert-only per SPEC; UPDATEd in practice)
```

Nine SPEC tables (`tool_calls`, `fs_config`, `fs_inode`, `fs_dentry`, `fs_data`,
`fs_symlink`, `fs_whiteout`, `fs_origin`, `kv_store`). Code deviations:

- `tool_calls`: Rust/TypeScript/Python add `status TEXT NOT NULL DEFAULT
  'pending'` and make `completed_at`/`duration_ms` nullable
  (`sdk/rust/src/toolcalls.rs:95-105`); Go implements the SPEC literally (no
  `status`, NOT NULL timestamps — `sdk/go/schema.go:66-76`). The two families
  write incompatible rows to the same table name.
- `fs_whiteout`: SPEC requires `parent_path` + parent index; Go matches;
  Rust's table is `(path, created_at)` only (`overlayfs.rs:97-101`) — Rust and
  Go overlay databases are mutually incompatible despite the Go README's
  compatibility claim (`sdk/go/README.md:503-507`).
- Root inode: SPEC says `nlink=1` (`SPEC.md#initialization`); Rust inserts
  `nlink=2` with the calling user's uid/gid and rewrites root ownership on
  every open (`agentfs.rs:597-609`).
- No foreign keys anywhere — "Manually handle cascading deletes since we don't
  use foreign keys" (`agentfs.rs:2186`); sparse files intentionally break
  "size MUST match chunks" (`agentfs.rs:207-208`).
- Versioning: `AGENTFS_SCHEMA_VERSION = "0.4"`; Rust detects legacy versions by
  `fs_inode` column introspection and fails the open on mismatch
  (`sdk/rust/src/schema.rs:54-104`); `agentfs migrate` bypasses the SDK open
  ("to avoid version check", `cli/src/cmd/migrate.rs:29-34`) and applies
  additive idempotent ALTER steps (v0.0→v0.2 adds `nlink`; v0.2→v0.4 adds nsec
  + `rdev`; `migrate.rs:118-222`). Go stamps `schema_version` with
  `INSERT OR IGNORE` *before* comparing (`sdk/go/agentfs.go:158-169`), so a
  legacy DB is stamped current and passes.
- No SPEC extension point (xattrs, ACLs, quotas, version history, dedup, TTL)
  is implemented anywhere; FUSE xattrs are explicit ENOSYS stubs
  (`cli/src/fuser/mod.rs:560-578`).

### 4.2 Engine, connections, durability

The engine bet: `turso` 0.4.4 (Rust) and `pyturso` 0.4.4 (Python); TypeScript
runs a *pre-release* engine line (`@tursodatabase/database ^0.4.0-pre.18`); Go
runs stock pure-Go SQLite (`sdk/go/go.mod:5`). turso leaves WAL sidecars in
ordinary use and panics — no typed error — when an encrypted database is opened
with a wrong or missing key (`sdk/rust/src/lib.rs:888-915`). Encryption is
whole-database at rest, experimental (`experimental_encryption(true)`,
`lib.rs:338-346`), mutually exclusive with sync, enforced in code with no
stated reason (`lib.rs:302-307`; `MANUAL.md#local-encryption`).

### 4.3 Chunked VFS mechanics

`fs_data` rows are fixed-size chunks (default 4096, immutable in `fs_config`).
Handle writes do read-modify-write of affected chunks — SELECT existing chunk →
zero-resize → splice → `INSERT OR REPLACE` — and update `fs_inode.size` in the
same Immediate transaction (`agentfs.rs:344-418,213-222`); full-chunk writes
replace without reading. Reads clamp to the size snapshot and zero-fill missing
chunk indexes (sparse by omission; `agentfs.rs:88-184`). Two parallel APIs
diverge: the path-based `pread` concatenates existing chunks without
size-clamping or zero-fill, so a sparse file reads back *shifted* through the
path API and zero-filled through the handle API (`agentfs.rs:1310-1379`).
`statfs` reports logical `SUM(size)` (`agentfs.rs:2470-2486`). Nothing
content-addresses or dedups anything: every copy-up and rewrite stores new bytes.

### 4.4 Overlay and host filesystems

`OverlayFS::new(base: Arc<dyn FileSystem>, delta: AgentFS)` (`overlayfs.rs:65`)
— base is trait-typed (in practice `HostFS` over the host cwd); the delta is
the agent database.

```text
Copy-on-write flow per guest operation on /path

 OverlayFS lookup
      │
      ▼
 whiteout (path or any ancestor)? ──yes──► ENOENT      (code order; SPEC says
      │no                                               delta-first — a drift)
 delta layer (the .db)? ──yes──► serve delta inode
      │no
 base layer (HostFS over the real dir) ──► serve base, read-only by discipline

 first open (even O_RDONLY) / chmod / chown / utimens / link / rename
      └──► copy_up: whole-file pread → create_file + pwrite into delta;
           record fs_origin (delta_ino → base_ino) for inode stability
 delete of a base entry
      └──► INSERT fs_whiteout (path-keyed; ancestor checks hide subtrees;
            no opaque-directory flag)
```

Copy-up is **whole-file and happens on first `open`
regardless of flags — even `O_RDONLY` — and on
`chmod`/`chown`/`utimens`/`link`/`rename`** (`overlayfs.rs:904-971,501-599`):
one full `pread` + one `create_file` + one `pwrite`; directories copy shallowly;
timestamps are not preserved. Whiteouts are path-keyed rows; `is_whiteout`
checks the path *and all ancestors*, so whiteouting a directory hides its whole
base subtree (`overlayfs.rs:193-205`); there is **no opaque-directory flag**.
Lookup order in code is whiteout → delta → base (`overlayfs.rs:640-708`),
whereas `SPEC.md#overlay-lookup-semantics` says delta → whiteout → base — a
SPEC/code disagreement, normally masked because creates remove whiteouts.
Origin tracking (`fs_origin`) preserves inode stability across copy-up by
reusing the overlay inode assigned to the base inode in lookup/readdir_plus —
not, as `SPEC.md#inode-origin-tracking` phrases it, by substituting the base
inode in stat (`overlayfs.rs:659-676,713-733`). `HostFS` is a full passthrough
(all mutating methods are real syscalls — base read-only-ness is overlay
discipline, not a HostFS property): darwin path-based with a self-acknowledged
TOCTOU trade-off, linux fd/O_PATH-based after libfuse's `passthrough_hp`
(`hostfs_darwin.rs:43-47`; `hostfs_linux.rs:36-55`). Overlay in-memory maps are
process-local and never refreshed after `load()`; `forget` never removes
mappings; overlay inode numbers restart at 2 each session
(`overlayfs.rs:88,1319-1321`). `agentfs diff` classifies delta paths and
whiteouts against the *live host* (`A|M|D <type> <path>`; "Modified" = exists
in both layers, not bytes differ) (`cli/src/cmd/fs.rs:199-270`).

### 4.5 Audit trail

Only explicit tool-call records exist: `tools.start` inserts a `pending` row;
`success`/`error` UPDATE it; `tools.record` is the SPEC-compliant insert-once
method (`sdk/rust/src/toolcalls.rs:129-265`). No filesystem or KV operation
writes an audit row anywhere; the MCP server records nothing (zero
`tool_calls` references in `cli/src/cmd/mcp_server.rs`); the shipped examples
never call `tools.record`. README's "Every file operation, tool call, and state
change is recorded" (`README.md:40`) is broader than the implementation:
**state persistence ≠ event recording** — all *state* is queryable, but no
*events* are captured automatically. `agentfs timeline` reads
`recent(limit)` then filters name/status in memory, so `--filter` returns a
subset of the N newest rows overall, not the N newest matching rows
(`cli/src/cmd/timeline.rs:53-65`). The two-phase start/success pattern violates
the SPEC's own insert-only rule in three of four SDKs, and a process death
between `start` and `success` strands permanent `pending` rows. Nothing prunes
or bounds the log.

### 4.6 Snapshot, restore, portability, sync

Snapshot-as-copy is the *only* snapshot mechanism: no restore/rollback command
or API exists, and MANUAL has no snapshot section. `cp agent.db snapshot.db` is
presented as unconditional (`README.md:41`), but ordinary use leaves
`.db-wal`/`.db-shm` sidecars and the only checkpoint logic is `fsync()`'s
FULL-escalation (sync databases additionally get `agentfs sync checkpoint`,
`sdk/rust/src/lib.rs:443-447`) — a copy taken while a writer is open can miss
recent commits, and no document states the precondition. The FAQ's
"SQLite's write-ahead log enables snapshotting and time-travel forking"
(`README.md:191`) has **no mechanism in code**: fs tables mutate in place, no
history/version columns exist, "Version history and snapshots" is an
unimplemented SPEC extension point, and the WAL is never surfaced. Portability
caveats: overlay databases embed an absolute host path
(`fs_overlay_config.base_path`); a copied *synced* `.db` silently reopens as a
plain local database (the `-info` sidecar is left behind); uid/gid are
machine-relative; old schemas fail the open until `agentfs migrate` runs —
which itself cannot handle encrypted or synced databases (no `--key`, local
builder only; `cli/src/cmd/migrate.rs:29-34`). Sync is whole-database engine
replication: `turso::sync::Builder::new_remote` with remote URL + auth token,
`pull/push/checkpoint/stats` as one-line delegations
(`sdk/rust/src/lib.rs:316-336,429-454`); partial sync (128 KiB prefix bootstrap
/ segment defaults) is explicitly experimental (`cli/src/cmd/init.rs:30-70`).
Frame format, conflict semantics, and checkpoint internals are engine
properties outside the snapshot.

### 4.7 Sandbox and execution

```text
agentfs run — three platform modes over the same overlay composition

 Linux (default)                macOS                        Linux (experimental)
 FUSE mount of the overlay      NFS-serve the overlay        ptrace interception
 at ~/.agentfs/run/<id>/mnt     on 127.0.0.1:<port>          (reverie-based)
      │                              │                            │
 child: unshare(NEWUSER|        child: sandbox-exec SBPL      child: MountTable +
   NEWNS); overlay bind-        profile: (deny default),      FdTable; SqliteVfs
   mounted onto the cwd;        (allow file-read*) — the      serves the db at
   everything else remounted    overlay does copy-on-write;   /agent (whole file
   read-only except the         writes only to an allowlist;  read into memory per
   allow-list                   network: always allowed       open; written back
                                                               on fsync/close)
```

Three modes. (1) **Linux default**: per-session `delta.db` as overlay delta
over the host cwd; the cwd fd is opened *before* the FUSE mount so the base
layer bypasses the overlay (`/proc/self/fd/N`); the child
`unshare(CLONE_NEWUSER | CLONE_NEWNS)`, gets uid/gid maps, private mounts, the
overlay bind-mounted onto the cwd, and everything else remounted read-only
except an allow-list (`cli/src/sandbox/linux.rs:13-16,179-243,584-647,654-798`).
(2) **macOS**: the same overlay served over NFSv3 on localhost; the child runs
under a generated `sandbox-exec` SBPL profile — `(deny default)`,
`(allow file-read*)` (the overlay does copy-on-write), writes restricted to the
mountpoint, run dir, `/tmp` variants, `/dev`, Keychain/Spotlight paths, and
`--allow` extras; `agentfs run` hardcodes full network access
(`cli/src/sandbox/darwin.rs:63-171`; `cli/src/cmd/run_darwin.rs:365-396`).
(3) **Linux experimental**: the `agentfs-sandbox` crate — reverie-based ptrace
interception of a defined syscall set (file/process/stat/xattr), a guest
`MountTable`/`FdTable` virtualizing fds, and `SqliteVfs` exposing the database
at `/agent` with **whole-file read-into-memory on open and write-back-on-fsync**
(`sandbox/src/vfs/sqlite.rs:131-201,476-536`); unknown syscalls fail with
ENOSYS, not passthrough (`sandbox/src/syscall/mod.rs:498-507`). Default
writable dirs are agent-tool homes (`.claude`, `.config`, `.npm`, …) and differ
from `MANUAL.md#agentfs-run`'s list (macOS code has `.bun/.amp/.gemini` and no
`.codex`; `cli/src/cmd/run_darwin.rs:205-217`); `AGENTFS_SESSION` is set on
Linux only, documented for both. Sessions persist (delta kept, resumable,
`agentfs ps`); there is no commit/publish step — the delta *is* the result, and
`agentfs diff` is the review surface.

## 5. Interfaces

### 5.1 CLI and mount UX

Verbs per `MANUAL.md`: `init` (incl. `--base`, `--key/--cipher`,
`--sync-remote-url` + partial-sync options, `-c`); `exec`; `run`; `mount`
(listing = Linux `/proc/mounts` parse filtering `agentfs:` sources — NFS mounts
are invisible to it); `serve mcp`; `serve nfs` (the Firecracker example boots a
VM with the export as its *root filesystem*,
`examples/firecracker/firecracker.sh:48-102`); `sync`; `migrate`; `fs`;
`diff`; `timeline`. One `FileSystem` trait, two mount backends behind a
unified RAII `mount_fs` (`cli/src/mount/mod.rs:1-15`); default backend FUSE on
Linux, NFS elsewhere; macOS FUSE is refused outright
(`cli/src/mount/mod.rs:177-191`). macFUSE was supported until 0.4.0 removed it
"replaced by NFS" (`CHANGELOG.md:237-239`) — no rationale exists in-repo.

FUSE specifics: the vendored pure-Rust fuser (direct `mount()` syscall with
fusermount fallback); single-threaded read-dispatch loop with one reused 16 MiB
buffer; `readdirplus` backed by the SDK's `readdir_plus`, "avoiding N+1
database queries" (`cli/src/fuse.rs:344,410-418`); `flush` a no-op since writes
go straight to the database; and a **deferred notifier** because
`FUSE_NOTIFY_INVAL_ENTRY` from the session thread deadlocks the kernel
(`d_invalidate → iput → FUSE_FORGET` while the loop is blocked in `writev()`)
(`cli/src/fuser/deferred_notify.rs:13-24`).

NFS specifics: NFSv3 over TCP with SunRPC record marking, the mount protocol,
and a faked portmapper that always returns the listener's own port —
portmap/mountd/nfsd all on one unprivileged port
(`cli/src/nfsserve/portmap_handlers.rs:79-96`); 16-byte filehandles = process
generation number + fileid, so **all client handles go stale across a server
restart** (`cli/src/nfsserve/vfs.rs:48-61,277-298`); real `wcc_data` pre/post
attributes on mutating ops; writes always answer `FILE_SYNC`; retransmissions
suppressed by xid (at-most-once replies, not replay); macOS-client cookie
quirks handled by never returning `BAD_COOKIE`
(`cli/src/nfsserve/nfs_handlers.rs:927-1048`); no silly-rename handling; NFS
`COMMIT` is `proc_unavail`. In code terms: FUSE negotiates kernel caches
directly (writeback, TTL=MAX + explicit invalidation); NFS buys no-kext,
no-dependency mounting and network exportability at the price of client-side
cache semantics, single-mutex serialization, and stale handles on restart. No
in-repo document quantifies the latency difference.

### 5.2 SDKs across four languages

Four engines, four independent implementations of the same SQL schema (no SDK
calls another): Rust turso 0.4.4; TypeScript `@tursodatabase/database
^0.4.0-pre.18` (Node), `database-wasm` (browser — `openWith(db)` only, cannot
open a path), `@tursodatabase/serverless` (remote HTTP, not a local engine);
Python `pyturso==0.4.4`; Go `modernc.org/sqlite v1.29.1`. Surface drift:
Rust's `FileSystem` is inode-based (kernel-shaped: `lookup`, `readdir_plus`,
`forget`, `TimeChange`, flags on `open`); TS/Python/Go are path-based — TS
lacks `link`/`chmod`/`mknod`, Python lacks symlinks/`statfs`/file handles
entirely, Go is the superset (adds `MkdirAll`, `Chmod`, `Utimes`, `io/fs`,
configurable pool, chunk-size override). Semantic drift: symlink traversal is
full-resolution in Go, final-component-only in Rust, ENOSYS in TS
("symbolic links not supported yet", `sdk/typescript/src/guards.ts:64-73`),
absent in Python; Go puts id-based databases in `~/.agentfs/` (home) while
Rust/TS/Python use CWD-relative `.agentfs/` (`sdk/go/agentfs.go:249-255`);
`statfs` bytes are `SUM(size)` in Rust/Go but `SUM(LENGTH(data))` in TS;
`tool_calls` schemas and stats semantics differ per §4.1/§4.5. Rust-only:
overlay+hostfs composition, sync, encryption, ephemeral `:memory:`,
`/proc/mounts` parsing, overlay introspection for the CLI. The TypeScript
Cloudflare integration is a **fifth** full implementation over Durable Object
`SqlStorage` with its own copy of the DDL
(`sdk/typescript/src/integrations/cloudflare/agentfs.ts:292-380`). Only Go
ships an ID-tagged SPEC conformance suite (`sdk/go/SPEC_TESTS.md`), whose
status columns are unfilled in the snapshot.

### 5.3 MCP server

`agentfs serve mcp`: stdio JSON-RPC 2.0 (protocol 2024-11-05); tools
`read_file/write_file/readdir/mkdir/remove/rename/stat/access` +
`kv_get/kv_set/kv_delete`, mapping 1:1 onto SDK ops; single-text-block results;
`..`-containing paths rejected; `--tools` filtering; whole-tree
`resources/list`/`resources/read` (`cli/src/cmd/mcp_server.rs:73-227,644-916`).
Drift: `kv_list` is advertised in `tools/list` and `MANUAL.md#agentfs-serve-mcp`
but has no dispatch arm ("Unknown tool: kv_list", `mcp_server.rs:181-226`);
the CLI help lists tools (`rmdir`, `rm`, `unlink`, `copy_file`) that do not
exist in the server (`cli/src/opts.rs:307-309`).

### 5.4 What the SPEC fixes vs what bindings add

The SPEC fixes the nine-table SQL schema, chunk semantics, overlay lookup
order, and consistency rules, and explicitly leaves session grouping,
versioning, dedup, xattrs, and TTL as unimplemented extension points. The
bindings add, inconsistently: the `status` column and UPDATE-based recording
(Rust/TS/Python), `fs_overlay_config` (Rust), prefix-KV queries and typed
generics (Go), streaming/file-handle APIs (Rust/TS/Go), pooling (Rust hard-1 /
Go configurable), caching (Rust dentry LRU / Go path LRU), and engines.
Nothing machine-checks SPEC conformance except Go's (stale) suite.

## 6. What LayerFS can learn

LayerFS grounding used here: `docs/general/concepts.md` (durable + Workspace
model), `docs/roadmap/0.2/README.md` (target topology),
`docs/roadmap/0.2/agent-branch-reconciliation/README.md` (0.1↔0.2 gap),
`docs/roadmap/0.1/0.1.7/README.md` (this release's boundary),
`docs/roadmap/0.2/cloud-sqlite-vfs.md` (Store pragmas, WAL stance, replication
roadmap). These are recommendations handed to the v0.1.7 checklist, not
decisions.

### 6.1 Concept mapping

| AgentFS concept | Closest LayerFS concept |
| --- | --- |
| agent database `.agentfs/<id>.db` | `LayerStackStore` (one SQLite file) |
| mutable `fs_inode`/`fs_dentry`/`fs_data` | no equivalent — canonical immutable tree/file objects, content-addressed by `ObjectId` |
| fixed 4 KiB chunk (`ino, chunk_index`) | CDC chunks + rope extents (content-defined, deduplicated) |
| `tool_calls` insert-only log | Monitor receipts (typed, automatic, process-local; no durable equivalent) |
| `kv_store` | no equivalent (files are the state store; agent state is out of LayerFS ownership) |
| OverlayFS delta (mutable tables in the db) | Workspace dirty frontier / captured mutations |
| OverlayFS base = `HostFS` over live host dir | Workspace base = pinned Branch head snapshot (LayerFS never overlays a live host tree) |
| `fs_whiteout` path-keyed deletions | 0.2-roadmap OverlayFS projection whiteouts (unimplemented) |
| `fs_origin` copy-up inode stability | no equivalent (needed only because AgentFS overlay inodes are a remapped virtual space) |
| whole-file copy-up on first open | no equivalent — LayerFS capture is diff-based (CDC), never a private full copy |
| run session delta.db, resume, `ps` | Workspace lifecycle (Create→Exec→Commit→End; explicit Commit, CAS on Branch head) |
| `agentfs diff` (path classification vs live host) | Branch Diff / commit diff aspects (content-based) |
| `agentfs timeline` | no durable equivalent; Monitor snapshot is the in-process analog |
| sync pull/push (whole-database engine frames) | no equivalent; `cloud-sqlite-vfs.md` D1-C pack-log replication is the analog |
| `agentfs sync checkpoint` | no equivalent — Layer publication (LayerStack checkpoint) is the concept |
| mount via FUSE (Linux) / NFS (macOS) | `layerfs-fuse` projection |
| `serve nfs` (network export; Firecracker root fs) | no equivalent (network projection not in LayerFS) |
| run sandbox (namespaces / sandbox-exec / ptrace) | Workspace Exec/Shell in containers (`layerfs-daemon`) |
| `SqliteVfs` under ptrace | no equivalent |
| MCP tool server | no equivalent (0.2 "typed automation surface" is the slot) |
| encryption at rest | no equivalent |
| `{db}-info` sync marker sidecar | no equivalent |
| schema detection by column introspection | Store schema identity + preflight checks (v10) |
| — no AgentFS equivalent — | LayerStacks, Layers, Branches, Commits, immutable history, CAS publication (`HeadMoved`), global dedup, conflicts/reconciliation, Materialize projection, daemon protocol, Monitor `CandidateStats` |

### 6.2 Observations and verdicts

1. **"Auditability" is state-queryability plus a manual log — not automatic
   event recording.** AgentFS records no fs/kv events; only explicit
   `tools.record` calls reach `tool_calls` (`sdk/rust/src/toolcalls.rs:129-265`;
   zero audit writes in `cli/src/cmd/mcp_server.rs`), despite `README.md:40`.
   LayerFS is the mirror image: Monitor receipts capture every public
   operation automatically with typed outcomes and dedup statistics
   (`crates/layerfs-monitor/src/operation.rs:18-72`) — but they are
   process-local, not durable or queryable after the fact. **Verdict:
   borrow-with-changes.** A durable, insert-only, SQL-queryable receipt log
   populated from the existing typed receipts would give LayerFS what AgentFS
   promises but does not deliver. Do not copy the start-then-UPDATE pattern
   (violates insert-only; strands `pending` rows on crash). Patch-release
   stability says this is 0.2-scale, not 0.1.7.
2. **Claim accuracy is a documentation lesson.** Three headline claims outrun
   the code: "every file operation recorded" (`README.md:40`), unconditional
   snapshot-by-copy (`README.md:41` vs WAL sidecars and the sync `-info`
   marker), and WAL "time-travel forking" with no mechanism (`README.md:191`).
   **Verdict: adopt (as process).** LayerFS's documentation policy already
   requires measured facts; this is concrete evidence of how product framing
   drifts from implementation when nothing checks it.
3. **Snapshot-as-copy versus immutable checkpoints.** AgentFS's copy story is
   unsafe while a writer is open, has no in-band restore, and its databases
   embed machine-relative state (absolute overlay base path, uid/gid, sync
   marker). LayerFS's LayerStack/Layer model already answers this properly: a
   snapshot is a content-addressed immutable object set plus a catalog row,
   published by CAS, never mutated. **Verdict: nothing to adopt; keep the
   stronger design.** The residue worth copying: document copy/backup
   preconditions explicitly (what a Store file means on disk, when it is
   quiescent) — which AgentFS fails to do.
4. **Durability policy is per-transaction in both systems — convergent
   design.** AgentFS runs `synchronous = OFF` and escalates to FULL only inside
   `fsync` (`agentfs.rs:436-441,2495-2507`); LayerFS runs `journal_mode=MEMORY`,
   `synchronous=OFF`, `locking_mode=EXCLUSIVE` and escalates
   `reserve_inode_serials` to DELETE/FULL for its one durability-critical
   transaction (`docs/roadmap/0.2/cloud-sqlite-vfs.md` §1). Two independent
   systems arrived at "cheap by default, durable by explicit exception".
   **Verdict: not-applicable as a change** (LayerFS already does this), but it
   supports the cloud-vfs Stage 2 position that per-transaction policy is the
   right seam — and AgentFS's *unstated* crash window (what `synchronous=OFF`
   tears under turso's WAL is engine-internal) is exactly the gap LayerFS's
   docs currently acknowledge honestly.
5. **Serialized single-lane access with torn multi-step states.** AgentFS
   serializes per handle (pool of one) but leaves metadata ops autocommitting
   statement-by-statement, so other processes can observe orphan inodes and
   leaked chunks (`agentfs.rs:2977-3065,3341-3513`). LayerFS's one-writer Store
   plus atomic admission transactions is strictly stronger. **Verdict: avoid
   the pattern (already avoided).** One small borrow: AgentFS maps
   pool-timeout/busy to retryable signals (`EAGAIN` in FUSE,
   `NFS3ERR_JUKEBOX` in NFS; `cli/src/fuse.rs:27-41`, `cli/src/nfs.rs:33-52`)
   — LayerFS's `Busy`/`HeadMoved` outcomes already give agents the same
   machine-distinguishable retry contract, which this validates.
6. **Overlay mechanics are prior art for the 0.2 OverlayFS projection — take
   the mechanics, not the storage model.** AgentFS's whiteouts (path-keyed,
   ancestor-aware) and origin tracking (inode stability across copy-up) are a
   working, SPEC-documented overlay (`SPEC.md#overlay-filesystem`;
   `overlayfs.rs:193-205,659-676`). Its gaps map exactly onto LayerFS's 0.2
   checklist: **no opaque-directory flag** (LayerFS 0.2 requires proving
   opaque-directory behavior), no rename-of-directory subtree semantics, and a
   whole-file copy-up that duplicates bytes with zero dedup. **Verdict:
   borrow-with-changes** for the projection (whiteout/origin concepts, the
   stat-stability requirement, the kernel-inode-cache rationale); **avoid**
   whole-file copy-up and mutable-in-place delta tables for any LayerFS
   capture path — CDC + content addressing already solves this better.
7. **The FUSE layer holds three concrete, transferable lessons.** (a) Deferred
   `FUSE_NOTIFY_INVAL_ENTRY`: issuing kernel invalidations from the session
   thread deadlocks (`d_invalidate → iput → FUSE_FORGET` while blocked in
   `writev()`); AgentFS queues notifications to a dedicated thread
   (`cli/src/fuser/deferred_notify.rs:13-24`). (b) TTL=MAX entry caching with
   explicit post-mutation invalidation, justified by single-writer ownership
   (`cli/src/fuse.rs:71-74`) — the same invariant LayerFS has per Workspace.
   (c) `readdir_plus` to avoid N+1 lookups (`cli/src/fuse.rs:344,410-418`).
   **Verdict: borrow-with-changes** into `layerfs-fuse`. Contrast: AgentFS
   *vendored* fuser and nfsserve (`CHANGELOG.md:66-67`) — LayerFS repo policy
   forbids vendoring/patching third-party crates, so adopt the mechanisms,
   never the vendoring.
8. **NFS-as-mount-backend is a proven no-kext fallback with known costs.**
   macFUSE was removed at 0.4.0 in favor of an in-tree NFSv3 server: one
   unprivileged port, fake portmapper, and it doubles as a network export
   (even a Firecracker root fs). Costs visible in code: whole-fs serialization
   behind one mutex, stale filehandles across restart (generation number), no
   silly-rename, `COMMIT` unimplemented, correctness leaning on `wcc_data` +
   nanosecond timestamps (§5.1). **Verdict: not-applicable today** (LayerFS's
   FUSE projection is the current path); if a future projection needs a no-kext
   macOS or network-exportable backend, this is the trade-off table to start from.
9. **Four SDKs, four engines, four drifting implementations.** The
   `tool_calls` schema is mutually incompatible between Go and the other three
   (§4.1); `fs_whiteout` is incompatible between Go and Rust despite a
   compatibility claim (`sdk/go/README.md:503-507`); symlink semantics differ
   per language (§5.2); the engine is a different product per language,
   including a pre-release line for TypeScript; the only conformance suite
   (Go's) is stale. **Verdict: not-applicable for 0.1.x** (patch stability;
   `layerfs-sdk` is Rust-only) — but if LayerFS ever ships multi-language
   surfaces: one canonical core (WASM/FFI binding) or a machine-checked
   conformance suite; never N independent reimplementations of a written spec.
   A written spec alone demonstrably does not hold them together.
10. **Engine-frame sync cannot express LayerFS's invariants.** AgentFS sync
    replicates the whole database via engine frames and marks sync-ness with a
    sidecar that a plain file copy silently strands (§4.6). LayerFS's
    append-only pack region is precisely what a frame replicator cannot name —
    `cloud-sqlite-vfs.md` §3's conclusion (replicate the pack log, keep a
    catalog authority) is validated by this counter-example. **Verdict: avoid
    engine-frame sync; not-applicable today.** The marker-file hazard is a
    portability caution for any future LayerFS marker scheme.
11. **No lifecycle, no multi-agent story — LayerFS 0.2's problem is unsolved
    here.** AgentFS sessions accumulate a delta with no commit/publish
    boundary; "multi-agent" means sharing one session database with no locking,
    merge, or namespacing (§4.7; `SPEC.md#extension-points` lists session
    grouping as unimplemented). **Verdict: not-applicable** — nothing to adopt;
    confirmation that the 0.2 reconciliation problem statement is real and
    unclaimed territory.
12. **An MCP tool surface is cheap and agents actually use it.** The MCP server
    is ~950 lines exposing SDK ops as tools (`cli/src/cmd/mcp_server.rs`), and
    the example agents consume AgentFS through SDK embeddings rather than
    mounts. LayerFS's 0.2 "typed automation surface" could ship with an MCP
    delivery vehicle exposing lifecycle operations. **Verdict:
    borrow-with-changes, 0.2 scale** — LayerFS's surface is lifecycle
    operations with typed outcomes, not flat file ops; and the drift bugs
    (advertised-but-undispatched `kv_list`) show why the tool list must be
    generated from one source.

### 6.3 Direct answers to the six comparison questions

- **Auditability vs `layerfs-monitor`:** verdict 1 — LayerFS records more,
  durably less; the gap is persistence of receipts, not recording.
- **Snapshot-as-copy vs LayerStack/Layer:** verdict 3 — keep the stronger
  immutable-checkpoint design; document copy preconditions.
- **Single-file portability vs Store layout:** both are single-file-with-caveat.
  AgentFS's caveats: engine WAL sidecars, a sync marker sidecar, an embedded
  absolute overlay base path (§4.6). LayerFS's Store has *no* journal sidecar
  by policy (`MEMORY` journal) but trades that for no crash guarantee — an
  honest trade that should stay explicit in docs.
- **SDK-across-languages vs `layerfs-sdk`:** verdict 9 — stay single-core;
  multi-language only with one core or machine-checked conformance.
- **FUSE/NFS trade-offs vs `layerfs-fuse`:** verdicts 7-8 — borrow deferred
  invalidation, the TTL strategy, `readdir_plus`; NFS is the fallback option,
  not the path.
- **SQLite pragmas vs LayerFS's:** verdict 4 — convergent per-transaction
  durability. AgentFS inherits WAL-by-default from the engine where LayerFS
  rejects WAL at preflight; the deeper difference is ownership of the
  one-writer invariant (LayerFS: `EXCLUSIVE` file lock; AgentFS: engine
  busy-locking, weaker and multi-process).

## 7. Unknowns

1. **turso engine internals** (not in the snapshot): default journal mode and
   checkpoint cadence (WAL implied by sidecar names and `README.md:191` but
   never set in code); what `synchronous=OFF` tears on crash; whether the
   fsync FULL-escalation actually checkpoints as commented;
   `Transaction`-on-drop semantics; sync frame format and conflict rules;
   `-info` contents; encryption page coverage. The study's largest blind spot;
   it caps every durability claim above.
2. **Behavioral statements are static reading, not execution** —
   filter-after-limit in `timeline`, cross-SDK schema incompatibilities, FIFO
   copy-up blocking, base-directory rename semantics (no test covers it). The
   read-only mandate forbade running anything.
3. **Why macFUSE was removed** — only the CHANGELOG line exists
   (`CHANGELOG.md:237-239`); no rationale in-repo.
4. **The `agentfs:fuse` internal mount** filtered by `get_mounts()`
   (`sdk/rust/src/lib.rs:59-62`) is created by no code in this snapshot —
   apparently vestigial; unverifiable without history.
5. **README's `/agent` run transcript** (`README.md:102-112`) matches only the
   experimental ptrace mode, not the default paths; cannot be reconciled
   without execution.
6. **`agentfs-sandbox` publication status** — no `publish` field; badges list
   only `agentfs-sdk`; cannot confirm crates.io presence offline.
7. **Whether the Go conformance matrix was ever current** —
   `sdk/go/SPEC_TESTS.md` status columns are blank in the snapshot.
8. **Claim chronology** — CHANGELOG has no tool-call entries, so the
   `record`/`start` split and the `status` column cannot be dated against SPEC
   revisions from this snapshot.

## 8. Source index

Subject (snapshot root `/Users/yifanxu/Ephemeral-AI-Lab/study/agentfs`):
`README.md`, `SPEC.md`, `MANUAL.md`, `CHANGELOG.md`, `TESTING.md`;
`sdk/rust/Cargo.toml`, `sdk/rust/src/{lib,connection_pool,error,kvstore,schema,
toolcalls}.rs`, `sdk/rust/src/filesystem/{mod,agentfs,overlayfs,hostfs_darwin,
hostfs_linux}.rs`; `sdk/typescript/package.json`, `src/{agentfs,errors,guards,
toolcalls,kvstore}.ts`, `src/filesystem/interface.ts`,
`src/integrations/{cloudflare/agentfs,serverless/adapter}.ts`;
`sdk/python/pyproject.toml`, `agentfs_sdk/{agentfs,filesystem,kvstore,
toolcalls,errors,guards}.py`; `sdk/go/{go.mod,agentfs,schema,filesystem,
overlay,toolcalls,kvstore,errors,iofs}.go`, `sdk/go/README.md`,
`sdk/go/SPEC_TESTS.md`; `cli/src/{main,opts,fuse,nfs,daemon}.rs`,
`cli/src/mount/{mod,fuse,nfs}.rs`, `cli/src/fuser/{mod,session,
deferred_notify,notify}.rs`, `cli/src/fuser/mnt/fuse_pure.rs`,
`cli/src/nfsserve/{rpcwire,portmap_handlers,vfs,permissions,
transaction_tracker,write_counter,nfs_handlers}.rs`, `cli/src/cmd/{init,exec,
run,run_darwin,mount,ps,nfs,sync,migrate,fs,timeline,mcp_server}.rs`,
`cli/src/sandbox/{darwin,linux,linux_ptrace}.rs`; `sandbox/Cargo.toml`,
`sandbox/src/lib.rs`, `sandbox/src/vfs/{mod,bind,mount,fdtable,file,sqlite}.rs`,
`sandbox/src/syscall/mod.rs`, `sandbox/src/sandbox/mod.rs`;
`examples/{ai-sdk-just-bash,claude-agent,openai-agents,mastra,cloudflare,
firecracker}/`.

LayerFS grounding (read-only): `docs/general/concepts.md`,
`docs/roadmap/0.2/README.md`,
`docs/roadmap/0.2/agent-branch-reconciliation/README.md`,
`docs/roadmap/0.1/0.1.7/README.md`, `docs/roadmap/0.2/cloud-sqlite-vfs.md`,
`crates/layerfs-monitor/src/{lib,operation}.rs`, `crates/layerfs-sdk/src/lib.rs`.

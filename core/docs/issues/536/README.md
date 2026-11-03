# #536 layerfs-control: one management layer for machines, Stores, Sandboxes and Workspaces

> **Status:** Proposal; target LayerFS 2.x; not a released contract.
> Sub-issue of [#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535);
> **starts after #535's terminal pass** (superseded 2026-11-03: the owner
> chose to interleave #535 and #536 on one branch with one combined terminal
> pass; the order is in [535/HANDOFF-2.md](../535/HANDOFF-2.md) §5). Written 2026-11-03 from the owner's
> design session and four independent read-only reviews (architecture and
> crate boundaries; states, records and locks; object model and calls;
> simplicity, migration and tests). Code facts were read at
> `layerfs_v2_0_0` `58b8d1742b`; #535 changes many of them, so re-read
> everything at #535's final commit before starting.

## 1. Outcome

All management of machines, Stores, projects, Sandboxes, Workspaces and
buckets lives in one library, **`api/crates/layerfs-control`**, with one home
layout, one way to record an operation in flight, and states that can be
read from the records instead of reconstructed by scans.

- **Organisation.** The home has one folder per entity kind, and every
  entity has one identity: its name. Sandboxes are at the top of the home
  (`sandbox/<name>/`); Workspaces are children of their Sandbox; projects are
  children of their Store.
- **Relationships.** A machine holds 0..n sqlite Stores (each sqlite Store
  names its one machine). Every PostgreSQL Store is usable from every machine,
  with no attach records. A Sandbox names its machine and its one project; a
  project may have many Sandboxes.
- **sqlite moves by push and pull.** `store.push` streams the file from
  `native` into a machine's worker; `store.pull` brings it back to `native`.
  The caller drives both. Formerly: the caller streams the file into the
  machine's worker, which verifies and installs it; then the record names the
  new machine and the old copy is deleted. One live copy, never pulled by a
  guest. Between two machines: pull, then push.
- **States are facts plus one intent.** A record holds facts and, while an
  operation runs, one `op` intent with an id and an owner. Everything else
  (running, idle, in use, gone) is derived when asked.
- **`layerfs-api` becomes thin:** the 57-call table, argument checks, scopes
  and the JSON codec. Everything that decides, records or executes moves to
  `layerfs-control`.
- **Less code.** About 4,000–5,000 production lines of 1.1-era host
  paths are deleted with the op names that only they served.

Done means:

- the layouts of §5 and §6 exist and nothing else; a 2.0 home converts once;
- no call scans every Sandbox record; `sandbox.*` and `workspace.*` read one
  folder;
- every multi-step operation records exactly one `op` intent in its entity's
  record, with the same owner identity everywhere;
- `layerfs-api/src` holds `calls/`, `lib.rs` and nothing that opens a Store,
  writes a record or talks to a worker or daemon;
- the terminal pass (§12) passes.

## 2. Settled decisions

Owner, 2026-11-03.

| # | Decision |
| --- | --- |
| C1 | The management layer is a crate named **`layerfs-control`**, in the api layer (`api/crates/layerfs-control`, reconsolidated 2026-11-03): the library under the call surface `layerfs-api`. `connect/` keeps drivers only. |
| C2 | Sandboxes are recorded at the top of the home, `sandbox/<name>/`; the name is the only identity. The 1.1-era numbered state (`store/<s>/project/<p>/sandboxes/<n>/`), `machine/<m>/sandbox/<name>/` and `machine/native/sandbox-root/` are retired into it. |
| C3 | Workspaces are children of their Sandbox (`sandbox/<name>/workspace/<id>.json`) and may have an optional user-chosen name, unique within the Sandbox. |
| C4 | PostgreSQL Stores are usable from every machine automatically; there is no `store.attach`/`store.detach` (applied in #535 G3 and §7.4). Reach is rewritten per platform and checked inside the first `sandbox.create` on a machine. |
| C5 | sqlite Stores move by two caller-driven calls (owner, 2026-11-03): **`store.push <store> <machine>`** from `native` into a machine, and **`store.pull <store>`** from its machine back to `native`. Between two machines: pull, then push (#533 O12). Guests never fetch a Store themselves; no `store.copy`. |
| C6 | Projects are data in a Store; a Sandbox is compute bound to one project on one machine; a project may have many Sandboxes. |
| C7 | Backup to buckets stays a separate operation family (`layerfs-store-backup`). |

## 3. Gates

Recommended answers from the reviews. Each is recorded in this issue's
DECISIONS log in phase 0 once the owner confirms it.

| Gate | Question | Recommended answer |
| --- | --- | --- |
| H1 | Name of the sqlite placement calls. | **Owner, 2026-11-03: `store.push <store> <machine>`** (native → machine) and **`store.pull <store>`** (machine → native), replacing `store.move` (no alias; 2.0 is unreleased). `store.push` is refused unless the Store is on `native` ("Store app is on dev-lima; `store pull app` first"); `store.pull` is refused when it already is. |
| H2 | Shape of the Rust API. | **Free functions over a `Home` handle**: `<entity>::<op>(&Home, Request, &mut Events) -> Result<Record, Refusal>`. No `Control` object holding managers (it would become a service locator; repo rule). Typed records are the "objects". |
| H3 | Where the Store-call implementations (`fs/`, `branch/`, `base/dispatch.rs`) live. | **In the engine, `layerfs-store-service/src/methods/`** (reconsolidated 2026-11-03). They are the Store engine's typed operations and depend only on store-service, bridge, history and content; the worker server in `layerfs-control` calls them from below, so there is no cycle. |
| H4 | Persisted states. | **Facts and one `op` intent only.** No stored `Running`, `Moving`, `Restoring`, `Closing`, `Gone` or Workspace state. Derived states are computed by one `state()` function per entity. |
| H5 | Where intents live. | **Inside the entity's own record** (`op` field), not a separate journal folder. Owner identity is always `{pid, pid_start, pid_boot}`. |
| H6 | Settling an `Unknown` outcome. | **No new call.** The existing `machine.status`, `store.status` and `sandbox.get` results carry `op` (`{id, name, phase, started, uncertain}`) and, for an uncertain op, what the worker or daemon reports about it (read-only). Clearing the intent is an explicit existing call (`sandbox.close`, `store.remove`, a new `store.push`), never the read. |
| H7 | A reconciler. | **No reconciler and no new family.** One report-only **`layerfs doctor`** at the top level, beside `import-env` (CLI), also exposed as one top-level function `layerfs.doctor()` in Node and Python, outside the call table. It checks records, modes, broken references, dead and uncertain intents, stale leases, conversion blockers, machine-side leftovers and worker versions, and changes nothing; each finding names the call that fixes it. A `home` family is decided only if more home-wide calls arrive (H8). |
| H8 | Labels, `status --json`, per-entity idle policies. | **Deferred** (no consumer yet). |
| H9 | Per-machine PostgreSQL URL (a VM or ssh host that sees the server at another address). | **Deferred.** Until then such a machine refuses with the reach check's message naming the address it tried. |
| H10 | Converting a 2.0 home. | **Refuse while anything is live** (Sandboxes not closed, live leases, open intents), naming each; then convert in one pass under the home lock. Guest-side leftovers are deleted by the worker on its first start from the new bundle. |
| H11 | `project.init` into a sqlite Store on a non-native machine. | **Allowed through that machine's worker** (today refused, `family/projects.rs:154-163`). |

## 4. Facts at the base

At `58b8d1742b`, before #535. Re-check after #535.

- `layerfs-api/src` is about 19,080 lines: `host/` 7,184, `family/` 3,528,
  `calls/` 1,690, `home/` 1,515, `sandbox/` 1,141, `machine/` 1,085, `base/`
  913, `store/` 821, `worker/` 702, `fs/` + `branch/` + `project/` +
  `operator/` + `names.rs` + `lib.rs` about 500.
- The call table has 57 calls in 9 families (`calls/table.rs`). Front ends
  (`layerfs-cli`, `layerfs-node`, `layerfs-python`) call `calls::call` and see
  `{value} | {refused{code, message, detail}}`.
- A Sandbox is recorded in three places: `machine/<m>/sandbox/<name>/record.json`
  (2.0), `store/<s>/project/<p>/sandboxes/<n>/` (1.1-era `sandbox.record`,
  `workspace-<hex>.record`, `create.intent`, `*.unmount`; binary `Answer`
  codec; on worker roots for VMs) and the backend's directory
  (`machine/native/sandbox-root/` on native). `sandbox_of` scans every
  machine's folder per call, and the scan is repeated in
  `family/stores.rs` and `family/backups.rs`.
- Four intent mechanisms: Store `in_progress` (pid, start, boot), machine
  `removing` (pid only), Sandbox `state: "creating"` + `pid` and 1.1
  `create.intent`, and per-Workspace `.unmount` records. Plus
  `log/uncertain.jsonl`, written and never read.
- PostgreSQL Stores take no lease (`store/route.rs` passes `None`), so
  `store.disconnect`'s `InUse` check is always empty.
- Machine `record.state: "running"` is written but the live state is probed.
- The worker op `host.call` has no sender; `host.backend` has one
  (`family/projects.rs:315`, `runtime.store.remove_project`). Through them,
  `host/{sync,copies,local_stores,progress}.rs`, most of `host/discovery.rs`
  (990 lines), the 1.1 `runtime.*` and `backend.*` op names, most of
  `sandbox/` and `base/{internal,recovery}.rs` are reachable only from tests,
  examples or import-env.
- `home/layout.rs`: `LAYOUT_VERSION = 2`; there is no 2.x → 3 conversion path.
- The worker rewrites the whole Store record under its root on every
  `sandbox.host` (`family/sandboxes.rs` `in_worker`).

## 5. Home layout (layout 3)

Every record has `"v": 1`. Every file is 0600 and every folder 0700 unless
noted. "Live" means probed when asked, never stored.

```
<home>/
├── layerfs.json                      { layout: 3, id }
├── layerfs.lock                      home lock: name claims and cross-entity checks only
├── machine/<m>/
│   ├── record.json                   { v, name, platform, created, spec: { cpus, memory, disk, image?,
│   │                                   nested?, host?, ssh? }, vm_name, lasting?, error?, op? }
│   ├── worker.json                   { address, secret, forward_pid?, worker_version, started }
│   ├── start.lock                    start/create/remove serialization
│   └── known_hosts                   ssh only
├── store/<s>/
│   ├── record.json                   { v, name, kind, created, binding, cursor,
│   │                                   sqlite?:     { machine, sqlite_version, last_transfer? },
│   │                                   postgresql?: { url (no password), password_ref, schema, synchronous_commit },
│   │                                   backups: { <bucket>: { file, heads, last } }, op? }
│   ├── lock                          per-Store lock (record changes, leases)
│   ├── open/<pid>                    leases "<ms> <start> <boot>", BOTH kinds
│   ├── store.sqlite                  only while placed on native (+ .incoming, .backup, -wal, -shm transient)
│   └── project/<p>/record.json       { v, name, store, stack, created }
├── sandbox/<name>/
│   ├── record.json                   { v, name, machine, project: "<s>/<p>", backend, route, image?,
│   │                                   runtime_id?, created, op? }
│   ├── lock                          per-Sandbox lock
│   ├── control.json                  { endpoint, secret, peer, instance }   daemon keys (#535 G4)
│   ├── workspace/<id>.json           { v, id, name?, branch, path, created }
│   └── run/                          native only: config/daemon.setup, overlay/, workspaces/<id>/, tmp/, daemon.log
├── backup/<bucket>/record.json       { v, name, provider, …, key_ref }
├── log/                              uncertain.jsonl, migration.json, … (for people; nothing reads them)
└── trace/
```

**Derived, never stored:** machine → its sqlite Stores (scan `store/*`
`sqlite.machine`); machine → its Sandboxes and project → its Sandboxes (scan
`sandbox/*`; only `machine.remove`, `project.remove`, `store.remove` and
`doctor` need it); machine running/stopped; Store idle/in use/busy/down;
Sandbox running/gone; Workspace live status (from the daemon).

## 6. Machine-side layout

The worker's root. Guests never read the home; the caller sends each request
what it needs.

```
/layerfs/                             (docker: the worker container + volume <vm>-data)
├── bundle/                           layerfs (worker), layerfs-daemon, layerfs-sandbox-helper, sandbox-image.tar
├── image/                            this machine's own Sandbox image archive (optional)
├── state/                            worker.pid, secret, worker.log, ops.jsonl (recent op outcomes, read by status calls)
└── data/
    ├── store/<s>/
    │   ├── record.json               thin: { v, name, kind: "sqlite" }   (no copy of the home record)
    │   ├── open/<pid>
    │   └── store.sqlite (+ .incoming during a push or pull)
    ├── sandbox/<name>/
    │   ├── record.json               thin: { v, name, project, backend, runtime_id }
    │   ├── config/daemon.setup       (holds a PostgreSQL URL for the Sandbox's lifetime)
    │   ├── overlay/  workspaces/<id>/  tmp/  daemon.log
    └── firecracker/vms/<vm>/         a Lima host only
```

`sandbox/<name>/run/` in the home (native) and `/layerfs/data/sandbox/<name>/`
on every other machine are the same thing; one name identifies the Sandbox's
record, its directory, and its container or process (`layerfs-<home id>-<name>`).

## 7. States

### 7.1 Persisted facts and the one intent

```jsonc
"op": {
  "id": "op-<16 hex>",                       // minted by the caller, unique per attempt
  "name": "store.push",                       // store.push | store.pull | store.remove | store.restore | machine.create
                                              // | machine.connect | machine.remove | sandbox.create | sandbox.close
  "phase": "send",                            // the op's own phase names
  "args": { "to": "dev-lima" },               // only what status and doctor need
  "owner": { "pid": 4812, "pid_start": 98231, "pid_boot": "…" },
  "started": 1762160000000, "updated": 1762160004200,
  "uncertain": false                          // true once an outcome could not be known
}
```

- An operation writes `op` before its first effect, updates `phase` as it
  goes, and on success writes the result and removes `op` in one record
  write. A certain failure removes `op`. An uncertain one keeps it with
  `uncertain: true` and appends a line to `log/uncertain.jsonl` for people.
- A live owner blocks every other operation on that entity (`Busy`, naming
  the op). A dead owner's certain-failure intent is ignored; a dead owner's
  uncertain intent blocks with `Unresolved`, naming the status or get call
  that shows it and the call that clears it.
- This replaces Store `in_progress`, machine `removing`, Sandbox
  `creating`/`pid`, 1.1 `create.intent` and Workspace `.unmount` records.

### 7.2 Derived states

| Entity | Derived from | Values |
| --- | --- | --- |
| Machine | record + `op` + `error` + live probe | `creating`, `removing`, `failed`, `running`, `stopped`, `missing`, `unresolved` |
| Store | record + `op` + leases + worker answer | `idle`, `in_use`, `busy` (op), `down`, `unresolved` |
| Sandbox | record + `op` + daemon answer | `creating`, `closing`, `running`, `gone`, `unresolved` |
| Workspace | file exists + daemon answer | `mounted`, `lost` (file, no daemon), live status from the daemon |
| Project | record exists | — |

### 7.3 Push and pull (sqlite between native and a machine)

The same phases in both directions; only the two sides swap.

| | `store.push app dev-lima` | `store.pull app` |
| --- | --- | --- |
| allowed when | the Store is on `native` | the Store is on a machine |
| source → target | `native` (caller) → the machine's worker | the machine's worker → `native` (caller) |

```
check     store lock: kind sqlite, placement as above, no live leases, no Sandboxes,
          capacity: target free disk ≥ bytes + headroom → write op{store.push | store.pull, phase check}
snapshot  source side: refuse a non-empty WAL; { bytes, sha256, heads }
send      ONE worker connection, streamed frames (#535 R8) → target .incoming
verify    target: bytes, sha256, heads equal the snapshot's
install   target: rename .incoming → store.sqlite (never over a Store)
record    store lock: sqlite.machine = target, last_transfer, remove op   ← placement flips here
retire    source side: delete store.sqlite, -wal, -shm
```

A crash before `record` leaves the Store on the source (doctor reports the
target's `.incoming`); a crash at `record` is `uncertain`; after `record` the
target holds the Store and `retire` only cleans up. Between two machines the
caller runs `store.pull` then `store.push`; the Store passes through
`native` and each half is its own operation.

## 8. Locks

| Lock | Scope |
| --- | --- |
| `layerfs.lock` | claiming a name (machine, Store, project, Sandbox); cross-entity removal checks; layout conversion |
| `machine/<m>/start.lock` | start, create, remove of that machine |
| `store/<s>/lock` | that Store's record changes and lease grants |
| `sandbox/<name>/lock` | that Sandbox's record changes and its Workspace records |

Order: home → start → store → sandbox; never the reverse. `store.push` and `store.pull` take
the machine's start lock only to start it when it is stopped. No lock is held
across a worker call, a VM boot or a byte transfer: the `op` intent is what
keeps other operations out meanwhile. Leases are granted under the Store lock
only while the record has no `op`; `op` is written only when there are no
live leases. Workspace calls take no home lock (they go to the daemon; their
record is written after success under the Sandbox lock).

## 9. Structure

Reconsolidated 2026-11-03 by four structure reviews (api mapping, engine
Store calls, connect drivers, cross-cutting consistency). Every piece goes
to the layer it belongs to; dependencies point down only:
`layerfs-api → layerfs-control → connect drivers → engine → core`.

### 9.0 Layers and glossary

| Layer | Holds |
| --- | --- |
| core | algorithms (content, storage, history, telemetry) |
| engine | persistence (sqlite \| postgresql), store-service **with `methods/` (the Store calls)**, daemon, overlay, workspace, mount-ports, fuse \| nfs, bridge, project |
| connect | **drivers only**: `layerfs-machine`, `layerfs-sandbox`, `layerfs-store-backup`. They take every input as arguments, know nothing of the home and write no records |
| api | **`layerfs-control`** (the management library) and **`layerfs-api`** (the SDK call surface); the front ends `layerfs-cli`, `layerfs-node`, `layerfs-python` depend on `layerfs-api` only |

| Word | Means only |
| --- | --- |
| platform / machine | `Platform` is the kind (`Platform::Lima`); a machine is a named record of one |
| persistence backend / sandbox backend | a Store kind (`backend/{sqlite,postgresql}`) / an isolation driver (`backend/{docker,host_process,namespaces}`); never imported together |
| `KernelRoute` | FUSE or NFS (bridge). `Executor` (Local, Worker, Daemon) is where a call runs. `calls/route.rs` is call-table dispatch. `workspace::Route` stays engine-internal |
| control | the crate `layerfs-control`. The daemon channel module is `daemon/`; the daemon's own side stays `layerfs-daemon/src/control/` |
| worker protocol / worker bring-up | the protocol (`layerfs-control` `machine/worker/`) / installing and starting it in a guest (`layerfs-machine` `setup/worker.rs`) |
| `methods/` / `operator/` | store-service's SDK Store calls / its internal `backend.store.*` calls |
| `serve.rs` | the executing side of an entity: runs in the caller (Local) or in a worker |

### 9.1 `api/crates/layerfs-api` (the SDK surface, about 2,000 lines)

```
src/
├── lib.rs                 re-exports for the front ends; doctor()
└── calls/                 table.rs  check.rs  scope.rs  json.rs  text.rs  keys.rs  dispatch.rs  route.rs
                           route.rs: one line per call → layerfs_control::<entity>::<op>
```

`Context` wraps control's `Events`, so the wire shapes (`{progress}`,
`{stream, bytes}`) and the bindings do not change. Dependencies:
`layerfs-control`, `layerfs-bridge`; features `machine`, `postgresql`, `tls`
pass through.

### 9.2 `api/crates/layerfs-control` (the management library), by entity

```
src/
├── lib.rs                 declarations and re-exports only
├── common/                what every entity uses (named common/, not core/, to avoid the layer name)
│   ├── names.rs           typed ids: MachineName StoreName ProjectName ProjectRef SandboxName
│   │                      WorkspaceId WorkspaceName BucketName OpId            ← names.rs
│   ├── outcome.rs         Refusal {code,message,entity,phase,related,detail}, Event, Outcome   ← host/outcome.rs
│   ├── op.rs              the one in-flight intent: begin / phase / end / uncertain / owner liveness
│   ├── place.rs           plan(): Store kind + machine → Executor {Local | Worker | Daemon}   ← store/route.rs (part)
│   ├── secrets.rs         password refs, worker secrets, daemon keys   ← family/postgres.rs url(), machine/record.rs
│   ├── trace.rs           the call trace   ← host/trace.rs
│   └── home/              layout.rs paths.rs records.rs (versioned codec) lock.rs (home + entity locks)
│                          lease.rs (← store/lease.rs) guest.rs import_env.rs
│                          migrate/ mod.rs v110.rs (← migrate.rs + place.rs) v2.rs (layout 2 → 3)
├── machine/               record.rs state.rs create.rs connect.rs power.rs remove.rs update.rs status.rs
│   │                      get.rs list.rs (← machine/calls.rs split per op) bundle.rs endpoint.rs
│   └── worker/            frame.rs client.rs server.rs idle.rs   ← worker/ (server loses host.call, host.backend)
├── store/                 record.rs (StoreRef) state.rs create.rs connect.rs (← family/postgres.rs)
│   │                      remove.rs status.rs get.rs list.rs   (← family/stores.rs)
│   │                      resolve.rs call.rs (← store/route.rs)   calls.rs (← family/files.rs: file/branch/
│   │                      commit argument shaping → engine methods)   identity.rs (← host/stores.rs, live half)
│   │                      open.rs (← store/open.rs)   service.rs (← host/direct.rs: StoreService registry)
│   └── transfer/          mod.rs (the phases) push.rs pull.rs steps.rs (← store/files.rs; runs on the
│                          receiving side) stream.rs
├── project/               record.rs init.rs (← family/projects.rs, host/postgres_init.rs, host/project.rs helpers)
│                          remove.rs (worker op project.remove replaces host.backend) list.rs get.rs
├── sandbox/               record.rs (+ control.json) state.rs create.rs close.rs get.rs list.rs
│                          route.rs (caller side: on_machine) serve.rs (executing side ← host/sandbox.rs,
│                          host/dispatch.rs public table) launch.rs (← host/local.rs) runtime.rs (← host/runtime.rs)
│                          deploy.rs (← family/deploy.rs, both sides) exec.rs output.rs live.rs files.rs
│                          (live half of host/discovery.rs)
├── workspace/             record.rs state.rs mount.rs exec.rs commit.rs unmount.rs status.rs
│                          serve.rs (← host/workspace.rs, live half) unmount_guard.rs (← host/unmount.rs)
├── backup/                bucket.rs (bucket records ← family/backups.rs, store-service remote/settings.rs codec)
│                          create.rs restore.rs list.rs link.rs (← remote/state.rs: per-Store links, now Store
│                          record fields)
├── daemon/                channel.rs (← base/control.rs) pool.rs (← host/pool.rs) call.rs (← base/sdk.rs,
│   │                      operator/call.rs) failure.rs (← base/failure.rs) values.rs (← sandbox/values.rs)
│   └── workspace/         mount.rs exec.rs commit.rs status.rs unmount.rs   ← sandbox/{mount,exec,commit,status,unmount}.rs
├── inventory.rs           by-name reads; derived relationships; can_remove / can_push / can_pull / can_close
└── doctor.rs              layerfs doctor (report-only)
```

Largest files stay under 999 lines (about 560 for `sandbox/serve.rs`, 450
for `sandbox/files.rs` and `sandbox/create.rs`, 400 for `project/init.rs`);
every `mod.rs` is declarations only. Dependencies: `layerfs-bridge`,
`layerfs-store-service`, `layerfs-persistence`, `layerfs-project`,
`layerfs-sandbox`, `layerfs-store-backup`, `layerfs-machine` (optional,
feature `machine`), core crates, `nix`, `libc`. The guest worker
(`layerfs-cli --no-default-features`) builds it without `layerfs-machine`.

Worker ops: `worker.hello` (protocol version and bundle hash; a mismatch is
refused before anything is sent), `store.call`, `store.file`, `sandbox.host`
(create, close, get), `project.remove`, `op.outcome` (internal: what the
worker knows about an op id). `host.call` and `host.backend` are deleted.

Deleted with the 1.1 paths (each in its own commit, listing its tests):
`host/{sync,copies,local_stores,progress}.rs`; the dead parts of
`host/{discovery,dispatch,exec,postgres,project,sandbox,stores,workspace}.rs`
(`runtime.*`, `backend.*`, `get_log`, `reclaim`, `processes`, `cancel`);
`base/{internal,recovery}.rs`; `operator/inspect/`; `examples/issue367_rm_probe.rs`;
the `sandbox/{provider,create,close}.rs` facade with `tests/{sandbox_operations,
provider_ownership}.rs` and `examples/sandbox_*.rs` unless kept as
`layerfs-control/examples/`.

### 9.3 Engine: the Store calls in `layerfs-store-service/src/methods/`

They depend only on store-service, bridge, history and content (verified:
`Answer`, `Arg`, `invalid` already live in store-service `history/lookup.rs`;
`paging`, `text` in `operator/args.rs`), so the move is about 600 lines and
needs no new dependency edge.

```
engine/crates/layerfs-store-service/src/
├── methods/               the SDK Store-call facade (SdkCall → Answer)
│   ├── mod.rs             declarations; pub fn call
│   ├── dispatch.rs        ← layerfs-api base/dispatch.rs   the method table + message check
│   ├── args.rs            ← layerfs-api base/args.rs
│   ├── file/              ← layerfs-api fs/   one file per call (read, stat, list, glob, grep, blame,
│   │                         diff_files, write, edit, rm, mkdir, mv, cp, apply)
│   ├── branch/            ← layerfs-api branch/   fork, list_commits, merge, publish, diff_commits, diff_branches
│   └── project.rs         ← layerfs-api project/list_branches.rs
├── operator/              ·  backend.store.* facade (unchanged)
└── read/ write/ history/ store/ overlay/ service/ …   ·
```

The crate description and `lib.rs` doc change from "knows no SDK method
names" to "`read`, `write` and `history` know no SDK method names;
`methods/` is the SDK facade". About 50 test files that drive the method
table (`sdk_*`, `merge_*`, `diff_*`, `replay_merges`, `write_cost`,
`write_refusals`, `history_*_oracle`, `commit_get_pages`, `concurrent_commits`,
`lane_merge`, `fs_mode`, `stat_symlink`, …) with `tests/support/{sdk,
installed_store,view}.rs` move to `layerfs-store-service/tests/`; dev-deps
`layerfs-project`, `rusqlite`, `blake3`.

### 9.4 Connect: what this issue changes in the drivers

#535 builds the three driver crates. This issue adds three corrections:

- **`layerfs-store-backup` loses its home records.** store-service's
  `remote/settings.rs` (the `remotes` file) and `remote/state.rs` (per-Store
  links) read and write home files, so they move to `layerfs-control`
  `backup/bucket.rs` and `backup/link.rs` (link state becomes Store record
  fields, §5). The driver keeps `Driver`, providers, pieces, credentials
  references, object naming and the create/restore/heads/copies steps.
- **`Driver` becomes sealed** (`pub trait Driver: sealed::Sealed`), the one
  public provider trait left in connect (#535 G2).
- **The worker readiness probe stays in `layerfs-machine`.** `setup/worker.rs`
  keeps its 25-line hello probe (it checks the port it just launched);
  `layerfs-machine` may not depend on `layerfs-control`. Its frame bound is
  kept equal to control's `MAX_HELLO`.

### 9.5 Tests

| Crate | Tests |
| --- | --- |
| `layerfs-api` | `calls_table`, `calls_scope`, `method_refusal_order` (argument checks only) |
| `layerfs-control` | `home_*`, `store_*`, `project_family`, `sandbox_*`, `machine_*`, `backup_*`, `postgres_*`, `worker_*`, `mount_control`, `observed_*`, `kernel_*`, `r5_1_actual_daemon_commit`, `unmount_dirty`, `refresh_lookups`, settings codec tests (← store-service `remote_settings.rs`), `support/` (daemon binary, fixtures) |
| `layerfs-store-service` | the method-table tests of §9.3 |
| `layerfs-store-backup` | `folder_pieces`, `keep_alive`, `minio`, `progress_listing`, `copies` |
| deleted | `local_root_modes`, `remote_store` and every test arm that drives `host.call`, `host.backend`, `runtime.*` or `backend.*` |

### 9.6 Guard, tools, CI and repo docs

- `tools/check_product_boundary.py`: `layerfs-control` → `API`;
  `ALLOWED_DEPENDENCIES["layerfs-api"] = {layerfs-control, layerfs-bridge}`;
  `layerfs-control` gets the edges of §9.2; a rule that `layerfs-api` reaches
  records, machines and Stores only through `layerfs-control`; retired paths
  added for `layerfs-api/src/{host,family,home,store,worker,fs,branch,machine,
  sandbox,base,operator,project}/` and `layerfs-store-service/src/remote/`;
  unsafe and forbid lists include `layerfs-control`.
- `tools/production_loc.py`: `layerfs-control` counted under `api`.
- `tools/ci_tests.py`: SLOW and mount entries for moved tests change package
  from `layerfs-api` to `layerfs-control` or `layerfs-store-service`;
  `.github/workflows/nightly.yml` mount lane `-p layerfs-control`.
- `api/node/src/index.js` and `api/python/src/layerfs/__init__.py`: one
  top-level `doctor` export each.
- `api/AGENTS.md`: add `crates/layerfs-control` ("the management library:
  home records, typed records, op intents, locks, placement, worker protocol,
  daemon channel"); `crates/layerfs-api` becomes "the SDK call surface: call
  table, checks, scopes, JSON codec; one line per call into
  `layerfs-control`". `engine/AGENTS.md`: store-service owns `methods/`.
  `connect/AGENTS.md`: drivers write no records. This amends #533 O21a's
  wording while keeping its intent (one library, no `layerfs-sdk` crate).

### 9.7 Shape of an operation

```rust
pub fn create(home: &Home, req: sandbox::Create, events: &mut Events) -> Result<sandbox::Record, Refusal>;
//   Create { name: SandboxName, project: ProjectRef, machine: Option<MachineName>, image: Option<String> }
//   1. home lock: claim sandbox/<name>/, write record with op{sandbox.create}
//   2. place::plan(home, &store_ref, machine) → Plan (machine defaults: sqlite → placement, PostgreSQL → native)
//   3. executor.run: sandbox::serve → sandbox::launch (here or in the worker), PostgreSQL reach check inside
//   4. sandbox lock: write runtime_id, control.json; remove op
```

Each typed record has one `to_sdk(View)` (full or list row) and one
`from_record`, so the call table never re-encodes fields by hand.

## 10. SDK and CLI

The CLI, the Node package (`api/node`) and the Python package (`api/python`)
are generated from the call table. After #535 and this issue: **9 families,
61 calls** (today's 57, plus #535's `machine.platforms`, `machine.start`,
`machine.stop`, and `store.pull`), plus the top-level
`layerfs doctor` (CLI, Node, Python) and `layerfs import-env` (CLI only, as
today). `store.move` is the only call removed, replaced by `store.push` and `store.pull`.

Legend: ★ new · ✎ changed · blank unchanged. Arguments with `?` are optional.

| | Call | Arguments | Options | Change |
| --- | --- | --- | --- | --- |
| ✎ | `project.init` | `project`, `folder` | `timeout`, `on_progress` | allowed into a sqlite Store on any machine, through its worker (H11) |
| | `project.list` | `store?` | page | |
| ✎ | `project.get` | `project` | | result adds `sandboxes` (derived); `machine` only for sqlite |
| | `project.remove` | `project` | | `InUse` while Sandboxes of it run |
| | `branch.list` / `get` / `fork` / `merge` / `publish` / `diff` | as today | as today | |
| | `commit.list` / `get` / `diff` | as today | as today | |
| | `file.read` / `stat` / `list` / `glob` / `grep` / `blame` / `diff` / `write` / `edit` / `rm` / `mkdir` / `mv` / `cp` / `apply` | as today | as today | |
| ✎ | `sandbox.create` | `project`, `name`, `machine?` | `image` | `machine` optional: a sqlite Store's machine, or `native` for PostgreSQL; PostgreSQL on every supported machine (#535) with the reach check inside |
| ✎ | `sandbox.list` | `project?` | page | one read per Sandbox, no scan; each item adds `state`, `op`, `backend`, `route` |
| ✎ | `sandbox.get` | `name` | page | result adds `state`, `op` (H6), `backend`, `route`, Workspace `name`s |
| | `sandbox.close` | `name` | `force` | also clears an uncertain create |
| ✎ | `workspace.mount` | `sandbox`, `branch` | `name` | optional name, unique within the Sandbox |
| ✎ | `workspace.exec` / `commit` / `unmount` / `status` | `ws` | as today | `ws` is `<sandbox>:<path>` or `<sandbox>:<name>`; straight to the daemon |
| ✎ | `machine.list` | | | each machine adds `stores`, `sandboxes` (derived) and `state` |
| | `machine.get` | `name` | | |
| ✎ | `machine.status` | `name` | | adds `op` (H6) |
| ✎ | `machine.create` | `name`, `platform` | declared by the platform (`cpus`, `memory`, `disk`, `image`, `nested`, `host`) | an option the platform does not take is refused (#535) |
| ✎ | `machine.connect` | `name`, `host` | `user`, `port`, `identity`, `known_hosts`, `root` | declared by the ssh platform (#535) |
| | `machine.remove` / `disconnect` | `name` | | `InUse` lists the sqlite Stores and Sandboxes on it |
| | `machine.update` | `name` | `cpus`, `memory`, `image`, `docker`, `wsl` | |
| ★ | `machine.platforms` | | | #535: the support matrix for this host and build (replaces `can_create`) |
| ★ | `machine.start` / `machine.stop` | `name` | | #535: explicit power for UIs; `stop` refused `InUse` while Sandboxes run |
| ✎ | `store.list` | | | each Store adds `state` and `op` |
| | `store.get` | `name` | | result keys unchanged (top-level `machine` for sqlite) |
| ✎ | `store.status` | `name` | | adds `op` (H6); PostgreSQL Stores report leases |
| | `store.create` | `name` | | an empty sqlite Store on `native` |
| ✎ | `store.connect` | `name`, `url` | `password`, `synchronous_commit` | usable from every machine (C4) |
| ✎ | **`store.push`** | `name`, `machine` | `on_progress` | **replaces `store.move`** (H1): `native` → `machine`; refused unless the Store is on `native`; free-disk check first; streamed over one connection |
| ★ | **`store.pull`** | `name` | `on_progress` | its machine → `native` (H1); refused when already on `native`; same phases and checks |
| | `store.remove` | `name` | | |
| | `store.disconnect` | `name` | | `InUse` now correct while a PostgreSQL Store is open |
| | `backup.connect` / `disconnect` / `list` / `create` / `restore` | as today | as today | |
| ★ | `layerfs doctor` (top level) | | `--text` | report-only (H7): `{ ok, findings: [{ severity, entity, message, fix }] }`; Node and Python `layerfs.doctor()` |

Rules for every call:

- **Result keys stay stable.** Record layout 3 is internal: every call result
  keeps today's keys (for example a top-level `machine` on a sqlite Store,
  `runtime: "native"`), with only additions (`state`, `op`, Workspace
  `name`, derived lists). Each typed record's `to_sdk` produces today's shape.
- **Refusals** keep `{code, message, detail}`; `detail` carries
  `{entity: {kind, name}, phase?, related: [...]}` when known; new codes
  `Busy` (an operation is running on the entity) and `Unresolved` (an earlier
  operation is uncertain; the message names the call that shows it and the
  call that clears it).
- **Events** keep their wire shapes (`{progress}`, `{stream, bytes}`).
- **First call on a 2.0 home** under the new build: refused `InUse` naming
  each running Sandbox until it is closed; then the home converts once (H10).

The `store.move` → `store.push` + `store.pull` commit updates, together: the call table,
`docs/versioned/2.0.0/{calls,sdk,machines-and-stores,home,limitations}.md`,
`.agents/skills/layerfs-js-sdk/references/methods/{store,project,sandbox}.md`,
`api/node/test/events.mjs`, `api/python/test/test_calls.py`,
`api/node/test/verify/cells/cell.mjs` and the Rust tests that name it. The
`doctor` commit adds one top-level export to `api/node/src/index.js` and to
`api/python/src/layerfs/__init__.py`, and a line to `layerfs help`.

## 10b. Contract for UIs

What a UI (LayerFS Connect, #537) may rely on.

**Lists carry state.** `machine.list`, `store.list` and `sandbox.list` items
carry `state` and `op` (when present), so a page needs no get or status call
per item. `machine.list` also carries `supports` (#535 §9a).

**No call starts a machine except those that need its worker.** Never start
one: every `*.list`, `*.get`, `*.status`, `machine.platforms`, `layerfs
doctor`. Start one on demand (programmatic callers): Store calls on a sqlite
Store on that machine (`file.*`, `branch.*`, `commit.*`, `project.list`
branches, `project.init`, `project.remove`), `sandbox.create`, `store.push`,
`store.pull`, `backup.create` (it snapshots on the Store's machine).
`backup.restore` installs on `native` and starts nothing. UIs call
`machine.start` first and never rely on on-demand start.

**Calls by Store state.**

| Store state | Reads from records (`store.get`, `project.list`, `project.get`) | Store calls (`file.*`, `branch.*`, `commit.*`) | `sandbox.create`, `project.init`, `project.remove` | `store.push` / `store.pull` / `store.remove` / `backup.restore` |
| --- | --- | --- | --- | --- |
| `idle` | answer | answer | allowed | allowed |
| `in_use` | answer | answer | allowed | refused `InUse` (`related`: lease holders, Sandboxes) |
| `busy` | answer | answer from the source until the record step, then from the target | refused `Busy` (naming the op) | refused `Busy` |
| `down` | answer | start the machine (or refused `Unavailable` when the start fails) | start the machine | start the machine |
| `unresolved` | answer | answer where the placement is | refused `Unresolved` | refused `Unresolved`, except the clearing call the message names |

**Push and pull eligibility.** Refused while any Sandbox **record** of the
Store exists (running or not) or any live lease is held; `related` lists them.

**Progress.** During `store.push`, `store.pull`, `backup.create`,
`backup.restore` and `project.init`, events are
`{ progress: { phase, done, total } }`; `done`/`total` are bytes during
`send` (and the backup transfer), and `null` in other phases. Phase names:
`check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire`.

**Workspace handle.** `sandbox.get` `workspaces[]` items carry `ws`, which
is `<sandbox>:<name>` when the Workspace has a name and `<sandbox>:<path>`
otherwise, plus both `name` and `path`. Every `workspace.*` call accepts
either form.

**Refusal detail.** `InUse` from `machine.remove`, `machine.stop`,
`store.remove`, `store.push`, `store.pull`, `project.remove` and
`store.disconnect` always fills `detail.related` with
`{kind, name}` entries (Stores, Sandboxes, leases).

**PostgreSQL removal.** `store.remove` on a PostgreSQL Store is refused
`InvalidInput` naming `store.disconnect` (its data stays in the server).

**Deferred:** a home-wide watch or event stream (H8); UIs poll the list
calls.

## 10a. Management before and after

| Concern | Before (`58b8d1742b`) | After (#535 + this issue) |
| --- | --- | --- |
| Who decides, records and executes | `layerfs-api` (~19k lines): `family/`, 1.1-era `host/`, `store/`, `machine/`, `sandbox/`, `worker/`, `home/`, plus the Store calls | `api/crates/layerfs-control` by entity; Store calls in engine `store-service/methods/`; `layerfs-api` keeps the call table (~2k lines) |
| What exists and how it relates | scans spread over `family/`, `machine/record.rs`, `stores.rs`, `backups.rs` | `inventory` |
| A Sandbox's records | three places, two identities | `sandbox/<name>/` only |
| Operations in flight | four mechanisms, three liveness checks | one `op` field, one owner identity |
| Where a call runs | `store/route.rs`, `family/sandboxes.rs`, duplicated `deploy.rs` resolvers | `place/` |
| Executing, here or in a worker | split between `family/` and `host/` | each entity's `serve.rs` / `launch.rs` / `transfer/steps.rs` |
| Platforms and backends | `machine/kind.rs` + `setup.rs` matches; `runtime.rs` | `layerfs-machine` / `layerfs-sandbox` folders, private traits (#535) |
| Locks | one home lock for everything; leases for sqlite only | home lock for names; machine → store → sandbox locks; leases for both kinds |
| Secrets | four places | `secrets` |
| A Sandbox call on a VM | scan all Sandbox folders, ping + call, worker rewrites the Store record | one read, one worker connection, thin guest record; Workspace calls go to the daemon |

## 11. Work order

> Interleaved with #535 (owner, 2026-11-03): the order actually followed is
> [535/HANDOFF-2.md](../535/HANDOFF-2.md) §5, with one combined terminal pass (§6 there).

Starts from #535's final commit. Moves and behaviour changes never share a
commit; every commit carries `Production LOC: <before> -> <after> (delta …)`
and passes #535 §11's checks.

0. **Docs.** DECISIONS.md (C1–C7, gate answers); `connect/AGENTS.md` and
   `api/AGENTS.md` updated for the new crate.
1. **Moves (delta 0)**, in this order so every commit builds (each with
   its guard, LOC-counter, CI and `Cargo.toml` changes):
   1. `fs/`, `branch/`, `project/list_branches.rs`, `base/{dispatch,args}.rs`
      → `engine/crates/layerfs-store-service/src/methods/` (§9.3), with the
      method-table tests; `layerfs-api` calls `methods::call`.
   2. `api/crates/layerfs-control` created; `home/` → `common/home/`;
      `names.rs`, `host/{outcome,trace}.rs` → `common/`.
   3. `worker/` → `machine/worker/`; `machine/` → `machine/`;
      `layerfs-cli/src/worker.rs` repointed.
   4. `store/`, `family/{stores,postgres,files}.rs`, live `host/{direct,stores}.rs`
      → `store/`.
   5. `family/{projects}.rs`, `host/{project,postgres_init}.rs` (live parts)
      → `project/`; `family/backups.rs`, store-service `remote/{settings,state}.rs`
      → `backup/` (§9.4).
   6. `family/{sandboxes,deploy}.rs`, live `host/{sandbox,local,runtime,exec,
      output,registry,discovery,dispatch,workspace,unmount,pool}.rs`,
      `base/{control,sdk,failure}.rs`, `operator/call.rs`, `sandbox/*`
      → `sandbox/`, `workspace/`, `daemon/` (§9.2).
   7. `layerfs-api` keeps `calls/` and `lib.rs`; its tests are split per §9.5.
2. **Deletions** (one commit per group, each listing the tests it deletes):
   `host.call`/`host.backend` → typed `project.remove`; the 1.1 `runtime.*`
   and `backend.*` op tables and what only they reach (`host/sync.rs`,
   `copies.rs`, `local_stores.rs`, `progress.rs`, the unreachable parts of
   `discovery.rs`, `sandbox/` diagnostics, `base/{internal,recovery}.rs`).
   The 57-call surface is unchanged.
3. **Typed model.** `common/names.rs` ids, each entity's `record.rs` with `v`, `to_sdk` and
   `from_record`, `state()` functions, `inventory.rs`; all `get_text`/`with_field`
   record access in control goes through them.
4. **Layout 3.** `sandbox/<name>/`, Workspace records, thin guest records,
   native `run/`; the three Sandbox scans deleted; `migrate/v2.rs`
   (refuse while live, then convert); the worker deletes leftover numbered
   folders on first start from the new bundle.
5. **Intents and locks.** `common/op.rs`; Store, machine, Sandbox, push and pull
   use it; `uncertain` handling unified; per-entity locks; PostgreSQL leases;
   pid-only liveness checks removed.
6. **Calls.** §10: `store.push` and `store.pull`, optional `machine`, Workspace names,
   `project.init` through a worker, structured refusal `detail`, typed events.
7. **Safety.** `secrets.rs`; capacity checks in `store.push`, `store.pull` and
   `sandbox.create` (16 Workspaces, free disk); worker hello version and
   bundle hash.
8. **Observability.** `op` in `machine.status`, `store.status` and
   `sandbox.get` (H6; the worker's `state/ops.jsonl` keeps recent outcomes,
   read through the internal `op.outcome`); top-level `layerfs doctor` and
   `layerfs.doctor()` (H7).
9. **Conformance kit.** One test target in `layerfs-machine` and one in
   `layerfs-sandbox` that every platform adapter and backend passes with the
   stand-in tools (lifecycle, refusals, `reach`, push and pull steps); real platforms
   run it in the terminal pass.
10. **Terminal pass** (§12).

## 12. Terminal pass

On one product commit and one sealed Linux bundle, Disposable Stores only,
recorded in `TERMINAL-PASS.md` with receipts under `receipts/<commit>/`. A
NOT_RUN or unexpected refusal fails it, except Windows, Durable and R2
without an account.

1. #535's terminal pass items 1–8 still pass (sqlite and PostgreSQL cells on
   every platform, many Stores per machine, refusals, contracts, roundtrips,
   binding parity, checks), using `store.push` and `store.pull`.
2. **Conversion:** a 2.0 home with closed Sandboxes on `native` and one VM
   converts to layout 3; a home with a running Sandbox is refused naming it;
   after conversion the 12 cells pass on both machines.
3. **Intents:** a `store.push`, a `store.pull` and a `sandbox.create` killed at each phase
   leave the records `doctor` predicts; `store.status`, `sandbox.get` and
   `layerfs doctor` report them without changing anything; the documented
   explicit call clears each.
4. **Locks:** two processes creating Sandboxes of different projects at once
   do not wait on each other; two processes pushing the same Store are refused
   `Busy`.
5. **Layout:** no Sandbox record outside `sandbox/<name>/`; no
   `store/*/project/*/sandboxes/` on any machine; `doctor` reports nothing
   on the final home.
6. **Size:** `layerfs-api/src` contains only `calls/` and `lib.rs`; the
   commit table shows the deleted 1.1 paths.
7. **Checks:** #535 §11 on the final commit.

## 13. Risks

- **Conversion refusal is strict:** users must close Sandboxes before the
  first call under the new build. The refusal names each one and the call to
  close it.
- **Deleting 1.1 paths:** some tests drive `runtime.*` op names; they are
  deleted with the paths, listed in each commit. The 12 cells are the
  behaviour check.
- **Two-step guest cleanup:** a VM whose worker never restarts keeps its old
  numbered folders; `doctor` reports them.
- **PostgreSQL behind a different address on one machine** (H9) is refused
  until a per-machine URL is designed.

## 14. Not in this issue

A `home` family; labels, a home-wide status call, idle policies per machine
or Sandbox (H8); a per-machine PostgreSQL URL (H9); desired-state files,
automatic placement, push or pull resume, a long-lived per-user control process (it
must never sit in the data path).

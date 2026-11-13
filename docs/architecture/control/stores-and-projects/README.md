# Stores and projects

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs-store` keeps each Store's and project's record in the home,
decides from those records where every Store call runs, holds the Stores
this process has open, and moves or removes a sqlite Store's file.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-store` (everything but `backup/`); `layerfs-control` [`store.rs`](../../../../control/crates/layerfs-control/src/store.rs) and [`project.rs`](../../../../control/crates/layerfs-control/src/project.rs) add the checks that read Sandbox records |
| Source | [`layerfs-store/src`](../../../../control/crates/layerfs-store/src) |
| Uses | [Home](../home/README.md), [Machines: workers](../machines/workers-and-reach.md), [Store calls](../../engine/store-calls/README.md), [Store](../../engine/store/README.md), [Project Init](../../engine/project-init/README.md) |
| Used by | [Sandboxes](../sandboxes/README.md), [Backups](../backups/README.md), [Doctor](../doctor/README.md), [API: store](../../api/sdk/store.md), [branch](../../api/sdk/branch.md), [commit](../../api/sdk/commit.md), [file](../../api/sdk/file.md), [project](../../api/sdk/project.md) |
| Flows | [Call](../../flows/call.md), [Init](../../flows/init.md), [Commit](../../flows/commit.md) |

## Picture

```text
 branch/commit/file call (project "<store>/<project>")
      |
 resolve::target     store/<s>/record.json + project record
      |              (NoSuchStore, NoSuchProject)
 store_call          record.unblocked()  (Busy, Unresolved)
                     lease::claim under store/<s>/lock
      |
      +-- StoreRef::Postgres --> this process: URL from the record,
      |                          password reference resolved now
      |
      +-- StoreRef::Sqlite --> place::plan(machine)
              |                         |
        Executor::Local          Executor::Worker
        open::run on             worker op "store.call" {op, args,
        store/<s>/store.sqlite   store, project, store_record};
        (open-Store cache)       worker opens <root>/store/<s>/store.sqlite
              |                  and holds the lease there
              v
        StoreService (engine) behind the kernel guard
```

The route is decided once per call from the Store's record, read under
its lock ([`call.rs`](../../../../control/crates/layerfs-store/src/call.rs)).
`plan` reads no worker record and dials nothing
([`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)). The
full per-call table is "Where each call runs" in [Control](../README.md).

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Store record | `store/<name>/record.json`, read once into `StoreRecord` with its parsed `StoreRef`; nobody reads a record's `kind` text or fields except through it; `v` is read past and never shown | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) |
| Kind | no `kind` is sqlite; `sqlite` or `postgresql`; any other text `InvalidInput` (only the step that needs the kind refuses) | same, `kind_of` |
| Placement | a sqlite record names its `machine` (none is `native`); a 1.1.0 Store whose machine is unknown is `Unavailable`, naming `layerfs import-env`; a PostgreSQL Store is on no machine | same, `StoreRef::machine` |
| New sqlite Store | `store.create`: an empty Store on `native`, record `{machine: native, profile: disposable, created, sqlite}` | [`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs), `record.rs` `created_on_native` |
| Resolve | `<store>/<project>` split and both records read; a bare name `InvalidInput` | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs) |
| Store calls | `branch`, `commit`, `file` (and project stats): each call translated once to one engine method (snake_case to camelCase, absent options not sent) and run on its route | [`calls/`](../../../../control/crates/layerfs-store/src/calls/mod.rs), [`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs) |
| Placement plan | PostgreSQL: this process. sqlite: this process when the machine's platform executor is `Local` on this host (`native` on macOS and Linux); else that machine's worker (`docker` on every host) | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| Leases | every process using a Store holds `store/<s>/open/<pid>`: the opener, or for a worker Store the host process sending calls; one file per process, removed by its last holder | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |
| Open-Store cache | one open Store per path per process, shared by the caller's route and a worker; closed by a reaper after 60 s unused, never while a call runs | [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs) |
| WAL checkpoint | when the last call on a SQLite Store returns, or a Sandbox on it closes, its WAL is checkpointed and truncated once while the Store stays open; one that cannot complete waits for the idle close | same |
| Store service | one engine `StoreService` per Store path per process, opened by the first client and closed by the last; its private overlays live under a random name in the temp directory | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs) |
| State | `store.list`, `status`: `idle`, `in_use` (another live lease), `busy` (live `op`), `down` (machine did not answer), `unresolved`; reads take no lock and leave stale leases | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) `state`, [`status.rs`](../../../../control/crates/layerfs-store/src/status.rs), [`list.rs`](../../../../control/crates/layerfs-store/src/list.rs) |
| Stats, diagnostics | Store-wide counters on the ordinary route; read-only engine entry points without leases or writable opens | [`stats.rs`](../../../../control/crates/layerfs-store/src/stats.rs), [`diagnostic.rs`](../../../../control/crates/layerfs-store/src/diagnostic.rs) |
| Who uses a Store | `store_idle`, `can_remove_project` read Sandbox records in `layerfs-control` and are passed in as `Related` | [`related.rs`](../../../../control/crates/layerfs-store/src/related.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |

### Store family

| Call | Kind | Does | Source |
| --- | --- | --- | --- |
| `list`, `get`, `status`, `stats` | both | records, leases, size where it lives | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) |
| `create` | sqlite | empty Store on `native`; `Exists` if named | [`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs) |
| `push`, `pull` | sqlite | move the file between `native` and a machine | [SQLite and PostgreSQL](sqlite-and-postgresql.md#transfer-sqlitetransfer) |
| `remove` | sqlite | delete the file wherever it is, with its projects; PostgreSQL `InvalidInput` | [`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs) |
| `connect`, `disconnect`, `attach`, `detach` | PostgreSQL | record or forget a database; record or forget a machine that may run its Sandboxes | [SQLite and PostgreSQL](sqlite-and-postgresql.md#postgresql-postgresql) |

### Multi-step operations

```text
 home lock + Store lock : check leases (InUse), write op phase 1   short
 no lock                : the work (worker steps, byte transfer)  long
 home lock + Store lock : write result, drop op                   short
 lost answer            : keep op uncertain, log/uncertain.jsonl, Unknown
```

A live `op` refuses every lease, Store call and new operation (`Busy`),
this process included. A dead owner's uncertain `op` is `Unresolved`; its
refusal names the clearing call: a new `store push <s> <m>` or `store pull
<s>` for a transfer, else `store remove <s>`
([`record.rs`](../../../../control/crates/layerfs-store/src/record.rs)
`unblocked`, `clearing_call`).

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every decision on a Store's kind matches on `StoreRef` or `StoreKind`; `sqlite/` and `postgresql/` never import each other | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs); test `kind_isolation` |
| A lease is taken under the Store's lock after the record shows no `op`, so a lease and a push, pull or remove never overlap | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) `claim` |
| A lease names its process by pid, kernel start time and boot id; a stale one is removed under the Store lock by the next claim or by push, pull, remove, disconnect, and logged to `log/leases.jsonl`; reads ignore it | same |
| The Store lock is taken after the home lock, never before, and never held across a worker call or byte transfer | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) `lock` |
| No call starts a machine: a worker not recorded or not answering is `Unavailable`, naming `layerfs machine start` | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| A guest never reads the home: the caller sends the Store's record with each call; the worker finds the file under its own root by name | [`call.rs`](../../../../control/crates/layerfs-store/src/call.rs), [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Direct Store access only where every opener shares one kernel and a local filesystem: Linux ext4, xfs, btrfs; macOS apfs, hfs; else every Store call is `Unsupported` with its reason, no fallback | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| One attempt per call; nothing is resent | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| Store records are created with `profile: disposable` (Disposable/WAL/synchronous=OFF, the only permitted profile); Durable execution is `NOT_RUN — disabled by owner until explicit reauthorization` | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) `created_on_native`, `restored` |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| No Store / no project | `NoSuchStore` / `NoSuchProject` from `resolve`; `NotFound` from the Store family | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs), [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) |
| Live / dead uncertain `op` | `Busy` / `Unresolved` | `record.rs` `unblocked` |
| Store lock held | `Unavailable` after 120 s | [Home](../home/README.md#locks-and-leases) |
| Leased or used by a Sandbox (push, pull, remove, disconnect, restore) | `InUse`, `detail.related` lists `{kind: "lease", name: <pid>}` and `{kind: "sandbox", name}` | [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle` |
| Open-Store idle close | 60 s (`IDLE`); a Store with a call running is never closed (`close` refuses `InUse`) | [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs) |
| Store kernel guard | `Unsupported`, naming the filesystem | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Store file missing | `NoStore` | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| `layerfs-api/tests/store_routes.rs` | `cargo +1.85.1 test --locked -p layerfs-api --test store_routes` | Linux (ext4, xfs, btrfs), macOS (apfs) | a call goes where its records say, a worker gets the records, leases are held, read and cleaned |
| `layerfs-api/tests/store_family.rs` | `cargo +1.85.1 test --locked -p layerfs-api --test store_family` | same | create on native, push to a worker machine and pull back with equal heads; push, pull and remove refusals |
| `layerfs-control/tests/store_reads.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test store_reads` | macOS, Linux | `store.status`, `list`, `get` take no lock and leave a stale lease as it was |
| `layerfs-store/tests/kind_isolation.rs` | `cargo +1.85.1 test --locked -p layerfs-store --test kind_isolation` | any | `sqlite/` and `postgresql/` never import each other |

## Pages

| Page | What it covers |
| --- | --- |
| [SQLite and PostgreSQL](sqlite-and-postgresql.md) | sqlite create, remove, push and pull; PostgreSQL connect, attach, open, the host read session and credentials |
| [Projects](projects.md) | `project.init` (in process and through a worker), `export`, `remove`, project records and discovery |

# SQLite and PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The two Store kinds' own calls: a sqlite Store's file created, moved and
removed across machines, and a PostgreSQL database connected, attached to
machines and opened, within [Stores and projects](README.md).

| | |
| --- | --- |
| Source | [`layerfs-store/src/sqlite/`](../../../../control/crates/layerfs-store/src/sqlite), [`layerfs-store/src/postgresql/`](../../../../control/crates/layerfs-store/src/postgresql), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs), [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |

`sqlite/` and `postgresql/` never import each other; the kind-neutral root
chooses by `StoreKind`. Every new Store runs the Disposable profile
(WAL, synchronous=OFF), the only permitted one; Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Create and remove (`sqlite/create.rs`, `sqlite/remove.rs`)

| Call | Does | Refusals |
| --- | --- | --- |
| `store.create <s>` | an empty Store at `store/<s>/store.sqlite` on `native` | `Exists` |
| `store.remove <s>` | deletes the file wherever it is (here or a worker `store.file remove`), its projects, and any copy an ended push or pull left on the other machine (`op.args.leftover`, so a second remove finishes an interrupted one); clears a gone process's unresolved `op` | `InvalidInput` for PostgreSQL (`store disconnect` keeps its data); `InUse`; `Conflict` if the Store moved meanwhile; a live `op` `Busy` |

## Transfer (`sqlite/transfer/`)

`store.push <s> <machine>` moves a sqlite Store's file from `native` to a
machine's worker; `store.pull <s>` brings it back. Between two machines
the caller pulls, then pushes; each half is its own operation
([`phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)).

```text
 check     record unblocked; sides; InUse (leases, Sandboxes) under the
           Store lock; both executors reached (stopped -> Unavailable);
           target Stat: no file there (Exists), free >= bytes + WAL +
           max(1 GiB, bytes/10) (Capacity, detail {needed, free});
           write op {from, to}
 snapshot  source: the idle Store's bytes, sha256, heads
 send      Read pieces of <= 8 MiB from source -> Write to target's
           store.sqlite.incoming                     progress {done,total}
 verify    target: the copy's bytes, sha256 and heads are the snapshot's
 install   (op written uncertain first) rename incoming -> store.sqlite
 record    record names the new machine; last_transfer {.., retired:false}
 retire    delete the source's copy; retired: true; op dropped
```

| Step (`StoreStep`) | On the side holding the file | Source |
| --- | --- | --- |
| `Stat` | exists, size, WAL size, partial `incoming` size, free bytes | [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `Snapshot` | the idle Store's bytes, sha256 and heads | same |
| `Stage`, `Unstage` | a consistent snapshot of the live Store beside it (`.backup`), for `backup.create` | same |
| `Read`, `Write` | one piece (at most 8 MiB); `Write` offset 0 starts the file again | same |
| `Verify`, `Install` | compare the copy with the snapshot; rename it into place (0600) | same |
| `Retire` | delete a copy the record does not name | same |
| `Remove` | delete the Store's whole directory on the machine | same |

Steps run in the caller's process for `native` and in the machine's worker
(`store.file`, decoded once into `StepFrame`) for every other machine.
Paths come from the root and the Store's name only. A step that reads,
replaces or deletes the file first closes this process's open copy; a call
running on it is `InUse`
([`step.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/step.rs)).

| Case | What is kept | Cleared by |
| --- | --- | --- |
| a phase fails certain | whatever exists; the refusal names the phase | — |
| `Unknown`, or any failure at `record` (both copies exist) | both copies; `op` uncertain; `log/uncertain.jsonl` | a new push or pull, which first deletes the copy the record does not name |
| a retire that did not finish | the source's copy; `last_transfer.retired: false` | the next push or pull |
| a push that failed certain at `send`, `verify` or `install` | the target's `store.sqlite.incoming`; the record's `incoming_on` names the machine | the next `store push` or `store remove` (doctor reports its bytes) |

The copy the record names is never touched, and nothing is deleted while
another operation holds the Store
([`resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).

## PostgreSQL (`postgresql/`)

```text
 store.connect(name, url, {password: "env:PG", synchronous_commit})
   refuse_password(url)       user:pass@ or any *password* param -> InvalidInput
   synchronous_commit         off (default) | local | remote_write | on
                              | remote_apply, else InvalidInput
   resolve(password)          checked once now
   Exists?                    same name, or same server+database, or
                              (once read) same schema binding
   schema `layerfs`           empty database: create it, new binding;
                              existing: read its binding
   attach native              checked with the Store's connection
   projects in the schema --> project records; record written under
                              the home lock (re-checked)
```

| Call | Does | Refusals | Source |
| --- | --- | --- | --- |
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: URL without password, schema, `synchronous_commit`, password reference, binding, a cursor key of this home's own, `machines: {native: ..}` | `InvalidInput`, `Exists`, `Unavailable` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `store.disconnect` | forgets the Store; its data stays | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it | same |
| `store.attach <s> <machine> {url, password}` | the machine's platform must run PostgreSQL Sandboxes (support table); the check runs on the side that dials (this process, or the worker's `store.reach`); records `machines.<m>: {url?, password?, checked: {at, server_version, latency_ms}}` | `Unsupported` (row reason; a worker without `store.reach` names stop and start); `Unavailable` (server not reached, naming the host, never the URL, plus the platform's hint; or another Store's schema); `Denied` (login or privilege); `Conflict` if the record changed meanwhile | [`attach.rs`](../../../../control/crates/layerfs-store/src/postgresql/attach.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |
| `store.detach <s> <machine>` | forgets one attachment; data and other machines stay | `InvalidInput` if not attached; `InUse` while a Sandbox of the Store is recorded there | [`attach.rs`](../../../../control/crates/layerfs-store/src/postgresql/attach.rs) |
| Store calls | open the schema in this process with the URL resolved now; built without the `postgresql` feature, `Unsupported` | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A record without `machines` (written before attachments) is attached to
`native` alone. Nothing converts a sqlite Store into PostgreSQL: a project
reaches PostgreSQL only by an Init into it.

### Credentials

```text
 Store record: url postgres://u@h/db  password env:NAME | file:/abs/path
      |  each call, in the caller only
      v
 secrets::with_password -> u:<percent-encoded password>@h/db  (memory)
      v
 machine reach(spec, url) -> postgresql field of ONE request
      |  native: same process; else worker sandbox.host, whose decode
      |  drops url and password from the Store record it keeps
      v
 DaemonStore::Postgres in the daemon setup -> <sandbox>/config/daemon.setup
                                              0600 in a 0700 folder
```

| Rule | Source |
| --- | --- |
| A reference is `env:NAME` (unset or empty `Unavailable`) or `file:/absolute/path` (unreadable `Unavailable`, trailing newline trimmed); anything else `InvalidInput`; a reference needs a URL with a user | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Every byte outside `A-Z a-z 0-9 - . _ ~` is percent-encoded | same, `percent` |
| Resolved again on each call; the full URL is never written to a record, log, trace or `Debug` output | same; [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) `decode` |
| The daemon runs `--config <setup file>`; the setup is in neither its arguments nor its environment; `DaemonStore` prints `url` as `[redacted]` | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) |

How the URL is rewritten for a machine is
[Reach](../machines/workers-and-reach.md#reach-reachrs).

## Related

- [Stores and projects](README.md): records, routes and leases.
- [Machines: workers and reach](../machines/workers-and-reach.md): `store.file`, `store.reach` and URL rewriting.
- [Backups](../backups/README.md): `Stage` and `Unstage` for `backup.create`.
- [Store: SQLite](../../engine/store/sqlite.md), [Store: PostgreSQL](../../engine/store/postgresql.md): what the engine opens.
- [API: store](../../api/sdk/store.md).

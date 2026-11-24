# SQLite and PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The two Store kinds' own calls: a sqlite Store's file created, moved and
removed across machines, and a PostgreSQL database connected, updated,
checked from a machine and opened, within [Stores and projects](README.md).

| | |
| --- | --- |
| Source | [`layerfs-store/src/sqlite/`](../../../../control/crates/layerfs-store/src/sqlite), [`layerfs-store/src/postgresql/`](../../../../control/crates/layerfs-store/src/postgresql), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs), [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |

`sqlite/` and `postgresql/` never import each other; the kind-neutral root
chooses by `StoreKind`. Every sqlite Store opens with the Normal profile
(WAL, synchronous=NORMAL; its record's label is `normal`); Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Create and remove (`sqlite/create.rs`, `sqlite/remove.rs`)

| Call | Does | Refusals |
| --- | --- | --- |
| `store.create <s>` | an empty Store at `store/<s>/store.sqlite` on `native` | `Exists` |
| `store.remove <s>` | deletes the file wherever it is (here or a worker `store.file remove`), its projects, and any copy an ended move left on the other machine (`op.args.leftover`, so a second remove finishes an interrupted one); clears a gone process's unresolved `op` | `InvalidInput` for PostgreSQL (`store disconnect` keeps its data); `InUse`; `Conflict` if the Store moved meanwhile; a live `op` `Busy` |

## Transfer (`sqlite/transfer/`)

`store.move <s> <machine>` moves a sqlite Store's file from `native` to a
machine's worker; `store.move <s> native` brings it back. One side is always `native`: between two
machines the caller moves the Store to `native`, then on; each half is its
own operation
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
 record    incomplete SQLite project records without an `op` are completed
           read-only from the local `store.sqlite` (`restored::complete_existing`),
           best effort: each one left incomplete is a line of log/repair.jsonl;
           record names the new machine; last_transfer {.., retired:false}
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
| `Unknown`, or any failure at `record` (both copies exist) | both copies; `op` uncertain; `log/uncertain.jsonl` | a new `store move`, which first deletes the copy the record does not name |
| a retire that did not finish | the source's copy; `last_transfer.retired: false` | the next `store move` |
| a move that failed certain at `send`, `verify` or `install` | the target's `store.sqlite.incoming`; the record's `incoming_on` names the machine | the next `store move` or `store remove` (doctor reports its bytes) |

The copy the record names is never touched, and nothing is deleted while
another operation holds the Store
([`resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).

## PostgreSQL (`postgresql/`)

```text
 store.connect(name, url, {auth, synchronous_commit})
   refuse_login(url)          a user, user:pass@, any *password* param
                              or passfile
                              -> InvalidInput
   auth                       required, and saved (`store save-auth`):
                              else InvalidInput / NotFound
   synchronous_commit         off | local (default) | remote_write | on
                              | remote_apply, else InvalidInput
   Exists?                    same name, or (once read) same schema
                              binding, whatever address or login
   schema `layerfs`           empty database: create it, new binding;
                              existing: read its binding
   projects in the schema --> project records; record written under
                              the home lock (re-checked)
```

| Call | Does | Refusals | Source |
| --- | --- | --- | --- |
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: URL with no user or password, schema, `synchronous_commit`, its saved login `auth`, binding, a cursor key of this home's own, no machine. On a connected Store the URL becomes the address every machine dials (`readdress`); with `--machine <m>` it is that machine's own, `machines.<m>: {url}`, checked from the side that dials (this process, or the worker's `store.reach`): the server answers, its schema holds this Store, and the role has what a Sandbox's reads and writes need (the schema's `USAGE` and `CREATE`, the database's `TEMP`, each table's and sequence's privileges; never what only a compaction needs, which `store compact` checks itself), and the machine's platform must run PostgreSQL Sandboxes. Both refuse `--auth` and `--synchronous-commit` | `InvalidInput`, `Exists`, `NotFound` (no such saved login), `Unavailable`, `Unsupported` (`--machine` on a platform that runs no PostgreSQL Sandbox), `Denied`, `Conflict` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `connect` |
| `store.disconnect` | forgets the Store; its data stays. With `--machine <m>`, removes that machine's own address, which then dials the Store's, and forgets its reach | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it; `NotFound` when the machine has neither an address of its own nor a reach | same |
| `store.update <s> {auth, synchronous_commit}` | another saved login or `synchronous_commit` (one is required); a new login is checked once from this process on the Store's address, which must hold this Store, and saved again under the home lock; the write is clean (`StoreRecord::cleaned`) | `InvalidInput`; `Unavailable` (not reached, or another Store's schema); `Conflict` if the record changed meanwhile | [`update.rs`](../../../../control/crates/layerfs-store/src/postgresql/update.rs) |
| Store calls | open the schema in this process with the URL resolved now | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A PostgreSQL Store is on no machine. Its Sandboxes run on every machine
whose platform runs PostgreSQL Sandboxes (#536 C4), and `sandbox.create`
checks from the machine that it reaches this Store, so no call comes
between `store.connect` and the first Sandbox. `machines` holds only the
machines with a connection of their own (H9). A record holding what 2.0 no
longer writes (a `password`, a URL with a user or a login parameter, a
`machines.<m>` entry without `url` or with more, such as an old `checked`)
reads `broken` `outdated` (`StoreRecord::outdated_fields`), fixed by `store
update <s> --auth <auth>`; every PostgreSQL record write (`connect`,
`update`, `save_own`, `forget_machine`) drops those fields
(`StoreRecord::cleaned`, spec v23 V23-6), and no read does.
`machine.remove` forgets the machine's entry and its reach in every Store.

### Who connects (spec v23)

What was seen when a side dialled the database is kept beside the record,
in `store/<s>/observed.json` with the `at` a live read writes:
`here` (this computer) and `reach.<machine>`, each `{ at, ok, latency_ms,
server_version, error }`
([`observed.rs`](../../../../control/crates/layerfs-store/src/observed.rs)).
Writers: `store.connect --machine` (its check), `store.doctor` (this
computer, measured with the same check, then every machine with an address
of its own, a Sandbox, or `--machine`) and a Sandbox create on a machine
(`{ at, ok: true }`, on the host after the reply,
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)
`reach`). `native` with no address of its own dials as this computer and is
recorded as `here`. A writer runs after its Store I/O, under the Store's
lock, and writes nothing once the record is gone; a write never fails its
call and moves no `generation`. `connect.rs` `connections` reads it back:
this computer, then every machine with an address of its own, a reach or a
Sandbox (handed in by `layerfs-control`), each `{ machine, url, own, server,
reached, reach }`. Nothing converts a sqlite Store into PostgreSQL: a project
reaches PostgreSQL only by an Init into it.

### Credentials

A PostgreSQL Store's record names a saved login (`auth`), which
`postgresql/login.rs` `dial` puts into the address it dials
(`secrets::with_login`); a machine's own address (`machines.<m>.url`) is
dialled with the same login. A record that names no `auth` (written before
saved logins) is `broken` `outdated`, and every call that dials it is
refused `InvalidInput` with the fix `layerfs store save-auth <auth> --user
<user> --password-stdin && layerfs store update <store> --auth <auth>`; its
old `password` reference is not read. A record naming a login the home does
not hold is `broken` `missing` with the save of that login and the same
update as its fix
([`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)
`dial`, `no_auth_fix`; [`state.rs`](../../../../control/crates/layerfs-store/src/state.rs)).

```text
 Store record: url postgres://h/db  auth <auth>
 secret/auth/<auth>.json: user, password (0600)
      |  each call, in the caller only
      v
 secrets::with_login -> <user>:<percent-encoded password>@h/db  (memory)
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
| The login is a saved one: a URL that names a user, holds a password or names a `passfile` is refused `InvalidInput` (`refuse_login`), with a fix giving the address alone (`refuse_login_with_fix`: `save-auth` + `--auth` on a first connect, `save-auth` + `store update --auth` before the address alone on a connected Store); `sslkey` and `sslcert` pass; a named login not saved is `NotFound`, one with no user or password `Failed` | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) `read` |
| Every byte outside `A-Z a-z 0-9 - . _ ~` is percent-encoded | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) `percent` |
| Put in again on each call; the full URL is never written to a record, log, trace or `Debug` output | same; [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) `decode` |
| The daemon runs `--config <setup file>`; the setup is in neither its arguments nor its environment; `DaemonStore` prints `url` as `[redacted]` | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) |

The URL is never rewritten for a machine; which URLs a machine refuses is
[Reach](../machines/workers-and-reach.md#reach-reachrs).

## Related

- [Stores and projects](README.md): records, routes and leases.
- [Machines: workers and reach](../machines/workers-and-reach.md): `store.file`, `store.reach` and the loopback refusal.
- [Backups](../backups/README.md): `Stage` and `Unstage` for `backup.create`.
- [Store: SQLite](../../engine/store/sqlite.md), [Store: PostgreSQL](../../engine/store/postgresql.md): what the engine opens.
- [API: store](../../api/sdk/store.md).

# Home and records

[Architecture](../../README.md) › [Control](../README.md) › [Management](README.md) › Home and records

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The LayerFS home and its records.

| | |
| --- | --- |
| Component | [Control](README.md) |
| Source | [`control/crates/layerfs-control/src/common`](../../../../control/crates/layerfs-store/src), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |

## Home layout

The home is `LAYERFS_HOME`, else `~/.layerfs`; it must be absolute
([`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs)).
Layout 3, from [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs):

```text
<home>/
  layerfs.json              {layout: 3, id: <32 hex>}
  layerfs.lock              home lock (flock, host processes only)
  machine/<name>/
    record.json             platform, size, state, op
    worker.json             worker address and secret (0600)
    start.lock              held across a start, stop, create, remove
  machine/native/run/       native Sandbox folders, images/ cache
  store/<name>/
    record.json             kind, machine, backups, op
    lock                    the Store lock
    store.sqlite            a sqlite Store's file while on native
    open/<pid>              one lease per process using the Store
    project/<name>/record.json
  sandbox/<name>/
    record.json             name, project, machine, backend, op
    control.json            daemon keys (0600)
    lock                    the Sandbox lock
    serve.lock              turn of a start, mount or close
    workspace/<id>.json     one per mounted Workspace
    workspace/<id>.unmount  an unmount's outcome
  backup/<bucket>/record.json  provider, url, prefix, key reference
  log/                      *.jsonl logs, conversion reports
  trace/                    the call log; 1.1.0 traces/ after conversion
    calls.jsonl             one line per public call (0600; trace/ is 0700)
    calls.1.jsonl           the one rotated file, past 16 MiB
    calls.lock              rotation and reset take turns (flock)
```

| Entity | Identity | Children | Source |
| --- | --- | --- | --- |
| Machine | folder name | worker record | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Store | folder name | projects, leases | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Project | `<store>/<project>` | none | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Sandbox | folder name, unique in the home | Workspaces | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Workspace | `<sandbox>:<path>` or `<sandbox>:<name>` | none | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| Bucket | folder name | none | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |

Relationships (machine to its Stores and Sandboxes, Store to its
Sandboxes, project to its Sandboxes) are derived by
[`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs),
never stored. `machine/native/` and `machine/docker/` exist from the first
call and are never removed (`ensure_built_ins` in
[`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs)).

A machine's worker keeps the same paths under its own root (the machine's
`/layerfs/data`), but that root is not a home: it is never converted, has no
id and uses the caller's home id sent with each request
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## Records

```text
 write_record(path, value)
   private_dir(parent)            0700
   .<name>.<pid>.<n>.tmp          0600, JSON + "\n", bytes as {"hex": ..}
   rename(tmp, path)              reader sees old or new, never part
```

| Rule | Source |
| --- | --- |
| One JSON value per file, replaced whole by one rename | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) |
| Entity records carry `"v": 1` first (`RECORD_VERSION`) | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) |
| A record holds facts and at most one `op` intent; states such as `running` are derived when read | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| A Sandbox, daemon or Workspace record is typed field by field; its codec keeps the JSON as read: file order, null against absent, unknown and mistyped fields; a changed field keeps its place, a new one comes last | [`home/fields.rs`](../../../../control/crates/layerfs-home/src/home/fields.rs), [`record/`](../../../../control/crates/layerfs-sandbox/src/record/mod.rs) |
| Logs are appended JSON lines in `log/` (0600) | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) `log_line` |

The `op` intent, from [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs):

```text
 op: { id: "op-<16 hex>", name, phase, args,
       owner: {pid, pid_start, pid_boot},
       started, updated, uncertain }

 begin -> write op -> move phase ... -> success: result + drop op (1 write)
                                   \-> certain failure: drop op
                                   \-> uncertain: keep op, uncertain=true,
                                       append log/uncertain.jsonl
```

| Owner alive | `uncertain` | Standing | A new operation on the entity |
| --- | --- | --- | --- |
| yes | any | `Running` | `Busy` |
| no | true | `Unresolved` | `Unresolved`, naming the show and clear calls |
| no | false | `Stale` | proceeds; the intent holds nothing |

Liveness compares pid, kernel start time and boot id, so a reused pid is
not the owner ([`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) `recorded_alive`).

### Locks and leases

| Lock or file | Guards | Wait | Source |
| --- | --- | --- | --- |
| `layerfs.lock` | record writes, name claims, Sandbox-name uniqueness | 120 s, then `Unavailable` | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs) |
| `machine/<m>/start.lock` | a start, stop, create or remove | 600 s | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| `store/<s>/lock` | leases, `op` and phase writes, the record | 120 s | [`home/entity_lock.rs`](../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| `sandbox/<n>/lock` | the Sandbox and Workspace records | 120 s | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `store/<s>/open/<pid>` | a lease: this process uses the Store | taken under the Store lock | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |

Order is home, start, store, sandbox. No lock is held across a worker
call, a VM boot or a byte transfer, except `start.lock` across its start.
A stale lease (its process gone) is removed under the Store's lock by the
next lease taker or by push, pull, remove and disconnect, and logged to
`log/leases.jsonl`; reads leave it.

## Names

| Kind | Rule | Type | Source |
| --- | --- | --- | --- |
| machine, Store, project, Sandbox, bucket, Workspace name | 1–63 of `a-z 0-9 _ . -`, first and last a letter or digit, not a Windows device name (`con`, `nul`, `com1`…) | `MachineName`, `StoreName`, `ProjectName`, `SandboxName`, `BucketName`, `WorkspaceName` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| project | always `<store>/<project>`; a bare name is refused | `ProjectRef` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| Workspace | `<sandbox>:<absolute path>` or `<sandbox>:<name>` | `WorkspaceRef` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| built-in machines | `native`, `docker`; refused as new machine or Sandbox names | `BUILT_IN` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |

A bad name is `InvalidInput`. Each name is checked once by its type's
`parse`; `listed` takes a folder name the home already holds.

## Secrets

| Secret | Where it lives | Source |
| --- | --- | --- |
| PostgreSQL password | never stored; the record holds the URL without it and a reference `env:NAME` or `file:/abs/path`, resolved on each call | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs), [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| Bucket key | a reference in `backup/<bucket>/record.json`, never the key | [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| Worker secret | `machine/<m>/worker.json` (0600) | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) `worker_secret` |
| Sandbox daemon keys | `sandbox/<n>/control.json` (0600) | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) `daemon_keys` |

```text
 store.connect(name, url, {password: "env:PG_PW"})
   refuse_password(url)      URL with user:pass or ?password= -> InvalidInput
   resolve("env:PG_PW")      checked once now
   record: {url (no password), password: "env:PG_PW", ...}

 each Store call:  secrets::url(record) -> full URL in memory only
                   -> one request (this process, or a worker), never a record
```

A reference whose variable is unset or file unreadable is `Unavailable`;
a password that is not a reference is `InvalidInput`.

## Traces

| Trace | Written when | Source |
| --- | --- | --- |
| `LAYERFS_TRACE` file | each connection to a machine's worker appends `{at, event: "worker.connection", op, worker, sent, ms}`; unset, nothing is written | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) `connection` |
| SDK trace scope | a call's `observe` argument (32 bytes hex) makes its daemon exchanges traced observations; at most 64 telemetry values per call are kept | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) |
| `<home>/trace/calls.jsonl` | the call log (#659): every public call appends one line when it ends; see below | [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| `<home>/trace/` (other files) | 1.1.0's `traces/` after a conversion | [`migrate/v110.rs`](../../../../control/crates/layerfs-control/src/migrate/v110.rs) |

A trace that cannot be written changes no outcome.

### The call log

Every call that goes through the dispatcher (`calls::dispatch::call_scoped`:
the `layerfs` command, Node, Python and direct `layerfs_api::call`) appends
one JSON line to `<home>/trace/calls.jsonl` when it ends, in completion
order. A running call has no line yet; a killed process writes none. Worker
requests, the parts a call sends internally, commands outside the table
(`help`, `--version`, `worker`, `import-env`, `doctor`), usage errors and the
`logs` calls write no line. `LAYERFS_LOGS=0` turns it off for the process;
`LAYERFS_TRACE` and `LAYERFS_TRACE_STEP` are independent of it.

```json
{"v":1,"at":1793000004120,"ms":212,"client":"node","argv":["file","write","demo/app","main","/index.ts","<5321 bytes>"],"outcome":"ok","kind":"Committed","commit":"<66 hex>","tag":"dsh:s_4d2a:a91e"}
```

| Field | Meaning |
| --- | --- |
| `v` | line format, `1` |
| `at`, `ms` | wall-clock start (ms since the epoch) and duration on a monotonic clock |
| `client` | `cli`, `node`, `python` or `rust` (direct library calls) |
| `argv` | the CLI words that would make the same call, rendered from the call table and the checked arguments before a folder scope rewrites them ([render.rs](../../../../api/crates/layerfs-api/src/calls/render.rs)); a call refused before its arguments were checked logs `[family, call]` only |
| `outcome` | `"ok"` or the refusal code; refusal messages and `detail` are never logged |
| `kind`, `commit`, `layer` | the result's `kind`, and the Commit or Layer a saving call saved or names (`file` writes, `workspace.commit`, `branch.fork`, `merge`, `publish`); no other result value |
| `tag` | the caller's tag: a per-call `log_tag` argument, else `LAYERFS_LOGS_TAG`; at most 128 bytes and no control characters, else dropped |

Values are cut to 200 characters; Content and Json arguments, and the Text
arguments `command`, `message`, `old`, `new`, `password` and `url`, are
logged as `<N bytes>`; functions are left out.

A line is opened with `O_APPEND`, written with one `write` and closed, so a
long-lived process never writes into a rotated or reset file. Nothing is
written in a folder that has no `layerfs.json` yet. Past 16 MiB, the writer
that gets a non-blocking `flock` on `calls.lock` renames the file to
`calls.1.jsonl`, replacing an older one; `logs.reset` takes the same lock,
waiting, truncates `calls.jsonl` in place, removes `calls.1.jsonl` and
answers `{kind: "Reset", removed}`. `logs.list` reads both files newest
first, filtered and paged; after a rotation or reset its cursor is stale
(`InvalidInput`), and a torn line is counted in `skipped`
([SDK: logs](../../api/sdk/logs/list.md)).

## Migration

```text
 open(home)
   detect(home)  (reads only)
     Current(3) ----------------------------> finish layout-3 step 3 if
                                              log/layout-3.pending; use it
     Empty | Old | Previous(2)
       take layerfs.lock, detect again
         Empty       -> create layerfs.json (layout 3, new id)
         Previous(2) -> v2::convert
         Old (1.1.0) -> v110::convert -> layerfs.json at layout 2
                        -> log/migration.json -> v2::convert
     newer layout -> Unsupported;  broken -> Failed
   failure -> log/migration-failures.jsonl; the next call continues
```

| Conversion | What it does | Source |
| --- | --- | --- |
| 1.1.0 to layout 2 | moves `.stores/`, project folders, `stores.json`, `remotes`, `remote-state`, `.runtime/` and `traces/` into `store/`, `backup/`, `log/` and `trace/`; old files kept under `log/1.1.0/`; `layerfs.json` written last | [`migrate/v110.rs`](../../../../control/crates/layerfs-control/src/migrate/v110.rs) |
| sqlite Store files from Docker volumes | linked where readable, else copied and sha256-checked; a file with a WAL is not placed | [`migrate/volume.rs`](../../../../control/crates/layerfs-control/src/migrate/volume.rs) |
| layout 2 to 3 | refused `InUse` while anything is live, or while a Sandbox's runtime cannot be checked; a layout 2 Sandbox of the retired docker backend is forgotten, as `sandbox.close` forgets it, with a note in `log/migration-2.json` naming its container to remove (D-8); writes Sandbox records to `sandbox/<name>/`, adds `"v": 1`, writes `log/layout-3.pending`, flips `layerfs.json`, then deletes the layout 2 paths and writes `log/migration-2.json` | [`migrate/v2.rs`](../../../../control/crates/layerfs-control/src/migrate/v2.rs) |
| 1.1.0 environment | `layerfs import-env` records settings from `LAYERFS_*` variables once; never writes a secret, makes a VM or connects a host; logs to `log/import-env.jsonl` | [`migrate/import_env.rs`](../../../../control/crates/layerfs-control/src/migrate/import_env.rs) |

A worker clears what an earlier bundle left under its root on its first
start from a layout 3 bundle (`retire_layout_2`, marker `.layout-3`,
[`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## Related

- [Control](README.md), [Families](families.md), [Doctor](doctor.md)
- [Workers](../routing/workers.md): the worker's root and `home_id`
- [Errors](../../api/call-table/errors.md#uncertain-outcomes): `Unknown`, `Busy`, `Unresolved`
- [Glossary](../../glossary.md)

---

← [Control: Management](README.md) · [Up: Management](README.md) · [Management: Families](families.md) →

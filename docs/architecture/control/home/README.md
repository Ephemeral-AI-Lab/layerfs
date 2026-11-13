# Home

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The home is the folder of JSON records that says which machines, Stores,
projects, Sandboxes and buckets exist, plus the locks, intents, names,
secrets and logs every control call shares.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-home` |
| Source | [`layerfs-home/src`](../../../../control/crates/layerfs-home/src), [`home/`](../../../../control/crates/layerfs-home/src/home) |
| Uses | [Bridge](../../engine/bridge/README.md) (`SdkValue`, `SdkRefusal`, process identity) |
| Used by | [Machines](../machines/README.md), [Stores and projects](../stores-and-projects/README.md), [Sandboxes](../sandboxes/README.md), [Backups](../backups/README.md), [Doctor](../doctor/README.md), [API calls](../../api/calls/README.md) |
| Flows | [Call](../../flows/call.md) |

The crate knows no entity: machine, Store, Sandbox and Workspace records
build on its record files, locks and `op` intent. Converting an older home
is in [Migration](migration.md).

## Picture

```text
 <home>/                      LAYERFS_HOME, else ~/.layerfs (absolute)
   layerfs.json               {layout: 3, id: <32 hex>}
   layerfs.lock               home lock (flock; host processes only)
   machine/<m>/  record.json  worker.json (0600)  start.lock
   machine/native/run/        local Sandbox folders, images/ cache
   store/<s>/    record.json  lock  store.sqlite (sqlite on native)
                 open/<pid>   one lease per process using the Store
                 project/<p>/record.json
   sandbox/<n>/  record.json  control.json (0600)  lock  serve.lock
                 workspace/<id>.json  <id>.unmount  <id>.unmount-lock
   backup/<b>/record.json     provider, url, prefix, region, key ref
   log/                       *.jsonl logs, conversion reports
   trace/calls.jsonl          the call log; calls.1.jsonl  calls.lock

 write_record(path, value)
   private_dir(parent) 0700 -> .<name>.<pid>.<n>.tmp 0600 -> rename(path)
```

A machine's worker keeps the same paths under its own root
(`/layerfs/data` in the machine). That root is not a home: it has no
`layerfs.json`, is never converted and serves each request under the home
id its caller sent
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Finds the home | the call's explicit home, else `LAYERFS_HOME`, else `$HOME/.layerfs`; a relative path is `InvalidInput` | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Names each record's path | one folder per entity kind, one identity per entity (its name); projects nest under their Store, Workspaces under their Sandbox | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Reads the layout | `layerfs.json` holds `layout` and a 16-byte random `id` written once; a newer layout is `Unsupported`, an unreadable one `Failed`; reading never writes | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) |
| Writes records | one JSON value per file, replaced whole by a rename; bytes as `{"hex": ..}`; entity records carry `"v": 1` first | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) |
| Keeps unknown fields | a typed record reads its known fields and keeps every other field (unknown or mistyped) exactly as read, in file order; a changed field keeps its place, a new one comes last | [`home/fields.rs`](../../../../control/crates/layerfs-home/src/home/fields.rs) |
| Records one intent | a multi-step operation writes `op` before its first effect and drops it with its result in one write | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Checks names | each kind of name is a type checked once by `parse` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| Resolves secrets | password references resolved per call; worker secret and daemon keys in 0600 files | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Streams events | one event stream per running call: progress, a worker's events, command output | [`events.rs`](../../../../control/crates/layerfs-home/src/events.rs) |
| Names this process | pid, kernel start time and boot id, so a reused pid does not keep an `op`, lease or forward alive | [`process.rs`](../../../../control/crates/layerfs-home/src/process.rs) |
| Writes traces and the call log | `LAYERFS_TRACE` lines and `trace/calls.jsonl` | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs), [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| Prints JSON | whole numbers only; non-UTF-8 bytes as `{"base64": ..}`; worker frames carry byte values as attachments | [`json.rs`](../../../../control/crates/layerfs-home/src/json.rs) |

### Entities

| Entity | Identity | Children | Source |
| --- | --- | --- | --- |
| Machine | folder name | worker record | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Store | folder name | projects, leases | same |
| Project | `<store>/<project>` | none | same |
| Sandbox | folder name, unique in the home | Workspaces | same |
| Workspace | `<sandbox>:<absolute path>` or `<sandbox>:<name>` | none | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| Bucket | folder name | none | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |

Relationships (a machine's Stores and Sandboxes, a Store's or project's
Sandboxes) are derived when needed, never stored
([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)).

### The `op` intent

```text
 op: { id: "op-<16 hex>", name, phase, args,
       owner: {pid, pid_start, pid_boot}, started, updated, uncertain }

 begin -> write op -> move phase ... -> success: result + drop op (1 write)
                                   \-> certain failure: drop op
                                   \-> uncertain: keep op, uncertain=true,
                                       append log/uncertain.jsonl
```

| Owner alive | `uncertain` | Standing | A new operation on the entity |
| --- | --- | --- | --- |
| yes | any | `Running` | `Busy`, naming the operation, phase and pid |
| no | true | `Unresolved` | `Unresolved`, naming the call that shows it and the call that clears it |
| no | false | `Stale` | proceeds; the intent holds nothing |

### Locks and leases

| Lock or file | Guards | Wait, then | Source |
| --- | --- | --- | --- |
| `layerfs.lock` | record writes, name claims, Sandbox-name uniqueness | 120 s, `Unavailable` | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs) |
| `machine/<m>/start.lock` | a machine start, stop, create or remove | 600 s, `Unavailable` | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| `store/<s>/lock` | leases, `op` and phase writes, the Store record | 120 s, `Unavailable` | [`home/entity_lock.rs`](../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| `sandbox/<n>/lock` | the Sandbox and Workspace records | 120 s, `Unavailable` | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `sandbox/<n>/serve.lock` | a Sandbox start, Workspace mount or close (the discovery turn) | 60 s, `Busy` | [`project/discovery.rs`](../../../../control/crates/layerfs-store/src/project/discovery.rs) |
| `store/<s>/open/<pid>` | a lease: this process uses the Store | taken under the Store lock | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |

Waits are readiness waits, not retries. Within one process, calls take
turns before the file lock (a `flock` belongs to the open file
description); a thread that holds the home lock takes it again at once.

### Names

| Kind | Rule | Type | Source |
| --- | --- | --- | --- |
| machine, Store, project, Sandbox, bucket, Workspace name | 1–63 of `a-z 0-9 _ . -`, first and last a letter or digit, not a Windows device name (`con`, `nul`, `aux`, `prn`, `com1`–`com9`, `lpt1`–`lpt9`) | `MachineName`, `StoreName`, `ProjectName`, `SandboxName`, `BucketName`, `WorkspaceName` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| project | always `<store>/<project>`; a bare name is refused even when one Store holds it | `ProjectRef` | same |
| Workspace | `<sandbox>:<absolute path>` or `<sandbox>:<name>` | `WorkspaceRef` | same |
| built-in machines | `native`, `docker`; refused as new machine or Sandbox names | `BUILT_IN` | same |

A bad name is `InvalidInput`. `listed` takes a folder name the home
already holds as it is.

### Secrets

| Secret | Where it lives | Source |
| --- | --- | --- |
| PostgreSQL password | never stored: the record holds the URL without it and a reference `env:NAME` or `file:/abs/path`, resolved on each call into an in-memory URL | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Bucket key | a reference in `backup/<bucket>/record.json`, never the key | [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| Worker secret | `machine/<m>/worker.json` (0600) | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Sandbox daemon keys | `sandbox/<n>/control.json` (0600) | same |

### Traces and the call log

| Output | Written when | Source |
| --- | --- | --- |
| `LAYERFS_TRACE` file | each connection to a worker appends `{at, event: "worker.connection", op, worker, sent, ms}`; with `LAYERFS_TRACE_STEP` too, one `{at, event: "step", op, step, us, ms}` line per timed step (a worker-routed call asks its worker for its steps) | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) |
| SDK trace scope | a call's `observe` argument (32 bytes hex) traces its daemon exchanges; at most 64 telemetry values per call are kept | same |
| `trace/calls.jsonl` | every public call through the dispatcher (CLI, Node, Python, direct `layerfs_api::call`) appends one line when it ends | [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |

```json
{"v":1,"at":1793000004120,"ms":212,"client":"node","argv":["file","write","demo/app","main","/index.ts","<5321 bytes>"],"outcome":"ok","kind":"Committed","commit":"<66 hex>","tag":"ci:run-42"}
```

| Field | Meaning |
| --- | --- |
| `v` | line format, `1` |
| `at`, `ms` | wall-clock start (ms since the epoch) and duration on a monotonic clock |
| `client` | `cli`, `node`, `python` or `rust` |
| `argv` | the CLI words that would make the same call, rendered from the call table and the checked arguments ([`render.rs`](../../../../api/crates/layerfs-api/src/calls/render.rs)); a call refused before its arguments were checked logs `[family, call]` |
| `outcome` | `"ok"` or the refusal code; messages and `detail` are never logged |
| `kind`, `commit`, `layer` | the result's `kind` and the Commit or Layer a saving call saved or names; no other result value |
| `tag` | per-call `log_tag`, else `LAYERFS_LOGS_TAG`; at most 128 bytes, no control characters, else dropped |

Values are cut to 200 characters. Content and Json arguments and the Text
arguments `command`, `message`, `old`, `new`, `password` and `url` are
logged as `<N bytes>`.

No line is written for a running or killed call, worker requests, a
call's internal parts, commands outside the call table (`help`,
`--version`, `worker`, `import-env`, `doctor`), usage errors or the `logs`
calls. `LAYERFS_LOGS=0` turns the log off for the process; it is
independent of `LAYERFS_TRACE`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A reader sees the old record or the new one, never a part: write to a private temp file, then rename | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) `write_record` |
| Records are private: files 0600, folders 0700; `traversable` adds only o+x down to a namespaces Workspace | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) |
| A record holds facts and at most one `op`; states such as `running` are derived when read; reading never clears an intent | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Liveness compares pid, kernel start time and boot id, so a reused pid is not the owner | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`process.rs`](../../../../control/crates/layerfs-home/src/process.rs) |
| Lock order is home, start, Store, Sandbox; none is held across a worker call, a VM boot or a byte transfer, except `start.lock` across its start | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs), [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| A stale lease (its process gone) is removed under the Store lock by the next lease taker, or by push, pull, remove and disconnect, and logged to `log/leases.jsonl`; reads leave it | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |
| Only host processes take `layerfs.lock`; a worker's root is never treated as a home | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs), [`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs) |
| A password never reaches a record: `store.connect` refuses a URL that carries one; the record keeps only the reference | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `refuse_password`, [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| A trace or call-log line that cannot be written changes no outcome and is not retried | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs), [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| A call-log line is one `write` on a file opened `O_APPEND` and closed, so a long-lived process never writes into a rotated or reset file; nothing is written where `layerfs.json` is absent | [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Home lock, Store lock, Sandbox lock held by another | `Unavailable` after 120 s, nothing changed | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs), [`home/entity_lock.rs`](../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| Machine start lock held | `Unavailable` after 600 s | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| Discovery turn (`serve.lock`) held | `Busy` after 60 s | [`project/discovery.rs`](../../../../control/crates/layerfs-store/src/project/discovery.rs) |
| Live `op` on the entity | `Busy` | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Dead owner, uncertain `op` | `Unresolved`; `detail` names the entity and the intent | same |
| Bad name or relative home | `InvalidInput` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs), [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Password reference unset or unreadable | `Unavailable`; a password that is not a reference is `InvalidInput` | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Home of a newer layout | `Unsupported` | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) |
| Call log size | rotates past 16 MiB to the one `calls.1.jsonl` under a non-blocking `flock` on `calls.lock`; `logs.reset` waits for that lock, truncates in place and removes the rotated file | [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| `logs.list` page | default 100, at most 1000 lines; a cursor after a rotation or reset is stale (`InvalidInput`); a torn line is counted in `skipped` | same |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| `layerfs-control/tests/home_layout.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test home_layout` | macOS, Linux | `layerfs.json` and the layout version, one test per layout |
| `layerfs-control/tests/op_intent.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test op_intent` | macOS, Linux | `Busy`, `Unresolved` and stale intents; the per-entity lock |
| `layerfs-control/tests/typed_names.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test typed_names` | macOS, Linux | each name kind's refusals and texts |
| `layerfs-control/tests/typed_records.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test typed_records` | macOS, Linux | typed records answer their stable shape and read past `"v"` |
| `layerfs-home/tests/json_codec.rs` | `cargo +1.85.1 test --locked -p layerfs-home --test json_codec` | macOS, Linux | base64 and quoted strings round-trip exactly |
| `layerfs-api/tests/logs.rs` | `cargo +1.85.1 test --locked -p layerfs-api --test logs` | macOS, Linux | the call log's lines, rotation and reset |

## Pages

| Page | What it covers |
| --- | --- |
| [Migration](migration.md) | Opening a home: creating it, converting 1.1.0 and layout 2 homes, `import-env`, and clearing a worker root's leftovers |

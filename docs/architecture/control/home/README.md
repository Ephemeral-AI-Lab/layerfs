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
build on its record files, locks and `op` intent. [Opening a home](#opening-a-home)
says how the first call creates one and why a folder that is not a LayerFS
2.0 home is refused rather than written.

## Picture

```text
 <home>/                      LAYERFS_HOME, else ~/.layerfs (absolute)
   layerfs.json               {layout: 3, id: <32 hex>}
   layerfs.lock               home lock (flock; host processes only)
   machine/<m>/  record.json  worker.json (0600)  start.lock
                 forward.log  forward.1.log   (lima: its ssh forwards)
   machine/native/run/        0755: Sandbox run folders, <name>.root,
                              images/ cache
   store/<s>/    record.json  lock  store.sqlite (sqlite on native)
                 store.sqlite.incoming        a move's partial copy
                 backup.lock  backup.staging  store.sqlite.backup
                 open/<pid>   one lease per process using the Store
                 project/<p>/record.json
   sandbox/<n>/  record.json  control.json (0600)  lock  serve.lock
                 workspace/<id>.json  <id>.unmount  <id>.unmount-lock
                 workspace/<id>/exec/<x>.json  one per running command
   backup/<b>/record.json     provider, url, prefix, region, access key id
   secret/<kind>/<name>.json  0600: saved logins (auth), bucket keys
   log/                       uncertain.jsonl; leases, exec .jsonl and
                              their <name>.1.jsonl; rotate.lock
   trace/calls.jsonl          the call log; calls.1.jsonl  calls.lock

 /layerfs/data/               a machine's data root: its worker's root
   store/<s>/    record.json (name and kind)  store.sqlite
                 store.sqlite.incoming  store.sqlite.backup
   sandbox/<n>/  as in a home, for the Sandboxes its worker serves
   machine/native/run/        as in a home
   incoming/init-<16 hex>/    a project.init's folder copy, while it runs
   log/          worker.log  worker.1.log (0600)
                 exec.jsonl  exec.1.jsonl  rotate.lock
 /layerfs/state/              0700, beside the data root: secret,
                              worker.pid

 write_record(path, value)
   private_dir(parent) 0700 -> .<name>.<pid>.<n>.tmp 0600 -> rename(path)
```

A machine's worker keeps the same paths under its own root
(`/layerfs/data` in the machine). That root is not a home: it has no
`layerfs.json`, is never created or converted and serves each request under the home
id its caller sent
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).
[Files](#files) says who writes each entry and when it goes.

## Opening a home

This build converts nothing and knows no older layout (owner, 2026-11-19:
"cut and cleanup old incompatible aggressively, no lfs, and fallback to old
2.0 or 1.x", and of the conversion code, "just drop it. we will never need
it").

```text
 open(home)
   worker root? ----------------------> answer the caller's home id; no write
   detect(home)  (reads only)
     Current(3) ----------------------> use the home
     Foreign -------------------------> Unsupported, fix
                                        `mv <home> <home>.old && layerfs home init`;
                                        nothing written
     Empty
       take layerfs.lock, detect again
         Empty   -> create layerfs.json (layout 3, new id)
         Foreign -> Unsupported, the same fix
     layerfs.json or the folder listing refused by the system -> Failed
                                        (`unreadable`, the OS error, no fix)
```

| Layout | Recognised by | Source |
| --- | --- | --- |
| Empty | no folder; or no `layerfs.json` and nothing but what 2.0's own layout allows before it: `layerfs.lock` (the home lock `open` takes first); the temporary `.layerfs.json.<pid>.<n>.tmp` it is renamed from (seen mid-write, or left by a killed process); the folders reserved for integrations, `runtime/`, `job/`, `stat/` and `preset/` ([Top-level names](#top-level-names); `@layerfs/runtime` makes `runtime/` before the first call), each a folder; and macOS's `.DS_Store`. `log/` is not in the set: every `log/*.jsonl` writer runs after the home is open | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) `detect`, `own_only`, `INTEGRATION` |
| Current (3) | `layerfs.json` with `layout: 3` and a 32-hex `id` | same, `LAYOUT_VERSION` |
| Foreign | `layerfs.json` with any other layout number, or one that cannot be parsed: not JSON, no layout number, or layout 3 with no 32-hex `id` (MIG, RC-8); or no `layerfs.json` and anything else in the folder (a reserved name that is a file included) | same, `parse` |
| refused `Failed` | a `layerfs.json` (or a home folder listing) the system refused to read (EACCES, EIO, EISDIR): `unreadable` in the detail, the OS error, no fix; it may be a real 2.0 home | same, `records.rs` `unreadable` |

Every call whose entry opens the home runs `open` first
([`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs));
`home.init` runs it itself, to say what it did. A current home costs one
read of `layerfs.json`; an empty one, one listing of the folder.

| Call | On a foreign folder | Source |
| --- | --- | --- |
| any call that opens the home | refused `Unsupported`, "not a LayerFS 2.0 home; this build writes nothing in it", with the fix `mv <home> <home>.old && layerfs home init`; refused before the home lock, so nothing is written, not even `layerfs.lock` or the call log | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) `foreign`, `foreign_fix`; [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) `append` |
| `home.init` | the same refusal; on an empty home `outcome: "created"`, on a current one `"unchanged"` | [`home.rs`](../../../../control/crates/layerfs-control/src/home.rs) `init` |
| `home.get` | `state: "broken"`, `reason: "outdated"`, the same `fix`; it reads only | same, `get` |
| `home.doctor` | `home.layout` fails with the same fix; `home.records` is not run | [`doctor/host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) `layout` |

The fix keeps the old folder and makes a new, empty home at the path.
Nothing in it is carried over. A worker's root is not a home: `open`
answers the home id its caller sent and writes nothing, and `home.init`
there is refused `InvalidInput`. The root is never created, converted or
cleared
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Finds the home | the call's explicit home, else `LAYERFS_HOME`, else `$HOME/.layerfs`; a relative path is `InvalidInput` | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Names each record's path | one folder per entity kind, one identity per entity (its name); projects nest under their Store, Workspaces under their Sandbox | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| Reads the layout | `layerfs.json` holds `layout` and a 16-byte random `id` written once; any other layout is a foreign home, `Unsupported` ([Opening a home](#opening-a-home)), an unreadable file `Failed`; reading never writes | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) |
| Writes records | one JSON value per file, replaced whole by a rename; bytes as `{"hex": ..}`; entity records carry `"v": 1` first | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) |
| Keeps unknown fields | a typed record reads its known fields and keeps every other field (unknown or mistyped) exactly as read, in file order; a changed field keeps its place, a new one comes last | [`home/fields.rs`](../../../../control/crates/layerfs-home/src/home/fields.rs) |
| Records one intent | a multi-step operation writes `op` before its first effect and drops it with its result in one write | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Checks names | each kind of name is a type checked once by `parse` | [`names.rs`](../../../../control/crates/layerfs-home/src/names.rs) |
| Keeps secrets | saved PostgreSQL logins and bucket keys in `secret/<kind>/<name>.json`, put into an in-memory URL or request per call; worker secret and daemon keys in 0600 files | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs), [`home/secrets.rs`](../../../../control/crates/layerfs-home/src/home/secrets.rs) |
| Says a state | the one state of a machine, Store, project, Sandbox or Workspace: `ready`, `busy`, `stopped`, `blocked`, `broken` or `unknown`, each with its companion keys and no other (`op`, `blocked_by`, `reason` and `fix`, `observed_at`); `State::write` sets them on a result, and `State::blocks` gives what needs a thing its `blocked` state, naming the root of the block | [`state.rs`](../../../../control/crates/layerfs-home/src/state.rs) |
| Counts record changes | `generation`, a number that moves when a record of the home is written or removed ([below](#the-generation-counter)) | [`home/generation.rs`](../../../../control/crates/layerfs-home/src/home/generation.rs) |
| Folds refusal codes | the thirteen public codes and the raised codes each holds; a refusal's `fix` line and the flat fields its next call needs | [`outcome.rs`](../../../../control/crates/layerfs-home/src/outcome.rs) `CODES`, `FOLDS`, `with_fix`, `with_next` |
| Streams events | one event stream per running call: progress (`phase`: `{ phase, done, total }`), a worker's events, command output | [`events.rs`](../../../../control/crates/layerfs-home/src/events.rs) |
| Names this process | pid, kernel start time and boot id, so a reused pid does not keep an `op`, lease or forward alive | [`process.rs`](../../../../control/crates/layerfs-home/src/process.rs) |
| Writes traces and the call log | `LAYERFS_TRACE` lines and `trace/calls.jsonl` | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs), [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| Reads a log by cursor | one window of a log file and the one file it rotates to: its last lines, or the lines after or before a cursor that names the file's identity; shared by `home.log` (identity and cursor text) and the inspect route | [`tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs), [Inspection](../sandboxes/inspect.md#the-cursor) |
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

### Inventory

The relationships no record stores, read when a call needs them
([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs),
in `layerfs-control`):

| Read or check | Answers |
| --- | --- |
| `machines`, `stores_on`, `sandboxes_on`, `hosted_on` | every machine record; the sqlite Stores and Sandboxes of a machine; old Firecracker records a Lima machine hosted |
| `sandboxes_of_store`, `sandboxes_of_project` | the Sandboxes of a Store or project |
| `can_remove_machine` | `machine.remove`: `InUse` while it holds sqlite Stores, Sandboxes or hosted Firecracker records (a PostgreSQL Store is on no machine); next fields `stores`, `sandboxes`, `machines`; held by Sandboxes alone, `fix` `layerfs sandbox remove <s> && … && layerfs machine remove <m>`, none when its record says it is stopped |
| `can_stop_machine` | `machine.stop`: `InUse` while Sandboxes are recorded on it; next field `sandboxes`; `fix` `layerfs sandbox remove <s> && … && layerfs machine stop <m>` (the message names `--force` as the other way) |
| `can_change_machine` | `machine.update`: `InUse` while Sandboxes are recorded on it; next field `sandboxes`; `fix` `layerfs sandbox remove <s> && …` |
| `store_idle` | `store.move`, `remove`, `disconnect`, `backup.restore`: `InUse` while another live process holds a lease or a project's Sandbox is recorded (removes stale leases); next field `sandboxes`; with no live holder, `fix` `layerfs sandbox remove <s> && …` |
| `can_remove_project` | `project.delete`: `Busy` while a Sandbox of the project is recorded |

Each `InUse` or `Busy` lists what holds the entity in `detail.related` as
`{kind, name}`; an `InUse` also names them in the next fields above, flat
beside `error` (ruling R-LF2-1), and a `fix` may join several commands
with `&&`. Layout 3 keeps each Sandbox at `sandbox/<name>/`, so a Sandbox
by name is one read; only the relationships list `sandbox/*`.

### Files

Entries of the [picture](#picture) beyond records, locks and leases. `<root>`
is the home or a worker's root.

| Entry | Written by | Goes | Source |
| --- | --- | --- | --- |
| `layerfs.json`, `layerfs.lock`, `machine/native/`, `machine/docker/` | `home.init`, or the first call of any other kind except `home.get` (which reads a home without opening it), on an empty or missing folder; `docker`'s record carries its `vm_name` | never | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs), [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) `ensure_built_ins` |
| `machine/<m>/forward.log`, `forward.1.log` | the stderr of each detached `ssh -N -L` a `lima` machine's forwards run. Lima's `ssh.config` is not here: it stays in Lima's own instance folder and is only read | with the machine's folder, at `machine.remove` | [`setup/forward.rs`](../../../../control/crates/layerfs-platform/src/setup/forward.rs), [`lima/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/observe.rs) `tunnel` |
| `machine/docker/worker.log.ended` | a copy of an ended worker's log, taken out of its stopped container | in the call that read it | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) `ended` |
| `<home>/run/image-lock/<machine>/<hex>.lock`, `<hex>.json` | one copy of an image on one machine: the `flock` it holds to its end, and its owner record (`pid`, `pid_start`, `pid_boot`, machine, image, since, `since_ms`, and `phase` once the copy names one: `export`, `send`, `unpack`), which `machine.images` reads for a copy's `busy` item, never testing the `flock`; the folder itself is the prune gate, an exclusive `flock` `machine.prune` holds while it runs and a copy passes before it writes | both end with the copy, and the next copy overwrites a record an owner left by dying; `machine.remove` deletes the machine's folder | [`image_lock.rs`](../../../../control/crates/layerfs-sandbox/src/image_lock.rs) `ImageLock`, `PruneGate` |
| `<root>/store/<s>/store.sqlite.incoming` | the receiving side of a `store.move` | renamed to `store.sqlite` at install | [`transfer/steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `store/<s>/backup.lock` | `backup.create`, which holds its `flock` to its end | never; the file stays | [`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) `Creating` |
| `<root>/store/<s>/store.sqlite.backup`; `store/<s>/backup.staging` | `backup.create`: the snapshot staged beside the Store's file, and in the home the copy fetched from a worker | when the create ends | same, [`transfer/steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) `Stage`, `Unstage` |
| `<root>/sandbox/<n>/workspace/<id>.json` | a mount, once it succeeded: `{ v, id, name?, branch, path, namespace, created }`. `id` is the Workspace's 64 hex digits; `namespace` is the daemon's number for it, which names its overlay database `overlay/<namespace>.sqlite` in the run folder | its unmount | [`record/workspace.rs`](../../../../control/crates/layerfs-sandbox/src/record/workspace.rs), [`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs) `token_of`, [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| `<id>.unmount`, `<id>.unmount-lock` | an unmount: who owns it, and its lock | the Sandbox's removal | [`unmount_guard.rs`](../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs) |
| `workspace/<id>/exec/<x>.json` | a command's start | its end; a stale one at the next exec of its id, cancel, unmount or Sandbox removal, with a line in that side's `log/exec.jsonl` | [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs), [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) |
| `/layerfs/data/incoming/init-<16 hex>/` | the parts of a `project.init` folder copy; no lock or lease is taken | every outcome the worker knows | [`project/staged.rs`](../../../../control/crates/layerfs-store/src/project/staged.rs) |
| `/layerfs/state/secret`, `worker.pid` | each worker launch | `/layerfs/state` is a tmpfs on `docker` and goes with each container; on a VM its files are replaced by the next launch | [`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs), [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |

The run root's entries are in [Backends](../sandboxes/backends.md#the-run-folder).
What an interrupted operation leaves of these LayerFS removes itself, at
fixed points in later calls and only where it can prove nothing owns it;
each removal is a line of `log/sweep.jsonl`
([Doctor: leftovers](../doctor/checks.md#leftovers)).

### Where each record lives

| Sandbox on | `sandbox/<n>/record.json` | `control.json` | Workspace and command records | Run folder | Source |
| --- | --- | --- | --- | --- | --- |
| `native` | the home, with `daemon` | the home | the home | the home's `machine/native/run/` | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_args`, [`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs) `save_sandbox` |
| a machine whose daemon port is forwarded here (`lima`) | the home, with `daemon.forward`; the worker's root, with `daemon` | both | the home | the worker's root | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs), [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs) |
| any other machine | the home, without `daemon`; the worker's root, with `daemon` | the worker's root | the worker's root | the worker's root | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `in_worker` |

Machine, project and bucket records are only in the home. A Store's record
is in the home; a worker's root that holds a sqlite Store's file keeps
`{ v, name, kind }` beside it, written only when it changed
([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)
`in_worker`).

### Top-level names

| Names | Whose | Source |
| --- | --- | --- |
| `layerfs.json`, `layerfs.lock`, `generation`, `machine/`, `store/`, `sandbox/`, `backup/`, `log/`, `trace/` | LayerFS's: every path it builds in a home starts with one of them | [`home/paths.rs`](../../../../control/crates/layerfs-home/src/home/paths.rs) |
| `secret/` | LayerFS's saved logins and bucket keys, `secret/<kind>/<name>.json` | [`home/secrets.rs`](../../../../control/crates/layerfs-home/src/home/secrets.rs) |
| `runtime/`, `job/`, `stat/`, `preset/` | an integration's. LayerFS creates none of them; made before the first call they leave the folder a new home ([Opening a home](#opening-a-home)), and neither the home doctor's records scan (four fixed folders) nor a leftover sweep (which never lists the home itself) lists, reads or reports them | [`doctor/scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `scan`, [`sweep.rs`](../../../../control/crates/layerfs-sandbox/src/sweep.rs) |

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
| `sandbox/<n>/serve.lock` | a Sandbox start, Workspace mount or Sandbox removal (the discovery turn) | 60 s, `Busy` | [`project/discovery.rs`](../../../../control/crates/layerfs-store/src/project/discovery.rs) |
| `store/<s>/open/<pid>` | a lease: this process uses the Store | taken under the Store lock | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |

`generation` is a seventh lock file, a leaf: its `flock` is taken last, for
one read and one write, and nothing is taken while it is held
([the generation counter](#the-generation-counter)).

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

### Names outside the records

Every name LayerFS gives a provider or a run folder starts with
`layerfs-<id8>-`, where `<id8>` is the first 8 hex digits of the home's id,
so two homes on one host never share one
([`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)
`provider_prefix`). No name starts with `lfs-`, and none is computed for
a record that lacks one (owner, 2026-11-19).

| Name | Form | Rule | Source |
| --- | --- | --- | --- |
| A machine's VM or worker container | `layerfs-<id8>-<machine>`; each character of the machine name that is not an ASCII letter or digit becomes `-` | recorded as `vm_name` by every `machine.create` and in a new home's `docker` record; `native` has none | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `vm_name`, `built_in`; [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) |
| Its data volume (`docker`, `apple-container`) | `<vm_name>-data` | follows the VM name | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs), [`apple_container/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/observe.rs) |
| A machine record without `vm_name` | none | made by an earlier build: it reads `broken` `outdated`, and every call that needs its VM or container is refused `Failed` (it has no `vm_name`, no call repairs it, nothing outside the home was touched) with the fix `layerfs machine remove <machine>`, which removes the record alone; the built-in `docker` is then recorded again with its 2.0 name by the next call | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `provider_name`, `no_vm_name` |
| A Sandbox's run folder | `layerfs-<id8>-<sandbox>` under the run root, with its private root `<that name>.root` beside it | a recorded Sandbox is used only under the name its record holds, which must be exactly `layerfs-<id8>-<sandbox>` (no prefix is stripped), in the run root this home computes; any other recorded name is refused with the fix `layerfs sandbox remove <sandbox>`, which removes its records alone and names the run folder and daemon process it left; the run root's sweep considers only `layerfs-` names | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `sandbox_name`; [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs); [`force.rs`](../../../../control/crates/layerfs-sandbox/src/force.rs) |
| An unpacked image | `images/<64 hex>.2`; `.2` is the cache layout (file times restored); no other form is read | shared by the run root's Sandboxes | [`rootfs/cache.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/cache.rs) |
| Not prefixed | a Lima data disk `lfs<8 hex>` (a hash of the VM name: its ext4 label `lima-<disk>` holds 16 bytes); a Sandbox's cgroup `/sys/fs/cgroup/layerfs/<container id>`; the image build's temporary names (`lfs-image-*`); a retired docker backend's container, only as its daemon record names it (no name is computed for it) | each keeps its own form | [`lima/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/observe.rs) `disk_name`, [`namespaces/cgroup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/cgroup.rs), [`machine_image.py`](../../../../tools/machine_image.py), [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `retired_container` |

On a worker the prefix is the caller's: each request carries the home's id
and its prefix
([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)
`host_args`).

### Secrets

| Secret | Where it lives | Source |
| --- | --- | --- |
| PostgreSQL password | a saved login (user and password) in `secret/auth/<auth>.json` (0600), named by the Store record's `auth`; put into an in-memory URL on each dial. A PostgreSQL record with no `auth` is refused `InvalidInput` with the fix `layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update <store> --auth <auth>`; no other password reference is read | [`home/secrets.rs`](../../../../control/crates/layerfs-home/src/home/secrets.rs), [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) |
| Bucket key | its id (`access_key`) in `backup/<bucket>/record.json`, its secret in `secret/bucket/<bucket>.json` (0600); a record with no `access_key` is refused `Failed` with the fix that connects it again | [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) `credentials`, [`backup/secret.rs`](../../../../control/crates/layerfs-store/src/backup/secret.rs) |
| Worker secret | `machine/<m>/worker.json` (0600) | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Sandbox daemon keys | `sandbox/<n>/control.json` (0600) | same |
| A daemon's private key and, for a PostgreSQL Store, its URL with the password | `config/daemon.setup` (0600, in a 0700 folder) of a started Sandbox's run folder: under the home for a `native` Sandbox | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) `prepare`, [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) `DaemonStore` |

A copy of a home carries its secrets: every `worker.json` and
`control.json`, each started `native` Sandbox's `daemon.setup`, and every
saved login and bucket key in `secret/`.

### Moving or copying a home

No call moves or copies a home, and nothing rewrites a path a record holds.

| Fact | Consequence | Source |
| --- | --- | --- |
| The home is named by an absolute path on each call | a moved home is used by naming its new path | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| A started local Sandbox's record holds its run folder's absolute path (`daemon.directory`), the run folder's `owner` marker holds the real path of the record's folder, and its `daemon.setup` and `helper.spec` hold absolute paths | a Sandbox started on `native` before a move is refused `InvalidInput` afterwards, "neither used nor deleted": its recorded folder is not the one the new path gives. Close every `native` Sandbox first; a home moves only while nothing in it runs | [`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs) `local_fields`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `mark_owner`, [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs) `local_record_of`, [`helper_spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/helper_spec.rs) |
| A machine's records name its VM, container and volume (`vm_name`) and its worker's loopback endpoint, not a path in the home; a worker's root records paths under `/layerfs/data` | machines, their Stores and their Sandboxes are unchanged by a move | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `provider_name` |
| A record may name a file outside the home: `image_archive` and `daemon` of a machine record | these are neither moved nor copied with it | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) |
| The id in `layerfs.json` is written once and read as it is | a copy has the original's id, so the same `layerfs-<id8>-` prefix and the same recorded `vm_name`s: both homes drive the same VMs, containers and volumes (a `docker` start replaces the worker container of that name and mounts the same volume), and on a machine a Sandbox of one name has the same record and run folder for both | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs), [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) `start` |

### The generation counter

`<home>/generation` holds one decimal number. It moves by one each time a
record of the home is written or removed, and never for a read, so a client
that keeps what `home get --with tree` answered reads the records again only
when `home.get`'s `generation` differs. It is not a version of any record
and orders nothing: two changes may share one read.

| Moves it | Where |
| --- | --- |
| a machine, Store, project, bucket, Sandbox or Workspace record written | `MachineRecord::write`, `StoreRecord::write`, `ProjectRecord::write`, `discovery::save_project`, `BucketRecord::write`, `SandboxRecord::write_in`, `WorkspaceRecord::write` |
| one of those removed | the machine, Store, project, bucket, Sandbox and Workspace removals, and the rollbacks of a create that had published its record |

A write moves it by calling `generation::moved(home)` (or `moved_above(path)`
where the writer holds only a record's path), so a write opts out by not
calling it: `layerfs.json`, secrets, a command's `exec` record, a lease, an
observation a later package may keep in a file do not move it. Under a machine's worker root there is no `layerfs.json`, and
nothing is counted. Counting is best effort under the file's own `flock`:
a count that cannot be written changes no outcome. A reader takes no lock
and reads 0 until the first change.

### Traces and the call log

| Output | Written when | Source |
| --- | --- | --- |
| `LAYERFS_TRACE` file | each connection to a worker appends `{at, event: "worker.connection", op, worker, sent, ms}`; with `LAYERFS_TRACE_STEP` too, one `{at, event: "step", op, step, us, ms}` line per timed step (a worker-routed call asks its worker for its steps) | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) |
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
| `outcome` | `"ok"` or the refusal code as it was raised (`home.log` answers its public code); messages and `detail` are never logged |
| `kind`, `commit`, `layer` | the result's word (a capitalized `kind`, or a migrated call's `outcome`) and the Commit or Layer a saving call saved or names; no other result value |
| `tag` | per-call `log_tag`, else `LAYERFS_LOGS_TAG`; at most 128 bytes, no control characters, else dropped |

Values are cut to 200 characters. Content and Json arguments and every
argument its table entry flags `redact` (today the Text arguments `command`,
`message`, `old`, `new`, `password`, `url`, `pattern` and every path
argument: `path`, `left_path`, `right_path`, `from_path`, `to_path`,
`path_prefix`) are logged as `<N bytes>`; a path counts its normalized form,
one leading `/` stripped (`/a` and `a` are both `<1 bytes>`), and a glob or
grep `pattern` counts its own bytes. A refusal contributes only its code: its message and
`detail` are never logged.

No line is written for a running or killed call, worker requests, a
call's internal parts, commands outside the call table (`help`,
`--version`, `worker`), usage errors or a call whose entry
is `quiet` (`home.get`, `home.log`, `home.clear_log`, `sandbox.log`,
`machine.log`, and the monitoring reads `sandbox.files`, `machine.files`
and `machine.stats`). The six doctors are table calls and log a line. `LAYERFS_LOGS=0` turns the log off for the process; it is
independent of `LAYERFS_TRACE`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A reader sees the old record or the new one, never a part: write to a private temp file, then rename | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) `write_record` |
| Records are private: files 0600, folders 0700. The run root `machine/native/run/` is 0755, as the backends require. Under the namespaces backend (`native` on Linux, and every worker's root) `traversable` adds only o+x to each folder from the run root up to the home or worker's root that lacks it, so a Linux home and its `machine/` and `machine/native/` are 0701: commands run as another identity and must pass, never list. A worker's root and its `store/` and `sandbox/` are made 0755 by its launcher. `home.doctor`'s records check (`home.records`), which scans the home only, accepts both modes: a home folder is an error only with a group bit or with read or write for others (mask `076`); a Store's own folder must be 0700 and a record file 0600 (mask `077`) | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) `private_dir`, `traversable`; [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_args`; [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs); [`doctor/scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `folder`, `store_folder`, `file_mode` |
| A record holds facts and at most one `op`; states such as `running` are derived when read; reading never clears an intent | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Liveness compares pid, kernel start time and boot id, so a reused pid is not the owner; a compaction's recorded owner also compares its PID namespace, so a probe in another namespace never reads it as live | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`process.rs`](../../../../control/crates/layerfs-home/src/process.rs) |
| Lock order is home, start, Store, Sandbox; none is held across a worker call, a VM boot or a byte transfer, except `start.lock` across its start | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs), [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| A stale lease (its process gone) is removed under the Store lock by the next lease taker, or by `store.move`, `store.remove` and `store.disconnect`, and logged to `log/leases.jsonl`; reads leave it | [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs) |
| Only host processes take `layerfs.lock`; a worker's root is never treated as a home | [`home/lock.rs`](../../../../control/crates/layerfs-home/src/home/lock.rs), [`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs) |
| A password never reaches a record: `store.connect` refuses a URL that names a user or carries a password; the record names its saved login (`auth`) only | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `refuse_login`, [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) |
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
| New machine name | at most 46 characters, else `InvalidInput` stating the limit: a VM name is at most 63 characters and `layerfs-<id8>-` takes 17. A machine made earlier with a longer name keeps its recorded VM name | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) `NEW_NAME_MAX`, [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) `plain_name` |
| New `lima` machine name | shorter where Lima's home path is long: Lima refuses an instance whose socket path `<Lima home>/<VM name>/ssh.sock.<16 digits>` is not shorter than 104 characters, so the VM name is at most 76 minus the length of the Lima home (`LIMA_HOME`, else `~/.lima`, links resolved) and the machine name 17 less; else `InvalidInput` stating the longest name that fits on this host | [`lima/limit.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/limit.rs), [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) `provider_room` |
| New machine whose VM name another machine has | `Exists`, before any effect: a VM name keeps no punctuation (`a.b` and `a_b` both give `…-a-b`), and the new name is compared with each machine's recorded `vm_name` (a record without one is skipped) | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) `new_name` |
| Sandbox name ending in `.root` | `InvalidInput`: `<name>.root` in the run root is another Sandbox's private root | [`create.rs`](../../../../control/crates/layerfs-sandbox/src/create.rs) |
| PostgreSQL Store with no saved login (`auth`) | `InvalidInput`, fix `layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update <store> --auth <auth>`; a named login not saved is `NotFound` | [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) `dial`, `read` |
| A folder that is not a LayerFS 2.0 home (a `layerfs.json` of another layout, or anything but `layerfs.lock`, the integration folders and `.DS_Store` with no `layerfs.json`) | `Unsupported`, fix `mv <home> <home>.old && layerfs home init`; nothing written ([Opening a home](#opening-a-home)) | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) `foreign` |
| Call log size | rotates past 16 MiB to the one `calls.1.jsonl` under a non-blocking `flock` on `calls.lock`; `home.clear_log` waits for that lock, truncates in place and removes the rotated file | [`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs) |
| Other log sizes | One rule: past 16 MiB a log moves once to its `.1` name, replacing the one before. `log/*.jsonl` (`leases`, and `exec` on the side that keeps the command records): a writer opens, appends one line and closes, and a log then past the size is renamed to `<name>.1.jsonl` under a non-blocking `flock` on `log/rotate.lock`; a writer that does not get the lock skips it. `log/uncertain.jsonl` never rotates: it is the only record of uncertain outcomes. `machine/<m>/forward.log` is checked when a forward starts: it is copied to `forward.1.log` and emptied in place, never renamed, since the machine's other forwards hold it open ([forwards](../machines/workers-and-reach.md#forwards)). A machine's `/layerfs/data/log/worker.log` is renamed to `worker.1.log` only when a worker starts, never while one runs ([workers](../machines/workers-and-reach.md#the-workers-log)). A Sandbox's `daemon.log` is new with its run folder and goes with it: it is never rotated and there is no `daemon.1.log` ([Backends](../sandboxes/backends.md#local-one-daemon-process-on-this-kernel)) | [`home/records.rs`](../../../../control/crates/layerfs-home/src/home/records.rs) `log_line`, [`log_file.rs`](../../../../engine/crates/layerfs-bridge/src/log_file.rs), [`setup/forward.rs`](../../../../control/crates/layerfs-platform/src/setup/forward.rs), [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) `log_start` |
| `home.log` page | default 200, at most 1000 lines, and at most 64 KiB of rows before it ends early with a cursor; a cursor carries its call and filters (another call's, or one given with different filters, is `InvalidInput`); a cursor after a rotation or clear is stale (`InvalidInput`); a torn line is counted in `skipped` | same |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| `layerfs-control/tests/home_layout.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test home_layout` | macOS, Linux | an empty or missing home (a lone `layerfs.lock` and the temporary included) gets a layout and id; a stray file with no `layerfs.json`, and a `layerfs.json` of layout 1, 2 or 4, is `Foreign`, refused with the fix and left byte-identical; a broken `layerfs.json` is `Failed` |
| `layerfs-control/tests/op_intent.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test op_intent` | macOS, Linux | `Busy`, `Unresolved` and stale intents; the per-entity lock |
| `layerfs-control/tests/typed_names.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test typed_names` | macOS, Linux | each name kind's refusals and texts |
| `layerfs-control/tests/typed_records.rs` | `cargo +1.85.1 test --locked -p layerfs-control --test typed_records` | macOS, Linux | typed records answer their stable shape and read past `"v"` |
| `layerfs-home/tests/json_codec.rs` | `cargo +1.85.1 test --locked -p layerfs-home --test json_codec` | macOS, Linux | base64 and quoted strings round-trip exactly |
| `layerfs-api/tests/logs.rs` | `cargo +1.85.1 test --locked -p layerfs-api --test logs` | macOS, Linux | the call log's lines, rotation and reset |

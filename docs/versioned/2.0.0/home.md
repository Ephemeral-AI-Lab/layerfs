# The home

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Source:
`engine/crates/layerfs-api/src/home/` on branch `issue533-v2`.

The home is LayerFS's control center. Every record a host process needs is
in it: machines, Stores, projects, Sandboxes and buckets. Only host
processes read or write it. No guest (a VM, an ssh host, a container)
mounts or reads it; the caller sends a worker what each call needs and
writes every record itself.

## Where it is

| Setting | Meaning |
| --- | --- |
| `LAYERFS_HOME` | the home folder. Unset, it is `~/.layerfs` (O28) |
| `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | tracing. `LAYERFS_TRACE` names a file: each connection to a machine's worker appends `{at, event: "worker.connection", op, worker, sent, ms}` to it. With `LAYERFS_TRACE_STEP` also set (any non-empty value), each call appends one `{at, event: "step", op, step, us, ms}` line per step it timed: the caller's (`dispatch`, `route`, `connect`, `ready`, `request`, `answer`) and, for a worker-routed call, the worker's as `worker.<step>`, which the worker sends back in its answer (a namespaces `workspace.exec` adds `worker.exec.*` and its helper's `worker.helper.*` stages). `LAYERFS_TRACE_START` is still not read |
| `LAYERFS_CONSTRUCTION_WORKERS` | measurement |

LayerFS reads no other user setting. A Node or Python client can name
another home: `layerfs.client({ home })`, `layerfs.client(home=...)`.

Secret references (`env:NAME`, `file:/path`) name variables or files that
hold a password or a bucket key. They are not settings; see
[machines, Stores and backups](machines-and-stores.md#secret-references).

## Layout

```
<home>/
  layerfs.json                 layout version and home id
  layerfs.lock                 the home lock (host only)
  machine/<name>/
    record.json                kind, size, lasting state, op (while a create, connect or remove runs)
    worker.json                endpoint, secret, live forward (0600)
    known_hosts                ssh machines only
    start.lock                 taken while a process starts, makes or removes the machine
  machine/native/run/          native Sandboxes' local directories, and their image cache
  sandbox/<name>/
    record.json                v, name, project, machine, backend, route, ..., op?
    control.json               the Sandbox daemon's keys (0600; native, and machines reached directly)
    lock                       the Sandbox lock: its record and Workspace record changes
    serve.lock                 held by the call that runs the Sandbox (native, and machines reached directly)
    workspace/<id>.json        one per mounted Workspace: v, id, name?, branch, path, created
  store/<name>/
    record.json                kind, machine, profile, backup links
    store.sqlite               a sqlite Store's file while it is on native
    open/<pid>                 leases of processes that hold it open
    project/<name>/
      record.json
  backup/<bucket>/record.json  provider, bucket, prefix, region, key reference
  trace/                       trace files
  log/                         migration.json, migration-2.json, import-env.jsonl,
                               uncertain.jsonl, init-failures.jsonl, ...
```

- **Records** are one JSON value each, written to a private temporary file
  (named with the process id and a per-process counter, so threads never
  share one) and renamed into place, so a reader sees the old record or the
  new one. Every writer that reads, changes and writes a record holds a
  lock: the home lock where a name is claimed or another entity is checked,
  and the entity's own lock (`store/<s>/lock`, `sandbox/<name>/lock`) for
  its record's other changes. Locks are taken home first, then a machine's
  `start.lock`, then a Store's, then a Sandbox's (#536 §8).
  Files are 0600 and folders 0700. Bytes are stored as `{"hex": "…"}`.
  Every machine, Store, project, bucket, Sandbox and Workspace record
  carries `"v": 1` as its first field; call results never show it.
- **`layerfs.json`** holds `{"layout": 3, "id": "<32 hex digits>"}`. The id
  is written once. The `docker` machine names its worker container with it (O33),
  VM names carry its first 8 digits (`lfs-<id8>-<machine>`), and backups
  record it as where they were exported from. A home of a newer layout is
  refused `Unsupported`; a layout 2 home is converted once
  ([below](#a-layout-2-home)).
- **`layerfs.lock`** guards record writes and Sandbox-name uniqueness.
  Taking it waits up to 120 s for another holder; that is a readiness wait,
  not a retry. If it is still held, the call is refused `Unavailable` and
  nothing changes. A thread that already holds it takes it again at once.
  It is held only for checks and record writes, never across a machine
  start, a Store step, a download or a VM's deletion.
- **`machine/native/` and `machine/docker/`** exist from the first call and
  are never removed.
- **`store/<name>/open/<pid>`** is a lease. Every process that uses a sqlite
  Store holds one: the process that opens it, or, for a Store behind a
  worker, the host process that sends it calls; a PostgreSQL Store's
  in-process calls hold one too. `store push`, `store pull` and `store remove` read the
  leases under the Store's lock and refuse `InUse` while one is held. A
  lease is taken under the Store's lock, and refused `Busy` while a push, pull or
  remove of the Store runs (`Unresolved` after one whose outcome is not
  known). It records its process's start time and boot: a lease whose
  process is no longer alive on this host, or whose id now names another
  process, is stale; a push, pull, remove or disconnect of the Store removes
  it under the Store's lock and logs the removal (O35). Reads (`store
  status`, `store list`, `store get`, `doctor`) ignore it and change nothing.
- **`sandbox/<name>/`** is all a Sandbox has in the home, whatever its
  machine: listing Sandboxes reads `sandbox/*` and nothing else, and one
  Sandbox is one folder read. Its `record.json` holds facts: its name,
  project and machine and, once started, its id, `backend`, kernel `route`
  (`fuse`, or `nfs` on a macOS native Sandbox) and `runtime_id`; and,
  while a `sandbox.create` or `sandbox.close` runs or after one lost its
  answer, one `op` intent (`{id, name, phase, args, owner: {pid,
  pid_start, pid_boot}, started, updated, uncertain}`, #536 §7.1). Its
  state is derived, never stored. Its `lock` is taken after the home lock
  for each change of the record and of its Workspace records, and never
  held across a machine or daemon call; the call that runs a native
  Sandbox takes turns on `serve.lock`, a different file. On `native` the side
  that runs the Sandbox adds what reopens its runtime (`daemon`) to the
  same file, and its daemon keys to `control.json`. A Sandbox on `ssh`
  or `lima` whose daemon the caller reaches directly (#535 G4)
  has its keys in `control.json` too, and a `daemon` of `{ forward:
  { address, machine, pid?, pid_start?, pid_boot? }, name, runtime,
  platform, image }`: the host loopback address the caller dials and, on
  `ssh` and `lima`, the `ssh -L` that carries it, named by id, start time
  and boot. Its Workspace records are then in the home's `workspace/`,
  not the machine's. `runtime_id` and `daemon` are never shown. The
  Sandbox's `id` is its name.
- **`machine/native/run/`** holds each native Sandbox's local directory
  (`<prefix><name>`) and the image cache the namespaces backend unpacks
  once (`images/`). A Sandbox's directory is created by its create and
  deleted by its close.
- **`log/uncertain.jsonl`** lists every outcome that is not known
  (`Unknown`), with what was kept.

A machine's own disk (a VM's data disk, or an ssh host's root) is mounted at
`/layerfs/data` inside the machine. It holds `store/<name>/store.sqlite`
with a thin `store/<name>/record.json` (`{"v":1,"name","kind"}`, written
only when it changed), and `sandbox/<name>/` for the Sandboxes on that
machine (`record.json`: `v`, `name`, `project`, `backend`, `runtime_id`
and `daemon`; `control.json`; `workspace/`), with their local directories
under `machine/native/run/`. On its first start from a layout 3 bundle the
worker deletes what an earlier bundle left there, once, and logs each
deletion: numbered Sandbox folders (`store/*/project/*/sandboxes/`),
`machine/*/sandbox/`, old local directories in
`machine/native/sandbox-root/` (renamed `machine/native/run/`, its
`images/` kept) and full Store records (rewritten thin). The marker
`/layerfs/data/.layout-3` keeps it from running again.

## A new home

The first call on an empty or missing folder creates it with
`layerfs.json`, `layerfs.lock` and the two built-in machines. Run:

```console
$ export LAYERFS_HOME=/tmp/lfs-new
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable",…}
$ cat /tmp/lfs-new/layerfs.json
{"layout":3,"id":"2f51a567b6f153b6e914393cab0904b6"}
```

(Run before layout 3; the answer is the same at layout 3 but for the
layout number.)

## A layout 2 home

A home whose `layerfs.json` says layout 2 (2.0 before #536) is converted
by the first call of this build, under the home lock. It is refused
`InUse` while anything in it is live, naming each in the message and in
`detail.related` (`{kind, name, why}`) with what ends it:

| Refused while | `kind` | Ends it |
| --- | --- | --- |
| a Sandbox's runtime runs (its container or local daemon, observed once) | `sandbox` | `layerfs sandbox close <name>` with the build that started it |
| a Sandbox on a machine behind a worker is recorded with an id (this build does not ask the old worker) | `sandbox` | the same |
| a Sandbox create is in flight (its creating process lives) | `sandbox` | let it finish |
| an earlier create's outcome is unknown (`create.intent`) | `intent` | remove what it left, then the file |
| a Store lease is held by a live process | `lease` | end that process |
| a Store move or remove runs (`in_progress`) | `store` | let it finish |
| a machine removal runs (`removing`) | `machine` | let it finish |

A refused conversion changes nothing (it only appends to
`log/migration-failures.jsonl`); the next call tries again. Otherwise, in
one pass:

1. It writes each Sandbox record from `machine/<m>/sandbox/<name>/` to
   `sandbox/<name>/record.json`, without layout 2's `state` and creator
   `pid` (a 2.0 record names its creator by pid alone, so that is how the
   check above reads it). A Sandbox whose native runtime had ended, or
   whose create's process ended, is written with no id (`gone`); an ended
   runtime is deleted once and its numbered folder removed; `sandbox
   close` then answers `was: gone`. A create whose outcome was `unknown`
   is written with an uncertain `sandbox.create` `op` owned by no process
   (`unresolved`; `sandbox close` clears it). It drops a Store record's
   `in_progress` whose process ended (2.0 did not let it hold the Store;
   layout 3 records hold `op` instead), with a note. It writes `"v": 1`
   into every machine, Store, project and bucket record, dropping a
   machine record's 2.0 intent (a gone remover's `removing`, a `state` of
   `creating` or `unknown`; its `error` stays).
2. It writes `log/layout-3.pending`, then `layerfs.json` at layout 3.
3. It deletes `machine/*/sandbox/` and `store/*/project/*/sandboxes/`,
   renames `machine/native/sandbox-root/` to `machine/native/run/` (its
   image cache kept; any other folder left there is noted, not deleted),
   removes the marker and writes `log/migration-2.json`
   (`from`, `at`, `sandboxes`, `reclaimed`, `versioned`, `removed`, `notes`).

A conversion stopped before step 2 runs again from the start (each write is
idempotent); one stopped after it is finished by the next call, which finds
the marker. Checked by the `home_layout_3` tests on synthetic homes; no
live layout 2 Sandbox was converted for this manual.

## A 1.1.0 home

A folder without `layerfs.json` that holds 1.1.0's own records (`.stores`,
`stores.json`, `stores.v1.json`, `stores.json.lock`, `remotes`,
`remote-state`, `.runtime`, `worker`, `traces`, or a `<p>/project.record`)
is a 1.1.0 home; any other folder, with or without files of its own, is a
new home. The first 2.0 call on a 1.1.0 home converts it once, under the
home lock, to layout 2 and then on to layout 3 (above):

| 1.1.0 | 2.0 |
| --- | --- |
| `.stores/<s>/store.record` and its `projects/` | `store/<s>/record.json` and `store/<s>/project/<p>/record.json` |
| `<home>/<p>/project.record` (a project with a Store of its own) | Store `<p>` holding project `<p>` |
| `stores.json`, `stores.v1.json` | folded into each Store's record (created time; a Docker volume sets the machine to `docker`, unless its file is placed, below) |
| `remote-state/<s>` | the Store's backup link in the bucket of the same name |
| `remotes` | one `backup/<bucket>/record.json` per r2 or MinIO remote |
| `.runtime/init-failures.jsonl` | `log/init-failures.jsonl` |
| `traces/` | `trace/` |

- Every 1.1.0 file it read is moved, unchanged, under `log/1.1.0/`. Nothing
  is deleted.
- An unknown file is left where it is and listed in `log/migration.json`.
- `layerfs.json` is written last. A conversion stopped part way is continued
  by the next call; a second run changes nothing.
- A `local_folder` remote has no 2.0 form. It is reported and kept only in
  `log/1.1.0/remotes`.
- A PostgreSQL Store from 1.1.0 kept no URL. Run `store connect` for it
  again.
- Each sqlite Store's file is placed where 2.0 opens it when this host can
  read it in 1.1.0's Docker volume (`<volumes>/<volume>/_data/global/store.sqlite`,
  Docker on Linux): a hard link at `store/<name>/store.sqlite`, or across
  filesystems a copy whose sha256 is checked before it is renamed in. The
  1.1.0 file is never moved or deleted. A placed Store's record says
  `machine: native` and `file_1_1_0: <path>`, and `log/migration.json`
  lists it under `placed`. A file with a write-ahead log, or one this host
  cannot read (Docker Desktop's VM on macOS and Windows), is not placed:
  the Store keeps its `volume` and the report says where the file is.
- A sqlite Store whose machine 1.1.0 did not record has `machine: null`.
  Calls on it are refused `Unavailable` until `layerfs import-env`
  names its machine.
- 1.1.0 Sandbox folders inside a project's folder are layout 2's numbered
  state: the layout 3 conversion probes their runtimes as above.

Run (a folder with a `traces/` folder and one unknown file; before
`92c2db72b3` added `placed` to the report):

```console
$ LAYERFS_HOME=./old layerfs store list
{"items":[]}
$ cat old/log/migration.json
{"from":"1.1.0","at":1793574185664,"stores":[],"projects":[],"buckets":[],"moved_to_log":["traces -> trace"],"unknown":["notes.txt"],"notes":[]}
```

A conversion with real 1.1.0 Store, project and remote records was checked
by the branch's `home_migrate` tests, not by hand for this manual.

## `layerfs import-env`

`layerfs import-env` writes the current environment's 1.1.0 settings as
records, once, so the home then works with `LAYERFS_HOME` alone. It is a
shell command, not a call of the table.

- It fills only fields that are not recorded yet, and keeps the rest.
- It never writes a secret.
- It never makes a VM or connects an ssh host. For those settings it
  answers the enabled `machine create` command to run. Firecracker and SSH
  settings are reported as ignored with their permanent policy reason.
- Its answer lists `written`, `kept`, `commands` and `ignored`, by setting
  name. Each run is also appended to `log/import-env.jsonl`.

| 1.1.0 setting | What it becomes |
| --- | --- |
| `LAYERFS_DAEMON` | `machine/native/record.json` `daemon` |
| `LAYERFS_IMAGE_ARCHIVE` | `machine/native/record.json` `image_archive` |
| `LAYERFS_DOCKER` | `machine/docker/record.json` `docker` (a leading `unix://` is dropped) |
| `LAYERFS_IMAGE` | `machine/docker/record.json` `image` |
| `LAYERFS_REMOTE_ENDPOINT` and `LAYERFS_REMOTE_BUCKET` (with `_PREFIX`, `_REGION`, `_PART_SIZE`, `_CREDENTIALS`) | a MinIO bucket named `remote`; its key is a reference, default `env:LAYERFS_REMOTE` |
| `LAYERFS_SANDBOX` or `LAYERFS_PLACEMENT` = `docker`, `local` | the machine `docker` for each Store with no machine recorded (and `LAYERFS_VOLUME` as its volume) |
| `LAYERFS_SANDBOX` or `LAYERFS_PLACEMENT` = `native`, `mac`, `linux`, `namespaces` | the machine `native` for each Store with no machine recorded |
| `LAYERFS_SANDBOX` = `vm:<kind>` (with `LAYERFS_VM_CPUS`, `_MEMORY`, `_STORE_SIZE`, `_IMAGE`) | answered: `layerfs machine create <kind> <kind> …` |
| `LAYERFS_SSH_HOST` (with `_USER`, `_PORT`, `_IDENTITY` or `_KEY`, `_KNOWN_HOSTS`, `_STORE`) | ignored: SSH is permanently disabled; no connection command is suggested |
| `LAYERFS_STORE_POSTGRESQL` | ignored: a secret; run `store connect <name> <url> --password env:NAME` |
| `LAYERFS_STORE_POLICY` | ignored: Init uses the default policy |
| `LAYERFS_LOCAL_DIR` | ignored: native Sandboxes live under `machine/native/run/` |
| `LAYERFS_PROJECT` | ignored: every call names its project |

Run (in an empty home):

```console
$ LAYERFS_DOCKER=unix:///var/run/docker.sock LAYERFS_SANDBOX=vm:lima \
  LAYERFS_VM_CPUS=4 LAYERFS_PROJECT=x layerfs import-env
{"written":[{"setting":"LAYERFS_DOCKER","what":"<home>/machine/docker/record.json docker"}],"kept":[],"commands":[{"setting":"LAYERFS_SANDBOX","what":"layerfs machine create lima lima --cpus 4"}],"ignored":[{"setting":"LAYERFS_PROJECT","what":"every call names its project"}]}
```

The old settings, and old Connect and dsh homes, are removed in 2.0; run
`layerfs import-env` once instead (O4, G14).

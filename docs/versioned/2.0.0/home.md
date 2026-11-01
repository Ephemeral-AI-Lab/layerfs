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
| `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | tracing |
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
    record.json                kind, size, lasting state
    worker.json                endpoint, secret, live forward (0600)
    known_hosts                ssh machines only
    start.lock                 taken while a process starts the machine
    sandbox/<name>/
      record.json              name, project, machine, id, platform, state
  store/<name>/
    record.json                kind, machine, profile, backup links
    store.sqlite               a sqlite Store's file while it is on native
    open/<pid>                 leases of processes that hold it open
    project/<name>/
      record.json
  backup/<bucket>/record.json  provider, bucket, prefix, region, key reference
  trace/                       trace files
  log/                         migration.json, import-env.jsonl,
                               uncertain.jsonl, init-failures.jsonl, ...
```

- **Records** are one JSON value each, written to a private temporary file
  and renamed into place, so a reader sees the old record or the new one.
  Files are 0600 and folders 0700. Bytes are stored as `{"hex": "…"}`.
- **`layerfs.json`** holds `{"layout": 2, "id": "<32 hex digits>"}`. The id
  is written once. The `docker` machine names its containers with it (O33),
  VM names carry its first 8 digits (`lfs-<id8>-<machine>`), and backups
  record it as where they were exported from. A home of a newer layout is
  refused `Unsupported`.
- **`layerfs.lock`** guards record writes and Sandbox-name uniqueness.
  Taking it waits up to 120 s for another holder; that is a readiness wait,
  not a retry. If it is still held, the call is refused `Unavailable` and
  nothing changes.
- **`machine/native/` and `machine/docker/`** exist from the first call and
  are never removed.
- **`store/<name>/open/<pid>`** is a lease. Every process that uses a sqlite
  Store holds one: the process that opens it, or, for a Store behind a
  worker, the host process that sends it calls. `store move` and
  `store remove` read the leases under the home lock and refuse `InUse`
  while one is held. A lease whose process is no longer alive on this host
  is stale; it is removed under the home lock and the removal is logged
  (O35).
- **A Sandbox's own state** (daemon keys, mounts) is still kept where 1.1.0
  kept it, under the project's folder; only its 2.0 record is under
  `machine/<machine>/sandbox/<name>/`. The name is the only handle callers
  see.
- **`log/uncertain.jsonl`** lists every outcome that is not known
  (`Unknown`), with what was kept.

A machine's own disk (a VM's data disk, or an ssh host's root) is mounted at
`/layerfs/data` inside the machine. It holds `store/<name>/store.sqlite` and
`sandbox/<name>/` for the Stores and Sandboxes on that machine.

## A new home

The first call on an empty or missing folder creates it with
`layerfs.json`, `layerfs.lock` and the two built-in machines. Run:

```console
$ export LAYERFS_HOME=/tmp/lfs-new
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable",…}
$ cat /tmp/lfs-new/layerfs.json
{"layout":2,"id":"2f51a567b6f153b6e914393cab0904b6"}
```

## A 1.1.0 home

A folder with files but no `layerfs.json` is a 1.1.0 home. The first 2.0
call on it converts it once, under the home lock:

| 1.1.0 | 2.0 |
| --- | --- |
| `.stores/<s>/store.record` and its `projects/` | `store/<s>/record.json` and `store/<s>/project/<p>/record.json` |
| `<home>/<p>/project.record` (a project with a Store of its own) | Store `<p>` holding project `<p>` |
| `stores.json`, `stores.v1.json` | folded into each Store's record (created time; a Docker volume sets the machine to `docker`) |
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
- A sqlite Store whose machine 1.1.0 did not record has `machine: null`.
  Calls on it are refused `Unavailable` until `layerfs import-env` or
  `store move` names its machine.
- Sandbox folders stay inside their project's folder.

Run (a folder with a `traces/` folder and one unknown file):

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
  answers the `machine create` or `machine connect` command to run.
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
| `LAYERFS_SSH_HOST` (with `_USER`, `_PORT`, `_IDENTITY` or `_KEY`, `_KNOWN_HOSTS`, `_STORE`) | answered: `layerfs machine connect ssh <host> …` |
| `LAYERFS_STORE_POSTGRESQL` | ignored: a secret; run `store connect <name> <url> --password env:NAME` |
| `LAYERFS_STORE_POLICY` | ignored: Init uses the default policy |
| `LAYERFS_LOCAL_DIR` | ignored: native Sandboxes live under `machine/native/` |
| `LAYERFS_PROJECT` | ignored: every call names its project |

Run (in an empty home):

```console
$ LAYERFS_DOCKER=unix:///var/run/docker.sock LAYERFS_SANDBOX=vm:lima \
  LAYERFS_VM_CPUS=4 LAYERFS_PROJECT=x layerfs import-env
{"written":[{"setting":"LAYERFS_DOCKER","what":"<home>/machine/docker/record.json docker"}],"kept":[],"commands":[{"setting":"LAYERFS_SANDBOX","what":"layerfs machine create lima lima --cpus 4"}],"ignored":[{"setting":"LAYERFS_PROJECT","what":"every call names its project"}]}
```

The old settings, and old Connect and dsh homes, are removed in 2.0; run
`layerfs import-env` once instead (O4, G14).

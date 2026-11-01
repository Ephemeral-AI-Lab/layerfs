# Machines, Stores and backups

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources:
`engine/crates/layerfs-api/src/machine/`, `src/family/` and `src/store/`,
and `engine/crates/layerfs-sandbox/src/machine/` on branch `issue533-v2`,
with the owner decisions in
[DECISIONS.md](../../../core/docs/issues/533/DECISIONS.md).

Storage is separate from compute (D1). A **machine** is one kernel where
Sandboxes run. A **Store** holds projects and their history. A **backup** is
a copy of a sqlite Store in a bucket.

## Machines

| Kind | How many | Made by | Sandboxes run as | Tool |
| --- | --- | --- | --- | --- |
| `native` | one, built in | — | macOS: host processes with a loopback NFS mount. Linux: namespaces (needs root). Windows: none; `native` holds only records (O5) | — |
| `docker` | one, built in | — | Docker containers (not built in 2.0 yet; see below) | `docker` |
| `apple-container` | many | `machine create` | namespaces inside the VM | `container` |
| `microsandbox` | many | `machine create` | namespaces inside the VM | `msb` |
| `lima` | many | `machine create` | namespaces inside the VM | `limactl` |
| `firecracker` | many, inside a `lima` host | `machine create --host <lima>` | namespaces inside the VM | `limactl` |
| `ssh` | one per remote host | `machine connect` | namespaces on the host | `ssh` |

- "Platform" is only a machine's kind. A caller never chooses how a
  Sandbox runs; the kind decides.
- `machine list` answers every machine and, under `can_create`, each kind
  that could be made here, with `ready`, `missing` (such as `tool:msb`) and
  the tool's `version`.
- `native` and `docker` are never created, removed or disconnected.

### Creating a VM machine

```bash
# not run
layerfs machine create dev lima --cpus 4 --memory 8G --disk 40G
layerfs machine create host1 lima --nested
layerfs machine create fc1 firecracker --host host1
```

- Defaults: 2 CPUs, 4 GiB memory, 20 GiB disk. Sizes are bytes, or text
  with `K`, `M` or `G`.
- Each VM machine has one VM named `lfs-<first 8 digits of the home
  id>-<name>`, one data disk mounted at `/layerfs/data` that holds many
  Stores, and one worker.
- `image` is the image its Sandboxes run in.
- `nested` is for `lima` only: a VM able to host Firecracker machines.
  `firecracker` requires `host`, a `lima` machine made with `--nested`. The
  `lima` machine is `InUse` while it hosts any.
- A create checks the kind's tools first; a missing tool is `Unsupported`
  with `detail.missing`.
- A create needs the sealed Linux bundle (O16): the guest worker, the
  daemon, the Sandbox helper and the Sandbox image, in
  `<cache>/layerfs/bundle/<version>/` with `bundle.json` holding its hash.
  `<cache>` is `XDG_CACHE_HOME`, else `~/.cache`. Without it the create is
  `Unsupported` with `detail.missing` = `bundle:<folder>`. Fetching the
  bundle is not built; a build places it.
- A create that fails before the VM exists removes what it made. Once the
  VM exists, a failure keeps it with state `failed` and its `error`.
  `Unknown` keeps everything and is logged.
- The home lock is held only to claim the name and to record the outcome,
  never while the VM boots.

### Connecting an ssh host

```bash
# not run
layerfs machine connect ec2 10.0.0.5 --identity ~/.ssh/id_ed25519
```

- `user` defaults to `root`, `port` to 22.
- The host's key must already be known. `known_hosts` defaults to
  `~/.ssh/known_hosts` and is copied to `machine/<name>/known_hosts`; it is
  checked strictly.
- `root` defaults to `/layerfs/data`. Its filesystem may be ext4, xfs or
  btrfs (O18).
- A second machine on the same host is `Exists`.
- `disconnect` forgets the host. Its data root and the Stores in it stay.

### Removing and updating

- `machine remove` (VMs) and `machine disconnect` (ssh) are `InUse` while
  the machine holds Stores, Sandboxes or hosted Firecracker machines. Each
  is `InvalidInput` on the other kind.
- `machine update native` and `machine update docker` set `image` and
  `docker` (the Docker socket); `wsl` is a Windows host's setting.
- Resizing a VM with `machine update` is `InUse` while Sandboxes run on it
  and otherwise `Unsupported` ("not built yet"); nothing is changed.

### Start on demand and idle stop

- A machine starts on the first call that needs it. The caller finds the
  worker gone, starts the machine and its worker, waits until it is ready
  and records `worker.json`. That wait is a readiness wait, not a retry;
  the call is then sent once.
- Two processes that need the same stopped machine take turns on its
  `start.lock`. The second waits up to 600 s, then is refused
  `Unavailable`.
- The worker stops after 10 minutes (O2) with no open Store, no Sandbox, no
  call in flight and no call. In a VM guest it then powers the guest off.
  On an ssh host only the worker stops.
- `machine status` answers `state`, `free_disk`, `tool_version` and
  `down_reason` (O34). A stopped VM answers without being started. There is
  no change event; callers poll.

### Routes

- Calls on a sqlite Store on `native`, and calls on a PostgreSQL Store, run
  in the caller's process.
- Calls on a sqlite Store on any other machine go to that machine's worker,
  behind a secret, over a forwarded loopback port. The caller sends the
  Store's record with the call; the worker never reads the home.
- One process can use Stores on several machines at once.
- Not verified: the route table in `layerfs-sandbox` marks `docker` on a
  Linux host as in-process, while the decisions log says every machine
  other than `native` goes to its worker. No Linux host was used on this
  branch.

## Stores

A sqlite Store is one file on one machine at a time. A postgresql Store is
one database. There is no conversion between the two kinds (D2). Every
sqlite Store is Disposable (WAL, `synchronous=OFF`); its record says so.

### sqlite Stores

```console
$ layerfs store create shop          # run
$ layerfs store move shop dev --progress   # not run
```

- `store create` makes an empty sqlite Store on `native`, at
  `store/<name>/store.sqlite`. Its record has `name`, `kind`, `machine`,
  `profile`, `created` and `sqlite` (the SQLite version); `store get` and
  `store list` add `projects` and never show the Store's keys.
- `store move <name> <machine>` moves the file to another machine. It runs
  its phases once each: check, snapshot, send, verify, install, record and
  retire. The bytes pass through the calling process in pieces, so a move
  between two machines other than `native` is one call (O12).
  - It is `InUse` while any process holds the Store, a Sandbox of one of
    its projects runs, or another live process moves it.
  - It is `Exists` when the target already holds a Store file of that name,
    and `InvalidInput` for the Store's own machine or a postgresql Store.
  - A failure names its phase in the message and in `detail.phase`, and
    keeps whatever exists.
  - An `Unknown` step keeps both copies, leaves the record's `in_progress`
    set and is logged to `log/uncertain.jsonl`.
  - While it runs, `store status` shows `state: "moving"` and
    `in_progress`.
- `store remove` deletes a sqlite Store wherever it is, with every project
  in it. Its backups stay in their buckets. It is `InUse` while the Store is
  held.
- Init writes only into a sqlite Store on `native`. To add a project to a
  Store on another machine, move it to `native`, Init, and move it back.
- A Sandbox of a sqlite project runs only on the Store's machine.

### PostgreSQL Stores

```bash
# not run
export LAYERFS_PG_PASSWORD=...
layerfs store connect pg-main "postgresql://lfs@db.example:5432/layerfs" --password env:LAYERFS_PG_PASSWORD
layerfs project init pg-main/load ./load
```

- The URL starts `postgresql://` or `postgres://` and names its user. A URL
  that holds a password is `InvalidInput`; give the password as a
  reference with `--password` (`env:NAME` or `file:/absolute/path`). It is
  resolved on each call and never written to the home.
- `synchronous_commit` is `off` (the default), `local`, `remote_write`,
  `on` or `remote_apply`.
- The Store lives in the fixed schema `layerfs`. An empty database gets the
  schema with a new identity. A database that already holds the `layerfs`
  schema keeps its identity, and its projects become this home's project
  records. A Store name already in the home is `Exists`.
- Not verified: the decisions log (O6) says a database whose one LayerFS
  schema has another name is opened and one with several is refused. The
  connect code names only the schema `layerfs`.
- `store status` adds a `server` field from one connection.
- `store disconnect` forgets the Store; its data stays on the server. It is
  `InUse` while the Store is held.
- `project init` into a PostgreSQL Store runs in the caller's process.
- PostgreSQL needs a build with the `postgresql` feature; without it these
  calls are `Unsupported`.
- Not built: Sandboxes of PostgreSQL Stores, `project remove` from a
  PostgreSQL Store, and backups of PostgreSQL Stores (back it up with
  PostgreSQL's own tools).
- The step 10a and 10b commits record `store connect` and Init into a
  throwaway `postgres:17` container on macOS. Not run for this manual.

## Backups

Backups are for sqlite Stores. Buckets are r2 or MinIO; 1.1.0's
`local_folder` provider is not in 2.0. Each Store has one link per bucket
(O23), so it can be backed up to several buckets.

```bash
# not run
export LAYERFS_BUCKET_LAB_ACCESS_KEY=... LAYERFS_BUCKET_LAB_SECRET_KEY=...
layerfs backup connect lab http://127.0.0.1:9000/layerfs/backups
layerfs backup create shop lab
layerfs backup list lab --text
layerfs backup restore shop lab
```

- **`backup connect <name> <url>`.** The URL is
  `http(s)://<host>[:port]/<bucket>/<prefix>` for MinIO, or
  `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
  `key` is a reference; it defaults to `env:LAYERFS_BUCKET_<NAME>` (the name
  in upper case, `-` and `.` as `_`). `region` defaults to `us-east-1` for
  MinIO and is `auto` for r2. `part_size` defaults to 64 MiB. One listing
  checks the bucket and key before anything is recorded. A name already
  connected is `Exists`.
- **`backup create <store> <bucket> [file]`.** `file` defaults to the
  Store's name. The snapshot is taken on the Store's own machine; its bytes
  pass through the caller's process, so machines never hold bucket keys.
  The object is `<prefix>/<file>/store.sqlite` (the whole file, or a
  manifest) plus pieces `store.sqlite.piece-NNNNNN`, with
  `layerfs-exported-from` metadata. A bucket object that is not the copy
  this home last moved is `Conflict`; nothing is uploaded.
- **`backup restore <store> <bucket> [file]`.** It downloads the file,
  verifies it and installs it as the sqlite Store `store` on `native`: a new
  Store, or one that replaces an idle Store on `native`. Replacing a Store
  that changed since its last backup or restore through that bucket is
  `Conflict`. A Store on another machine is `Unsupported` (move it to
  `native` first). The home's project records are rebuilt from the Store.
- **`backup list [bucket]`.** Every Store file on the connected buckets,
  checked live, each with a `state`:

  | State | Meaning |
  | --- | --- |
  | `current` | linked to a Store of this home, which has not changed since |
  | `store_changed` | linked, and the Store has changed since |
  | `unlinked` | no Store of this home links it |
  | `missing` | linked, but gone from the bucket |

- **`backup disconnect <name>`** forgets the bucket and this home's links
  to it. Its objects stay. LayerFS never deletes a bucket object.
- The step 11 commit records the flow against MinIO (create, the four
  states, `Conflict`, restore as a new Store, a Store behind a worker). R2
  was `NOT_RUN` (no credentials). Not run for this manual.

## Secret references

| Reference | A bucket key | A PostgreSQL password |
| --- | --- | --- |
| `env:NAME` | reads `NAME_ACCESS_KEY` and `NAME_SECRET_KEY`; `NAME` must start with `LAYERFS_` | reads `NAME` |
| `file:/absolute/path` | reads lines `access_key=…` and `secret_key=…`; refused if anyone but its owner can read the file | reads the file's text |

No other scheme exists; there is no keychain (O1). A reference is resolved
at each call and its value is never written to the home. Anyone who can
read the home can make every call.

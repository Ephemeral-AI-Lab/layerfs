# Workers and reach

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a machine's worker is set up and spoken to, and how the caller's
PostgreSQL server and Sandbox daemons are reached across the machine
boundary, within [Machines](README.md).

| | |
| --- | --- |
| Source | [`layerfs-machine/src/worker/`](../../../../control/crates/layerfs-machine/src/worker), [`layerfs-control/src/worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) and [`worker_server/`](../../../../control/crates/layerfs-control/src/worker_server), [`layerfs-platform/src/setup/`](../../../../control/crates/layerfs-platform/src/setup), [`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs), [`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |

A worker is one `layerfs worker` process per machine whose kernel the
caller cannot open a sqlite Store on. It runs the library's host
operations for the requests a caller sends, behind a secret, on a port the
caller reaches on its own loopback. Guests never read the home: each
request carries what its operation needs. Which calls take a worker is the
"Where each call runs" table in [Control](../README.md).

## Setup (`setup/worker.rs`, `guest.rs`, `tool.rs`, `vm.rs`, `artifacts.rs`)

```text
 1 adapter makes or boots the kernel; data disk at /layerfs/data
 2 prepare  mkdir /layerfs/data/{store,sandbox}, /layerfs/state 0700;
            echo 0 > /proc/sys/user/max_user_namespaces, read back;
            echo 1 > /proc/sys/net/ipv4/ip_forward (the guest's own
            namespace: its Sandboxes' networks route through it)
   install  bundle files (layerfs, layerfs-daemon, layerfs-sandbox-helper,
            sandbox-image.tar) copied per file when their hash changed;
            runtime/ (the Sandbox runtime layer) copied whole when
            sandbox_runtime.hash changed or the guest lacks it, then
            swapped in; bundle.json published last
 3 launch   new secret (32 random bytes, 64 hex) -> /layerfs/state/secret
            0600 via stdin; end an earlier worker; start detached as root:
            LAYERFS_WORKER_LISTEN=<bind>:7070
            LAYERFS_WORKER_SECRET=/layerfs/state/secret
            LAYERFS_WORKER_INCARNATION=<the start's ordered token>
            >> /layerfs/data/log/worker.log   (rotated first, see below)
 4 ready    guest /proc/net/tcp listens on the port AND host 127.0.0.1:<port>
            accepts; every 200 ms, at most 60 s
 5 the caller writes worker.json {address, secret, started} and record.json
```

| Step | Failure | Source |
| --- | --- | --- |
| 2 | the guest keeps user namespaces: `Unsupported` | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| 2 | a bundle whose `bundle.json` has no file identities is refused `InvalidInput`, "rebuild it with `python3 tools/linux_bundle.py`"; an unchanged Sandbox image is never recopied; an interrupted copy is never sealed | [`setup/artifacts.rs`](../../../../control/crates/layerfs-platform/src/setup/artifacts.rs) |
| 4 | a worker that ends meanwhile: `Failed` with the last 8 lines of `worker.log`. On `docker` the worker's end stops its container, which `docker exec` cannot enter: the log is copied out of the stopped container (`docker cp`, which reads its data volume) into `machine/docker/worker.log.ended`, read and removed | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |

The `docker` machine does steps 2 and 3 in its own container start (bundle
mounted read-only, entry script makes the folders and the log and writes
`/layerfs/state/worker.pid`; its one sysctl write is `ip_forward` on Docker
Desktop, never on a Linux Engine, whose namespace is the host's);
step 4 is shared ([Platforms](platforms.md#docker-platformsdocker)).

### The start order and the ordered incarnation

`machine.start` holds the machine's `start.lock` and, before launching a
worker, stops the one it recorded and confirms both the provider state and
the recorded endpoint say it is gone. Only then does it mint the ordered
`p1:<home id>.worker:<milliseconds>:<nonce>` incarnation (one past the
previous incarnation's milliseconds when the clock stepped back), write it
into the home `MachineRecord.incarnation`, and pass it to the launched worker
as `LAYERFS_WORKER_INCARNATION`; `machine.create` writes the same field
before its provider creates the worker. The worker records the value in
`worker-incarnation.json` under its root before it serves; a mount intent
carries the incarnation its control caller read from the home record, never
a process-wide value. Values of different homes never order, so on a Store
two homes share another home's `box` never proves this home's `box` mount
dead. After the worker answers, the host reconciles
the Stores on **that machine** with the value only when `worker.ping`
reports the same value; a start that cannot stop the old worker, record the
token or read it back fails before any intent is settled. A home restore
that brings back an older `MachineRecord` therefore stays uncertain: the
reported value differs and the reconcile is skipped, so no live owner is
marked dead. An old worker started without the setting mints its own token
with no home id, which orders against nothing and never matches the record,
so its start skips the reconcile; a record without `incarnation` is no place
proof, never death.

### The worker's log

`layerfs worker` writes its log to its stderr and cannot redirect it
itself, so on every machine kind the launcher's shell appends the worker's
output to `/layerfs/data/log/worker.log` (`$LAYERFS_WORKER_LOG`; folder
0700, file 0600), on the data disk: it stays across restarts and, on
`docker`, across the containers each start makes
(layerfs-project#178 step 0).

```text
 launcher (VM launch script, docker's entry), once no earlier worker runs
   mkdir -p /layerfs/data/log
   worker.log larger than 16 MiB -> mv -f to worker.1.log (replaces it)
   append "layerfs worker: launched <UTC time>"
   start the worker with  >> /layerfs/data/log/worker.log 2>&1
```

| Rule | Detail |
| --- | --- |
| Rotation | only by a launcher, only at a start, only when the log is past 16 MiB. A smaller log is appended to, so a failed start or a restart loop keeps the lines of the run before, and a reader's position in the log holds across restarts |
| While the worker runs | never rotated: a rename would not move the worker's descriptor. The log grows until the next start (start-only is the rule for 2.0). `forward.log`, which a machine's forwards share, is the one log copied and emptied in place, when a forward starts |
| Content | the worker's own lines (`layerfs worker: …`) and a panic's message. Never a request's arguments, the secret, or a PostgreSQL URL |
| `docker` | the container's own output (what the entry says before it becomes the worker) is bounded too: `--log-driver local --log-opt max-size=16m --log-opt max-file=2`, always the three together, since `--log-opt max-size` alone fails `docker run` where the Engine's default driver is journald |

| `Guest` | Host command | Source |
| --- | --- | --- |
| `Lima` | `ssh -F <dir>/ssh.config lima-<vm> -- sudo -n -- …` | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `AppleContainer` | `container exec [-i] <vm> …` | same |
| `Docker` | `docker -H unix://<socket> exec [-i] <container> …` | same |

| Helper | Bound | Source |
| --- | --- | --- |
| `Guest::exec`, `script` | 60 s | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `Guest::copy_in` / `copy_out` (one `tar` stream / one `cat`, never buffered whole) | 600 s | same |
| `tool::run`: runs once, keeps 1 MiB per stream, a timeout is `Unknown` | per call | [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `tool::within`: every command a probe starts ends by its deadline, killed with its process group | caller's | same |
| `tool::detach`: a helper reparented to init, stderr to a log, pid returned | 10 s to start | same |
| `vm::call`, `checked`: one provider CLI call | 30 s; 120 s create; 300 s boot | [`setup/vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| `vm::recorded_port`, `new_port`: the worker's host port | — | same |

### Where the worker listens

| Platform | Binds in the guest | Carried to host `127.0.0.1:<port>` by |
| --- | --- | --- |
| `lima` | `127.0.0.1:7070` | the template's port forward |
| `apple-container` | `0.0.0.0:7070` | `--publish 127.0.0.1:<port>:7070` at create |
| `docker` (macOS) | `0.0.0.0:7070` | `-p 127.0.0.1::7070`, read back with `docker port` |
| `docker` (Linux) | `127.0.0.1:<free port>` | nothing: host network |
| `host` (macOS, Linux) | no TCP port: the host worker listens on `machine/<host machine>/worker.sock`, `writes.sock` and `worker.ctl` ([below](#host-worker-exchange-20-implementation-plan)) | eligible CLI Store reads go to the host worker; other calls run in the caller's process |

Every host-side address is loopback; the secret guards the worker on every
bind address. `layerfs worker` reads its two settings, makes its root
(`/layerfs/data`, or `LAYERFS_WORKER_ROOT` in tests) `0701`, sweeps its run
root of what interrupted Sandbox starts and removals left (once, before it
accepts; [Checks: leftovers](../doctor/checks.md#leftovers)), and once it
has bound prepares its Sandboxes' networks (`prepare_network`): a listen
address that is not loopback is closed to every Sandbox's commands at the
machine's own addresses (its port published on the host is not: [Backends:
host-side forwards](../sandboxes/backends.md#every-namespaces-sandboxs-own-network)), and on
Linux `table inet layerfs` is written for the machine's pool, one log line
either way (a table that cannot be written is logged, and each Sandbox
start then refuses with its own reason;
[Backends: own network](../sandboxes/backends.md#every-namespaces-sandboxs-own-network)).
It serves until `machine.stop`: it never stops by itself
([`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs)).

## Protocol (`layerfs-machine/src/worker/`)

```text
 caller (client.rs)                          worker (worker_server.rs)
   | connect 127.0.0.1:<port>  (<= 10 s)         |
   |--- {secret} -------------------------------->| reads <= 4 KiB, <= 10 s in all
   |<-- {ready:true, protocol, bundle} ----------| or {refused: Denied}
   |    other protocol or bundle: send nothing   |
   |--- {op, args, home_id?, trace_step?} ------->| <= 30 s between bytes
   |         from here nothing is resent         | runs op once,
   |<-- {event} ... (progress, output) ----------|   home = its root; each send
   |<-- {value} | {refused} (+ steps?) ----------|   <= 30 s without progress
   x  closes first, once it has the answer       x waits <= 30 s for that close
```

```text
 plain frame     | len: u32 BE, top bit 0 | JSON |
 attached frame  | 1<<31 | len | json len: u32 | JSON | count: u32 |
                 |   count x (len: u64 BE, raw bytes) |
   each byte value is {"attachment": i}; each attachment named once
   first frame <= 4 KiB (MAX_HELLO); any frame <= 512 MiB (MAX_FRAME)
```

| Item | Detail | Source |
| --- | --- | --- |
| Hello | `{ready: true, protocol: 3, bundle}`; `bundle` is the hash in `bundle.json` beside the executable, `null` outside a sealed bundle | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| Build check | same protocol and same bundle hash as the bundle this build deploys; a worker outside a sealed bundle is checked by protocol alone | same, `Hello::expected` |
| Byte values | travel raw in attached frames, never base64; a plain frame's `{"base64": ..}` still reads as bytes | [`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs) |
| Oversize frame | `TooLarge` from its length, before its body is read; a malformed attached frame `Failed` | same |
| Concurrency | one thread per connection; one request per connection; no cap on connections ([below](#one-connection-never-stops-the-worker)) | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Diagnostic connection | an allowlisted hello and ping (`doctor.sandbox`, `store.diagnose`) under one absolute deadline; never starts a worker | [`worker/diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs) |

| Outcome | Error | Request sent? |
| --- | --- | --- |
| connect, secret or hello fails | `Unavailable` (`Unreached`) | no |
| wrong secret | `Denied` (`Unreached`) | no |
| worker of another build | `Unavailable`, `detail {worker, expected}`, naming `layerfs machine stop` then `start` | no |
| the worker closes before the hello (it had no thread for the connection) | `Unavailable` (`Unreached`) | no |
| request write fails part way | `Unknown`, "it was not resent" | maybe |
| answer lost (the connection closed: the operation panicked, or the caller read nothing for 30 s) | `Unknown`, "it may have run, and was not resent" | yes |
| a read-only operation panicked before its first event | `Failed`, naming `worker.log` | yes |

Only `Unreached` lets `machine start` (and `layerfs_machine::running`)
start a machine, and then only when no worker is recorded or the provider
reports the machine not running ([Lifecycle](#lifecycle)). A ping
waits at most 10 s; other operations wait as long as they run
([`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs)).

## Server (`worker_server.rs`, `worker_server/`)

| Operation | Runs on the machine |
| --- | --- |
| `worker.ping` | `{kind: "worker", version, pid}`; `args.capacity` adds `free_disk` of the root's filesystem |
| `store.call` | one Store method on `store/<name>/store.sqlite` under the root, holding the Store's lease there |
| `store.compact` | one sqlite Store's compaction on `store/<name>/store.sqlite` under the root, holding the Store's lease there; `{ store, store_record }`, each phase sent as a `progress` event; an event that cannot be written does not end the run (spec S6, #42 D5) |
| `store.upgrade` | one sqlite Store's format upgrade in place on `store/<name>/store.sqlite` under the root (#804): `{ store, store_record, op }`; it closes the worker's own open copy, is refused `InUse` while another process holds the Store's lease under the root and `Busy` while a compaction runs there, then holds the lease for the run; each phase (`check`, `convert`, `verify`, `commit`) is sent as a `progress` event as it starts, and an event that cannot be written does not end the run. Its cancel flag is registered under `op` until the run ends |
| `store.cancel` | sets the cancel flag of the running `store.upgrade` named `{ op }` and answers `{}`; `NotFound` when none of that id runs there. The host sends it once on a second connection, after the run's first phase came back, while the first connection keeps reading the answer (no reliance on half-close through a port forwarder); a cancel at or after `commit` changes nothing |
| `store.format` | read-only: `{ store }` → the `format` section (`version`, `latest`, `status`, `fix`) of the file under the root from its header on a raw read-only connection, `status: unknown` when it has no such file; for `store get/list --with format` |
| `store.file` | one step of a `store.move` (either direction), `store.remove`, stat or backup, with `progress` events for hashing steps |
| `store.reach` | the check of this machine's connection to a PostgreSQL Store with the address and login the host saved (or a new one `store.connect` checks before saving it), for `store.attach`, `store.connect` and `store.doctor`; a refusal names its connection `step` |
| `store.diagnose`, `doctor.sandbox` | doctor's read-only observations |
| `network.inspect` | doctor's read-only observation of the machine's Sandbox network for `machine.network`: `{ table, forwarding, links: [{ link, addresses, sandbox }] }`, `sandbox` the recorded Sandbox the link is named for, or null |
| `sandbox.host` | a Sandbox or Workspace host operation on the machine's own kernel (its guest root, `namespaces` backend), decoded into its typed request first; a `sandbox.create` sweeps the run root first ([Checks: leftovers](../doctor/checks.md#leftovers)) |
| `inspect` | one read-only inspect request `{ kind, .. }` (a window of `worker.log` or of a Sandbox's `daemon.log`, overlay file sizes with the Sandbox's process and usage, the machine's stats, which Sandboxes' daemons run) on a helper thread, two at a time, answered within 6 s; the operation adds `worker: { version, pid, started_at, protocol, bundle }` to the request ([Inspection](../sandboxes/inspect.md)) |
| `project.stage`, `project.init`, `project.export`, `project.delete`, `project.delete.admit`, `project.delete.entry`, `project.delete.release` | write one part of a folder copy; add it as a project; stream a pinned snapshot back; admit, mark, remove the entry of and release a project's deletion |
| anything else | `InvalidInput`, "the worker has no operation …" |

The worker uses its own root as the home and the Store file under it as
the Store's path, whatever the caller sent. The root is not a home:
`home_id` lends it the caller's id
([Home](../home/README.md)).

### One connection never stops the worker

A worker serves every Store and Sandbox call of its machine, and on
`docker` it is the container's main process. A failed accept, a panic or a
peer that stalls ends at most its own connection (layerfs-project#177).
Nothing here is a retry: no request is read twice and none is resent.

```text
 accept loop (worker_server.rs)             one thread (worker_server/connection.rs)
   accept ---- ok ----> thread ---- ok ---->  hello: all of it <= 10 s
     |                    |                   request: <= 30 s between bytes
     | EMFILE ENFILE      | the OS has no     catch_unwind( run op; events )
     | ENOBUFS ENOMEM     | thread: log,        each send <= 30 s without progress;
     | ECONNABORTED EINTR | drop the stream     first failed send: nothing more
     |   log, 50 ms,      v                     is written, both sides closed
     |   accept again    (caller: Unavailable,  answer, then wait <= 30 s for
     v                    nothing was sent)     the caller's close
   any other error (EBADF, EINVAL, ENOTSOCK): log, serve returns, the worker exits
```

| Rule | Detail | Source |
| --- | --- | --- |
| Accept | the six errors above say nothing about the listener: one log line, a 50 ms pause, the next `accept`. Any other error ends `serve` (`layerfs worker` then exits "the worker stopped") | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs), `passing` |
| Thread | `std::thread::Builder::spawn`; when the OS has none, the stream is dropped before its hello, so the caller is `Unavailable` and sent nothing | same, `Server::spawn` |
| Log | `layerfs worker: …` on stderr, which its launcher appends to `/layerfs/data/log/worker.log` ([above](#the-workers-log)). At most one line per kind of accept failure, and one for threads, every 10 s; the next line of a kind says how many went unlogged | same, `Limited` |
| Hello | one deadline of 10 s (`HELLO_WAIT`) for the whole first frame, however many reads it takes; a peer that trickles bytes is closed at it | [`worker_server/connection.rs`](../../../../control/crates/layerfs-control/src/worker_server/connection.rs), `Until` |
| Write | a write timeout of 30 s (`REQUEST_WAIT`) on every accepted stream. It bounds one send that makes no progress, not a frame: a caller that still reads is never cut | same, `exchange` |
| Closed writer | after the first failed or timed-out send, every later event fails at once without touching the socket, no answer is written, and the stream is shut down so the caller reads `Unknown` at once | same, `Wire` |
| Stalled caller | `store.file` and `sandbox.host` ignore the failed event and end their step; `project.export` returns at its next part, closes the Store and releases its lease; the thread then ends | [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs), [`export/route.rs`](../../../../control/crates/layerfs-store/src/project/export/route.rs) |
| After the answer | the worker waits at most 30 s for the caller's close, once | `Wire::answer` |
| Panic | `catch_unwind` around each connection. One line `<op> panicked: <message>` (300 characters at most; never arguments, a secret or a request), then the connection closes with no answer: `Unknown` (layerfs-project#179 D7) | `connection::serve` |
| Panic, read-only | `worker.ping`, `network.inspect`, `machine.images`, `store.reach`, `store.format`, `doctor.sandbox`, `store.diagnose` and `inspect` answer `Failed` instead, only if no frame was started after the request | `ops::READ_ONLY` |

**No connection cap, and no bound on frame memory** (decided in
layerfs-project#177). A cap must make a connection wait, never refuse it
`Busy`: `project.init` sends four parts at once and a refused part fails
the Init. But a waiting cap would put new calls behind commands that run
for hours, each of which holds a connection, so one slow command could hold
up every Store call of the machine. What bounds a connection instead:

| Peer | Holds | For at most |
| --- | --- | --- |
| before the secret matches | one thread, one descriptor, 4 KiB | 10 s |
| with the secret, sending its request | one thread, one frame of up to 512 MiB | 30 s without a byte |
| with the secret, reading nothing | one thread, what its operation holds | 30 s without progress |
| past the worker's descriptor limit | a place in the listen queue | until a descriptor is free |

Frames past the hello come only from a process that holds the secret: the
home's owner. Its largest ordinary load is four 8 MiB parts at once, so a
byte budget would guard only against the owner's own process; it is not
added. If one is, it waits as a cap would.

Mutexes a panic could poison (a poisoned lock used to refuse every later
call, "binding state poisoned"). Each is recovered (`into_inner`), because
its holders leave it whole at every point they can panic:

| Mutex | What a holder does with the guard | Decision |
| --- | --- | --- |
| `SANDBOXES` ([`registry.rs`](../../../../control/crates/layerfs-sandbox/src/registry.rs)) | one map lookup, insert or remove per guard; a replaced or removed Sandbox may be dropped there (`try_wait`, closing connections) | recovered, also where [`force.rs`](../../../../control/crates/layerfs-sandbox/src/force.rs) takes it directly: the map operation is complete before anything is dropped |
| a Sandbox's `mounts` (same) | one lookup, insert or remove, or the whole map replaced after a refresh built outside the guard | recovered: entries are plain values |
| `OUTPUTS` ([`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs)) | insert, remove, or look up and clone a file handle | recovered |
| `Controls.pool` ([`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs)) | pops or pushes an idle connection, changes `open` or `limit`; dialling runs outside the guard | recovered in `lend` as its `Lease` drop already did; a failed connection is now closed after the guard is released |
| `STORES` ([`service.rs`](../../../../control/crates/layerfs-store/src/service.rs)) | `acquire` opens the Store under the guard (I/O that may panic), then inserts it; counts change in one step | recovered: a panic while opening leaves no entry |
| discovery `HELD` ([`discovery.rs`](../../../../control/crates/layerfs-store/src/project/discovery.rs)) | set lookup, insert or remove, and the bounded wait | recovered at its third site as at the other two |
| `OPEN` ([`open.rs`](../../../../control/crates/layerfs-store/src/open.rs)) | — | already recovered; unchanged |

The first three and `STORES` are taken through `layerfs_home::helpers::lock`,
which is now the lock of such a table and never refuses. Recovery does not
undo what a panic leaves outside a guard: a Store that `with_service` opened
stays counted as open, and a pool slot counted before a dial stays counted.

The fault tests use the `worker-fault` cargo feature of `layerfs-control`
(`worker_server::fault`: the next accept or thread fails, an operation
panics, hangs or floods events, shorter waits). No shipped build turns it
on and no environment variable stands in for it; the boundary guard allows
it in the worker server's files alone and refuses a manifest that enables it.

### Lifecycle

| Event | Effect | Source |
| --- | --- | --- |
| `machine.start` | a recorded worker that answers its secret counts as running (one of another build is refused `Unavailable`, recorded `outdated`); else, under `start.lock`, a recorded worker whose hello failed makes the provider be asked of the VM alone (`provider_within`, 10 s): a machine it reports running is recorded `broken` `not_answering` and refused `Unavailable`, `fix: layerfs machine stop <m> && layerfs machine start <m>` (`--force` while Sandboxes are recorded on it, the message naming how many and `layerfs machine doctor <m>`), since a start would end that worker and every Sandbox on it; one it cannot say of is refused `Unavailable`, fix its doctor. With no worker recorded, or a machine stopped, failed or missing, the start runs and checks the new hello (a stale bundle is `Unavailable`, naming `python3 tools/linux_bundle.py`) | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| any other call | dials `worker.json` once; none recorded or no answer is `Unavailable` naming `layerfs machine start`; nothing is started; later steps of one operation reuse the endpoint | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `machine.stop` | refused `InUse` while Sandboxes are recorded; stops worker and VM, removes `worker.json` | [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| idle | nothing: a worker has no idle stop | [`worker/mod.rs`](../../../../control/crates/layerfs-machine/src/worker/mod.rs) |
| `machine.get --with live` (and `machine.list --with live`) | the provider is asked of the VM alone, then one hello to the recorded worker within 2 s: a running kernel whose worker does not answer (or with none recorded) is `broken` with `not_answering`, one of another build `outdated`; a bare connect is not enough, since a forward accepts with no worker. The platform's own `status` (`setup/calls.rs`, hello `{"ready":true}` within 2 s) says the same for its other callers: `Status::reason` is `not_answering` for a running kernel whose worker does not answer, never `stopped` | [`live.rs`](../../../../control/crates/layerfs-machine/src/live.rs), [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `hello_within`, [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |

## Reach (`reach.rs`)

A PostgreSQL Store's URL is the user's configuration: LayerFS dials it
exactly as configured from every machine, rewrites no part of it and reads
no port (owner, 2026-11-24).
`reach(spec, store, url)` answers that URL or refuses it; it starts nothing,
runs no provider command and resolves no name, and no message holds any
part of the URL but a loopback host. Each adapter declares its `Loopback`.

```text
 dial(spec, (store, url), adapter.loopback(spec))
   Shared ---------------------------------------------> url as given, unread
   Apart(this host):
     hosts = url_hosts(url): the host list and host= / hostaddr= parameters,
             read as the client reads them (none: the socket)
     not a URL the client reads -----------------------> InvalidInput
     a host empty, or a path (a Unix socket) ----------> Unsupported + advice
     a host is this host's loopback -------------------> Unsupported + advice
     else ---------------------------------------------> url as given
   advice: "connect the Store again with an address the machine reaches
            (<this host>): `layerfs store connect <store> <url>`", the
            Store named
```

| Platform | `Loopback` | The refusal names |
| --- | --- | --- |
| `host` | `Shared` | — |
| `docker` | `Shared` on a Linux host (`--network host`); `Apart` on Docker Desktop | `host.docker.internal` |
| `lima` | `Apart` | `host.lima.internal` (192.168.5.2) |
| `apple-container` | `Apart` | the gateway the last start recorded, else `192.168.64.1`; the server must listen on it, not on 127.0.0.1 alone |

| Rule | Detail | Source |
| --- | --- | --- |
| Hosts, as the client reads them | The daemon's client is tokio-postgres (`Config::from_str`), and `url_hosts` reads a URL as it does: the userinfo ends at the first `@` anywhere; the host list ends at the first `/` or `?` and splits at each `,` before a host's `[...]` or first `:` is read (`h1:5432,h2`, `h1,[2001:db8::1]` and `[::1]:5432,h2` are valid lists); an empty host (`h1,,h2`, `host=`) is a name whose lookup fails, so it is left out; hosts, query keys and `hostaddr` values are percent-decoded (`%6Cocalhost` is `localhost`, `h%6Fst=` is `host=`); a host decoding to a leading `/` is a socket directory (`@name` is a name). A `host=` value the client takes as one host is split at `,` too, so no part of it goes unchecked. Only `postgres://` and `postgresql://` URLs reach here, each an address alone with the login put in front: a Store's address and a machine's own are dialled only when they hold no `@` or `#` and the client reads them with no login, and the URL made is read back by the client before it is dialled (`layerfs_store::postgresql::address::with_login`), so its one `@` ends the user part for every reading; what is not such a URL, or what the client refuses to read (an unclosed `[`, a parameter with no `=`, a host that does not decode to UTF-8), is `InvalidInput` | [`support/postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/support/postgres_url.rs) `url_hosts`; [`address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs) `with_login` |
| Loopback | `localhost` (any case, with or without its final dot); every address of 127.0.0.0/8 and the unspecified `0.0.0.0`, in every form the machine's `getaddrinfo` reads through `inet_aton` (`127.1`, `2130706433`, `127.000.000.001`, `0x7f.1`, `017700000001`); `::1` and `::` in any spelling, with or without brackets or a zone id (`::1%lo`), and their IPv4-mapped forms; in any host the URL names | same, `loopback`, `ip` |
| Host parameters | `host=` and `hostaddr=` are checked like the URL's hosts, then dialled as given | same, `url_hosts` |
| TLS | No host is rewritten, so a certificate's name still matches. The client reads `sslmode` `disable`, `prefer` or `require` only, and `require` verifies the server's certificate chain and host name (the engine's `native-tls` connector, with its defaults); `verify-full`, `verify-ca`, `sslrootcert`, `sslcert` and `sslkey` are refused at `store connect` (`refuse_login`, naming the one met) and never dialled | `reach.rs`; [`address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs) `refuse_login` |
| A name resolving to loopback | Not resolved to be refused: a hosts entry that maps a name to 127.0.0.1 on the machine fails at its one dial (the create's reach check, `store.reach`, the create's daemon), and that failure carries the refusal's advice (`loopback_hint`, sent to the worker as `reach_hint`). The hint is added only on an `Apart` machine, only when the URL names at least one host by name and gives no `hostaddr` (an address is read, never resolved, every loopback one was refused, and with a `hostaddr` the client dials that address alone, `url_hostaddr`), and only when no connection was made (`not_connected`: the client's `error connecting to server` or `timeout waiting for server`, a silent dial, the session's `no connection to the server was made`; for a create's daemon, its last line saying `cannot reach PostgreSQL Store`, read by `launch/local.rs` `unreached`), never when the server answered (a refused login, a missing database, any error it sent). One attempt: nothing redials | [`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) `loopback_hint`, [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) `not_connected`, [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `reach_check`, [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `from_machine`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `unreached` |
| Names in the Sandbox | A `namespaces` Sandbox resolves names with its machine's resolver files, bound into its root at start ([Backends](../sandboxes/backends.md)); a remote host is reached through the machine's own NAT and DNS | [`helper root.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/root.rs) `resolver` |
| Per call | Only the operations that dial the Store's URL on their side build it in the caller (`Op::store_url`): `sandbox.create` (its daemon and reach check), Commit (its Branch's head and `expected`) and a Workspace status (its head). The worker process reads the Store per call, and the password travels only in that one request; building it is a record read and this check, no provider command. A close resolves it only to end the mount intents a forced unmount left. It runs without it only when the machine cannot dial it (`platform::reach` refuses it): its side then hands the intents bound by id back in `ending`, and the caller, where PostgreSQL Store calls run, ends each once with the Store's own URL (`mount_intents::end_handed`); one not ended is a `left` sentence. Any other failure to build it (a password this home does not hold, a record naming no user, an unreadable machine record) refuses the close with its own message and fix, forced or not. The handed intents live only in the close's answer: a lost answer (`Unknown`), a failed removal of the Sandbox's folder after stop and delete (`Failed`) or a caller that ends before `end_handed` leaves them, and a retried remove answers `AlreadyGone` with an empty `left` (S1 of the 3cf2cfc246 review; no record holds them). Every other operation (`sandbox.get`'s reads, mount, exec, cancel, unmount) asks only the Sandbox's daemon, which dials the URL its create gave it, and never reads the URL: a Sandbox whose URL its machine cannot dial can always be read and removed, while a password this home does not hold refuses its close | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_op`, `in_worker`; [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs) `store_url`; [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `close`; [`mount_intents.rs`](../../../../control/crates/layerfs-sandbox/src/mount_intents.rs) `end_handed` |

`url_hosts` keeps no userinfo and nothing here puts any part of the URL in a
message but a host
([`support/postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/support/postgres_url.rs)).

## Forwards

A Sandbox daemon listens on `127.0.0.1:<guest_port>` in its machine.
`forward(spec, guest_port)` answers the host loopback address at which this
process reaches it, so Workspace calls skip the worker
([`setup/forward.rs`](../../../../control/crates/layerfs-platform/src/setup/forward.rs)).

```text
 host                                       machine guest
 Workspace call -> 127.0.0.1:<free>         Sandbox daemon 127.0.0.1:<gp>
                     | ssh -N (detached, pid kept)          ^
                     +======================================+
 ssh -N -o ExitOnForwardFailure=yes <opts> -L 127.0.0.1:<free>:127.0.0.1:<gp>
```

| Platform | Answer |
| --- | --- |
| `host` | `127.0.0.1:<guest_port>`, no pid |
| `lima` | a free host port through Lima's `ssh.config` with `ControlMaster=no`; the `ssh` pid |
| `docker`, `apple-container` | `Unsupported`: "the <platform> machine <name> forwards no Sandbox daemon port: …"; Workspace calls stay on the worker |

| Step | Bound |
| --- | --- |
| port 0 refused `InvalidInput`; pick a free host port; `tool::detach` the `ssh`, stderr appended to `<work_dir>/forward.log`, which the machine's forwards share. A log past 16 MiB is first copied to `forward.1.log` and emptied in place: it is never renamed, since the other forwards hold it open and would go on writing into the renamed file (a line one of them writes between the copy and the emptying is lost) | 10 s to start |
| wait, then check the pid runs with `127.0.0.1:<port>:` on its command line; dead is `Failed` with the log's last 3 lines, not retried | 600 ms |
| `forward_close`: only if the marker is still on the pid's command line; SIGTERM, then SIGKILL | 3 s |

## Related

- [Machines](README.md): the adapter that runs step 1.
- [Sandboxes: daemon channel and exec](../sandboxes/daemon-channel-and-exec.md): who dials a forwarded daemon.
- [Stores and projects](../stores-and-projects/README.md): `store.call`, `store.file` and `project.*` routes.
- [Stores and projects: SQLite and PostgreSQL](../stores-and-projects/sqlite-and-postgresql.md): the password inside the URL.
- [Bridge](../../engine/bridge/README.md): the daemon wire contract.

## Host worker exchange (2.0 implementation plan)

The host exchange is versioned independently of the guest exchange. The
ordinary socket is `worker.sock`, the streamed write socket is
`writes.sock` and the management socket is `worker.ctl`, in the host
machine directory. Each listener has its own admission capacity. A
saturated ordinary or write listener leaves its backlog in the kernel,
while management retains two slots.

Both peers check the other's effective UID before sending or reading frames.
The server sends the first frame only after admission to the connection
queue. It is a plain bounded JSON frame with `ready: true`, `kind: "host"`,
`protocol: 2`, `home_id` (the current home's record ID), `generation`
(a fresh worker incarnation token), and `build`. The build combines the
running CLI's embedded source identity and the executable's filesystem
identity. The client validates the home, protocol and build before sending
a request, including when its install path has since been replaced.
Each request uses the guest envelope's `op` and `args` keys and adds a
`generation` echo and a `request_id` correlation token. A final answer
echoes the request ID and has exactly one of `ok` and `error`.
`ping`, `store.close`, `store.postgresql_retire` and `drain` are
management operations. `store.call`, `store.postgresql_read` and a
single-change `store.write` are ordinary operations; a streamed
`store.write` is the write socket's only operation, and each socket
refuses the other's writes `Unsupported`. The host resolves Store authority from its own home
records.

`store.postgresql_read` (#819 P-B) carries exactly `method`, `store`,
`project`, `fields`, `login_revision` and `password`, and serves only its own
three read methods (`fs.read`, `fs.stat`, `fs.list`). An ordinary thread
reads it and hands it to the worker's PostgreSQL lane (8 threads, at most
64 handed over and unanswered), so a PostgreSQL read holds no ordinary
thread or slot. A lane thread starts the oldest queued read whose Store has
a free session slot, so a read of a busy Store waits in the queue on no
thread and never holds a thread another Store could use. Queued reads whose
caller closed its socket are dropped unrun. At 64 the ordinary thread
answers `Unavailable` (`postgresql-reads-full`) at once, unrun: the answer
follows the request it read, which §8.5 allows because nothing ran. The worker reads no secret from its environment or the home: it puts
the sent password into the record's own address for the sessions of that
Store and overwrites the copies it owns once they are no longer needed
([password lifetime](../stores-and-projects/sqlite-and-postgresql.md#held-sessions-in-the-host-worker-819-p-b)). It sets `RLIMIT_CORE` to 0 before it
reads its first request, and on Linux clears `PR_SET_DUMPABLE` as soon as
it decodes its first PostgreSQL request, before any session holds the
password (a worker that served none stays readable by its user, which
proves its cleared environment); macOS has no such flag. `store.postgresql_retire` (`{ store, revision }`, answered
`{ closed }`) retires that login revision's held sessions unless it is the
record's current one; `store connect` and `store disconnect` send it, past
their locks, to a running worker only, never starting one. The ping
answers `postgresql { stores, idle, lent, reads, queued }`.

The v1 host request and answer bodies are plain JSON with byte ceilings of
64 KiB and 1 MiB respectively. Hello and management requests are limited to
4 KiB. Check ceilings before reading a frame body and reject attachments on
request and answer frames. A deadline-bounded receive reads any reply bytes
buffered before the peer closed. An ordinary call waits for the first reply
byte for the duration of the Store operation and bounds the transfer from
that byte onward. That wait has no deadline by design (spec §8.5, §11): a
Store call has no time cap (a long `file.grep` is legitimate), the exchange
has no cancel message, so a client deadline would turn a live call into
`Unknown` without stopping or freeing it, and nothing may be resent. A worker
that exits or crashes closes the stream, so the caller sees `Unknown` at once;
only a live worker keeps it waiting, as an in-process SDK call would. The
caller can still be interrupted by its own caller. The guest exchange keeps
its existing framing and hello.

Each side has its own hello deadline (§11). The worker writes the hello
within 10 s of a pool thread taking the connection, not of its accept, so a
connection that waited in the queue is still greeted. An ordinary caller
connected to a running worker waits up to 30 s (`CALL_HELLO_WAIT`) for its
hello: behind 8 busy pool threads, in the 16-slot queue or in the kernel's
listen queue. 30 s is three times the start deadline; it lets the §16.1 C7
load (20 callers, small calls) and a queue behind calls of ordinary length
complete. A caller still waiting at 30 s (all 8 threads held by long calls,
or a stopped or wedged worker) gets a pre-send `Unavailable` naming the load
("no call thread was free within 30 s"), so nothing is half sent. Only a
caller that spawned the worker it reached keeps the startup lock to its
hello and the 10 s deadline, as do management calls and connecting. A
caller that took the startup lock while the listen queue was full, or while
another worker's socket was not yet private, and started nothing releases it
once connected and waits as any queued caller does: 30 s from its connect,
refused naming the load. A worker that exits
closes every waiting connection, also before anything was sent.

The worker counts only ordinary calls toward its idle clock: a `ping` from
doctor or `machine get --with live` observes it without keeping it alive
(§8.3, §12). It is idle after 60 s without a call and with nothing in flight,
including connections accepted but not yet admitted and held writes. It
then first removes every socket name under its lifetime lock, so no new
caller can join a listen
queue, keeps serving anything already queued (a `connect` that resolved the
name just before is in the queue within microseconds; the worker keeps reading
for 1 s, `UNPUBLISHED`), and drains only when it is quiet with every listen
queue empty. A caller after the removal finds no endpoint and can start the next
worker only once this worker releases `worker.lock`. The old worker keeps
that lock while it serves a call accepted in its last 1 s window and while it
closes and checkpoints its Stores, so the next start is not guaranteed within
the new caller's 10 s start deadline: that caller can get `Unavailable` with
nothing sent, and maintenance meanwhile gets `InUse` naming the old worker's
pid. No caller is reset mid-queue by an idle exit.

A `store.write` request names its method, Store, project, field map and
`body`: `none` (`edit`, `mkdir`, `rm`, `cp`, `mv`), `content` (`write`) or
`changes` (`apply`). Content and changes never travel in the request. A
streamed body follows the request on the same connection until one end
frame:

| Frame | Header | Body | Ceiling |
| --- | --- | --- | --- |
| change | top bit clear | `{"change": {...}}`, the Store's form of one change | 64 KiB |
| spilled change | top bit clear | `{"spilled": true}`, then the change's JSON text as content | 64 KiB, then data frames |
| wait | top bit clear | `{"wait": true}`: the caller's input is still coming | 64 KiB |
| end | top bit clear | `{"end": {"changes": n, "bytes": b}}` | 64 KiB |
| data | top bit set | content bytes; an empty data frame ends the content | 256 KiB |

A streamed write in an apply is its change with `streamed: true`, then its
data frames. The worker reads a frame only when the Store call asks for its
next change or piece, so a faster caller blocks in the kernel socket buffer.
A data frame is at most four engine records (4 × 65,519 = 262,076 bytes,
about 256 KiB), so whole pieces are written as whole records.

Spool. The caller streams only regular files. An input that is not one (a
pipe, a FIFO, a terminal) is first copied in 64 KiB pieces into an unnamed
private file: created 0600 and exclusive in the host machine's `spool`
folder (`machine/<host>/spool/`, 0700; an existing folder that is not this
user's or not private is not used), named `<pid>-<nanoseconds>-<n>`, and
unlinked at once, so no exit, a signal's included, leaves it behind (the
spooler also removes a name whose process is gone). The copy takes disk
equal to the input, on the home's filesystem, until it is sent: the CLI
closes the file once its input thread has read the last byte for sending
(or, on the direct route, once it was read whole), not when the answer
comes. So the worker always reads at disk speed: a slow or interactive
producer is never timed out by the worker's progress rule. The request
header is checked (`request_fits`) before the copy. If the file cannot be
made or written (a full disk, say), the part already read and the rest of
the input run direct, as without a worker. There is no free-space check
before the copy: a copy that fits can leave the Store too little room for
the Commit, which is then refused `Capacity` before any effect (a known
residual risk on a nearly full disk, accepted in round 6 of #819).

Memory. A content's size does not change it: the caller holds at most
`QUEUED` + 2 = 4 pieces of 262,076 bytes (about 1 MiB): two queued by its
input thread, the one that thread is reading and the one being sent; the
worker one frame. `file apply` changes are the exception. The caller's input
thread builds each list element whole from the input and converts it to
the Store's form by moving its values; changes pass to the sender through a
rendezvous channel, so the caller holds one change being read, one being
sent and that change's printed text (within the budget below, at most
about 1.4 MB). A write change's content is part of its change, so the
caller's worst case is about the two largest consecutive changes, contents
included, plus 1.4 MB. A write's content then leaves its change and
travels as data frames. Any other change is printed once and must fit the
worker's budget for one change, the frame budget: a decoded-size estimate
of 4 MiB (three bytes per text byte plus 256 per container, field and
element), 128 fields per object and 32 levels, measured on the change
frame the worker decodes, `{"change": …}` included (one more level and
545 more units than the change alone, so a spilled change, decoded without
it, fits too). A change larger than a 64
KiB frame is sent spilled; the worker builds it from its data frames under
that budget, holding one frame and the value, and refuses one past it
`InvalidInput` before any effect. The caller checks before sending
anything: it reads the list once (from its regular file or its spool),
holding one element at a time and measuring it without printing it, and,
if any change is past the budget, routes the whole call direct (a route,
never a refusal). A caller that sends an over-budget change anyway is
refused by the worker before effect. Object fields are checked for
duplicate names by sorting, O(n log n) in the width. The caller keeps
sending `wait` while it parses a large element, since parsing runs on its
input thread.

The worker's decoded request memory, per lane, in the frame budget's
decoded-size estimate (`B` = 4 MiB, `DECODED_BUDGET`; a request frame is
at most 64 KiB, `HOST_MAX_REQUEST`, and a hello or management frame 4 KiB):

| Lane | What it holds decoded | Bound |
| --- | --- | --- |
| ordinary | one request per accepted connection, 16 slots (`ORDINARY.slots`) | 16 × B = 64 MiB |
| management | 2 slots of 4 KiB frames | 2 × B = 8 MiB (the frame budget; such a frame decodes to far less) |
| PostgreSQL | reads handed over and not yet answered, 64 (`POSTGRES.slots`), each with its request | 64 × B = 256 MiB |
| write | 2 intake threads reading a request (2 × B = 8 MiB); held headers, at most `HOST_HELD_WRITES` = 32, each checked before it is held: 5 envelope fields and at most 12 named fields, all scalar, so at most 3 × 65,536 + 256 × 34 = 205,312 bytes (32 × 205,312 = 6,569,984 bytes, 6.3 MiB); one change being built per write thread, framed or spilled (4 × B = 16 MiB) | 31,735,808 bytes, 30.3 MiB |
| total | | 358.3 MiB |

These bound the requests and changes the worker decodes, not a call's own
work (an answer up to 1 MiB, a Store's pages). S2.2 does not change the
PostgreSQL lane; its 256 MiB is its bound as it stands, since a read is
handed over before its request's shape is checked.

A write needs a Commit per call, not a held connection. The CLI knows a
write's route before it dials. A single-change write (`body` none) goes
to the ordinary socket and runs on the ordinary thread that read it, as a
read does. A streamed write goes to the write socket, `writes.sock`
(same run folder, same peer UID check and hello as the others): one of 2
intake threads reads its request and checks its request ID, generation
and header (method, body, field names and scalar values) there, and then
holds it, with its connection, for a separate write lane: 4 write threads
(`WRITE_THREADS`). So however many writes are held, they take no ordinary
place or thread, and small reads keep all 8 ordinary threads. Calls wait
(§11): the write socket accepts only while its connections and held
writes together are below `HOST_HELD_WRITES` (32), so further writers
wait in its listen queue and, past their 30 s hello bound
(`CALL_HELLO_WAIT`), get only the hello's `Unavailable` naming the load
("no write place was free within 30 s"), before they send anything. A
held write waits for a write thread with nothing sent but its request:
its caller sends input only after the worker's `go` frame
(`{"request_id", "go": true}`), sent when a write thread starts reading
it, and waits for it without a time bound, so a queued write is never
timed out. A refusal instead of `go` is definite. A waiting write whose
caller went away (a killed CLI) is dropped unrun by the same check the
PostgreSQL lane uses (`peer_gone`), when a write thread looks for work
and, while the write socket is full, at least every 5 s (`IDLE_CHECK`), so
its place goes to the next writer. After `go` the caller's sends have no
time bound either: the worker is reading, and its Store work may take as
long as a direct call's. A worker that is stuck before `go` or before the
input's end leaves the CLI waiting with nothing effective sent: nothing
commits before the end frame, so Ctrl-C is safe. A write read once the
worker drains, or still waiting for a thread then, is refused
`Unavailable` before any effect.

Minimum progress (§11): once the worker reads a write's input, the input
must deliver 262,076 bytes (`STREAM_PROGRESS`, four engine records, frame
headers included) within 30 s of waiting (`STREAM_WAIT`), or end. Only
time the worker spends blocked on the input counts, never its own Store
work or the wait for a write thread. A `wait` frame is not progress, so a
caller that sends keepalives but no input, or slower input, is refused
`Unavailable` ("delivered less than 262076 bytes (four engine records) in
30 s of waiting; nothing was committed"). The CLI streams from a regular
file and copies only pipes, FIFOs and terminals, so a regular file on a
stalled filesystem (a hung network mount) can still meet this rule; so
can a misbehaving caller. A caller whose input
produced nothing for 10 seconds sends `wait`, which only shows the worker
is still there. The end frame's counts must match what was received. The
worker commits only after reading it, so input that stops earlier commits
nothing. A change refused mid-stream answers at once and closes; the
caller, whose next send fails, reads that early refusal. The request's
frame and the end frame bound the custody boundary. Before the end frame
is completely written the caller reports a definite refusal; after it, a
missing or invalid answer is `Unknown` with the request ID.

Drain (`machine stop`, idle exit) never stops an admitted write (§8.3): a
running write reads its whole input and finishes as admitted, and the
worker exits after it. Held writes not yet running are refused unrun. The
drain answer counts the admitted writes still running (`writes_running`),
never one taken and then refused by the drain; a drain asked again while
draining answers the same account. `machine stop` does not
wait for them: it reports `running_writes` and `running_writes_note` ("N
writes still running; see worker.log") and, while a running write holds
its Store, `InUse` naming the worker as a holder.

Records (§8.5). Only an outcome the caller may not know is recorded: an
engine `Unknown`, and a `Committed` answer the worker could not deliver.
A definite refusal changed nothing and is never recorded. An engine
`Unknown` is written to `machine/<host>/retained/<request id>.json` before
its answer is sent, and its answer carries the request ID; the record's
detail carries the engine's phase (a stable name: `admission`, `capture`,
`begin`, `construct`, `finish`, `validate`, `publish`, `install`), whether
it published, its attempt, expected head and candidate root. A record is
written to a `.part` file and linked into place, so a reader never sees
half a record and an existing record (an `uncertain` one) is never
overwritten by a later `undelivered`. Records are not synced (the home's
Normal profile; a power loss may lose one). A record names the call,
Store, project, Branch, path and outcome, never content.

Each record write is one attempt; nothing waits on it or retries it. One
that fails is logged to `worker.log` without content. An uncertain outcome
without a record still reaches its caller: its answer is the `Unknown`
with the request ID, the engine's detail and `recorded: false`. The worker
counts such outcomes (`unrecorded` in its ping) and lists the first 32 in
the answer of the drain that ends it (`unrecorded`, `unrecorded_count`),
which `machine stop` passes on as `unrecorded_writes` and
`unrecorded_writes_count`. The list covers outcomes up to the drain; a
write still running then (see `running_writes`) reports its own outcome to
its caller, and one with no record only to `worker.log`. An idle exit has
no caller and only the log.
Drain, idle exit and a Store's close never wait for a record, so `machine
stop` cannot be refused by one; a `machine stop --force` goes on without
its worker's answer.

At most 256 records are kept (`RETAINED_CAP`). A new one at the cap first
removes the oldest `undelivered` records, which only inform: a killed CLI
whose Commit landed adds one. An `uncertain` record is never removed by the
worker; when they alone fill the cap, a new record fails as above (logged
and reported). Drain, stop, crash and replacement leave the records;
`machine.worker.retained` reports them, names the folder and gives `rm
<home>/machine/<host>/retained/*.json` as its fix, once their Branches
were checked. There is no call that removes them.

A scoped apply (`file apply --scope`) runs in the caller's process, not
through the worker: its Workspace route is not a host Store write.

A separate startup lock serializes launch attempts until the child is
ready. Only the worker holding the lifetime `worker.lock` may recover stale
sockets, bind or remove endpoints. A worker binds each socket and then sets
it to 0600; callers never dial a socket with another mode. Such a socket
whose lifetime lock is free has no owner (its worker died between bind and
chmod), so a starting caller treats it as absent and launches a worker that
replaces it, and a caller that does not start one reports no endpoint.
The CLI starts the worker with an empty environment except `LAYERFS_HOME`
and `PATH`, `HOME`, `TMPDIR`, `LANG`, `LC_ALL`, `LC_CTYPE`, `SSL_CERT_FILE`
and `SSL_CERT_DIR`, in `/`: the worker serves later callers too, so it
takes no caller's secrets (such as `PGPASSWORD`), per-call settings or
directory (spec P-B). A started worker holds that lock
through listener shutdown, Store close, and socket removal. Incompatible
builds receive a definite refusal before operation bytes, with idle exit as
the replacement path. Transport loss after an operation send begins has
outcome `Unknown` with the request ID and no automatic replay.

## Host power and admission

Host start and stop serialize on the machine lifecycle lock. The built-in
host's record defaults to enabled. Stop atomically records stopped under
the home lock used by Sandbox creation and worker socket publication.
Each SQLite lease claim and reused worker Store checks the machine record
under its Store lock. Maintenance operations check the same fence before
recovering an old intent or recording their new one. After fencing admission, stop drains the worker,
closes the calling process's own idle copies of the machine's Stores (as a
move's settle does, before any record lock) and inspects holders under each
Store lock: other processes' leases and a copy a call in the calling
process is using still block it. A copy that fails to close, or a Store
whose holders cannot be read, is a holder naming the failure, so the
answer stays `InUse` with `forced_outcome` attached after `--force`. This ordering lets earlier leases
finish and exposes the process identities still holding the Store.
Force uses the existing per-Sandbox stop with its recorded runtime identity.
The machine remains stopped when a holder or teardown refusal is reported.

Lifecycle calls recheck the machine intent after acquiring the lifecycle lock.
An explicit forced stop can reconcile an uncertain stop from the same or
a terminated owner. Other unresolved operations keep their custody.

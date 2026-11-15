# Inspection

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The route of the reads a client polls to watch a machine and its Sandboxes:
one worker operation, `inspect`, with bounded admission and deadlines, within
[Sandboxes](README.md).

| | |
| --- | --- |
| Source | [`layerfs-sandbox/src/inspect`](../../../../control/crates/layerfs-sandbox/src/inspect), [`layerfs-sandbox-backend/src/local/confined.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/confined.rs) with `os/{linux,macos}/confined.rs`, [`layerfs-home/src/tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs), [`layerfs-store/src/place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `on_machine_read`, `Worker::read`, [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `send_within`, [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) |
| Used by | [API: `logs.machine`, `logs.sandbox`](../../api/sdk/logs.md#logsmachine), [`sandbox.files`](../../api/sdk/sandbox.md#sandboxfiles), [`machine.stats`](../../api/sdk/machine.md#machinestats), [`sandbox.list`](../../api/sdk/sandbox.md#sandboxlist) with `live`, [`sandbox.get`](../../api/sdk/sandbox.md#sandboxget) of a forwarded Sandbox |

## Picture

```text
 call (logs.machine, logs.sandbox, sandbox.files, machine.stats, sandbox.list live)
   |  inspect::ask(home, machine, kind, args)
   v
 on_owner = place::on_machine_read     the machine's record and worker record,
   |                                   read as files: no intent check, no lock
   +-- Executor::Local  (native) ----> admit -> serve(Side::Home(home))
   |
   +-- Executor::Worker -------------> worker op "inspect" { kind, .. }
         one connection, one attempt     |  place::READ: connect 2 s, hello 2 s,
         no answer in time: Unavailable  |  answer 8 s
                                         v
                                 in the worker: admit -> serve(Side::Worker(root))

 admit (owner.rs), per process:
   2 permits ---- none free -------------------------> Busy, at once
   helper thread "inspect" owns the permit until it ends
   answer within 6 s (HARD) --- else ----------------> Unavailable, "abandoned";
                                                       the helper keeps its permit
   a panic in the helper ----------------------------> Failed
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| One route | A request is `{ kind, .. }`. `ask` runs it in this process for a machine whose calls run here, and sends it once as the worker operation `inspect` otherwise; a Sandbox whose daemon the home reaches directly (`lima`) is asked through its machine's worker too, where its run folder is. | [`inspect/route.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/route.rs) `ask`, `in_worker` |
| Kinds | `machine_log` (`log/worker.log` and `worker.1.log` under the worker's root), `sandbox_log` (`daemon.log` in the Sandbox's run folder), `sandbox_files` (a Sandbox's roots, a folder page or a file window: [Files](#files)), `overlay_sizes`, `machine_stats`, `sandboxes_live` (the three below). Another kind is `InvalidInput`. | `serve`; [`log.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/log.rs), [`files.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/files.rs) |
| `overlay_sizes` | `{ sandbox, namespaces }` to `{ overlay_sizes: [{ namespace, bytes }], daemon, usage, usage_reason? }`: one `lstat` per Workspace overlay file, and the Sandbox's daemon process and cgroup usage, which ride the same answer (`namespaces` may be empty). What a get reads beside the run folder, here for a forwarded Sandbox. | [`overlay.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/overlay.rs), [`run_folder.rs`](../../../../control/crates/layerfs-sandbox/src/run_folder.rs) `observed` |
| `machine_stats` | `{ stores: [name], sandboxes: [name] }`, the sqlite Stores and Sandboxes the home records on the machine, to `{ observed_at, worker, resources, data_disk, stores, sandboxes: { live }, complete, reason? }`: processors and memory from the kernel, one `statvfs` of the side's root, one `lstat` of each Store's `store.sqlite` and `-wal`, and the liveness check below. The home adds the machine's platform, the `scope` of each measure, the provider's volume name and the recorded count. | [`stats.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/stats.rs), [`host.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/host.rs) |
| `sandboxes_live` | `{ sandboxes: [name] }` to `{ sandboxes: [{ name, live, reason? }], complete, reason? }`: each Sandbox's daemon record under the side's root, and whether the process it names is that process (pid, start time, boot) and runs. No record or no daemon record is `live: false`. | [`live.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/live.rs), [`process.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/process.rs) `live` |
| Who the worker is | The worker's `inspect` operation puts `worker: { version, pid, started_at, protocol, bundle }` on every request it serves; `machine_stats` answers it. On `Side::Home` there is no worker and it is `null`. | [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) `about` |
| A Sandbox's process and usage | `daemon: { pid, started_at }` from the daemon record (`started_at` only while the process runs). `usage` from four files of `/sys/fs/cgroup/layerfs/<container>` for a `namespaces` Sandbox (`cpu.stat`, `memory.current`, `memory.peak`, `pids.current`), each value `null` where the file is absent; `usage: null` with `usage_reason` `host_process`, `cgroup_v1` or `no_cgroup` otherwise. | `process.rs` |
| Find the run folder | The `directory` of the daemon record in `<root>/sandbox/<name>/record.json` on the side that runs the Sandbox, read as a file. It is used only when it is a folder directly under that side's run root; a missing record or folder is `NoSuchSandbox`. | `run_folder` |
| Admit | Two permits per process (`PERMITS`). A request with none free is `Busy`. The work runs on a helper thread that owns the permit until it ends, so a read stuck in the kernel holds one permit and one thread and nothing else. | [`owner.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/owner.rs) `admit` |
| Deadlines | Soft 4 s (`SOFT`): `Budget::spent()` is what a kind made of many units checks between them (a folder page looks before each entry and every 1024 names, and then answers what it has with `has_more: true, reason: "deadline"`). Hard 6 s (`HARD`): the caller is answered `Unavailable` and the helper is left to end by itself. The caller of a worker waits at most 8 s for the answer (`place::READ`), then reports `Unavailable`; a read changes nothing, so it is never `Unknown`. | `owner.rs`; [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `READ`, `Worker::read` |
| Panic | In the helper: `Failed`, "an inspect request panicked and changed nothing", and the permit is free. On the worker's connection thread before any frame: `Failed`, as every read-only operation (`ops::READ_ONLY`). | `admit`; [Workers and reach](../machines/workers-and-reach.md#one-connection-never-stops-the-worker) |
| Read a log | One window per request: the last lines, the lines after a cursor, or the lines before one, over a file and the one file it rotates to. At most 64 KiB is read per request; files are opened `O_NOFOLLOW | O_NONBLOCK` and must be regular. | [`tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs) `read` |

## The cursor

```text
 c1.<file>.<inode hex>.<first-line hash hex>.<offset hex>
      0 = the current file, 1 = the rotated one

 worker.1.log  [ older lines ............... ]   offset
 worker.log    [ newer lines ..... | held-back partial line ]
                                   ^ cursor of a tail read
```

A cursor names a byte offset in one file of the pair and that file's
identity: its inode and a hash of its first line. A read with a cursor whose
file is no longer that file (rotated, replaced, shorter than the offset) is
`InvalidInput`, "the cursor is stale"; the caller reads again without one. A
backward read that reaches the start of the current file goes on in the
rotated file; a forward read that reaches the end of the rotated file goes on
in the current one. A current file that does not exist yet has the cursor
`c1.0.0.0.0`, which reads the file from its start once it is there.

A line longer than the window is answered in pieces, each `{ text,
continued: true }` but the last; windows are cut on UTF-8 boundaries, so the
pieces join to the line. Control characters are written as Rust escapes
(`\t`, `\u{1b}`) and bytes that are not UTF-8 as U+FFFD.

## Files

`sandbox.files` is served by one walker
([`files.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/files.rs)),
written for the views that follow it (a machine's folders, a Sandbox's whole
root). A view is a `Scope`: a base folder, the path its entries are shown
under, the mount points beneath it, and whose hard links are read.
`files::answer(scope, relative, request, spent)` serves a folder page or a
file window in any of them.

```text
 sandbox.files s1                      roots::sandbox
   workspaces  <- Workspace records    never stat'ed: kind "mount"
   home, log   <- stat in the run folder

 sandbox.files s1 <home>/a/b           files::sandbox_page
   under <run folder>/tmp ? ----- no -----------------> InvalidInput
   Scope::open: Base (the home's descriptor), mount table
   answer:
     every folder above the path:  a mount point? kept private? -> InvalidInput
     the entry, stat'ed in its folder:  mount, restricted, log,
                                        link, FIFO, socket, device -> InvalidInput
                                        a file with several names
                                        its commands do not own    -> InvalidInput
     confined open, then stat of the descriptor
       folder -> page     file -> window
```

### A folder page

```text
 depth 2, limit 4, cursor <base>/a/x          tree order:  a  a/x  a/y  b  c  c/q

 folder <base>   cursor names [a, x]
   enter a  (it is still a folder)            names of a after "x":   y
   then the names of <base> after "a":        b, c         keep limit - listed + 1
     b   stat, list
     c   stat, list, enter (depth 2): names of c:  q
       q  stat, list            page: a/y  b  c  c/q    next: <base>/c/q
```

- Names are in byte order and a folder's entries follow it, so the list is
  the tree a client draws. The cursor is the path of the last entry
  answered: a position in that order, not in a directory stream.
- A page reads the names of each folder on the cursor's path once
  (`readdir` on the folder's descriptor, no `stat`), keeping in a heap the
  smallest names it still has room for after the cursor's, and one more:
  the extra name is how a full page knows whether anything follows
  (`has_more`). It then `stat`s only the entries it answers. A page costs
  the names of those folders, at most `limit` `stat`s and `limit` names of
  memory. Reading a whole folder of n entries page by page reads its names
  n / `limit` times.
- A walk goes on from its cursor whatever changed: an entry that is gone is
  skipped, a cursor whose entry or folder is gone resumes after where it
  was, a new name after the cursor is listed and one before it is not.
- An entry is judged before it is `stat`ed: a mount point named by the
  mount table is `mount` with its `fs` and is never `stat`ed; so is an
  entry of a run folder's `workspaces/`; what `roots::kept` keeps is
  `restricted`. Every other entry is told from its own `stat`: `file`,
  `folder`, `symlink` (with `target`), `other`. A folder is entered only
  when it is a `folder`.

### What is kept private

`roots::kept(root, path)`
([`roots.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/roots.rs))
is the one place a path is judged against LayerFS's layout; a read asks it
of the path and of each folder above it, a listing of each entry.

| Kept | Paths | Shown as |
| --- | --- | --- |
| `Restricted` | `store.sqlite*` anywhere (a Store's database and its companions). In a run folder: `config/` (the daemon's private key) and `overlay/` (the Workspaces' databases). Elsewhere under the side's root: `control.json`, `worker.json`, `lock`, `*.lock`, a Store's `open/` (leases) | `restricted`: name, `size`, never entered or read |
| `Log` | a run folder's `daemon.log`; `log/worker.log` and `worker.1.log` | a `file` with `role: "log"`; read by `logs.*` only |
| `Workspace` | what is under a run folder's `workspaces/` | `mount`, never `stat`ed |
| `Open` | everything else; a run folder's `tmp/` is its commands' own, so a `Cargo.lock` there is a file | by its `stat` |

### The platform split

Only three things differ by platform, in
[`layerfs-sandbox-backend/src/local/confined.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/confined.rs)
and its `os/` files: the confined open, the `stat`, and the mount table.

| | Linux | macOS |
| --- | --- | --- |
| Open under the base (`Base::open_at`) | one `openat2` from the base's descriptor: `O_RDONLY \| O_NONBLOCK \| O_NOFOLLOW \| O_CLOEXEC \| O_NOCTTY` with `RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS \| RESOLVE_NO_MAGICLINKS \| RESOLVE_NO_XDEV`. A kernel before 5.6 is `Unsupported`; nothing else is tried. `View` is where another resolve mode goes | one `openat(O_NOFOLLOW \| O_NONBLOCK)` per name; each folder on the way on the base's device (`st_dev`); the last descriptor's path (`F_GETPATH`) under the base's |
| After the open | a `stat` of the descriptor: only a regular file or a folder is answered; a FIFO opens at once and is refused | the same |
| `stat` of an entry (`Opened::stat_of`) | `newfstatat` with `AT_SYMLINK_NOFOLLOW \| AT_NO_AUTOMOUNT \| AT_STATX_DONT_SYNC` (the kernel takes the `statx` query flags there since 4.11), `AT_EMPTY_PATH` for a descriptor | `fstatat(AT_SYMLINK_NOFOLLOW)`, `fstat` for a descriptor |
| Mount table (`confined::mounts`) | `/proc/self/mountinfo`, read as a file | none yet: an entry on another device than its folder is `mount`, known after its `stat` |

Two parts of the design are not built, because no dependency of
`layerfs-sandbox-backend` wraps them without `unsafe`: the `statx` system
call itself on Linux (and so `STATX_MNT_ID`), and `getfsstat(MNT_NOWAIT)`
on macOS. Until the second is there, a mount under a macOS Sandbox's home
whose server does not answer can hold a read until its hard deadline: the
request is then `Unavailable` and its helper keeps one permit.

## Invariants

| Rule | Enforced by |
| --- | --- |
| An inspect request takes no lock or lease, opens no Store, asks no daemon, starts nothing and writes nothing, on the home and on the machine. | the module uses none of `registry`, `lease`, `open::hold`, `daemon::pool` or `route`'s host operations; [`logs_inspect.rs`](../../../../api/crates/layerfs-api/tests/logs_inspect.rs) and [`machine_stats.rs`](../../../../api/crates/layerfs-api/tests/machine_stats.rs) compare every file of both sides before and after |
| A Store file is sized by `lstat` of its path and never opened: closing a descriptor on it would drop its owner's locks. | `stats.rs` `store_bytes` |
| A kind made of many units answers what it has at the soft budget: `complete: false`, `reason: "deadline"`, the units not reached `null`. | `live::checked`, `stats::serve`; [`inspect_stats.rs`](../../../../control/crates/layerfs-sandbox/tests/inspect_stats.rs) |
| It is not refused while an operation holds the machine or a Store. | `place::on_machine_read` skips the intent check `on_machine` makes |
| At most two run per process; a third is `Busy` at once. | `owner::admit` |
| A stuck or panicking request never stops the worker's other operations. | the helper thread; [`worker_fault_inspect.rs`](../../../../control/crates/layerfs-control/tests/worker_fault_inspect.rs) |
| Only `worker.log`, `worker.1.log` and `daemon.log` are read as logs; never `log/uncertain.jsonl`. | `log.rs` |
| A record cannot point a read outside the run root. | `run_folder` |
| A files read stays under its base: no link is followed, on the way or at the end, wherever it leads; no other mount is entered; a `..` is refused before any system call. | `confined::Base::open_at`; [`confined.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/confined.rs) |
| Only regular files and folders are opened for an answer; a FIFO is never waited on. | every open is `O_NONBLOCK` and followed by a `stat` of the descriptor |
| A Workspace mount is never `stat`ed, opened or read through; nor is a mount point the mount table names. | `roots::sandbox`, `roots::kept`, `Scope::class` |
| A Store's database, an overlay database, a key, a lease or a lock is never opened. | `roots::kept`, asked for a path and for each folder above it |
| A file a command linked into its home from elsewhere is not read. | `Scope::own_link`: a regular file with several names not owned by the record's numeric command uid |
| `logs.machine`, `logs.sandbox`, `sandbox.files` and `machine.stats` write no call-log line. | the dispatcher skips the `logs` family and a `quiet` call, which `sandbox.files` and `machine.stats` are ([Calls](../../api/calls/README.md#the-call-log)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Requests at a time | 2 per process (`PERMITS`), else `Busy` | `owner.rs` |
| Request time | soft 4 s, hard 6 s, then `Unavailable` | `owner.rs` |
| Caller's wait for a worker | connect 2 s, hello 2 s, answer 8 s (`place::READ`), then `Unavailable` | `place.rs`, `worker/client.rs` `Budget` |
| Window | 64 KiB of file per request (`CHUNK`); 200 lines by default, 1000 at most | `tail.rs` |
| `overlay_sizes` request | at most 1024 namespaces, else `InvalidInput` | `overlay.rs` |
| Folder page | 500 entries by default, 1000 at most; 1 level by default, 32 at most | `files.rs` `ENTRIES`, `MAX_ENTRIES`, `DEPTH`, `MAX_DEPTH` |
| File window | 256 KiB by default, 1 MiB at most; one `pread` loop of that size | `files.rs` `WINDOW`, `MAX_WINDOW` |
| Names | a name that is not UTF-8 is not listed | `Walk::names` |
| Path outside the view, link, special file, mount, kept private | `InvalidInput` | `files.rs` `answer`, `refusal` |
| Path that is not there | `NotFound` | `files.rs` `refusal` |
| Linux before 5.6 | `Unsupported` (no `openat2`) | `confined.rs` `Refused::Unsupported` |
| `machine_stats`, `sandboxes_live` request | at most 4096 names in a list (`MOST`), each a valid Store or Sandbox name, else `InvalidInput` | `live.rs` |
| A worker of a build without a kind | `InvalidInput`, "no inspect kind"; `sandbox.list` shows it as `live: null`, `live_reason: "invalid_input"` | `route.rs` `serve` |
| macOS memory | one run of `/usr/sbin/sysctl` per `machine_stats` request on a macOS side; a failed run is `memory_total` and `memory_used` `null`, `resources.reason` `memory_not_read` | `host.rs` |
| Stopped or unreached machine | `Unavailable`, naming `layerfs machine start`; nothing starts it | `place::on_machine_read` |
| A machine whose calls run in the caller's process | `logs.machine` is `Unsupported`, `detail.reason` `no_worker` | `log.rs` |

## Adding a kind

One arm in `inspect::serve` (`route.rs`), taking `(side, request)` and, for a kind made of
many units of work, the `Budget`; and one `inspect::ask(home, machine,
"<kind>", args)` in its home-side function. A kind that can run out of
budget is to answer what it has, marked incomplete (`has_more: true` as a
folder page does, or `complete: false` with `reason: "deadline"` as
`machine_stats` and `sandboxes_live` do), rather than run to the hard
deadline.

A files view (a machine's folders, a Sandbox's whole root) is a kind whose
serving function builds a `files::Scope` and calls `files::answer`; its
roots answer goes in `roots.rs`, beside `roots::sandbox`. What it adds to
the platform files is one `confined::View` with its `resolve` arm, and what
it keeps private one line of `roots::kept`.

## Demonstration

| Test | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`tail.rs`](../../../../control/crates/layerfs-home/tests/tail.rs) | `cargo +1.85.1 test --locked -p layerfs-home --test tail` | Unix | tail, forward and backward windows over a rotated pair; long lines in pieces; a held-back partial line; escaping; stale cursors |
| [`inspect_admission.rs`](../../../../control/crates/layerfs-sandbox/tests/inspect_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test inspect_admission` | Unix | the third request is `Busy` and a stuck one is abandoned at the hard deadline; work past its soft budget answers what it has; a panic is `Failed` and frees its permit; a native Sandbox's log from its recorded run folder; no run folder is `NoSuchSandbox`; overlay sizes by `lstat` |
| [`inspect_stats.rs`](../../../../control/crates/layerfs-sandbox/tests/inspect_stats.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test inspect_stats` | Unix | `machine_stats` and `sandboxes_live` on a scratch root: Store sizes by `lstat` with a sparse file, live and ended daemons, names past the soft budget answered `null` with `complete: false`, over 4096 names refused; cgroup usage from a scratch cgroup tree, `cgroup_v1` and `no_cgroup` |
| [`machine_stats.rs`](../../../../api/crates/layerfs-api/tests/machine_stats.rs) | `cargo +1.85.1 test --locked -p layerfs-api --features machine --test machine_stats` | Unix; this build's worker on a scratch root | `machine.stats` and `sandbox.list` with `live` on the native and worker routes with a Store's lock held and the machine held: nothing written, no lease, no call-log line for `machine.stats`; a stopped machine is `Unavailable` and its Sandboxes `machine_stopped` |
| [`logs_inspect.rs`](../../../../api/crates/layerfs-api/tests/logs_inspect.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test logs_inspect` | Unix; this build's worker on a scratch root | both calls on the native and worker routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; rotation; a stopped machine; a forwarded Sandbox's run folder |
| [`worker_fault_inspect.rs`](../../../../control/crates/layerfs-control/tests/worker_fault_inspect.rs) | `cargo +1.85.1 test --locked -p layerfs-control --features worker-fault --test worker_fault_inspect` | Unix | two stuck reads hold two helpers, the third is `Busy`, Store and Sandbox operations go on, descriptors come back; a panic is `Failed`; a silent worker is `Unavailable` at 8 s |
| [`logs.rs`](../../../../api/crates/layerfs-cli/tests/logs.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test logs` | Unix | `layerfs logs sandbox` by cursor, usage errors and refusals; `layerfs sandbox files`: roots, a page continued, a window, refusals, no call-log line |
| [`confined.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/confined.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test confined` | Linux and macOS | an open stays beneath its base and follows no link, out or in; a FIFO is refused at once; a device where one can be made; Linux: `/proc` is refused as another mount and listed in the table |
| [`inspect_files.rs`](../../../../control/crates/layerfs-sandbox/tests/inspect_files.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test inspect_files` | Linux and macOS | 3000 entries paged by name through additions and removals; tree order at depths 1 to 4; byte order; a page cut by its budget; file windows, binary, a sparse 10 GiB file; every refusal; hard links by owner; the roots with Workspace mounts never touched; a stopped Sandbox's home |
| [`sandbox_files.rs`](../../../../api/crates/layerfs-api/tests/sandbox_files.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test sandbox_files` | Unix; this build's worker on a scratch root | `sandbox.files` on the native, worker and forwarded routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; a stopped machine |

## Related

- [API: logs family](../../api/sdk/logs.md).
- [Workers and reach](../machines/workers-and-reach.md).
- [Sandboxes](README.md).

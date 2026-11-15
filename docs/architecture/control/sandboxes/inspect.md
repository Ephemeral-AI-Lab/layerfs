# Inspection

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The route of the reads a client polls to watch a machine and its Sandboxes:
one worker operation, `inspect`, with bounded admission and deadlines, within
[Sandboxes](README.md).

| | |
| --- | --- |
| Source | [`layerfs-sandbox/src/inspect`](../../../../control/crates/layerfs-sandbox/src/inspect), [`layerfs-home/src/tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs), [`layerfs-store/src/place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `on_machine_read`, `Worker::read`, [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `send_within`, [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) |
| Used by | [API: `logs.machine`, `logs.sandbox`](../../api/sdk/logs.md#logsmachine) |

## Picture

```text
 call (logs.machine, logs.sandbox)
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
| Kinds | `machine_log` (`log/worker.log` and `worker.1.log` under the worker's root), `sandbox_log` (`daemon.log` in the Sandbox's run folder), `overlay_sizes` (`{ sandbox, namespaces }` to `{ overlay_sizes: [{ namespace, bytes }] }`, one `lstat` per Workspace overlay file). Another kind is `InvalidInput`. | `serve`; [`log.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/log.rs), [`overlay.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/overlay.rs) |
| Find the run folder | The `directory` of the daemon record in `<root>/sandbox/<name>/record.json` on the side that runs the Sandbox, read as a file. It is used only when it is a folder directly under that side's run root; a missing record or folder is `NoSuchSandbox`. | `run_folder` |
| Admit | Two permits per process (`PERMITS`). A request with none free is `Busy`. The work runs on a helper thread that owns the permit until it ends, so a read stuck in the kernel holds one permit and one thread and nothing else. | [`owner.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/owner.rs) `admit` |
| Deadlines | Soft 4 s (`SOFT`): `Budget::spent()` is what a kind made of many units checks between them. Hard 6 s (`HARD`): the caller is answered `Unavailable` and the helper is left to end by itself. The caller of a worker waits at most 8 s for the answer (`place::READ`), then reports `Unavailable`; a read changes nothing, so it is never `Unknown`. | `owner.rs`; [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `READ`, `Worker::read` |
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

## Invariants

| Rule | Enforced by |
| --- | --- |
| An inspect request takes no lock or lease, opens no Store, asks no daemon, starts nothing and writes nothing, on the home and on the machine. | the module uses none of `registry`, `lease`, `open::hold`, `daemon::pool` or `route`'s host operations; [`logs_inspect.rs`](../../../../api/crates/layerfs-api/tests/logs_inspect.rs) compares every file of both sides before and after |
| It is not refused while an operation holds the machine or a Store. | `place::on_machine_read` skips the intent check `on_machine` makes |
| At most two run per process; a third is `Busy` at once. | `owner::admit` |
| A stuck or panicking request never stops the worker's other operations. | the helper thread; [`worker_fault_inspect.rs`](../../../../control/crates/layerfs-control/tests/worker_fault_inspect.rs) |
| Only `worker.log`, `worker.1.log` and `daemon.log` are read as logs; never `log/uncertain.jsonl`. | `log.rs` |
| A record cannot point a read outside the run root. | `run_folder` |
| The two calls write no call-log line. | the dispatcher skips the `logs` family ([Calls](../../api/calls/README.md#the-call-log)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Requests at a time | 2 per process (`PERMITS`), else `Busy` | `owner.rs` |
| Request time | soft 4 s, hard 6 s, then `Unavailable` | `owner.rs` |
| Caller's wait for a worker | connect 2 s, hello 2 s, answer 8 s (`place::READ`), then `Unavailable` | `place.rs`, `worker/client.rs` `Budget` |
| Window | 64 KiB of file per request (`CHUNK`); 200 lines by default, 1000 at most | `tail.rs` |
| `overlay_sizes` request | at most 1024 namespaces, else `InvalidInput` | `overlay.rs` |
| Stopped or unreached machine | `Unavailable`, naming `layerfs machine start`; nothing starts it | `place::on_machine_read` |
| A machine whose calls run in the caller's process | `logs.machine` is `Unsupported`, `detail.reason` `no_worker` | `log.rs` |

## Adding a kind

One arm in `inspect::serve` (`route.rs`), taking `(side, request)` and, for a kind made of
many units of work, the `Budget`; and one `inspect::ask(home, machine,
"<kind>", args)` in its home-side function. A kind that can run out of
budget is to answer what it has with `has_more: true` rather than run to the
hard deadline; no kind here is made of more than a few reads.

## Demonstration

| Test | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`tail.rs`](../../../../control/crates/layerfs-home/tests/tail.rs) | `cargo +1.85.1 test --locked -p layerfs-home --test tail` | Unix | tail, forward and backward windows over a rotated pair; long lines in pieces; a held-back partial line; escaping; stale cursors |
| [`inspect_admission.rs`](../../../../control/crates/layerfs-sandbox/tests/inspect_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test inspect_admission` | Unix | the third request is `Busy` and a stuck one is abandoned at the hard deadline; work past its soft budget answers what it has; a panic is `Failed` and frees its permit; a native Sandbox's log from its recorded run folder; no run folder is `NoSuchSandbox`; overlay sizes by `lstat` |
| [`logs_inspect.rs`](../../../../api/crates/layerfs-api/tests/logs_inspect.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test logs_inspect` | Unix; this build's worker on a scratch root | both calls on the native and worker routes with a Store's lock held and the machine held: nothing written on either side, no lease, no call-log line; rotation; a stopped machine; a forwarded Sandbox's run folder |
| [`worker_fault_inspect.rs`](../../../../control/crates/layerfs-control/tests/worker_fault_inspect.rs) | `cargo +1.85.1 test --locked -p layerfs-control --features worker-fault --test worker_fault_inspect` | Unix | two stuck reads hold two helpers, the third is `Busy`, Store and Sandbox operations go on, descriptors come back; a panic is `Failed`; a silent worker is `Unavailable` at 8 s |
| [`logs.rs`](../../../../api/crates/layerfs-cli/tests/logs.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test logs` | Unix | `layerfs logs sandbox` by cursor, usage errors and refusals |

## Related

- [API: logs family](../../api/sdk/logs.md).
- [Workers and reach](../machines/workers-and-reach.md).
- [Sandboxes](README.md).

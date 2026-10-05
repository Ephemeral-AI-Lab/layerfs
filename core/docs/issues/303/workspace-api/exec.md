# workspace_api.exec — ordinary Bash against a ready Workspace

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner revisions: 2026-10-05. Product baseline:
> `f96d97651be5299f153ccde2bc8d921dd58807ad`; supersedes the Exec direction
> of design `334fc743751b9a181e670d0601a24fb3169208f9` where stated below.
> No implementation, throughput measurement or workload qualification is claimed.

This document owns process-runner behavior. [Mount](mount.md) supplies a complete
ready filesystem; the [daemon/SQLite engine](../daemon-sqlite.md) owns local data,
[FUSE](../fuse.md) owns kernel requests, and [Commit](commit.md) publishes an
explicit capture. Process launch is not another filesystem implementation.

## 1. Ordinary execution contract

[owner requirement]

Exec launches an ordinary Bash command in the mounted directory under the
configured command identity/environment. For Bash semantics the shell is
`/bin/bash -c`; direct program invocation and interactive PTY sessions, if exposed,
are separately declared runner modes. Launching `bash -c` does not itself supply
an interactive terminal or full job control.

There is no automatic runtime timeout, command count, lifetime output cap or
hidden preparation budget. A build, server or logger can run until it exits or
an explicit lifecycle action cancels it. Storage/control RPC deadlines cannot
silently become a shell lifetime limit. OS descriptor/process/argument limits,
declared CPU/memory/disk resources and caller-selected cancellation still exist.

The selected base already contains `.git`, ignored paths, dependencies, symlinks,
caches and output. Exec does not restore filtered files, install dependencies,
interpret `git`/compiler/package-manager commands, reroute selected paths outside
the Workspace, scan changes at exit, flush through command-specific hooks, or
implicitly capture, Commit, mount or unmount. A command that explicitly requests
an install still performs an install normally; preparation is not injected by
LayerFS.

Any authorized process accessing the same mount receives the same filesystem
semantics, whether launched through this API or another ordinary route.

One Workspace can serve many sequential or overlapping calls for a long
lifetime. Each uses that Workspace's current live filesystem; Exec completion
does not unmount, reset or require Commit before the next call. The caller can
issue repeated incremental Commits while calls continue. Sharing a Workspace
provides ordinary filesystem visibility, not isolated per-call transactions.

Per-tool-call is the expected common orchestration mode and per-task is also
supported. Neither implies Exec duration. Every command can finish quickly or
remain long-lived; no command classifier, default timeout or presumed short
request lifetime controls output, Commit or Workspace teardown. Process/stream/
filesystem ownership must remain valid throughout the actual execution.

## 2. Inputs, results and independent lifetimes

[proposed design; final SDK/wire types remain integration work]

| Item | Contract |
| --- | --- |
| Workspace selector | Exact ID/incarnation of a Ready mount; stale selectors fail |
| Program mode | Bash command by default, or explicitly supported direct/PTY mode |
| Cwd/environment | Mounted directory or authorized relative cwd; ordinary caller environment with private daemon credentials excluded |
| Streams | Bounded stdin/stdout/stderr chunks or explicit caller-owned sinks, with ordinary backpressure |
| Process ownership | Exec identity, process/process-group custody and event-driven child/descendant observations |
| Normal result | Actual exit/signal information and stream completion/disposition; no inferred filesystem Commit |
| Launch failure | Exact failure before acknowledged launch; no guessed successful command |
| Lost result | Process/result may exist; retain Exec identity and known state, no automatic command re-execution |
| Cancellation | Explicit runner action with fenced process/stream state; accepted filesystem mutations are not rolled back |

Root-shell exit, stdout/stderr EOF, descendant lifetime and mounted handles are
distinct events. A descendant can retain a pipe or open file after the shell
exits. Define response/stream behavior for this case; do not infer quiescence from
`wait()` of the shell. If a caller requires background services, their lifetime
and eventual teardown must be explicit rather than accidentally limited by a
control envelope.

## 3. Launch, I/O and filesystem workflow

[proposed design]

```text
 Caller / SDK             Daemon process runner               Kernel / Workspace
 ------------             ---------------------               ------------------
 exec(W, Bash, streams) -> check exact Ready incarnation
                              |
                              v
                         admit process resources
                         spawn Bash with cwd + identity
                              |
 launch/result stream <-------+--- stdout/stderr chunks <---- ordinary process pipes
                              |                                |
 stdin chunks ----------------+------------------------------> |
                              |                                v
                              |                          ordinary syscalls
                              |                                |
                              |                       +--------+---------+
                              |                       | FUSE mount       |
                              |                       | read/write/name  |
                              |                       +--------+---------+
                              |                                |
                              |                         Workspace semantics
                              |                                |
                              |                         bounded engine job
                              |                                |
                              |                       transaction COMMIT
                              |                                |
                              |                      successful syscall reply
                              |
                         child exit / pipe EOF events
                              |
 terminal result <------------+

 no registry/Workspace lock spans process runtime or stream backpressure
 no output collector grows with total bytes emitted
 no Exec-specific capture, status scan, dependency restore or history publication
```

When output goes to stdout, it uses runner pipes/sinks. `printf ... >> log` inside
the mount is file logging and follows the ordinary filesystem append path. These
are different workloads. Only redirected file bytes participate in filesystem
Commit; retained stdout is not implicitly inserted into the namespace.

Successful filesystem writes commit their bounded local transaction before reply.
A request buffer is not a delayed write log: once acknowledged, SQLite ownership
or the declared filesystem representation owns the bytes. Runtime atomicity of
the disposable overlay is not crash durability.

## 4. Bounded buffers without total-flow limits

[proposed resource contract]

```text
 BIG FILE / MANY EDITS                 PROCESS STDOUT / STDERR

 write syscall                        process pipe
     |                                    |
 bounded FUSE request                 bounded stream chunk
     |                                    |
 payload units + metadata             bounded queued chunks
     |                                    |
 atomic overlay transaction           caller sink / streamed consumer
     |                                    |
 acknowledgement                      backpressure if sink is slow
     |                                    |
 next request                         next chunk

 Total accepted data grows on disk.    Runtime/output length does not grow RAM.
 No file/edit/flow cap.                No silent 8 KiB total truncation.
```

Many tiny files create indexed metadata and small payload records. Repeated edits
replace the active visible representation rather than retain a chronological
FUSE log or a resident piece table. Big writes use successive bounded windows.
Sparse writes retain hole/cutoff semantics. The engine must bound operation work
under old fragmentation and reclaim discarded bytes outside acknowledgement.
These are required algorithms, not consequences of choosing SQLite.

All windows include queued, executing and blocked-producer bytes. A slow sink
applies ordinary pipe backpressure; it does not cause unlimited buffering or
truncate total output. Explicit disk sinks have declared capacity and failure
handling. Failed delivery cannot be reported as complete output.

## 5. Concurrency and Commit independence

[owner requirement; proposed scheduling]

Several Execs can share one Workspace, and Execs in different Workspaces can run
concurrently. Their processes and computation run independently. Requests meet
at inode ordering, the shared SQLite writer/engine scheduler, transport demand
capacity and host runtime service. Fair bounded jobs and deferred FUSE waiters
must keep unrelated activity runnable. SQLite's one writer per database is not
permission to serialize whole Execs or Commit lifetimes.

```text
 time ------->

 Exec A:  [read] [write X] [compute................] [write Y] [read] [exit]
 Exec B:       [create] [append.......................] [rename] [exit]
 Commit:                 | CAPTURE C | [construct/save/history] | INSTALL |
                         ^
                         |
                  acknowledged requests before here -> C
                  later requests                     -> active A

 Live view while constructing: A over C over old base
 Live view after known install: A over new base
 No command pause for the duration of Commit.
```

An ordinary syscall split across FUSE requests may straddle capture. Buffered
mmap stores not yet locally published are outside that captured mutation frontier.
Exec exit must not be used as an unproved mmap flush/capture hook. The caller
chooses a quiescent command boundary if its tool-call policy wants one, while
the filesystem's general Commit remains usable during activity.

For smallest granularity the caller normally uses:

```text
 mount full previous root -> exec command -> explicit commit -> terminal unmount
                                             |
                               delta includes ignored caches/output too
```

The caller can instead retain the same Workspace across any number of calls and
incremental Commits, independent of task boundaries. Commit captures its whole
published frontier, potentially including several callers' changes, then known
install advances the base and retains later active changes. The next Exec uses
that live view; it does not require another mount. Only explicit terminal unmount
ends the Workspace.
A nonzero command exit does not mean no data changed; the caller decides whether
to preserve that resulting state. An unknown Commit cannot be hidden by treating
Exec completion or unmount as history success.

## 6. Failure, cancellation and terminal unmount

[proposed design]

```text
 before spawn          launched                      process exited
     |                    |                                |
 failure -> no command    +-- stream/result lost ----------> retain identity/state
                          +-- explicit cancel ------------> signal/fence owned group
                          +-- ordinary unmount request ---> Busy; no silent kill
                          +-- forced unmount -------------> explicit cancel + drain
                                                            then lifecycle custody

 accepted writes remain live until explicit Commit or successful terminal discard
 no automatic resend of an unknown Exec: re-running a command can duplicate effects
```

Daemon/container termination can stop processes and loses a disposable local
Workspace; host history reflects whichever operations completed. Signal policy,
process-group custody, stream detachment and descendant handling must be defined
without assuming a killed shell terminated every descendant. The mount cannot
be reclaimed while a process, callback or transport operation still retains its
data. [Unmount](unmount.md) owns the full fence sequence.

## 7. Workloads and acceptance

[proposed validation; IDs are documentation cases, not frozen benchmark selections]

| Case | Workload | Required proof/cost observation |
| --- | --- | --- |
| E1 | Immediate `git status`/build on full committed tree | `.git`, ignored dependencies/caches/output present; no hidden preparation |
| E2 | Create/copy/hard-link many tiny files, wide directories | No dirty metadata frontier limit; request service and bytes/journal/index work attributable |
| E3 | 100,000 repeated/scattered edits followed by full Commit | No edit-count refusal or prior-fragment-sized new write; exact final bytes |
| E4 | Large regular files and sparse writes beyond former 4 GiB cap | Windowed flow and correct range reads; sparse Commit prerequisite not bypassed |
| E5 | Continuous tiny file appends, tail/read, rotate/truncate/unlink-open | No whole-log append reconstruction, growing orphan chain or payload-sized stall |
| E6 | Long-running Bash plus large streamed stdout/stderr | No automatic 30 s timeout/8 KiB truncation; bounded queued/blocked bytes |
| E7 | Multiple Execs and concurrent Workspaces/Commits | Independent progress; no control slot held across Exec; no contention EBUSY |
| E8 | Background descendants, inherited pipe and open file at shell exit | Separate process/stream/handle ownership; ordinary unmount reports Busy |
| E9 | Cancellation or lost connection while writing/outputting | No automatic command replay; exact output/process disposition and accepted bytes |
| E10 | Same command through Exec and another authorized process route | Identical filesystem semantics, permissions and visible paths |
| E11 | Persistent Workspace with overlapping calls and many incremental Commits | Same-mount current view and valid caches; later writes preserved; no lifetime/call cap or growing orphan/generation chain; cleanup progresses before final unmount |

The historical full fixture is 130,045 entries / 3,475,776,149 regular-file bytes;
dependency replay is 95,021 entries / 2,126,509,110 bytes. Exact source/manifest
identities and evidence limits are in [mount](mount.md). A source-only subset or
small smoke package cannot replace final full-workload execution and Commit proof.

Load-bearing acceptance also requires the engine's fragmentation/lifetime/
fairness/residency work and cluster one's deferred-edit, directory-memory,
new-parent, sparse and faithful-import corrections. Streaming output alone does
not establish either filesystem throughput or the complete tool-call latency.

Future timing separates launch, actual command, output drain, Commit and unmount;
reports overlapping observations without adding them twice. Use the repository
cold/equal-cache, one-sample, command-budget, independent-proof and append-only
rules from [validation](../07-implementation-validation.md). Harness stop budgets
never become product Bash timeouts. No workload was run for this document.

## 8. Current source versus replacement

[source-verified baseline]

The dormant [SDK](../../../../crates/layerfs-api/sdk/src/workspace.rs) uses
`exec(id, command)` through a 30,000 ms control call. The
[wire contract](../../../../crates/layerfs-bridge/src/contract/execution.rs) sets
`WORKSPACE_EXEC_MAX_MS = 30_000`, 8,192 output bytes per stream, and a bounded
whole result. [execution.rs](../../../../crates/layerfs-daemon/src/execution.rs)
launches `/bin/sh -c`, null stdin and piped output, collects capped Vecs, polls
Workspace revision for progress and kills the process group on error.
[control.rs](../../../../crates/layerfs-daemon/src/control.rs) retains the old
serialized slot. The dormant packages are excluded from the active workspace.

These are required replacement points. Current product code has not been changed
to implement ordinary Bash, unlimited runtime, streaming output, concurrent
control or the engine described here. Actual Exec streams/control framing must
be specified and verified in integration without restoring `layerfs-server`.

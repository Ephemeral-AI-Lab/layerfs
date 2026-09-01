# LayerFS V3 minimal prepared-container daemon experiment

Status: binding experiment specification.

This document defines the smallest experiment that can prove whether one
resident Rust process inside an already-prepared container materially reduces
LayerFS command-dispatch latency. It does not authorize a new process platform,
a Workspace pool, a storage redesign, or a production daemon rollout.

The V2 two-database architecture, identities, Stores, Pull, Fork, Commit, Push,
durability, Monitor, and public SDK/CLI method signatures remain authoritative.

## 1. Question under test

The current noninteractive container Exec path pays Docker Engine Exec create,
start/attach, output multiplexing, inspect, and a private `/bin/sh` PID-file
wrapper before it reaches the exact requested command.

V3 tests one replacement:

```text
public SDK
  -> authenticated local Unix stream
  -> one already-running layerfs-daemon
  -> one fresh exact requested process
  -> stdout/stderr/Exit on that stream
```

Only the daemon is resident. Every user process, Bash, workload, Workspace,
FUSE helper, and mount is created on demand.

The experiment is successful only when authentic public SDK operations become
decisively faster without a warm shell, process, mount, target, or cache.

## 2. Small responsibility boundary

`layerfs-daemon` is a local command transport and, in a later separately gated
stage, a launcher for the fixed `layerfs-fuse` helper. It is not a process
manager and does not own filesystem state.

The daemon may:

```text
accept an authenticated local stream
validate one bounded exact argv request
spawn one fresh direct child/process group for that stream
stream bounded stdout/stderr
accept Stop on that same stream
wait and reap its direct child
return exact exit/signal/stopped status
later launch/reap one fixed FUSE helper per Workspace
```

The daemon may not:

```text
schedule, queue, prioritize, list, attach, resume, or retry commands
keep a process database or durable process history
create a worker, Bash, interpreter, helper, or Workspace pool
reuse a process or command result
manage per-command cgroups
maintain a PID/mount recovery journal
inspect command purpose or attribute file changes to a process
read Workspace files, roots, Stores, or canonical objects
participate in capture, Commit, Push, placement, or durability
expose a public daemon SDK/CLI family
```

Keeping a live `Child`, process group, pipes, and byte counters until Exit is
ordinary spawn hygiene required for exact output, Stop, and reap. It does not
make the daemon a scheduler or general process manager.

## 3. Ownership remains outside the daemon

| Concern | Owner |
|---|---|
| LayerStackStore and BranchStore | V2 Store crates |
| Branch, Layer, Commit, and Object identity | V2 storage |
| Workspace tree and dirty ranges | host `WorkspaceWorker` |
| FUSE request semantics | existing `layerfs-fuse` and host port |
| Capture and checkpoint fence | host Workspace/FUSE control |
| Commit, CAS/CDC, rebase, Push, durability | existing host/Store path |
| Command concurrency policy | public SDK/host and container resources |
| Exact command launch/output/reap | one daemon Exec stream |
| Container-wide PID/CPU/memory containment | prepared-container supervisor |

The daemon receives an opaque Workspace ID only to bind an Exec to its verified
Workspace working directory. It never sees a Branch ID, Commit ID, root ID,
Object ID, dirty path, or captured byte range.

## 4. Public SDK remains unchanged

The experiment uses only the existing product surface:

```rust
Client::create_workspace_session(...)
Client::exec_workspace_session(...)
Client::workspace_output(...)
Client::stop_workspace_execution(...)
Client::commit_workspace_session(...)
Client::end_workspace_session(...)
```

There is no public `daemon` command or daemon SDK.

Receipts replace the current Boolean transport marker with:

```rust
pub enum ExecutionTransport {
    Host,
    Daemon,
    DockerEngineFallback,
    DockerCliFallback,
    DockerCliInteractive,
}
```

A benchmark arm requires one exact transport. It may not silently fall back
after scheduling begins.

## 5. Staged authorization

The first worktree implements only E0 and E1.

```text
E0  prove the real SDK process can reach/authenticate the daemon
E1  prove exact fresh noninteractive Exec beats direct Docker Engine Exec
```

Do not implement FUSE transport changes, checkpoint concurrency changes,
production installers, cross-platform relays, or destructive fallback removal
until E1 passes its value gate.

After E1:

```text
E2a test only daemon-side launch/reap of the fixed FUSE helper while retaining
    the current ProxyHost data/control path

E2b consider direct Unix FUSE-channel handoff only if E2a measurements prove
    that the remaining proxy transport is materially expensive
```

Passing a stage authorizes only the next experiment. It does not make the
daemon a production default or establish a comparison with Cloudflare
Computer.

## 6. Prepared environment and no-prewarm contract

The container image contains `layerfs-daemon`, `layerfs-fuse`, Bash, and the
registered benchmark workload. Installation or image build never occurs inside
a public Workspace or Exec timer.

Before the first registered operation, only this state may be resident:

```text
one running layerfs-daemon process
its demand-paged Rust code
immutable container configuration
one protected Unix listener
one authenticated owner/liveness connection
```

Forbidden prepared state is:

```text
Workspace or Workspace pool
FUSE helper, mount, namespace, or descriptor pool
Bash, shell, interpreter, workload, or preforked child
target file, repository root, path/inode/object cache
Store handle, selected root, canonical object, output, or result
benchmark name, marker, offset, or digest
```

Daemon startup must not open, stat, hash, read, or execute Bash, the workload,
the FUSE helper, a Workspace root, or a benchmark target. The fixed helper is
opened for the first time inside a measured E2a Mount.

No production `PreparedState` request is added merely for benchmarking.
No-prewarm proof comes from a separate sealed syscall/process trace plus
external `/proc`, cgroup, Store, and mount snapshots. Formal timing runs use
passive receipts only and do not run a tracer in the headline interval.

The OS page cache is uncontrolled and labeled as such. Daemon and direct-Engine
arms use the same image and cache policy. The first Bash row must be the first
Bash invocation in that arm's fresh prepared container.

Daemon startup is excluded only from the clearly labeled prepared-container
table. A separate cold-container table reports container and daemon startup
through the first exact public operation.

## 7. E0 route and authentication

The current benchmark's real SDK process runs inside the measured Linux
container. E0 uses:

```text
/run/layerfs/daemon.sock
AF_UNIX SOCK_STREAM
directory mode 0700
socket mode 0600
```

The prepared controller creates one 256-bit random capability for the
container. It is delivered through a protected file/descriptor, never argv,
environment, logs, Docker labels, output, or mountinfo.

One long-lived owner connection performs the only full mutual authentication:

```text
ServerHello { magic, version, daemon_boot_id, nonce }
ClientAuth  { capability proof, client nonce }
AuthOk      { owner_id, server proof }
```

Keyed BLAKE3 may be reused from existing workspace dependencies. Linux
`SO_PEERCRED` is mandatory. The daemon records the exact owner PID/UID/GID while
the owner connection remains live.

Later streams are accepted only from that exact live peer PID/UID/GID. PID
reuse cannot occur while the original owner connection and process remain
live. Their first frame binds the stream to its fixed purpose; they do not
repeat the three-message authentication handshake.

Successful `AuthOk` is readiness. E0 adds no `Ping` RPC.

E0 is No-Go if the actual SDK process needs Docker Exec, TCP, a host relay, or
another process after route selection to reach the daemon. Do not hide a relay
inside headline timing. Cross-kernel macOS/Windows/vsock work is deferred.

## 8. Minimal protocol

The protocol is not HTTP, REST, JSON-RPC, gRPC, ttrpc, or a generic RPC
framework. It is a bounded binary protocol over blocking `UnixStream`.

Magic and version are negotiated once on the owner connection. A bound stream
then uses only:

```text
u32 payload_length  big-endian
u8  kind
message-specific payload
```

Initial bounds are:

```text
control payload       <= 1 MiB
data/output payload   <= 64 KiB per frame
argv aggregate        <= 1 MiB
individual argv       <= 128 KiB
argument count        <= 4,096
cwd                    fixed by the verified Workspace; not arbitrary input
```

Lengths are checked before allocation. Overflow, trailing bytes, unknown kinds,
invalid empty argv, and embedded NUL are rejected before spawn. Argv and output
are opaque Unix bytes and need not be UTF-8.

### 8.1 E1 Exec stream

The client-first frame is:

```text
Exec {
  owner_id,
  WorkspaceId,
  ExecutionId,
  verified absolute cwd,
  exact argv[]
}
```

The stream is single-purpose. Its complete happy path is:

```text
client -> daemon: Exec
daemon -> client: Started
daemon -> client: zero or more Stdout/Stderr frames
client -> daemon: optional Stop on the same full-duplex stream
daemon -> client: Exit or Error
```

There is no:

```text
PrepareExec
StartExec
AttachExec
FinalizeExec
StopExec connection
QueryStatus
ListProcesses
request retry
output replay
terminal-summary service
```

The existing host `Execution` record owns the stream handle used by public
`stop_workspace_execution`. One daemon connection handler owns one direct
child from spawn through reap. A small live-ID set may reject a concurrently
active duplicate; no completed-command database or tombstone remains.

E1 has no daemon Mount registry, so the thin host adapter obtains `cwd`
internally from the active `WorkspaceWorker`'s container placement. It is not a
new public SDK field. The daemon requires an absolute, NUL-free path beneath
the admitted Workspace-root prefix and verifies the current live mount identity
before spawn. The `(WorkspaceId, cwd)` binding lasts only for that Exec stream.
E2a additionally requires it to equal the daemon's live Mount record. This
adds no registration request or protocol round trip.

### 8.2 Later Workspace lifecycle stream

E2a may add one retained stream per Workspace:

```text
client -> daemon: Mount
daemon -> client: WorkspaceReady
... same stream remains open for the Workspace lifetime ...
client -> daemon: Close
daemon -> client: WorkspaceClosed
```

EOF is cleanup, not a retry trigger. There is no second End connection,
QueryWorkspace, FinalizeWorkspace, request-digest database, or closed tombstone.

## 9. Exact Exec semantics

The daemon performs the moral equivalent of:

```text
create stdout/stderr pipes
fork/clone one child
place it in one fresh process group
set the admitted uid/gid/groups, cwd, umask, rlimits, and environment
execve(argv[0], argv)
stream output
wait/reap
```

It inserts no shell. Bash is explicit:

```text
argv = ["/bin/bash", "-lc", "..."]
```

Every such request loads and starts a fresh Bash. A native argv starts no shell.
There is no process pool or warm child.

The prepared environment is fixed for the container. Daemon-internal socket
and capability variables/descriptors are removed or close-on-exec. Environment
mutation, stdin streaming, PTY, resize, and interactive terminal signaling are
not part of E1. Existing `shell_workspace_session` remains an explicitly
receipted Docker CLI interactive path.

Output uses 16–64 KiB read buffers and <=64 KiB frames. Unix-socket and pipe
backpressure bound memory; the daemon has no output spool. The existing host
`OutputLog` remains the sole public output store.

`Exit` is sent only after:

```text
direct child exit/signal is known
stdout EOF
stderr EOF
direct child is reaped
byte counts and timing are final
```

Normal completion drains and waits. The daemon does not kill descendants merely
to publish Exit. Explicit Stop, Exec-stream EOF, or owner/container shutdown
signals the owned process group and then drains/reaps. A deliberately escaped
process is contained by the prepared container supervisor, not a daemon process
database or per-execution cgroup.

Public receipts retain only:

```text
ExecutionId
ExecutionTransport
exit code/signal/stopped
stdout/stderr byte counts
public total and balanced timing phases
```

PID, PGID, starttime, and argv digest remain internal trace evidence. They do
not become permanent public SDK schema unless a real consumer requires them.

## 10. Failure and cleanup

Before any Exec frame is sent, daemon unavailability may select the explicit
Docker Engine fallback. After an Exec or Mount frame may have been sent:

```text
never retry the command
never remount the Workspace
never select another transport
never convert incomplete output into success
```

Exec-stream loss causes the handler to signal the owned process group, wait,
reap, release buffers/FDs, and return `InfrastructureLost` or `OutputFailed` to
the host. False-negative infrastructure failure is preferable to duplicate
filesystem mutation.

Workspace-lifecycle loss in E2a requests helper cleanup and reports
`InfrastructureLost`; it does not query a tombstone service or remount.

Owner connection EOF:

```text
rejects new streams
marks active operations InfrastructureLost
closes lifecycle channels
signals/reaps direct children as shutdown cleanup
notifies the prepared-container supervisor
```

The supervisor terminates/recreates the whole prepared container. The daemon is
never restarted in place and keeps no recovery journal. Cleanup never fabricates
normal Stop, Commit, clean End, or successful exit receipts.

## 11. Resource behavior

There is no LayerFS semantic limit such as “32 Workspaces” or “one execution per
Workspace.” Different Exec streams are independent and the daemon has no queue.

The implementation uses a single immediate admission counter derived from its
fixed memory/FD budget and `RLIMIT_NOFILE`. Each admitted stream reserves its
small fixed record, two pipes, thread/task, and bounded buffers; RAII releases
the reservation. Exhaustion returns `LimitExceeded` immediately. There are no
priorities, per-Workspace quotas, or dynamic resource scheduler.

Required bounds:

```text
idle RSS/PSS                  <=32 MiB target, <=64 MiB hard
idle CPU                      <=0.1% target, <=0.5% hard of one core
idle polling                  <=1 wake/second target
per-output frame              <=64 KiB
control/argv allocation       <=1 MiB per request
persistent payload/Store cache 0 bytes
```

The prepared container supervisor owns CPU, memory, PID, and I/O cgroups. E1
does not create per-execution cgroups, resource-policy RPCs, or a daemon
scheduler.

A 100-Exec leak test is sufficient before accepting the E1 pilot. It must leave
no child, zombie, leaked FD, live-ID record, output buffer, or monotonic RSS
growth after allocator warm-up. A 1,000-cycle soak and hostile flood campaign
belong to production hardening after the value gate.

## 12. Minimal E0/E1 implementation tree

```text
Cargo.toml
Cargo.lock

crates/layerfs-daemon/
  Cargo.toml
  src/
    lib.rs          shared client types
    protocol.rs     bounded codec and owner authentication
    main.rs         blocking listener plus one-stream Exec/Stop/reap
  tests/
    live_exec.rs    exact argv/output/Stop/no-prewarm/leak proof

crates/layerfs-workspace/src/
  daemon.rs         thin internal client and route
  execution.rs      add Daemon beside existing Host/Engine/CLI transports
  session.rs        ExecutionTransport receipt enum

benchmark/fs-bench-pro/
  Dockerfile.layerfs
  src/main.rs        extend existing diagnose path
  run.sh             same-image daemon/Engine profiles

docs/research/history/v3/
  layerfs-daemon.md
```

Do not add in E0/E1:

```text
daemon server/execution/fuse module hierarchy
daemon_probe.rs
async runtime
generic handler registry or trait/factory layer
service installer
deployment abstraction
supervisor integration code
process or mount journal
```

Split `main.rs` only after measured functionality makes a split clearer than
the single blocking implementation. The handwritten production-source ceiling
applies to Rust source, not this specification, benchmark evidence, generated
output, or SQL.

## 13. E0 proof

The experimental image starts `layerfs-daemon` as the prepared container
process instead of `sleep`. The existing outer benchmark-harness `docker exec`
occurs before public LayerFS timers and is reported as setup; after the SDK
selects the daemon route, measured Exec performs zero Docker calls.

Both daemon and direct-Engine arms use the same image with the daemon resident,
so daemon memory and preparation are not hidden from the comparison.

E0 runs one route/authentication smoke and records:

```text
actual SDK PID reaches /run/layerfs/daemon.sock
owner mutual authentication succeeds
SO_PEERCRED matches that exact live SDK process
zero Docker operation after route selection
zero prepared Workspace/mount/helper/Bash/workload/target access
daemon startup wall, RSS/PSS, CPU, threads, FDs, and wakeups
```

Failure is No-Go for the daemon route. Do not implement Exec or a relay to
rescue E0.

## 14. E1 benchmark and value gate

Reuse the existing `fs-benchmark-pro` `diagnose` and public SDK timer. Do not
add a separate daemon probe module.

The public interval starts before `Client::exec_workspace_session`, includes
`Client::workspace_output` acquisition and terminal following, and ends only
after complete output and Exit. Monitor/evidence collection occurs afterward.
Every Unix-socket connect, peer check, request/response byte, wait, and output
frame after the timer starts is included; no IPC or transport time is
subtracted.

Rows are:

| Row | Exact public behavior | Purpose |
|---|---|---|
| native true | fresh `/bin/true` | control-plane floor |
| Bash no-op | fresh `/bin/bash -lc ':'` | user-visible shell startup |
| helper no-op | fresh Bash to native helper | Bash plus workload child |
| edit | fresh Bash to ten-byte `pwrite` + `fsync` through real FUSE | authentic small edit |
| nonzero | exact nonzero child | exit propagation |
| output | bounded stdout/stderr | byte/EOF proof |
| Stop | public Stop on attached stream | exact group termination |
| disconnect | injected stream loss | no retry/fallback/residue |

E1 deliberately retains existing Workspace create, ProxyHost/FUSE, Commit,
Push, and End code. Create/End are recorded but are not part of the Exec
mechanism headline.

Run one balanced daemon/Engine smoke pair for all rows. If valid, run ten
balanced pairs for Bash no-op and edit: five AB and five BA under a frozen seed.
`/bin/true` and helper no-op remain attribution rows. Do not run Computer or a
30-pair formal campaign before the value gate.

Every arm/row uses:

```text
same sealed source and image
same resident daemon, including the Engine arm
fresh container and Store pair
fresh Workspace, real FUSE helper, and mount
fresh exact command/process group
fresh Bash/workload when requested
same Bash profiles, uid/gid/groups, cwd, umask, rlimits, and environment
same target, pwrite, fsync, output, and correctness oracle
no target pre-read, pool, batch, or hidden cache
OS page cache labeled uncontrolled
```

E1 Go requires all correctness/no-prewarm/resource gates plus:

```text
fresh Bash no-op median              <=10 ms
fresh Bash no-op p95                 <=15 ms
ten-byte pwrite+fsync median         <=15 ms
ten-byte pwrite+fsync p95            <=20 ms
daemon / direct-Engine median ratio  <=0.70
paired 95% CI upper bound            <0.85
zero Docker calls on daemon Exec
zero duplicate execution
zero child/FD/output residue
```

If E1 fails, stop V3 and retain direct Engine. Do not build FUSE transport or
production-hardening machinery to rescue a weak Exec result.

## 15. Current baseline

Round 028 is the focused current performance best. Round 029 is the exact
final-source custody confirmation.

```text
Round 028 complete EDIT16  1.012593378 s
  Workspace create           51.565708 ms
  aggregate Exec            552.974336 ms
  aggregate Commit          126.403251 ms
  aggregate Push            276.285167 ms
  Workspace End               5.364916 ms

Round 029 complete EDIT16  1.151741298 s
  Workspace create           50.567875 ms
  aggregate Exec            690.254086 ms
  aggregate Commit          128.786294 ms
  aggregate Push            277.454585 ms
  Workspace End               4.678458 ms
```

Recent focused diagnostic medians are approximately:

```text
/bin/true                         30.6 ms
fresh Bash :                      31.7 ms
fresh Bash -> helper no-op        38.9 ms
fresh Bash -> pwrite+fsync        39.3 ms
```

The small true-to-Bash/edit delta supports the E1 hypothesis: Docker Exec
object creation/start/inspect and the private wrapper dominate short commands.
These are LayerFS-only mechanism baselines, not Computer comparisons.

## 16. E2a: minimal FUSE-helper launch experiment

E2a is authorized only after E1 passes.

It changes only how the fresh fixed `layerfs-fuse` helper is launched and
reaped:

```text
keep current host ProxyHost data/control protocol
keep current real FUSE semantics and mountinfo proof
keep current authenticated shutdown behavior
daemon launches the fixed installed helper instead of Docker Exec/helper copy
one fresh helper and mount per Workspace
no helper access at daemon startup
```

Do not add direct FUSE FD passing or replace `ProxyHost` in E2a. The current
Create path is already approximately 45–52 ms and healthy End approximately
5–8 ms. The obsolete Round 022 114/116 ms lifecycle is not a valid decision
baseline.

E2a Go requires:

```text
SDK Workspace Create median       <=25 ms
SDK Workspace Create p95          <=40 ms
SDK Workspace End median          <=8 ms
SDK Workspace End p95             <=12 ms
SDK End / same-seal fallback      <=1.15 paired median ratio
SDK three-operation median        <=60 ms
SDK three-operation p95           <=90 ms
zero Docker calls on daemon Create/Exec/End
fresh real FUSE helper/mount       every Workspace
zero mount/helper/child residue
```

Create includes the Workspace lifecycle-stream connect/bind, Mount/Ready
frames, existing ProxyHost loopback/container-network setup, fresh helper and
mount, and readiness wait. End includes Close/Closed on the retained stream,
shutdown traffic, unmount verification, and reap. No local IPC, loopback, or
container-network time is subtracted.

The CLI lifecycle uses one already-open persistent `CliSession`, whose cached
`Client` owns the ephemeral Workspace worker. Its outer timer begins before
command parsing and ends after the final `Finished` event is rendered. It does
not start a new OS process or reconnect both Stores for each operation.

```text
CLI Workspace Create median       <=27 ms
CLI Workspace Create p95          <=45 ms
CLI Workspace End median          <=10 ms
CLI Workspace End p95             <=15 ms
CLI overhead above matching SDK   <=2 ms median, <=5 ms p95
```

A one-shot CLI cold-start/context-connect timer is reported separately. It is
not pooled with Workspace lifecycle timing and cannot be used to hide SDK or
daemon work.

If E2a fails, keep the current one-call Create and zero-call End. Do not build
direct FUSE channels.

## 17. E2b is evidence-gated, not planned work

Only a passing E2a trace that attributes material remaining time to TCP
ProxyHost connection setup may authorize E2b.

E2b may then test direct Unix socketpairs/file-descriptor handoff while reusing
the existing FUSE request codec unchanged. It must not create a second
filesystem protocol. Separate data and pause/fence control channels remain
justified so control cannot queue behind data backpressure.

E2b is not part of the first worktree, file tree, or completion claim.

## 18. JIT checkpointing is a separate filesystem refinement

The desired live-FUSE filesystem semantics remain process-agnostic:

```text
any process may write through the mount
filesystem changes are not attributed to an ExecutionId
a live-FUSE checkpoint may establish an exact filesystem-visible fence while
commands remain alive
```

That behavior belongs to host `WorkspaceWorker` and `layerfs-fuse`, not the
daemon. It is deliberately excluded from E0/E1 so transport measurements do
not mix with a concurrency redesign.

Current source still:

```text
returns Commit Busy while an execution exists
returns End WorkspaceBusy while an execution exists
rejects quiescence for open writers
returns PortError::Busy to FUSE calls while paused
```

A separately reviewed live-FUSE refinement must destructively replace those
gates with:

```text
Accepting -> Freezing -> Frozen -> Accepting
new callbacks wait during the short freeze instead of receiving EBUSY
callbacks accepted before fence T drain
open writable FDs and process count do not affect capture
Commit includes filesystem-visible state through T
post-T calls resume into the next dirty generation
```

This guarantee applies only to real live FUSE. A materialized host directory
retains the current Busy rule until it has an equivalent filesystem freeze.

Closing while commands run is also host policy. The host owns attached Exec
streams and may Stop/wait them before Close; the daemon does not enumerate or
discover container processes. A stopped command must receive an honest stopped
receipt, never false success. `EndWorkspaceMode::Discard` authorizes dropping
dirty filesystem state, not fabricating a successful command.

## 19. Required E0/E1 proof

Required before accepting value results:

### Route and protocol

```text
owner mutual authentication and exact SO_PEERCRED
bounded fragmented/coalesced frame decoding
wrong capability/peer/version rejection
same-kernel route from actual SDK process
zero Docker call on daemon Exec
```

### Exact process and output

```text
exact non-UTF-8 argv round-trip
empty argv and NUL rejection
exact cwd/uid/gid/groups/environment/umask/rlimits
daemon inserts no shell
fresh PID/process group for every Exec
fresh Bash/workload identity when requested
stdout/stderr byte equality and EOF before Exit
nonzero exit propagation
Stop and disconnect reap without residue
no retry or fallback after dispatch
```

### No-prewarm and resources

```text
separate sealed syscall/process trace
zero preheadline Bash/workload/helper/target access
zero Workspace/mount/process pool
idle RSS/CPU/thread/FD/wakeup receipt
100-Exec child/FD/RSS plateau
```

### Public timing

```text
timer starts before exec_workspace_session
workspace_output acquisition included
timer ends at complete terminal receipt
Monitor/evidence collection after timer
one host monotonic clock is authoritative
daemon phases are nested diagnostics only
all timer equations balance
```

## 20. Deferred until value is proven

Do not implement during E0/E1:

```text
direct FUSE Unix/FD channels
ProxyHost replacement
JIT checkpoint/concurrency changes
PTY, stdin streaming, resize, environment mutation
QueryStatus or attach/replay
request retry, tombstones, terminal-summary service
daemon PID/mount journal or in-place restart
per-execution cgroups or resource-policy RPCs
hostile-tenant process management
cross-platform relay/vsock/TCP listener
installer/service manager or production-default routing
full decoder fuzz campaign and 1,000-cycle soak
formal 30-pair or Computer campaign
destructive removal of proven fallbacks
```

Add a deferred item only after a measured failure shows the minimal design is
insufficient.

## 21. Terminal experimental outcome

The first worktree is complete only when E0 and E1 either:

1. pass all route, correctness, no-prewarm, resource, timing, and performance
   gates from one sealed source/image; or
2. fail the predeclared value gate and leave the current direct-Engine path as
   the unchanged product default.

A compile-only daemon, internal microbenchmark, warm-shell result, hidden
Docker fallback, or partially timed process is not a successful experiment.

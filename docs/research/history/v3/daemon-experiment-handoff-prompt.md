# LayerFS V3 minimal daemon experiment handoff prompt

You are the sole implementation owner for the LayerFS V3 prepared-container
daemon value experiment.

The repository is:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

The binding experiment specification is:

```text
docs/research/history/v3/layerfs-daemon.md
```

Read that specification completely before creating or editing source. It is
authoritative for daemon scope, protocol, staging, fairness, timing, gates, and
deferred work. V2 remains authoritative for Stores, Workspace data, FUSE
semantics, Commit, Push, durability, and public SDK/CLI operations.

## Objective

Create an isolated Git worktree and implement the smallest honest experiment
that answers:

> Can one small Rust process already running inside a prepared container launch
> a fresh exact noninteractive command substantially faster than the current
> direct Docker Engine Exec path?

Only the daemon may be resident. Every Workspace, FUSE helper, mount, requested
process, Bash, interpreter, and workload must be created on demand. Do not
prewarm, pool, cache, batch, pre-read, or bypass anything to reach the target.

Use only the existing public SDK/CLI operation families. The daemon is an
internal transport, not a public API.

## Worktree prerequisite and creation

Begin in the original repository and run read-only checks:

```text
git status --short
git diff --stat
git diff
git log -1 --oneline --decorate
git worktree list --porcelain
git branch --list 'codex/v3-daemon-experiment*'
rg --files
```

The experimental worktree must start from a committed ref containing:

```text
the current Round 028/029 product mechanisms
the current docs/research/history/v3/layerfs-daemon.md
this handoff prompt
```

Do not silently create the worktree from a stale `HEAD`. If the latest required
source/spec exists only as dirty or untracked files and no committed ref contains
it, stop before mutation and report the exact external action required: the user
must commit the intended starting state or provide its exact ref. Do not stage,
stash, commit, copy, absorb, or discard the dirty main worktree to manufacture a
base. Preserve every unrelated user/agent change.

Once an exact starting ref is available, create:

```text
branch:   codex/v3-daemon-experiment
worktree: /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v3-daemon-experiment
```

Use non-destructive `git worktree add -b`. If either exact name already exists,
inspect it; never delete, overwrite, prune, or reuse an unrelated worktree.
Choose a clearly suffixed `codex/v3-daemon-experiment-*` branch/path and report
the actual names.

Perform all implementation, build, test, benchmark, and report writes inside
the new worktree. Do not commit or push unless the user separately requests it.

## Required initial review

Before editing, launch up to three read-only subagents with disjoint scopes:

1. Inspect the current `Workspaces::spawn`, `DockerExec`, `OutputLog`, Stop, and
   receipt paths and identify the smallest daemon integration seam.
2. Audit the proposed Unix protocol, owner authentication, exact argv/cwd,
   output/Exit, disconnect, and Stop-on-the-same-stream semantics for correctness
   and minimal round trips.
3. Audit `fs-benchmark-pro`, Round 028/029 raw evidence, image/run environment,
   timers, and anti-prewarm proof; identify what can be reused unchanged.

Tell every subagent that others share the worktree, that the task is read-only,
and that they must never revert or overwrite another contributor. Synthesize
their evidence yourself. Do not implement speculative recommendations that
conflict with the binding specification.

## Non-negotiable daemon boundary

The daemon may only:

```text
authenticate one prepared SDK owner
accept one bounded Exec stream
validate the internally supplied Workspace cwd and exact argv
spawn one fresh direct child/process group for that stream
stream bounded stdout/stderr
accept Stop on that same full-duplex stream
wait and reap its direct child
return exact exit/signal/stopped status
later, only after E1 passes, launch/reap one fixed layerfs-fuse helper
```

It must not:

```text
schedule, queue, prioritize, list, attach, resume, or retry processes
maintain a completed-command database or output replay service
add Prepare/Start/Attach/Finalize/Query RPCs
open a separate Stop connection
create a warm shell, executor, helper, mount, or Workspace pool
create per-execution cgroups or resource-policy RPCs
maintain a PID/mount journal or restart in place
inspect file contents or attribute mutations to ExecutionIds
receive Store/root/Object/Commit identities
participate in capture, Commit, Push, placement, or durability
add HTTP, JSON-RPC, gRPC, ttrpc, an async runtime, or a protocol framework
```

Keeping a live `Child`, process group, pipes, and counters only until Exit is
required spawn hygiene, not authorization to become a process manager.

## Minimal E0/E1 source shape

Start with only:

```text
crates/layerfs-daemon/
  Cargo.toml
  src/lib.rs
  src/protocol.rs
  src/main.rs
  tests/live_exec.rs

crates/layerfs-workspace/src/daemon.rs
crates/layerfs-workspace/src/execution.rs
crates/layerfs-workspace/src/session.rs

benchmark/fs-bench-pro/Dockerfile.layerfs
benchmark/fs-bench-pro/src/main.rs
benchmark/fs-bench-pro/run.sh
```

Do not create `server.rs`, daemon `execution.rs`, `fuse.rs`, `daemon_probe.rs`,
generic handler traits, factories, installers, or deployment modules during
E0/E1. Split `main.rs` only after measured functionality makes the split
clearly smaller or safer. Keep handwritten production Rust files below the
existing source ceiling. Documentation and evidence are not subject to that
ceiling.

Prefer the standard library and already-locked dependencies. Use blocking Unix
streams and bounded threads. Do not add dependencies without a measured need.

## E0: prove the route before Exec

Use the same sealed image for daemon and direct-Engine arms. The daemon is
resident in both arms so its preparation and resource footprint are not hidden.

The experimental image starts `layerfs-daemon` as the prepared container
process instead of `sleep`. The existing outer benchmark-harness Docker Exec is
setup before public LayerFS timers and must be reported separately. After route
selection, a measured daemon operation must make zero Docker CLI/Engine calls.

Implement only the owner route and full mutual authentication:

```text
/run/layerfs/daemon.sock
AF_UNIX SOCK_STREAM
0700 runtime directory
0600 socket
one 256-bit capability
ServerHello -> ClientAuth -> AuthOk
mandatory exact SO_PEERCRED PID/UID/GID
one retained owner/liveness connection
```

`AuthOk` is readiness. Do not add Ping or PreparedState production operations.
Later streams are accepted only from the exact still-live owner PID/UID/GID and
bind their purpose in the first frame; they do not repeat the full handshake.

E0 proof must show:

```text
the actual SDK process connects directly
zero Docker call after route selection
zero prepared Workspace/mount/helper/Bash/workload/target access
daemon startup and idle RSS/PSS/CPU/thread/FD/wakeup footprint
wrong capability/peer/version rejection
```

If the real SDK needs Docker, TCP, a host relay, or cross-VM bridge to reach the
socket, E0 is No-Go. Do not hide a relay or broaden the experiment.

## E1: one-stream exact Exec

Use a compact bounded binary protocol. Negotiate magic/version once on the
owner connection. Bound streams use only:

```text
u32 big-endian payload length
u8 message kind
message-specific payload
```

The first Exec frame contains:

```text
owner_id
WorkspaceId
ExecutionId
verified absolute cwd
exact opaque argv[]
```

The thin host adapter derives cwd internally from the active
`WorkspaceWorker`'s `WorkspacePlacement::Container.root`. It is not a new
public SDK field. Validate absolute/no-NUL/admitted-prefix/current-live-mount
before spawn. This adds no registration request or extra round trip.

The entire normal stream is:

```text
client -> daemon: Exec
daemon -> client: Started
daemon -> client: Stdout/Stderr frames
client -> daemon: optional Stop on that same stream
daemon -> client: Exit or Error
```

Use one fresh process group and exact `execve`. Insert no shell. Bash is fresh
only when exact argv requests Bash. Apply the existing prepared uid/gid/groups,
environment, cwd, umask, and rlimits. Close daemon secrets and internal FDs on
exec.

Reuse the host `Execution`, registry, `OutputLog`, `OutputReader`, public
receipts, and timers. Replace `direct_engine: bool` with exact
`ExecutionTransport` throughout SDK/Monitor/CLI/benchmark JSON and tests.

`Exit` is legal only after direct-child status, stdout EOF, stderr EOF, final
byte counts, and reap. The daemon has no output spool; bounded pipes/frames and
socket backpressure bound memory.

Before any Exec byte is sent, daemon unavailability may select the explicit
direct-Engine fallback. After possible dispatch, never retry argv, never switch
transport, and never turn incomplete output into success. Stream loss signals
the owned process group, drains/reaps, releases resources, and returns a typed
infrastructure/output failure.

Do not modify Workspace Create, ProxyHost/FUSE, lifecycle, Commit, Push, or End
semantics during E0/E1. JIT checkpointing is separate host/FUSE work and must
not contaminate the transport experiment.

## Benchmark reuse and environment

Reuse the existing `benchmark/fs-bench-pro/src/main.rs` diagnose path and
`run.sh`. Do not add `daemon_probe.rs` or an internal-only benchmark.

The public Exec timer begins before `Client::exec_workspace_session`, includes
Unix connect/peer check/request bytes, `Client::workspace_output`, all output
frames, and terminal following, and ends after complete Exit. No transport,
IPC, process, Bash, FUSE, or fsync time is subtracted. Monitor/evidence
collection occurs afterward.

Build the same image once per source seal using existing BuildKit/Cargo caches
outside candidate state. Do not rebuild or repeat environment setup without a
source/image change. Each measured arm/primary row uses a fresh container,
Store pair, Workspace, real FUSE helper/mount, command process group, Bash, and
workload. The daemon remains resident in both arms.

Do not run Computer during E0/E1/E2 daemon-versus-Engine proof.

Rows:

```text
/bin/true
fresh /bin/bash -lc ':'
fresh Bash -> native helper no-op
fresh Bash -> ten-byte pwrite+fsync through real FUSE
nonzero exit
bounded stdout/stderr
public Stop
injected stream disconnect
```

Run one balanced daemon/Engine smoke pair for every row. If valid, run ten
balanced primary pairs for Bash no-op and pwrite+fsync: five AB and five BA
under a frozen seed. Retain true/helper as attribution rows. Do not start a
30-pair formal campaign before the value gate.

Current honest mechanism context:

```text
Round 028 complete EDIT16       1.012593378 s
Round 029 complete EDIT16       1.151741298 s
/bin/true median                approximately 30.6 ms
fresh Bash no-op median         approximately 31.7 ms
helper no-op median             approximately 38.9 ms
pwrite+fsync median             approximately 39.3 ms
Workspace Create               approximately 45–52 ms
healthy Workspace End          approximately 5–8 ms
```

E1 Go requires all correctness/no-prewarm/resource gates and:

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

Do not weaken or tune gates after observing results.

## Iterative optimization loop

After every meaningful run:

1. Validate source/image/route/timer/no-prewarm/correctness gates before reading
   the headline.
2. Append the run, including failures, to the experiment history.
3. Attribute elapsed time using the public host clock and nested daemon
   diagnostics: accept/bind, decode, spawn, runtime, output drain/reap, terminal
   publication, and unattributed time. Never add clocks from different hosts.
4. Identify the dominant removable LayerFS cost, not native Bash/tool time.
5. Inspect the shared call path and all callers before editing.
6. Make the smallest root-cause change; do not add a cache, pool, batch, or
   protocol phase.
7. Run the smallest focused correctness test, then the affected benchmark
   smoke. Do not rerun a full pilot after every trivial change.
8. Ask read-only subagents to review a materially surprising result or proposed
   architectural expansion before implementing it.
9. Preserve mechanisms that are already fast and stable; explicitly record
   them as “no improvement needed.”

Do not stop after one failed benchmark. Continue E1 until the gate passes or
all safe in-scope transport/spawn/output opportunities are exhausted and an
independent audit agrees the measured stable result is an honest No-Go. Do not
use FUSE, batching, prewarming, or weaker proof to rescue E1.

## Append-only report and raw custody

Maintain one append-only report:

```text
benchmark-results/fs-bench-pro/daemon-experiment-history.md
```

Store raw artifacts under unique immutable directories:

```text
benchmark-results/fs-bench-pro/runs/daemon-<stage>-<source>-<timestamp>/
```

Never overwrite or silently rerun a scheduled sample. Every report round must
contain:

```text
round/stage and local+UTC timestamp
base commit, dirty flag, full diff SHA-256, source seal, image digest
exact hypothesis and code change since prior round
exact commands, argv, image, resource envelope, and cache label
raw artifact paths and inventory digest
all scheduled successes/failures; no outlier deletion
median, p95, paired ratios, confidence interval when applicable
public timer balance and daemon nested breakdown
exact transport proof and Docker-call count
fresh PID/PGID/Bash/workload/FUSE/mount evidence
correctness, final bytes/digest, fsync, output, Stop/disconnect results
idle and active RSS/PSS/CPU/thread/FD/buffer evidence
defects and measured bottlenecks
what is fast/stable and needs no improvement
next bounded optimization or Go/No-Go decision
```

The final section of that same file must be a terminal summary comparing the
best valid daemon result with the same-seal direct-Engine result. Do not publish
a Cloudflare Computer or production superiority claim from this experiment.

## Resource and anti-cheat proof

Before accepting the ten-pair pilot, prove:

```text
idle RSS/PSS <=32 MiB target, <=64 MiB hard
idle CPU <=0.1% target, <=0.5% hard of one core
idle polling <=1 wake/second target
output frames <=64 KiB
control/argv allocation <=1 MiB per request
zero persistent payload/Store cache
100 Execs leave no child, zombie, FD, live-ID, output, or monotonic RSS growth
```

Use one immediate RAII admission counter derived from fixed memory/FD budget
and `RLIMIT_NOFILE`; return `LimitExceeded` rather than queueing. Do not add
priorities, per-Workspace quotas, or a resource scheduler.

No-prewarm proof requires a separate sealed syscall/process trace plus passive
formal receipts. The daemon must have zero preheadline access to Bash, workload,
FUSE helper, Workspace root, or target. OS page cache is uncontrolled and
labeled identically in both arms.

## Required verification

Run focused tests after each semantic change. Before an E1 terminal verdict,
run and retain:

```text
cargo fmt --check
focused layerfs-daemon protocol/live tests
focused layerfs-workspace execution/output/Stop tests
focused layerfs-sdk execution tests
focused fs-benchmark-pro tests
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
release build used by the sealed image
E0 route/no-prewarm/resource proof
E1 correctness smoke, 100-Exec leak check, balanced pilot when authorized
```

Run existing live Docker/FUSE gates in the environment required by the repo.
Do not restore, compile, test, or depend on the removed TUI, Ratatui, or
Crossterm.

## Conditional E2a only after E1 passes

If and only if E1 passes, implement the smallest helper-launch experiment:

```text
one retained Workspace lifecycle stream
Mount -> WorkspaceReady
Close -> WorkspaceClosed on that same stream
daemon launches/reaps the fixed installed layerfs-fuse
current ProxyHost data/control protocol remains unchanged
current FUSE request codec, mountinfo, and shutdown remain unchanged
one fresh helper and real mount per Workspace
no helper access at daemon startup
```

Do not implement direct FD handoff or replace ProxyHost in E2a.

E2a gates:

```text
SDK Create median                  <=25 ms
SDK Create p95                     <=40 ms
SDK End median                     <=8 ms
SDK End p95                        <=12 ms
SDK End / same-seal fallback       <=1.15 paired median ratio
SDK create->edit->end median       <=60 ms
SDK create->edit->end p95          <=90 ms
persistent CliSession Create       <=27 ms median, <=45 ms p95
persistent CliSession End          <=10 ms median, <=15 ms p95
CLI overhead above matching SDK    <=2 ms median, <=5 ms p95
zero Docker calls on daemon Create/Exec/End
fresh real FUSE helper/mount and zero residue
```

Every local Unix/loopback/container-network request and wait after the public
timer begins is included. Owner/container preparation is reported separately.

If E2a fails, keep the current one-call Create and zero-call End. Do not build
E2b to justify the daemon.

E2b direct Unix FUSE FD handoff is not part of this handoff. It requires a
separate measured trace showing current ProxyHost setup remains material and a
new explicit authorization. JIT Commit-during-active-execution is also separate
host/FUSE work and must not be implemented here.

## Terminal response

At completion, report:

```text
actual worktree path, branch, and starting ref
source/image seal and exact files changed
minimal architecture and internal protocol actually implemented
all removed or deliberately deferred scope
focused and full test/Clippy/build results
E0 route/auth/no-prewarm/resource result
every E1 round and the final same-seal daemon/Engine comparison
public and nested timing breakdowns
fresh-process/FUSE/fsync/output/Stop/disconnect proof
memory/FD/child leak proof
E1 Go or honest No-Go with exact evidence
E2a result only if E1 authorized it
append-only report and raw evidence paths
remaining external blockers or deferred production work
```

Do not report success from compilation, an internal microbenchmark, a warm
shell, hidden fallback, partial timing, or one cherry-picked sample. Stop only
after a genuine terminal experimental verdict or a proven external starting-ref
or environment blocker after all independent safe work is exhausted.

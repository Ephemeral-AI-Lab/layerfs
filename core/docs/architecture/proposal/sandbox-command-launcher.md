# Proposal: a long-lived command launcher per Sandbox

> **Status:** **Declined by the owner, 2026-10-18** (§13). Not implemented;
> kept as a record. `sandbox.exec` keeps one Docker exec per call. Written
> 2026-10-18 by the #352 liaison; an earlier approval (§12) was withdrawn the
> same day before any code was written. Nothing here changes current
> behaviour or the execution-ownership rules in
> [core/AGENTS.md](../../../AGENTS.md#filesystem-access-and-execution-ownership).
> Evidence: #352 finding 013.a1 (`core/docs/issues/352/fixes/013-a1-oneshot-exec-launch.md`,
> RESULTS.md `cb6dd1d84`) and the liaison's live `sandbox.exec` experiments
> (2,600+ calls through the SDK, 2026-10-18). Labels: **[E]** measured,
> **[I]** inferred.

## 1. Problem

Every `sandbox.exec` starts a new process through the Docker Engine
(`sdk/js/native/src/exec.rs` → `layerfs_api::internal::exec` →
`layerfs-sandbox/src/platform/docker/`):

1. `POST /containers/{id}/exec` on a new Unix-socket connection (`Connection: close`);
2. `POST /exec/{id}/start`, upgraded and attached for output;
3. `GET /exec/{id}/json` on a third connection for the exit code;
4. inside the VM: dockerd → containerd → shim → `runc exec` (namespace and
   cgroup entry) → `bash -c <command>`.

Measured cost of `exec true` [E]:

| Measure | Value |
| --- | --- |
| p50 (30 calls, twice) | 40.0 / 40.8 ms, bimodal: a 20–25 ms mode and a 37–57 ms mode |
| Floor | ~18–24 ms |
| Under host load | bursts of 100–300 ms per call |
| LayerFS's own share (SDK queue, discovery) | ~2 ms |
| Grows with exec count, Sandbox age, concurrency, Sandbox count | no |

The command itself adds nothing measurable (`true`, `:`, `ls`, `cat` all
p50 49–54 ms in one interleaved run) [E]. So the launch path, not the work,
is the cost: about 40 ms per tool call, of a one-shot Workspace's ~118 ms p50
(range 13) [E].

Cancel is worse: today it signals the process group from a second Docker
exec, then checks liveness with further execs up to 50 times, 100 ms apart
(`exec.rs` `cancel`, `alive`), because the container's first process (the
daemon) does not reap orphans [impl].

## 2. Proposal

Each **SDK process** opens, per Sandbox it uses, one **launcher session**:
a single Docker exec started once, running a small launcher program as the
Sandbox's command identity. Commands are then sent to the launcher over that
already-attached stream; the launcher forks `bash -c <command>` in the
Workspace directory and streams the output and exit status back.

```
today                                   proposed
SDK ──3 HTTP + runc──▶ bash -c cmd      SDK ══ one attached stream (opened once) ══▶ launcher
SDK ──3 HTTP + runc──▶ bash -c cmd                                            ├─ fork ▶ bash -c cmd
SDK ──3 HTTP + runc──▶ bash -c cmd                                            ├─ fork ▶ bash -c cmd
                                                                              └─ fork ▶ bash -c cmd
```

Expected per-call cost: one frame each way plus `fork`/`exec` of bash, a few
milliseconds [I, not measured]; the Docker Engine round trips and the runc
start are paid once per session.

## 3. Ownership

| Question | Answer |
| --- | --- |
| Who owns the launcher | The Sandbox runtime (`layerfs-sandbox` and the SDK binding), as the Docker exec path is today |
| Is it the filesystem daemon | **No.** The daemon keeps no exec, supervisor, launcher mode, command registry or exec protocol (core/AGENTS.md); the launcher is a separate process the daemon never talks to |
| Identity | The Sandbox's command identity (`uid:gid`), `HOME=/tmp`, working directory the mounted Workspace, exactly as today's `run()` |
| Filesystem semantics | Unchanged: commands reach the Workspace only through FUSE like any process; Commit captures what they wrote regardless of who launched them |
| Which process may use a session | Only the SDK process that opened it; other SDK processes open their own session. No new network port: the stream is Docker's authenticated exec attach |
| Reaping | The launcher makes itself a child subreaper and reaps its commands, so exit codes come from `waitpid`, not from Docker inspect, and finished commands do not stay zombies under the daemon |

Shipping: the launcher is a small program in the Sandbox image, built from
first-party source in the Sandbox runtime crate (new production code; its LOC
is reported when built). No new dependency.

## 4. Protocol (sketch)

Length-prefixed binary frames on the attached stream, multiplexed by a
per-session command id so several commands run at once (the replay runs up to
8 concurrently [E]):

| Frame | Direction | Content |
| --- | --- | --- |
| `Run` | SDK → launcher | id, argv (`bash -c <command>`), cwd, env, foreground or background |
| `Started` | launcher → SDK | id, pid, process-group id, start time (from `/proc`) |
| `Out` / `Err` | launcher → SDK | id, a bounded chunk |
| `Exit` | launcher → SDK | id, exit code or terminating signal |
| `Cancel` | SDK → launcher | id or (pid, start time), signal |
| `Refused` | launcher → SDK | id, exact reason (cannot fork, bad cwd, …) |

Version the frame format from the first frame (`Hello` with a version);
a mismatch fails the session exactly.

## 5. Output limits

Keep today's contract [impl]:

- Foreground: the SDK keeps the first 1 MiB of each stream, counts every byte
  and reports `truncated`. The launcher forwards output in bounded chunks
  (for example 64 KiB) and never holds a whole command's output.
- Backpressure, not caps: if the SDK reads slowly, the stream's flow control
  blocks the launcher's read of that command's pipe, and the command blocks on
  write, as with Docker today. No total output, time or command-count cap is
  introduced (AGENTS.md: no artificial caps; ordinary Bash has no automatic
  runtime timeout).
- One slow reader must not stall other commands on the session: the launcher
  keeps a bounded per-command buffer and stops reading only that command's
  pipe when it is full.
- Background: output goes to `/tmp/layerfs-<process>.log` as today; nothing
  streams.

## 6. Failure

One attempt per operation, exact outcomes, no retry (AGENTS.md §4):

| Case | Outcome |
| --- | --- |
| Session cannot be opened (Docker exec of the launcher fails) | the call fails exactly with Docker's error; no fallback to a per-call Docker exec (one route) |
| `Run` refused by the launcher | `Refused` with its reason; nothing ran |
| Session lost before `Started` | outcome `Unknown`: the command may or may not have started; never resent |
| Session lost after `Started`, before `Exit` | outcome `Unknown` with the pid and start time; the command keeps running (as when a Docker exec client disconnects today); never killed implicitly |
| Launcher crashes | the session is lost (rows above); the next call opens a new session as a new operation, never a replay of the lost one |
| Workspace not mounted | `NotMounted`, unchanged |

A lost reply does not undo a command, and Bash exit is not proof that the
filesystem work is drained; those rules are unchanged.

## 7. Cancel

- Background processes: `Cancel` sends `TERM` to the process group (identity
  checked by pid and start time, as today) through any session in the same
  container. The launcher that started it learns of the exit by `waitpid`; a
  canceller in another SDK process confirms it from `/proc` in the launcher
  (one frame, not 50 Docker execs). The bounded wait stays (about 5 s), then
  reports `Stopped` or the exact still-running status.
- Foreground commands: today there is no per-exec cancel. With a launcher it
  becomes possible (`Cancel` by id). **Owner decision:** add it, or keep
  foreground cancel out of scope.
- Forced Workspace teardown still does not kill caller-owned processes
  implicitly (core/AGENTS.md).

## 8. What changes and what goes

- `sdk/js/native/src/exec.rs`: `run()`, `start_time()`, `alive()` and the
  cancel polling move to the session; process records and discovery stay.
- `layerfs-sandbox` Docker backend: keeps container create/start/stop and the
  one exec that starts the launcher.
- The per-call Docker exec route is removed after the switch (one route);
  tests of it are deleted, not adapted, and surviving behaviour (exit codes,
  output bytes, truncation, identity, cwd, background survival, cancel) is
  re-covered on the launcher (AGENTS.md test rule).
- Independent of this proposal and smaller: exec should stop running the full
  `discovery::refresh()` on every call (one daemon status per mounted
  Workspace: traced exec p50 49.6 → 107.0 ms from 1 to 16 mounts [E]).

## 9. Proof before switching

1. Equivalence, old route vs launcher, same commands: exit codes (including
   signals), stdout/stderr bytes, truncation flags, uid/gid, cwd, environment.
2. Background survives the SDK process exiting; `list_processes` and cancel
   work from a second SDK process.
3. Lost session before and after `Started` gives `Unknown`, and the command
   is neither resent nor killed.
4. Eight concurrent commands on one session, one of them writing 100 MiB of
   output slowly read, the others finishing without waiting for it.
5. Timing: `exec true` p50/p90 over 30 calls, old vs new, same cache state;
   one-shot Workspace p50 in the replay; reported raw.
6. No zombies left in the container after 1,000 commands.

## 10. Risks

- A new long-lived process per SDK process per Sandbox: a few MB each [I].
- Docker Desktop's attached streams must stay healthy for hours; a dropped
  stream is a lost session (§6), visible, not silent.
- The launcher is first-party code running inside every Sandbox; its frame
  parser must be strict and bounded.
- The 20–40 ms saved per call matters most for per-tool-call orchestration
  (the expected common case); long-running commands gain little.

## 11. Decisions for the owner

1. Adopt a launcher at all, or keep a Docker exec per call.
2. Transport: per-SDK-process session over one Docker exec stream
   (recommended: no new port, Docker's authentication) or a socket published
   by the Sandbox.
3. Foreground cancel: add it, or keep it out of scope.
4. Timing: before or after #352 P3 (the replay's Sandbox share is about one
   call in ten, so P3 and later ranges gain the most).

## 12. Owner decisions, 2026-10-18

1. **Adopt** the launcher; the per-call Docker exec route is removed after the
   switch (one route, no fallback).
2. **Transport:** one launcher session per SDK process and Sandbox over one
   Docker exec stream; no published port.
3. **Foreground cancel:** out of the first version; background cancel moves
   to the launcher.
4. **Timing:** before #352 measured P3, built in parallel by a separate
   worker on its own branch (not the fixer), touching the SDK exec code and
   `layerfs-sandbox` only. #352 F-029 (per-call `discovery::refresh` on exec)
   is folded into the same work. The switch shares the P3 re-baseline after
   F-027 rather than forcing a second one.

## 13. Declined, 2026-10-18

The owner withdrew the approval in §12 the same day: the launcher is a new
command-launch route (a launcher program in every Sandbox, a custom frame
protocol, lost-session rules) to save about 35 ms per call, which is Docker's
process-start cost, not LayerFS work. The implementation session was stopped
before any change or commit.

Instead, within the current route:
- `sandbox.exec` keeps one Docker exec per call (create, start, inspect).
- #352 F-029: exec stops running the full `discovery::refresh()` on every
  call (one daemon `workspace.status` per mounted Workspace).
- Docker Engine connection reuse in `layerfs-sandbox/src/platform/docker/client/http.rs`
  instead of a new Unix-socket connection per request, one attempt per request.
Both go to the #352 fixer as small fixes. The gates judge LayerFS work
separately from process start (the split one-shot probe).

# Exec to edit: SDK-only Workspace benchmark guideline

> **Status:** Current planning checklist; no release candidate exists.

> **Route correction, 2026-09-25:** The user-facing Workspace shell route
> accepts arbitrary commands. An explicit range ioctl from a cooperating
> tool is a separate opt-in workload; its 56-case v3 functional result does
> not qualify generic shell edits. See the [mounted POSIX proof and revised
> phases](../../issues/232/SHELL_ROUTE_CORRECTION.md). The POSIX contract
> below remains the generic route baseline.

This guideline is the prospective #232 agent-edit route. It does not register
cases, set numeric gates, authorize collection, or relabel the 56 historical
direct SDK range-edit cases. Freeze a new family specification and update
[#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232) before adding
cases or samples. The [general measurement rules](../../../../docs/general/benchmark_rules.md)
still govern cache, timing, resources, custody and one-sample collection.
The [#232 implementation specification](../../issues/232/SPEC.md) fixes the
Edit → Commit boundary, release-build route, telemetry source and speed goal.

## Operation contract

The edit enters through public `layerfs-sdk::WorkspaceApi::exec` and runs a
command with its working directory inside the mounted Workspace. The command
uses ordinary POSIX syscalls on that FUSE directory; the edit is local and
unpublished until public `WorkspaceApi::commit` acknowledges it. The family
is a **Workspace Exec/FUSE edit** claim. Calling `exec` through the SDK does
not make the mutation a `Client::edit_workspace_file_range(s)` claim.
Use the v0.1.6 `edit_*` family names in the
[#232 specification](../../issues/232/SPEC.md), with new `-exec-v1` scenario IDs
and an explicit SDK Exec/FUSE operation contract in every receipt. Matching
family names do not make these rows direct-SDK range-edit claims or allow old
PASS results to be reused.

```text
first-use SDK Init → sealed master       one measured Edit-and-Commit attempt
per-case clone → layerfs-server open      WorkspaceApi::exec(command)
→ Branch → Sandbox → Mount               → check exit/output
                                         → WorkspaceApi::commit(workspace_id)
                                         → acknowledgement

untimed cleanup                          Workspace.status → Unmount
                                         → Sandbox.delete
separate verification                    independent bounded reopen oracle
```

The caller records `exec_ns`, `commit_ns` and the enclosing `edit_commit_ns`
through `layerfs-telemetry` timing scopes and native LFT1 output. The enclosing
timer starts immediately before `WorkspaceApi::exec` and ends on the typed
Commit result. It includes shell startup, command I/O, FUSE callbacks, host
delivery and publication. It does
not include fixture creation, Project Init, Branch creation, Sandbox Create,
Mount, Status, Unmount, Sandbox Delete or verification; those have separately
reported wall times.
An Exec failure or unknown Commit result remains a failed/uncertain attempt,
not a success or a replay opportunity.

## SDK-only gate

Preparation and performance must call the public `layerfs-sdk` package for
**every product operation**: Project Init, Branch creation, Sandbox Create,
Mount, Exec, Commit, Status, Unmount and Sandbox Delete. The performance driver
may read declared
fixture/oracle metadata, emit
receipts and call `layerfs-telemetry` for operation wall/CPU/RSS. Python may
prepare sealed input files, build immutable release artifacts, launch and
supervise the SDK driver, check cache eligibility, record complete-command wall,
and run a separate verifier. It must not supply an alternative operation timer
or CPU/RSS sampler. Neither Python nor the driver may perform the measured
mutation directly, call internal Service/Bridge/Store/
Workspace APIs, or use `docker exec` as an alternate editing path. Sandbox
teardown uses public `SandboxApi::delete`; external supervision may capture
logs and enforce deadlines but cannot replace that product operation.

The SDK package's implementation modules are `project`, `sandbox` and
`workspace`. Rename the existing `layerfs-service` package to
`layerfs-server`: keep its Service handler, move the native listener module
from `src/server/` to `src/host/`, group the request handler and its Project
Init adapter under `src/service/`, and add
Store/history, telemetry and SandboxOwner assembly there. The SDK constructs
its API objects from a borrowed Server; Server does not depend on SDK. The
existing SDK `Host` and `Client` types must move/retire with their Init callers
before #232 registration. Do not add a second acceptor implementation.

**Resolved:** the Core SDK now exposes `ProjectApi::fork`, `Server::open`,
`SandboxApi::delete` and post-timer `WorkspaceApi::status`, and the registered
release driver performs every product operation through them. The family is
registered under scenario version 2 and collected once per case; the earlier
`NOT_RUN` state and its receipts stay historical. A benchmark-only direct
Service fork or Docker owner remains unacceptable. Keep the existing #236
functional receipt under its own identity. Results and open blockers:
[the #232 baseline report](../../issues/232/exec-fuse-edit-v2-baseline.md).

## Commands and fixture custody

- Freeze each command string, executable/image digest, working directory,
  payload source and declared editor algorithm in the case registry. A tiny
  `printf` edit is a functional canary, not a 1–500 MiB workload.
- A deterministic tool invoked *by* `WorkspaceApi::exec` may use `pwrite`,
  `ftruncate`, append, or a declared temporary-file-and-rename save. Its bytes
  must pass through the mounted FUSE path. Record the tool binary hash and
  exact arguments. A full-file rewrite is named and measured as such; it is
  not an in-place range-edit equivalent.
- Keep source fixtures and expected-result data sealed and independent. No
  path pre-touch or recent setup write may credit a timed read. Declare and
  enforce one cache state for every comparable arm; check residency for a
  cold claim. A clone is setup reuse, not a cold claim.
- The edit timer sees one declared Exec command and one explicit Commit.
  Record public call count, child process outcome, product projection
  read/write/upstream counts from a public SDK status call **after** the timer,
  and any fallback/forbidden route. Current status wire omits these counts, so
  the #232 spec makes that a product prerequisite. Count size SETATTR/rename
  for cases that use them; do not invent byte totals. Independent post-Commit
  state must prove the declared mutation.
  Unexpected mutation outside FUSE invalidates a row.

## Verification and admission

Verify the command's exit status and bounded output inside the timed closure.
After the timer, independently reopen the published Branch and check declared
boundary bytes, size, portable metadata, canonical root/count and expected
history; state `full_file_bytes_verified=false` when using the bounded fast
oracle. Check retained older roots, conflict semantics and cleanup where
applicable. The independent verifier has a hard 15 s ceiling under the owner's
#232 ruling, with <10 s as the design goal. Keep its wall separate from
performance. Raw `layerfs-telemetry` LFT1 spans and process CPU/RSS are retained
with their process scope. The host caller and Service share one process;
daemon RSS/CPU exclude the edit child process. No shared-process CPU or sampled
RSS is called exclusive phase cost or exact peak.

The runner must fail closed when any required SDK-only route or projection
field, cache
proof, source/binary/image identity or verifier receipt is absent. Keep every
`FAIL`, `INELIGIBLE` and `NOT_RUN` row. Collect one performance sample per
registered case and arm at a frozen source, with separate identity-matched
verification. Old range-edit receipts and the #236 five-call diagnostic are
different operation surfaces and cannot supply an Exec-to-edit PASS.

The Core runner now rejects direct backend crate references and common host
process/file mutation patterns in registered `benchmark_*` driver source
before build or binary reuse. This is a structural guard, not a complete
proof of the executed route; the prospective Exec family must also record
public SDK calls and product projection counts from the real route.
Its separate verifier may use independent public
C1/C2/C5 readers as an oracle, outside the performance driver and timer.

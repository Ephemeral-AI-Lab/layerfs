# Exec to edit: SDK-only Workspace benchmark guideline

> **Status:** Current planning checklist; no release candidate exists.

This guideline is the prospective #232 agent-edit route. It does not register
cases, set numeric gates, authorize collection, or relabel the 56 historical
direct SDK range-edit cases. Freeze a new family specification and update
[#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232) before adding
cases or samples. The [general measurement rules](../../../../docs/general/benchmark_rules.md)
still govern cache, timing, resources, custody and one-sample collection.

## Operation contract

The edit enters through public `layerfs-sdk::WorkspaceApi::exec` and runs a
command with its working directory inside the mounted Workspace. The command
uses ordinary POSIX syscalls on that FUSE directory; the edit is local and
unpublished until public `WorkspaceApi::commit` acknowledges it. The family
is a **Workspace Exec/FUSE edit** claim. Calling `exec` through the SDK does
not make the mutation a `Client::edit_workspace_file_range(s)` claim.

```text
untimed, declared setup                  one measured edit-and-Commit attempt
Project Init → Branch → Sandbox → Mount   WorkspaceApi::exec(command)
                                         → check exit/output
                                         → WorkspaceApi::commit(workspace_id)
                                         → acknowledgement

untimed, separate verification            independent reopen + exact oracle
```

The caller records `exec_ns`, `commit_ns` and the enclosing `edit_commit_ns`
from monotonic caller timers. The enclosing timer starts immediately before
`WorkspaceApi::exec` and ends on the typed Commit result. It includes shell
startup, command I/O, FUSE callbacks, host delivery and publication. It does
not include fixture creation, Project Init, Branch creation, Sandbox Create,
Mount, verification or cleanup; those have separately reported wall times.
An Exec failure or unknown Commit result remains a failed/uncertain attempt,
not a success or a replay opportunity.

## SDK-only gate

The performance driver must call the public `layerfs-sdk` package for **every
product operation**: Project Init, Branch creation, Sandbox Create, Mount,
Exec, Commit and Unmount. It may read declared fixture/oracle metadata, emit
receipts and run monotonic clocks. Python may prepare sealed input files,
build immutable artifacts, launch the SDK driver, supervise it, sample external
resources, and run a separate verifier. Neither Python nor the driver may
perform the measured mutation directly, call internal Service/Bridge/Store/
Workspace APIs, or use `docker exec` as an alternate editing path. Docker
cleanup after the sample is external supervision and must be recorded.

**Current blocker:** the Core SDK has `ProjectApi::init`, `SandboxApi::create`
and `WorkspaceApi::{mount,exec,commit,unmount}`, but the live route test forks
its Branch with direct `Service::handle`, and `Host::create` does not assemble
the sandbox owner. Add public SDK setup for those operations before an
SDK-only edit driver is registered. Until then the family is `NOT_RUN`; a
benchmark-only direct Service fork or Docker owner is not an acceptable
shortcut. Keep the existing #236 functional receipt under its own identity.

## Commands and fixture custody

- Freeze each command string, executable/image digest, working directory,
  payload source and expected syscall behavior in the case registry. A tiny
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
  Record public call count, child process count, FUSE read/write and metadata
  callback counts, upstream Service calls, bytes read/written, and any
  fallback/forbidden route. Unexpected mutation outside FUSE invalidates a row.

## Verification and admission

Verify the command's exit status and bounded output. After the timer,
independently reopen the published Branch and compare exact bytes, size,
portable metadata and expected history; check retained older roots, conflict
semantics and cleanup where applicable. Keep the verifier and its wall time
separate from performance. Raw `layerfs-telemetry` LFT1 spans and process
CPU/RSS are retained with their process scope; external command/cgroup wall
and memory are labeled separately. No shared-process CPU or sampled RSS is
called exclusive phase cost or exact peak.

The runner must fail closed when any SDK-only route field, FUSE count, cache
proof, source/binary/image identity or verifier receipt is absent. Keep every
`FAIL`, `INELIGIBLE` and `NOT_RUN` row. Collect one performance sample per
registered case and arm at a frozen source, with separate identity-matched
verification. Old range-edit receipts and the #236 five-call diagnostic are
different operation surfaces and cannot supply an Exec-to-edit PASS.

The Core runner now rejects direct backend crate references and common host
process/file mutation patterns in registered `benchmark_*` driver source
before build or binary reuse. This is a structural guard, not a complete
proof of the executed route;
the prospective Exec family must also record public SDK call and FUSE counts
from the real runtime. Its separate verifier may use independent public
C1/C2/C5 readers as an oracle, outside the performance driver and timer.

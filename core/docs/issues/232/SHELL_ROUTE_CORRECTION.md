# #232 Workspace shell route correction — 2026-09-25

**Status:** The user-facing arbitrary-command edit goal remains open. The
completed 56-case v3 campaign proves the opt-in cooperating ioctl route only.
This correction changes no historical registry, target or receipt.

The public SDK method is `WorkspaceApi::exec(command)`; “Workspace shell” is
the product concept, not another SDK method. The daemon runs `/bin/sh -c` with
the mounted Workspace as its working directory. It neither parses the shell
text nor turns an ordinary file write into a range-replacement request. The
program's syscalls determine the FUSE callbacks. An ordinary `WRITE` passes
an offset and bytes to `write_file`; `SETATTR` can change size; `RENAME` changes
the directory entry. The checked range ioctl is a separate callback that a
cooperating program must explicitly issue on an open mounted descriptor.
The ordinary write mutation overwrites the supplied range and can extend EOF;
it has no byte-insertion or suffix-shift intent to infer. See the
[Exec launcher](../../../crates/layerfs-daemon/src/execution.rs),
[FUSE adapter](../../../crates/layerfs-fuse/src/adapter.rs), and
[Workspace mutation](../../../crates/layerfs-workspace/src/filesystem/write.rs).

## One mounted observation

The committed [one-attempt pre-run contract](evidence/shell-posix-diagnostic/PRE_RUN.json)
and [runner](evidence/shell-posix-diagnostic/run.py) selected a fresh byte copy
of the sealed 1 MiB master, the pinned release image and SDK driver, and this
arbitrary shell command:

```sh
printf 'ABCD' | dd of=payload.bin bs=4 seek=131072 conv=notrunc 2>/dev/null
```

The command ran through public SDK Project fork, Sandbox create, Workspace
mount, `exec`, Commit, Status, Unmount and Sandbox delete. On execution commit
`77b0f8fee7d82411edf7558db066193fed3e112a`, its one complete command
returned in 914.079 ms. Status recorded `write=1`, `range_state=0`,
`range_edit=0`, `setattr=0`, `rename=0`; accepted range-ioctl payload and
shifted-suffix counters were both zero. The independent read-only verifier
checked the entire published file against SHA-256
`946380842d9c48c82944c7b84c46dd841820abf82d5c99b9c141cf50e8062c01`,
confirmed the retained pristine root, and passed. Unmount and public Sandbox
delete passed. The append-only raw output and receipt are in
`benchmark-results/fs-bench-pro/shell-posix-diagnostic-v1/` on the execution
worktree; [the retained summary](evidence/shell-posix-diagnostic/RESULT.json)
pins their hashes. Cache state is uncontrolled; this is a functional route
diagnostic, **not** a latency sample, cold-cache PASS, or proof for all shell
commands.

## What the existing campaigns establish

| Selection | Edit command and observed outcome | Applicable claim |
| --- | --- | --- |
| [Scenario v2](exec-fuse-edit-v2-baseline.md), 56 cases | POSIX mounted editor; 45 completed and verified, 11 structural shifts hit the 5 s Exec progress limit. All latency rows were cache-ineligible. | The generic POSIX/FUSE workflow baseline, with the named editor algorithms. |
| [Scenario v3](evidence/phase2-all-ioctl/REPORT.md), 56 cases | Cooperating mounted editor explicitly issued STATE/EDIT or staged BEGIN/DATA/APPLY ioctls; 56 completed and verified, zero ordinary FUSE WRITE callbacks. All latency rows were cache-ineligible; capped-500 MiB raw insert was 87.607 ms against a 70 ms aim. | The opt-in ioctl workflow only. |
| This diagnostic | `/bin/sh -c` ran `dd`; one FUSE WRITE and zero range-ioctl callbacks. | One ordinary overwrite route and published-byte proof. |

The ioctl results cannot be relabelled as generic shell-command results. A
fixed-size overwrite can benefit from a faster implementation of ordinary
`WRITE`, and append/truncate from their own POSIX callbacks. An arbitrary
command that reads and rewrites a suffix still requests those bytes through
FUSE: LayerFS can reduce per-callback metadata cost and Commit overhead, but
cannot make the command's `O(M)` reads/writes disappear without changing the
command or its filesystem semantics. Callback sequences alone do not identify
one atomic insert/delete request.

## Revised phases

1. Keep the independent [Phase 1B `service.finish` count and telemetry work](../241/evidence/phase1b-finish-diagnostic/REPORT.md).
   Optimize a measured substep only when its own counts support the change.
2. Keep v3 and the 128-edit ioctl liveness screen as opt-in evidence. Pause
   staged all-56 ioctl work as a proposed solution to arbitrary shell edits.
3. Use the frozen v2 POSIX commands and eleven retained failures to locate
   actual generic-route limits. Count bytes requested by the editor, FUSE
   callbacks, per-write piece-index work and Commit work; use cause-finding
   diagnostics rather than resampling the same arm. Improve the shared
   `WRITE`/`SETATTR` and Commit paths where counts justify it, preserving the
   one-worker and 5 s progress contracts.
4. Before new performance admission, freeze the actual shell commands,
   editor algorithms, expected callbacks and output oracle under a new source
   identity. Enforce an equal declared cache state, collect one sample per
   case, retain failures and verify at the same identity. An uncontrollable
   Edit-written FUSE backing cache leaves a numeric row `INELIGIBLE`.

The existing opt-in ioctl remains useful for programs that choose its API.
It is a separate capability and performance selection from arbitrary shell
commands.

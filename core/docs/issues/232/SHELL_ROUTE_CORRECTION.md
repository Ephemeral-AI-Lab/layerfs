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

## Phase 2 feasibility finding: the eleven failures also exceed replay capacity

The [reproducible v2 limit derivation](evidence/shell-posix-diagnostic/derive_v2_limits.py)
reads the frozen registry and retained baseline and produces
[all 20 shift case counts](evidence/shell-posix-diagnostic/V2_LIMITS.json).
It finds that **exactly the eleven retained failures** move more than the
current 8 MiB replay limit. The 10 MiB prepend moves 10 MiB; a 100 MiB
middle insert moves 50 MiB; a capped-500 MiB middle insert moves almost
250 MiB. The benchmark tool reads and writes each affected byte in 128 KiB
blocks. Every successful ordinary write owns Local replacement bytes in the
existing file's private overlay. The current `pieces::splice` refuses an
existing-file overlay with more than `MAX_REPLAY = 8 MiB` of non-Base pieces;
the Bridge independently limits `EditFile` input to 8 MiB. A faster Exec
progress path cannot make those exact in-place commands fit this contract.

The *observed* failure is still the 5 s silent Exec result `Unknown` in the
v2 receipts. None of those receipts observed a Capacity error because they
stopped first. The capacity result is a source-and-registry deduction, not a
relabelled outcome or a second sample. It also explains why optimizing only
the current all-piece rebuild cannot finish the 56-case POSIX selection.
Each WRITE currently reloads all pieces, splices over the list and rebuilds
all piece-index pages, so the 128 KiB shift blocks add rising metadata work.
The already retained 10 MiB shifts took about 2.7–2.8 s for 40 blocks. The
larger cases need hundreds to thousands of blocks and exceed the replay
limit independently of that time growth.

A fresh temporary file followed by RENAME avoids the existing-file replay
limit but does not make the capped-500 MiB tier ready. FUSE advertises a
128 KiB maximum WRITE; the current callback acquires a separate payload
record for each write, and adjacent Local pieces coalesce only when they
share that payload identity. Writing 500 MiB therefore needs at least
**4,000** callbacks and distinct pieces, while the current piece vector caps
at **1,024**. This is another source bound, not an observed 500 MiB
temp-file receipt. A temp-file workflow changes the registered editor
algorithm and still needs a piece-index/product design for the largest tier.

## Five revised checkpoints

| Phase | Gate, commit and verification | Current outcome |
| --- | --- | --- |
| 1. Establish the user-facing route | Commit `8ec08e800` records the pre-run, one public SDK mounted POSIX probe, independent full-file verifier and corrected workflow. | **PASS** for the one ordinary overwrite route; no broad command or latency claim. |
| 2. Explain retained structural failures | `derive_v2_limits.py` checks 56 registry rows, 20 shifts, the 8 MiB source limit and exact agreement between eleven over-limit rows and eleven retained v2 failures. | **PASS** for the source-and-registry deduction; the historical observed outcome remains 5 s Exec `FAIL`. |
| 3. Select a capacity-preserving generic POSIX design | The current in-place 10/100/500 MiB shifts cannot fit existing Workspace/Bridge replay. A temp-file-and-rename command is a different editor algorithm and also crosses the 1,024-piece limit at 500 MiB. The independent [Phase 1B drain diagnosis](../241/evidence/phase1b-finish-diagnostic/REPORT.md) stays valid, with no narrow optimization yet justified. | **BLOCKED on route/contract selection and piece-index design.** No cap, worker or timeout inflation is an acceptable shortcut. |
| 4. Prove all 56 generic-shell cases | Freeze the selected actual shell commands, source/build/image identities, callback counts and byte oracles; then verify one fresh changed-source attempt per case. | **NOT_RUN.** The v2 POSIX baseline is 45 verified, eleven failed. The v3 ioctl campaign is a separate selection. |
| 5. Qualify performance and release | Enforce an equal cache state for Edit-written backing bytes, retain one sample per case, independent verification and cleanup, and compare only prospective same-route targets. | **NOT_RUN.** The current FUSE backing-cache domain is ineligible for cold latency. |

Phase 3 requires an explicit route decision because preserving the exact v2
in-place commands needs a new private-overlay and replay architecture, while
choosing ordinary temp-file-and-rename commands changes the benchmark
operation and still needs a larger piece index for 500 MiB. Either path must
preserve arbitrary `WorkspaceApi::exec` semantics; neither may be labelled
as the existing v3 ioctl result.

The existing opt-in ioctl remains useful for programs that choose its API.
It is a separate capability and performance selection from arbitrary shell
commands.

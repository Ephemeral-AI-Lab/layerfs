# Authenticated daemon CloseClean

> **Status: implemented and verified through the actual authenticated daemon/Linux mount route; target v0.1.7, not released.**
> Implementation parent: `dab1751312adecdc57d073145582a9a702449744`.
> Product input seal: `e3201c7c956df6a832857172631f46f8d007961488f75ac26e597e57a3672304`.
> Selected 2026-09-22 as the next individual R1-C lifecycle operation. The
> overlay-release rule below was refreshed in the issue-179 documentation round
> against product source `f802cc124`.

## Operation and exact completion

`Operation::WorkspaceCloseClean { workspace, incarnation }` uses the existing
profile 3 native control route, opcode 11, and tag 13 result. Request ID is positive;
Store, generation and response-data allowance are zero. Existing managed-name
ASCII/63-byte and exact nonzero incarnation validation apply. Input is empty,
including authenticated END_INPUT. Request/result bounds are 124/100 bytes;
the declared request budget remains 1..5000 ms.

The proposal's Unmount-only Rust DTO names become the concrete shared
`WorkspaceLifecycleWire` and `WorkspaceLifecycleOutcome::{Completed, Retained}`.
There are no compatibility aliases for unreleased names. Unmount remains opcode 10,
tag 12, and byte-for-byte the same wire representation. Each operation still has
its own Response variant; the native Client requires that exact variant, identity
and zero data. Cross-operation terminals, lost/malformed replies and illegal data
produce Unknown without replay. Both operations remain lifecycle mutations, with
no content/history-metadata mutation or Store permission bit.

Daemon grants add CloseClean bit 4; Status 1 and Unmount 2 retain their meanings,
and mask 0..7 is valid. Existing mask 3 peers gain no CloseClean permission. Service
owners explicitly refuse all three controls before Store lookup/admission/input.
No new endpoint, client, framing, credentials or service-private algorithm is added.

CloseClean reuses the lifecycle try_lock and original deadline with 100 ms terminal
headroom. Complete input, authorization, exact launched identity, expiry, deadline
and control-stop checks precede native entry. It calls only the existing
`Workspace::close_clean_until`; it does not implicitly unmount, Commit or discard.
Mounted, dirty, submitted, active, held or externally pinned states retain the
native Busy refusal. The daemon profile was read-only when this round ran, so
dirty/submitted network routes were not qualified then; the writable close is now
evidenced end to end by the real-daemon small-project scenario, whose checked
Unmount + CloseClean follows two explicit Commits
(`core/target/pair1-evidence/round57/final3-t1-small-project-01`).

**The overlay release.** The live overlay is the Workspace's own reference to
its arena roots, exactly like the generations a Commit retires: a Commit whose
successor root carries records leaves the arena unreclaimable if the overlay
keeps holding it, so `close_clean_until` takes and drops `state.overlay` before
the arena is reclaimed (`runtime/lifecycle.rs`). Nothing runs after `stopping`
is set, so a refused teardown has no later operation to mislead; a later
explicit request continues the checked cleanup.

Pre-admission refusals use Failure. Once native cleanup is entered, incomplete
cleanup returns Retained(Busy/Deadline/Unsupported/Io), preserving the same owner
and its checked state. Backing TimedOut maps to Deadline. Retained does not promise
unchanged state: cleanup may set stopping or remove owned backing before failing.
A later explicit request can continue checked cleanup; transport uncertainty never
authorizes automatic replay or guessed deletion. Completed means this exact target
is closed; checked already-closed state is idempotently Completed. Response identity
is allocated before native work and the lifecycle mutex is released before sending.

## Closed target and shutdown

The fixed daemon target remains inspectable through Status after clean closure,
with the same incarnation, closed=true, and zero active/node/handle/cookie counts.
Closure releases the native Workspace table/registry ownership, not every remaining
Arc/control reference; consumer accounting is not promised to become zero. No new
Attach, replacement target, remount, process exit or restart recovery is implied.

Explicit signal shutdown still joins control before using the same mount owner.
After checked Unmount, it skips CloseClean when Status already confirms closed.
It therefore handles a previously successful remote CloseClean, including a lost
terminal, without turning a second native close into a shutdown error. Incomplete
signal cleanup still retains process/owners until another explicit signal.

Resources remain the existing fixed target, one lifecycle mutex, Q=0 session,
response box and bounded grant byte. No new worker, queue, FD, payload window,
Workspace budget, cache or dependency is added. Shared lifecycle DTO layout and
100-byte wire envelope are unchanged. These are source bounds, not RSS/cgroup
measurements, performance results or durability guarantees.

## Declared verification and next dependency

The actual Linux daemon selections are clean unmounted close and repeated explicit
close; mounted/held Busy without stopped presentation; independent operation/target/
incarnation/service authority; failed mount-leaf removal with retained state followed
by explicit cleanup; and lost terminal with Unknown followed only by fresh Status.
Every selection checks later ordinary signal shutdown. Existing Unmount regressions
exercise the shared lifecycle admission and teardown path. Codec/native socket tests
cover bounds, exact operation/identity correlation, retained versus pre-admission
failures and uncertainty. Locked Rust 1.85.1 checks and raw receipts are recorded below.

Remote Attach/Mount and writable daemon startup/edit/Commit remain separate public
operations. Before writable startup, select shutdown/control admission ordering so
a dirty shutdown refusal does not strand a Workspace after stopping its only Commit
control. Namespace/new-inode/larger-input/npm and matched R6 remain open; prior
mounted WRITE/SETATTR limitations and source-pinned evidence are retained. No issue
is closed and this round does not complete Pair 1.

## Actual checks and routes

All five new CloseClean selections and eight affected Unmount regressions pass:
**13 PASS, zero failures**. Each case creates an actual Linux 6.12.76-linuxkit
AArch64 FUSE mount with the production daemon and calls its authenticated endpoint
through the existing native Client. Each case checks normal later signal cleanup
and removes its owned container/volume. No same-worktree build overlaps these
selections. Other-worktree interference snapshots remain in each receipt.

| Selection | Complete command seconds |
| --- | ---: |
| CloseClean close_authority | 1.967692084 |
| CloseClean close_cleanup_failure | 1.160179125 |
| CloseClean close_loss | 1.109457791 |
| CloseClean close_mounted | 1.044679334 |
| CloseClean close_success | 2.779631375 |
| Unmount regression authority | 1.793639834 |
| Unmount regression expiry | 10.368823292 |
| Unmount regression loss | 1.090825125 |
| Unmount regression partial_input | 1.341870458 |
| Unmount regression shutdown_retained | 11.070283667 |
| Unmount regression signal | 1.022676500 |
| Unmount regression success | 1.014127750 |
| Unmount regression timeout | 1.428074959 |

The clean-close case observes the same target/incarnation after Unmount and after
CloseClean. Its active operations, nodes, handles and cookies become zero, and the
mount leaf disappears. Aggregate accounted Workspace allocation falls from 1420380
to 24664 bytes; remaining host/target references stay owned. This is declared
accounting, not measured RSS or a memory qualification. A second explicit CloseClean
returns Completed; ordinary SIGTERM then exits normally.

A held actual FUSE file causes Retained(Busy) while mounted=true, stopping=false
and handles=1. After release, explicit Unmount and CloseClean succeed. An externally
created owned file in the unmounted mount leaf makes native remove_dir fail: the
result is Retained(Io), stopping=true, closed=false, with node/accounting ownership
unchanged. The exact owned file is verified and removed by the test; a later explicit
CloseClean completes. There is no implicit content deletion or automatic retry.

Former mask 3/no-permission peers and wrong target/incarnation are refused. A bit 4
peer cannot Status or Unmount but can CloseClean. The actual service endpoint refuses
CloseClean even with every Store grant. A ciphertext-only proxy drops the successful
terminal: the native Client reports Unknown; only one CloseClean is submitted.
Later Status observes closed, and later signal cleanup succeeds. Neither observation
is presented as the missing operation receipt.

Bridge tests preserve literal Unmount bytes, Close bounds and cross-operation
response rejection in both directions. Ten authenticated socket scenarios include
Completed, Retained(Deadline), pre-admission Deadline, wrong identity, malformed/lost
terminal and illegal result data. Existing exhaustive 256-code lifecycle validation
is reused. The service public test refuses all three controls before reading input
or acquiring Store admission. Source review found no concrete correctness/security
findings; source reasoning is distinct from these runtime routes.

Rust 1.85.1 host whole-core tests: **659 passed, zero failed, three ignored**.
Linux Bridge/daemon/service tests: **86 passed, zero failed/ignored**. Host and
Linux whole-core all-target warning-denying Clippy and bins/examples builds pass.
Formatting, the 241-file product boundary guard and all six self-tests pass. No
compile, lint, unit or functional failure occurred. The full prior Status-only
selection and writable mount suites were not rerun: their source-pinned evidence
remains in 27 and 26; this round exercises Status directly in every lifecycle case
and reruns every affected Unmount case. No earlier receipt is promoted.

[Raw functional receipts and explicit NOT_RUN rows](evidence/control-close-clean/functional-index.json),
[compiled/executed identities](evidence/control-close-clean/control-close-inputs-01.json),
[exact source inventory](evidence/control-close-clean/control-close-source-01.json),
[check commands](evidence/control-close-clean/checks/commands.json), and
[independent source review](evidence/control-close-clean/source-review.json).
Each functional complete command is below 60 seconds. These values are budget
observations, not performance results. Fixtures reuse independent byte copies of
the closed read-only Store/history; no warm/cold performance or writable-history
recovery claim is made. Construction workers remain 1; ARM config SHA256 is
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.

Reproduce each declared case once with fresh output and the identities above:

```sh
python3 core/crates/layerfs-daemon/tests/control_close.py \
  --fixture core/target/pair1-evidence/mounted-07/result.json \
  --binaries <host_binaries-from-inputs-01> \
  --linux-daemon <linux_daemon_binary-from-inputs-01> \
  --case close_cleanup_failure --output <fresh-owned-output>
```

Use control_unmount.py for the eight named regressions. Neither driver builds or
changes product source. Raw failures would remain append-only; none occurred here.
No durability, performance, full writable management or Pair 1 completion is claimed.

## Exact production LOC and changed files

First parent `dab1751312adecdc57d073145582a9a702449744`; counted staged tree
`54094c02a538a9ba7dfc4edfbb58e93ca9ad3630`. Production LOC: **108474 → 108562
(delta +88)**. Reference65417 →65417 (+0); core43057 →43145 (+88).
Bridge4288 →4340 (+52), daemon790 →824 (+34), service2029 →2031 (+2).
Workspace/FUSE algorithms are unchanged. Growth adds this authenticated lifecycle
operation and checked closure handling; no reference retirement or relocation.
[Changed source/test files](evidence/control-close-clean/changed-files.json).

Method: `git archive <revision> crates core/crates`, then identical
`python3 tools/production_loc.py --root <archive> --json`; counter blob
`b5b9617d08204977176302311e0b2c72a811b420`. Nonblank/noncomment production
Rust/runtime SQL excludes inline/external tests, fixtures, examples, docs, tooling,
manifests and generated output. [Exact comparison](evidence/control-close-clean/production-loc.json).
Final receipt/doc additions do not change the counted production tree.

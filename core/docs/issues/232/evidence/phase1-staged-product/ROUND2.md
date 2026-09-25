# #232 staged product functional round 2

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Live product functional checks only; no Edit→Commit performance admission.

The source-pinned [mounted capacity and refusal case](kernel-staged-bounds-01/result.json)
passed at test source `448b83c5ef8f4f8b3d808d25406a888fea15b5e3` with
unchanged staged product source `a1f745221121b27ed0d828bd50501282c154ddeb`.
On a fresh writable copy of the closed [64 MiB master](fixture-64m.json), it
verified `ENOSPC` for a declared replacement above 8 MiB before mutation,
`ESTALE` for a stale BEGIN stamp, `EINVAL` for bad digest at APPLY and wrong
DATA order, `EBADF` for a token used on another descriptor, and `ENOSPC` for
a second stage while one 8 MiB reservation exhausted the per-mount quota.
It then sent one Zero DATA descriptor of logical length exactly 8 MiB and
one APPLY. Same-FD and alias size, revision +1, zero boundary readback, EOF,
literal accepted-byte count 0, canonical Commit readback and cleanup passed.
Complete functional command wall was 2.342 s; cache state and LFT1 operation
scope were not qualified, so this is **not a latency row**.

The separate [close, deadline and unmount case](kernel-stage-cleanup-01/result.json)
passed at test source `17197d28cc505d176cdaa90f37f363a39445cf65`
with the same product source. Closing the owner descriptor released its 8 MiB
reservation, allowing another descriptor to BEGIN. After the frozen real
30-second expiry, a third descriptor acquired the full reservation without
a DATA or APPLY on the expired stage. `MountHandle::unmount` then started
with a private stage outstanding; the remaining descriptors closed during
its drain, and unmount/clean close succeeded with no retained container or
volume. Complete command wall was **31.963 s**, deliberately above the 15 s
performance-command cap: this is an expiry diagnostic only, never a benchmark
attempt or exception to that cap. The test does not expose private stage
state at the instant mount stop clears it; source review establishes the
`stop_admission` → `Stages::clear` ordering, while the live test establishes
successful drain and cleanup.

Both case receipts retain source, binary, immutable Docker image, kernel,
fixture and test identities plus exact stdout/stderr, selection and cleanup.
The test-only [carrier probe](../phase1-staged-carrier-probe/REPORT.md) separately
retains a lost APPLY reply yielding caller UNKNOWN. A product daemon crash at
the postpublication reply boundary is not claimed here. The remaining public
SDK Exec→ioctl→Commit proof, independent benchmark oracle, cache eligibility
and 56-case Phase 2 campaign are outside these functional checks.

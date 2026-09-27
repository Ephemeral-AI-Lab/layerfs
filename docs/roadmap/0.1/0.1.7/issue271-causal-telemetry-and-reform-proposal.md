# Issue 271: causal telemetry before a write-path reform

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.

This proposal follows the source audits of [Exec ownership](../../../../core/docs/issues/271/RESEARCH-OWNERSHIP-EXEC.md),
[payload and FUSE coherence](../../../../core/docs/issues/271/RESEARCH-PAYLOAD-FUSE.md),
and [Commit](../../../../core/docs/issues/271/RESEARCH-COMMIT.md). The
four-hop product source `933b3457c916519f458137e4c4f19662c8e9228e`
retains a [25 s public 4,097 gate FAIL](../../../../core/docs/issues/271/FOURHOP-GATE-FAIL.md).
The separate [60 s count diagnostic](../../../../core/docs/issues/271/FOURHOP-4097-EXTENDED-DIAGNOSTIC.md)
completed in 32.421651 s, with 27.918267 s SDK Exec and 0.434905 s Commit,
but uncontrolled cache leaves numeric latency INELIGIBLE. No later diagnostic
can promote that failed gate.

## The question and the arithmetic

For `N` one-byte writes, `E` final extents, `R` changed runs, `H` extent-tree
height and `Δ` actually changed custody edges, the writer's `N` successful
syscalls and observed `N` FUSE WRITE request/reply pairs are part of the
public workload. One SDK Exec and one Commit control request are not repeated
per write. The present write path additionally acquires `N` private payloads,
copies tree paths, charges/refunds page ownership and sends `N` checked inode
invalidations. Commit then walks the frozen final sequence and streams `E`
descriptors and `R` replacement runs through one SaveFile, rather than replaying
`N` root publications.

An abstract bounded-fanout path-copy tree can perform `O(N log E)` page-path
work, but this says little about the number of 4 KiB ledger RMWs and private
`openat`/`fstat`/`fallocate` calls. Four-hop sponsorship still sorts entire
copied edge lists and full-recharges on the fifth copy. The retained
per-512-write ledger reads rise near the height-two transition and then
nearly plateau: this does not prove an `O(N²)` runtime or remove the large
finite-range coefficient. Commit's ordered traversal and stream are
`O(E+R+S)` with `S` replacement bytes, plus `O(P)` private payload opens for
`P` Local payload files. A different representation could reduce `P` without
changing the required `Ω(R)` description of separated runs.

The diagnostic's existing FUSE timers sum to 12.083 s in `own_payload` and
15.650 s in `write_file` through WRITE 4,096. The first bucket includes
old-root/payload reclamation, and the second includes a second maintenance
call and the checked kernel notifier. Neither is a measured primitive cost.
A raw 2× Exec target requires about 13.96 s of savings; eliminating one
ordinary SDK round trip or halving the 0.435 s Commit cannot supply it.
Halving Commit alone saves about 0.217 s of the 32.422 s complete command.
With lifecycle unchanged, halving that complete command would require Exec
near 11.71 s, a much deeper change than a 2× Commit improvement. These are
opportunity bounds on one cache-ineligible run, not performance promises.

## First change: bounded operator attribution

Add ordinary, fixed-size operator telemetry on the real route, using the
existing diagnostic mode and sparse 512-WRITE snapshots. It must not add a
test-only product hook, unbounded event log, extra read of the input, warmer
cache, per-write status scan, or timer work outside the product boundary.
Counters and monotonic nanoseconds are cumulative; errors and missing
intervals remain explicit. Parent and child timers are labelled as inclusive
or exclusive and are never blindly summed across host/daemon clocks.

| Scope | Causal breakdown to record | Why |
| --- | --- | --- |
| FUSE/Workspace Exec | `own_payload`: metadata maintain, payload maintain, actual acquire; `write_file`: publication maintain, root/candidate work, checked notifier; successful and failed call counts | Separates old-root cleanup and payload-file cost from publication and kernel coherence. |
| Backing ownership | Ledger opens/identity checks, 4 KiB reads/writes, adjacent RMW runs versus distinct candidate ledger pages, new-page create/allocate/write, sponsor attempt/accepted/depth-four fallback, cleanup pages/refunds | Distinguishes avoidable revisits from required charged edges; 4 KiB counters alone are not syscall times or physical bytes. |
| Commit daemon | Frozen cursor visits, descriptor count/bytes, Local payload open and aligned-read calls/time, actual Bridge body frames/bytes | Separates traversal and thousands of tiny payload reopens from the one Service request. |
| Commit host | A timed `file_stream::read`/end-input child before C2 `begin_save`, parsed extents, spool record write calls/bytes/time; retain existing C1 and C2 children | Attributes the approximately 0.358 s uninstrumented SaveFile span without blaming C1's measured 0.015 s child. |

The first implementation should time coarse boundaries with one pair of
`Instant` reads per operation and expose source-wide cumulative totals in
existing operator status. If a coarse component dominates, add a second
prospectively named count instrument at that specific I/O boundary. Per-ledger
RMW nanosecond probes at hundreds of thousands of sites can perturb the run;
start with call counts and a timer around the enclosing candidate/cleanup
step. Keep instrumentation placement, overhead and counter scope in the
receipt. `metadata.rs` is near its 999-line ceiling; use focused owning
modules for any new telemetry implementation.

Prospectively name one `diagnostic4097cause` selection in the existing public
runner. Freeze one 4,097 one-byte `pwrite` writer, one process/fd, exact
Mount → Exec → Commit, 512-WRITE checkpoints, one worker, unchanged full
old/new-head oracle and separate verifier, and a **60 s diagnostic-only**
complete-command limit. The original gate keeps 25 s. Use a fresh independent
writable byte-copy clone of the closed verified master, locked release
binaries, explicit compilation/source/harness/image/workload seals and no
cache warming. If host server code changes, rebuild the linked SDK driver
and verifier for this source while reusing and independently validating the
closed master; a stale executable cannot claim the new compilation seal.
One attempt at the frozen source; retain every receipt, timeout, verifier and
cleanup result. This count diagnostic never becomes a speed or gate PASS.

## Reform choices after attribution

1. **If repeated ledger-page visits dominate Exec:** form a bounded
   candidate-scoped overlay of owner/ref deltas, then apply one authenticated
   update per distinct ledger page before root publication. The target is
   `O(U)` 4 KiB RMWs for `U` distinct pages rather than `O(G)` disjoint runs
   when `G ≫ U`. Reads of candidate pages must see pending deltas; partial
   writes need exact progress/quarantine; old G1/G2 roots, slot reuse and quota
   refunds need independent proofs. This is a substantive algorithm reform,
   not a larger `change_refs_run` loop.
2. **If tiny-file create/reopen dominates both phases:** design an authenticated
   small-payload pack or immutable inline-piece format. Target fewer per-byte
   `openat`/`fallocate`/4 KiB direct I/O operations while preserving distinct
   payload identity and independent old-root lifetime. Charge physical bytes
   and bounded resident assembly before acknowledging each WRITE; prove
   per-slot cleanup, partial failures and exact refund. This is the strongest
   cross-phase candidate, but it needs a versioned format and migration ruling.
3. **If checked invalidation dominates publication:** investigate an
   origin-specific coherence proof with concurrent readers, aliases, private
   faults/splice, size/mtime changes and failure replies on supported kernels.
   Until that proof exists, keep the pre-reply notifier. Fewer FUSE callbacks
   via caller batching or writeback cache changes the declared workload and
   acknowledgement semantics.
4. **If host pre-save spool dominates Commit:** replace four eight-byte writes
   with one 32-byte write per edit first. Preserve validation and spool bounds.
   Only pursue a new streaming/buffering protocol if that smaller source
   change and measured pre-save breakdown leave material headroom.
5. **If path copying remains dominant after these:** a versioned, indexed
   write journal/MVCC overlay can aim for amortized `O(1)` append with
   `O(log E)` observable reads, then one `O(E+R+S)` final construction. This
   would replace the current per-WRITE immutable root publication. It must
   define per-callback acceptance, bounded read visibility, frozen Commit
   snapshots, crash/unknown outcomes, quotas, retained old roots and exact
   cleanup. A bare append log with linear lookup or uncharged memory is not
   acceptable.

Do not choose a redesign from elapsed ratios alone. Select the path whose
measured cost is large enough to make the target arithmetically possible,
then freeze its format/algorithm spec before editing source. Every prototype
must retain one-process public writes, old/new-head and full-byte oracle,
old-root/G1/G2/abandoned-candidate custody proof, quota refunds, bounded
residency, clean close and the unchanged 25 s one-shot gate at its new source
identity. A general 2× latency claim additionally needs separately declared,
cache-qualified matched workloads beyond this extreme one-byte case.

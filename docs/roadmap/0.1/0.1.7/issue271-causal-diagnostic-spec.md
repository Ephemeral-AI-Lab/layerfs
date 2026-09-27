# Issue 271: one causal Exec and Commit diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This specification is committed before telemetry, runner changes, builds or
sampling. Its starting evidence is the [one extended four-hop diagnostic](../../../../core/docs/issues/271/FOURHOP-4097-EXTENDED-DIAGNOSTIC.md)
at source `61eed04a979341353054781a551dd973fcb04d63`, which retains the
distinct original 25 s 4,097 gate FAIL. The
[research](../../../../core/docs/issues/271/RESEARCH-OWNERSHIP-EXEC.md)
and [proposal](issue271-causal-telemetry-and-reform-proposal.md) explain the
source-derived cost model. This selection asks which *internal work* accounts
for Exec and Commit; it does not ask whether a new source is faster.

## Identity, route and limits

Add only `diagnostic4097cause` to the existing public separated-write runner,
scenario `issue271-separated-4097-cause-v1`. Its work is the same one closed,
verified 8,194-byte master, independent writable byte-copy clone, static
release writer, one process/fd and exactly 4,097 one-byte `pwrite`s at offsets
`2*i`. The public route is one SDK Mount → one generic Exec → one explicit
Commit, then status, unmount and sandbox deletion. Require exactly 4,097
observed FUSE WRITE callbacks, four upstream Service calls, one committed new
head, 4,097 changed runs, full old/new-head and byte oracle, and clean
product close. No internal driver write or per-write RPC is permitted.

Enable ordinary bounded operator telemetry plus the existing 512-accepted-
WRITE snapshots at 512, 1,024, ..., 4,096. The last WRITE's ledger/phase
counters remain unavailable unless a source-owned final observation is
available without inserting a public status call between Exec and Commit.
Keep the one construction worker, original 30 s product Exec deadline,
unchanged writer and verifier, no cache priming or buffer policy change.
The one **60 s complete-command timeout** applies only to this labelled
cause diagnostic, with a separate 9 s verifier. The original public gate
remains 25 s. Cache state is uncontrolled; every latency and memory peak
without qualified phase coverage is `INELIGIBLE`, and the diagnostic is
`admission_eligible=false` even if it finishes.

Source commit/tree, product/harness/compilation/dependency seals, release
binary hashes, image ID, source workload/spec hashes, clone method and cache
contract are pinned before the one attempt. Reuse the previously closed,
verified master without re-running Init or seed. Rebuild any SDK, verifier or
daemon executable whose transitive compilation inputs changed; the host SDK
links `layerfs-server`, so server telemetry cannot borrow the prior SDK
binary. Validate the master with the newly sealed verifier before cloning.
Use a fresh append-only output directory and retain every failure, raw log,
receipt, verifier, clone and cleanup outcome. Do not rerun the same source arm.

## Required additive observations

All times are cumulative monotonic nanoseconds in their own process. Count
and time failed as well as successful internal calls; retain observed
callback counts as the denominator. A parent bucket is inclusive of named
children. Keep a `residual` rather than attributing an unmeasured interval
to a guessed cause. Host and daemon clocks are never subtracted as if they
shared an epoch. Instrumentation must use fixed-size counters and sparse
snapshots, without an unbounded trace or a read of future payload data.

| Product scope | Required timer/count split | Attribution rule |
| --- | --- | --- |
| FUSE `own_payload` | Acquisition-maintenance ns/calls; actual `PayloadHost::acquire` ns/calls; metadata- and payload-maintenance child ns/calls | Existing `acquisition_ns` remains the inclusive parent. Metadata/payload maintenance are children of the applicable maintenance call. |
| FUSE `write_file` | Publication-maintenance ns/calls; post-maintenance mutation core ns/calls; checked notifier ns/calls | Existing `publication_ns` remains the inclusive parent. The notifier is inside it; FUSE reply is outside it. |
| Ownership/backing | Existing 4 KiB ledger reads/writes and page reads; ledger-file open/validate calls and cumulative ns; inclusive metadata-page create/allocate/write calls and cumulative ns; sponsor attempts, accepted and depth-four fallbacks | These are overlapping cost centers within the parent FUSE phase, not additional wall to add. Count local opens and I/O separately from SDK/Service request count. |
| Commit daemon | Final `E` extents, `R` changes, replacement `S` bytes, descriptor-source calls/bytes, Local payload reader opens and aligned reads with cumulative ns | Preserve the one SaveFile request and bounded stream. A source read is not a Service RPC. |
| Commit host | LFT1 child around pre-save `file_stream::read` and end-input; spool edit-record write calls/bytes and cumulative ns; existing begin/C1/finish children | This identifies whether the approximately 0.358 s prior SaveFile gap is transfer, parsing/spool or another unmeasured component. |

Export named versioned operator logs or additive status fields at the
checkpoint and Commit boundary, with units and counter provenance. The
benchmark parser must fail closed on missing, duplicated, nonmonotone or
mis-scoped counters; unavailable mandatory causes are `INCOMPLETE`, never
zero. A record may show an inclusive parent and exclusive children, but
must not sum overlapping source and wall clocks. Count actual Bridge frames
if a source-owning boundary exposes them; otherwise report frame count
`UNAVAILABLE` and distinguish source-derived byte/frame bounds from observed
transport. No instrumentation is allowed to pre-touch the Store, backing
files, payloads or master.

## Decision after this one row

Report per-512 deltas, exact ledger I/O, subphase nanoseconds, actual
request/reply counts, full oracle and cleanup beside the original gate FAIL.
If repeated ledger-page open/RMW work dominates, specify a bounded
candidate-scoped coalescing design with old-root/quota/failure proof. If tiny
file create/read dominates both phases, specify a versioned packed-payload
format. If notifier dominates, prove kernel visibility before considering
elision. If host pre-save spool dominates, test the four-to-one record write
change first. If no local change can remove the required ~14 s for a raw 2×
Exec target, pursue the indexed MVCC journal architecture under a separate
format and acknowledgement contract. No elapsed result from this diagnostic
alone establishes general 2× performance or qualifies a release speed gate.

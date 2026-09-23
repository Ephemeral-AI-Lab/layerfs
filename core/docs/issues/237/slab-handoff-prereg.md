# #237: bounded native import handoff, 10k preregistration

> Prospective Core-only diagnostic. This protocol precedes product edits and
> timed arms. It does not change the v0.1.6/Core head-to-head receipt or claim
> release admission.

## Question and arms

Does sending ordered, bounded producer slabs to the existing single C2 Save
consumer reduce the Core 10k public Init wall and handoff wait observed in the
[common-source comparison](v016-v017-common-source-results.md)? The old run
used 1,203 slabs; the current Core run received 34,562 individual Object/Done
events. Those child timings differ in scope, so the experiment measures one
matched **Core control and candidate** rather than subtracting the old timing
from the new one.

- **Control:** source commit `50e4f778d` native import topology: four C1
  producers, `sync_channel<Message>(8)`, one message for each finalized object
  and file completion. Add only the same aggregate diagnostic counters used in
  the candidate; keep C1, C2, C5 and SQLite behavior unchanged.
- **Candidate:** four existing C1 producers; one four-slot bounded channel of
  slabs of ordered `Object`/`Done` events. An ordinary slab holds at most 256
  KiB canonical payload, 512 objects and 512 completions. A canonical object
  over 256 KiB first flushes the worker's partial slab, then travels alone in
  an oversized singleton slab. The established C2 16-MiB canonical-object
  limit remains the hard per-object limit. Every event in a slab is consumed
  in order, with the existing `SaveHandoff::accept` called once per object.
  Each worker flushes its final partial slab before exit. The receiver
  processes each whole slab before checking that all files completed.

The handoff owns at most four queued and four producer-held slabs at a time.
Canonical payload ownership is therefore at most 8 × 256 KiB = 2 MiB for
ordinary slabs and at most 8 × 16 MiB = 128 MiB in the pessimistic all-singleton
case, before unchanged C1 construction scratch and C2 pending-wave ownership.
Each slab also holds at most 1,024 bounded event records; vector capacity and
file-completion metadata are reported separately from canonical payload. Count
live handoff-owned canonical bytes and their peak in both arms. An oversized
object is never rejected solely for exceeding the ordinary slab target.

Do not move packs out of SQLite, change SQLite page size (4,096 B), change the
128-KiB whole-file cutoff, change object format/IDs, change C2 admission, or
change the four Init workers. No source/cache warming from a previous arm.

## Fixed input and timed boundary

Use the same sealed seed-1 fixture manifest as the common-source comparison:
SHA-256 `c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`,
10,000 files in 100 data directories and 300,000,000 logical bytes including
the 100-MB anchor. Reuse the closed master for **untimed setup only**. Each arm
gets its own independent writable byte copy, fresh Store, fixed Stack/scope
operation identity, fresh transport identity and absent output directory. Use
the existing H3 `cold_diagnostic.py --case namespace-10000
--independent-source-copy --fixed-operation-identity` route. Full hash,
source-page invalidation and whole-input nonfaulting residency checks must
show zero resident payload pages immediately before each public call. Record
the check-to-timer gap, backend identity and failures. Metadata cache remains
unqualified; classify the rows as exploratory even if both payload checks
pass. No OS-specific purge.

Run **one** release-profile, performance-only public operation per arm,
control first and candidate second, without an in-timer verifier or a rerun
to choose a better number. The timer is the daemon native Init request through
its `StackCreated` acknowledgment; fixture copy, binary build, Store setup,
readback and cleanup wall stay separate. Preserve an incomplete or failed arm.
After both arms, run one separate full reopened readback for each retained Store
using the same fixture manifest. Compare exact root and content IDs across the
two Core arms, but do not use readback wall in the speed result.

Before the control arm, freeze its commit, product and harness seals, release
binary hash, output path and exact command here. Freeze the candidate identity
the same way **before its own arm**, after implementing the already-preregistered
mechanism. This sequencing keeps the control binary intact while the candidate
is built. Do not start either timed arm until the root task grants a host window.

## Identical diagnostics and decision

Add the same aggregate source counters to each arm before timing: source read
calls/bytes/wall, worker job count and construction wall sum, successful send
calls, object and completion events, canonical payload bytes, send-block wall
from a full channel, receiver calls/wait wall, C2 accept calls/wall, and live
handoff payload peak. Report every scope and nesting; worker sums overlap.
Retain the existing C2 `SaveOutcome` object/pack/SQL/COMMIT counters and
Service/daemon telemetry. Report public wall/decimal MB/s, process CPU,
sampled RSS and coverage, Store/History page size and apparent/allocated
bytes, pack capacity/used/slack, root/ID/readback status, and nonpassing lines.
The event-count target is at most 3,456 slab receives (a 90% reduction from
34,562); speed is the observed one-pair raw delta, not a median or a claim
about steady-state variance. A candidate failure, different root/IDs, failed
readback, nonzero source payload residency, or unqualified telemetry remains
visible and cannot become an admission PASS. A faster raw candidate alone is
insufficient for a fully cold or release claim.

## Frozen control identity before timing

The control implementation is clean commit `47c3564a925c37d6ced7494d400b76336f4e1fec`
(same handoff algorithm as `50e4f778d`, with aggregate research counters).
Its product seal is
`e3ab6745217c40e3a38d859c57420f1d1c3ad94a6d70ef44a3e0b8ee13c016de`,
harness seal
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
and release Service binary SHA-256
`e1f4032c36d23f13a39f29e515c661378cc560751969b27577e527cdb5b215de`.
The H3 driver SHA-256 is
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`.
The fixture master is reused by an untimed symlink into this isolated
worktree's `benchmark-results/fs-bench-pro/prepared/`; the driver creates a
new independent writable byte copy before the public call. Its manifest hash
was checked as the fixed value above. The frozen, absent output path is
`benchmark-results/fs-bench-pro/issue237-slab-control-01`. Command:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --case namespace-10000 --independent-source-copy \
  --fixed-operation-identity \
  --out benchmark-results/fs-bench-pro/issue237-slab-control-01
```

## Append-only correction after the control arm, before candidate source

The frozen control source and product/harness seals were correct, but the
binary hash quoted above was from a manual single-package build. The H3
runner rebuilt with its declared multi-target command and actually timed
archived Service SHA-256
`f41a4486c974e0323f178ee3aa3e4800daadd1c5ab754dd953901cdf59525db4`.
The control's exact-binary preregistration therefore failed. Keep its one
public sample and all raw evidence as an exploratory/incomplete row; do not
rerun or retrospectively replace the original quote. Build the candidate
with the runner's exact command **before** freezing its binary hash.

The earlier eight-slab payload bound omitted the slab currently held by the
receiver while four queue slots refill and four senders block. The actual
maximum handoff ownership is **nine** slabs: four queued, four producer-held
and one receiver-held. Consequently the ordinary canonical-payload bound is
9 × 256 KiB = 2.25 MiB and the pessimistic all-oversized bound is
9 × 16 MiB = 144 MiB. Four C1 objects being constructed and C2's pending
wave remain additional, unchanged ownership. The candidate must measure its
live handoff payload peak and enforce per-slab byte/event limits; vector
capacity/event-record overhead is reported separately. This correction is
prospective for the candidate and does not relabel the control.

### Further pre-timing accounting clarification

While a producer flushes a full partial slab, it can already hold the next
finalized object passed to `FinalizedConsumer::accept`. Four such callback
objects add at most 4 × 16 MiB = 64 MiB under the existing C1/C2 canonical
limit. The conservative nine-slab-plus-four-callback bound is **208 MiB** of
canonical payload before unchanged C1 scratch and C2 pending-wave ownership.
The candidate's live-payload peak counter includes these callback objects at
the start of `accept`. `max_slab_event_slots` is the maximum **per slab** vector
capacity, not an aggregate live event-memory peak; the hard event-record bound
is 9 × 1,024 × `size_of::<Message>()`, plus four callback records. On an early
abort, the diagnostic `live_payload_bytes` is `UNAVAILABLE_AFTER_ABORT`
because discarded queued events do not all pass the receiver's accounting
point. This does not alter the failure result or cleanup route.

## Frozen candidate identity before timing

The candidate implementation is clean product commit
`3293255879dd418e877d2450b17aa3970dc3f6eb`. Its product seal is
`520c6a40caee0c1e8cdfd26b22dcec8c3c3a7a52a187e25f5fe708ea0de9b102`,
harness seal
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
Cargo.lock SHA-256
`cedd3b27d78d3dd9d5db1aa4d6d5a8036e308fbce09f5d8db645174d5e2296b9`,
and H3 driver SHA-256 remains
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`.
The **exact H3 runner multi-target build** passed before timing and archived
Service binary SHA-256
`0db02334d1220842c5514a91705a5bdc8f267f75a9719456d937404d7c1ceaa6`
and daemon binary SHA-256
`fd5b6ed0b3eb815f7a6b6f8493bf4186ce22f59cd2f41e11cfec4eadff68900e`.
Its build-only record is retained under
`benchmark-results/fs-bench-pro/issue237-slab-candidate-prebuild/build.json`;
it invoked no public operation. The frozen, absent timed output path is
`benchmark-results/fs-bench-pro/issue237-slab-candidate-01`. Command:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --case namespace-10000 --independent-source-copy \
  --fixed-operation-identity \
  --out benchmark-results/fs-bench-pro/issue237-slab-candidate-01
```

The control yielded exactly one 10k public sample; its status was
`INCOMPLETE` from daemon telemetry loss. It is not replaced or repeated.
Separate full readbacks remain pending at this freeze point.

## Prospective count-only follow-up after both public arms

The two public arms did **not** emit `SaveOutcome` counters, so their exact
C2 transaction/SQL counts are `NOT_MEASURED`. One separate count-driven
diagnostic may add temporary once-per-Save reporting to the frozen slab
candidate: file Save in `operation/import_native.rs` and prerequisite/tree
Saves in `operation/history_bootstrap.rs`. It must print each successful
`SaveOutcome` exactly once: inserted/reused objects, object INSERT statements,
presence queries, pack creates/appends/submitted bytes, COMMIT count and
COMMIT/SQL nanoseconds. No algorithm, policy, pack format, SQLite location,
page size, cutoff, worker count or public route changes. Archive its exact
source diff and restore the reporting after the diagnostic. This produces
**diagnostic counts on a new operation**, not counts for either timed arm and
not a replacement speed sample.

Use the same sealed seed-1 master, independent writable source copy, fixed
Stack/scope identity, new Store and H3 zero-resident payload preflight/recheck.
The fresh absent output will be
`benchmark-results/fs-bench-pro/issue237-slab-countdiag-01`; invoke the same
H3 perf-only command as above with that output. Run it once, retain failure
and telemetry status, and do not compare its public latency to either arm.
Source commit, product/harness seals, exact runner binary hashes and reporting
diff hash must be appended before the diagnostic call. A count logged on the
diagnostic is explicitly labelled separate from the two original samples.

### Frozen count diagnostic before its sole call

Temporary reporting is product commit `018d16b37` atop the frozen slab
candidate; its archived gzip diff is
[`evidence/slab-handoff/count-diagnostic.diff.gz`](evidence/slab-handoff/count-diagnostic.diff.gz),
SHA-256 `a5ccadc99cddd0347fff073a6287b5f931598131791c7bb8d1ef508cc8cfc028`.
The exact H3 runner multi-target build passed in 2.604 s and archived Service
binary SHA-256
`147054975abd324345e6bce697faf072e96e7a8f52359d1a990e3add569aa2a0`.
Its product seal is
`3f7690251005c595fd99724e1ac65063e32e7a1f1f29717bba433014246ad080`,
harness seal
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
and H3 driver SHA-256
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`.
The build-only record is retained in
`benchmark-results/fs-bench-pro/issue237-slab-countdiag-prebuild/build.json`.
The fresh, absent output is
`benchmark-results/fs-bench-pro/issue237-slab-countdiag-01`:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --case namespace-10000 --independent-source-copy \
  --fixed-operation-identity \
  --out benchmark-results/fs-bench-pro/issue237-slab-countdiag-01
```

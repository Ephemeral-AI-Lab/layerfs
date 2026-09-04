# Architecture reassessment: low-CPU, bounded-memory Commit

2026-09-04. Read-only review by the primary agent and three independent subagents.
No implementation files, running tasks, containers or measurements were changed.
This is a recommendation with quantitative hypotheses, not a new performance result.

## Decision

Keep the current generic affected-page tree merger, authenticated reference
ledger and checked identity handoff. The most useful architectural change is to
**carry one finalized candidate description forward through complete preflight
and admission**, removing duplicate indexes, random metadata probes and repeated
ordering passes. Keep one bounded content pool for a frozen Commit generation.

Do not immediately replace the tree format, add more SQLite writers, or stream
all construction output directly into persistent CAS. The first two target small
or unproven bottlenecks; the third changes preflight failure behavior unless new
proof establishes equivalence. A more ambitious final-subgraph pipeline is worth
researching only after these lower-risk changes have measured results.

For the first combined package, use **7.5–10 seconds complete create Commit on
two CPUs** as a planning band, with a **9–10 CPU-second work target**. An eight-CPU
**6.5–9-second** band is only a conditional stage-budget screen, not a forecast.
A deeper architecture might warrant **5–8 seconds on two CPUs / 4–7 seconds on
eight CPUs**, but only after substantial work reduction and safe restructuring
are demonstrated. **2.766 seconds is not supported by current Commit evidence.**
The differences between these bands and their assumptions are detailed below.

## Evidence identities and full roadmap review

Implementation source reviewed: HEAD
`8165519b337bc5eaa3e28d9ae0842fc9d2f5272e` in the independently running
`layerfs-workspace-commit-engine` worktree, including evolving working files.
Latest retained measurements discussed here belong to candidate
`939f05feb5a58bd23720f329767dcb05585d1e81`, not that later working source.
Source analysis is not a new qualification of the later code.

The team read every document in the v0.1.1 roadmap directory, in full:

| Document | Scope and lesson |
| --- | --- |
| [README](../../docs/roadmap/0.1/0.1.1/README.md) — 342 lines | Live chronology, source seals, target versus measured CPU/cache limits, nonterminal development evidence. |
| [Architecture shift](../../docs/roadmap/0.1/0.1.1/architecture_shift.md) — 1,366 lines | Point-update and spill cliffs; final-only construction; owned slabs; useful overlap; explicit eligibility and ownership limits. |
| [Baseline checkpoint](../../docs/roadmap/0.1/0.1.1/baseline-2026-09-02.md) — 477 lines | Append-only evolution and superseded conclusions; READDIRPLUS removed a supposedly unavoidable per-file round trip. |
| [Initial handoff](../../docs/roadmap/0.1/0.1.1/handoff-prompt.md) — 365 lines | Historical baseline-first methodology and distinct phase/custody boundaries; not current execution authorization. |
| [Namespace optimization spec](../../docs/roadmap/0.1/0.1.1/namespace-optimization-spec.md) — 1,029 lines | Retained/rejected experiments, bounded ownership, CPU limits, single SQLite owner and initialization-only failure cleanup. |
| [Namespace-v2 handoff](../../docs/roadmap/0.1/0.1.1/namespace-v2-handoff-prompt.md) — 609 lines | Combined pipeline and exact measurements; no unsupported cache/worker/SQL experiments. |
| [Release blog draft](../../docs/roadmap/0.1/0.1.1/release-blog-draft.md) — 737 lines | Explanation of the measured progression and remaining ceilings; narrative/draft claims are not additional benchmark evidence. |

Historical commands and handoff instructions were treated as evidence, not run.
The primary agent read the baseline/initial handoff, and the subagents covered
the remaining documents alongside their source reviews. Their earlier proposals
were revised after this full reading.

## What v0.1.1 actually teaches

1. **Eliminate whole repeated stages, not merely calls inside them.** Final-state
   construction and large transactions already existed in the 4.502-second
   pre-direct result. The later pipeline eliminated about 647 MB of object-segment
   writes and 647 MB of rereads—approximately **1.294 GB of intermediate traffic**—
   and the serial boundary between production and admission.
2. **Reduce generated work and then overlap it.** Exact metadata interning cut
   canonical puts from about 1.132 million to 439,000 (~61%) and pending duplicate
   comparisons from 708,845 to 15,845 (~98%). It was not a standalone wall-time win
   while the pipeline still serialized preparation and admission.
3. **A zero-buffer stream is not automatically efficient.** The rendezvous trial
   reached 3.806 seconds with eight producers; ten gained only about 1.2% while
   adding roughly one system CPU-second. The retained design used owned
   256-KiB/512-object slabs, four queue slots and one SQLite owner.
4. **Retain small bounded spills unless they matter.** The approximately 6.464-MB
   compact inode-pair stream was intentionally retained. Removing small metadata
   streams is lower priority than eliminating full canonical-payload passes.
5. **Low memory must not cause quadratic CPU.** The old 8-MiB index cliff caused
   linear spool searches inside repeated lookups. A later ~1.4-million-ID spill
   fallback also remained. Correctness under a low-memory test does not establish
   bounded CPU scaling. The present `SpillableObjectSet` still deserves this audit;
   it is not proven to trigger in the current 412k-object case.
6. **Do not copy eligibility or cleanup exceptions.** The direct initializer
   required an empty Store, favorable directory topology and no hard links.
   Handled failure could clear every admitted object. Generic Workspace Commit
   must preserve historical objects and its own preflight/early-admission rules.
7. **Do not turn one profile into a universal limit.** The early baseline called
   further compatible FUSE acceleration unavailable; the later audit removed
   redundant lookup traffic with READDIRPLUS. Conversely, removing bounded
   verifier concurrency caused a measured regression. Follow measured operation
   redundancy, not generic rules about “fewer threads” or “FUSE is inherently slow.”

The full review rejects repeating initialization payload-read/collision caches,
giant transactions, extra SQLite owners, smaller SQLite caches solely to reduce
reported memory, and more producers solely to reduce wall time. The old direct
path was write-bound; current candidate-index work must be measured separately
rather than assuming it is the same bottleneck.

## Current measured opportunity

Candidate 6, 100k create, seed 1, two CPUs / 2 GiB:

| Complete Commit stage | Wall |
| --- | ---: |
| Content construction and coordinator | 3.555611 s |
| Final closure | 0.860924 s |
| Membership/preflight | 2.192793 s |
| SQLite admission | 2.412597 s |
| Installation and required cleanup | 1.277146 s |
| Planning, namespace, publication and remainder | 0.400478 s |
| Total | **10.699549 s** |

CPU: **11.41 seconds**, about 1.07 CPUs busy on average. The two producers
account for about 2.302 CPU-seconds; increasing their count does not parallelize
all remaining work. There were 1,002 pool lifetimes and 2,004 worker starts/joins.
Admission already used **139 transactions**, a maximum of 8,191 objects and
less than 4 MiB: do not claim the old 3,234→~131 transaction improvement again.
Admission moved/read **565,781,158 canonical payload bytes**, with zero reported
admission payload-copy bytes. There is still a large private payload reread, but
its measured time is not all irreducible disk I/O.

Process sampled peak was **335,454,208 B**; cgroup lifetime peak **1,638,821,888 B**
includes page cache and other container ownership. These are different scopes.
The process was already resident at approximately 153 MB before Commit. Do not
promise the initializer's ~98-MiB phase HWM as a Workspace peak without accounting
for that live state.

Generic delete on the same candidate was **0.379839 s / 0.38 CPU-s**; base inode
reads fell from 100,634 to one. Keep that algorithm. A two-CPU planning band of
roughly **0.3–0.5 s** for the new pipeline is a nonregression expectation, not
another demonstrated speedup or a three-seed median. The tree/ledger is no longer
the primary multi-second create bottleneck. Larger matched verification remains
required by the implementation spec.

## Recommended architecture and algorithmic changes

### A. Frozen-generation scheduling and bounded sequential installation

One coordinator owns the frozen generation, final references, record selection,
tree updates and handoff. Workers receive only bounded in-flight `FrozenFile`
inputs and their memory shares. Reuse the pool across files/directories; do not
prefreeze every file or give every producer a separate full allowance.

The desired normal lifecycle is O(worker count) thread starts per Commit rather
than 1,002 directory-associated lifetimes. Real memory pressure may require a
bounded drain; only an explicit memory-budget denial may trigger retry. Disk,
Store, integrity and other limit errors must retain their original behavior.
Count partial-attempt work and private orphan objects, even if later pruned.

Installation currently reads one 112-byte handoff record from a raw File at a
time (`lifecycle.rs:888`). A charged 64-KiB reader would reduce approximately
100k small read requests to about **171–172 buffer fills** for 11.2 MB, plus
short-read effects. Reuse an available buffer allowance; preserve seek-to-zero
retry and published-outcome ownership. This is a precise operation reduction,
not a prediction that the full 1.277-second cleanup/install stage disappears.

### B. One final-object descriptor stream, preserving complete preflight

Current architecture: private objects → exact closure → reachable IDs → indexed
length lookup → SQLite membership → missing-ID set → physical-order rescan →
missing order → payload lookup/read → admission.

Proposed architecture:

```
frozen generic changes → private canonical objects
  → exact final closure emits unique (ID, length, location) descriptors
  → complete sequential membership/collision/accounting preflight
  → checked missing-descriptor stream in usable physical order
  → bounded payload-on-demand admission by the sole SQLite owner
  → conditional publication → checked identity installation and cleanup
```

Carry length/location when the object is already resolved, rather than discarding
and re-querying them. Replace missing-set/order machinery; do not add another full
in-memory manifest. New-heavy and reused-heavy candidates use the same path.
Skip reused payload I/O where the existing integrity contract permits, while
retaining every authentication/equality check at its proper trust boundary.

For C final objects, remove roughly C repeated index-length probes, the separate
missing-set insertion/probe work and the full ordering-selection pass. A bounded
merge/sort may still be required to preserve physical payload order; account for
it explicitly. Streaming classification is O(C) record processing plus required
SQLite/index checks, not a claim of constant-time Store membership. Sorting
fixed descriptors is O(C log C) comparison work with bounded runs; avoid an
O(C²) linear-spill lookup fallback. Keep the current generic tree complexity
proportional to affected pages/keys and necessary balancing neighbors, not all
surviving entries.

A 48-byte descriptor for ~412k objects is approximately **19.8 MB on private
storage**, not an in-memory vector. A 128-record window is about **6 KiB** before
headers. Replace existing index/order ownership and reserve the period where old
and new streams coexist. Preserve original production order or checked explicit
offsets; closure graph order must not silently turn sequential admission into
random payload reads. Do not copy/sort all canonical payload bytes.

**Preserve the no-write-before-preflight boundary.** A naive insert-or-verify
stream can commit earlier objects before discovering a late collision, length,
capacity or accounting failure that currently occurs before any insertion. The
existing allowance for late admission residues does not make that equivalent.
The admission reviewer explicitly revised the initial insert-first proposal after
this audit and the full v0.1.1 failure-cleanup reading.

### C. Deeper final-subgraph architecture: conditional research

The next possible step is retaining final dependency descriptors during encoding
so closure does not reread/decode payloads merely to recover their edges, and
transferring proven final subgraphs through a bounded owner pipeline. This may
remove more payload staging and serial boundaries, following v0.1.1's strongest
lesson. But not every generated object is final: rope mutations, balancing,
memory-pressure retries and reconciliation can leave private intermediate output.
A frozen Workspace alone is not a reachability certificate.

Before persistent construction/admission overlap, prove final ownership, dedup,
root binding, failed-attempt rollback and **whole-candidate preflight equivalence**.
If equivalence cannot be established, retain the preflight barrier; changing the
failure footprint would require a separate contract decision. Do not clear old
CAS rows or silently accumulate superseded data. This is not currently a safe
shortcut to claim the initializer's schedule.

Packed physical CAS storage might reduce SQLite per-object overhead, but it
changes storage/recovery machinery and is outside this Phase 1 recommendation.
The evidence does not justify it before removing redundant candidate processing.
Compact mutable inode slots and shared live staging likewise belong in Phase 2.

## Quantitative expectations: scenarios, not measurements

The three reviewers gave overlapping broad bands. The primary recommendation uses
the conservative resource-audit stage model for package A+B. Values below are
hypotheses to revise after the first experiment, not confidence intervals.

| Stage | Observed 2-CPU | A+B 2-CPU budget | A+B 8-CPU conditional budget |
| --- | ---: | ---: | ---: |
| Construction/coordinator | 3.556 s | 2.5–3.3 s | 1.4–2.4 s |
| Final closure | 0.861 s | 0.7–0.9 s | 0.7–0.9 s |
| Complete preflight | 2.193 s | 0.7–1.4 s | 0.7–1.4 s |
| Admission | 2.413 s | 2.1–2.6 s | 2.1–2.6 s |
| Install/cleanup | 1.277 s | 1.2–1.4 s | 1.2–1.4 s |
| Other | 0.400 s | 0.3–0.5 s | 0.3–0.5 s |
| Total scenario | **10.700 s measured** | **7.5–10.1 s** | **6.4–9.2 s** |

The main assumptions are that pool reuse reduces setup/idle work and descriptors
reduce repeated metadata/index work. No large admission or cleanup speedup is
assumed; those budgets remain near observed time. A regression is possible if
additional descriptor sorting or payload seeking outweighs the removed work.

Thus **7.5–10 s on two CPUs** is a useful initial expectation: approximately
**0.7–3.2 seconds / 7–30% less wall time** than 10.70 s. Target **9–10 CPU-seconds**,
about **12–21% less CPU work**. Do not call a wall-only win with substantially
higher CPU a successful low-CPU redesign without explaining the tradeoff.

The eight-CPU band is a conditional screen only: more CPUs help actual parallel
construction, not all preflight/SQLite/cleanup work. No matched eight-CPU result
supports a forecast yet. A successful deeper package C could justify testing
**5–8 s on two CPUs / 4–7 s on eight CPUs**, but those are lower-confidence design
budgets contingent on proof and measured work reduction. They are not an upgrade
to the current performance promise.

Memory acceptance: keep existing category/aggregate limits and aim for **no
increase from the current matched-source whole-operation peak**. Retain one
shared 8-MiB final-delta allowance, the existing file-construction envelope,
candidate/index bounds and documented admission ownership envelope. Do not
multiply any of them by worker count or shrink the measured useful SQLite cache
arbitrarily. A specific RSS reduction is not defensible before measuring replaced
index ownership and spill transitions. The algorithm targets bounded buffer RAM
and sequential private metadata storage, not a new namespace-sized RAM cache.

## Comparison with the 2.766-second initializer

The retained [raw historical result](evidence/historical-100k-result.json) gives:

- Initialization wall **2.766279583 s**.
- Initialization CPU **12.968067459 s**, from user 5.249351959 + system 7.718715500.
- Eight producers and one coordinator; average busy CPUs approximately **4.69**.
- Direct pipeline **2.611808 s**; 500 decimal MB from prepared native files.

The roadmap's **14.07 CPU-s is a gate**, not this sample's CPU total. The selected
2.766-s result is same-seal development evidence; README also records a newer
3.019-s initialization attempt under another seal and an unrelated small-Create
failure preventing terminal campaign PASS. Do not silently replace or combine
those identities. The historical millisecond localized Commit was a ten-byte
overwrite, not a bulk-create/delete Commit result.

Current Commit 10.6995 / historical init 2.7663 = **3.87× wall**, but CPU 11.41 versus
12.968 is a different-workload comparison and cannot establish efficiency parity.
Current workload has 500 MiB plus witness and a dominant `bulk` top-level task;
old initialization had a favorable different distribution. Keep names/layout
unchanged in the matched comparator; eight CPUs alone will not reproduce eight
busy initializer producers with only one substantial top-level task.

Necessary arithmetic, not hardware lower bounds for a redesigned algorithm:

- Current unchanged 11.41 CPU-s needs at least **5.705 s on two CPUs**.
- A 3-s two-CPU Commit needs at most 6 CPU-s; matching 2.766 s needs at most 5.5326
  CPU-s, **over 51% less CPU work** before allowing for idle or serialized work.
- Historical 12.968 CPU-s would itself need at least **6.484 s on two CPUs**.
- Removing current closure+membership entirely, with everything else unchanged,
  leaves **7.646 s**. Required checks are not actually free.
- Perfect overlap of today's content and admission alone saves at most 2.413 s,
  leaving **8.287 s**, and first requires a safe reorganization of the current
  barrier. Even pretending closure and membership then vanish gives 5.233 s:
  an unrealistically generous zero-replacement-cost envelope, not a prediction.

The low-risk plan therefore does **not** justify predicting 2.7 s. A 4–7-s
higher-risk eight-CPU scenario would still be approximately 1.45–2.53× historical
init wall. Reaching 2.7–3 s needs further demonstrated work reduction and a safe
pipeline schedule, not a claim that all Commit-specific overhead is unavoidable.
The correct goal is initialization-like shared-work throughput under matched
conditions, with every material residual explained.

## Next experiments and acceptance

1. Complete current pool reuse and the small buffered-handoff change; measure
   pool lifetimes, actual in-flight ownership, underlying handoff reads, CPU and
   install/cleanup separately. Reuse the postpublication retry/handle test once.
2. Prototype final descriptors on one new-heavy and one reused-content/mixed
   small candidate. Predict fewer private index probes, no missing-set/order
   rebuild and no extra payload copies. Force spill and late preflight failure:
   require **zero newly admitted CAS rows** at that failure, unchanged root and
   retryable Workspace. Preserve ordinary late-admission failure behavior too.
3. Run one source-bound 100k sample only after the mechanism and relevant correctness
   checks pass. Compare wall, CPU, bytes and peaks. Test the spill boundary's CPU
   scaling so a low-memory pass does not conceal quadratic work.
4. Obtain matched two-CPU and separately labeled eight-CPU initialization/Commit
   comparisons, exact files/metadata/witness, configured limits and recorded
   actual occupancy. Preparation and independent canonical/FUSE proof stay outside
   performance. Keep the existing three-seed completion gates for stable source.
5. Consider package C only if residual payload/closure work is measured as dominant
   and a mixed/failed-attempt/reconciliation proof establishes final reachability
   and unchanged preflight failure semantics. Do not start a broad Store redesign
   simply because the historical initializer is faster.

No proposal was sent to or installed into the independently running implementation
task during this review. These recommendations are available for the user's
implementation decision; they do not supersede current source-bound results.

## Follow-up: preflight is an architectural choice, not an immutable-root theorem

Further independent admission/resource review found a more ambitious route worth
specifying: an operation-owned admission epoch under the existing shared Store
ticket gate and SQLite exclusive connection. Stream bounded final-object slabs,
journal exactly the IDs whose INSERT created new rows, and conditionally publish
only after all required checks. On handled prepublication failure, rollback could
remove only this operation's never-published inserts, never any preexisting object.
This would permit construction/admission overlap without relying on an empty Store.

It is not implemented or implicitly approved. It restores logical row inventory,
not necessarily Store file length, freelist or physical-write observations. Journal
append must precede committing each insert batch; append failure rolls back that
SQL transaction. Rollback failure needs retained ownership and must block later
publication until recovery. Uncertain publication must be resolved before undo;
after publication, undo authority is permanently disabled. Crash durability is
not added by an in-memory cleanup owner. Exact duplicate accounting and final-only
emission across pruning/retries/reconciliation remain mandatory. An owned-ID journal
is about 13.2 MB of private disk for 412k new IDs, plus framing, and uses bounded
buffers rather than a RAM set. Independent connections and all publication paths
must be verified to obey the gate/exclusive-ownership assumption.

Normal filesystem deletion would still never delete historical CAS chunks.
Physically removing exclusively owned unpublished inserts is a distinct proposed
rollback operation; the user's restriction and existing failure observability
must be explicitly reconciled in the design before implementation. The current
private-file cleanup queue does not already provide this behavior.

This is a genuine alternative to the conservative descriptor-only plan, and it
is much closer to the v0.1.1 direct-pipeline improvement. It does not prove 2.766
seconds: current admission 2.413 s plus postpublication install/cleanup 1.277 s still
sum to 3.690 s even if all construction is hidden. Those costs are changeable, not
fundamental lower bounds. Matching the initializer would require reducing the
writer path and final tail as well as removing staging and serial preflight.
The earlier planning ranges must not be read as proof that a more complete
redesign cannot approach initialization on matched resources.

First proof: nonempty Store with duplicates and retained roots; journal-append
failure; late collision/head failure; rollback failure and gated retry;
publication success followed by cleanup failure; exact old-row retention and
correct new-row inventory. Only after the failure footprint is reviewed should
a matched larger-CPU performance experiment test removal of full-payload staging.
No such experiment or semantic change was performed in this review.

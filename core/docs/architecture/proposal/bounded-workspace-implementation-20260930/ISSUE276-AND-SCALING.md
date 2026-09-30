# Latest #276 requirements and quadratic-work exclusion

> **Status: Research; informative and not a product contract.**
> Source baseline `7edddbdb8e8512627aed0ed42533ef099d802384`, 2026-09-30.
> The owner explicitly requests the latest issue comments and elimination of
> quadratic scaling in the replacement architecture. No implementation or
> measured improvement is claimed by this document.

Read [README](README.md), [Workspace](WORKSPACE.md), [Server](SERVER.md),
[concurrency](CONCURRENCY.md), [acceptance](ACCEPTANCE.md) and
[scenarios](../../../../../scenarios.md). The
[source capture](ISSUE276-LATEST-COMMENTS.json) retains the latest eight of the16
comments available at capture time, including their IDs/timestamps/URLs. Comments
are source observations and prior rulings, not authority to resume old benchmark
or implementation instructions. The current owner request is architecture/spec.

## 1. Latest-comment traceability

| Source requirement | Current evidence class | Replacement architecture response | Future completion criterion |
| --- | --- | --- | --- |
| [Whole-frontier/local-reconcile memory](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5902583820) | Confirmed current mechanism; old10240 outcome is historical and now owner-deferred | Paged catalogs/cursors/results; READY resources; current-G2 install without a population patch | Independently vary dirty files, final extents/file, one directory width and pins; actual resident and physical owner proof |
| [Deep270 repeated validation](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828) | Source-derived quadratic chain work; timer covers all validation | One exact sealed-graph proof with shared colors; target v2 verified parent paths | Count distinct visits, duplicate completed walks, ancestor/index pages and negative final-batch cycles |
| Same comment's271 inode reservations | Confirmed retained trace/current per-create mechanism | Generic monotone ranges plus C5 catalog admission independent of C2 bulk Save | Verify consumed/unused/unknown serial custody and actual reservation-call count, including two active Saves |
| [Dirty exit7/unmount/delete](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903461365), [owner deferral](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903530127) | Unsupported cleanup assumption; deliberate dirty guard | Separate semantic detach/clean-close/explicit discard/forced runtime delete outcomes | Exit7 writes survive; no implicit Commit/rollback; final actor/mount/owner release observed |
| [Optimization-lane scope](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905580416) | Prior execution assignment; superseded here by research-only scope | Compound design preserves generic route, safety/worker/resource/storage rules | Focused future source-bound proofs; no broad Family2 or seven-family sweep |
| [Completed-descendant candidate](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905762244) | Unmerged changed-source count diagnostic, not main or qualified latency | Reuse the proof algorithm, replace its operation-sized state with bounded/external authority | Exact roots/final-parent/alias/refusal plus simultaneous color/stack/row capacities |
| [Latest memory/correctness priority](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905916098) | Current source risk inventory and ordering | Full live+Commit+Server+install/resource path designed before #249 enablement | Every population and scope named; partial descriptor removal cannot satisfy whole-operation bound |
| Same latest comment:128 handles/32 views | Deliberate owner/profile limits, not universally current defects | Byte-admitted capabilities, paged cookie/reference authority, exact selected bytes | Supported profile review, stale/forged refusal, final-pin refunds; selected bytes distinct from lease count |
| Same latest comment:statfs/interruption/death | Current zero-statfs and provider/runtime gaps; no new reproducer here | Quota projection, cancellable request ownership and outside-daemon mount custody | Ordinary kernel route, actor permissions, cancel linearization, exact dead-mount recovery |
| Same latest comment:16/32/128 KiB pinned read | Current API/frame mismatch plus historical Io; scope labels preserved | Typed bounded ResultData and exact total/terminal framing | Full requested bytes and boundary/oversize/partial/Unknown tests, not extrapolation from16 KiB |
| Same latest comment:#283 | Measurement capability limited; not proven product defect | Descriptor-owned phase observer and separate heap/cache/child domains | Correct same-FD reset proof and symmetric cache eligibility before numeric admission |
| Same latest comment:#235/#174 | Historical report completeness/attribution and residual audit | Owned bounded telemetry completion and scoped provider work; current-source audit | Missing/clipped/dropped reports remain INCOMPLETE; no stale issue text promoted to current bug |

The partial Core83de candidate removes per-file extent/descriptor materialization.
That is reference evidence only. The target also removes full dirty/name/result
state, live affected-range plans, C1 graph/drafts, post-publication reconciliation,
retirement registries and old-generation expansion. No blanket donor merge.

## 2. What is actually quadratic and what is not

The chain workload rebinding D directories causes fresh overlapping descendant
walks in the baseline cycle checker. Its source work is:

```text
baseline chain work = D + (D-1) + ... + 1 = D*(D+1)/2
D=270: 36,585 directory visits

sealed-graph proof: each needed directory reaches completed state once
target v2: verify changed parent paths and distinct effective ancestors
```

The unmerged candidate counted fresh270 directory row lookups36,585→270 and
inode demands37,398→814; fresh16 lookups136→16. Inherited/restated270 baseline
refused at its unchanged work ceiling; the candidate completed with1,080
lookups. These are retained diagnostics from different source identities. They
do not give a passing inherited pair, whole-process memory proof, universal
linear law or a measured speedup. The reported483,269,583 ns baseline validation
interval includes more than cycles. Do not divide that timer by the count ratio.

The10240 reconciliation failure is a **linear population admission** problem,
not itself proof of quadratic time. Its historical prepared-key+patch charges
were8,489,658 B before ordinary state, versus an8,388,608 B Budget. Current
post-credit behavior remains unknown/owner-deferred. Raw WRITE count also differs
from final surviving fragmentation. Repeated overwrite can leave one interval;
disjoint edits can leave many. Every claimed complexity axis must name its input.

## 3. Work laws for the complete target

Let n be accepted writes, E_i the intervals actually affected by write i, H the
declared index height, delta_j the current-generation record/data work for
Commit j, G the graph visited for initial/untrusted certification, K changed
namespace edges, D distinct effective ancestors, U actual owner cleanup work,
L genuinely differently keyed records, Q active commands and N lifetime commands.

| Mechanism | Forbidden repeated-work pattern | Selected target law and remaining cost |
| --- | --- | --- |
| Tiny append/overwrite | Walk/rebuild all previous intervals for each write: sum1..n | Per-file boundary paths/selected hot frontier, O(nH) path work plus actual affected data and custody |
| Wide overwrite/shrink | Enumerate/store the entire affected list and rebuild unaffected file | Split/join carries removed subtree roots; bounded boundary work plus actual eventual cleanup U |
| Cursor lowering | Restart at file/tree beginning for each next record | Monotone leaf/path position; one visit per required record/page plus declared fixed metadata passes |
| Successive Commits | Descend every predecessor extent on each small later change | First-touch opaque complete-parent span; current-generation deltas, O(sum delta_j) plus required tree/cleanup work |
| Old-version reads | Follow a new recursive resolver for every past WRITE/Commit | Terminal canonical/private sources and one bounded pending captured-parent edge |
| Namespace cycles | Fresh descendant DFS for every rebound edge | Exact active/completed state under one immutable final graph; O(G) graph visits plus external lookup/order costs |
| Target v2 directory move | Scan all moved descendants to rediscover parents | Verified parent index: O((K+D)H) path/proof work and O(KH) newly changed index pages, plus actual released descendants |
| Reference/content joins | Repeatedly scan a growing full map for each serial | Ordered merge joins and exact external reductions; linear records where orders match |
| External ordering | Re-sort all prior records on every append | Bounded run creation and fixed-fan-in merge; comparison O(L log L), not falsely constant or linear |
| Ownership/normalization | Scan all active/retired owners after each mutation | Exact paged edges and work-triggered continuation; actual admitted work, no history scan |
| Command lifecycle | Search every completed command/token/timer for every new request | Keyed live indices, removable timers/ready entries; O(log Q) admission/cancel and actual ready/exit work |
| Closing one Workspace | Visit every past command or all sibling owners | Its live owner count plus actual selected cleanup; independent entries continue |
| Small-file construction | Reopen/allocate/finish for every tiny file | Same FileSet assembler with deterministic byte/count units; ceil(F/512) only when byte target permits |
| New inode allocation | One round trip per create | Generic ranges; roughly ceil(fresh/4096) acquisitions plus boundary policy; protected C5 progress |

These are engineering bounds requiring enforcement. Device latency, random
external-state lookups, necessary initial graph certification and cleanup remain
real work. Paging an O(n²) loop leaves it O(n²) and adds I/O; it is explicitly
rejected by this architecture. A log factor or unavoidable deleted-subtree walk
is reported honestly instead of being renamed quadratic.

### First-touch parent-span law

Each successor file starts from one opaque span of its complete selected parent.
The first edit splits only required boundaries and adds its replacements. The
lowering cursor emits an inherited span descriptor without opening predecessor
fragment leaves. A candidate crossing capture rewrites only its bounded retained
boundaries under exact frozen coordinates. Within one generation, subsequent
writes splice the same interval tree rather than wrap every previous WRITE.

Known result resolves the pending source through the selected v2 immediate-parent
policy. An old expected-root recipe is not retained in the current target merely
to force equality with a legacy ledger. Pinned readers keep their immutable older
authority and charges. This law prevents bounded-memory processing from secretly
replaying an ever-growing history over many small Commits.

### Exact final-parent law

Namespace v2 derives parent rows and forward effects from the same checked
authority. Verified Base validity is a nonforgeable live capability, not root
hashing alone. The complete batch relation includes replacement, removal, new
directories and cross-moves. Active re-entry rejects cycles; completed reuse is
valid only for that sealed graph. Regular-file aliases remain legal under counts;
directory/symlink parent restrictions and root exceptions stay exact.

Legacy/untrusted initial graphs still need complete paged certification. That
cost occurs before locality-sensitive mutation and is timed wherever the product
actually performs it. A service trust-epoch reset requires explicit recertification;
there is no unbounded resident cache of every historical verified root.

## 4. Scaling evidence to collect later

Prospective diagnostics measure work, not another unchanged performance arm.
Use existing retained receipts/candidate counters first. Freeze minimal additional
functional/count shapes and exact limits before any later diagnostic.

- Graph: directory/edge visits, distinct completed proofs, repeated completed
  traversal, ancestor/index page reads, Base inode demand cardinality, actual
  stack/color/cache/run high-water and negative cycle/alias results.
- Live file: per-write boundary/affected page visits, full-root reseeks, emitted
  update count, payload acquire/release bytes, normalized parent spans, maximum
  resolver depth and cleanup edges. Distinguish append/dispersed/repeated shapes.
- Commit: captured dirty identities, current-generation descriptors versus opaque
  inherited spans, number of metadata passes, bytes read/transferred/replayed,
  file units and known seals, saved-result queries, install path work and actual
  cleanup. Repeated small Commits must not revisit a growing historical E.
- Admission: allocator calls and unused/exposed serials, C2 versus C5 permits,
  protected catalog progress while both Save slots are held, response/window
  ownership and peak simultaneous engine/cache/physical allocation domains.
- Commands: current versus lifetime count, entries visited per new/cancel/close,
  ready/timer registrations and removals, process-domain exits/reaps and retained
  output/FD/context capacity. Fixed active Q with growing N must retain O(Q) state.

No general speed multiplier is inferred from these counts. Numeric comparisons
require matching source/profile/shape/oracle, symmetric cache states and genuine
phase memory. Complete command/cleanup timers cannot omit newly paged maintenance.
Family2 remains reused when its measured mechanisms are unaffected.

## 5. Bounded complete telemetry and the residual audit

Current telemetry has bounded report nodes and explicit dropped/clipped health;
Collector also has a fixed1..16 producer profile. The architecture must not turn
that into a hidden16-Exec ceiling. Register sources at admitted component/process
assembly, and route each operation's fixed identity/aggregate through those
bounded producers or its owned protocol completion. No producer/thread/timeline
is allocated for every historical command or every tiny file.

Select a fixed operation `ObservationSeal` with origin/operation identity,
required-source mask, observed sequence/count/digest and completeness flags.
Admission reserves its terminal-summary capacity. Detailed logging remains
bounded and may report loss; required evidence cannot silently disappear when
a queue saturates or shutdown precedes drain. Data outcome and observation
outcome are separate: a known Commit remains known while its numeric/telemetry
qualification can be INCOMPLETE. Never invent missing values as zero or mark
unknown publication merely because a timing record was lost.

Count per-phase/owner work in fixed aggregates; optional clipped detail is
explicit. Shared channels carry exact source/parent-operation binding. Required
terminal summaries are delivered/checked before releasing their observation
lease; a failed sink/delivery retains a loss disposition. No full historical
event tree or unbounded report spool is introduced. Reactor/protected control
capacity includes these summaries but telemetry cannot consume install credits.

Proposed observation responsibilities, reusing the existing telemetry package:

```text
layerfs-telemetry/src/operation/
  identity.rs                 <=250  bounded origin/operation/source binding
  aggregate.rs                <=450  fixed phase/work counters, no history tree
  completion.rs               <=350  required-source/completeness seal
layerfs-telemetry/src/output/
  retention.rs                <=500  admitted queue/terminal custody and loss
layerfs-bridge/src/contract/observation.rs
                              <=300  opaque bounded completion wire grammar
```

Names are a responsibility map for the implementation, not additional factories
or a second tracing runtime. Existing optional scoped reads and bounded codec
stay usable; origin/byte ownership and explicit completeness are the contract.
The integrated forecast counts only the new/retired product implementation;
harness observer and production-counter repairs have production delta0.

[Issue235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235) records a canary
with a dropped Service report. That old exact-identity receipt remains INCOMPLETE.
Its new proof is the smallest real public-route completeness selection, not
a broad benchmark rerun. Future checks deliberately observe bounded queue loss,
clipped reports, disconnected source, partial frame and shutdown/drain ownership
through external adapters; product test hooks/fake clocks are prohibited.

[Issue174](https://github.com/Ephemeral-AI-Lab/layerfs/issues/174) is an older
residual inventory. Current `AuthenticatedObjects` already exposes optional
scoped batch reads, and mapping navigation/payload uses it. That demonstrates
the interface exists, not that every current report attribution is qualified.
Audit provider override, caller scope, hosted source collection and completeness
at the selected implementation identity; do not reintroduce a redundant timing
interface or quote the old22.13 ms as a current measurement.

The current production counter's SQL function strips full-line -- comments but
does not actually strip block/inline SQL comments or include src/*.sql. Baseline
audit found53 classified runtime SQL files, no block/inline comment instances
and no omitted src SQL files, so those defects do not change this packet's
reproduced baseline totals. Future runtime SQL must stay correctly classified;
repair the counter with external tool tests before using new affected formats,
then apply the same revised method to both snapshots. Never make a future count
smaller by placing implementation outside its scope.

Legacy dead exports/double clones/wave gaps, unknown SQL acknowledgment and
uninduced lock interleavings in #174 each require a current symbol/status audit.
They are not all newly reproduced bugs. The target's allocation transfer,
byte/count waves, exact partial-I/O and known/Unknown ownership contracts supply
their enforcing boundaries; absence of a fault proof remains visible.

## 6. Completion standard

The spec addresses #276 when every current limitation is mapped to an enforcing
architecture contract, every superlinear mechanism has an explicit work law,
and the independent source/profile/custody/resource proof is selected. The issues
are resolved in product only after that implementation and covering evidence
exist. No issue is closed or historical receipt promoted by this research.

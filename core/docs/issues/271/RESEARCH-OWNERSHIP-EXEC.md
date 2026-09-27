# #271 research: Exec ownership and extent publication

> **Status:** Research; informative and not a product contract.

Source inspected: four-hop product commit
`933b3457c916519f458137e4c4f19662c8e9228e`. The retained 4,097 diagnostic
used that product seal and the separately pinned runner source in
[FOURHOP-4097-EXTENDED-DIAGNOSTIC.md](FOURHOP-4097-EXTENDED-DIAGNOSTIC.md).
Its cache state is uncontrolled, so all raw times are **INELIGIBLE** for a
speed PASS. The original 25 s 4,097 gate remains **FAIL**.

## What one acknowledged WRITE actually does

The public runner issues one-byte `pwrite` calls at distinct even offsets; the
workload requires exactly one observed FUSE WRITE callback per call
([prospective workload spec](../../../../docs/roadmap/0.1/0.1.7/issue271-root-edge-workload-spec.md)).
One callback acquires a payload, then invokes the projected write before replying
([adapter.rs:496–577](../../../crates/layerfs-fuse/src/adapter.rs#L496-L577)).
`own_payload` runs `maintain_backing` before acquisition
([payload.rs:716–738](../../../crates/layerfs-workspace/src/backing/payload.rs#L716-L738));
maintenance can reclaim an unheld old root and released payloads
([reclaim.rs:290–299](../../../crates/layerfs-workspace/src/backing/reclaim.rs#L290-L299),
[metadata_reclaim.rs:456–550](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs#L456-L550)).
The existing `acquisition_ns` bucket therefore **includes reclamation**; it is
not a payload-create timer. Publication also calls maintenance before taking
the metadata writer gate ([write.rs:383–425](../../../crates/layerfs-workspace/src/filesystem/write.rs#L383-L425)).

For the accepted edit, publication reads the old keyed inode, creates one
candidate, gives the payload a custody record, splices its extent sequence,
updates the keyed dirty and inode cells, seals intermediate pages, then installs
the new root under the state check
([write.rs:426–437](../../../crates/layerfs-workspace/src/filesystem/write.rs#L426-L437),
[write.rs:591–675](../../../crates/layerfs-workspace/src/filesystem/write.rs#L591-L675),
[write.rs:676–750](../../../crates/layerfs-workspace/src/filesystem/write.rs#L676-L750)).
The splice shares untouched subtrees and rewrites touched leaves and their
ancestor path ([splice.rs:573–699](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs#L573-L699)); the keyed update similarly copies a path, but its new pages still charge **all** decoded edges
([update.rs:275–370](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/keyed/update.rs#L275-L370),
[ownership.rs:862–911](../../../crates/layerfs-workspace/src/backing/ownership.rs#L862-L911)).

For each copied extent page, four-hop sponsorship is considered only when the
new page has at least four edges and the old sponsor's recorded depth is below
four. It reads and authenticates that old owner and page, decodes and sorts both
edge multisets, charges new edges plus one sponsor reference when that saves an
edge, and otherwise charges the whole encoded edge list
([sponsored.rs:10–49](../../../crates/layerfs-workspace/src/backing/ownership/sponsored.rs#L10-L49),
[sponsored.rs:74–108](../../../crates/layerfs-workspace/src/backing/ownership/sponsored.rs#L74-L108)).
A fifth successive eligible copy after depth four falls back to full ownership;
it does not extend the chain. The immutable page is then created, allocated,
written and given an owner record before its edges are incremented
([sponsored.rs:110–143](../../../crates/layerfs-workspace/src/backing/ownership/sponsored.rs#L110-L143),
[ownership.rs:751–845](../../../crates/layerfs-workspace/src/backing/ownership.rs#L751-L845)).
The ledger batches **adjacent references to one ledger page** into one
authenticated 4 KiB read-modify-write; it does not coalesce every later visit
to that same ledger page across a candidate
([ledger_batch.rs:13–73](../../../crates/layerfs-workspace/src/backing/ownership/ledger_batch.rs#L13-L73),
[ledger_batch.rs:77–101](../../../crates/layerfs-workspace/src/backing/ownership/ledger_batch.rs#L77-L101)).

Old roots are released only when unheld. Cleanup recomputes a page's charged
edge difference from authenticated bodies, releases those edges before its
sponsor, descends on a zero reference, then unlinks and refunds the page and
returns its slot ([metadata_reclaim.rs:16–194](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs#L16-L194)).
Seal also cleans orphaned intermediate pages before returning
([metadata.rs:841–925](../../../crates/layerfs-workspace/src/backing/metadata.rs#L841-L925)).
This makes per-write reclamation real work inside Exec even though the final
Commit has not begun.

## Cost model and what the retained run supports

Let `N` be acknowledged one-byte WRITEs, `E_i` the live extent records before
WRITE `i`, `H_e(i)` the extent root height, `H_k(i)` the keyed root height,
`P_i` the number of immutable pages created by that write, `F_p` the number of
child or Local custody fields in copied page `p`, `A_i` the charged additions,
`D_i` the charged decrements from seal and old-root cleanup, and `G_i` the number
of adjacent same-ledger runs those ref changes visit. The current format holds
4 KiB pages, at most 124 extent records per new leaf, a **32-child new-branch
target**, up to 248 children in an older readable branch, 62 owner records per
ledger page, and a declared tree-level ceiling of seven
([metadata_pages.rs:4–14](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs#L4-L14),
[splice.rs:27–29](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs#L27-L29),
[pack.rs:12–15](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/pack.rs#L12-L15),
[ownership.rs:16–18](../../../crates/layerfs-workspace/src/backing/ownership.rs#L16-L18),
[architecture §14](../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md#14-bounded-branch-ownership-edges-271-2026-09-27)).

For a narrow edit away from a split, tree traversal/copy is `O(H_e + H_k)`
pages, with `O(F_p log F_p)` CPU to decode and sort each copied page's edge
lists and `O(F_p)` for its multiset comparison. The sponsor read adds one old
page and owner lookup per eligible copy. Its edge ledger I/O is at least tied
to `G_i`, and `G_i ≤ A_i + D_i`; owner creation, custody creation, free-slot
handling and page cleanup add separate ledger RMWs. A useful **counting**
expression is therefore

```text
Exec ownership cost ≈ Σ_i [new-page owner RMWs + sponsor owner/body reads
                         + G_i charged-edge RMWs + zero-ref cleanup RMWs]
plus O(Σ_i Σ_(p copied at i) F_p log F_p) CPU for edge extraction/comparison.
```

That expression is not an elapsed-time formula: one RMW also opens and checks
a private file, and direct I/O calls need not equal physical media transfers.
`ledger_reads`/`ledger_writes` count issued 4 KiB backing operations
([ownership.rs:288–395](../../../crates/layerfs-workspace/src/backing/ownership.rs#L288-L395));
the path calls `openat`, `fstat`/`metadata`, `pread` or `pwrite`, and close, not
a Service RPC ([ownership.rs:202–229](../../../crates/layerfs-workspace/src/backing/ownership.rs#L202-L229),
[segments.rs:222–270](../../../crates/layerfs-workspace/src/backing/segments.rs#L222-L270),
[segments.rs:294–328](../../../crates/layerfs-workspace/src/backing/segments.rs#L294-L328)).
Splits and old-root cleanup can make one callback dearer than a simple path
copy. With an abstract unbounded height and fixed fanout, repeated narrow
edits with `E=Θ(N)` suggest `O(N log N)` path work plus bounded-width edge
charging and amortized reclamation; there is **no source proof of global
`O(N²)`**. Conversely, the hard seven-level/slot ceilings make asymptotic
notation alone a poor predictor of the supported finite range. The depth-four
reset and rising branch occupancy can create a large finite-range coefficient.

The retained four-hop 4,097 diagnostic observed exactly 4,097 callbacks and
reported 27.918267 s raw Exec, 0.434905 s raw Commit, 32.421651 s complete
wall, all cache-INELIGIBLE. Its first 512 WRITEs issued **19,426/9,807** ledger
reads/writes; by WRITE 4,096 the cumulative counts were **220,377/119,492**.
Per 512-write block, reads were 19,426, 22,356, 28,173, 29,575, 29,518,
30,285, 30,513, 30,531; writes were 9,807, 12,071, 15,190, 16,653,
15,870, 16,500, 16,740, 16,661
([retained report](FOURHOP-4097-EXTENDED-DIAGNOSTIC.md),
[raw receipt](evidence/fourhop-4097-extended-v1/run/receipt.json)).
Root height first reached two at WRITE 1,039. The near plateau after that
transition **argues against sustained quadratic growth within this observed
range**, but does not prove a global bound. The final sampled FUSE buckets
through WRITE 4,096 were 12.083 s acquisition and 15.650 s publication; both
include work outside the named primitive, and their sum is short of full SDK
Exec. Attribution to ownership or reclaim is currently **an inference**.

There is one SDK Exec for the shell workload, not one SDK or Service call per
byte. Reducing 4,097 FUSE callbacks would change the specified one-byte
`pwrite` workload or its cache semantics. Potentially removable roundtrips
inside the product are local backing open/stat/read/write/close sequences and
repeated ledger-page RMWs. The current counts do not reveal how much wall time
those sequences cost, so a general 2× Exec claim would be premature.

## Structural options, in order of testable promise

| Option | Expected work removed | Cost and proof obligation |
| --- | --- | --- |
| **Candidate-scoped ledger coalescing** | Accumulate all owner and ref changes for one unpublished candidate by ledger index, then issue one authenticated RMW per distinct touched ledger page rather than one per disjoint run. Potential I/O reduction is governed by repeated visits `G_i − U_i`, where `U_i` is distinct ledger pages touched, **not** by `N` alone. | A bounded, quota-charged overlay must answer reads of newly created candidate pages and owner records before flush. Persist before root publication; record applied-page progress for definite failure; quarantine on an uncertain write. Old roots and concurrent readers must never see a freed edge or half-published root. The writer gate and current seal order give a place to investigate, but no proof or speed result exists. |
| **True structural edge sharing** | Give an immutable page a separately owned, persistent edge-set root so a one-edge change path-copies `O(log F_p)` edge-set nodes and charges only new refs. It removes the every-fifth full-list recharge without an unbounded sponsor chain; live old snapshots share edge-set nodes. | This changes the ownership format and adds pages/opens. The edge-set must exactly cover each encoded child/Local field, with multiplicity, and old-root/G1/G2 reads must survive arbitrary release order. Reclamation must be bounded and charge/refund every node once; a one-byte write may get slower if extra metadata dominates. Compare it only after measuring the current ownership breakdown. |
| **Mutable versioned edit journal with indexed reads** | Append one accepted edit and payload reference per WRITE, avoiding immediate extent and keyed root path copies; batch or build the final extent tree on Commit. Aim for `O(1)` amortized append plus `O(log N)` indexed read, `O(N)` final walk/build. | A different transaction format and public read path. It must preserve last-write-wins overlap, append, truncate, sparse zeros, frozen Commit inputs, exact old/new roots, retained G1/G2 generations, bounded memory, quota/partial-failure accounting, and unknown-outcome quarantine. An append-only log **without** an index merely moves an `O(N)` scan into each read; an unbounded in-memory index violates the resident budget. This is a high-risk architectural option, not a ready fix. |

No option should be credited with fewer kernel WRITEs. A single arena file or
bounded authenticated FD lease might reduce per-page filesystem syscall cost
if that dominates, but it leaves edge-count complexity intact and must retain
pathname/identity and allocation checks
([ownership.rs:171–229](../../../crates/layerfs-workspace/src/backing/ownership.rs#L171-L229)).

## Next diagnostic and decision rule

Prospectively declare one **cause** diagnostic rather than re-running an
unchanged treatment arm. First pass: use fixed-size, per-operation counters
and cumulative monotonic nanoseconds, snapshot them only at the existing
per-512 checkpoint. Time exclusive children of the current FUSE
`acquisition_ns`/`publication_ns` buckets
([adapter.rs:546–563](../../../crates/layerfs-fuse/src/adapter.rs#L546-L563),
[write_sample.rs:52–78](../../../crates/layerfs-fuse/src/write_sample.rs#L52-L78)):

| Boundary for a bounded timer/counter | Distinction it must answer |
| --- | --- |
| `own_payload` around `maintain_backing` and `PayloadHost::acquire` ([payload.rs:716–738](../../../crates/layerfs-workspace/src/backing/payload.rs#L716-L738)); inside maintenance, around metadata versus payload reclaim ([reclaim.rs:290–299](../../../crates/layerfs-workspace/src/backing/reclaim.rs#L290-L299)) | Old-root reclaim, payload reclaim, and actual payload acquisition must have separate totals. Count roots/pages/payloads freed, not all registry records inspected on every WRITE. |
| `publish_file_mutation` around its maintenance, old-root lookup, custody, extent splice, keyed update, seal and state install ([write.rs:395–437](../../../crates/layerfs-workspace/src/filesystem/write.rs#L395-L437), [write.rs:595–675](../../../crates/layerfs-workspace/src/filesystem/write.rs#L595-L675), [write.rs:676–750](../../../crates/layerfs-workspace/src/filesystem/write.rs#L676-L750)) | Which publication segment grows with `N`; count copied extent versus keyed pages, split events, root height and cleanup frames. A timer around all of `write_file` alone cannot answer this. |
| `ledger_file` open plus identity/allocation validation ([ownership.rs:202–229](../../../crates/layerfs-workspace/src/backing/ownership.rs#L202-L229)); `read_owner`, `set_owner`, `change_refs_run` at their direct-I/O calls ([ownership.rs:288–395](../../../crates/layerfs-workspace/src/backing/ownership.rs#L288-L395), [ledger_batch.rs:17–73](../../../crates/layerfs-workspace/src/backing/ownership/ledger_batch.rs#L17-L73)) | Count and time open/stat validation separately from authenticated 4 KiB reads and writes, grouped by owner-create, charged-add and cleanup-decrement cause. Track adjacent runs `G_i` and distinct ledger indices `U_i` with a bounded candidate-local set; their difference is the coalescing opportunity. |
| `write_raw_page` sponsor decision and `create_file` ([sponsored.rs:74–111](../../../crates/layerfs-workspace/src/backing/ownership/sponsored.rs#L74-L111), [ownership.rs:751–845](../../../crates/layerfs-workspace/src/backing/ownership.rs#L751-L845)) | Count sponsor attempt, accepted, depth-four fallback and no-savings fallback; time old-owner/body read plus edge decode/compare separately from `create`/`fstat`/`fallocate`/page write. |
| `cleanup_step` and `seal` ([metadata_reclaim.rs:16–194](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs#L16-L194), [metadata.rs:841–925](../../../crates/layerfs-workspace/src/backing/metadata.rs#L841-L925)); `complete_projection_mutation` and the FUSE notifier ([coherence.rs:568–630](../../../crates/layerfs-workspace/src/runtime/coherence.rs#L568-L630), [mount.rs:247–260](../../../crates/layerfs-fuse/src/mount.rs#L247-L260)) | Attribute zero-ref descents/unlinks/refunds and the mandatory notifier wait separately from page publication. The existing publication bucket includes the notification because `mutate_file` invokes it before returning. |

Keep timed child totals nonoverlapping; retain inclusive parent totals only for
reconciliation with FUSE callback and SDK Exec wall. Count actual local
`openat`/`fstat`/`pread`/`pwrite`/`fallocate`/`unlink` calls by cause, not
assumed calls from 4 KiB I/O totals. Counters must be fixed-size and inert
when disabled; no per-write log, all-record scan, uncharged dynamic map or
cache warmup. `metadata.rs` is already **963 physical lines** against its 999
ceiling, so place any implementation in the owning focused modules or existing
write-sample path instead of enlarging that file. Record the instrumentation
cost and cache status with the diagnostic; these timings cannot promote the
retained gate.

Choose ledger coalescing only if repeated ledger-index visits and their wall
time are large enough. Choose an edge-set format only if full-list resets or
unchanged-edge charging dominate after coalescing. Explore a journal only if
path copies remain the dominant term across workloads. Every prototype needs
the external old-root/G1/G2, abandoned-candidate, depth/failure, exact-quota
refund and clean-close proofs already exercised by
[owner_finalization.rs:185–303](../../../crates/layerfs-workspace/tests/owner_finalization.rs#L185-L303),
plus public old/new-head and full-byte oracles. A measured 2× general-speed
claim would need cache-qualified, source-matched workloads beyond this tiny
write case; a successful 60 s diagnostic cannot repair the retained 25 s gate.

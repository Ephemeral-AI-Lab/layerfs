# SQLite plan and count/cause investigation, 2026-10-04

> **Status:** Research; informative and not a product contract.

Investigation starts at clean d9da19ecf (product freeze 8ddb4c8e2). All existing
receipts remain unchanged. The [diagnostic](checks/sqlite-plan-profile1/diagnostic.json)
and [stage attribution](checks/sqlite-plan-profile1/stage-attribution.json) retain
raw inputs, actual EXPLAIN / EXPLAIN QUERY PLAN, counts and omissions. No new
performance sample has been taken. Retained Store SHA256 before/after is
620e50693da1024fc93f5c45a65a4b952d3e9cab3320edbd2a80d40934b5eb56.
All mutations were confined to an independent byte copy under
benchmark-results/fs-bench-pro/issue302-sqlite-plan-profile1. This copy cannot
supply a gate sample. OS cache is uncontrolled in these cause diagnostics.

## Actual statements and work

Native system SQLite3.51.0 EXPLAIN plans are stored as *.plan with literal bounded
inputs; Python SQLite3.51.2 plans are supplemental. Publication INSERTs are only
EXPLAINed, never executed on the retained Store. Metadata ordinal and locator CID
come from real rows. The mapping diagnostic uses each of the1271actual pack IDs
exactly once, preserving the production query's LIMIT257.

| Candidate whole operation statement class | Executions | Returned rows | VM steps | Observed step ns |
| --- | ---: | ---: | ---: | ---: |
| All-unit mappings | 330219 | 13364974 | 124577015 | 18067313000 |
| Singleton locator plus pack descriptor | 467155 | 467131 | 10744253 | 8294268000 |
| Bounded control plus descriptor | 330219 | 330219 | 7595037 | 5690321000 |
| BEGIN | 812184 | 0 | 2436552 | 586991000 |
| COMMIT | 814724 | 0 | 2444172 | 1790612000 |
| Individual unit INSERT | 11755 | 0 | 575995 | 54957000 |
| Singleton value-group covering | 2702 | 2702 | 181034 | 46195000 |

Observer spans include overhead/nesting and are not an exclusive CPU attribution.
The mapping query's existing unique index already supports ordered lookup. EXPLAIN
contains DeferredSeek plus table Column instructions for offset/length: these
metadata fields live beside the BLOB, and are not covered by the index. A plan
alone does not establish physical acquisition. The separate once-per-pack native
count diagnostic supplies that mechanism evidence below. No missing-index claim:
the proposed change covers an existing lookup.

The locator uses object_location's WITHOUT ROWID primary key and pack's integer
primary key, not a full scan. The bounded control uses a pack integer-primary-key
lookup and CASE length check; its extraction remains bounded. The singleton
value-group query uses a descending predecessor lookup and integer-primary-key
fetch; DISTINCT/ORDER BY use temporary B-trees. Its measured step span is small
relative to mapping. No evidence currently justifies removing multi-ordinal
uniqueness/order validation or enlarging batches.

Unit INSERT EXPLAIN includes uniqueness/check/foreign-key and B-tree insert work.
Publication has11755unit inserts across1271packs; that is materially fewer than
13364974mapping rows. No justification exists to remove constraints, immutability,
prospective row/binding charges or atomic publication. BEGIN/COMMIT plans show
transaction controls, not row traversal. Actual transaction counts are scoped:

| Candidate stage | Statements | BEGIN executions | Pack acquisitions | VFS requested read bytes | Observed step ns |
| --- | ---: | ---: | ---: | ---: | ---: |
| Construction | 5267 | 1612 | 431 | 13912004 | 264998000 |
| Filesystem | 475128 | 131409 | 70518 | 1577673982 | 12440952000 |
| Save/custody | 2346130 | 679163 | 259270 | 8298991902 | 25622074000 |

Acquisition stage has zero SQL work. Baseline BEGIN uses another SQL spelling,
so its zero exact-BEGIN count is not zero baseline transactions. Stage spans are
differences from existing snapshots, not a new run. Save/custody accounts for
679163/812184 exact candidate BEGINs, but no transaction enlargement is proposed.

## Mapping cardinality and allocation

The closed Store has1271packs/11755groups, mean9.2486groups per distinct pack,
maximum136; complete histogram is retained. The real run averages40.4731mapping
rows per acquisition (13364974/330219). This is repeated validation weighted
toward larger packs, not a single scan over11755unique rows. The product must
continue validating every mapping before any group BLOB opens.

Typed conversion already eliminates generic per-row cells. Current mapped results
still start Vec::new, grow geometrically and copy entries during reallocations,
even though validated directory views give the exact bounded count before SQL.
Reserving that count is an authorized, format-preserving implementation change;
no cross-acquisition memo or cache is proposed. It does not remove row stepping
or repeated directory validation, and no speed claim follows from its source.
Per-row step/decode lifecycle timers also remain real cost; removing diagnostic
coverage is not part of this treatment.

## Covering-index diagnostic and concrete schema review

On the independent copy, CREATE INDEX diagnostic_pack_unit_cover ON
pack_unit(pack_id,group_number,offset,length) changes mapping to a covering-index
lookup (rowid/unit_id is already carried by the index). One pass per pack gives:

| Count/cause metric | Original | With covering index |
| --- | ---: | ---: |
| Queries | 1271 | 1271 |
| Returned rows | 11755 | 11755 |
| Integer-cell checksum | 424508089 | 424508089 |
| Actual VM steps | 122317 | 109291 |
| SQLite cache hits | 5552 | 2540 |
| SQLite cache misses | 3091 | 53 |
| Diagnostic wall ns | 12678000 | 3086000 |

SQLite page cache is explicitly2MiB/mmap0/temp-memory, then released before each
count traversal. OS pages may be resident. These times do not predict a cold
LayerFS speed ratio. The additional index uses53pages/217088B/175382payload B;
this is not the resulting gate Store's final allocation. It increases publication
index maintenance and storage, requiring a fresh matched arm and unchanged bounds.

Concrete proposed production treatment for review: add an explicit creation-only
SqlitePackLayout::GroupRowsIndexed, schema3, with the same immutable encoded unit
rows and an additional non-unique covering index on(pack_id,group_number,offset,length).
Keep UNIQUE(pack_id,group_number). Ordinary open dispatches deterministically on
stored versions1/2/3; historical schema2 definitions/source identities remain
accepted exactly. No migration, automatic index creation at open, replacement of
the default monolithic layout, physical-format change or fallback. Publication
rows/binding bytes retain their original caps; the index's storage is counted in
the same92342273B strict allocated ceiling. Cold/proof/buffer/worker/time bounds
remain. Schema3 must have its own prospective selection/identity because old
schema2receipts cannot be relabeled.

This schema3 creation option requires owner review before production application:
the handoff explicitly requires concrete review for additional schema treatments
outside the approved GroupRows scope. The native diagnostic and schema design are
reviewable now; this document is not implementation approval. Authorized exact
result reservation can proceed independently.

At this point17/53at a new artifact, Durable, Init, full payload audit and all-seven
release admission are NOT_RUN. Current Disposable157performance remains its
original +0.585734% result, not a new result of this investigation.


## Owner approval and prospective qualification

Owner explicitly selects **Approve schema3 covering index and matched qualification**.
The schema3 treatment and exact bounded result reservation are implemented together.
Prospective case `phase7-sqlite-disposable-history-stride1-group-rows-indexed-v1`
uses the same157states,300s complete performance/30s separate bounded proof,
10% comparison margin and92342273B strict allocated ceiling. Normal runner only,
one cold sample per arm, new matched original-product reference first. Fresh paths
are `issue302-indexed-mapping-history157-reference1` and
`issue302-indexed-mapping-history157-candidate1`. Reuse sealed worktree targets,
prepared corpus and observer; no fixture regeneration or unchanged speed rerun.
Both arms bind the changed harness; raw old rows remain unchanged. Final results
will be appended with identities, cold/proof/cleanup/count/storage and omissions.


## Final functional freeze

[checks/indexed-mapping1](checks/indexed-mapping1/checks.json) retains locked Core
workspace/all-target tests514/98targets/0ignored, all-target Clippy-Dwarnings,
fmt--all--check,454-file product boundary,23tool selftests and11focused history
harness checks, all PASS. Focused schema2 tests also passed7/7 for the separate
reservation edit before approval. No schema2performance arm was sampled.
The first formatting invocation lacked--all and refused to find workspace-root
targets; adding the required workspace option performed formatting, then the
frozen formatting check passed. There was no failed product test or performance
attempt. No CI or retired aggregate preflight was run.


## Indexed-mapping matched result at9847f0f9c

One fresh matched Disposable157pair completed. Reference product183913176750ns,
candidate186070175750ns(+2156999000ns,+1.172835%); complete commands197768933958/
198908779084ns. Separate bounded proofs16337964875/18966352167ns both PASS,
all cold/cleanup checks PASS. Final allocated86179840/85172224B; candidate remains
below92342273B, but is1224704B above original83947520B. Canonical104618objects/
871337620B and157roots match; pack bodies74809772B. Candidate proof covers157states,
904143paths,66content paths,921174logicalB/5347088acquiredB, not exhaustive payload.
Reference proof acquires17878185B under the same32MiB cap. Source/harness/observer
and immutable manifests will be audited into the final comparison index.

Mapping retains330219executions/13364974returned rows. Actual VM steps110881822,
exactly13695193below earlier schema2mapping124577015(one per step/DONE). New mapping
step span2423818000ns; earlier schema2span18067313000ns is separate-window mechanism
evidence, not a matched speed attribution. Save/custody91041543245vs76076072373ns;
filesystem41814624428vs54850878040ns. Whole-child CPU131924785000/131911476000ns,
RSS336150528/349945856B(lifetime, not phase-only). Save VFS requested7082877534/
11939753738B; acquisitions259270/271880. No full slowdown removal is demonstrated.

Native final work still has3452679unit BLOB open/read/close calls,5905886097read B,
815043transactions/2541writes,2827545statements. Statement wall41493023233ns and
transaction wall85058540278ns overlap; BLOB read19506147688ns excludes opening and
closing. Avoid attributing the entire remainder to a single cause. Existing
observer classification also performs linear searches of up to790classes; its
inclusive overhead differs with statement shape. No observer overhead speed claim
or product-cache treatment is inferred from that observation.

## Further count/cause diagnostic and authorized cursor optimization

[checks/unit-reopen-cause1](checks/unit-reopen-cause1) runs native read-only SQLite
once over each closed multi-group pack, not another product sample. Every body is
read inside the diagnostic. Original11268open/close pairs versus784open/close pairs
and10484reopens return identical64092418B/byte-sum7763801632. Diagnostic walls
53690000/25408000ns have uncontrolled OS cache and are not a cold speed comparison.
SQLite2MiB/mmap0settings and a released SQLite cache are recorded. No retained DB
is mutated; current gate Store SHA is retained in its original manifest.

The supported `rusqlite::Blob::reopen` API is used for subsequent groups within
one acquisition callback. All metadata checks precede opening; each moved cursor
checks BLOB length and exact offset read, then checked-close. No cursor crosses
callbacks/requests/writes, no SQL result buffer is introduced, no cache/transaction
or physical schema/publication bound changes. Failed moves do not retry and preserve
uncertain-outcome quarantine. External observer delegates reopen and updates only
its optional acquisition-trace row ID; acquired-byte accounting stays on actual
successful reads. New tests exercise one open/seven reopens/eight reads/one close,
separate callbacks, and wrong reopened-body length with checked close. Existing
sparse/all-lane/misbinding/rollback coverage remains.

Prospective changed product/observer requires another newly matched pair under
`phase7-sqlite-disposable-history-stride1-group-rows-indexed-v1`, unchanged300s/30s,
10%margin/92342273B. Fresh outputs `issue302-unit-reopen-history157-reference1` and
`issue302-unit-reopen-history157-candidate1`. No old performance/control reuse,
no unchanged candidate rerun, no timeout/cache/buffer/worker relaxation. Reuse
prepared corpus/worktree-local sealed builds only.17/53,Durable,Init,all-seven and
exhaustive payload remain NOT_RUN at the new artifact.


## Reopen functional freeze

[checks/unit-reopen1/checks.json](checks/unit-reopen1/checks.json) records Core locked
workspace/all-target tests516/98targets/0ignored, all-target Clippy-Dwarnings,
fmt--all--check,455-file boundary,23tool selftests and6observer calibration tests,
all PASS at this finalized product. New tests preserve byte-authentication/failure
semantics while proving one cursor per callback and actual reopen work. Harness
selection/proof scope is unchanged from the11checks at9847f0f9c; no redundant
unchanged harness or performance rerun occurred. Native3.51.0reopen count-cause
read-only diagnostic has no failures. Both indexed-mapping manifests pass every
retained file length/SHA check, including the retained Stores, with the exact
index in[checks/indexed-mapping-final1/comparison.json](checks/indexed-mapping-final1/comparison.json).
The earlier +1.172835% result stays unchanged and remains a numeric-margin PASS,
not a strict-faster PASS. Raw check stdout is preserved separately where the
tracked presentation removes trailing EOF whitespace; custody records bind both.

# Typed singleton locator and control allocation treatment, 2026-10-04

> **Status:** Research; informative and not a product contract.

Owner answers “proceed” to the proposed allocation/count investigation. Start at
clean f3025f81e, product freeze 6e143ac60, in the existing isolated checkout.
No subagents, new worktree, primary checkout changes or external messages.

## Evidence before implementation

Prior SQL plans and the final retained profile already identify the unchanged
singleton locator and bounded control/descriptor SQL. They execute 467155 and
330219 times respectively in the current history workload. Their SQL text and
plan are unchanged by this treatment; no missing-index or new SQL-plan claim.
The current query wrapper creates owned bindings and generic row cells, including
heap-owned fixed-width object ID/digest copies followed by copies during parsing.
Singleton lookup also constructs deduplication and SQL-format collections.

The external example `metadata_allocation_probe` uses a counting Rust global
allocator and the actual public read-only persistence ports. It selects the first
1024 sorted distinct CIDs and first128 sorted pack IDs once each from a retained
closed schema3 Store. Setup, input inventory, connection creation and caller-owned
output capacity precede counting. Locator output IDs are checked; control/first-group
reads check returned-byte sums. No benchmark/product test hooks or alternative
implementation enter src/. No new dependency is added.

[The diagnostic record](checks/typed-descriptor-cause1/comparison.json) pins the
input Store hash and both compiled binaries. That Store still matches its original
manifest SHA256 0871680ffd8d28695a3a689b54a7597a64b9d48143642f9e9b064567cf5234de.
Caches are uncontrolled; these are count/cause diagnostics, not speed samples.
The counter covers Rust allocation/reallocation requests, not C malloc, live
memory, process RSS or phase-only memory. The same vehicle/counters/inputs are
used before and after; Rustfmt changes vehicle presentation only.

| Diagnostic | Calls | Before allocations | After allocations | Before requested bytes | After requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Singleton locator | 1024 | 21504 | 7168 | 2616320 | 1187840 |
| Control plus first-group acquisition | 128 | 2304 | 1664 | 2928969 | 2881865 |

Per-call Rust allocation count is **21 →7** for a found singleton and **18 →13**
for control/first-group acquisition. Control reallocation count stays128. Exact
checksums remain118302818 and324587421. These counts do not extrapolate to a
measured campaign allocation total or prove a clock percentage.

The first vehicle build failed E0599 by assuming into_parts on the AcquiredPackRead
enum. Explicit matching of its Units carrier repairs the diagnostic; no public
call or performance sample occurred in that failed launch. Initial build/native
launch logs and empty outputs remain, followed by successful before2/after outputs.
No unchanged product performance arm was retried.

## Treatment and invariants

Singleton locator input chooses the same literal IN(?) SQL, borrowed fixed-width
ID binding and direct typed LocatedObject decode through the existing measured
mapped wrapper. It preserves the original input/SQL limits, one transaction,
missing-ID behavior, role/signed-coordinate checks and descriptor validation.
Result ownership stays private until query DONE and lifecycle cleanup succeed.
Multi-ID requests retain their original bounded deduplication/batch path.

Group-row controls use the same CASE-bounded SQL, borrowed stack integer bindings
and a typed PackInfo/control pair. Both typed and generic descriptor decoding
share exact positive-ID/domain/digest-width/length checks. The one necessary
control copy remains, with SQL and Rust length checks before group BLOB acquisition.
All directory/mapping rows and subsequent BLOB length/authentication work remain.
Statement/VM/binding-byte/lifecycle accounting and unknown-outcome quarantine use
the existing measured wrapper. No query, transaction, cache, schema, publication,
worker, body/output limit, durability profile or dependency changes.

## Frozen checks and prospective qualification

The finalized product passes locked Core workspace/all-target tests, all-target
Clippy -Dwarnings, formatting,455-file boundary and23tool selftests; exact commands,
counts and walls are in [checks/typed-descriptor1](checks/typed-descriptor1/checks.json).
New tests cover singleton/batch equivalence for schema1/2/3, missing IDs, unchanged
three-statement/one-transaction boundary, malformed domain/digest/length before any
BLOB opens, invalid roles and negative coordinates. Prior observer/harness checks
retain their identities; neither source changes in this treatment.

Run one fresh newly matched Disposable157pair under the existing selection
`phase7-sqlite-disposable-history-stride1-group-rows-indexed-v1`,300s complete
performance,30s separate bounded proof, unchanged10%time margin and92342273Bstrict
allocated ceiling. Fresh paths `issue302-typed-descriptor-history157-reference1`
and `issue302-typed-descriptor-history157-candidate1`. Sole normal runner.py;
candidate requires the new reference root-pins.json. Reuse sealed worktree-local
release/locked targets, observer and prepared corpus; no fixture regeneration,
old speed-control reuse, warm credit or unchanged sample replacement.
17/53,Durable,Init,all-seven and exhaustive payload audit remain NOT_RUN at the
new artifact. PostgreSQL/MinIO M4pause remains unchanged. Final outcomes follow
in an appended record with exact identities/arithmetic/nonpassing lines.


## Final matched qualification, 2026-10-05

Frozen product edcfae4ac: reference182385692375ns/candidate173624895625ns,
**−8760796750ns /4.803445180%, strict faster PASS**. Complete commands
196530167834/185722603417ns; separate proofs17056830708/18164147875nsPASS;
source/per-state cold and cleanupPASS. Canonical104618objects/871337620B,
all157roots/904143paths and bounded921174logicalB/5347088candidateacquiredBmatch.
Allocated85172224Bunchanged: approved92342273BceilingPASS; original83947520Bstrict
FAIL(+1224704B /1.458892413%). Store bytes and all prior statement/transaction/
mapping/BLOB-read/publication counts remain identical; no work was skipped.

Final Save/custody83726565663ns versus77748205867nsreference remains7.689386179%
slower; filesystem37576296325vs51817843454nssaves27.483866907%. Whole-childCPU
121402363000vs131034572000ns; lifetimeRSS335347712vs348160000B(notphase-only).
Actual read transaction origins:469825locator/330219scoped-pack requests account
for800044transactions,98.159728996%of815043. These are ordinary request snapshot
boundaries; no accidental nested BEGIN is demonstrated and no transaction collapse
is performed under the unchanged contract.

[Final report](SQLITE-TYPED-DESCRIPTOR-FINAL-REPORT.md) records exact reproduction,
limits/checks/LOC/nonpassing lines. [Sealed comparison](checks/typed-descriptor-final1/comparison.json)
audits23reference/21candidatefile lengths/SHA plus archived binary hashes;
[transaction origins](checks/typed-descriptor-final1/transaction-origins.json) and
[count continuity](checks/typed-descriptor-final1/count-continuity.json) preserve
scope/identity. Prior/new clocks remain separate windows; no exclusive delta is
claimed from the old candidate clock.17/53,Durable,Init,all-seven/full-payload
NOT_RUN; PostgreSQL/MinIO M4pause unchanged. No push/merge/PR/release/external message.

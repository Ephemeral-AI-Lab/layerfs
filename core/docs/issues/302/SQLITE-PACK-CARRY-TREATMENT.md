# Partial pack queues retained across input waves

Status: Count mechanism and closure qualification pass; speed unmeasured.
Based on8d4775e33.1536 deterministic500B ordinary objects produced5packs/
4publications because each input wave unconditionally drains partially filled
lane queues. Count test firstFAIL,target/phase7-agent/pack-carry-before.log.
Removing that unconditional drain uses4packs under the same existing grammar.

The exact same queues now persist until their existing pressure bounds, needed
reference/advisory/read closure or finish drains them. Aggregatequeued bytes+
lane framing<=256KiB,queuedrows<=512; no new capacity, producer or cache window.
Ready packs publish at each wave, pending signature rows await their objects,
finish drains all groups/queues/acks. No published BLOB append or weakened
first-wins/unknown-outcome semantics. Canonical/codec identity unchanged; mixed
physical grouping may differ. Reservation bound includes carried groups as before.

Scoped final checks:6 transaction/cache/pack tests,3 reference acquisition tests,
9 publication tests and project full100/1000 namespace oracle PASS (19unique).
Same-save read of carried objects and parent closure force sealing and full
readback. Earlier tests18PASS plus new closure test; no new source fix afterthat.
Storage/persistence/project all-target Clippy-Dwarnings,fmt,boundary438 and
23self-testsPASS. Other unchanged suites not repeated. Retained count failure
is not a speed sample.

Prospective next: one matched release/locked1000-file Init-v2 pair at frozen
identity with same15s complete/9.5s separateproof,cold-content attestation,fresh
create/import/checkpoint/allocationrelease/close product clock and unchanged
allocation gate. Other threeInit/history rows remain required/unrun at this
identity. No original failure relabel, unchanged-identity resample or cap change.


## Matched1000 result at22e5f1d3c

Reference137991542ns/current228675584ns, ratio1.657171017, timeFAIL;
exact10*current=2286755840 >11*baseline=1517906962.
Roots match. Independent sampled proof/cold-content attestation/cleanupPASS.
Finalallocation23101440/20553728B PASS. Complete201397375/1143337458ns and
separateproof48751000/43237458ns fit15s/9.5s. Checkpoint/allocationrelease1755000ns
inside product clock; main20520960B before/after (no extra allocation that window).

Current317statements/203876actualVM,35transactions,20writecommits,commit125580584ns,
95sealed bodies/20125893B. C2packreads0,8reservations/9publications. The bounded
queue mechanism reduces packs on deterministic count workload5->4; this sample
still has20writecommits. No controlled cross-window time delta, physical sync
count or all-seven speed claim. Nested spans overlap; VMsteps are executedcounts.

Raw issue302-packcarry1000-{baseline,candidate}-treatment1; comparison JSON
issue302-packcarry1000-comparison-treatment1. Commands runner.py run --case
phase7-sqlite-init-1000-v2 --arm baseline --baseline-root target/phase7-baseline/
layerfs --out <fresh-owned-output>, then --arm candidate; read
benchmark_agent_report.md before each. No unchanged-identity replay. Other three
Init and all three histories NOT_RUN at thisidentity; all required/earlier failures
remain visible. Source/harness/helper/dependency/build/resource/cache fields in
receipts. Scope remains full create/import/checkpoint/release/close product time.

Next: reduce unnecessary acknowledged allocation/publication exchanges within
existing explicit limits and qualify actual history driver/proof. Do not enlarge
buffers/workers/cache, weaken sync, move work outside timer or reinterpret target.
Goal active. CodeLOC137678->137677(-1),reference65417/core72261->72260,
active28217->28216/inactive44044,old191/new7807->7806/rest64263; exactsnapshot
counter incommit. Evidence-only followup+0. No CI/preflight/push/PR/merge.

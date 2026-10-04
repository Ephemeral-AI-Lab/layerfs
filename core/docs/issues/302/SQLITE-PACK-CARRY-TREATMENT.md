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

# Actual Store admission and scratch association

> **Status: Current planning checklist; no release candidate exists.**
> Implementation input: published `d43ebe556`, product parent `7a3a9b34c`.

SC-07/08. Existing Store constructors obtain one continuing native arbitration
authority from the C2 device/inode registry; clones retain that Arc and independently
opened handles to the same live file share it. Expose an equality predicate on this
unforgeable continuing authority. Server compares this authority directly rather
than paths, StoreAccess IDs, catalog IDs or reconstructed numeric native identities.
Persisted active_slot/writer-budget SQL remains cross-process capacity authority.
This association does not make independently retained derivation indexes identical.
Store path replacement/native identity guarantees remain their existing provider
scope; this checkpoint adds no durability, deployment quiescence or physical claim.

Service assembles one Arc SaveBudget and one Arc Construction per distinct actual
Store authority, with bounded aliases in the existing at-most-four StoreAccess
entries. Catalog deduplication continues to use its distinct actual catalog Arc.
Refusal precedes prepared body and scratch/Save creation. No whole-operation lock
is added and pure catalog/read allowances remain independently admitted.

Expected external proof holds two real prepared sources through different aliases
after observing persisted active_slots and the two actual scratch files. A third
alias request refuses before its unread body or additional scratch domain. Finish
the held requests with exact independent canonical roots and known cleanup.
Separately cloned/reopened/hardlink handles share the actual association; a distinct
Store with the same policy/ID-like scalar remains distinct. Independent processes
still rely on the unchanged persisted writer reservation. No byte-index/consumer,
engine/protected-native or global physical gate is inferred from counter equality.

Root exclusively owns cas/store.rs and Server admission/handler wiring, external
tests and architecture service description. Other owners preserve these files.
Checks remain UNRUN before this implementation freeze.

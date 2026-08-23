# G5-A reopen authority report

Disposition: **`RETAIN_FULL_REOPEN_AUTHENTICATION`**.

## Why

Closing/reopening destroys the same-open witness. The retained G4 implementation therefore authenticates the visible transition and reachable file graph before issuing a single-use permit. The 154.019-ms G4 first-edit-after-reopen control measures this authority work, not merely SQLite open time.

Xet's gap summaries and reconstruction hashes assume an immutable server-owned base. They can show that a supplied prefix/suffix composes with dirty data, but they cannot prove that a mutable native file did not change through an unmediated writer. Paths, inode numbers, mtimes, sizes, watcher state, cached receipts, and replayable digests are observations, not non-replayable authority.

## Minimum prerequisites for any future authority experiment

All of the following must exist before a fast path may be preregistered:

1. A real protection domain that mediates every mutation and binds file identity to content state.
2. Non-replayable, integrity-protected authority tied to store instance, validation authority, profile, integrity epoch, generation, root, transition, and writer epoch.
3. Writer fencing and revocation across process death, connection loss, external writers, rollback, downgrade, and copied/restored stores.
4. A crash/restart state machine whose durable ordering is data first, authority/publication last.
5. Fresh ambiguous-outcome reconciliation against durable visible state.
6. Exact hit, miss, stale, rebuild, revoke, and fallback counters; a miss must perform complete current authentication.
7. Hard bounds on receipts, descriptors, queues, indexes, caches, and rebuild work, with terminal cleanup and Q zero.
8. Fault cases for substitution, replay, stale receipt, rollback, corrupted authority, unmediated mutation, lost acknowledgement, and concurrent writer races.

Until these prerequisites are concrete, the smallest safe system is the current complete reopen authentication. No G5-A experiment or implementation is authorized by G5-1.

Explicitly rejected as authority: prior root/receipt alone; path; device/inode; mode/type/length; mtime/ctime/birth time; SQLite-local generation or change counter; watcher state without trusted gap-free mediation; clone lineage; native seed; cached validation after open loss; bearer token; Xet file/Merkle/gap/term/proof-of-possession hash; HMAC dedup metadata; successful prior materialization; wall time; process restart; and cooperative SQLite locking by itself. Each can authenticate or describe a replayed/stale state without establishing current freshness against raw mutation, image restore, downgrade, or an alternate writer.

H11 v2 is not an authority fast-path experiment. It shows that retained complete current+parent authentication has history-independent non-genesis work at N=10/100/1,000. It does not test rollback, restore, raw-writer mutation, watcher gaps, downgrade, or an external authority provider.

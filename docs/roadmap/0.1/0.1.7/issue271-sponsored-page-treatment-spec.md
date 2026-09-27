# Issue 271: bounded extent-page custody sharing trial

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before changing the owner ledger or taking a sample. Starting source
`1c9a672ba28851155edf184da45f7db86e35e261` retains the combined
100/512 baseline, the rejected 32/8 and 64/32 packing trials, and the
32-child 2,048/4,097 FAILs. No unchanged arm is retried.

## Ownership hypothesis and exact rule

An immutable copied extent page currently adds one owner-ledger reference for
every child or Local custody it names; cleanup releases each again. A new
page may instead hold one explicit reference to the old page it replaces and
charge only the multiset difference between their encoded edges. The old page
keeps all its edges until both its original root and the new sponsored page
release it. The new page's owner record names the sponsor and cleanup releases
exactly its charged difference plus that sponsor. A page already sponsored by
another page cannot sponsor again: the chain is at most one hop. A page with
fewer than two unchanged edges uses ordinary full ownership, so this rule
never adds a sponsor solely for a cosmetic shape change.

The sponsor is an authenticated, same-level extent page in the same arena.
Old 64-byte owner records have a null sponsor and remain readable. Each page
write still completes every acknowledged ledger edge before root publication.
An unfinished write records the sponsor and its edge-progress prefix so
cleanup releases only acknowledged references; uncertain ledger writes retain
the existing quarantine rather than guessing. G1/G2 retained old roots keep
their own page and byte identities. Quotas charge every additional retained
page until its final ref reaches zero and refund only after authenticated
cleanup. This is a custody rule, not a page-body or canonical Commit change.

## Frozen evidence selection

Reuse the sealed master, release SDK/verifier, one-process/one-fd positional
writer, writable byte-copy clone, one public Mount → Exec → Commit, cache
policy and worker count from the combined #271 baseline. Take one 100 and
one 512 separated-write row, in that order, at one frozen source. Capture
actual FUSE callbacks, page/ledger and child/Local/sponsor edge counts,
within-run progress, root height/occupancy, payloads, Store work, resource
charges and positive cleanup. An independent verifier checks full old/new
heads and bytes within 9 s. The ordinary complete-command limit remains 15 s.
Both ledger reads and writes must fall below the original combined baseline
2,905/1,511 at 100 and 20,953/12,150 at 512, with exact callbacks, full
oracles, retained-root/partial-failure proofs and clean close. Host/container
cache remains uncontrolled, so raw wall is INELIGIBLE even if these counts
improve. Keep every FAIL and INELIGIBLE receipt at a fresh path.

Only if those conditions hold, check the public 100-write
append/dispersed/repeated siblings and attempt one 4,097 public gate at the
frozen final source. Its previously declared 25 s complete-command exception,
under-10 s verifier and #249's separate product Exec timer do not change.
No gate is inferred from a lower count. A failure or incomplete callback,
Commit, quota, custody or cleanup proof is retained plainly.

# Issue 271: four-hop extent custody trial

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before a new source or sample. Starting source
`1eb0ebf23011d611c5d801052102744a42b0f595` retains the sole
one-hop 4,097 gate FAIL at 25.004969 s and the separate 2,048 diagnostic
FAIL at 15.004927 s. They are not retried or relabeled. The daemon's 2,048
checkpoints rose from 20,573/10,988 to 113,143/67,771 ledger reads/writes;
the four 512-write increments kept rising. One-hop sponsorship eliminated
many repeated edges but every second copied page still charges its full edge
list.

## Bounded change

Allow an authenticated copied extent page to sponsor a previous page whose
own sponsor depth is below four. The live page owner's otherwise unused
`length` field records depth 1..4 for role-1 pages; unsponsored pages have
depth zero. Older role-1 records with null sponsor remain valid, and an older
non-null sponsor with no depth marker takes the full-ownership path. Each
sponsorship charges one reference to its immediate old page plus only the
multiset difference in edges. The transitive chain is at most four pages.
Before publication each charged edge remains acknowledged in the ledger;
unknown updates still quarantine. Cleanup releases new edges before the
sponsor, and its fixed charged stack is enlarged to cover the bounded
tree-height-plus-sponsor depth. Account the extra stack bytes in the owner's
resident charge, and refund retained old pages only at their final zero ref.
No page body, authenticated identity, canonical Commit format, worker,
cache, timeout or generic shell rule changes.

## One-source decision

At a frozen source, run the native old-root/G1/G2, abandoned-candidate,
depth/fallback and exact quota-refund proof, then one 100 and one 512 public
separated selection with the same master, release driver/verifier, one
process/fd writer, Mount → Exec → Commit and full independent oracle. Count
actual callbacks, charged child/Local/sponsor edges, ledger 4 KiB I/O,
within-run slopes, live pages, resource charges and clean close. The same
15 s complete-command and 9 s verifier bounds apply. Both ledger reads and
writes must fall below the unchanged combined baseline 2,905/1,511 and
20,953/12,150 respectively, and both must improve against the one-hop
source's 2,866/1,446 and 20,573/10,988 to justify a new gate. Host/container
cache remains uncontrolled, so latency stays INELIGIBLE.

If that and the three public 100-write sibling patterns pass, attempt one
new-source 4,097 public gate under the original 25 s complete-command
exception and separate verifier. A timeout, missing callback, unverified
root or failed cleanup remains FAIL; no unchanged-source rerun follows.

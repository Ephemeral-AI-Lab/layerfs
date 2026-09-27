# Issue 271: moderate extent-leaf packing treatment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before this treatment, its runner changes, or new samples. Starting
source `3c3343e0b116f99b1e4abe1a3e9795fff28b3f79` retains the combined
100/512 baseline, the rejected 32-record/eight-child treatment, and the
32-child source's append-only 2,048 and 4,097 command-timeout failures.
Neither failed arm is eligible for an unchanged-source retry.

## One change and its count question

Pack newly written extent leaves to **64 records**, retaining the 32-child
branch target and the physical codec capacities of 124 records and 248 children.
Use the existing balanced touched-leaf fold and shared `RootOwner` publication;
older full pages remain readable and are repacked only when touched. This
reduces Local edges recharged per copied leaf, while adding leaf and branch
pages. It is a hypothesis, not a speed or ledger-I/O claim.

Use one independently verified old-root/custody test and the existing public
one Mount → one generic-shell Exec with one process/fd → one explicit Commit
writer and separate full old/new-head verifier. Compare one prospective 100
and one 512 separated-write selection at this frozen source with the retained
combined baseline: 2,905/1,511 and 20,953/12,150 ledger 4 KiB reads/writes.
Capture actual FUSE callbacks, extent leaf/branch writes, Local/child edge
adds/removes, live pages and root height. Keep the 100/512 raw wall times
`INELIGIBLE` under uncontrolled host/container cache. Counts support the
treatment only if **both** ledger reads and writes fall below baseline in
**both** selections, full oracles and cleanup pass, and all callbacks occur.
Retain any regression or failure as an append-only rejection.

If those conditions hold and native checks cover old 124-record leaves,
old 248-child branches, retained roots, G1/G2 custody, quotas and definite
failure cleanup, attempt one public 4,097 gate at the frozen final identity.
Keep its previously declared 25 s complete-command exception and the ordinary
15 s limit for 100/512; run the independent verifier within 9 s. No product
Exec timer, worker, workload, cache policy, fixture or progress interval changes.
The gate is `FAIL` on timeout or incomplete callback/Commit/cleanup evidence;
uncontrolled cache keeps latency `INELIGIBLE` even if its functional route
completes. Run the 100-write append/dispersed/repeated siblings at the final
source; retain all receipts, including non-passing lines, with exact seals,
commands and custody outcome.

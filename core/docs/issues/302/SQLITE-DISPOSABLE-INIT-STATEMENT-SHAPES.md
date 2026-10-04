# Bounded locator statement-shape treatment

> Status: prospective changed product treatment; no new speed result yet.

Based on ec4c12a66. The first supported Disposable Init campaign completed all
four sizes but10k missed the exact1.10x gate by2.7308122ms. Keep all its original
results. Existing10k query-wrapper prepare/cache-checkout wall is36.948095ms;
variable locator INSERT lengths can force distinct prepared statements. This is
an optimization hypothesis, not a proven missing-index or complete cost partition.
The separate close diagnostic still points to a late descriptor close; required
close remains in the complete lifecycle and is neither deferred nor subtracted.

Locator INSERTs now use ordered power-of-two row counts up to the existing
512-object batch bound and actual SQL/bind limits. All subpages share the same
physical/logical publication transaction and acknowledgement. No fake padding
rows, extra reservation, changed ID order, signature stamp change or conflict
policy is introduced. Returned inserted IDs still form the same set; lost IDs
still follow original caller order. A late SQL failure rolls back bodies and all
previous locator subpages. Canonical/physical4MiB-minus1 and8191-row publication
limits, page/cache/busy settings, profile durability, workers, queues and buffers
remain unchanged; no dependencies or third-party code are modified. The code
narrows temporary parameter/SQL demand, rather than growing statement cache.
The changed SQL subpage count can increase executed statements/VM setup work;
measure that tradeoff in a new matched campaign instead of assuming it is faster.

Covering checks:5 allocation-release,2 ordinal-lookahead,13 publication and9
transaction-unit tests, plus the selected-profile100/1000 full namespace oracle
(30 test functions total; namespace oracle includes both profiles/sizes). New
1025-row conflict and1537-row late-failure cases cover multiple subpages in both
profiles, one atomic commit and caller-order loss/rollback. All PASS. Owning
persistence/project all-target Clippy PASS; Core fmt applied; boundary441
production Rust/SQL files PASS. No aggregate/preflight/CI or full-workspace claim.

The tracked campaign_disposable_init.py declares and retains all four cases,
one sample per arm, through the sole runner.py. It pins its own orchestration
source and the matched harness/source identities before invocation; fresh outputs,
original prepared source reuse, locked release artifacts, native source cold
attestation, actual settings, separate mandatory proof, final storage/cleanup and
15s/9.5s limits remain. It does not turn a missing receipt/root into PASS or
select a best result. The next campaign uses the changed frozen source; old
passing rows and the failing10k arm are not relabeled or pooled with it.

All three history selections and Durable remain NOT_RUN. The immediate milestone
is all four supported Disposable Init joint gates, then the history selections;
Durable gap follows all-seven Disposable qualification at the same implementation.

## Campaign2 at76c066138

All four changed-source matched rows completed through the tracked sole-runner
orchestration, one declared sample per arm. Do not pool with campaign1.

|Files|Phase4.5 ms|Candidate ms|Ratio|Joint gate|Reference allocated B|Candidate allocated B|
|---:|---:|---:|---:|---|---:|---:|
|100|46.436000|45.053500|0.970227840|PASS|7,372,800|5,214,208|
|1,000|129.185750|137.608708|1.065200365|PASS|23,101,440|20,525,056|
|10,000|1633.508334|1822.394458|1.115632177|FAIL|319,848,448|305,029,120|
|100,000|5604.959333|5445.364958|0.971526221|PASS|518,029,312|514,916,352|

All eight cold0/root/independent sampled proof/storage/cleanup/budget checks PASS.
The full family remains unqualified:10k exact time miss25.5352906ms (not PASS).
Candidate10k Init1587.272708ms/close231.007667ms/checkpoint0.243000ms. Native
prepare/cache-checkout25.107640ms,5177statements/2,526,232VM steps/989transactions/
133write commits.100k10587statements/14,769,582VM/1015transactions/430write commits,
prepare50.627710ms. More statements and different read/packing schedules are
visible; no causal cross-window speed claim or unchanged resampling is allowed.
Raw identity/binary/settings/fixtures/cold/proof/limits in receipts; compact copies
omit large DBs, whose original raw manifests remain authoritative.

Owner now prioritizes exact allocation handle teardown attribution and adapting
Phase4.5's temporary allocation lifecycle, before another unrelated SQL change.
The statement-shape implementation and all its evidence stay intact. Next cause
instrument records actual allocation descriptor/device/inode and separates
F_TRANSFEREXTENTS/scratch close from source close. A synthetic current-runtime
external writer probe stays SQLITE_BUSY after an extra writable fd closes while
another connection holds BEGIN IMMEDIATE. This is a scoped capability check,
not universal locking proof or a speed arm. No timing work is moved outside the
complete operation; any mere relocation of close cost is reported as such.

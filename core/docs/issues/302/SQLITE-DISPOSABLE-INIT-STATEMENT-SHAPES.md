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

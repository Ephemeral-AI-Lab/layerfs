# Live composition and garbage maintenance

> **Status:** Current general guide. Initial S6 composition slice after S5
> `a0dc7da9b`; S6 remains IN PROGRESS, not acceptance or release qualification.

Schema v9 adds a single `consolidating` generation and indexed backed maintenance
items. [Failure resolution](../../crates/layerfs-overlay/src/lifetime/composition.rs)
requires the caller to establish definite nonpublication and fence external work.
Unknown history retains the original capture. Resolution changes six statements
of ownership metadata, with no payload scan/copy. A next capture is not ready
until this one lower namespace domain is composed; ordinary writes remain live.

[Maintenance](../../crates/layerfs-overlay/src/maintenance/ready.rs) rotates ready
`(ns,kind,resource,target)` keys through a fixed cursor. A fold turn transfers one
name or effective cell into active state, preserving newer finals. Current upper
cutoff/epoch/staleness are re-read within each transaction; a discarded lower cell
cannot overwrite newer valid data. Final cutoff composition and source-inode
deletion are atomic after the cells finish. Absolute `born`, directory counts and
later attributes stay correct. Names use the same active-plus-lower bounded view
during composition. Reader leases on the exact generation park that target;
their exact release makes it ready without an owner sweep.

[Garbage jobs](../../crates/layerfs-overlay/src/maintenance/garbage.rs) visit at most 14 cells
or 64 small metadata/step records per turn. Retired domains use generation indexes
and keyset cursors. Stale-cell work starts at the shrink boundary, and rechecks
the current epoch/staircase before deleting. Rewrites after regrow survive.
Abandoned steps seek `depth > height`; released operation scratch uses its own
keyset/byte window. These jobs do not shrink the shared SQLite file. Page/freelist
observations remain physical shared allocation, distinct from declared data bytes.

The daemon alternates live and terminal maintenance after at most eight ordinary
owner jobs and continues while idle. A capture waiting for composition permits
later mutation jobs; its finite reply/publication fence begins only when that
readiness condition ends. Maintenance progress wakes parked capture jobs.
One automatic maintenance failure is retained and stops that service without
replay. Global immutable history/objects are outside local maintenance.

Owner diagnostics attribute `sql_foreground` and `sql_maintenance` from exclusive
job snapshots, recorded before completed replies are exposed. Raw connection
snapshots include background jobs and cannot be used as exclusive foreground
profiles. Read-only `MaintenanceIdle` observes the live work queue; terminal
cleanup retains its separate `CleanupState`. Count comparisons start after prior
cleanup completes and separately report automatic work, without latency/cache
claims. Two connection-local readiness hints avoid probing empty SQL queues
after every request. Exact empty-ready queries clear them; every enqueue or
owner wake marks possible work. Rolled-back enqueues may cause one false-positive
probe but cannot hide accepted work or authorize deletion. No namespace mirror
or growing resident maintenance roster is introduced.

External proofs in [S6 checks](../issues/307/checks/s6-lifetimes/) preserve exact
failed builds and repairs. Twenty-four failed-capture rounds with interleaved
shrink/regrow/writes preserve bytes and reach the next capture without adding a
generation chain. Reader custody blocks transfer until exact release. Stale
cleanup of a 4 MiB file removes 4,186,112 data bytes in 76 bounded turns while
preserving newly written bytes; page count stays 1,189 and freelist rises to
1,163. These are work/resource diagnostics, not throughput or residency gates.

The following checkpoint implements [independent orphan, file/captured-reader and
operation custody](33-independent-custody.md), including live removed-payload
cleanup. The subsequent [name/non-file checkpoint](34-name-and-lookup-custody.md) supplies
exact whiteout facts and lookup/symlink custody. Unfinished S6 exits: physical reservations,
cleanup headroom and actual device ENOSPC; comprehensive generation/owner/debt and
EXPLAIN/runtime profiles. The current raw generic lease contract remains a trusted
primitive. Integrated Commit, kernel references/output and transport cancellation
remain later milestones. This initial slice must not tick S6's checkbox.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.

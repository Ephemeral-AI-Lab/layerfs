# S6 failure ledger

> **Status:** Append-only diagnostic evidence. S6 remains in progress.

`initial-check.log` fails compilation because the first Command patch matched
the ServiceClass Capture variant instead. The typed command is moved to Command;
the existing six service classes stay unchanged. `owner-check.log` passes.

`layerfs-workspace-foundation-final.log` and the corresponding Linux foundation
log fail the existing real-owner exact-profile comparison. The raw connection
snapshot now includes variable automatic maintenance, and the prior operation's
cleanup state differs when the next read begins. This is a diagnostic attribution
failure, not a loss of bytes. Owner diagnostics now keep exclusive foreground
and automatic-maintenance aggregates. The proof waits for prior cleanup before
starting each count comparison and reports maintenance independently.

`profile-repaired.log` passes those counters but reveals 281,690 maintenance
statements beside 4,096 unrelated files, predominantly empty-queue probes. The
owner's service counter was not reset after a no-work turn; synchronous callers
also caused idle probes after each response. Source diagnosis led to two fixed
connection-local readiness hints. Every accepted enqueue/wake marks work; exact
empty-ready observation clears the hint. A rolled-back enqueue can produce only
an extra empty probe. `profile-ready-repaired.log` passes the same foreground
counts and reports 180 maintenance statements/57 changed rows/5,487 VM steps
for the original three-scale fixture. This is a count diagnostic, not a latency
comparison, cache-cold result or rewritten receipt.

All revised overlay/Workspace/daemon package checks pass on macOS and Linux
under explicit 120-second deadlines after --no-run builds. Earlier passing
source pins and failed receipts remain retained. No test reaches the ceiling.

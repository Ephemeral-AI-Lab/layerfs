# Native lookup component failure analysis

Build04/05 fails only because the new external engine test used the Daemon JobSql
family() accessor on Overlay DatabaseWork. DatabaseWork exposes its fixed public
statements array; the test now uses that owning API. No product change was needed.

Selection07/08 passes native lookup, native read plan, engine and lifetime cases.
The existing source plan test fails after adding source kind2. Raw EXPLAIN shows
OpenEphemeral2 plus Rewind2 for the constant CHECK(kind IN(0,1,2)); persistent
source lookup/delete plans remain indexed. Replace this contiguous integer enum
check with CHECK(kind BETWEEN0 AND2), preserving the exact STRICT-table grammar
while avoiding the new ephemeral set and restoring the hot-source VM invariant.
The original failed receipt is retained. No test reached100s.

Cargo per-package test commands selected narrower feature graphs than the initial
combined all-target --no-run, so some07 complete commands include compilation.
Their functional results remain at their exact binary/source scope; no elapsed
value is promoted as pure runtime/performance evidence. Final selections execute
the exact test binaries emitted by the combined --no-run build, under explicit
100s limits, eliminating compilation from their runtime boundaries.

Source review additionally retains a minted read candidate before native_read
association insertion can fail; only a successful original transaction result
establishes a usable owner. Existing non-native FileRead release reuses its current
owner-validation seek to identify the positive/negative request domain; it does
not pay an extra native association DELETE. Native releases delete only their
exact association. No schema migration/reopen path or failure replay is added.

Selection14/15 passes the new component cases and the existing Overlay/Workspace
checks, then fails the Daemon owner schema assertion: actual17, expected16.
The selected schema17 adds the native ownership tables; the old assertion had
not been advanced with the Overlay startup assertions. Update only that current
test expectation and run its repaired selection plus the two unrun Daemon checks.
No product change is needed and the earlier passing component evidence remains
valid. Preserve the original failure; it did not reach its100s limit.

Selection17/18 repairs owner and passes job_cost, then the previously unrun
completion_ownership saturation test observes one remaining2000-byte credit
immediately after dropping all returned results. Source establishes that
Publisher::publish publishes DONE before Cell::settle returns and drops the
publisher's original Arc. That Arc correctly retains the credit during wakeup;
the immediate zero assertion races its release. No credit leak or product
correction is established. The existing completion_future tests already use a
bounded diagnostic observation for this boundary. Apply the same5s bounded
observation to released-credit checkpoints in completion_ownership, including
between its two charge samples, with no new SQL job, replay or product polling.
Exact held-slot/byte assertions remain unchanged. The failure receipt is retained.

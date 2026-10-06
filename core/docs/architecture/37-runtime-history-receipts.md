# Bound host history attempts and retained receipts

> **Status:** Current general guide.
> S9 checkpoint after the S6 stopping boundary; complete runtime admission remains open.

The active SDK's [history handler](../../crates/layerfs-api/sdk/src/runtime/handlers/history.rs)
uses the existing initialized HistoryProvider and Storage owners. It extends the
[serving-scope Save registry](22-sdk-runtime.md), preserving its public entry points.
The new real handler group follows the owner-selected S7–S13 organization.

Binding now retains the root inode serial from the authenticated filesystem root.
It is acquired with the existing bind/root demand, without scanning any descendants.
Every history entry validates the exact runtime/catalog incarnation, authenticated
peer, Workspace/Branch authority and Save capability again. Foreign bindings or
Save IDs fail before history work. Revoked authority is not bypassed by a receipt.

`stage_saved` requires a retained successful SaveFinish. It derives all expected
head/base/root, Branch, Workspace, scope/profile and intended base fields from the
bound coherent snapshot. The caller supplies candidate root and generation. One
saved demand authenticates the candidate root, decodes its grammar and checks its
scope/profile; the reply decoder retains only the fixed root value. No duplicate
canonical payload buffer is added. This checks saved root admission, not complete
contextual topology or faithful native import; those obligations remain explicit.

`commit_saved` uses only the exact acknowledged stage/token. It stores either the
original typed CommitStagedOutcome (Committed or UpToDate) or HistoryError, including
the stage disposition from the deciding transaction. It never refreshes the Branch,
re-stages, retries, guesses install or implicitly discards a conflict. `discard_saved`
is a separate explicit exact-token attempt. Unknown transition/discard cannot be
replayed or discharged by an unfenced provider read.

HistoryReceipts remain with the finished Save slot. Receipt access inspects prior
knowledge without invoking history again. Active/unknown stage custody prevents
slot release. A known successful transition or discard, or the deciding transaction's
known absent stage, permits local receipt acknowledgement. A failed transition
with retained stage requires its explicit known exact-token disposition. Slots
are bounded configured live-session admission, independent of file/Commit totals;
no provider lock spans a whole Save. Duplicate local stage owners for the same
Workspace are refused. A new caller attempt uses a new capability; no automatic
operation replay exists.

Public real-host proofs cover two interleaved candidates, a same-Branch winner
and exact retained conflict, UpToDate, explicit discard, stale/cross-peer IDs,
authority denial, wrong object role and stage-before-finish refusal. They exercise
native KK authentication for binding and the real macOS Store/catalog. They do not
supply logical RPC framing, transport delivery/restart fences, complete native
acquisition or an unknown-history resolver. The subsequent
[typed service](38-authenticated-runtime-service.md) adds bounded fair local
dispatch and local attachment/result fences. The remaining obligations are
[S9 prerequisites](../issues/307/S9-EXIT-AUDIT.md); S10 Commit/install integration
and P3/P6/P7/P13/P14 are outside this batch.

## Bounded candidate context continuation

The continuation after `a41d131f262c926015798d05096ebeaa01c70cdc` replaces the
root-envelope-only admission described above with the same authorized root reader
used at bind. Before `HistoryCatalog::stage_changes`, the candidate must match
the captured scope, profile and root serial, and its actual root inode must be a
zero-reference Directory with a valid directory root page and portable mode/mtime.
Root serial mismatch is rejected before inode-table demand. Authority precedes
each canonical acquisition and the first original provider/authority error is
retained in the one stage attempt. There is no whole-root traversal or implicit
re-stage. Exact saved reference closure and full topology/provenance are separate
R2/K2 obligations; this bounded check does not assert them.

# Atomic saved-candidate publication

> **Status:** Implemented source boundary after `e2f2e62c7`; Store Save and daemon
> local install are separate operations and still need their integrated proof.

HistoryCatalog::stage_and_commit takes the existing complete StageRequest and
returns CommitStagedOutcome. Persistence performs exactly one write transaction:
validate context, insert the temporary stage with its token, validate frozen
Branch expectations, insert/verify the immutable Commit if changed, compare and
advance the Branch, and delete that exact stage. UpToDate removes the temporary
stage without creating a Commit. All existing validation and canonical identity
rules are reused through the original staging/commit implementations.

BEGIN IMMEDIATE is attempted once. Busy produces no stage or counter change.
A definite HeadMoved returns exact expected/actual head and base and rolls back
the temporary stage/token. Other definite failures also roll back this attempt;
a stage that predates it is never removed. No second discard transaction is
needed for this new route. The older independently acknowledged stage/transition
methods retain their original token/discard semantics and historical receipts.

UnknownOutcome remains terminal, with shared provider quarantine. The temporary
stage was never separately acknowledged, so this method does not attach an
invented retained/acknowledged stage observation. Its caller must keep the exact
StageRequest and candidate/capture custody and may not infer a final state from
an unfenced reread. The method contains no retry, reconnect or guessed cleanup.

C5 still does not inspect content. Save::finish and bounded root checks must
establish a saved candidate before this call; no transaction spans construction
or Save. A known history result still precedes a separate local overlay install.
The [atomic proof](../issues/307/PRE-S8-F8-HISTORY-20261007.md) counts transactions,
retains exact conflicts, exercises a second-process writer and verifies stage
and token effects. It is history evidence, not a complete live Commit claim.

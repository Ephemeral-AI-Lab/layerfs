# Atomic saved-candidate publication

> **Status:** Implemented source boundary, overwrite supersession after `bd5ab61d6`;
> Store Save and daemon
> local install are separate operations; their integrated Store-half proof is
> [recorded separately](65-store-commit-composition.md).

HistoryCatalog::stage_and_commit takes the existing complete StageRequest and
returns CommitStagedOutcome. Persistence performs exactly one write transaction:
validate context, insert the temporary stage with its token, validate captured
Branch provenance/base, insert/verify the immutable Commit if changed, overwrite
the Branch head, and delete that exact stage. The Commit parent is the captured
head. A concurrent head advance does not refuse publication or rebase the
candidate. UpToDate compares against the current root inside that transaction
and removes the temporary stage without creating a Commit. An unchanged captured
candidate still overwrites a different current root. All existing validation and canonical identity
rules are reused through the original staging/commit implementations.

BEGIN IMMEDIATE is attempted once. Busy produces no stage or counter change.
A definite validation refusal rolls back the temporary stage/token.
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
retains the historical conditional-policy outcomes. The current
[overwrite decision and proof](../issues/307/BRANCH-OVERWRITE-DECISION-20261007.md)
covers ordered publications, captured provenance, current-root UpToDate and
second-process Busy. This is history evidence, not a complete live Commit claim.

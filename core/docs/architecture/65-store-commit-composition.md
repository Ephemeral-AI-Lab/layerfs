# In-process Store Commit and local install

> **Status:** Implemented Store-half composition after `01e60021a`. S10 live
> namespace normalization and S8 FUSE/Exec are separate work.

BoundWorkspace::commit admits one active lifecycle for that Workspace, captures
once through the existing overlay owner, creates independently owned Storage/Save
state and invokes one synchronous Content producer with the exact capture and
frozen BranchSnapshot. The producer returns a canonical root or an original
CommitError; it must construct the capture faithfully. There is no hidden native
Init, automatic Branch refresh, whole-Commit mutex or repeated attempt.

Save finishes before the adapter performs bounded candidate scope/profile/root
inode/directory/metadata checks and prepares the paired local base transition.
It then calls HistoryCatalog::stage_and_commit once. The exact known outcome
supplies the next head/root without a reread. One original InstallPrepared owner
job advances the shared Workspace base and preserves later active mutations.
The local Branch metadata is replaced only after known install. Ordinary reads
and other Workspaces retain independent sources and read capacity throughout.

CommitSuccess carries the exact history outcome, Save outcome and per-producer
Storage diagnostics plus original capture/install Completions. The counts are
Store calls/transactions, not a whole-operation timing/resource report; F14 still
owns complete accounting. A changed Commit or UpToDate with the selected small
input uses one initial reservation, one publication batch and one history write.
Required block refills remain separately counted for larger Saves.

CommitFailure retains phase, Workspace/route and original binding, capture and
its Completion, StageRequest/candidate when known, Save diagnostics/outcome,
prepared root, known publication, and any attempted local completion/error.
The first original Storage error from same-Save Content reads now survives both
take_failure and finish, matching output-sink custody. A public producer can
return typed Storage, Workspace, History or owner failures; opaque provider
uncertainty is retained conservatively rather than called definite.

After definite nonpublication, one ResolveFailed job preserves the composed live
view and starts existing bounded local consolidation. That is not Store cleanup
or a history retry. The Commit slot releases only on a known safe disposition.
Unknown outcome performs no resolution, install, resend or inferred discard.
Known publication with a failed/unattempted local install is explicitly still
published and blocks another Commit. A later observer read does not settle an
original unknown. These retained results remain the caller's ownership; restart
custody and an unknown resolver are not introduced.

The [Store-half proof](../issues/307/PRE-S8-F8-COMPOSITION-20261007.md) first writes
real overlay bytes, then directly constructs the matching captured change with
public Content APIs. It checks Committed/UpToDate, exact HeadMoved, missing
references, original read failures, real process-held writer Busy before Save and
at history publication, known publication/local install refusal, and later writes
through successive installs. The acknowledgement-loss case is an external public
port adapter over a real committed Store; it qualifies caller custody only,
not a native SQLite I/O failure or quarantine mechanism. A new Workspace reads
the exact published root. This does not implement or qualify S10's normalizer.

The shared Storage unknown-outcome classifier also examines both causes of a
CleanupFailed wrapper. Original unknowns supplied by a Content producer must
retain capture even when nested, while a definite wrapper still resolves once.
The real Commit regression and its prior failure are retained in the
[nested uncertainty checkpoint](../issues/307/PRE-S8-NESTED-UNKNOWN-20261007.md).

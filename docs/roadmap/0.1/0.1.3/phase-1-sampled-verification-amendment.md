> **Superseded: Phase 1 verification has been withdrawn by the user.** See [closure decision](phase-1-verification-withdrawal.md) and [final performance report](phase-1-final-results.md). Earlier instructions and pending verification queues below are historical.

# Bounded sampled verification: latest Phase 1 acceptance

On 2026-09-05 the user explicitly required at most ten minutes of total
verification across all families, at most one minute per family, and stated:
“you do not have to read the full files, just some random sampling is enough”.
This supersedes the remaining exhaustive per-file and per-case routine fast
queue. It does not alter the completed performance matrix or any actual outcome.

Retain all 370 qualified active performance observations, all 29 targeted proofs,
48 routine full proofs, and completed qualified fast checks at their actual
source/input/environment identities. Do not replay satisfied coverage. The
remaining families receive bounded representative sampled verification, with
case selection, seed, files, byte ranges, metadata, aliases and sampled absences
recorded explicitly. Unselected cases, seeds, files, bytes and historical
snapshots remain unverified or deferred to Phase 2; they are not passing proofs.

Sample selection uses reproducible hash ranking and offsets, including boundary
bytes and size representatives. Expected bytes come from the independent recipe;
actual output cannot define the oracle. Existing canonical and public native
routes compare selected ranges and metadata. For a sampled history case, retain
exact Commit membership and parent topology checks, and inspect genesis, middle
and final snapshots. Other snapshots receive no sampled-content claim.

The existing case machinery enforces resources and cleanup. No expected-error
oracle, fixture size, product workload, resource ceiling, or previously satisfied
targeted gate is relaxed. The incomplete exponential-history-oracle attempt
remains interrupted/nonpassing. Its byte-equivalent recipe construction repair
is verified with a small deterministic regression before the sampled replay.

New assurance is `sampled_iteration_verified`, never `fully_verified` or a claim
of exhaustive fast coverage. Family acceptance joins all required performance
rows to the already accepted proofs and the bounded representative sample. The
report lists the remaining routine inventory as deferred, not as completed
individual verification slots. Families already fully accepted are not rerun.

A per-family wall budget of 60 seconds includes command setup, selected-input
preparation, workload replay, checking and cleanup. A 600-second total wall budget
limits the new campaign. Exceeding either budget remains incomplete/failed and
must be reported; it cannot be relabeled passing. Measure the first whole command
before dispatching the rest. Keep central issue #21 open.

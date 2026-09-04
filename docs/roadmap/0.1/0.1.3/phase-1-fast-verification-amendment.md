# Phase 1 fast-verification acceptance amendment

On 2026-09-04 the user explicitly requested: “i want to reduce the verification
time use fast path is enough”. This changes Phase 1 verification acceptance,
not merely execution order. It supersedes the earlier requirement that fast
verification have no Phase 1 acceptance credit.

Qualified fast verification is sufficient for routine Phase 1 benchmark
verification. Stop scheduling exhaustive per-case proofs solely to fill the
old full-verification inventory. Preserve completed exhaustive proofs and use
their compatible input certificates. Defer the remaining exhaustive coverage
to Phase 2 without deleting its definitions, implementation or evidence.

Fast verification must still use the independent expected-state oracle, check
the affected content, metadata, aliases and expected absences, authenticate
its reused evidence, and exercise the declared deterministic witnesses. Reuse
qualified inputs and certificates; do not repeat a full proof when its existing
certificate is compatible. Missing fast-profile coverage must be implemented
or reported explicitly, never represented by a fabricated passing receipt.

Keep exact expected-error oracles and the required targeted reliability,
resource and cleanup checks. An observed correctness or cleanup failure still
requires investigation and focused repair confirmation. This amendment does
not turn an actual failure into a pass, excuse the known cancellation cleanup
failure, change fixture sizes or public operation routes, raise resource caps,
or require another run of the already-qualified 600-second proof.

Update the Phase 1 report, terminal evaluator and issue acceptance accounting
to distinguish completed exhaustive proofs, accepted fast verification,
exhaustive checks deferred to Phase 2, and unresolved failures. Fast evidence
keeps its actual assurance label and explicit skipped coverage; it must never
be labelled `fully_verified`. Missing exhaustive checks alone no longer block
Phase 1 terminal acceptance when the corresponding qualified fast checks and
remaining applicable gates pass.

Reuse valid performance measurements at their original source and environment
identities. Batch compatibility documentation and publication at useful
checkpoints. Reconcile and publish this amended scope before closing remaining
children; keep central issue #21 open for later phases and release.

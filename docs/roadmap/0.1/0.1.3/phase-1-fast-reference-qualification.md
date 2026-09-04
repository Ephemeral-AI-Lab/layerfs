# Qualified references for Phase 1 fast verification

This implements the user's [fast-acceptance amendment](phase-1-fast-verification-amendment.md).
It changes verification assurance and scheduling only. Fixed scenarios, seeds,
fixtures, public routes, expected errors and resource limits remain unchanged.
The performance stream remains free of benchmark verification work.

A completed, compatible full proof is accepted directly and is never rerun to
produce a fast receipt. For remaining routine slots, the existing fast verifier
is extended using each family's independent fixture and expected-state recipes.
The affected set includes additions, changed content or metadata, expected
absences, affected ancestors and hard-link closure. Structural recipe inequality
is treated conservatively as a change; a product dirty set is not an oracle.
Every affected content body remains checked, together with the declared
deterministic size/depth witnesses. Entirely new outputs may therefore still be
expensive: the fast profile does not promise a fixed speedup for every case.

## Reference assurance

Use an already qualified full-input certificate where it matches exactly. A
full proof of a larger or modified tree may certify unchanged content components
only through an explicit independent path/content-recipe mapping and the sealed
file-root evidence. Such reuse is not a claim that the whole input root had a
full proof. A prepared Store's file hash alone is custody, not initializer
correctness, and cannot serve as a correctness certificate.

For a distinct input that lacks a suitable existing certificate, qualify its
canonical state once using the existing independent canonical verifier and
expected fixture. Run this against an independent clone of the pristine input;
preserve the producing source, exact input/metadata/format identities, canonical
root, verified file roots, oracle identity and sealed artifacts. Reuse the result
for compatible cases and seeds only when those identities match. Its assurance
is `canonical_input_qualified`: it does not establish exhaustive FUSE readback.
Do not repeat the expensive canonical input qualification per sample.

For outputs whose content is entirely affected, verify that content directly;
do not fabricate a reused-base certificate. Final fast checks still authenticate
the current namespace/global inode table and check metadata, aliases, affected
content and deterministic witnesses through the existing canonical and native
paths. Report the selected paths/bytes and the skipped content and native checks.
Fast results are not labelled `fully_verified`.

## Family-specific scope and accounting

History retains exact Commit membership, parent topology and a check of every
prescribed snapshot against its independent cumulative expected state. Dedup
families retain applicable extent/reference and transcript comparisons. Unrun
exhaustive object-union and storage-census checks are explicitly deferred to
Phase 2; missing values are not manufactured or presented as verified storage
claims. Measured physical Store deltas and raw counters retain their actual
performance sources and observation limits.

The active routine inventory is 353 verification slots: 348 new-family seeded
slots and five capped replacements. At the amendment checkpoint, 48 routine
slots already have accepted full proofs, leaving 305 routine fast slots. Their
305 unrun exhaustive counterparts are deferred, not passing full proofs and not
Phase 1 blockers once qualified fast checks pass. The 29 targeted slots remain:
the CDC boundary proof and all 28 reliability subcases. Fourteen targeted slots
are qualified at this checkpoint; the other fifteen still require their exact
checks, including cancellation and disconnect cleanup. The existing 600-second
proof is retained. Counts must be derived from the sealed inventory at each
subsequent checkpoint, not copied forward as unchanging totals.

Qualify the added selection/reference logic with small negative checks and one
relevant integration before broader fast collection. Preserve all actual
failures and incomplete attempts. The final report must distinguish accepted
full proofs, accepted fast proofs, qualified canonical references, Phase 2
exhaustive deferrals and unresolved mandatory failures. Central #21 stays open.

# Verification profiles

> **Acceptance amended 2026-09-04:** The user's subsequent explicit request
> makes qualified fast verification sufficient for routine Phase 1 checks.
> Apply the [fast-verification amendment](phase-1-fast-verification-amendment.md)
> instead of the earlier development-only/no-acceptance-credit restrictions
> below. Preserve actual assurance and coverage; defer unrun exhaustive checks
> to Phase 2 and retain targeted failure, resource and cleanup gates.

The user requested approximately ten times faster verification for development
iteration after the fixed 100,000-file background made each tiny-file proof take
about three minutes. This authorizes a separate fast profile and removal of
redundant verifier work. It does **not** amend the exhaustive Phase 1 or release
acceptance requirements.

## Exhaustive verification

The existing `verify` profile retains its full independent canonical Store and
fresh FUSE readback checks, including unchanged paths, bytes, metadata, links,
expected errors, resources and cleanup. Successful evidence is
`fully_verified`. Keep existing passing evidence at its actual producing source;
verifier-only changes do not automatically invalidate it or performance data.

Batching authenticated reads, reusing independently generated expected data,
and eliminating duplicate work inside one immutable verification are allowed
when they preserve every required check. Those implementation changes must have
explicit source compatibility and focused qualification.

## Fast iteration

The separate `fast-verify` profile is development assurance, labelled
`fast_iteration_verified`. Its initial target is a representative tiny-file
case in approximately 18–20 seconds instead of about 180 seconds. This is an
unmeasured target until a qualified comparison is recorded.

The fast profile must:

- Derive changed, created and deleted paths, affected metadata and alias
  relationships from the independent input and expected-state oracle.
- Check changed content and expected absences after fresh Store reconnect.
- Validate namespace membership and metadata within its explicitly reported
  coverage, without substituting product dirty counters for the oracle.
- Bind reused immutable evidence to a qualified certificate with matching
  fixture, oracle, source and relevant state assumptions. Missing or
  incompatible certificates fail closed with a reason that a full check is
  required.
- Exercise deterministic untouched FUSE witnesses across the declared width,
  depth and file-size boundaries.
- Record the certificate identities, actual checks, reused evidence, omitted
  reads and limitations. Required error oracles, resource caps and cleanup
  remain enforced.

A matching root or subtree identifier is a reference to certified content; it
does not prove that every current stored byte was reread or that every pathname
works through a new FUSE mount. A directory subtree alone also does not cover
the global inode table. Witness readback is not exhaustive FUSE readback. These
limitations must remain visible in raw receipts and reports.

Fast results occupy separate evidence slots and cannot replace, overwrite or
silently satisfy required exhaustive verification. Changing that acceptance
scope requires a further explicit user amendment.

## Qualification and rollout

Use one small negative-check campaign covering wrong changed bytes, unexpected
deletion or extra paths, metadata or alias mismatch, and certificate mismatch.
Then run one representative fast tiny-file case with existing qualified inputs
and compare it with its retained exhaustive observation. Preserve every actual
failure and incomplete attempt. Report the measured gain and remaining blind
spots before broader execution; do not repeatedly rerun full proofs or
recollect performance merely because verifier code changed.

The exhaustive campaign was paused after its current sample completed, leaving
37 new full checks and two older full proofs retained. At that checkpoint,
343 further full checks remained under the existing Phase 1 contract. The
separate 373 admitted performance samples remain retained, and the fourteen
runtime-suppressed scenarios remain implemented for Phase 2.

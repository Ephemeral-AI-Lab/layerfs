# Phase 4.5 hot publication evidence, 2026-09-28

These are functional/count/custody records, not performance samples. Frozen
proof source `4ae36ad3a`, last product change `a2359620a`. Checkpoint 5 is
**NOT_RUN**, pending owner review. The [phase log](../../../PHASE4.5-LOG.md)
records implementation, exact observations, failures, qualification and LOC;
the [frozen handoff](../../../HANDOFF-PHASE45-FROZEN.md) carries the next step.

- [FROZEN-CANDIDATE.json](FROZEN-CANDIDATE.json): source, product/test/harness,
  release binary, runtime, build-input and closed-fixture identities. Local
  SHA-addressed executable copies are declared there, with no cache/speed claim.
- [RESULTS.json](RESULTS.json): every retained public attempt, including failures
  and separate manual cleanup. Original receipts were never overwritten.
- [ARCHIVE-MANIFEST.json](ARCHIVE-MANIFEST.json): raw artifact origins, byte
  counts and SHA-256; regular byte copies, excluding Store/history databases,
  input payloads and generated service data.
- [phase45-route-4ae36ad3a/](phase45-route-4ae36ad3a/): the final registered
  public functional set, with one source/harness/test executable identity.
- [phase45-core-transfer-20260928-1/](phase45-core-transfer-20260928-1/): final
  Linux backing/readable/tree/attachment/ownership/pieces checks. Its staged
  product blobs match `a2359620a`; the dirty-source identity is preserved,
  not retroactively changed into a clean measurement seal.
- [phase45-transfer-workspace-checks-20260928-1/](phase45-transfer-workspace-checks-20260928-1/):
  Host release Workspace tests, release Clippy/all targets, fmt, boundary and
  guard self-tests, native binaries and locked ARMv8 Linux test build.
- [phase45-custody-fixture-checks-20260928-1/](phase45-custody-fixture-checks-20260928-1/)
  and [attempt 2](phase45-custody-fixture-checks-20260928-2/): retained FileExt
  compile failure and its scoped repair check; no product source changed.
- [phase45-commits-loc.json](phase45-commits-loc.json): exact first-parent versus
  committed production LOC for every implementation/proof commit; identical
  full production-input blobs share the exact counter result.

The earlier `phase45-core-check-*`, `phase45-core-epoch-*` and public source
folders retain the original failures and diagnostics. `cleanup.json` or
`manual-cleanup.json` records later removal of only owned resources without
changing a failed receipt. Successful environment removal is not a product
clean-close claim; the failed-cleanup case deliberately keeps charged custody.
The first Clippy argument-count diagnostic was recorded in the session/log,
not captured as a raw command file; no raw artifact is invented for it.

Verify this archive with `shasum -a 256 -c SHA256SUMS` from this directory.
No original checkpoint-4 or phase-4.5.1 receipt/checksum file was changed.

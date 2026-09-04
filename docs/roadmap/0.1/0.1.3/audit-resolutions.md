# Phase 1 audit corrections and evidence reuse

> **Subsequent amendment:** Required product repairs supersede the nine-slot-only recollection plan below. See [functional repair status](functional-repair-status.md): all 84 original runtime rows remain baselines and need corrected-candidate collection because the combined repairs alter their paths or memory layout.

This records harness corrections before expanding beyond the first two performance
families. It does not declare Phase 1 complete. Product optimization remains in
Phase 2; all actual failures remain retained.

The audited execution source was `4c207c70f3282c316d5ab18d832504085835eda3`.
Its 84 initial outcomes comprise 48 raw passes, 27 failed Commits and nine failed
Execs. The new producing source, binaries and runtime image are bound by the
next sealed build and the campaign's `evidence-builds.json` selection record.
Old outcomes retain their own source, image, input and environment identities.

## Corrected harness behavior

- Failed outcomes receive authenticity, timing purity, reached-operation,
  resource and recovery validation. Failure classification never bypasses those
  checks. Later Git custody artifacts are required only when their phases ran.
  Retained native failure output is decoded without changing the original bytes.
  Iterator errors have explicitly narrower wrapper counters; actual FUSE
  callback observations remain mandatory.
- The runner acquires its measurement lock before writing an invocation,
  finalizes returned/interrupted invocation state, and recovers complete sealed
  orphan attempts before scheduling. Partial attempts remain retained and cannot
  become passing evidence. A hard interruption's unknown command wall remains
  unknown. The small recovery check covers a real competing flock, orphan
  recovery, partial rejection and interruption finalization.
- Canonical manifests, file roots and extents stream losslessly into gzip
  artifacts. All snapshots and decompressed rows remain required; the 64 MiB
  retained-output cap is unchanged. The format model round-trips all three
  artifact types. Actual large-history resource qualification is still pending.
- Metadata history now performs its specified one-file chmod and sync. It no
  longer normalizes unrelated directories. Native dedup failures retain actual
  acknowledged writes and partial operation/identity/purity receipts.
- Open-unlinked verification compares the complete independent 4096-byte state,
  writes its prescribed edit, syncs and rereads before closing the descriptor.
  Chmod observes both aliases after each transition. Mtime checks exact values
  immediately. Normalization cannot repair the behavior under test.
- A separate runtime observation window covers all selected-run work, including
  late canonical/history/FUSE verification and recovery. It excludes process
  owner teardown and supervisor polling; pure operation timings are unchanged.
- A pre-discard scalar getter emits maintained physical-spool allocation and
  peak counters after failure. It performs no oracle, namespace census, payload
  read or fault injection. Unavailable counters and observation errors fail
  evidence validation while preserving the original product error and recovery.
- Docker compilation inputs precede provenance-only arguments and scripts.
  Final source/image seals remain mandatory. Layer reuse is inspected during
  the next required build, without a separate build comparison campaign.

## Precise invalidation

The nine pre-Commit Exec failures lack the required physical-spool event peak:
`tiny-bulk-create-100`, `tiny-bulk-create-500` and `tiny-bulk-delete-500`, each with
three seeds. Only these failed performance slots are recollected after adding
that observation. Their original failures stay sealed, with explicit
invalidation reasons and reproduction references. The 48 raw passes and 27
failed Commit outcomes have the necessary existing observations and are reused.

No final verification, CDC boundary proof, reliability proof or dedup/history
performance had been collected when these corrections were made. Those use the
new sealed assets from their first execution. Old ordinary performance can be
paired with new verification only through an explicit reviewed compatibility
record: unchanged product and frozen contract, checked unchanged fixture/oracle
source files, equal input/environment identities, and the documented effect of
host changes outside the successful timed-call path. This does not relabel old
results as newly produced evidence.

Focused new checks are under
`benchmark-results/fs-bench-pro/phase1-v013/qualification/`: runner recovery,
`audit-workload/`, `history-artifact-format/` and report receipt checks. They do
not replace any required initial sample or late independent proof. No passing
benchmark family was rerun for these corrections.

# #237 native Init research preregistration

> **Status:** Research; informative and not a product contract.

Source control starts at `7df25f9790996cf83232782c7b35f7c26fcc3252` in the
isolated `codex/issue237-init-research` worktree. All trials below are labelled
diagnostics, not #231 gate samples. Each case and source identity is invoked at
most once. The #231 fixture profile, seed 1, daemon-host public operation and
15-second request/command limits stay fixed. `#236` owns the pending host-direct
SDK route; these trials cannot qualify it. A 100k native Init run is not registered
by the current runner, so it remains NOT_RUN here unless a separate prospective
contract and driver are completed.

## D1: same-route cold-source mechanism trial

- Control: unchanged main product, `namespace-10000`, one call and full verifier
  if a confirmed root returns.
- Treatment: one unmerged research diff in `history_bootstrap::prerequisites`:
  reuse the C1 metadata root for entries with identical kind, mode and mtime.
  The one C2 save, public command, file reader, FileView validation and C5
  publication remain in the same places. No changed worker or Store policy.
- Expected counts: 10,101 metadata lookups but two unique metadata builds on
  this fixture (regular file and directory), instead of 10,101 builds. This is
  source-derived and must be checked against the manifest and Store rows. C2
  representation and canonical root should be unchanged. If physical Store size,
  `sum(length(data))`, pack occupancy or full readback worsens, reject it.
- Source cache: after fixture seal/reuse check, use the existing Darwin
  `Residency` backend on **every** source file: invalidate data pages, then a
  separate nonfaulting whole-input `mincore` pass. Require zero resident pages
  and record the gap until caller timer. Both arms get this exact method. The
  runner itself still calls this `source-cache-uncontrolled-v1`; the research
  sidecar is independent and makes no gate PASS claim.
- Result: retain every control and treatment receipt, telemetry, Store and
  readback result. Compare public operation and named Service spans as one
  observation per arm, with CPU and Store bytes kept separate. A failed root or
  unequal cache state prevents a matched speedup claim, but its mechanism counts
  and failure remain evidence.

## D2: sparse-pack protection

The #229 stride1 archived/current figures are historical observations, not a
matched pair for this trial. Read its pack policy and calculate allocation from
the retained rows. A metadata memo must leave pack reservation behavior intact;
no claim of sparse-pack repair follows from a dense Init Store. A future pack
algorithm requires a separate sparse-history control/treatment and full readback.

## Attempt ledger

- `issue237-d1-control-edc290627-a`: script import failed before it created an
  output directory, fixture or product process (`runner` resolved to the legacy
  module, which has no `owned`). Zero performance samples. Fixed only the Python
  import order before the first D1 control sample.

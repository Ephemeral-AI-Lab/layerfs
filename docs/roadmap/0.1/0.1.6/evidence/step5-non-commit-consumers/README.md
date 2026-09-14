# Step 5 non-Commit consumer migration: cancelled in-flight work

Status: **CANCELLED AND UNVERIFIED. Do not treat anything here as product
evidence, and do not merge it as-is.**

The owner stopped the campaign while this migration was mid-flight. The
delegated agent was cancelled with 977 inserted lines across four files and no
evidence file written. This record exists so the work is recoverable rather
than silently lost.

## Contents

- `UNVERIFIED-cancelled-work.patch` — the exact working-tree diff at the moment
  of cancellation, against `crates/layerfs-workspace/src/`:
  `lifecycle.rs` (+466), `reconcile.rs` (+468), `registry.rs` (+52),
  `projection.rs` (+5).

## What is NOT true of this patch

- **It was never built or tested.** The cancellation happened before the agent
  ran its acceptance criteria. A full workspace suite run during the window
  reported one failure, which is void as evidence: it read files that were being
  edited seconds earlier, so it reflected a half-written state, not a product
  defect. That void run is recorded in the ledger as void and must not be cited
  as a regression either way.
- **It has no evidence file.** The intended
  `evidence/step5-non-commit-consumers/README.md` was never written, so there is
  no record of what the agent believed it had migrated, what it had tested, or
  what it had deliberately left alone.
- **It is not reviewed.** No one has read it for correctness, contract
  compliance, or interaction with the merged Steps 2-3 host-authority migration.

## What is known about its intent

The assigned scope was the consumer-audit rows still on the legacy authority on
the host path: fsync/cache operations, reconciliation and recovery, and
metrics/resource reporting — explicitly excluding V1, the container route,
`Materialize`, and any deletion of legacy helpers. Whether the diff actually
confines itself to that scope is unverified.

## How to resume

1. Read the patch and confirm it matches the assigned scope.
2. Apply it on a fresh branch off current `main` and build.
3. Run the acceptance criteria the task required: workspace suite at the
   recorded baseline of 167 lib + 12 + 2 passed / 0 failed / 13 ignored, plus
   real-assertion tests for each migrated consumer.
4. Write the missing evidence file with actual commands and raw results.
5. Only then treat any of it as reviewable.

If it is not worth resuming, delete the patch and re-dispatch the task with the
original specification; the working tree changes remain in place either way.

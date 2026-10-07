# R2 proof counter custody

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Source review after `d5ef7ba4e`,2026-10-07; focused correction before F15.

## Deepest-file plan

- Extend `layerfs-content/tests/filesystem_qualify.rs` with a validly encoded
  root containing one unreachable inode and nonzero caller work counters.
  Require refusal; also require successful proofs to report only their own pass.
- In `layerfs-content/src/filesystem/qualify/walk.rs`, make the per-pass work
  start fresh before any provider call. Caller counter history must never
  participate in proof closure.
- Document the per-call counter contract in `qualify/proof.rs` and the
  [architecture](../../architecture/59-root-qualification.md).
- Retain the reproducing failure and corrected host/Linux public-API proof
  under `checks/pre-s8-r2-counter-custody-20261007/`, plus scoped final checks
  and exact LOC. No Store profile, mount behavior or benchmark changes.

The existing pass compares `work.bindings` directly with the sum of declared
bindings. The input work structure is public and mutable. Source review predicts
that a preloaded count can compensate for an unreachable inode; the regression
is the next step and no successful correction is claimed yet.

## Outcome

Receipt02 confirms the defect: a caller-supplied binding count returned an
invalid proof for a root with an unreachable inode. The correction resets the
per-pass counters at entry. Receipt04 accidentally invoked the old artifact
from01 (same SHA-256 as02), not the new binary named by03; it is retained as
stale-artifact/ineligible evidence, not a corrected-source regression.

Correct binaries pass11 Content bodies and the real daemon overlay-scratch
body on host05/06 and Linux09/10. Build03/08 and source hashes07 pin the inputs;
Linux uses the established image and container-local overlay only. The new
regression also verifies that a second valid pass with the previous work value
reports exactly its own counts. The daemon still accounts2162 original jobs
for2022 inodes/2040 bindings with bounded windows and releases all owners.
Clippy11, formatting12, boundary13 and42 guard self-tests14 pass. Tests have100s
stops, no Store profile is executed, no benchmark runs and Durable stays deferred.

Production LOC:170525 ->170526 (delta+1). Core105108 ->105109; reference
65417 unchanged; active core61943 ->61944; excluded predecessor36325 and
excluded integration6840 unchanged. Receipt15 pins the exact staged comparison.
The independently requested WAL-throughout restoration is unstaged and excluded
from this commit's comparison.

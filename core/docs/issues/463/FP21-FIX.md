# FP-21 provider-read staging repair

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This test-only follow-up starts at migration commit `853abcbe2b`. The migration's
original FP-21 failure and verification exception remain recorded in
[IMPLEMENTATION.md](IMPLEMENTATION.md); this repair does not relabel that run.

## Confirmed cause

Both cases in
[fp21_detach_stages.rs](../../../../engine/crates/layerfs-daemon/tests/fp21_detach_stages.rs)
failed before their first Unmount, while waiting to hold a request in the
persistence provider. A diagnostic run reproduced both failures in 10.13 s:
OPEN had already answered `opened`, READDIR had answered the exact expected
names, active requests and gate holds were zero, and Storage reported many
pack-cache hits. The requests had completed without entering the armed gate.

`Store::new(cache_bytes=0)` disables the shared canonical cache, not a mounted
reader's kept pack, decoded-group and pooled-value caches. The caller's
preparation (`lstat` or directory open) populated those caches. The old fixture
assumed every subsequent logical Store demand would reach persistence.

## Repair

The new external
[pressure helper](../../../../engine/crates/layerfs-daemon/tests/support/fp21_read_pressure.rs)
saves 73 valid inode leaves with 7,300 distinct inline metadata values and a real
saved payload. Their count derives from the existing 512 KiB pooled-value
allowance and 73-byte value width. Distinct mtimes prevent value deduplication.
After caller preparation quiesces, the helper reads those leaves through the
same mounted Workspace's ordinary Store ports, checking every canonical answer.
The existing cache-overflow path discards the prepared stored values. The
target request must acquire them again; its actual held provider call establishes
the staging condition. Pack and locator caches stay enabled, the Store policy
stays at its default, and the global persistence profile remains explicitly
Disposable.

No production API, cache-reset hook, dependency, assertion or timeout changes.
No tests are deleted. A staging timeout now reports mount, gate, reader and
Storage counters plus any caller response. The existing checks still require
exactly one held request, ten Busy unmounts with no effect, unchanged ownership,
the original successful result after release, and complete subsequent cleanup.

## Verification and retained evidence

The first repaired run passed both cases in 0.25 s under a 90-second command
ceiling. Each case observed `active=1`, `holding=1`, `expired=0`, and unchanged
engine counts during ten Busy unmount attempts. OPEN then read the exact
3,000 bytes; READDIR returned the exact three names; both reached cleanup Gone.

Focused Linux test compilation and warning-denying Clippy passed, as did Rust
formatting, the full boundary guard (954 production inputs), its 34 self-tests,
fuser provenance and whitespace checks. This is a scoped fixture repair; no
full migration suite, benchmark or Durable execution is repeated.

Receipts are append-only outside Git under
`~/.cache/layerfs-worktrees/fix-fp21-provider-gate-194dabf32fd3/builds/`:

- `diagnostic-6738cd312831423dae9c3ef6f2dd308d`: reproduced staging failures.
- `pooled-pressure-0b3b856f7a794ae6919249bd2f50810a`: first successful repair.
- `cargo-linux-df19d4d4e85744ca980aa39abce7668d`: focused Clippy.
- `final-proof-abaf6103a7b34fbd92f42694aaae37bc`: final focused proof, 2/2 passed.

The final verification receipt and exact snapshot LOC comparison are retained
in the same registration. Production LOC: 102162 -> 102162 (delta 0), counted
with `tools/production_loc.py --json` on exact parent and staged trees. Core
38,241; engine 56,640; API 7,281. Reference absent (retired R9c; last counted
65,417 at tree `498dd1917812`).

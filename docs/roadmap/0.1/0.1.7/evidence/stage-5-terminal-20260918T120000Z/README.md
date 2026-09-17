# Stage 5 terminal round 4: the remaining ledger rows (2026-09-18)

> **Status:** Round-4 evidence for the Stage 5 terminal handoff
> ([`stage-5-terminal-handoff-20260917.md`](../../component-decoupling/stage-5-terminal-handoff-20260917.md)).
> Append-only. Nothing in this directory was edited after it was written, and no
> prior receipt, report or failed attempt was overwritten.

| | |
| --- | --- |
| Code tree | `9327f6695` (`head.txt`): `6c00e0f53` provider failures / pragma set / profile verification / unsafe boundary, `9327f6695` one reader per ordering tier |
| Production LOC | core 18,708 → 18,797 (+89): C1 11,902 → 11,922 (+20), C2 6,043 → 6,112 (+69), telemetry 763 → 763 (0); reference 65,417 unchanged |
| Rows claimed | `R2-F4`, `R2-F8`/`N-6`, `R2-F10`/`N-7`, `R2-F11`/`N-8`, `R2-F15`, `R2-F23`/`N-17`, `R2-F24`, `N-13`, `N-14`, `TR-5`, `F27` (info) |
| Checks | the eight handoff §6 commands, logs `check-*.log`, run on the round's final tree |

## What this directory proves

| Row | Receipt | What it shows |
| --- | --- | --- |
| `R2-F8` / `N-6` | `ordering-scaling.log` | the review's P1 grid re-run on the fixed tree, release profile, one sample per case: work counters **identical** to the review's pre-fix grid (rows spilled 448/960/1,984/3,968; `rows_read` 2,198/6,684/19,960/59,007; runs 14/30/62/124; peak owned 66,432/139,008/284,160/568,320 B); per-doubling read ratio still ~×3.0 (the tiered merge is O(n log n); no O(changes) claim); elapsed 16.3/32.7/79.6/157.6 ms vs the review's 15.8/35.6/87.6/154.5 ms, single samples, no faster claim |
| `R2-F8` / `N-6` | product test `filesystem_ordering_scan.rs` (`check-cargo-test.log`) | a counting global allocator proves the property the grid cannot show: after twelve spills leave several tiers live, an ascending sweep over all 768 serials reads exactly the spilled rows once, and a mixed continuing/restarting wave of 832 lookups performs **zero** heap allocations; the dead `LookupScan::total` field is gone and the module docs state the one-reader-per-tier bound |
| `TR-5` | `simultaneous-memory.log` | what one update holds at once during its references phase - pending rows (64 × 96 B, counter `peak_pending`), ordering bytes (568,320 B, counter `peak_run_bytes`), live-tier scan buffers (5 × 16 KiB, counter `peak_live_runs`), caller backing peak (counter `FileBacking::peak_bytes`) - plus the sequential-phase scratch peaks (directories 376,110 B, inodes 349,820 B); scope, unit and counter named per row; **no process-level RSS, cgroup or page-cache figure is claimed** |
| `R2-F4` / `VF-7` | `per-commit-loc-reread.log` | the six drift commits recomputed beside their disclosures (07f0fe8eb +26, 5d08d9e83 +12, bfd7abf2c −32/−38, c17f59bef −32, 821ddbe30 −15, f723663a5 −15), merge `a8a1ba848`'s missing line supplied (732 → 732, delta 0), the combined→core-only scope switch pinned at `64e3f9d6a`→`01d9f70f3`; reproduces the round-2 review's `per-commit-loc-stage5.log` column for column |
| `R2-F10` / `N-7` | `check-cargo-test.log` (`provider_errors`) | through public entry points: a corrupted pack header is `ContentError::ProviderFailure`, an unsaved object is `MissingObject`, an unpublished record is `ProviderFailure` ("record above the visibility ceiling") - absence and corruption are distinguishable at the provider boundary |
| `R2-F23` / `N-17` | source; `check-boundary.log` | the integer-pragma helper takes a closed `Pragma` enum; no caller string reaches SQL text (`format!("PRAGMA {name}")` deleted) |
| `R2-F24` | `check-cargo-test.log` (`connection_profile`) | `MutationOwner::acquire` re-verifies the declared profile on the caller-supplied connection: an unconfigured, a degraded and a busy-waiting connection are each refused with the named `Integrity` reason; a configured connection passes |
| `N-13` | `check-boundary.log`, `check-core-tools-tests.log` | `unsafe` is denied crate-wide in `layerfs-storage` and allowed on exactly the audited `encoding/codec.rs` (module doc carries the FFI inventory); the boundary guard rejects `unsafe` elsewhere in the crate and a storage `lib.rs` without the deny; the `forbid` deviation is recorded with the inventory in `physical-encoding-and-packing.md` |
| `N-14` | `filesystem-tree.md` §9 | the caller-owned input bound declared as an explicit adapter obligation (protocol-level request ceiling, enforced by refusal); C1 claims no bound on caller-held input |
| `R2-F11` / `N-8`, `F27` | `admission-and-persistence.md` | the SQL transaction row and the preparation-batch row stated as commit/flush triggers with the code's actual semantics; one maximal object may exceed the transaction figure alone |
| `R2-F15` / `VF-7` | `stage-5-report.md` §2 | the report's headline states the totals of the tree that carries it (C1 11,922 / C2 6,112 / telemetry 763 / core 18,797 / combined 84,214) |

## The diagnostics client

`diagnostics/s5term/` depends on `core/crates/layerfs-content` and
`core/crates/layerfs-telemetry` by path and reaches every claim above through
**public entry points only**. It is not part of the product workspace and nothing
in the repository imports it. Build and run (release profile, like the review's
own diagnostics):

```sh
cd docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T120000Z/diagnostics/s5term
CARGO_TARGET_DIR=/tmp/layerfs-s5term-target cargo +1.85.1 build --release
/tmp/layerfs-s5term-target/release/s5term grid      # ordering-scaling.log
/tmp/layerfs-s5term-target/release/s5term coexist   # simultaneous-memory.log
```

The author-run reproduction above is **not** the verification: each row is
re-verified by a read-only verification subagent with fresh context
(`verify-*.md` in this directory), and no row is marked PASS before that.

## Discarded attempt, retained honestly

The first run of the grid used the **dev** profile by mistake (`cargo run`
without `--release`); its elapsed column (46.3 / 94.1 / 182.0 / 376.1 ms) is not
comparable to the review's release-profile grid and is not retained as a
receipt. The release run above is the sample. Work counters were identical in
both profiles.

## A clipped-run receipt for `measure_components` (R2-F14) cannot exist

The round-2 row asked for "a receipt of a clipped run" proving
`measure_components` exits non-zero. The verification pass established that no
legal input can reach the clip path: the demo's timing trees are structurally
coarse (5 nodes for an 8 MiB C1 input - the largest legal input, which the
binary itself refuses to exceed; 4 for C2; 59 for the pipeline mode with its
waves capped at 32 objects), against a node budget of 1,024. The row's evidence
is therefore the wiring (`measure_components.rs` returns `Err` on
`is_incomplete()`, and `main` propagates it - empirically exit 1 on every error
path, the same `require_complete` shape as the Stage-5 pair, which the round-2
review itself verified by source) plus the telemetry crate's own tests proving
`is_incomplete()` is true for a clipped report. This is stated here so the
missing receipt is an established impossibility, not an omission; see
`verify-R2-F1-F2-F3-F14.md`.

## Verification records

Each row above was verified by a read-only verification subagent with fresh
context; the `verify-*.md` files in this directory are their reports, with every
command, exit code and `path:line` citation. Findings they raised that were
accepted and remediated in the round-4 remedy commit: the FFI inventory omitted
two called entry points (N-13, both inventories corrected); the completion
report retained two superseded "10% slower" statements (corrected by its §11
dated note); `MAXIMUM_LEVELS`, `MAXIMUM_READ_DEMANDS`, the group-count ceiling
and the scratch default figure had no limits row, derived note or boundary case
(N-16/VF-4, all remediated); the walk-ceiling figures were one binding
conservative in the file-count phrasing (R2-F7 erratum, corrected in
`limits.rs`, §6 and §13.2); two test bodies asserted less than their names
promised (VF-3, both strengthened); the single-read test never reached the
chunked route (R2-F20, a chunked-base case added); the N-14 declaration
attributed the walk ceiling to `FilesystemResources::check` (corrected); and the
round's own audit found `de648507b`'s disclosed LOC levels were stale
(`per-commit-loc-reread-2.log`). The tasking prompts contained a mistranscribed
full commit hash (the tree is `99743b2cff2470e6634874d7ee14b9d37d0ba16e`; every
verifier identified the 9-character prefix match and verified the actual tree).

No row is marked PASS by this directory. A row flips only when a verification
subagent reproduces it from these receipts or through the public entry points.

## Checks

`check-*.log` hold the eight handoff §6 commands, each with its exit code, run on
the round's final tree. `git-status.txt` and `head.txt` record the tree the
receipts ran on.

No row is marked PASS by this directory. A row flips only when a verification
subagent reproduces it from these receipts or through the public entry points.

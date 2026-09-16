# W1 evidence — CHUNK delta candidates obey the eligibility rule (G1)

## What the packet fixes

`core/crates/layerfs-storage/src/encoding/delta/select.rs:209-210` took
`advisory.first()` for `ObjectRole::Chunk` with no eligibility probe. An advisory
predecessor with no committed row then flowed into `acquire` and the save failed
with `StorageError::ObjectMissing`; the `WHOLE_FILE` lane probes through
`eligible()` and selects FULL by policy.

## Commands, exits and raw output

| File | Content |
| --- | --- |
| `w1-verify.log` | 9 recorded commands, each with its exit code and wall time; all exit 0 |
| `w1-fails-without-fix.log` | the control run: only the CHUNK branch reverted |

Recorded commands (`w1-verify.log`), all with `LAYERFS_CONSTRUCTION_WORKERS=1`
where a test binary runs:

1. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test delta_payload` — exit 0, **13 passed, 0 failed**, wall 0.24 s.
2. `... --test edit_pipeline` — exit 0, 4 passed.
3. `python3 core/tools/check_product_boundary.py` — exit 0.
4. `python3 -m unittest discover -s core/tools -p 'test_*.py'` — exit 0.
5. `python3 -m unittest discover -s tools -p 'test_production_loc.py'` — exit 0.
6. `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — exit 0.
7. `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` — exit 0.
8. `python3 tools/production_loc.py --files` — exit 0.
9. `git diff --check` — exit 0.

`edit_pipeline` was run because the test scaffolding now carries the advisory
predecessor list through the handoff, which is the path the edit pipeline uses.

## Control run (each new case fails without its fix)

`w1-fails-without-fix.log` keeps the raw stdout of the reverted tree:

* `an_absent_chunk_candidate_selects_full` — FAILED,
  `an absent candidate is not a failure: ObjectMissing(ObjectId("23e22d28..."))`.
* `an_ineligible_chunk_candidate_selects_full` — FAILED,
  `an ineligible candidate is a policy outcome: ObjectMissing(ObjectId("93b1a8c1..."))`.
* `a_same_save_unsealed_chunk_predecessor_selects_full` — FAILED,
  `an unpublished predecessor must not fail the save: ObjectMissing(ObjectId("7d362394..."))`.
* `test result: FAILED. 10 passed; 3 failed`.

The three passing cases that remain green in the control are the pre-existing
`delta_payload` cases, which do not exercise a CHUNK advisory candidate.

## Identities

| | |
| --- | --- |
| Source | commit `91c3a0741fff64e8161d5c1b6e759f347ffbf757` (first parent) plus the W1 change |
| Tree | see the commit that carries this directory |
| Toolchain | `cargo 1.85.1 (d73d2caf9 2024-12-31)`, `rustc 1.85.1 (4eb2518d3 2025-03-15)` |
| Profile | `dev` (debug) test profile, `--locked` |
| Worker count | `LAYERFS_CONSTRUCTION_WORKERS=1` |
| Fixtures | in-process, deterministic: `noise(len)` (xorshift64), `patterned(len)` (index*37+11), base `noise(400_000)`, replacements `patterned(8_192)` and `noise(8_192)` |
| Cache state | not a measurement: no timed phase, no cache claim, no sample count |
| Production LOC | core `10929 -> 10938` (delta +9); C1 4385, C2 5812 -> 5821, telemetry 732 |

## What this artifact does not prove

* It does not prove that a CHUNK candidate ever *wins* the cost comparison in a
  real edit pipeline; it proves that an absent or ineligible one selects FULL and
  never fails the save.
* The control run uses the debug profile and in-process fixtures; it is a
  correctness oracle, not a measurement.
* No speed, storage or memory figure is claimed or implied here.

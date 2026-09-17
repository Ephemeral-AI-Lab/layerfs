# Evidence: Stage 5 component comparison, re-collected (2026-09-17)

**Why this directory exists.** The earlier receipt
`../stage-5-component-comparison-20260917T073017Z/` records
`source_commit 3b4941f1e` and ran the candidate arm as
`--example filesystem_primitives_candidate`, which changed in `b22712844` after
collection. Under `AGENTS.md` §3.3 a harness change invalidates the pair, so that
receipt is not identity-matched and its rows are **diagnostic only**. This is the
fresh collection at the remediation commit.

| Field | Value |
| --- | --- |
| Source commit | `eb42c13477f85612fc864474af4489c2548d688d` (branch `main`) |
| Working tree | clean (`working_tree_dirty false`, over `core/crates` and `crates`) |
| Toolchain | `+1.85.1`, `--locked` on both arms |
| Profile | `release` on both arms |
| Samples | **one per case per arm** (`--samples 1`); no repetition, no best-of |
| Cache state | `warm in-process fixture; the base tree is built in the arm process before the timed region` - declared by the driver and identical for both arms |
| Scope | sorted directory merge, sorted inline-inode merge and root encoding |
| Excluded on both arms | input normalization; deriving final reference counts; full topology validation; reference ordering preparation; physical persistence |

## Rows

| case | files | changes | identity | reference ns | candidate ns | candidate / reference |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| `small` | 200 | 20 | **MATCH** | 235,208 | 153,542 | 0.652792 |
| `wide` | 2,000 | 200 | **MATCH** | 2,977,625 | 810,792 | 0.272295 |
| `large-few-changes` | 20,000 | 20 | **MATCH** | 3,829,000 | 1,838,958 | 0.480271 |

An identity `MATCH` means both arms agreed on all six pinned identities the driver
compares: `base_directory`, `base_table`, `base_root`, `updated_directory`,
`table` and `root`. A mismatch makes the driver record `MISMATCH` and print no
ratio; none occurred.

## What these rows are not

They are a **component** comparison of three primitive steps, not a
complete-operation comparison and not a qualification. `VF-6` (complete-operation
comparison against the reference) remains `NOT_RUN` with its pre-declared
source-backed reason: no equivalent public reference surface exists. These numbers
carry no cache credit beyond the declared warm in-process fixture, and the wall
times include the process build check and startup for each arm.

## Reproduction

    python3 tools/stage5_component_comparison.py \
      --output docs/roadmap/0.1/0.1.7/evidence/stage-5-component-comparison-<fresh-stamp>/run-1

`run-1/receipt.json` sha256
`43dbd9f440ea9b417278bd78498c33f5ad027901365f4940d00137fe2f014b24`;
`driver.stdout` is the driver's own transcript, exit 0.

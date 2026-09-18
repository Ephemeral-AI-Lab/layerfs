# Stage 6, round 4c — the product fix, and the oracle it unblocked

> **Status:** Round-4c closure receipt. Append-only. It supersedes nothing and does
> not edit
> [`../stage-6-round4-20260919T000000Z/`](../stage-6-round4-20260919T000000Z/README.md)
> or [`../stage-6-round4b-20260919T000000Z/`](../stage-6-round4b-20260919T000000Z/README.md).
> **This is the first receipt of the round whose product seal changed**: the two
> earlier ones were harness-only, so neither is comparable to this one.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **415.808 s** |
| Source commit | `90bbb617de4a43bfb5443aadd590bc37901eab1b`, **clean tree** |
| Harness binary sha256 | `00535ea829c39aece21c20b65436fabfa84e741a783a766c14f1bdc10ed5f863` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

Verification: **0 disagreements** across all 220 re-derived statuses; sealed
call-graph **PASS** over **120** product source files; runtime tripwires **PASS**
over **159** stores; verification budget `PASS` at **1.48 s** against the 60 s
limit. Calibration: E1 `REFUTED`, E2/E3/E4 `SATISFIED`, W1/W2/W4 `SATISFIED`.

| Round | PASS | FAIL | NOT_RUN | product seal |
| --- | ---: | ---: | ---: | --- |
| 3b | 194 | 0 | 26 | unchanged |
| 4 | 209 | 0 | 11 | unchanged |
| 4b | 217 | 0 | 3 | unchanged |
| **4c** | **217** | **0** | **3** | **changed** (`6435cf429`) |

## 2. The product defect (`6435cf429`)

`FilesystemRead::resolve` reported a name no directory binds as
`ContentError::MissingObject`. Four places in the product's own source say that is
wrong, in the same words:

- `object/access.rs`: "Absence is reported as `ContentError::MissingObject` and
  **nothing else**: a provider that holds state for the request but cannot serve it
  … reports `ContentError::ProviderFailure` so the two classes stay
  distinguishable."
- `cas/provider.rs`: "`MissingObject` is the answer for absence and **for nothing
  else** … which is the distinction a later adapter needs between 'this root is not
  in this Store' and 'this Store is corrupt'."
- `error.rs`, on `ProviderFailure`: "It is never the answer for an object the
  provider simply does not hold … so a caller that must distinguish 'this root is
  not in this Store' from 'this Store is broken' can."
- `architecture/01-boundary.md`: "the two classes stay distinguishable … only the
  first is a legitimate reason for a caller to choose a different representation."

All four are about the **provider**. A name that is not bound is neither class: the
tree was read successfully and the name is not in it, so nothing was missing from
the provider and no record was malformed. Collapsing it made "this path does not
exist" and "the provider does not hold an object this tree names" the same answer —
the conflation the product forbids.

The reference tree keeps the class apart under the same name: `CoreError::PathNotFound`
("path does not exist"), used at the analogous sites — `tree/directory/edit.rs` for
a name, `tree/inode/table.rs` for an inode record.

**The fix** is `ContentError::PathNotFound` and one call site in `resolve`.
`error.rs` states the three-way distinction and `architecture/01-boundary.md` is
amended in the same commit, because the provider error contract is documented there
verbatim and this adds a third class to it.

**The regression test** is
`filesystem_read.rs::an_unbound_name_is_not_provider_absence`. It asserts both
classes in one test — an unbound name is `PathNotFound`, a provider that does not
hold the tree's own root object is `MissingObject`, and they are not equal — so the
distinction is pinned rather than one side of it. Confirmed to reproduce the
original failure against the previous source before the fix was kept:

```text
left: MissingObject
right: PathNotFound
```

**Six external tests pinned the conflation** and were updated, each after checking
which class it was actually asserting: `filesystem_read.rs` (`d/absent`),
`filesystem_topology.rs` (`dead`), `filesystem_updates.rs` (`gone`),
`filesystem_hardlinks.rs` (`a/x`) and `filesystem_pipeline.rs` (`alias`, twice).

**Four sites were checked and left alone** because they assert genuine provider
absence: `filesystem_attributes.rs` and `filesystem_limits.rs` read through an empty
`TreeStore`; `edit_bounds.rs` and `file_read.rs` read through a stripped and a
poisoned provider; `provider_errors.rs` is the provider bridge's own class test.

**`FilesystemRead::inode` was deliberately not changed.** It also returns
`MissingObject`, for a serial the inode table does not hold. The reference uses
`PathNotFound` there, but in the replacement a serial the table lacks *after* the
directory page named it is a torn tree rather than an absent path, and
reclassifying it needs its own confirmation rather than this commit's momentum. It
is the remaining half and is recorded as such rather than quietly included.

## 3. The oracle the fix unblocked (`90bbb617d`)

The eight `tiny-unlink` / `tiny-bulk-delete` rows had **no listing oracle**. The
obvious expectation — "every directory the fixture has reads back empty" — is
wrong: the removal input unbinds the root's own bindings as well as each
directory's, so the derived directories leave the namespace rather than becoming
empty, and the correct expectation is **absent**, not empty. Writing that needs the
read path to distinguish a removed path from a broken provider, and until
`6435cf429` it could not.

The oracle is now two-sided:

- the root reads back with **0 bindings**, against the harness's own removal input;
- every directory in the fixture manifest reports **`PathNotFound`**, with
  survivors and misclassified errors counted separately, so a wrong error class
  fails rather than passing as "gone".

Both sides are two-sided on coverage as well, because the first version of this
oracle left its expectation list empty and passed with "0 directories" — a gate
that cannot fail, caught and removed rather than kept.

| Row | complete command | root bindings | directories `PathNotFound` | from its own output | from the base | gates |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `tiny-unlink-1-mixed-v4` | 0.004 s | 0 | 1 | 1 | 12 | 10 |
| `tiny-unlink-500-compact-v2` | 0.008 s | 0 | 16 | 16 | 228 | 10 |
| `tiny-bulk-delete-500-mixed-v3` | 0.008 s | 0 | 16 | 16 | 228 | 10 |

Eight rows move from 8 to 10 gates.

## 4. Production LOC

`19517 -> 19519` (**delta +2**): the `PathNotFound` variant and its `Display` arm.
The `resolve` change is a one-line substitution and the harness commit is outside
the production count. Reference `65417 -> 65417`; combined `84934 -> 84936`.

Counting method and scope are recorded in each commit message and reproducible with
`python3 tools/production_loc.py`. Scope: first-party product implementation
(`core/crates/*/src` + runtime SQL), comments, blanks and tests excluded.

## 5. Checks run

| Check | Result |
| --- | --- |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | **473 passed / 0 failed** (472 before, plus the regression test) |
| `cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | clean |
| `python3 core/tools/check_product_boundary.py` | PASS — 120 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK, 6 tests |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | OK, 17 tests |
| `cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml` | 84 passed / 0 failed |
| `python3 -m unittest discover -s $H/shared -p 'test_*.py'` | OK, 106 tests |
| `python3 $H/runner.py self-check` | PASS — lock parity 46 entries / 0 mismatches, registry, exceptions, golden |

`tools/preflight.sh` was not run and was not restored. No CI workflow and no
aggregate gate was created.

## 6. The three rows that remain

`component-primitives-{payload,filesystem,edit}` are `NOT_RUN` with
`driver unimplemented: component-primitives`. They are **not among the frozen 217**;
`CONTRACT.md` §3 excludes them from admission and from every count under D1, and
their reopening condition is fixed by D1 as "the owner commissions a reference-tree
entry point… a change to `crates/`, not to `core/`". Recorded, not waived.

## 7. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |
| `experiments-E1-E4-W1-W2-W4.json` | the calibration run |

# Phase 1 execution contract (frozen before collection)

> **Status:** Frozen contract for Phase 1 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), written into
> this evidence directory **before any Phase 1 receipt was collected**. It
> inherits the Phase 0 measurement contract
> ([`../phase0-baseline-20260917T221759Z/CONTRACT.md`](../phase0-baseline-20260917T221759Z/CONTRACT.md))
> unchanged: the frozen workload set of its §5, its identity discipline and its
> sample discipline. Nothing here relaxes it; a change after collection needs a
> new stamp and a new directory.

## 1. What is frozen, and what this contract adds

Everything in the Phase 0 contract §1–§2 (frozen toolchain, clean tracked tree,
`--locked`, `release`, one worker, one sample per workload, fresh `--output`,
declared and equal cache state, measurement lock, ≤ 15 s budget, no product/test/
harness change outside the item's own commit) applies to **every Phase 1
collection** verbatim.

Phase 0's workload set is **reused, not redefined**: the workload ids, vehicles
and parameters are the ones in `../phase0-baseline-20260917T221759Z/CONTRACT.md`
§5 and `RUNPLAN.md`. Phase 1 adds no workload to that set.

## 2. Where the vehicles live (the one Phase 0 limitation Phase 1 must lift)

Phase 0 recorded two `NOT_EXPOSED` gaps (`p0-3-counter-baseline.md` §1, §7):
`ValidationWork` in every frozen vehicle, and the edit family's `nodes_read`
outside `edit_timing_c1`. Phase 1's prerequisite commits **V1–V3** extend the
existing frozen vehicles (examples only, no product change) and nominate one new
example. The rule those extensions follow:

1. **The frozen vehicles keep their names, cases, parameters and printed fields.**
   A vehicle extension may only **add** printed lines; removal or renaming of a
   printed field is a new workload, not an extension.
2. **The probe client is new, and is a port, not an edit.** The ordering probe
   (`order <pairs> <pending>`) is re-implemented in this directory's `client/`
   from the Phase 0 client's shape, because the Phase 0 evidence directory is
   append-only and never edited. The port is verified by **matching the Phase 0
   published counters bit-for-bit** on the pre-item tree before it is used as an
   anchor (this contract's §4).
3. **A `--output` path is never reused.** Every collection gets a fresh
   `<round>/<step>/<label>` directory.

## 3. Rounds, arms and the append-only rule

One **round** per item (C1, V1–V3, P1-1..P1-16), one directory per round:

```text
rounds/<item>/before/     the pre-item tree's collection (the item's own arm)
rounds/<item>/after/      the post-item tree's collection (the gate sample)
rounds/<item>/receipt.md  the item's receipt, written once, never edited
rounds/<item>/verify-<item>.md   the verification subagent's only write
```

* `before/` is collected on the item's **parent commit** (or on the closest
  pre-item tree when the parent differs only in documentation) and `after/` on
  the item's own commit. Both are collected with the **same** client binary
  identity rule as Phase 0: a rebuilt artifact invalidates its matched arm.
* A receipt is **append-only evidence**. A correction is a new dated section in
  the same file marked as a correction, or a new round directory — never an
  overwrite.
* `elapsed_ns` is **diagnostic-grade in every Phase 1 receipt**: Phase 0 proved
  bit-identical work counters and a **+17.6%** same-binary elapsed spread
  (`p0-3-counter-baseline.md` §6). No Phase 1 box is gated on elapsed.

## 4. The before/after discipline

1. The **before** value is the Phase 0 published D-row where Phase 0 published
   one, or the `before/` collection of this round where Phase 0 recorded
   `NOT_EXPOSED`. Straddling a counter-semantics change is forbidden: every
   `pages_read`/`read_waves` anchor quoted after **C1** is the **C1 re-baseline**,
   not the Phase 0 value (Phase 0's `pages_read` columns are declared non-
   comparable and its batch-read `read_waves` values inflated ×2).
2. The **after** value is one sample on the item's committed tree, same counter,
   same workload, same declared cache state and worker count.
3. Every round collects at least one **labelled determinism re-run** of its
   primary row; the receipt states, per counter, whether the re-run agreed.
4. A predicted magnitude that the receipt does not reproduce is reported as a
   refutation, not smoothed over.

## 5. Checks every landing commit runs (exit codes recorded in the receipt)

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

Plus the item's own suites (`test-evaluation.md` §4) and the 34-test sealed-oracle
parity set (`test-evaluation.md` §1): `fixture_seal` 2, `filesystem_reference` 2,
`edit_reference` 2, `object_identity` 11, `filesystem_codec` 9,
`filesystem_updates` 6, `filesystem_profile` 2.

## 6. Acceptance (restated from the handoff)

A #178 box flips only when: the item's receipt exists and shows the counter moving
in the predicted direction on the frozen workload set; the new test passes **and
was shown failing on the pre-item tree**; the parity set is green and unchanged
(the two pre-authorized pinned updates excepted); the eight checks are green; the
commit message carries the production LOC line; and a verification subagent that
did not write the code has reproduced the movement through public entry points.

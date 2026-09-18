# Phase 2 execution contract (frozen before collection)

> **Status:** Frozen contract for Phase 2 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), written into
> this evidence directory **before any Phase 2 receipt was collected**. It
> inherits the Phase 0 measurement contract
> ([`../phase0-baseline-20260917T221759Z/CONTRACT.md`](../phase0-baseline-20260917T221759Z/CONTRACT.md))
> and the Phase 1 execution contract
> ([`../phase1-execution-20260918T090000Z/CONTRACT.md`](../phase1-execution-20260918T090000Z/CONTRACT.md))
> unchanged: their frozen workload set, identity discipline, sample discipline and
> cache-state rule apply to every Phase 2 collection verbatim. Nothing here
> relaxes them; a change after collection needs a new stamp and a new directory.

## 1. What is frozen

The Phase 0 workload set, reused through Phase 1 without redefinition: the
workload ids, vehicles, cases and parameters of
`../phase0-baseline-20260917T221759Z/CONTRACT.md` §5 and `RUNPLAN.md`, as driven
by `../phase1-execution-20260918T090000Z/collect.py` (sets `c1` D1–D6, `fs`
D7–D9, `edits` D10–D24, `order` D25/D26, `extra` D27, `v2` M1–M4, `c2` D28/D29,
`diag` X1/X2). Phase 2 adds no workload and removes none. The vehicle extensions
Phase 1 landed (`V1`–`V4`, the additive `M1`–`M4` rows) are inherited as they
stand.

Phase 2's own instrument extensions are **additive prints only**, in the same
class: `V5` adds the statement count to the `c2` rows, `V6` the ordinary-lane
group-decode count to the read rows, `V7` the save-connection cache observables
to the `c2` rows. A frozen vehicle keeps its name, its cases, its parameters and
every field it already printed.

## 2. Identity discipline

1. One toolchain: `+1.85.1`, `--locked`, `release`, stable toolchain. One
   construction worker (`LAYERFS_CONSTRUCTION_WORKERS=1`, exported by the
   driver); no run raises it (owner ruling 6). One sample per case per arm; a
   fresh `--output` path per run; an existing output path is refused by the
   driver.
2. The probe client links `layerfs-content`/`layerfs-storage` statically, so it
   **must** be the artifact built from the tree under test. Every arm rebuilds it
   (`B2`) and records `artifacts.txt` (sha256 per binary). A round whose client
   hash does not match the tree under test is invalid (the Phase 1 failure this
   rule exists for is recorded in the Phase 1 `ROUND-README.md`, correction 2).
3. Each round's `before/` arm is the tree of the item's parent commit; its
   `after/` arm is the item's own committed tree. Where two items land
   consecutively, the earlier item's `after/` arm **is** the later item's
   `before/` arm and the receipt says so — one collection, two citations, no
   re-collection and no drift.
4. `elapsed_ns` is diagnostic-grade everywhere (P0-3: **+17.6%** on one binary
   and input). No Phase 2 box is gated on elapsed.

## 3. Rounds and the append-only rule

```text
rounds/<item>/before/    the pre-item tree's collection (or the cited prior arm)
rounds/<item>/after/     the post-item tree's collection (the gate sample)
rounds/<item>/receipt.md         written once, never edited; corrections append
rounds/<item>/verify-<item>.md   the author-verification record
```

A receipt is append-only evidence. A correction is a new dated section in the
same file marked as a correction, or a new round directory — never an overwrite.
Failures, `NOT_RUN` rows and discarded attempts stay on disk.

## 4. Verification: single agent, author-verified

Phase 2 is executed by **one agent in DeepSeek Harness**. There is no second
reviewer, no subagent and no external coding agent: every number below was
produced by the agent that wrote the code. The verification record is therefore
labelled **author-verified**, never "independent", and its evidence is the
**reproducible receipt** — a named tree, a command and its output that any reader
can re-run:

1. `git archive <commit> | tar -x -C /tmp/verify-<item>`; build and run there;
   confirm the after-value from the archive, not from the working tree.
2. Falsify: the new test fails on the parent tree; the counter moved in the
   predicted direction and rough magnitude; no other counter moved unexplained;
   the parity set is green with `git diff <parent>..<commit> -- '*tests*'`
   showing only new tests and no re-pinned parity test; the commit is
   single-variable (`git show --stat`); the item's named risks are probed one by
   one. `P2-0`'s falsification is the absence of a test diff.
3. If a check cannot be run in this environment, the record says so and lists it
   under UNVERIFIED rather than skipping it silently.

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

Plus the 34-test sealed-oracle parity set: `fixture_seal` 2,
`filesystem_reference` 2, `edit_reference` 2, `object_identity` 11,
`filesystem_codec` 9, `filesystem_updates` 6, `filesystem_profile` 2. This
repository runs **no CI and no aggregate preflight** (both retired by owner
decision): the receipt reports exactly which checks ran, which did not and why,
and never claims "CI green".

## 6. Acceptance

A `#178` box flips only when its receipt exists and shows the counter moving in
the predicted direction on the frozen workload set; the new test passes **and was
shown failing on the pre-item tree**; the parity set is green and unchanged; the
eight checks are green; the commit message carries the production-LOC line; the
architecture document is updated in the same commit as any change to a counter, a
bound or a cache; and `verify-<item>.md` records the reproduction with its
falsification answers. A box may also close **measured-and-declined**,
**not delivered** or **blocked** — each with its receipt, which is a completed
item, not a missing one.

# verify-p2-0 — the `cas/owner.rs` responsibility split

> **author-verified** (single-agent Phase 2: there is no second reviewer and no
> subagent; the evidence is the reproducibility of the commands below on the named
> trees). Round: [`receipt.md`](receipt.md). Tree: `ed5ab5d95`, arm
> [`after/`](after/); before arm [`before/`](before/) on `0a1d74742` (product tree
> `502f2aae1`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive ed5ab5d95 \| tar -x -C /tmp/verify-p2-0` | 0 | clean P2-0 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` in the archive | 0 | examples built |
| R3 | the probe client built in the archive's own `…/client` | 0 | — |
| R4 | `phase0client order 4000 2000 64` | 0 | `rows_read 25809`, `rows_written 25760`, `runs_created 124`, `merges 61`, `dir_pages_read 17`, `ino_pages_read 81`, `read_waves 5`, `objects_read 98` — every field identical to `after/logs/D26` |
| R5 | `phase0client c2 8191 default` | 0 | `inserted 8191 reused 0 packs_created 33 pack_appends 8169 commits 31 pool_leaves 8191 pool_groups 8191` — identical to `after/logs/D28` |
| R6 | `edit_timing_c1 --case split`, `filesystem_timing_c1 --case directory-update --output …` | 0 | identical to `after/logs/M4`, `after/logs/D2` on every counter |
| R7 | `python3 compare_arms.py rounds/p2-0/before rounds/p2-0/after` (working tree) | 0 | `steps compared: 35, differing: 0` |

Only `elapsed_ns` differs between the archive and the arm, which is the declared
diagnostic-grade field (`CONTRACT.md` §2.4).

## 2. Falsification answers

**2a — does the test diff fail on the parent tree?** The item has no new test, so
the form of this answer is inverted: the only test diff is the one-line path of
the source-locator guard, and it **fails on the parent tree**. Archive
`0a1d74742`, apply just that line, run the guard:

```text
panicked at crates/layerfs-storage/tests/visibility.rs:501:6:
owner source: Os { code: 2, kind: NotFound, message: "No such file or directory" }
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out
```

That is the honest form of the answer: the guard cannot pass on the parent tree
because the file it now scans does not exist there, which is exactly why the
split needed the line changed — see receipt §5.

**2b — did the counters move in the predicted direction?** P2-0's prediction is
**identity**, not movement: a pure relocation changes no work. Measured: 35
measurement steps, 0 differing, across D1–D29, M1–M4, X1/X2, including every
root, every emitted byte count and every run/page/wave counter. The comparator is
proven live by a control (receipt §2) so this is a measurement, not a silent pass.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests pass
(`edit_reference` counts 3: the 34-test set plus the third pinned test accepted at
ruling 8). `git diff 0a1d74742..ed5ab5d95 -- '*tests*'` shows exactly one file and
one line — `visibility.rs`'s guard path:

```text
 core/crates/layerfs-storage/tests/visibility.rs | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)
```

No parity test was re-pinned, and no expectation value moved.

**2d — single-variable?** `git show --stat ed5ab5d95` touches exactly: the four
new `cas/` modules, `cas/owner.rs`, `cas/mod.rs`, the two `PoolCounters` paths in
`cas/store.rs`, the one guard path in `tests/visibility.rs`, and the four
architecture documents the boundary rule requires in the same commit. Nothing
else; no formatting churn (fmt --check clean), no dependency change.

**2e — the plan's named risk (coupling)?** §2.1 claims the coupling is
one-directional: `seal_pending`/`offer`/`finish_inner` → `seal_group`,
`select_record` → `select_pooled`, `select_pooled` → its own cluster. Probed by
the compiler in both directions: every call site resolves with exactly two
visibility widenings (`pub(super) fn select_pooled`, `pub(super) fn write_pack`),
and the boundary checker confirms no entry file gained an `impl`. The two
cross-module private methods are the whole widening; every other cross-module
method was already `pub`.

**2f — is elapsed a gate anywhere here?** No. Every gate above is a work counter,
an exit code or a line audit.

## 3. UNVERIFIED

* **The guard's coverage after the move is asserted, not proven.** The path change
  keeps the same three anchors and both assertions over the file the pooled lane
  now lives in; that the new file's regions are the *same* regions is shown by
  `move_check.py` (no parent code line lost), not by an independent scan.
* **`elapsed_ns` moved in both directions between arms** (e.g. D27 157 µs → 227 µs,
  M3 172 µs → 150 µs) — declared diagnostic-grade and not a gate; recorded so the
  spread is visible rather than cherry-picked.
* **The clean-tree reproduction ran a subset of the frozen set** (D2, D26, D28,
  M4) through the archive's own binaries, not all 35 steps; the full 35-step
  comparison is the working-tree arms, whose binaries' sha256 are recorded in
  `artifacts.txt` per arm.

# verify-p1-4 — validation's record demands behind one memo

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `bfb01f262`, arm [`after/`](after/);
> before arm [`../p1-3/after/`](../p1-3/after/) (tree `96c51d0df`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive bfb01f262 \| tar -x -C /tmp/verify-p14/tree` | 0 | clean P1-4 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` in the archive | 0 | vehicles rebuilt |
| R3 | `filesystem_timing_c1 --case directory-update --output <fresh>` | 0 | `validation: objects 21 waves 2 inode_demands 21 inode_pages 2 directory_pages 0 entries_examined 0` — identical to `after/logs/D2` |
| R4 | `--case hardlink-move` | 0 | `objects 6 waves 1 inode_demands 6 inode_pages 1 …` — identical to `after/logs/D4` |
| R5 | the other four cases | 0 ×4 | all fields 0 — identical to `after/logs/D1`, `D3`, `D5`, `D6` |
| R6 | `cargo +1.85.1 test … -p layerfs-content --test filesystem_bounds binding_lookups` | 0 | `1 passed` |
| R7 | the same test on the parent tree with only the test file copied in | 101 | `left: 2002, right: 6002` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes: `the wave count does not:
2002 against 6002` — on `96c51d0df` the validation's wave count is twice its demand
count (one root descent per demand), and P1-4 makes it constant in *k*.

**2b — did the counter move in the predicted direction, and did anything else
move?** Predicted: `validation.read_waves` and `inode_pages_read` down sharply on
D2/D4, `inode_demands`/`objects_read` identical, `entries_examined` identical,
`directory_pages_read` near-zero on D1–D6. Measured exactly that; and a diff of all
29 D-rows and the three M-rows between the arms, excluding the validation line, is
**empty**. A movement anywhere else would have been the finding.

**2c — parity green and unchanged?** The test diff in this commit is
`filesystem_bounds.rs` only; the seven sealed-oracle targets are untouched and were
re-run green on the C1 tree
([`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1). The
topology suite (`filesystem_topology.rs`, all acceptance and refusal cases) is
green in the workspace run.

**2d — single-variable?** Two files: `filesystem/validate.rs` (the state, the
prefetch, the threading) and `filesystem_bounds.rs` (the new test and the one
generalized cross-subsystem assertion). No doc change: no boundary, format or
named bound moves, and the memo is internal to validation.

**2e — error paths.** The plan's named risks were probed: (i) *error order* — the
prefetch can surface a corrupt base page before an earlier semantic error; the
classes are unchanged and the refusal suite is green, but no test pins the order of
two errors on one input, and the receipt says so; (ii) *the refusal* — a missing
base record still returns `missing base inode` (`lookup_one`); (iii) *the work
limit* — `the_cycle_check_work_limit_is_reachable_and_reported` and
`a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound` pass
unchanged, so the `entries_examined` charge and the refusal point are unmoved;
(iv) *the empty case* — D1 (empty tree) and D3/D5/D6 demand nothing and report
zeros, identical to before.

**2f — elapsed as a gate?** No. The gates are the validation counters and the
roots; `X1`/`X2` reproduce every counter.

## 3. UNVERIFIED

* **The memo's memory bound is stated, not measured.** The plan proposed "≤ 4,096
  entries/walk ⇒ ≤ ~150 KiB"; this implementation memoizes one `InodeValue` per
  *demanded* serial (not per walk entry) and nothing measures its footprint. The
  bound is the operation's own demand set, which is what the lazy path's demand
  set was; a declared ceiling is not added, and the receipt says so rather than
  quoting the plan's arithmetic as if it were measured.
* **The directory-page half of the plan's sketch is not implemented.** The memo
  covers inode records only; `list_after`'s per-page re-descent is untouched, and
  on D1–D6 that half would have been invisible anyway (`directory_pages_read` 0
  both before and after). The plan flagged the per-call re-descent as a possible
  follow-up (its risk (d)); it remains one.
* **`inode_demands` on a refusal path may differ** from the lazy path (the
  prefetch charges demands the loop never makes). No frozen row is a refusal path;
  this is stated in the receipt §5 and not measured.

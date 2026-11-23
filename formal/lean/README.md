# LayerFS integrity models in Lean

> **Status:** Research models. They prove properties of hand-written models
> of code on `layerfs_v2_0_0` at `04e55a7f4c`, not of the Rust or SQL
> themselves. They are not product contracts. The modelled files are
> unchanged from `04e55a7f4c` to `343c59b2ca`, the base of this branch.

Lean 4 models of five integrity properties, with their proofs. They use
core Lean only, with no Mathlib.

```bash
cd formal/lean && lake build
```

Toolchain: `leanprover/lean4:v4.36.0` (`lean-toolchain`), installed with
`elan`. A clean build takes a few seconds. Every main theorem uses only
Lean's standard axioms (`propext`, `Classical.choice`, `Quot.sound`), and
there is no `sorry`.

| # | Property | File | Main theorems |
|---|---|---|---|
| GC | Compaction's mark keeps everything live data needs | `Closure`, `History`, `Objects` | `run_correct`, `prune_doctor`, `compaction_safe` |
| 1 | History stays a DAG at any length; the doctor's cycle check passes | `HistoryDag` | `acyclic`, `doctor_cycles_pass` |
| 2 | A Commit that got a success reply is never lost from its Branch | `Lane` | `acknowledged_held`, `commit_guard`, `held_forever` |
| 4 | Commit capture and install keep the Workspace's live view | `Capture` | `install_view`, `fold_view`, `retire_view` |
| 5 | Delta chains stay decodable at any history length | `Delta` | `decode_ok`, `compact_keeps_record` |

`Examples.lean` runs each model on small inputs with `#guard`, which shows
the hypotheses of the theorems can actually be met.

---

## GC: the compaction mark

| Theorem | Statement |
|---|---|
| `run_correct` | The batched driver of `history/kept.rs::close` keeps exactly the reachable set, provided every `kept_node_expand` statement meets `ExpandOk` and the driver ends. |
| `required_followed` | Every reference the doctor requires (`diagnostic/history.rs`) is an edge `kept_node_expand.sql` follows. The LayerStack head is the one exception, and the seed covers it. |
| `prune_doctor` | Pruning a doctor-clean Store to a correct mark leaves history the doctor's reference checks accept. |
| `retired_branch_kept` | #42 item 29: a retired Branch named by a kept Commit stays, and so does its head. |
| `retain_closed`, `never_different_bytes`, `reachable_readable` | No dangling object reference or delta base; an ID resolves to its old object or nothing; live objects read back unchanged. |
| `compaction_safe` | All of the above combined. |

Model to code:

| Model | Code |
|---|---|
| `step`, `run` | `history/kept.rs` `close` (`BATCH_NODES`, `BATCH_ROWS`, the `rows.len() + batch.len() < BATCH_ROWS` test) |
| `seed` | `kept_node_seed.sql`; the `roots` CTE of the PostgreSQL `kept_node_mark.sql` |
| `sqlTargets`, `sqlEdges` | each `UNION` arm of `kept_node_expand.sql`; the PostgreSQL `VALUES` lists follow the same edges |
| `doctorRequired`, `Doctor` | `diagnostic/history.rs` reference queries and its `edges` table |
| `objEdges` | `FinalizedObject::admit(..).references()` plus `ChainBases::base_of` |

Assumptions:

1. **`ExpandOk` for the SQLite statement.** The completion test is only
   sound if every row of the recursive CTE is either a batch row or a
   returned row. If a discovered row were ever dropped by `ON CONFLICT DO
   NOTHING`, a statement that hit its limit could pass as complete. A cheap
   guard would be a test or debug assertion that compares the CTE row count
   with `rows + batch`.
2. **`StackLocal`.** The doctor does not check `source_branch_id` of a Layer
   with no parent for staying in the same LayerStack.
3. **A doctor-clean Store before compaction.**

## #1: History stays a DAG

The doctor peels history nodes parents-first and fails
`store.history.cycles` if any node is left.

- `ranked_apply` and `ranked_applyAll` keep the invariant through any run
  of operations: Commit and Layer inserts, compaction, and the operations
  that touch no Commit or Layer row (head moves, fork, removals).
- From that invariant, `acyclic` proves no path along the peel edges
  returns to its start.
- `doctor_cycles_pass` proves that when the doctor's peel loop stops, every
  node is done.
- `dagRefs_followed` shows every peel edge is also a GC keep edge, which
  links #1 to the GC model.

**Finding: `history_serial` is not a topological order.** A Layer that
republishes a Commit carries that Commit's serial (`layerstack.rs:431-433`).
A forced publish (#691) puts the new Layer on the current stack head,
whose source Commit can be newer. Along `layer.parent_layer_id` the serial
can therefore go *up*. The proof uses insertion order instead, carried as
a ghost `stamp`: every insert names only rows that already exist (the
foreign keys and `verify_layer`). Code that assumes serials decrease along
parent edges would be wrong. The `remove_file` frontier relies on the
republished serial on purpose.

## #2: An acknowledged Commit is never lost

A head moves inside one exclusive write transaction, in two places:
`commit_branch` (`history/commit.rs:87-181`) and `merge_into_branch`
(`history/merge.rs:57-195`).

- `acknowledged_held`: right after `commit_branch`, the Branch holds the
  new Commit.
- `commit_guard`: `commit_branch` meets the move guard whether forced or
  not. The guard is that everything the old Branch row reached is still
  reachable.
- `held_forever`: what a Branch holds, it keeps holding through any valid
  run of later Commits, merges, forks, publishes, removals and compactions
  run while the Branch is live.

**Finding: a forced Commit does not keep the old head as an ancestor.**
With `force` and a moved head (`commit.rs:108-110,149-161`), the new
Commit's parent is the bound head, and the old head is recorded only in
`displaced_id`. So "never lost" holds in the GC's sense: the old head stays
reachable along keep edges, which include `displaced_id`, and compaction
keeps it. It does not hold in the ancestry sense. A caller that walks only
`parent` edges from the head will not see a Commit that was displaced
after it got a success reply.

**Doc drift:** `docs/architecture/core/history/README.md` says "A Branch head
moves only through `commit_branch`". `merge_into_branch` also moves heads,
through `branch_move_branch.sql`. The same table says "the one removal is a
whole LayerStack", which no longer holds now that compaction removes
unreachable Commits.

Not modelled: merge fast-forward and merge Commits are covered only
through the general move guard; no concrete merge theorem like
`commit_guard` was written for them. Lane contention between processes
(the queue and directory lock) is not modelled either.

## #4: Capture and install keep the live view

The overlay keys every row by a key and a generation, and a read takes the
newest row in `(installed, active]`, otherwise the base. Capture moves
`active` to `g+1`; install sets `base := R'` and `installed := g`.

- `install_view`: every read is the same before and after install,
  provided that for every key **without** a `g+1` row, the published root
  agrees with the captured view. Construction meets this. Where it
  deliberately keeps the base's value (an mtime-only change),
  `keep_live_row` first copies the row into `g+1`. That condition is the
  exact obligation on the code.
- `install_view_constructed`: the common case, where the root is exactly
  what was constructed.
- `capture_view`: capture itself changes no read.
- `retire_view`: `RETIRE(g)` after install changes no read.
- `fold_view`: a failed Commit's `FOLD` of `g` into `g+1` changes no read.

If the Branch head moved and the Commit merged upstream changes, the view
changes by design, and the theorem does not apply. Not modelled: the
orphan area (generation `-1`), byte cutoffs inside a cell, `BaseSource`
staleness, and a retained capture after a lost reply.

## #5: Delta chains stay decodable

- `good_applyAll`: after any valid run of Saves and compactions, every
  record's base is a registered winner of the same role, with an older
  stamp, and the chain depth is at most the role's cap.
- `decode_ok`: the read's guarded walk (`delta/read.rs:243-270`) reaches a
  FULL root without tripping the cycle, depth, role or missing-base guard,
  and visits `depth + 1 ≤ cap + 1` records.
- `compact_keeps_record`: compaction changes no record, base or depth.

Rules the model assumes, each from the code: a base is a registered winner
of the same role (`save/select.rs` publishes pending bases first); winners
are insert-only and first wins; `trial_base` keeps `depth(base) + 1 ≤ cap`;
compaction keeps every base of a kept record and copies records byte for
byte. The caps are fixed (the Store policy is frozen). Bases can sit in a
higher `pack_id` than the record that uses them, so the proof orders
records by registration, not by pack.

---

## Not covered anywhere

- **Termination** of the GC driver. The proofs are of partial correctness.
- **`remove_file`.** The path-aware walk is not modelled.
- **The Rust object walk**, beyond the property it must meet (`MarkOk`).
- **Crash safety of the compaction install** (#3 in the original list).
- **Concurrency between processes.** Each model treats one transaction as
  one atomic step. Interleavings are better checked with TLA+ or Quint.
- **Fidelity.** The SQL and Rust were translated by hand. Nothing checks
  that the models stay in step with the code. Using `run`, `walk` and
  `view` as test oracles against the real code would close that gap.

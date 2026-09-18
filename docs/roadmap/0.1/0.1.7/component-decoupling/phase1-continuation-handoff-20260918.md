# Phase 1 continuation handoff (2026-09-18): the last five items

> **Status:** Executable assignment. Phase 1 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) is **11 of its 16
> items done** (`P1-1..P1-6`, `P1-9..P1-12`, `P1-16`) plus all four prerequisites
> (`C1`, `V1`–`V3`) and a fifth (`V4`) added during execution. **Five items remain:
> P1-8, P1-7, P1-14, P1-13, P1-15** — none started. Your job is to land those five
> and then run the closing pass. Phase 2 and the parked register are **not yours**.
>
> **How you work (owner instruction, unchanged): you are a SINGLE agent in DeepSeek
> Harness.** Do not launch or delegate to any subagent, and do not invoke any
> external coding-agent CLI. Your verification is **author-verification** and must
> be labelled as such; the honest evidence is the reproducible receipt (a named
> tree + a command + its output any reader can re-run), never your word.
>
> **Read first, in this order:** [`AGENTS.md`](../../../../../AGENTS.md) ·
> [`core/AGENTS.md`](../../../../../core/AGENTS.md) · the plan
> [`phase1-implementation-plan-20260917.md`](phase1-implementation-plan-20260917.md)
> (§1 the checklist, §2 the landing order, §3 the test strategy, **§4b the landed
> state**) · the predecessor handoff
> [`phase1-terminal-handoff-20260917.md`](phase1-terminal-handoff-20260917.md)
> (the loop, the invariants, the closing pass — still normative) · the four
> planning reports under
> [`../evidence/phase1-planning-20260917T230000Z/`](../evidence/phase1-planning-20260917T230000Z/)
> · the execution evidence
> [`../evidence/phase1-execution-20260918T090000Z/`](../evidence/phase1-execution-20260918T090000Z/)
> (`CONTRACT.md`, `ROUND-README.md`, `collect.py`, `rounds/`) · then `gh issue view 178`.

---

## 1. What is already done (do not redo, do not "improve")

| | |
| --- | --- |
| Landed | `C1` `7447f87d9` · `V1` `2b5e27e65` · `V2` `582dea9dd` · `V3` `aefcd95a5` · `V4` `8efdd9291` · `P1-2` `9ec299f13` · `P1-1` `32eda6f29` · `P1-3` `ff4d6d328` · `P1-4` `bfb01f262` · `P1-9` `d42cd969e` · `P1-5` `70dc75836` · `P1-12` `9b4eff169` · `P1-16` `84ca5c851` · `P1-10` `8327f87bb` · `P1-6` `360431d10` |
| Tree at handoff | `4a1b57248` (docs) / `360431d10` (product), pushed, clean |
| Checks at handoff | all eight exit 0; core workspace **452 passed / 0 failed**; sealed-oracle parity 34/34 green and unchanged |
| Production LOC | 18,792 → 19,019 (+227), core only; reference `crates/` unchanged at 65,417 |
| Evidence | 16 round directories (one is a diagnostic arm), 15 receipts, 15 `verify-*.md`; every box on #178 links its receipt |

**The anchor values you start from** (measured on `360431d10`, `rounds/p1-6/after/`):

```text
D26 order.forced64  spilled 3968 rows_read 25809 rows_written 25760 runs_created 124
                    merges 61 peak_run_bytes 568320 peak_live_runs 5 peak_backing 568320
                    dir_pages_read 17 ino_pages_read 81 dir_scratch 376158 ino_scratch 709396
                    objects_read 98 read_waves 5 bytes_read 400354 rows_touched 4001
D25 order.default   spilled 0 rows_read 0 rows_written 0 runs_created 0 merges 0
D27 edit_timing_c1  nodes_read 9   edit_nodes_read 10  root b6dca354…
M2  --case delete   nodes_read 4   edit_nodes_read 7   root 7d3eb265…
M3  --case shrink   nodes_read 11  edit_nodes_read 0   root 4a45d246…
M1  edit_memory_probe  peak_delta_bytes 262328 (= 4n + 184, n = 65536)
D21 edits.pipeline.chunked  readback connection opens 1
D2  c1.directory-update  validation: objects 21 waves 2 inode_demands 21 inode_pages 2
```

---

## 2. The five items, in the order you must land them

The order is load-bearing: P1-8 → P1-7 → P1-14 (all three rewrite `apply_edits`'s
assembly path and build on each other), then P1-13 → P1-15 (the ordering pair;
P1-13's policy is what P1-15's restart search is measured against). One item, one
commit, one variable, its own receipt and `verify-<item>.md`.

### 2.1 P1-8 — one ordered Retain cursor

* **Target:** retain segments `O(R·h)` → `O(h + shared)` provider demands.
* **Anchor:** M3 (`edit_timing_c1 --case shrink`) `nodes_read` **11** (the only
  available shape with a chunked base → whole-file result).
* **Where:** `core/crates/layerfs-content/src/file/edit/apply.rs`
  (`assemble_inner`; each `Segment::Retain` calls `view.read_range(...)` →
  `mapping::read_range` → `traverse` from the root every time) and
  `core/crates/layerfs-content/src/file/mapping/read.rs` (`traverse`, `Frontier`).
  `PlanReader` in `apply.rs` is the structural precedent.
* **Sketch (plan §P1-8):** build one cursor per `assemble_inner` when
  `view.file_state()` is `Some(state)` (whole-file bases keep the slice path):
  `FileState` + the per-level frontier **retained across segments** + one `Wave`.
  `read_segment(range, sink, scope)` asserts ascending ranges, validates
  `range.end <= logical_len` per segment, and resumes the sweep.
* **The one real trap:** `traverse`'s early `finished` break drops entries after
  the active range's end. The cursor must keep not-yet-visited siblings across the
  segment gap and filter **lazily** against the active range — never truncate the
  frontier, or later ranges lose their path.
* **Tests:** new (a) `retained_segments_share_one_descent` — a chunked base with
  two retain segments separated by a replacement (the large→small shape): with a
  `Counted` provider the mapping root is demanded **exactly once** for the whole
  assembly (today once per segment); (b)
  `straddling_payload_demanded_once_across_segments` — a payload straddling two
  retain segments is demanded once (today twice). Stay green:
  `edit_transitions.rs` (all four families, incl. `to-empty`'s exact
  `acquired.len() == 1`), `edit_single`, `edit_model`, `edit_reference`,
  `object_identity`, `fixture_seal`, `filesystem_reference`.
* **Receipt:** M3 `nodes_read` 11 → lower by ≈ (R−1)·h; **D27 stays 9** (negative
  control); `edited_root` and bytes identical; elapsed diagnostic only.

### 2.2 P1-7 — the comparing cursor + shared page memo

* **Land the corrected design, not the register's literal one.** The fused descent
  is REJECTED (plan §"Correction (2026-09-17)", commits `35f5229a6`/`4a86107fc`):
  `replace_chunked` publishes finalized nodes during the split/concat descent, so
  fusing comparison in would publish before a later segment's Equal verdict is
  known, breaking the zero-emission contract pinned at `edit_noop.rs:82`.
* **Design:** comparison stays its own pass, but becomes **one ordered cursor**
  (the `PlanReader` shape over the chunked `FileState`) whose page demands are
  **memo-served for the construction pass**. The memo is `id → canonical bytes`,
  **consulted by `load_node` before the increment/demand** so `nodes_read` stays
  honest; page-capped with evict-all; chunked bases only; one `apply_edits`
  lifetime; **cross-pass only** (the cursor's own frontier already prevents
  intra-pass re-demands).
* **Anchor:** D27 `nodes_read` **9** → ≈ 7 (plus P1-6's −0..2 where it fires).
* **Must not change:** the `edit.compare` child scope survives; `edit_noop`'s
  verdict **and** its zero-emission pin; `edit_timing.rs`'s scope pins.
* **Risk to pin deliberately:** error ordering can move (a memo hit answers a
  demand that would have re-read a page); no test pins it today — say so.

### 2.3 P1-14 — assemble the whole-file edit into one pre-sized buffer

* **Target:** memory peak ~3n → ~n + O(1); **gate: M1 `peak_delta_bytes`
  262,328 → ≈ 131,118** (the handoff's "≤ ~2n"). Note the plan's ~3n estimate
  missed `FileView`'s own copy of the base canonical (n+23); the measured before
  is **4n + 184**, so the after is ≈ 2n, not n.
* **Where:** `apply.rs` `assemble_inner` (WholeFile arm) + a new helper beside
  `encode_whole_file` in `file/content.rs`, e.g.
  `begin_whole_file_object(capacities, payload_len)`: both capacity checks with
  today's `what` strings and limits, then
  `try_reserve_exact(canonical_len(WHOLE_VALUE_HEADER + payload_len))` and write
  the 13-byte envelope + 10-byte value header. The pre-sized buffer **is** the
  sink; the final length check becomes
  `canonical.len() == HEADER_LEN + VALUE_LEN_BYTES + WHOLE_VALUE_HEADER + final_len`;
  `FinalizedObject::new` **moves** it. `encode_whole_file` stays for
  `construct_bytes`.
* **Do not:** charge `EditCounters` on this route — `edit_transitions.rs:771`
  pins `EditCounters::default()` there. Note in the receipt that the timing tree
  loses the `content.encode` child on this route (no test pins it there).
* **Tests:** new `whole_file_edit_emits_the_reference_bytes` (the emitted object
  equals `encode_whole_file(capacities, &expected)` byte for byte) + the probe as
  the measured gate. Parity: `edits.c1.small` root + 65,559 canonical bytes;
  `edits.pipeline.small` readback byte-for-byte.

### 2.4 P1-13 — merge fanout 4 (one-run-per-tier multiway cascade)

* **Target:** `rows_written` `O(r·log₂(r/P))` → `O(r·log₄(r/P))`; **D26
  prediction: `rows_written` 25,760 → ~12–15k, `merges` 61 → ~18–24** (both still
  at their pre-item values — your anchor is intact).
* **Where:** `core/crates/layerfs-content/src/filesystem/references/merge.rs`
  (tier policy) and `runs.rs` (`spill`, `levels`). Design **(a)** is the plan's
  recommendation; (b) (true size-tiered) rewrites `find`/scan ownership for
  ~+150 lines.
* **Open owner question to flag, not to decide silently:** the 5×16 KiB live merge
  buffers sit **outside the ownership account**; do not change the accounting to
  make a number look better — state it in the receipt and on #178.
* **Tests:** the plan's exact in-test counter assertion (`merges == 4` for 16
  single-row spills, today 15) + the ordering suites + threshold parity. P1-5's
  targeted scan reset means the per-tier scan invariant must survive your tier
  changes (`filesystem_ordering::a_spill_keeps_the_scans_of_tiers_above_its_level`
  is the guard).

### 2.5 P1-15 — hybrid binary-search-on-restart

* **Target:** restart lookup `O(position ≤ 2,048 rows)` → ~11 probes, each charging
  `rows_read`.
* **Where:** `references/runs.rs` (`LookupScan`, `find`).
* **Do not** replace the sequential resume: a **pure** binary search worsens the
  pinned one-pass ascending sweep (`filesystem_ordering_scan.rs` asserts
  `sweep_reads == total`). Binary-search only when the request is **behind** the
  cursor (`scan.resume` exists and `serial < resume`).
* **Gate on READS, not calls:** a random 96-byte `read_at` can defeat the buffered
  read, so the call count may rise while the rows read fall — the receipt must show
  `rows_read`, and the zero-allocation lookup test must stay green.
* **Anchor:** the D26 restart shape (`rows_read` 25,809 today).

---

## 3. The loop, per item (the same one the landed items used)

```text
ROUND
  1  implement it yourself, exactly per its plan section + the corrections above
  2  one commit: product change + its new tests + the pinned-test update only if
     pre-authorized + the architecture-doc update in the SAME commit + the LOC line
  3  run the eight checks (§4) and report every exit code and the test counts
  4  collect the receipt: before = the previous round's `after/` arm (same commit,
     cited by path), after = this item's commit; into `rounds/<item>/after/`
  5  write `verify-<item>.md` yourself, labelled author-verified, with the
     falsification answers and an explicit UNVERIFIED list
  6  tick the #178 box with its receipt link; if a falsification check failed,
     remedy and re-verify before ticking
  7  after the fifth item: the closing pass (§5), then the plan's final counter
     table, then the summary on #178, then STOP
```

### Collecting a round (the mechanics that already exist)

```sh
cd docs/roadmap/0.1/0.1.7/evidence/phase1-execution-20260918T090000Z
LAYERFS_CONSTRUCTION_WORKERS=1 python3 collect.py <round> after all     # ~12 s + builds
```

* Sets: `build c1 fs edits order extra v2 c2 diag`; `all` runs them all.
  `v2` = M1/M2/M3 (the edit vehicles), `extra` = D27, `order` = D25/D26,
  `diag` = X1/X2 (the labelled determinism repeats).
* `after/artifacts.txt` records the sha256 of every binary the arm ran. **The
  probe client is rebuilt by `B2` into `client/target/release/phase0client`** — the
  driver was fixed during Phase 1 (`ROUND-README.md` correction 2) because a stale
  `/tmp` copy once measured C1's `order` rows against a pre-C1 tree. Never run a
  client you did not just build; the hash in `artifacts.txt` is your proof.
* A fresh `--output` path per run; the driver refuses to reuse one. `measure_edits`
  **creates its own output directory** — do not pre-create it.
* Compare arms with a counter-only diff: strip `elapsed_ns` and the timing-tree
  lines, keep every counter line (including `validation:`, `edit_nodes_read:`,
  `readback connection opens:`), and diff the two arms' logs. A movement anywhere
  you did not predict is the finding.

## 4. The eight checks (every commit; report exit codes and counts)

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

Production LOC per commit: `tools/production_loc.py` over the **first parent**
(`git archive <parent> | tar -x -C /tmp/…`) and the staged tree, with the
per-file diff, in the commit message as
`Production LOC: <before> -> <after> (delta <signed>)` plus scope and method.

## 5. Invariants you must not break (they have held for 15 commits)

1. **Parity is absolute.** Persisted canonical bytes are identical before and
   after every item; the 34-test sealed-oracle set stays green and **unchanged**.
   The only pre-authorized pinned updates were C1's counter semantics and P1-1's
   `peak_wave() <= 32 → 256` width bound — **both already spent**. If your item
   needs another pinned value changed, the item is wrong: rework it. (Twice a
   landed item had to *generalize* a cross-subsystem assertion instead — C1
   receipt §10, P1-1 receipt §10; that is the pattern to follow if you hit it.)
2. **One item, one commit, one variable.** A found bug beside your item is
   reported on #178, not fixed in the item's commit.
3. **Gates are deterministic work counters, never elapsed.** Elapsed is
   diagnostic-only and labelled so in every receipt.
4. **Never mark a box on your own word** — the receipt must be reproducible
   (named tree + command + output) and the falsification list answered.
5. **Never silently drop an item.** A refuted plan target is recorded as a
   refutation; an item that turns out pointless is `measured-and-declined` with
   its receipt and its reason.

## 6. Traps this session hit (so you do not)

* **`git add -A` sweeps untracked files.** `core/docs/benchmark/fs-bench-pro-storage-content/{c1,c2}-families.md`
  (issue #182, Stage 6) are untracked in the tree and are **not** yours. Add
  explicit paths, and check `git show --stat` before every commit.
* **`gh issue edit` can time out mid-write.** After every edit, re-fetch the body
  and check it: 16 `P1-` boxes, 4 prerequisite boxes, no orphaned continuation
  lines, and the tick count you expect. A timed-out edit once truncated a line.
* **The scan suite counts process-global allocations.**
  `filesystem_ordering_scan.rs::lookups_allocate_nothing_after_the_tiers_are_built`
  fails if another test in that binary allocates concurrently — put an allocating
  test in `filesystem_ordering.rs` instead (P1-5's test lives there for this reason).
* **`edit_timing_c1` needs `--case <default|delete|shrink>`** (no argument = the
  frozen D27 shape; the `case:` line is additive).
* **Plan sketches have been wrong three times**: P1-5's `scans.truncate(level + 1)`
  is backwards (it keeps the replaced tiers); P1-1's `list_after` batching
  over-reads a bounded listing (declined); P1-9's printed `nodes_read` cannot move
  because it is the provider-demand count (V4 added). **Measure the sketch's
  premise before implementing it.**
* **A stale client invalidated a whole round once.** Rebuild, and record hashes.

## 7. The closing pass (only after the fifth box is ticked)

Write `closing-pass.md` in the final round's evidence directory, answering all six
checks of the terminal handoff §5: boxes ↔ receipts; a **full frozen-set re-run on
the final tree** tabulated against each item's plan prediction (a refuted
prediction is recorded, not explained away); the parity audit
(`git diff <phase1-start>..<final> -- '*tests*'` contains only the two
pre-authorized pin updates and the new tests); the commit audit (single-variable,
LOC disclosure re-run on a sample of three, architecture-doc update present); the
gate audit (no elapsed quoted as a gate, no counter "fixed" outside C1, every
verification file labelled author-verified); and the scope audit (no Phase 2
change, no parked item, no default change, no worker/timeout/cache tuning). Then
update the plan with the final counter table, post the summary on #178, and stop.

## 8. Out of scope (do not touch)

Phase 2 (`P2-1..P2-8`), the parked register (O4, O5, branch-row summaries,
pack-BLOB append rewrite, membership single-hash, persisted pool-index cursor),
the pending-ceiling default (P1-16 documents it; **re-defaulting is an owner
ruling**), any worker count, timeout or cache policy, and the two untracked Stage-6
documents.

# Handoff prompt — #205 the save path's growing cost: attribute it in time, then fix what it names

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after PR #206 (`faec3a83b`). This prompt carries existing
> measurements and a bounded next investigation; it is not a new measurement and not
> a release claim.

## Mission and decision rule

Continue [#205](https://github.com/Ephemeral-AI-Lab/layerfs/issues/205), which was
split out of [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190) after the
read-path depth term was removed and measured. The question is the **save path**:
`storage.accept_loop` is 50–55 % of a `history.*` row's operation, its per-object
cost grows as history, commits and stored objects accumulate, and **it has no time
attribution at all**.

The order is fixed by the owner's standing rule — confirm the mechanism from
measurements before writing product code:

1. **Build the instrument** (Step 1 below) and publish a split of `accept_loop` in
   *seconds*: resolution / FULL encode / delta encode / group codec / pack placement /
   SQL / commit.
2. **Then** pick exactly one treatment from what the split names, pre-register it,
   and measure it against matched arms.

Do not begin by enlarging a cache, adding a worker, changing the format, or rewriting
the pipeline. Do not pick a treatment from the correlations below — they are
correlations over **counts**, and L52 already had to correct the label one of them
carried.

Owner rulings persist (all still in force):

- **One second of stride10 operation reduction is worthwhile.** Do not dismiss a
  credible one-second improvement; do not chase sub-second ones.
- **No automatic cache growth.** Reuse with a correctness (invalidation) contract is
  allowed; a larger retained-bytes bound is an owner decision.
- **A small allocated-storage overage is acceptable when accompanied by good time
  reduction.** Report both axes; never relabel a historical miss as a pass.
- **Do not manufacture a cache claim.** No invented cold stance, no pre-touching
  measured inputs, no moving required reads into setup, no shrinking a selection.
- **Never hand-edit a pin.** `shared/pin_expected.py` is the only way a constant
  freezes.
- **Preserve authentication, bounds, error/visibility behaviour and the single
  construction worker.** A source difference is not a measured saving.
- **Do not close #190 or #205.** #190 needs the owner's D1–D5 qualification rulings
  and the v0.1.6 reconciliation; neither is this lane's to supply.

## 1. What is already measured (do not re-derive it)

Retained on the branch `codex/190-pooled-scope` (PR
[#206](https://github.com/Ephemeral-AI-Lab/layerfs/pull/206)), evidence under
`docs/roadmap/0.1/0.1.7/evidence/`:

| quantity | value | source |
|---|---|---|
| read-path depth term, removed and retained | stride10 operation 19.500 → 16.746 s; stride3 48.813 → 37.066 s; stride1 146.178 → **105.726 s** | L49/L51 |
| scaling curve, cost per changed byte at 17 / 53 / 157 versions | 52.3 / 68.6 / 85.0 M → 44.8 / 51.6 / **61.5 M** ns/MB; slope **−49 %** per e-fold; finest/coarsest 1.625× → 1.372× | L51 |
| `storage.accept_loop` (one span) | **9.134 s** stride10, **18.441 s** stride3; inside the state total at stride1 | L50 |
| `storage.accept_loop` per object written, early → late | 93.6 → 212.5 µs (**2.27×**) stride10 | L50 |
| `storage.finish` (final seal + watermark commit) | 0.391 s stride10, 1.601 s stride3 | L50 |
| pack-append pattern + its forced commits | **synthetic bound ≤ 1.03 s** of stride10's accept (4.43 GB of BLOB rewrite for 45.3 MB stored, 97.8×; commits every ~40 appends because `write_pack` charges the whole pack to the 4 MiB transaction budget) | L50 |
| group codec, level 19 against level 1 | level 19 cost **+1.685 s** stride10 / **+2.463 s** stride3 more, for 262 KB / 398 KB fewer bytes | L40 |
| C1 content construction | harness `content` span 1.61 s stride10 / 3.05 s stride3, plus unnamed time inside `filesystem` | L50 |
| resolution events per object written, early → late | reuse **0.112 → 1.032 (9.21×)**; trials 0.677 → 0.745 (1.10×) | L52 |
| chain objects per resolution event | 2.59 → 3.56 (1.37×) | L52 |
| accumulated at stride1 | 97,788 objects written, 648,299 chain objects, 653.7 MB encoded read, 84,473 appends, 1,797 commits, 855 packs | L50/L51 |
| chain depth cap | `whole_file_delta_max_depth = 8` **binds** (per-state max 7–8 in nearly every late state) | L51 |

## 2. The blocker, stated exactly

`storage.accept_loop` is **one span** around `for id { operation.accept(object) }`.
There is no span or counter for the *cost* of selection, base resolution, delta
encoding, compression, group framing, pack placement, row inserts or commits — in the
product or in the harness. Therefore:

- **compression, chunking and CAS placement have no cost counter at all**, only
  operation counts. A flat count cannot exonerate something that is not measured, and
  the correlation in L50/L51 (elapsed vs `save.chain.objects`, +0.947 at stride1) is
  a correlation over counts;
- the two numbers that exist for those buckets are the **synthetic** append bound and
  **L40's** group-codec *delta*; neither is a split of the save.

## 3. Step 1 — the instrument

**Seven disjoint nanosecond buckets, accumulated, published per state.** Design that
was worked out and then deliberately not implemented (the owner asked for this prompt
instead), so it is a starting point, not a verified recipe:

`SaveProfile` in `cas/owner.rs`: `resolve_ns`, `full_ns`, `delta_ns`, `group_ns`,
`place_ns`, `sql_ns`, `commit_ns`, plus `accumulate`, `total_ns`, and
`pub(crate) fn charge(slot: &mut u64, started: Instant)`.

| bucket | call site to wrap |
|---|---|
| `resolve_ns` | `encoding/delta/select.rs::eligible` (the `depth_of` edge walk), `::acquire` (`resolve_dependency`), `cas/membership.rs::stored_canonical` (`owner.resolve_location`) |
| `full_ns` | `select.rs::select` around `encode_full` |
| `delta_ns` | `select.rs::select` around `encode_prefix` |
| `group_ns` | `cas/placement.rs::seal_group` around `build_group` |
| `place_ns` | `seal_group` around `LanePlacement::select_many` |
| `sql_ns` | `seal_group` around `write::insert_objects`; `placement.rs::write_pack` around `write::insert_pack`/`append_pack`; `cas/pool_lane.rs::write_value_groups` around `sqlite::pool::insert_group` |
| `commit_ns` | `cas/lifecycle.rs::maybe_commit` and `finish_inner` around `write::commit` |

Plumbing: `OutcomeCounters.profile` and `MutationOwner.profile`; `SaveOutcome.profile`
plus the `From<OutcomeCounters>` arm in `cas/store.rs`; `SelectInput.profile:
&'a mut SaveProfile`, set where `SelectInput` is built in `cas/selection.rs`. In
`select.rs::eligible`, `ChainBases::new(input.packs)` and `input.depths` are disjoint
field reborrows of `&mut SelectInput`, so the charge must go **after** the walk
returns, not inside the closure.

Harness side: `SaveTotals` (`ops/history.rs`) gains the seven fields, `add()`
accumulates `saved.profile`, and `rows()` publishes them with unit `ns`, which makes
every state emit `history.state.<n>.save.*_ns`. That is the same accumulator the
existing `save.*` counters already use, so no new mechanism is needed.

**Why aggregate and not a span per object.** A stride1 row accepts ~10^5 objects; a
timer node per object is the node explosion the driver already refuses above 53
states. Seven `Instant::now()` pairs per object is ~10^6 clock reads, tens of
milliseconds against a 105.7 s operation — declare that arithmetic in the receipt.

**Bound the instrument before believing it.** Compare the instrumented candidate's
operation against the **uninstrumented** samples already on disk (16.908 / 36.810 /
105.726 s, L50/L51). The harness-publishing effect alone is already bounded by
L49 → L50 (16.746 → 16.908 s across a harness change). Instrument one arm for the
attribution, say so, and keep any *treatment* arm unprofiled.

**Then, and only if the split names it**, treat one thing. If the split leaves a large
remainder, the honest next step is another bucket, not a treatment.

## 4. The candidate levers, with what is already known

- **A. Do not re-verify what this save already verified.** Every occurrence that finds
  an existing row reconstructs that row's chain to compare stored bytes with offered
  bytes (`membership::stored_canonical` → `MutationOwner::resolve_location`) — 121,301
  occurrences at stride1 for 97,788 objects written, growing **9.21×** per object
  written. Rows are immutable within a save and the offered bytes carry the same
  identity, which is the hash of those bytes, so "already reconstructed and compared
  equal in this operation" is a reusable proof. **Missing: how many occurrences repeat
  an id verified earlier in the same operation** — count it first. A wave-scoped memo
  (bounded by the existing 512-object batch) is free; a whole-operation memo is
  retained bytes and therefore an owner decision.
- **B. Shorter chains — a policy experiment, not code.** `whole_file_delta_max_depth
  = 8` binds. Create the Store with a lower cap (0 disables delta entirely, which is
  the cleanest ablation of the whole delta machinery) and re-measure; report **both**
  axes as ruling 7 requires. Watch out: a different policy changes the work counters,
  so the harness's pinned O3 gates may report FAIL on an ablation arm — declare that
  as expected rather than promoting or suppressing it, and check what
  `tests/golden/expected.tsv` pins for `history.*` before choosing the arm.
- **C. One walk instead of two.** `eligible()` walks a chain's edges to measure depth,
  then `acquire()` walks the same chain again to rebuild it. The eligibility walk is
  counted nowhere; `resolve_ns` would lump it with acquisition, so give it its own
  bucket if C is to be sized. Low risk if it is material.
- **Not options:** parallelism (single construction worker); skipping authentication
  or the exact byte comparison; shrinking a selection or the corpus; enlarging the
  4 MiB / 512 KiB caches without an owner decision; promoting the diagnostic caps.

## 5. Measurement and resource protocol

Read [`AGENTS.md`](../../../../AGENTS.md), [`core/AGENTS.md`](../../../../core/AGENTS.md),
[benchmark rules](../../../general/benchmark_rules.md),
[benchmark agent rules](../../../../benchmark/AGENTS.md) and the
[quickstart](../../../../benchmark/fs-bench-pro/QUICKSTART.md) before work.

- Serialize builds, tests, performance and verification; hold the global
  `flock(LOCK_EX|LOCK_NB)` on the resolved
  **`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`**
  for the whole resource command. A held lock means defer. The harness private
  `.measurement.lock` is a different protocol — never open it with append+flock.
- Quiet preflight: no named competing `cargo`/`rustc`/`fs-bench` process and ≥70 % CPU
  idle on the second of two one-second observations. A busy preflight consumes no
  sample and is retained as a deferral.
- One sample per case per arm. No n3, no best-of, no re-run for a better number. Fresh
  `--output` per run; the collector must not pre-create the child's `--out` directory;
  receipts are append-only; failures and deferrals stay on disk.
- The established **diagnostic** caps: 120 s stride10, 240 s stride3, 720 s stride1
  (the last declared by the retained campaign before its first stride1 sample). Never
  promoted to admission budgets, never enlarged after a miss.
- stride1 runs the **ordinary recording** — the driver refuses per-state phase nodes
  above 53 states — so its `filesystem`/`accept_loop` spans sit inside the state
  total. Any comparison must compute each row's figures the same way on both sides.
- Rust 1.85.1 and `--locked`. No third-party edits, vendoring or patches. No CI, no
  `tools/preflight.sh`. Harness checks for a harness change: `cargo +1.85.1 test
  --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked`
  (117 tests) and a release build. Core checks (`test`/`clippy`/`fmt --manifest-path
  core/Cargo.toml --locked`, `core/tools/check_product_boundary.py` and its
  self-tests) are required for any **product** change. The harness carries **157
  pre-existing rustfmt hunks** (`ops/history.rs` alone carries 20): format only your
  own added lines, and note that reformatting moves the built binary because Rust
  embeds `panic!` locations — a reformat after a sample invalidates that sample's
  recorded binary identity.

## 6. Source, corpus and artifact custody

- Worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope`, branch
  `codex/190-pooled-scope`, tip `faec3a83b` (or the merge of PR #206). Record the
  actual HEAD.
- The L49/L50 **baseline arm** worktree is
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-baseline` (detached at `c4f757514`,
  harness patch applied, deliberately dirty over the harness only). Its binary is
  `079ae5e0a5b59d4918d7861aa1227608686d87c6053e8badeeb84403d29dc6db`; the candidate
  binary is `1822ec21a9d2a362698a5ca2f6dac1c2b68b517f1a8e108bc4a261845c7e9f99`, from
  product source `43f06aef7` with a clean seal. Both carry harness source seal
  `264e88c5d379dac36193b5a2182ec479ead061c0db38e5b9b5d718a337deb566`.
- Corpus `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`; manifest SHA256
  `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip
  `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. Stride10 `range(1,158,10) ∪ {157}`
  (17); stride3 `range(1,158,3)` (53); stride1 157 states. Confirm from
  `shared/history_corpus.py`; do not shrink selections.
- Saved Stores are excluded from Git (`/runs/*/raw/sample.sqlite`). The recorded
  constants are stride10 `4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487`,
  stride3 `f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e`, stride1
  `1635cf7bbbabdc7f9e4af81ac9c6b6a88f45be35b4100dda0f52394c85dcf418`.
- Existing evidence: `.../stage-6-history-190-scope-20260920T062421Z/` (L49),
  `.../stage-6-history-190-save-20260920T070711Z/` (L50–L52). Ledger entries L40,
  L47–L52 in `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`. The
  next entry is **L53**.

## 7. Evidence that must not be misused

- **The correlations are not an attribution.** L50/L51 published elapsed-vs-count
  correlations (+0.947 chain objects, +0.805 reuse, +0.409 trials at stride1); L52
  corrected the label they carried. They motivate the instrument; they do not
  substitute for it.
- **The 1.03 s append bound is synthetic**, replayed against a copy of a retained
  Store under the product's pragma profile — not the product's measured cost.
- **L40's group-codec numbers are a delta** (level 19 against level 1), not the
  current level-1 absolute, and payload level 3 has never been isolated.
- **All `history.*` rows are diagnostics**: admission `INELIGIBLE`, O3 `INCOMPLETE`,
  and the complete commands (36.072 / 61.902 / 158.464 s for the candidate arm) fit no frozen budget class, so
  every budget class is `NOT_RUN`. Never present one as admission evidence.
- **`pin_expected.counters_of` pins only `status == "PASS"` receipts**, so a row that
  cannot pass cannot be pinned. Do not hand-edit a value or bypass the gate.
- **The v0.1.6 time comparison is a scope error, not a gap** — the historical
  11,370,679,212 ns is v0.1.6's `commit_workspace_session_with_status`, so the
  "9,543,810,206 ns unmatched difference" is a whole operation minus a commit. No
  percentage, no shortfall claim.
- **The out-of-clock reconciliation gap** (~0.04–2.0 s; stride10 rows report
  `INCOMPLETE`): first execution of a freshly created executable is one measured cause
  (0.7–2.0 s before `main`); a second cause exists and is unidentified. It is outside
  the child's clock and affects no operation figure. A round that wants a reconciling
  stride10 wall should execute each freshly built binary once before the sample and
  declare it — never re-run a case for a better number.

## 8. Work organization, proof and deliverables

Keep the round finite. Produce:

1. **The instrument**, with its overhead declared and bounded against the
   uninstrumented samples of the same row.
2. **A published time split** of `storage.accept_loop` at stride1 and at least one
   coarser row — resolution / FULL / delta / group / place / SQL / commit — with the
   remainder stated as a remainder.
3. **One treatment**, pre-registered, matched arms, identity-matched sampled
   verification per identity, and equivalence: state roots and canonical inventories
   equal; Stores byte-identical for a scope/algorithm change, or both byte axes
   reported for a policy change.
4. **Measured evidence** under §5, with exact identities, fresh append-only outputs,
   and every failure and deferral retained.
5. **Required checks** for whatever tree is changed, reported exactly as run or not
   run.
6. **A report and an append-only ledger entry** (L53 or later), a #205 update, a #190
   update if the read path is touched, and exact first-parent/staged production LOC
   for each commit with reference/core subtotals.
7. **Do not close #190 or #205.**

## 9. Do not

- Do not treat before the split exists.
- Do not grow a cache without an owner decision; do not remove authentication or the
  exact reuse comparison; do not shrink a selection, the corpus or a workload.
- Do not add a second worker or helper lane; do not change the format, the schema or
  the persisted policy defaults without an owner ruling.
- Do not promote, enlarge or shrink a diagnostic cap; do not hand-edit a pin; do not
  re-run a case for a better number.
- Do not sweep the harness's 157 pre-existing format hunks into a commit, and do not
  reformat a file after a sample that its binary identity belongs to.
- Do not use `tools/preflight.sh` or restore CI.

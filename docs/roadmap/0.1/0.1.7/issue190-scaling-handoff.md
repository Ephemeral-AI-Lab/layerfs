# Handoff prompt — #190 retained-history scaling: the depth term and the pack/decoded-cache scope

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after PR #204 (`80e6b9868`). This prompt contains existing
> evidence and a bounded next investigation; it is not a new measurement or a release
> claim.

## Mission and decision rule

Continue [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190) from the
three-row measurement at `80e6b9868`. The qualification question is parked and
owner-blocked (see §5); **the product question is now the depth term**, and it is
bounded: per-byte cost rises with chain depth even at constant changed volume, the
counters localise it to fewer-but-larger authenticated reads, and the pre-registered
treatment for it already exists from L42's Priority B — pack/decoded-cache **scope**
with a Store-write invalidation contract.

Do not begin by enlarging a cache, adding a worker, changing the format, or rewriting
the pipeline. Begin by confirming the mechanism the counters point at, then test one
scope change under the existing bounds.

Owner rulings persist:

- **One second of stride10 operation reduction is worthwhile.** Do not dismiss a credible
  one-second improvement; do not chase sub-second ones.
- **No automatic cache growth.** Reuse with a correctness (invalidation) contract is
  allowed; a larger retained-bytes bound is an owner decision, and any such proposal
  must be quoted against the allocation targets in §4.
- **A small allocated-storage overage is acceptable when accompanied by good time
  reduction.** Report both; never relabel a historical miss as a pass.
- **Do not manufacture a cache claim.** No invented cold stance, no pre-touching
  measured inputs, no moving required reads into setup, no shrinking a selection to fit
  a budget.
- **Never hand-edit a pin.** `shared/pin_expected.py` is the only way a constant becomes
  frozen.
- **Do not communicate with #192 or other independent user-owned tasks.** Use the global
  locks directly. Do not create a new task merely to run this prompt.
- **Do not close #190.**
- Preserve authentication, bounds, error/visibility behaviour and the single
  construction worker. A source difference is not a measured saving.

## 1. Preserve these completed improvements

| PR | Retained change | Production LOC delta |
|---|---|---:|
| [#194](https://github.com/Ephemeral-AI-Lab/layerfs/pull/194) | Batched parent lookups, bounded authenticated base-record reuse | +49 |
| [#195](https://github.com/Ephemeral-AI-Lab/layerfs/pull/195) | Pooled physical-group telemetry; reuse through the existing 512 KiB decoded-group cache | +141 |
| [#196](https://github.com/Ephemeral-AI-Lab/layerfs/pull/196) | Existing bounded prepared-statement cache for catalogue `group_for` | −1 |
| [#199](https://github.com/Ephemeral-AI-Lab/layerfs/pull/199) | Group compression level 19 → 1 (payload level 3 unchanged) | 0 |
| [#202](https://github.com/Ephemeral-AI-Lab/layerfs/pull/202) | Ordinal-ordered pooled-leaf resolution: one catalogue statement per distinct covering group | +3 |

Product total at this checkpoint: **85,725 production LOC** — reference `crates/` 65,417;
replacement `core/` 20,308. Recount exact snapshots for any later commit.

[PR #197](https://github.com/Ephemeral-AI-Lab/layerfs/pull/197)'s New-row filter stays
archived and reverted. Do not reintroduce it.

Harness-side additions made since (benchmark code, **not** production LOC): the corpus
reading is now declared as preparation and two corpus-axis diagnostics are published
(PR [#204](https://github.com/Ephemeral-AI-Lab/layerfs/pull/204)). Preserve them; they
are what makes the budget figure and the cache stance honest.

## 2. Latest measured starting point

Retained campaign:
[`stage-6-history-190-corpus-phase-20260920T054215Z`](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/README.md),
[analysis](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/analysis.txt),
[depth term](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/depth-term.txt),
[scaling](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/scaling.txt),
[stride1 axes](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/stride1-analysis.txt),
[trend](evidence/stage-6-history-190-corpus-phase-20260920T054215Z/stride-trend.txt).
All three rows are **one diagnostic sample each**, retained source, no instrumentation
patch; reconciliation PASSES for all three.

| Quantity | stride10 (17) | stride3 (53) | stride1 (157) |
|---|---:|---:|---:|
| complete command (wall) | 37.613 s | 75.121 s | 199.025 s |
| preparation (window + declared corpus) | 17.766 s | 26.033 s | 52.664 s |
| **operation (sum of named children)** | **19.739 s** | **48.976 s** | **146.178 s** |
| declared phases / reconciliation | 37.521 s / PASS | 75.038 s / PASS | 198.876 s / PASS |
| budget (15 s / 25 s) | NOT_RUN / NOT_RUN | NOT_RUN / NOT_RUN | NOT_RUN / NOT_RUN |
| changed MB | 377.4 | 713.7 | 1,719.8 |
| ns per changed MB | 52,297,706 | 68,625,861 | **84,996,709** |
| corpus probe: files / resident pages | 45,338 / 0 | 78,439 / 4,298 | 180,128 / 2,933 |

### 2.1 The scaling finding (the reason this handoff exists)

**More chunks for the same history cost more.** All three rows cover the same 157
checkpoints: operation ×7.41 while states ×9.24, and **ns per changed MB rises 63%**
from the coarsest to the finest split. Finer chunking writes more total content (a path
changed ten times inside a stride-10 span is stored once, ten times at stride 1), and
each state pays the term below.

**Inside one chain, per-state cost rises with depth at constant content.** At matched
changed volume the late half of a chain costs **1.2–2.3×** the early half; ns per
changed byte rises 1.7–2.8×; elasticity of filesystem ns/MB against ln(state) is
**+8,755,936** (stride10) and **+13,108,101** (stride3).

**It is per-unit cost, not per-byte work** (early → late, matched volume):

| quantity | stride3 7–10 MB | stride3 4–7 MB | stride10 14–22 MB |
|---|---:|---:|---:|
| provider waves per changed MB | 139.4 → 111.3 (0.80×) | 140.6 → 66.1 (0.47×) | 250.7 → 112.4 (0.45×) |
| pack fetches per wave | 2.4 → 9.7 (**4.0×**) | 2.9 → 14.0 (4.8×) | 0.5 → 2.2 (4.4×) |
| KiB copied per fetch | 34.6 → 77.6 (2.2×) | 32.6 → 42.2 (1.3×) | 52.4 → 68.9 (1.3×) |
| decoded-group cache hit rate | 0.967 → 0.898 | 0.955 → 0.822 | 0.996 → 0.980 |
| **chain edges per record call** | **0.33 → 0.36 (flat)** | 0.34 → 0.39 | 0.22 → 0.33 |
| **pack bytes copied per changed MB** | **12.1 M → 86.2 M (7.1×)** | 13.8 M → 40.0 M | 6.1 M → 17.3 M |

The read path makes **fewer, far larger calls**: for one state whose diff is 8 MB it
copies 86 MB of application bytes late against 12 MB early. Delta-chain resolution per
record is flat, so this is **not** a deeper-chain algorithm.

## 3. The two explanations, and the experiment that separates them

1. **Bounded cache against a growing working set** (the reading the counters point at).
   The pack and decoded-group caches are fixed (512 KiB / 4 MiB class bounds) while the
   Store grows, so the share of each read they serve falls. Note: only the decoded-group
   cache publishes a hit counter; the pack cache's own hit rate is **not** published, so
   this is inference from `pack_fetches`/`pack_bytes`/`hit rate`, not a measured curve.
2. **Structural read amplification** — the update path asks for more per state as the
   tree grows (bigger trees imply wider dependency closures). Not supported by the
   counters (waves/MB *fall*), but not excluded: `records per leaf request` does rise
   slightly (6.01 → 7.26; 3.55 → 5.91 on stride10).

The experiment that separates them is the treatment itself, and it is already
pre-registered by L42's Priority B: **per-leaf pack scope with a Store-write invalidation
contract, 2.1–3.9 s target on stride10, reviewed and never implemented.** If reuse scope
is the term, the treatment removes the late-chain amplification; if the term is
structural, it will not move.

Pre-registration to keep:

- two arms, **identical instrumentation in both** (use the harness's own counters; add
  none), baseline = the retained tree at `80e6b9868`, candidate = the scope treatment;
- **stride10 first** against the owner's one-second bar, then stride3 to confirm no
  regression, then stride1 if the scaling claim is being asserted;
- equivalence gate: every state root matches, canonical and value-group inventories
  match, and the saved Stores are **byte-identical between the arms** (the retained
  campaign's property — reproduce it, do not assume it);
- identity-matched separate verification per performance identity, sampled, stated as
  sampled;
- retain only if stride10 improves by ≥ 1 s with preserved correctness and unchanged
  stored bytes; a stride3 regression rejects the treatment;
- one sample per case per arm; fresh outputs; append-only receipts; failures and
  deferrals retained.

Also worth *measuring before implementing*: whether an existing bound can be reused
rather than added. The decoded-group cache already exists; the question is its scope and
its invalidation on Save, not its size.

## 4. Store bytes — the one axis where v0.1.6 is directly comparable

A v0.1.6 allocation target is a byte count of the same selection's content, not a
phase-scoped time. Owner ruling 7 makes it a gate: below is the target, above is a
finding.

| row | core apparent / allocated | v0.1.6 apparent / allocated | delta allocated |
|---|---:|---:|---:|
| stride10 | 49,324,032 / 50,249,728 | 49,315,940 / 49,344,512 | **+905,216** above |
| stride3 | 62,152,704 / 63,078,400 | 64,000,100 / 64,024,576 | **−946,176** below |
| stride1 | 80,969,728 / 84,541,440 | 82,677,860 / 83,947,520 | **+593,920** above |

On **content** the core is below the historical figure on stride3 and stride1
(−1,847,396 / −1,708,132) and +8,092 above on stride10. **Allocation is not a precise
statistic**: stride3's allocated bytes read 62,152,704 in the retained campaign and
63,078,400 now, for Stores that hash identically — 1.5% on identical content, the same
order as stride10's +905,216 and larger than stride1's +593,920. Quote the apparent
axis for content claims and the allocated axis with this movement stated.

## 5. Evidence that must not be misused

- **The v0.1.6 time comparison is a scope error, not a gap.** The historical
  **11,370,679,212 ns is v0.1.6's Commit** — the daemon's commit call, timed at
  `benchmark/fs-bench-pro/src/storage_smoke.rs:737-738`
  (`commit_workspace_session_with_status`) — and excludes the fixture install (18.6 s)
  and the workload exec (59.7 s) that the v0.1.6 campaign reported separately. The
  "9,543,810,206 ns unmatched difference" is `20,914,489,418 − 11,370,679,212`, i.e. a
  whole operation minus a commit. On the matched interval the core save interval is
  9,683,862,043 ns against the historical 11.371 s, but that comparison crosses a native
  in-process save and a container/daemon commit, one sample each: state it as **no
  demonstrated shortfall**, never as a percentage. Closing the item properly means a
  source-based phase-scope reconciliation, or a matched v0.1.6 arm — not another
  subtraction.
- **The retained binary has never been timed.** The output half is now closed by
  measurement: the uninstrumented retained source reproduces the measured candidate's
  Stores byte for byte (stride10 `4af37932aa3391b1…`, stride3 `f5c7ff5a6b4f0821…`). Its
  **timing** remains the one gap in the chain.
- **All `history.*` rows are diagnostics.** Admission `INELIGIBLE`, O3
  `INCOMPLETE` (which `runner.re_derive_pins` would report as **FAIL**), and the
  complete command fits no frozen budget class: declared 37.5 s / 75.0 s / 198.9 s
  against 15 s ordinary and 25 s declared-exception limits. Never present one as
  admission evidence.
- **The qualification gaps are owner-blocked, not engineering-blocked.** Five decisions
  are open (D1 oracle, D2 selection, D3 bootstrap, D4 admission+budget, D5 corpus-axis
  stance); see
  [`DISPOSITION.md`](evidence/stage-6-history-190-qual-20260920T052703Z/DISPOSITION.md)
  §3. `pin_expected.counters_of` pins only `status == "PASS"` receipts, so a row that
  cannot pass cannot be pinned — do not hand-edit a value or bypass the gate to break it.
- **Do not promote the diagnostic caps.** 120 s / 240 s for stride10/stride3 and the
  720 s declared for stride1 (declared *before* its first sample, by the retained
  per-state convention) are diagnostics. The verification hard budget is 60 s.
- **Recording difference, not a defect:** stride1 cannot run the per-state phase
  diagnostics (the driver refuses above 53 states), so its `filesystem` and
  `accept_loop` spans are inside the state total and unnamed.
- **The `history_corpus.PINS` `canonical_bytes`/`canonical_objects` fields are not
  compared by the corpus probe** (`runner.py:1624-1630` checks only `states`,
  `path_states`, `logical_bytes`). Confirm their semantics before treating a difference
  as drift.

## 6. Measurement and resource protocol

Read [`AGENTS.md`](../../../../AGENTS.md), [`core/AGENTS.md`](../../../../core/AGENTS.md),
[benchmark rules](../../../general/benchmark_rules.md),
[benchmark agent rules](../../../../benchmark/AGENTS.md) and the
[quickstart](../../../../benchmark/fs-bench-pro/QUICKSTART.md) before work.

- Serialize builds, tests, performance, verification and large artifact inspection.
  Acquire global `flock(LOCK_EX|LOCK_NB)` on the resolved
  **`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`**,
  deduplicating identical resolved paths, and hold the descriptors for the whole resource
  command. A held lock means defer. The harness private `.measurement.lock` is a
  **different** protocol (`shared/receipt.py::measurement_lock`, `O_CREAT|O_EXCL`); never
  open it with append+flock and never blindly delete a marker.
- Quiet preflight: no named competing `cargo`/`rustc`/`fs-bench` process and at least 70%
  CPU idle on the second of two one-second observations. A busy preflight consumes no
  sample and is retained as a deferral.
- One sample per case per arm. No n3, no best-of, no re-run for a better number. Fresh
  `--output` per run; receipts are append-only; failures and deferrals stay on disk.
- **The collector must not pre-create the child's `--out` directory** — the child refuses
  an existing one (one attempt was refused for exactly that in this round and is retained
  in `runs/refused-history-stride10-exit2/`).
- Rust 1.85.1 and `--locked`. No third-party edits, vendoring or patches. No CI, no
  retired `tools/preflight.sh`. `cargo fmt` on the harness rewrites ~121 pre-existing
  diffs in files you did not touch: format only your own files and revert the rest.
- Harness checks for any harness change: `cargo +1.85.1 test --manifest-path
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` (117 tests) and a
  release build. Its Clippy set (21 warnings, 1 denied `never_loop` error at
  `src/ops/history.rs` HEAD) is **inherited** — record it, do not claim it as a pass and
  do not sweep unrelated fixes into your commit. Core checks
  (`cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
  `core/tools/check_product_boundary.py` and its self-tests) are required for any
  **product** change and are unnecessary for harness-only or evidence-only commits —
  state which ran and which did not.

## 7. Source, corpus and artifact custody

Start from a clean isolated checkout at the tip of
`codex/190-corpus-phase-declaration` (`80e6b9868`) or the merge of PR #204, and record
the actual HEAD. The retained campaign's Stores are large and excluded from Git; the
runs in `stage-6-history-190-corpus-phase-20260920T054215Z/runs/` keep their traces,
phase files, timing trees and receipts, and the `sample.sqlite` files are ignored by
`.gitignore`. The retained `candidate2` Stores live under the *other* worktree's ignored
tree (`/Users/yifanxu/.codex/worktrees/history-data-access/layerfs/.../runs/candidate2-*/raw/`)
— if an artifact is unavailable, state the custody gap instead of substituting a rebuild.

Corpus: `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`. Manifest SHA256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` (re-verified this
round), tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. Stride10 `range(1,158,10) ∪ {157}`
(17); stride3 `range(1,158,3)` (53); stride1 157 states. Confirm from
`shared/history_corpus.py`; do not shrink selections.

Toolchain and identities of this round: Rust 1.85.1, Python 3.14.3, released harness
binary SHA256 `190424195506d4d54b24d253b483986d9aadf542fe125ae61225d833c2102ab1`,
harness source `57d761b36`/`a2ea0ef46`/`80e6b9868`.

## 8. Work organization, proof and deliverables

Keep the round finite. Produce:

1. **A confirmation or refutation of the depth-term mechanism**, from counters that
   already exist (pack/decoded cache scope), before writing product code.
2. **One treatment**, pre-registered as in §3, with matched arms, byte-identical-Store
   equivalence, and an identity-matched verification per identity.
3. **Measured evidence** under §6, with exact identities, fresh append-only outputs, and
   every failure and deferral retained.
4. **Required checks** for whatever tree is changed, reported exactly as run or not run
   (§6).
5. **A report and an append-only ledger entry** after `L47`, plus a #190 update and exact
   first-parent/staged production LOC for each commit. Keep reference/core subtotals and
   disclose scope changes.
6. **Do not close #190.** Closing needs the owner's qualification decisions (D1–D5) and
   the v0.1.6 reconciliation, neither of which this lane can supply.

# #273 four-step iterative handoff after the 8,192-WRITE refusal

> **Status:** Owner-requested continuation prompt; a planning checkpoint, **not**
> release evidence, a new workload contract, or proof that the 8,192-WRITE
> failure is caused by disk exhaustion. Use the assigned worktree only. This
> document specifies a **repeated experiment → change → proof → synthesis
> loop**, not a predetermined patch or permission to inflate a limit.
>
> Source pin at handoff: `6e458a148518f509c2a2435dd5d3ed34e793df64`
> (tree `95b8a10fe0a7b2dd71f58fb3c00c0da3dbd38fed`); last product
> telemetry change `6ea57701b910f3b1e64add84a77de3cdae15b1b4`;
> nonregistered 8,192 test-only extension `91aca43f54627fe26100cc020042a9c5377ba1e4`.
> Recheck HEAD, clean status, PR #274 draft/#273 open, owned Docker resources,
> and Cargo owner on resumption. Do not edit another worktree or the frozen
> #271 product `48b51e874a41b3e1e6c6661e145316df8b408f07`.

## Agent prompt — continue the assigned branch

Work in `codex/issue273-active-head` at
`/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs`. Treat this
whole document as the task: execute steps 1→4, loop back to 1 on each
identified avoidable factor, commit every meaningful algorithm/bound change
with architecture and exact production LOC, and preserve raw failure evidence.
Begin with the exact 8,192 C5 refusal domain/site and the registered
100→512→4,097 WRITE page-growth cause; **do not** presume that 64 MiB was
physically allocated or that local quadratic-looking page versions are safe
to remove. Use public routes and committed production telemetry; keep frozen
control, workloads, quota/Budget, pins, process continuity, deadlines and
cache/custody contracts intact. Do not take an unchanged-arm speed resample
or close/merge #273/PR #274. Report explicitly what each iteration proved,
what failed, and what remains INELIGIBLE/INCOMPLETE/NOT_RUN.

## A. Evidence and limits before editing

Read [the append-only iteration ledger](CHECKPOINT5-OPTIMIZATION-LOG.md),
especially iterations 009–013, [the scaling decision](PROSPECTIVE-SCALING-DECISION.md),
[the active format/evaluation contract](ACTIVE-FORMAT-AND-EVALUATION-v1.md),
[phase 4.5 implementation authority](PHASE4.5-IMPLEMENTATION-SPEC.md),
[the execution specification](../../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md),
[repository rules](../../../../AGENTS.md), [Core rules](../../../AGENTS.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[release policy](../../../../docs/general/release-policy.md) and
[documentation policy](../../../../docs/general/documentation-policy.md).
The raw [iter-012 nine-cell RESULT](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/RESULTS.json)
and [iter-013 diagnostic RESULT](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-013/RESULTS.json)
plus their adjacent `SHA256SUMS` are **local, gitignored** artifacts in this
worktree; verify each manifest **from its own directory**. They have separate
source/harness identities. Do not publish a GitHub URL to raw evidence that is
not in Git; cite the committed ledger and the local evidence path explicitly.

**Do not simplify the question into “no quadratic WRITE”.** The registered
append/dispersed/repeated × 100/512/4,097 rows at `52878a584` are nine
independently byte-verified but **INELIGIBLE** single observations. At dispersed
100/512/4,097, Exec is 0.124/1.019/9.308 s (not a matched speed claim),
index seeks/accepted WRITE are 14.43/14.89/14.99, index page writes/WRITE
3.73/5.52/6.44, and normalization/WRITE 0.27/0.78/1.06. Page/normalization
and Exec growth are unresolved even though seeks do not exhibit a whole-W
scan. The latest WRITE counters are sampled at 4,096, *before* the 4,097th
WRITE and Commit. In the separate public Stage functional/count diagnostics
with real production telemetry: no-key-subtree page versions at dispersed
100/512/4,097/8,192 were **0/0/810/3,154**; 8,192/4,097 WRITEs ≈2,
3,154/810 ≈3.89. That is a near-quadratic **finite intermediate signature
for this subset**, not proof of an unbounded W² public WRITE or proof those
pages are avoidable. A page with no key update can still be necessary to
connect a selected hot/cold target. Keep selected height, carries, eviction,
pins and changed-key pages separated. No global 2×/10× or release claim.

**The 8,192 diagnostic FAILED after accepted WRITEs:** SaveFile completed
(8,192 final references, 8 windows, 824 pack loads), the canonical Commit
outcome was *known*, then **local C5 reconciliation** returned `Capacity`
with `installed_revision=None`. Its full final byte oracle and clean-close
refund were **NOT_RUN**; the owned failed Docker container/volume were
inspected and removed after saving evidence, not mistaken for Workspace
cleanup. Never retry the known canonical Commit on a guess. Preserve this
uncorrected receipt; do not rerun the unchanged diagnostic to find a better
number. This tier is not one of the registered 3×3 performance cases.

**Correct resource distinction:** [the external test fixture](../../../crates/layerfs-workspace/tests/support/native_workspace.rs)
chooses `disk_budget_bytes=Some(64 MiB)` and the product's default
`memory_budget_bytes=8 MiB`. **64 MiB is not the product's numeric disk
quota default.** Disk quota counts actual physical backing allocated **plus**
checked reservations, shared across the host; it is not a 64 MiB file-size
limit. With ≥8,192 `E` and ≥8,192 `R` deletions, the C5
[compaction heuristic](../../../crates/layerfs-workspace/src/backing/active/compaction.rs)
calculates `(updates.len() + 32) × 4,096 ≥ 64.125 MiB`, necessarily setting
`pressure=true` against the fixture's 64 MiB quota. **That value is a
headroom *estimate*, not an allocation or reservation.** It proves branch
selection, **not** the failing allocation or that actual backing exceeded
quota. A distinct Budget reserve, physical page reservation, compaction
scratch or another bounded count may have failed. The exact refusal site and
domain are not instrumented. Do not silently change either 64 MiB test quota
or 8 MiB product default to make the old attempt pass. On a *successful*
Commit, selected G1 owners are unlinked/refunded when their final pins end;
quota is never reset wholesale, and live G2, metadata, old readers and
failed/unknown owners remain charged. A no-pin sequence whose charged
backing grows across successful Commits without bound is a separate product
bug to investigate, not an excuse for a hidden reset.

Other outstanding admission gates stay explicit: #248 C1-zero provenance
**INCOMPLETE**; real same-Workspace SDK pinned-journal clean and one-edit
controls **NOT_RUN**; exact three-sequential-Exec and full mutation selections
**NOT_RUN**; common frozen control comparison and private cache/VM/backend/
device/host and phase-local cgroup evidence **NOT_RUN/INELIGIBLE**. Old frozen
control 25-second rows are censored walls, never phase speed denominators.
Keep PR #274 **draft**, #273 open; no merge or issue close without owner ruling.

## B. Repeat these four steps, in order, for every meaningful change

### 1. Pin the exact refusal and the WRITE amplification before selecting a patch

First inspect *every* `Capacity` source and release boundary in
`commit/active_reconcile.rs::{prepare,reconcile}` →
`active/lifetime.rs::publish_reconcile_map` →
`active/compaction.rs::plan` → `active/index.rs::prepare_file` →
`active/pages.rs::{reserve_pages,create_from,release}` and
`backing/budget.rs`, plus payload/metadata Host quota transfer/refund. Show
how the 16k-key C5 patch, geometrically charged extent/deletion vectors,
selected G1 view, mutable G2, actual block charge and 8 MiB Budget coexist.
Identify physical `allocated`, `reserved`, Budget `used/limit`, required
request, operation phase, selected pins, and **which** reservation returns
`Capacity`. If existing production counters cannot name it, add *real*
bounded production telemetry (not a test-only product hook or an uncharged
spool), plus an external parser and refusal test. A single causal diagnostic
on a **new committed source/instrumentation identity**, clearly labelled
`admission_eligible=false`, may reproduce 8,192; retain the original FAIL
and any further FAIL. Instrumented wall includes observer overhead. Do not
relabel or resample the same old identity. If the branch merely scans every
unrelated pack under a pessimistic estimate, count its reads separately from
actual attempted physical allocation. Determine whether failure occurred
before or after irreversible C5 publication, and prove continuation/final
known-outcome custody without a blind retry or hidden cleanup. In parallel,
for registered WRITE 100/512/4,097 and carry/slot transitions, classify new
index pages as changed-key, necessary parent connection, authenticated hot
admission/eviction, selected height/carry or pin-related—not just elapsed
time. A missing counter is INCOMPLETE, not zero.

### 2. Compete on source-bound architecture and implement only a proved winner

Hypotheses: (A) tighten *actual* C5 changed-page/reservation bounds or stream
charged reconciliation without rebuilding unrelated P/whole Workspace;
(B) pre-admit peak transient old+new Budget and physical pages while retaining
one atomic selection, selected G1 and live G2; (C) authenticated generic
hot-cursor continuation to avoid demonstrably unnecessary no-key ancestors.
Decide with actual failing domain and phase-local WRITE page pools, not a
larger test quota, widened hot cache/window or lower workload. Do not assume
all representation-only pages are avoidable; if bypassing one, prove exact
fence/ancestor/epoch, later eligibility, old selected bytes, owner/refund and
failure semantics. Each candidate needs count and peak RAM/quota formulas
for 100/512/4,097 **and** 8,192 as a separate diagnostic; explain what
increases across the registered three tiers. If the correct operation truly
cannot fit fixed limits, retain explicit refusal and request an owner profile
ruling instead of raising the limit mid-campaign. No new worker, daemon,
remount, barrier, uncharged file-sized spill, after-ACK work or fsync. A new
physical format/free-slot design requires a prospective v2/v3 reader,
migration/legacy/old-pin and custody spec *first*; never overwrite a selected
incarnation or change C1/C2 canonical identity for a timer. Document the
winning algorithm and bound in the affected architecture document in **the
same product commit**. Record exact first-parent/staged/committed Core,
reference and combined production LOC with
`python3 tools/production_loc.py --json --root <snapshot>` in each commit
and handoff; tests stay outside product `src/` and file ceilings hold.

### 3. Prove each change publicly, then run one labelled count diagnostic

Use one Cargo owner and **this** worktree's own target; protect other owners'
Docker containers/volumes. Run affected `cargo +1.85.1 test --release
--manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace`
(and FUSE when changed), all-target warning-denying Clippy and fmt, product
boundary and tools tests. Build exact aarch64 release Linux tests; run
`active_backing-<exact-hash>` on an **owned ext4** volume with
`TMPDIR=/work`, `LAYERFS_ACTIVE_TEST_ROOT=/work`, `--test-threads=1`.
Reissue affected public `stage_route.py` cases against the protected closed
functional fixture and release daemon/test binaries; keep
`performance_claim=false`, `cache_claim=null`, single construction worker,
full G1/G2/old reader bytes, process continuation, quota/Budget refusal,
actual `st_blocks * 512`, clean-close refund and known/unknown custody.
Do not replace the missing SDK pinned controls with a detached Store. Keep
all failed attempts and raw stdout/stderr. For **each changed case/arm/identity**
run **one** prospective causal count diagnostic (`checkpoint5_273.py run
--diagnostic` when applicable), not an unchanged performance rerun; require
per-WRITE, per-affected-identity, per-page and per-pin oracles across all
three patterns/tiers at the frozen product. Raw evidence goes in a *new*
`benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-NNN/`
with hashes, seals, actual commands, resource domains and `SHA256SUMS`.
For the external public Stage probe the command shape is:

```sh
python3 core/crates/layerfs-workspace/tests/stage_route.py \
  --fixture core/target/issue273/checkpoint3-prepared-v1/result.json \
  --binaries core/target/release \
  --test-binary core/target/aarch64-unknown-linux-musl/release/deps/stage-<exact-hash> \
  --case active_generic_profile_8192 --output <new-owned-iter-NNN-path>
```

Use a freshly built **exact** test binary; a new test identity is not a
qualified perf change. This extended case currently returns FAIL after
canonical Commit. Do not rerun it unchanged or change its quota to replace
that receipt. A new instrumentation identity permits one newly labelled
causal diagnostic, with source and test hashes, and retains all attempts.

The runner has **no** generic `--setup clone` or `--perf-fast`; reuse protected
masters/immutable binaries/images through its actual prepare/copy route, never
warm measured input or a mutated sample. 15/25 s complete-command limits,
8 MiB default Budget, <=8 resident pack pages, <=64 index nodes, <=8 cursors,
64 slots, 1 MiB hot bytes, and one worker remain fixed. A diagnostic beyond
registered tiers does not become a registered PASS.

### 4. Freeze and evaluate honestly; if a factor remains, return to step 1

At a frozen changed product/harness/source identity, perform the nine
append/dispersed/repeated × 100/512/4,097 public byte oracles and count/
resource checks **once per case/arm/identity**, including observer completeness
and cleanup. Predict page versions, seeks, affected closure, carries, evictions
and pins *before* running and compare with the archived **correct** operands;
do not compare a candidate to the frozen control's censored 25-second phase.
The new C5 refusal requires a successful and separately bounded 8,192
**diagnostic lifecycle** (or an honest supported-limit refusal) before claiming
extended support: a known remote Commit without local reconciliation is not
an incremental-success/refund proof. Include sequential no-pin Commits and
selected G1 versus live G2 and 32-pin release in quota continuity checks;
there is no quota reset at a Commit boundary. If a fix exposes another
avoidable page-copy/owner-scan/refusal factor, return to step **1** with a
new preregistered hypothesis, test and commit. Do not stop at a lower pack
load count or one shorter wall. If seeking a **numeric** control comparison,
use a **separate owned** checkout at unchanged #271 product, port only common
harness/observer fixes, prove its product seal, and collect all 12 selections
row-major control→candidate under identical declared cache/cgroup/verifier
identities. Without independent cache/phase-cgroup capability, C1-zero
provenance, real SDK pin contract or owner ruling, retain their exact
INELIGIBLE/INCOMPLETE/NOT_RUN statuses and keep the PR draft.

**Handoff/report per loop:** starting hypothesis + source call graph and
refusal arithmetic; parent/staged/commit/tree/artifact identities; predicted
counts/charge/physical bytes and observed phase-local deltas; byte/oracle,
pin/G1/G2, quota and clean-close results; every nonpassing receipt; code and
architecture files; LOC before/after/delta; next falsifier. Finish only when
no *known avoidable* scaling factor remains in the covered operations and
all admission blockers are either proved or reported explicitly. A finite
3×3 and one 8,192 failure cannot establish a universal scaling or speed law.

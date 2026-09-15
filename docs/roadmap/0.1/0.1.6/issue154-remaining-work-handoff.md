> **Status update (2026-09-16).** Sections 2 and 3 of this document are
> superseded. The verifier redundancy was removed at the root cause (ledger L10):
> the seven stopped verification rows now pass inside the unchanged 25 s gate, so
> there is no outstanding budget ruling. Section 3 still describes the three
> missing implementations, but the actionable, up-to-date brief is
> [`issue154-remaining-implementation-handoff-prompt.md`](issue154-remaining-implementation-handoff-prompt.md),
> and the current one-run-per-case state is
> `evidence/issue154/final-seed1-matrix.json`.

# #154 continuation handoff — what is measured, what is blocked, what to implement

State at `b4349ae91` + the rollout commits on `main`. Read
[`evidence/issue154-rollout-ledger.md`](evidence/issue154-rollout-ledger.md)
(L1–L9) and the phase comments on
[#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) first; the
aggregated matrix is on [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122).

## 1. Terminal now (seed 1/2/3, one perf + one separate verify each)

* `file_size_transition` — 7/7, all three seeds, walls 1.7–2.4 s.
* `dedup_branch_history` — `large-hotset` and `boundary-cycle` K10/K100, walls
  1.8–4.0 s.
* `mixed_load_bearing` / `multi_workspace_development` / `branch_development` —
  every performance row (L100 and L500, K10 and K100; four declared ≤25 s
  exceptions listed by case) and the L100 verification rows.
* Three extensions (seed 1): L100 exhaustive PASS 38.33 s, four-workspace
  PASS/PASS 9.54 s / 33.05 s, L500 exhaustive TIMEOUT 298.28 s.

## 2. Owner decision outstanding (escalation 1)

Seven verification rows cannot fit the 25 s ceiling and are retained as `FAIL`
with their stop walls: mixed L500 K10/K100, workspace L500 K10/K100, branch L100
K100 and L500 K10/K100 (× three seeds). Measured attribution is in ledger L5:
the schedule replays in 1.07 s (mixed L500 K10) and 8.74 s (branch L100 K100); the
tail is the declared verification scope at ≈0.5–0.7 ms per path, so a 30 000-path
complete inventory needs ≈15–20 s on its own. The L500 exhaustive extension
independently confirms the scale (298.28 s for 101 states against 38.33 s at L100).

**Do not** fix this by shrinking the verification scope, raising the watchdog
silently, or trimming the fixture. The owner rules between a declared larger
exception (with its measured wall) and a `NOT_RUN` disposition.

## 3. Three implementations that do not exist in this tree

Each is new work, not a port; none of them existed on the archived line either.

### 3.1 F5 — the `namespace-inode` (HN) compact history schedule

* Frozen contract: `docs/roadmap/0.1/0.1.6/benchmark-families.md`
  §`dedup_branch_history` — five stages per cycle: (1) unlink/recreate two tiny
  files, one recurring and one generation-specific; (2) two single-file SDK 256 B
  overwrites on two other tiny files; (3) rename the populated `tiny` directory to
  its alternate name; (4) one alias to a fifth tiny file, chmod two files plus the
  directory, explicit mtimes; (5) one 4 KiB atomic save over the aliased
  destination, observe the old inode through the alias, remove the alias. Four
  POSIX helper executions and two SDK calls per cycle, helpers finishing before
  each Commit. Envelope: S+2 names, S+8192 logical bytes, directories unchanged.
* Current state: the fixture profile exists
  (`SProfile::NamespaceInode`, `v016_compact.rs`), the case IDs are registered,
  and both routes fail with `fs-benchmark-workload: unsupported dedup native
  workload` because `v016_edits` returns *"the five declared HN stages have no host
  orchestrator"*.
* Work: a host orchestrator for the five stages, the `is_sdk`/route wiring, and
  the per-state oracle. The M1 engine (`src/v016_mixed.rs`) is the closest model
  for session/exec/SDK/Commit/receipt structure, but HN is a single-branch history
  on fixture S, so it should reuse `dedup_branch_history::expected`-style state
  derivation rather than the M1 shadow state.
* Consumers: `v016-access-inode-before/after-v1` (F6) need Commit 94 and 95 of
  `v016-history-namespace-inode-k100-v1`.

### 3.2 F4 — the two compact branch controls

* Frozen contract: `benchmark-families.md` §"Two compact branch controls".
  `v016-branch-convergent-content-v1`: fixture S, trunk10, A10 and B10 forked from
  trunk commit 5, 30 new commits, longest ancestry 15, 31 retained roots.
  `v016-branch-fork-descendant-v1`: the same plus C10 forked from A's local
  commit 5, 40 new commits, longest ancestry 20, 41 retained roots.
* Per-commit edit: one 256 B SDK overwrite on the first medium file,
  offset `4096*((j-1) mod 2)`, payload alternating A/B by `floor((j-1)/2) mod 2`,
  with those regions initialised to a distinct Z (`SProfile::BranchControl`
  already declares exactly these two regions and `d::BRANCH_Z` exists). `j` is the
  ancestral ordinal: trunk 1..10, children 6..15, descendant 11..20. Convergent
  children make **identical** content changes and must have equal file-content
  IDs; the descendant salts B/C by branch and preserves all sibling heads.
* Work: a compact topology in `v016_stages.rs` (cardinalities 30/40 total, 31/41
  roots, 15/20 ancestry), registration in `branch_development::cases()` (4 → 6),
  the compact round in `src/v016_mixed.rs`, and the oracle.
* Consumers: `v016-access-fork-point-v1` and `v016-access-divergent-head-v1`
  (F6) need the sealed convergent producer (trunk commit 5, 31 roots) and the
  descendant producer (B local commit 10, 41 roots).

### 3.3 F6 — the `historical_access` route and its six cases

* Frozen contract: `benchmark-families.md` §"historical_access: six additions" and
  `execution-and-verification.md`; the six IDs are
  `v016-access-{boundary-before,boundary-after,inode-before,inode-after,fork-point,divergent-head}-v1`.
* Each invocation mounts **one selected retained state of a sealed producer** with
  one live workspace, creates no commits, and reports the reader profile
  (fresh-session/application-cold, uncontrolled OS cache, no pre-read). Producer
  identity, graph size and selected ordinal are explicit inputs — never an
  automatic history-building step. Performance is `N/A`, never 0 or `PASS`.
* Current state: `families/historical_access/fixture.json` is still the inherited
  11-case artifact; `infra-list historical_access` returns nothing; neither the
  producer-sealing step nor the mount-one-retained-state route exists.
* Order: build the sealed producers first (the boundary-cycle producer already
  runs today; the inode and compact producers need 3.1 and 3.2), then the access
  route, then the six cases and their independent re-read verification.

## 4. Rules that must not be relaxed while finishing this

* One sample per case/arm/mode; fix, rebuild, re-take exactly one sample on the new
  identity.
* `--prepare` first (its own selected step, 120 s/300 s watchdogs), then the gate
  sample: first-use fixture construction inside a gated invocation is invalid — it
  cost 13–16 s per L500 case and produced two spurious budget failures (ledger L7).
* Record the host load with each invocation (`v016_rollout.py` does) and remove
  only this benchmark's orphaned containers before a phase; a contended sample is
  not comparable with a quiet one (ledger L7).
* The verification oracle is the thing under test's judge: never widen, shorten or
  sample it to fit a budget.

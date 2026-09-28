# Next-agent prompt: review and prepare the #273 -> #264 Phase 4.5 integration

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Starting snapshot for this handoff: #273 side-lane head
> `29fc5747d70eadf01fd8794bf84a50cf57ef3660`, tree
> `e5d86d996e8283048c078d7b53d65aa65f284823`; #264 main-lane head
> `6eb7553671d3000160ad023a57b335e52dd81a26`; date 2026-09-28.
> These are **starting** identities: recheck live PR heads, issue rulings,
> worktree owners, cleanliness, and product seals before working. This prompt
> gives an order and falsifiable exit gates, **not** authority to merge,
> publish a numeric speed result, modify another owner's receipts, raise a
> budget, or claim release readiness. The [order-1 source/evidence publication](PREINTEGRATION-ORDER1-SOURCE-AND-EVIDENCE.md)
> is the detailed evidence baseline, not a new benchmark receipt.

## Assignment and hard stop

Prepare **all** of the following, in this order, **before** anyone decides
whether to integrate #264. The owner-directed [#273 phase-4.5 spec](PHASE4.5-IMPLEMENTATION-SPEC.md#1-scope-sequencing-and-integration)
says finish the #273 hot-backing side lane before integrating #264 mounted
namespace. The [#276 tracker](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276)
remains the single open deferred limitations issue; earlier issue closures
are scoped handoffs, not approvals or proof. If an owner choice, test repair,
capability, code review, or resource is absent, report the blocking status
and stop at that decision boundary. **Do not silently move #264 first to
"unblock" #273, combine unlike receipts, or treat an API merger as release.**

```text
                  ORDER AND DECISION GATES

 [1] PUBLISH exact #273 evidence/architecture/LOC/complexity summary
                |
                v
 [2] REVIEW #262 -> #272 -> #274; resolve/rule #276 SDK/cache/red tests
                |
          stop if unresolved   ---->  NOT_RUN / INELIGIBLE / FAIL
                |
                v
 [3] REVIEW #263 (#258), then #269 (#264); #270 remains separate
                |
          stop if unreviewed   ---->  no combined-source sign-off
                |
                v
 [4] INTEGRATE onto reviewed #273 source in owned worktree *only after*
     authorization; identify first parent, record conflicts & LOC
                |
                v
 [5] REPEAT combined namespace + active backing + handles + G1/G2 +
     quota/custody + #248 functional proofs; run affected Core checks
                |
                v
 [6] REPORT exact tree and unresolved gates; owner decides merge/release

 #256 many-file scale / #270 C1 move-Commit: separate proof tracks, never
 inferred from the mounted rename or one-file #248 / #273 evidence.
```

### Rules to read before editing or sampling

Read root [AGENTS.md](../../../../AGENTS.md), [core/AGENTS.md](../../../AGENTS.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[documentation policy](../../../../docs/general/documentation-policy.md),
[release policy](../../../../docs/general/release-policy.md) and, before any
benchmark harness/sample work, [benchmark-tree rules](../../../benchmark/fs-bench-pro/AGENTS.md)
and [quickstart](../../../../benchmark/fs-bench-pro/QUICKSTART.md).
[Registered checkpoint-5 selections](../../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md)
freeze cases, budgets and cache procedure. No CI runs here;
`tools/preflight.sh` is permanently retired. Use a **worktree-local** Cargo
target, locked release profile where prescribed, one construction worker
(except namespace Init), no third-party patch, no unsupported capability
fallback, no `fsync` on Workspace backing, no process quiescence across
Commit, no repeated unchanged arm. Leave other agents' worktrees, Docker
resources and measurements alone. Do not run a benchmark merely to make a
planning document look complete.

## Step 0 — custody and source inventory before any review

1. `pwd`, `git status --short --branch`, `git worktree list`,
   `git rev-parse HEAD HEAD^{tree}`, remote PR metadata and the status of
   [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276).
   Verify whether any branch advanced from the pins in the banner. Record
   **actual** head/base/tree, draft/merged state and UTC timestamp. Do not
   write in another agent's worktree or interrupt their process/container.
2. Establish the ancestry with `git merge-base`, `git rev-list --left-right
   --count` and `git merge-base --is-ancestor`; inspect dependency bases.
   At this snapshot: [#262](https://github.com/Ephemeral-AI-Lab/layerfs/pull/262)
   head `6bcfa464f74ae9ca3859df31c678985ec69ba098`, base
   `6af2c5c59a48d0b6c85d656e55aecc353e346728`;
   [#272](https://github.com/Ephemeral-AI-Lab/layerfs/pull/272) head
   `48b51e874a41b3e1e6c6661e145316df8b408f07`, stacked after #262;
   [#274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274) head
   `29fc5747d70eadf01fd8794bf84a50cf57ef3660`, stacked after #272.
   All three are **open drafts** at the starting snapshot. [#263](https://github.com/Ephemeral-AI-Lab/layerfs/pull/263)
   head `ef3a310480254774d6e6966004fb5e0a4fa3b94d` shares #262's base;
   [#269](https://github.com/Ephemeral-AI-Lab/layerfs/pull/269) head
   `6eb7553671d3000160ad023a57b335e52dd81a26` builds on #263 and
   remains a **draft**. Recheck all these before using them.
3. Start a review ledger recording **source commit, first parent, tree,
   product/harness/test/workload/build seals, tests actually run, failures,
   limits, cleanup and who owns each unfixed blocker**. Keep receipt origins
   separate: committed [phase-4.5 functional archive](evidence/phase4.5/hot-publication-20260928/README.md),
   [historical 12-row campaign](CHECKPOINT5-LOG.md), and local gitignored
   iter-018/019 raw results are not interchangeable.

```text
              ANCESTRY, NOT AUTOMATIC MERGE ORDER

  6af2c5c (shared ancestor / review-fixes base)
      |
      +---- #262 (#261/#265/#266) -> #272 (#271) -> #274 (#273 hot)
      |                                               29fc574 ...
      |
      +---- #263 (#258 inherited rename) -> #269 (#264 namespace)
                                                    6eb7553 ...

  #274 !contains #269       #269 !contains #274
  "GitHub mergeable" != "source reviewed" != "integration proved"
```

## Step 1 — complete the #273 side-lane evidence publication FIRST

Work from the [order-1 publication](PREINTEGRATION-ORDER1-SOURCE-AND-EVIDENCE.md)
(including its diagrams, exact identities, complexity symbols and per-row
matrix), [phase-4.5 log](PHASE4.5-LOG.md), [current iteration log](CHECKPOINT5-OPTIMIZATION-LOG.md),
[format authority](PHASE4.5-IMPLEMENTATION-SPEC.md), [frozen identity](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
and the [exact per-commit LOC record](evidence/phase4.5/hot-publication-20260928/phase45-commits-loc.json).
Check that every local link resolves, that all named identities match the
receipt source, and that the new document has *one* status and no invented
GitHub raw URL. If a new commit changes product/harness/source, append a
new source-pinned addendum rather than moving old receipts to the new SHA.

**Required publishable conclusion (no optional positive-only abridgment):**

- Phase-4.5 frozen `4ae36ad3a` functional set: 17/17 public cases / 42/42
  named checks PASS, plus retained failed attempts; no speed claim.
- Latest 3 x 3 product `8801b9c` with pinned product/harness/workload seals:
  nine full-byte/Commit/cleanup PASS; **nine numerical INELIGIBLE**; old
  control not newly sampled. Quote the case grid's exact pack/C1 counts and
  limit walls only as unqualified factual observations, never a ratio.
- Source-bound **amortized structural** result for an authenticated eligible
  monotone frontier, **not** whole-CPU O(1) or constant-RAM Commit. Describe
  old frozen HotDirectory, slot epochs, page custody and G1/G2 continuation.
  Explain charged `O(E_f)` final extent scratch, conditional pin retirement,
  generic overlap/eviction and physical readback/release costs.
- Production LOC at `8801b9c`: Core **68,031**, reference **65,417**,
  combined **133,448**; immediate parent **133,439 -> 133,448 (+9)**.
  Phase-4.5 `0513f8a1a`..`4ae36ad3a`: Core
  **65,136 -> 67,158 (+2,022)**, with separately recorded per-commit
  comparisons. Later test/docs-only commits do not add production LOC.
- Changed-candidate #248 explicit C1 edit-load zero is functional proof
  only; historical missing-zero receipts remain INCOMPLETE, latest gate is
  INELIGIBLE. The fixed-Budget 8,192 diagnostic is **nonregistered**, earlier
  8,192 FAILs survive; e70b6ab raw loop ratios remain unqualified. SDK clean
  and one-edit pin controls **NOT_RUN**; cache/cgroup/matched numeric
  **NOT_RUN/INELIGIBLE**; two broad `filesystem_ordering` tests **FAIL**.

The author of an external PR/issue summary must link **committed** source
and committed archive, and describe the ignored raw iter-018/019 path as
**local-only** unless it has actually been published with checksum and
custody review. An issue closure is a scoped handoff, not admission. Only
post comments if that external write is authorized and possible; otherwise
supply paste-ready text and URLs without claiming to have posted it.

**Step-1 stop test:** if any row lacks its actual source, treatment,
limit, original status or local evidence custody, do not proceed to merge
review by silently constructing a nicer number. Fix the *documentation*
while retaining the original status and evidence.

## Step 2 — review the stacked write-side PRs and explicit blockers

Review in dependency order, **diffing each PR against its declared base**
not `main` or the unrelated namespace branch. For each, check publication
atomicity, byte oracle, checked extent/pack ownership, physical page lifetime,
failed notification/cleanup, cache admission, Budget/host quota refusal,
G1/G2 selected readers, reply-gap permit semantics, 128 B tiny-vs-large
Payload boundary, and runtime/format compatibility. A source review that
only lists green test names is not sign-off. Record concrete findings as
path:line + triggering call path, severity, affected proof, proposed remedy,
owner and status; do not invent a bug from a divergent diff.

| PR and emphasis | Evidence to challenge before acceptance |
| --- | --- |
| #262: mounted write treatments, #265/#266 | [#265 result](../265/RESULT.md) and [#266 result](../266/RESULT.md): one page vs large Payload, edge/failure custody, FUSE reply-gap wake/deadline/coherence, retained original `INCOMPLETE` parser and cache-INELIGIBLE rows. No historical #261 512 FAIL retroactively cured. |
| #272: #271 root ownership/proposal | [Workspace-scoped active-head proposal](../271/WORKSPACE-SCOPED-ACTIVE-HEAD.md), [causal ledger](../271/CAUSAL-DIAGNOSTIC-LEDGER.md): page/owner lifetime under pinned G1, retired pages, allocation/claim/refund, v1 formats and limit claims. A proposal is not a measured speed PASS. |
| #274: #273 v2 hot backing | [phase-4.5 spec](PHASE4.5-IMPLEMENTATION-SPEC.md), [frozen archive](evidence/phase4.5/hot-publication-20260928/README.md), [iteration 018](CHECKPOINT5-OPTIMIZATION-LOG.md): directory epoch reuse, branch fences, generic splice fallback, C5 charged transfer, old-pin release, exact #248 C1 zero, all nine numeric INELIGIBLE. |

**#276 admission and red-test rulings are gating decisions, not a footnote:**

1. Public SDK has no retained same-Workspace G1 journal pin spanning the
   registered sequential clean/one-edit controls. Ask the owner to choose
   prospectively between an approved lease contract with full G1/G2, custody,
   cancellation and 32-pin proof, **or** a named profile waiver that retains
   both `NOT_RUN` and never calls the 12-row registry complete. Do not invent
   a new public API or use a detached Store as a substitute. See [decision
   request §1](OWNER-RULINGS-SDK-CACHE-CONTROL.md#1-public-same-workspace-sdk-journal-pin-approve-a-contract-or-rule-unrun).
2. Private cache/VM/backend/device/host and phase-local container cgroup
   conditions cannot currently be shown identical across frozen control
   and candidate; `memory.peak` reset is falsified on this Docker host.
   Require owner-approved independently verified common capability/observer
   or an explicit **INELIGIBLE** ruling. Only after a prospective shared
   workload/harness seal and a genuinely common cache contract could a new
   row-major control->candidate single sample per selection be considered.
   Frozen #271 product must remain unchanged. No censored 25 s control
   wall can be a denominator. See [decision request §2](OWNER-RULINGS-SDK-CACHE-CONTROL.md#2-frozen-271-control-and-private-cachephase-cgroup-capability).
3. Broad locked-release Core `layerfs-content/tests/filesystem_ordering.rs`
   failed two tests at the candidate source (`ObjectLimitExceeded { limit:
   18, actual: 19 }`). Paired first-parent all-tests were **NOT_RUN**;
   diagnose in the owning lane, determine whether the failure survives at
   the exact parent and repair/falsify it. Do **not** enlarge the ceiling,
   mark it pre-existing without a paired run, or claim release green while
   only targeted suites pass.
4. Retain scope of [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276)
   §3: multi-file/package changed frontiers and normalization safety have
   not been universally proved. Fixed 8 MiB success at 8,192 one-file
   WRITEs is not #256 many-file admission or an unlimited-work result.

**Step-2 exit:** produce a review disposition (accept with bounded scope,
request change, or block), explicit owner decisions or outstanding requests,
negative test results, exact source identities, and a *reasoned* merge/no-merge
recommendation for each PR. If #276 §1/§2 has no ruling or the two red tests
remain unowned/unresolved, report **NO MERGE/NO RELEASE CLAIM**; do not
advance to live integration by calling the issue closure approval.

## Step 3 — review #263 for #258, then #269 for #264

The [#258 inherited rename](https://github.com/Ephemeral-AI-Lab/layerfs/blob/ef3a310480254774d6e6966004fb5e0a4fa3b94d/core/docs/issues/245/PHASE4_INHERITED_RENAME.md) preserves
stable directory identity, selected old reader paths and a fail-closed
inherited-descendant/path-length check. Check cached and *never resident*
descendants, replacement, alias, held orphan and namespace-move cancellation.
Its branch is separate from #262; it is not already in #274. Then review the
[#264 identity-relative namespace checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6eb7553671d3000160ad023a57b335e52dd81a26/core/docs/issues/245/PHASE4_5_IMPLEMENTATION.md)
and [design](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6eb7553671d3000160ad023a57b335e52dd81a26/core/docs/issues/245/PHASE4_5_IDENTITY_RELATIVE_NAMESPACE.md): resident
Node `(serial,parent,component,attached)` closure, component-relative
lookup/list/create/remove/readlink, attached-chain checks at publication,
held-detached behavior and budget charge. The mounted rename source-derived
`O(c*h + d*log P)` is conditional (constant changed keys `c`, private
height `h`, ancestor depth `d`, resident Nodes `P`); the earlier version's
`O(P)` replacement scan was corrected, and `forget` can still cost
`O(P*d*log P)`. The 3-versus-67-descendant private-page count diagnostic
is **not** proof of path-local C1 Commit time or all depth limits.

Check separately the [#270 C1 move-Commit follow-up](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6eb7553671d3000160ad023a57b335e52dd81a26/core/docs/issues/245/PHASE4_5_C1_COMMIT_FOLLOWUP.md):
canonical full-base parent-alias validation and moved-subtree cycle scan
remain. No versioned authenticated owner index, old-reader migration, final
batch cycle/unique-parent proof, C1 page-visit/file-save count or pure-move
`O((K+D)h)` path-local Commit has been established. #270's closure was a
**deferred transfer to #276**, not completion of its seven acceptance
criteria. Keep #256 large-many-file Workspace capacity separate too.

**Step-3 exit:** report PR #263 and #269 dispositions, source-based semantic
interaction risks with #273, limits unproved, C1 move-Commit as NOT_PROVED,
and which overlap paths/format boundaries need a combined proof. Do not
call a conflict-free Git merge an architecture review.

## Step 4 — plan an owned combined tree; execute only with authorization

Do **not** modify #273, #264 or frozen #271 owners' working trees, and do not
update `main` merely to get an artificial fast-forward. On explicit
integration authorization, create an owned worktree/branch from the **reviewed
#273 source**. Apply reviewed #263 and then #269 ancestry exactly once; the
#262/#272/#274 commits already present in #273 are not to be replayed.
A merge/cherry-pick strategy must declare the selected first parent, any
rebase rewriting identities, and real conflicts in files/format/call paths.
Record source before/after, parent/tree, independently typed v1/v2 and
namespace assumptions; verify no obsolete `RootOwner` route was resurrected
by conflict resolution. Never call this a `--ff-only` integration.

```text
 review only, then (IF AUTHORIZED) actual integration

                  first parent: reviewed #273 hot head
                                   |
                                   v
                           +----------------+
       reviewed #263 ------>| combined owner |<----- reviewed #269
          (#258)           | branch/worktree |          (#264)
                           +--------+-------+
                                    |
                            scoped proofs & LOC
                                    |
                     owner merge decision (not automatic)

                        G1 old selected directory
                        G2 live namespace ancestry
                        held handles + canonical head
                        MUST all agree on one combined tree
```

Before *each* commit, run `python3 tools/production_loc.py --json --root
<exact first-parent/staged git archive snapshots>` with the same counter
for both sides; compare the committed tree afterward. Include
`Production LOC: <before> -> <after> (delta <signed>)`, Core/reference/
combined subtotals and method in **every** commit message and handoff,
including docs-only commits. If conflicts alter source, rerun the comparison.
Architecture documents whose boundary/format/algorithm/bound changes must
change with product code, pinning the true source; don't give a side-lane
document a fictitious combined-tree pin.

## Step 5 — one final functional/check pass on the exact combined tree

Freeze one tree/product/harness/test identity and clean status; ensure the
Linux host + owned ext4 and Docker capabilities really exist. Design focused
external tests before running the affected checks, and run covering commands
**once at the final source**. A red test is diagnosed from its output and
source, fixed once, then covering commands run once. Never run a new
performance arm just to make the combined functional proof look green.

| Combined proof surface | Falsifier and required evidence |
| --- | --- |
| Namespace / ancestry | Inherited never-resident and resident directory move, deep component-relative lookup/list/readlink, move-back, replacement, cycle, detached-parent rejection and canonical old-head correctness. Compare stable serials/paths and charged Node/ancestor closure; no subtree copy-up or hidden full-path dependency. |
| Active backing + G1/G2 | Public tiny append/inherited Base/Zero/generic overlap/Payload, selected frozen HotDirectory slot epoch, independent old G1/new G2 bytes and same running PID/fd/inode across capture/SaveFile/C5/continued WRITE. No unmount/restart, process pause, historical replay or reset. |
| Handles and aliases | Pinned old directory/file handles, rename/unlink/forget/open-unlinked, stale handle reads, path serials and namespace revisions; no stale or unrelated authenticated target accidentally resolved through live directory. |
| Budget/quota/custody | Real prepublication quota/Budget refusal leaves revision and bytes unchanged; old+new reservation overlaps charged; known successful C1 plus local C5 failure retains custody and (if admitted) live G2 progress. Partial/unknown physical owner/unlink retains charge and fails closed, with exact clean-close refund *only after verified release*. |
| #248 named functional gate | Original registered 4,097 separated WRITEs, 4,096 space checkpoint below 3 MiB *only for that named one-file scope*, independent old/new byte oracle, complete anchored C1 edit-load zero provenance, correct namespace/handle and cleanup; do not copy iter-018 INELIGIBLE numeric status into a new speed PASS. |
| #256 and #270 boundaries | Explicitly list them **NOT_PROVED** unless an independent changed-source many-file package workload or pure-move C1 internal visit/index/old-reader/failure proof has actually been taken. |

From repository root, applicable locked Core checks include focused
package/external Linux native suites that cover the actual changed code,
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`,
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
--all-targets -- -D warnings`, `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all -- --check`, `python3 core/tools/check_product_boundary.py`
and `git diff --check`. Use `--release` for profile-required release proofs,
including locked ARMv8 native builds with root `.cargo/config.toml` AEAD
flags; record the **exact actual command**, host capabilities, failures and
unrun checks, not generic promises. Root checks alone do not exercise Core.
The broad red filesystem-ordering cases need an explicit resolution, not
just successful focused Workspace tests. No aggregate preflight or CI-green
claim. Keep complete test logs and own-directory SHA manifests for new
attempts; use fresh output paths, never edit earlier receipts.

The #248 *functional* repetition on a changed combined source is a new
source-bound correctness proof, not permission to repeat its unchanged
performance arm or to claim a speed threshold from an ineligible row.
Any future numeric campaign requires **prior** owner rulings and a common
prospectively sealed contract, identical enforced cache state in both arms,
phase-local resource capability, full registry and one sample per case/arm.
Absent that, all numbers remain `INELIGIBLE`/`NOT_RUN` and the previously
censored controls remain `FAIL`.

## Step 6 — deliver the decision, not a default merge

Report as a table for each step: source/first parent/tree/seal, artifact or
PR, accepted or rejected claim, tests/receipt IDs, cache and quota/phase
resource contract, positive **and negative** statuses, exact reproduction
command and cleanup, open owner ruling, and next action. Finish with:

1. `#273 side-lane evidence`: proven scope and nine INELIGIBLE numeric rows;
   publication link if actually posted, otherwise local document only.
2. `Stack reviews`: #262 -> #272 -> #274, then #263 -> #269; findings and
   unresolved #276 §1/§2/§5 and #270/#256 restrictions.
3. `Combined source`: exact commit(s), first-parent LOC per commit and
   verification **or NOT_BUILT with blocker**. No fast-forward inference.
4. `Admission`: #248 functional gate and Core broad tests explicitly PASS,
   FAIL, INCOMPLETE or NOT_RUN **on the combined tree**, not inherited from
   side-lane receipts. Speed, release and merge each have separate decisions.
5. `Unrun`: every required control/mutation, cache/cgroup capability or
   Linux host proof that was not performed, with its reason and owner.

Do not merge a draft PR or declare a release merely because #265/#266/#270/
#271/#273 issues were closed. **Owner review and explicit merge authorization
remain necessary.**

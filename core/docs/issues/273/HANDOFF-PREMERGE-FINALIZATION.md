# Next-agent prompt: finalize the #273 → #264 pre-merge program

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Written 2026-09-29 UTC by the Phase 4.5 pre-merge completion agent, from clean
> owned worktrees. This prompt gives order, pins and falsifiable gates, **not**
> authority to merge a GitHub PR, update `main`, claim release admission, change
> benchmark limits, or rewrite receipts. Where a gate below is satisfied, proceed;
> where it cannot be, finish with a complete source-pinned NO-MERGE handoff.

## START, OWNERSHIP AND REQUIRED READING

Work in **new owned worktrees** with worktree-local Cargo targets. Do not modify
another agent's worktree, receipts, binaries, targets, Docker resources or the
stale local `main`. Four owned branches from the previous pass exist and are
clean; **read them, build on them, do not rewrite them**:

```text
codex/issue273-c1-ordering-repair    fbda0f0f1  C1 ordering-charge repair (first parent 29fc5747d)
codex/issue269-seal-repair           05eb5c148  one-attempt rename seal (first parent 6eb7553)
codex/issue273-sdk-view-lease        6f18a5f42  public read-only WorkspaceViewLease (tree 016286f3)
codex/issue273-combined-functional   8dcc47206  combined tree + ledger (product tree cb23d730b)
```

Read before any product or measurement work: root `AGENTS.md`, `core/AGENTS.md`,
`benchmark/AGENTS.md`, `benchmark/fs-bench-pro/QUICKSTART.md`,
`docs/general/benchmark_rules.md`, `docs/general/documentation-policy.md`,
`docs/general/release-policy.md`, and, in the combined worktree:

- `core/docs/issues/273/PREMERGE-COMPLETION-20260928.md` — **the completion
  ledger: every lane, proof, FAIL/NOT_RUN and per-commit LOC of this pass. Start
  here.**
- `core/docs/issues/273/SDK-VIEW-LEASE-CONTRACT-20260928.md` — the finalized
  view-lease contract (bounds, grant bit, falsifiers), implemented by `6f18a5f42`.
- `core/docs/issues/273/HANDOFF-PREINTEGRATION.md` and
  `PREINTEGRATION-REVIEW-20260928.md` — the original program and reviews.

At the start, recheck with git and `gh` (at `/opt/homebrew/bin/gh`; docker at
`/usr/local/bin/docker`): live PR heads/bases/draft states, #276 for new owner
comments, branch ancestry, worktree cleanliness. Starting pins (2026-09-28):
#262 `6bcfa464`, #272 `48b51e8`, #274 `29fc5747d`, #263 `ef3a3104`, #269
`6eb7553` (all OPEN; #269/#274 mutually non-containing, both rooted at
`6af2c5c`); #275 `c39d6085` is an unrelated open statfs PR. Document any movement
before basing work on a branch.

## STATE INHERITED (do not redo, do not relabel)

1. **C1 ordering reds REPAIRED** (`fbda0f0f1`): targeted charged new-parent
   membership + per-serial declared-total rate; paired first-parent evidence
   271/3 at `29fc5747d` (three reds, incl. `parent_value_ordering_quota_matrix…`
   which the retained evidence had not named); repaired `layerfs-content`
   275/275 release. The two retained red tests and the third pass on the
   combined tree.
2. **#269 duplicate seal REPAIRED** (`05eb5c148`): origin was `a9657b85f`
   (2026-09-23, ancestor of both #263 and #269) — the old review's "introduced
   over #263" attribution was wrong. Focused suite 11/11 on owned ext4.
3. **SDK view lease IMPLEMENTED AND LIVE-PROVED** (`6f18a5f42`): pin/lookup/
   list/read/readlink/status/release through the daemon; opcodes 21-27; one
   shared view-authority grant bit 64 (the only free control bit); charged
   32-lease registry; checked retirement release. Live Docker proof (image
   `sha256:f35d5fbb5e46…`, daemon sha256 `18b8960d…`, aarch64-musl, repo-root
   AEAD profile) proved: pinned root/generation/revision stable across Commit;
   exact old-G1-via-lease vs live-G2-via-exec bytes in one Workspace; pinned
   listing survives live rmdir; released/cross-lease/malformed-tag/unregistered
   refusals; 32-admit/33rd-refuse; Completed release → unmount. ≈5-6 of 8
   falsifier groups proved (gaps listed in task 3).
4. **Capability probe verdict PARTIAL**: on Docker Desktop kernel
   6.12.76-linuxkit, `memory.peak` reset is **falsified** (write rc=0, peak
   unchanged) — phase-local peaks are NOT available; **`memory.reclaim` works**
   as per-container private-cache invalidation (file cache 808,337,408 → 49,152
   bytes, `memory.stat`-verified, idempotent at the floor), reachable/writable
   per-container from a privileged `--cgroupns=host` observer; phase-local
   `memory.current`/`memory.stat` deltas work. VM/backend/device-level cache
   identity is unestablished; the observer is a harness change needing
   prospective seals. No product arm was collected; numbers stay INELIGIBLE.
5. **Combined tree BUILT** (`8dcc47206`, product tree `cb23d730b`), first-parent
   chain `29fc5747d` → `6f18a5f42` → `24239e7e0` (C1 merge) → `f982970a2` (#263
   merge) → `1e1e9fef2` (#269 merge) → `8dcc47206` (ledger). Per-commit LOC
   (Core): 68,031 → 70,093 (+2,062) → 70,113 (+20) → 70,439 (+326) → 70,457
   (+18); combined 133,448 → 135,874. Two #258 semantics were **ported into
   `active_rename.rs`** (fail-closed `preflight_rename_paths` incl.
   never-resident subtrees; per-descendant `ActiveOrigins`), turning the
   #263-era inherited suite from 5/7 to **7/7**; `Inspect::InodeReadlink` and
   `BackingStatus.metadata_writes` (+ arena counter) were ported; the retired
   RootOwner mutation route was **not** resurrected; the #269 identity-relative
   rename/namespace rework, `rename_preflight` and orphan `ancestry.rs` were
   dropped as that retired route.

## EXECUTION ORDER — THE REMAINING ~30%

### Task 1 — Complete the combined-tree union (the main blocker)

On a new owned worktree based on `codex/issue273-combined-functional`, close the
two identified semantic gaps. Current failing state at `cb23d730b`
(Linux/owned-ext4, `LAYERFS_TEST_BACKING_ROOT` on direct-I/O storage):

- `layerfs-sdk --test inherited_workspace` (#269-era suite): **4/11, 7 FAIL**.
  Causes, all verified by this pass:
  a. `growing_inherited_move_reaches_uncached_descendant_beyond_4096_bytes` and
     `growing_move_keeps_cached_descendant_reachable_beyond_4096_bytes` expect
     moves beyond the 4,096-byte canonical path bound to **succeed** — #264's
     identity-relative resolution removed the path-length dependency, but the
     active view's canonical fallback (`resolve_child_active` in
     `filesystem/active_view.rs`) still resolves inherited names **by path**
     and the ported preflight enforces the bound. **Fix: port the canonical
     fallback to identity-relative resolution** — `Inspect::ChildAttributes
     {parent, name}` / `Inspect::InodeAttributes {serial}` /
     `Inspect::InodeList {serial, …}` already exist in the contract; use them
     instead of path queries, and revisit which parts of the path-bound
     preflight remain honest once resolution no longer depends on paths.
  b. `a_successful_directory_rename_seals_once_and_refunds_exactly` reads
     escrow 0 where the #269 route held 851,968 bytes — the active publication
     holds no metadata escrow. Decide and document: adapt the expectation to
     the active route's real refund points (do NOT delete the proof) or port
     the reservation shape; the one-attempt property itself already passes.
  c. `pinned_directory_retains_forgotten_ancestors_and_detached_parent_refuses_mutation`
     — a lookup the #269 route refused resolves successfully on the union;
     the detached-parent refusal shape differs. Align the active route's
     refusal with the #264 semantics or record a bounded divergence ruling.
  d. The remaining failures in that suite are variants of (a)/(b)/(c); each
     must end PASS or carry an explicit, reasoned disposition — never a
     deleted or weakened assertion.
- `layerfs-workspace --test readable`: **3/9, 6 FAIL**, all
  `Service(Unsupported)` — the #264 read-only route's service query surface is
  incomplete on the union. Enumerate exactly which queries the route issues,
  implement them in the service handler set, and keep the wire closed.

Constraint: this is **semantic integration, not re-architecture**. Keep the
active backing as the mutation route; do not resurrect the RootOwner overlay;
keep `mount/exec/commit/status/unmount` behavior unchanged; keep every file
under 999 physical lines (lib/mod ≤ 200); update the affected architecture
documents in the same commit; record first parent, actual conflicts and
resolutions, and per-commit production LOC
(`python3 tools/production_loc.py --json --root <git-archive-snapshot>` on the
first parent and the staged tree for EVERY commit, docs-only included).

### Task 2 — Rebuild and re-prove on the completed tree

After task 1: rebuild the daemon image from the completed tree
(`cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked
--target aarch64-unknown-linux-musl -p layerfs-daemon`; image FROM the pinned
alpine digest `sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`,
`COPY layerfs-daemon /layerfs-daemon`, ENTRYPOINT `["/layerfs-daemon"]`; the
sandbox owner requires a **digest-pinned** image reference). Then:

- Re-run the **SDK view-lease live proof** on the completed tree (the previous
  proof ran at `6f18a5f42`; the combined image was never rebuilt).
- Run the **full step-8 matrix** on the exact clean tree, in an owned
  Linux/ext4 container: namespace/ancestry (inherited never-resident and
  resident moves, deep component-relative lookup/list/readlink, move-back,
  replacement, cycle, detached-parent rejection, old canonical heads); active
  backing + G1/G2 (tiny append/inherited Base/Zero/generic overlap/Payload,
  selected frozen HotDirectory slot epoch, independent old-G1/new-G2 bytes,
  same PID/fd/inode across capture/SaveFile/C5/continued WRITE); handles and
  aliases (pinned handles, rename/unlink/forget/open-unlinked, stale reads);
  quota/custody (prepublication quota/Budget refusal leaves revision/bytes
  unchanged, known C1 + local C5 failure retains custody, partial/unknown
  physical cleanup fails closed, verified unlink/refund); and the **original
  #248 public 4,097-write functional gate with anchored C1 edit-load-zero
  provenance** — a new functional proof on the changed source, never a repeat
  of an unchanged performance arm and never a promotion of the nine historical
  INELIGIBLE rows.
- Run affected locked Core tests, warning-denying Clippy, fmt, product
  boundary + self-tests, `git diff --check`. For the Linux-gated suites, use
  the **registered route drivers** (`core/crates/layerfs-workspace/tests/
*_route.py`); a direct `cargo test` inside a container fails with
`Backing(Acquire, Unsupported)` on overlayfs (O_DIRECT) or
`Service(Unsupported)` (Native fixture needs the route-driver environment) —
that is an environment artifact, not a product failure, and was verified this
pass. Environment recipe that worked: privileged container with
`--device /dev/fuse --cap-add SYS_ADMIN --cap-add MKNOD`, an ext4 Docker
volume for `LAYERFS_TEST_BACKING_ROOT`, `TMPDIR` and `LAYERFS_STAGE_TEST_ROOT`
(direct-I/O storage required), rust:1.85.1-bookworm.

### Task 3 — Close the SDK falsifier gaps

Still unproved on the live route: (i) deterministic deadline forcing; (ii)
Budget/quota exhaustion beyond the 32-lease bound (the 33rd-lease Capacity
refusal is the charged-registry path; response-buffer Budget refusal is not);
(iii) a known-successful C1 Commit followed by a local C5 failure with a held
lease (view survives, custody retained) — the #263-era fixture's CommitGate
(`inherited_workspace.rs`) and the stage fixtures' fault gates are the pattern;
(iv) an uncertain-release custody outcome (retained cohort, workspace stops,
token spent). Implement deterministic proofs or record honest NOT_RUNs with
the exact missing mechanism. The registered clean/one-edit benchmark controls
remain NOT_RUN until a prospectively sealed harness identity exists for them.

### Task 4 — Capability completion or honest closure

Options in order: (a) independently verify a phase-local memory-peak
substitute (e.g. per-phase `memory.current`/`memory.stat` sampling with
`memory.reclaim`-enforced equal container-cache state, prospectively sealed as
a harness/observer change); (b) establish VM/backend/device-level cache
identity for frozen control and candidate, or (c) report the capability as
PARTIAL/falsified and keep the numeric campaign NOT_RUN/INELIGIBLE. Do not
collect a product performance arm during capability research; do not credit
warm pages to a timed phase; a successful write of `0` to `memory.peak` with
unchanged readback is NOT a reset (already falsified twice on this host).

### Task 5 — Reviews and decision

Re-verify the PR dispositions against the then-current heads (the ledger table
in `PREMERGE-COMPLETION-20260928.md` §1 is the baseline). If the union is
complete and the step-8 matrix passes, produce a reasoned MERGE/NO-MERGE
recommendation per PR with the new evidence. If gates remain unsatisfied,
finish with the NO-MERGE handoff — do not invent success.

## RUNNING RULES

Keep every command under three minutes; split and poll. Worktree-local Cargo
targets; measurement lock is per worktree. No third-party patch; locked
builds; no new dependency when an existing crate provides the capability. No
fsync on Workspace backing. One construction worker except namespace Init;
`LAYERFS_CONSTRUCTION_WORKERS=1` for any measured route. No CI/preflight claim
(there is no CI; `tools/preflight.sh` is permanently retired). Count exact
production LOC with the same counter for first parent and committed tree on
EVERY commit, including docs-only (delta 0), and put it in the commit message.
Never convert a failed gate into PASS by weakening it; never relabel or
rewrite a historical receipt; never claim "pre-existing" without a paired run.
Report FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN as plainly as PASS.

## TERMINAL HANDOFF

Deliver: current PR heads/bases/dispositions; the completed (or still-partial)
union with per-commit LOC; task-2 matrix results with exact commands, host,
profiles/seals, PASS/FAIL/NOT_RUN per cell and cleanup; SDK falsifier closure
status; capability verdict and numeric eligibility (expected: still
INELIGIBLE unless every gate in `PREMERGE-COMPLETION-20260928.md` §7 holds);
#256 and #270 explicitly NOT_PROVED; and a reasoned MERGE/NO-MERGE
recommendation. Do not merge any GitHub PR, close #276, or claim release
admission merely because the union and functional checks succeeded.

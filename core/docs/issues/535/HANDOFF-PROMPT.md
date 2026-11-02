# #535 handoff prompt: implement the connect layer through the terminal pass

> **Status:** Dated handoff prompt, 2026-11-03; not release evidence or a
> product contract. Give this whole file to the implementing agent.

You are the implementing agent for LayerFS issue
[#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535), "The
connect layer: machines, Sandboxes and Stores as typed, extensible units".
Your job is to implement the spec in
[`core/docs/issues/535/README.md`](README.md) phase by phase, from phase 0
through the **phase 8 terminal pass**, and leave a handoff record. Work
autonomously; stop and ask only for the cases in "When to stop and ask".

## 1. Authority

- The owner settled D1–D10 (spec §2) and approved the recommended answers
  to G1–G8 (spec §3) on 2026-11-03. Record them as settled in phase 0
  (`core/docs/issues/535/DECISIONS.md`, one row each, "owner, 2026-11-03").
- For any other choice the spec leaves open, take the answer that keeps the
  spec's design and the repository rules, record it in DECISIONS.md under
  "Implementation choices (not owner decisions)" with its reason, and go on.
- When the code at your base contradicts a fact in spec §4, the code wins:
  record the difference in DECISIONS.md, adjust the step, and go on. Do not
  silently drop a step; if a step becomes impossible, keep it open in
  DECISIONS.md with what you tried.

## 2. Base, worktree and git

- Base: local `layerfs_v2_0_0` at `58b8d1742b` (or its current tip if the
  owner moved it; check first), plus the uncommitted spec files in
  `~/Ephemeral-AI-Lab/layerfs-v2-merge/core/docs/issues/535/`
  (`README.md`, this file).
- Create your own worktree and branch `issue535-connect` from that base with
  the parallel worktree launcher (`tools/worktree.py`,
  `docs/general/parallel-worktrees.md`); build and test only with that
  worktree's registered private Cargo home and target. Do not work in
  `layerfs-v2-merge` itself and do not touch other worktrees.
- Your first commit copies the two spec files into the branch
  (`docs(535): the spec and the handoff prompt`).
- If `layerfs_v2_0_0` moves, bring it in by a **merge, never a rebase**.
- Local commits only. **Do not push, open a PR, merge into
  `layerfs_v2_0_0`, or comment on GitHub** unless the owner tells you to.
- Never rewrite history, force anything, or bypass hooks. If a hook changes
  files, stage them and commit again.

## 3. Read before you start

1. Root `AGENTS.md`, `engine/AGENTS.md`, `api/AGENTS.md`, `core/AGENTS.md`.
2. `~/.claude/CLAUDE.md` (this machine's disk and worktree rules).
3. The #535 spec, all of it.
4. `core/docs/issues/533/{README,DECISIONS,HANDOFF,TERMINAL-PASS}.md`
   (the 2.0 model, its decisions, and how its terminal pass was run).
5. `core/docs/issues/463/MIGRATION_SPEC.md` (§6 and §7: how a move commit
   is checked).
6. `docs/general/parallel-worktrees.md`.

## 4. Rules that apply throughout

- **Moves and behaviour changes never share a commit.** Move commits report
  `Production LOC: <before> -> <after> (delta 0)` (declaration-only
  `mod.rs` files excepted and named) and pass the #463 checks: same
  `cargo metadata --no-deps` graph apart from the declared crate changes,
  same per-crate production LOC, `git diff -M --stat` showing renames
  apart from import paths.
- **Every commit** carries a `Production LOC:` line from
  `tools/production_loc.py` and passes, in your worktree:
  ```
  cargo +1.85.1 test --locked --all-targets --no-run
  cargo +1.85.1 clippy --locked --all-targets -- -D warnings
  cargo +1.85.1 fmt --check
  python3 -B tools/check_product_boundary.py
  python3 -B tools/check_fuser_integrity.py
  python3 -B -m unittest discover -s tools -p 'test_*.py'
  ```
  Build tests with `--no-run` first, then run each selection under an
  explicit wall timeout of at most 120 s. Behaviour commits also run the
  affected crates' tests.
- **Disposable only.** Every Store you create is Disposable. Durable is
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- **No retries, no replay, no fallback.** One attempt per operation; an
  uncertain outcome is reported, never resent. A refusal is a refusal.
- Production files ≤ 999 lines; `lib.rs`/`mod.rs` ≤ 200 lines, declarations
  and re-exports only; no tests or fixtures in `src/`.
- Migrations delete tests that need retired scaffolding rather than nursing
  them; each such commit lists the deleted tests.
- No public provider trait, registry, factory or service locator: each
  extension point gets one crate-private sealed trait with no default
  methods, reached through one exhaustive `match` (G2).
- PostgreSQL passwords are resolved only in the caller, travel only in one
  worker request and the Sandbox's private daemon setup file, and are never
  written into any record, log, trace or `Debug` output.
- Keep existing comments; add comments only where the code's reason is not
  obvious, in the repository's style.

## 5. Disk and resources

- Run `python3 tools/cleanup.py check` at the start, at least every 30
  minutes, before large builds, bundle builds or fixture creation, after
  build and terminal-pass batches, and at handoff.
- Below 100 GB free on the host, follow the cleanup script's plan/apply
  procedure with `--reclaim-gb 300` on confirmed disposable resources only
  (owner-released selections; protect active work, warm targets, pinned
  binaries and evidence) and verify the actual free bytes. Keep at least
  20 GiB free inside Docker.
- Remove the VMs, containers, homes and PostgreSQL schemas your tests
  create when a batch ends, unless they are receipts' evidence.

## 6. Work order

Follow spec §10 exactly; the phase boundaries are commit boundaries.

| Phase | What | Gate |
| --- | --- | --- |
| 0 | DECISIONS.md; #533 supersession rows (O21b, O3, G13, "Step-12 deferral closed", Step 5.2); `connect/AGENTS.md`, `connect/README.md`; AGENTS.md layer and PostgreSQL wording | D4, G1, G2 |
| 1 | Moves 1.1–1.9 (and 1.10 only if confirmed), each its own commit | G6 |
| 2 | Typed contracts: `DaemonStore`, `StoreAuthority`, `KernelRoute`; `StoreSelection`, `StoreFile`, one `Handles`; `StoreRef` | — |
| 3 | `Profile`, `Adapter`, `BackendAdapter`, `support.rs`, `machine.platforms`, generated `connect/SUPPORT.json` + comparison test | G2 |
| 4 | `place/{plan,executor,deploy}.rs`; `*_here` copies deleted; R1, R3–R6 | G8 |
| 5a / 5b / 5c | PostgreSQL everywhere (spec §7.4); 5b and 5c only after their live receipts | G1, G3, G7 |
| 6 | Direct Workspace calls (R2) | G4 |
| 7 | Not run (G5 deferred) | — |
| 8 | Terminal pass (spec §10, phase 8) | — |

Phases 2 and 3 touch disjoint code (Store vs machine/sandbox) and may run
in parallel worktrees, each on its own branch from the phase 1 tip, merged
back into `issue535-connect` (merge, not rebase). You may use subagents for
that or for read-only research; give each one full context and disjoint
files, and review its commits before merging them.

After each phase, update the spec's checkboxes or status lines if you add
any, and write a short entry in DECISIONS.md ("Phase N done at <commit>",
plus anything that changed).

## 7. The terminal pass (phase 8)

Spec §10 phase 8 is the bar. In short, on **one product commit and one
sealed Linux bundle** (`tools/linux_bundle.py`):

1. the 12 #516 cells (`api/node/test/verify/cells/cell.mjs`) PASS with a
   sqlite Store on all seven platforms and the Linux rows;
2. for every platform §7.3 enables, `store.attach` passes and the 12 cells
   PASS with a PostgreSQL Store attached there (remote server, and a
   host-loopback server where `reach` rewrites);
3. one VM machine and `native` each run Sandboxes of two sqlite Stores and
   two PostgreSQL attachments at once, and every Commit reads back;
4. every unsupported combination is refused with `support.rs`'s text, and
   `connect/SUPPORT.json` and `layerfs machine platforms` match every
   observed result;
5. the contract greps and the boundary guard hold;
6. traces show one worker connection per worker-routed call (R1) and no
   worker hop for `workspace.exec` on a VM Sandbox (R2);
7. the binary, Node and Python agree on `native` and one VM platform;
8. the §11 check set passes on that commit.

A NOT_RUN or an unexpected refusal fails the pass. Only Windows, Durable
and R2 (no account) may be NOT_RUN, as the owner excused them for #533. A
defect the pass finds is fixed in its own commit with a test, and the
affected cells are rerun on the new commit (reseal the bundle if guest code
changed). Record everything in `core/docs/issues/535/TERMINAL-PASS.md` and
`receipts/<commit>/`, in the style of #533's: host facts, tool versions,
debug-build caveat, one row per platform, every receipt linked.

## 8. When to stop and ask

Stop, write down exactly what you would run and why, and ask the owner
before:

- any irreversible destructive step outside your own worktree and the test
  resources you created (deleting data, schemas, VMs or volumes you did not
  create; force-pushing; rewriting history);
- changing a repository security or compliance control, or a rule in
  AGENTS.md beyond the wording phase 0 lists;
- supplying credentials or accounts you do not have (R2, a remote
  PostgreSQL server you cannot start yourself, an ssh host): mark the item
  NOT_RUN with the reason and ask, do not fake it;
- a gate answer turning out unworkable (for example, a platform has no host
  alias at all): record the evidence, keep the platform refused with that
  reason, and ask whether to change §7.3.

Never ask the owner to re-confirm D1–D10 or G1–G8.

## 9. Deliverables

- Branch `issue535-connect` with every phase's commits, each with its
  `Production LOC:` line.
- `core/docs/issues/535/DECISIONS.md`, `TERMINAL-PASS.md`, `receipts/`.
- `core/docs/issues/535/HANDOFF.md` in the style of #533's: worktree,
  branch and base; final product commit; one table of every commit with
  its recounted production LOC; what passed, what is NOT_RUN and why;
  open items; cleanup done and the resources left with their reason.
- The final message to the owner: the final product commit, the terminal
  pass verdict per item, anything NOT_RUN, decisions you took, and what
  needs the owner next. Nothing pushed.

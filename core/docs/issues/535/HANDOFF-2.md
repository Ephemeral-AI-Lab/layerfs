# #535 + #536 handoff prompt: from #535 step 2c through one combined terminal pass

> **Status:** Dated handoff prompt, 2026-11-03; not release evidence or a
> product contract. Give this whole file to the implementing agent.

You continue LayerFS issues
[#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535) (the connect
layer) and [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536)
(`layerfs-control`, the management layer) on one branch, in the **interleaved
order of §5**, through **one combined terminal pass** (§6). The previous agent
stopped after #535 step **2b**; you start at **2c**. Work autonomously; use
subagents in parallel where §5a allows; stop and ask only for the cases in §8. [#537](https://github.com/Ephemeral-AI-Lab/layerfs/issues/537) (the
Connect UI) is a follow-up: read it for context, do not implement it.

## 1. Authority

- Owner decisions: #535 D1–D10 and G1–G8 (with **G3 as revised**, §3), #536
  C1–C7, and the recommended answers to #536 H1–H11. All are settled; never
  ask the owner to re-confirm them.
- **The owner chose to interleave #535 and #536 (2026-11-03)** so that each
  piece is built once in its final place. This supersedes #536's "starts
  after #535's terminal pass" and #535's separate terminal pass.
- Where the code contradicts a spec fact, the code wins: record the
  difference in the issue's DECISIONS.md, adjust the step, continue. Never drop
  a step silently; keep an impossible step open in DECISIONS.md with what you
  tried.
- Choices the specs leave open: take the one that keeps the design and the
  repository rules, record it under "Implementation choices" with the reason.

## 2. Branch, worktree and git

- Worktree `~/Ephemeral-AI-Lab/layerfs-issue535-connect`, branch
  `issue535-connect`, base local `layerfs_v2_0_0` `58b8d1742b`. Phase 0 is at
  `451916521e`, phase 1 at `791d510b18`, the refreshed spec at `7a35d4b655`,
  2a (`DaemonStore`, `KernelRoute`, `LAYERFS_KERNEL` gone) at `dffd241aee`;
  find 2b (persistence `StoreSelection`, `StoreFile`, one `Handles`,
  store-service fake path removed) in `git log` and DECISIONS.md. If 2b is not
  fully committed or the tree is not clean, stop and report (§8) before
  touching anything.
- Build and test only with this worktree's registered private Cargo home and
  target (`tools/worktree.py`, `docs/general/parallel-worktrees.md`).
- If `layerfs_v2_0_0` moves, bring it in by a **merge, never a rebase**.
- Local commits only: **no push, PR, merge into `layerfs_v2_0_0`, or GitHub
  comment** unless the owner says so. Never rewrite history, force anything
  or bypass hooks.

## 3. Finish correcting direction (your first commit; docs only, LOC delta 0)

`7a35d4b655` already brought in the owner's revised #535 spec (G3 without
attach, §9a). First **verify** it: the branch's `core/docs/issues/535/README.md`
must match `~/Ephemeral-AI-Lab/layerfs-v2-merge/core/docs/issues/535/README.md`
(740 lines; no `store.attach`; §9a has `machine.start`/`machine.stop`, the
`machine.platforms` shape and `supports`). If it differs, copy the current one.
Then do what is still missing, in one commit:

1. Copy `536/README.md`, `537/README.md` and this file (`535/HANDOFF-2.md`)
   from `~/Ephemeral-AI-Lab/layerfs-v2-merge/core/docs/issues/` into the branch.
2. `core/docs/issues/535/DECISIONS.md`: make sure the G3 row reads as revised
   (no attach; every PostgreSQL Store usable from every machine where §7.3 /
   `support.rs` allows; reach checked inside `sandbox.create`; per-machine URL
   deferred) and that owner rows exist for `machine.start`/`machine.stop`, the
   `machine.platforms` shape replacing `can_create`, per-machine `supports`,
   list/get/status never starting a machine, and the docker default image.
   Add: interleaving with #536 (§1); R3 skipped (superseded by #536 layout 3);
   G4's daemon keys go to `sandbox/<name>/control.json` (#536 §5).
3. Create `core/docs/issues/536/DECISIONS.md`: C1–C7, H1–H11 (recommended
   answers, owner-approved 2026-11-03), the interleaving row.
4. Root `AGENTS.md` "Machines" paragraph: no "PostgreSQL attachments
   (`store.attach …`)" or "attached to"; a PostgreSQL Store is usable from
   every machine where the support table marks the platform supported. Check
   that 2b's sweep renaming `Handles` to `SqliteHandles` in `AGENTS.md` and the
   architecture docs matches the code and is recorded in DECISIONS.md (the
   unified type is `Handles`; record the naming if it differs).
5. One pointer line each in `535/README.md` §13 and `536/README.md` header
   and §11 to this file for the interleaved order.

## 4. Read before you start

Root `AGENTS.md`, `core/`, `engine/`, `connect/`, `api/` AGENTS.md;
`~/.claude/CLAUDE.md` (disk and worktree rules); #535 README and DECISIONS;
#536 README (all of it, especially §5–§10b); #537 README (context);
#533 README and DECISIONS; #463 MIGRATION_SPEC §6–§7.

## 5. Work order (interleaved)

Every step's phase boundaries are commit boundaries. **Moves and behaviour
changes never share a commit.** Each commit carries
`Production LOC: <before> -> <after> (delta <signed>)` from
`tools/production_loc.py` and passes:

```
cargo +1.85.1 test --locked --all-targets --no-run
cargo +1.85.1 clippy --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --check
python3 -B tools/check_product_boundary.py
python3 -B tools/check_fuser_integrity.py
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

Run each test selection under an explicit wall timeout of at most 120 s,
after building with `--no-run`. Disposable Stores only; Durable is
`NOT_RUN — disabled by owner until explicit reauthorization`. One attempt per
operation; no retries, no replay; uncertain outcomes are reported and left
open. Production files ≤ 999 lines; `lib.rs`/`mod.rs` ≤ 200 lines,
declarations only; no tests in `src/`. Private sealed traits reached through
one exhaustive `match` only (G2).

| Step | Issue / phase | What |
| --- | --- | --- |
| A | — | §3 correction commit |
| 2c | #535 phase 2 | api `StoreRef` parsed once from the Store record, replacing every `get_text("kind") == Some("postgresql")` check in `layerfs-api` (16 at the time of writing) and `host/stores.rs` `identity_of`; then "Phase 2 done at …" in DECISIONS.md |
| B | #535 phase 3 | `Profile` table and `Adapter` in `layerfs-machine`; `BackendAdapter` in `layerfs-sandbox`; `support.rs`; `machine.platforms` with the §9a result shape (replacing `can_create`); per-machine `supports`; generated `connect/SUPPORT.json` and the test comparing it and every refusal to `support.rs` |
| C | #536 phase 1 | Moves (delta 0), in #536 §11's order: Store calls → `layerfs-store-service/src/methods/` with their tests; `api/crates/layerfs-control` created, organised by entity (§9.2); `home/`, `machine/` + `worker/`, `store/`, `project/` + `backup/` (incl. `link.rs` from 1.6b), `sandbox/` + `workspace/` + `daemon/`; `layerfs-api` keeps `calls/` and `lib.rs`. Guard, LOC counter, CI per commit (§9.6) |
| D | #536 phase 2 | Deletions of the 1.1 paths (one commit per group, each listing the tests it deletes); `host.call`/`host.backend` → typed `project.remove` |
| E | #535 phase 4, **inside `layerfs-control`** | `common/place.rs` (Executor Local/Worker/Daemon) and `sandbox/deploy.rs` with the `*_here` copies deleted; a Sandbox always launches through its machine; docker Worker executor on every host and the `Platform::route` comment corrected (G8); `machine.start`/`machine.stop`; every list/get/status call checked not to start a machine; docker default image from the bundle; R1, R4, R5, R6. **R3 is not built** |
| F | #536 phases 3–5 | Typed records with `v`, `to_sdk`, `from_record`, `state()`; `inventory.rs`; layout 3 (`sandbox/<name>/`, Workspace records, thin guest records, native `run/`, conversion `migrate/v2.rs` refusing while live, worker cleanup of old numbered folders); the one `op` intent; per-entity locks; leases for both Store kinds; pid-only liveness checks removed |
| G | #535 phase 5a | §7.4 without attach: namespaces `binds`; `Adapter::reach` for native, docker, ssh; docker `ExtraHosts host-gateway` on Linux; the reach check inside `sandbox.create`; the worker takes the resolved URL from the request; refusals removed for native Linux, docker, ssh; `verify-full` rewrite refusal |
| H | #535 phase 6 | G4 direct Workspace calls: `Adapter::forward`; daemon keys in `sandbox/<name>/control.json`; Workspace calls skip the worker |
| I | #536 phases 6–9 | Calls (§10, §10b): `store.push` + `store.pull` replacing `store.move` (with docs, skills, Node/Python tests and the cell runner in the same commit), optional `machine` in `sandbox.create`, Workspace names and handles, `project.init` through a worker (H11), structured refusal `detail` with `Busy`/`Unresolved`, lists carrying `state`/`op`; `secrets.rs`; capacity checks; worker hello version and bundle hash; `op` in status/get; top-level `layerfs doctor` (CLI, Node, Python); the conformance kit |
| J | #535 phases 5b, 5c | Live receipts (G7) for lima, apple-container, microsandbox host aliases, then firecracker NAT; flip their `support.rs` rows only after each receipt |
| K | — | Combined terminal pass (§6) |

#535 phase 7 is not run (G5). #535 step 1.10 stays open unless a pre-2.0
Linux native home is shown converting. After each step, add "Step X done at
<commit>" to the issue's DECISIONS.md with anything that changed.

## 5a. Working in parallel with subagents

Use subagents to go faster, but never at the cost of the commit rules. You
own the plan, every merge, DECISIONS.md, the LOC lines and the final checks.

**How.** Before each step, decide which parts are independent and give each
writing subagent:
- its own worktree and branch made from your current tip with
  `tools/worktree.py` (private Cargo home and target), named
  `issue535-<step>-<lane>`;
- an exact, disjoint set of files or crates it may change, the spec sections
  it implements, and the full rules of §5 (it cannot see this file unless you
  give it);
- the requirement to commit with `Production LOC` lines, run the §5 checks in
  its worktree, and report the commits, check results and anything it could
  not do.

Then review each subagent's commits, merge them into `issue535-connect`
(merge, never rebase), rerun the §5 checks on the merged tip, and only then
start the next step. If two lanes collide on a file, stop one; do not
hand-resolve semantic conflicts without re-running its tests. No subagent
pushes, opens a PR, writes DECISIONS.md or edits AGENTS.md.

**Lanes that are safe in parallel** (disjoint code):

| Step | Lane 1 | Lane 2 | Read-only helpers |
| --- | --- | --- | --- |
| 2c + B | 2c in `layerfs-api` (StoreRef) | B in `connect/crates/layerfs-machine` and `layerfs-sandbox` (Profile, Adapter, BackendAdapter, `support.rs`, `SUPPORT.json` generator); the `machine.platforms` call-table entry and `supports` fields go in **after** 2c merges | map every `layerfs-api/src` file to its #536 §9.2 destination and verify it against the code, ready for C |
| C | Store calls → `layerfs-store-service/src/methods/` with their tests (C1) | — (C2–C7 are sequential moves; one lane) | verify which `host/` functions are reachable only from 1.1 op names, ready for D |
| D | one subagent per deletion group, each on disjoint files, merged one at a time | | |
| E + G (connect parts) | E in `layerfs-control` (place, deploy, start/stop, R1, R4–R6) | G's connect/engine parts: namespaces `binds` (`layerfs-sandbox`, helper), `Adapter::reach` per platform, docker `ExtraHosts` (`layerfs-machine`, `layerfs-sandbox`); its `layerfs-control` parts wait for F | |
| F | records and `inventory` | layout 3 conversion (`common/home/migrate/v2.rs`) once records merge | |
| I | calls and refusals | `secrets.rs` + capacity checks | the conformance kit in `layerfs-machine/tests` and `layerfs-sandbox/tests` |
| J | live G7 checks, one platform at a time (they share this Mac's VMs and disk) | | |

**Limits.** At most three writing subagents at once, and only one building
when host free space is under 150 GB or Docker's under 30 GiB (§7). Live
checks on machines (Docker, VMs, ssh) run in one lane only. A read-only
helper may run beside any lane. Record in DECISIONS.md which subagent did
which commits.

## 6. Combined terminal pass

On one product commit and one sealed Linux bundle (`tools/linux_bundle.py`),
Disposable Stores only, recorded in `core/docs/issues/535/TERMINAL-PASS.md`
(covering both issues) with receipts under `receipts/<commit>/`. It passes
only when every item passes; a NOT_RUN or unexpected refusal fails it, except
Windows, Durable and R2 without an account.

1. #535 §10 phase 8 items 1–9, using `store.push`/`store.pull` instead of
   `store.move` and without attach.
2. #536 §12 items 2–7 (conversion, intents, locks, layout, size, checks).
3. The #536 §10b contract holds: lists carry `state`/`op`; no list/get/status
   call starts a machine; the call-by-state table; push/pull refused while
   any Sandbox record exists; progress fields; Workspace handles; `related`
   on `InUse`; `store.remove` on PostgreSQL refused naming `store.disconnect`.

## 7. Disk and resources

`python3 tools/cleanup.py check` at the start, at least every 30 minutes,
before large builds, bundle builds or fixture creation, after build and pass
batches, and at handoff. Below 100 GB free, run the plan/apply procedure with
`--reclaim-gb 300` on confirmed disposable resources only and verify actual
free bytes. Keep at least 20 GiB free inside Docker. Remove the VMs,
containers, homes and PostgreSQL schemas your tests create when a batch
ends, unless they are a receipt's evidence.

## 8. When to stop and ask

Stop, write down what you would run and why, and ask before: an irreversible
destructive step outside your worktree and the test resources you created; a
change to a repository security or compliance control or an AGENTS.md rule
beyond §3; credentials or accounts you do not have (mark the item NOT_RUN with
the reason); a decision that turns out unworkable (record the evidence, keep
the case refused with that reason, and ask).

## 9. Deliverables

- Branch `issue535-connect` with every step's commits.
- `core/docs/issues/535/DECISIONS.md` and `536/DECISIONS.md` up to date;
  `535/TERMINAL-PASS.md` and `receipts/`.
- `core/docs/issues/535/HANDOFF.md`: worktree, branch, base; final product
  commit; one table of every commit with its recounted production LOC; what
  passed, what is NOT_RUN and why; open items; cleanup done and resources left
  with their reason.
- The final message to the owner: the final product commit, the terminal pass
  verdict per item, anything NOT_RUN, decisions taken, and what needs the
  owner next. Nothing pushed.

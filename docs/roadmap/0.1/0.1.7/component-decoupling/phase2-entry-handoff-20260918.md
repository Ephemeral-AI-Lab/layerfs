# Phase 1 closure and Phase 2 entry handoff (2026-09-18)

> **Status:** Closure record and entry conditions. **Phase 1 is closed by owner
> ruling** on the tree `2137c8487`, with 14 of its 16 items landed and two
> (`P1-13`, `P1-15`) recorded as incomplete with a diagnosed cause. This document
> does two things: it states what Phase 1 leaves behind, and it lists the rulings
> and prerequisites Phase 2 needs before its first item can be implemented. It does
> **not** start Phase 2.

## 1. What Phase 1 leaves behind

| | |
| --- | --- |
| Final tree | `2137c8487`, clean, pushed. The untracked `core/docs/benchmark/` (Stage 6, issue #182) is **not** part of it and was never staged |
| Landed | C1, V1–V4 (prerequisites) and P1-1…P1-12, P1-14, P1-16 — **14 of 16** |
| Open | **P1-13** (merge fanout 4) and **P1-15** (hybrid restart lookup): both attempted, both found latent correctness defects, both reverted rather than landed with a known defect. Receipts: [`rounds/p1-13/receipt.md`](../evidence/phase1-execution-20260918T090000Z/rounds/p1-13/receipt.md), [`rounds/p1-15/receipt.md`](../evidence/phase1-execution-20260918T090000Z/rounds/p1-15/receipt.md) |
| Closing pass | [`closing-pass.md`](../evidence/phase1-execution-20260918T090000Z/closing-pass.md) — run as a **completion audit** of a stopped phase, with the frozen set re-collected on the final tree in `rounds/closing-pass/after/` |
| Checks | all eight exit 0; workspace **456 passed / 0 failed**; the 34-test sealed-oracle set green with its targets byte-identical to the phase-start tree |
| Production LOC | **18,792 → 19,264 (+472)**, core only; **no new production files**; reference `crates/` unchanged at 65,417 |
| Counter summary | D26 `rows_read` 59,007 → 25,809; D27 `nodes_read` 9 → 7; M1 peak delta 262,328 → 131,826; D25 still 0 spills at the shipped ceiling. P1-13's and P1-15's targets are **not** delivered |

### 1.1 Two Phase 1 findings that are still open rulings

Neither blocks Phase 2 mechanically, but both are contract deviations that Phase 2
would inherit silently if nobody rules on them:

1. **A third pinned test moved.** `P1-7` lowered `EditCounters::nodes_read`, which is
   what `edit_reference` (22 → 20, controls 4 → 3 and 4 → 3) and `edit_localized`
   (7/6 → 6/5) observe. Each was tightened with its old value kept as an upper bound;
   the plan's §P1-7 line says "MUST CHANGE: none". **Ruling needed:** accept the
   deviation, or rework P1-7 into the C1/P1-1 generalization pattern.
2. **A named bound changed without its architecture document.** `P1-10` moved the
   touched-serial divisor `ordering_bytes / 8` → `/ 16` with no
   `core/docs/architecture/` update in the same commit, against `core/AGENTS.md`.
   **Ruling needed:** accept, or add the missing document update.

## 2. The rulings Phase 2 needs before P2-1 is implemented

These are owner decisions, not engineering choices, and three of them are the same
class of thing Phase 1's own rules were written to protect. **No Phase 2 item should
be started until §2.1–§2.4 are answered.**

### 2.1 P2-1 is a cache-policy change, and cache policy is owner-ruled

`P2-1` (O1) proposes `cache_size` 2 MiB → the reference's 32 MiB with `cache_spill`
ON → OFF. Phase 1's out-of-scope list names exactly this: *"any worker count, timeout
or cache policy"*. The plan also **deprioritized** O1 on evidence: P0-2 proved no
spilling occurs at 1× or 4× the transaction ceiling, so the write-path premise is
unsupported and the read-path half was told to wait for P1-2 — which has now landed.
**Ruling needed:** is P2-1 authorized at all, and if so on which measured shape?

### 2.2 Does Phase 2 inherit P1-13 and P1-15, or do they stay open?

Two Phase 2 items sit on the ordering machinery P1-13 was meant to rewrite:

* **P2-7** (`drop copy_run in consolidate()`) targets `runs.rs`. P1-13's cascade
  rewrite would have moved that code. Since nothing landed, P2-7 rebases cleanly on
  today's cascade — but P1-15's restart probe would have touched `find` in the same
  file. **Ruling needed:** are P1-13/P1-15 closed as not-delivered, deferred into
  Phase 2, or carried as a standing debt with their receipts?
* Note the asymmetry: P1-13 and P1-15 are the two items whose attempts **found latent
  correctness defects** (`find` and the newest-first scan disagreeing about a serial;
  a stale row crossing tiers). That is unfinished verification of existing behaviour,
  not only an optimisation left on the table. If they are closed, that finding should
  be recorded as an open question about the ordering store rather than dropped.

### 2.3 The single-worker rule and the parked O4

`O4` (producer pool) is parked because it conflicts with the single-worker rule in
`AGENTS.md`. Two Phase 2 items are parallel/engine-shaped (`P2-2` multi-row INSERT,
`P2-3` `locking_mode = EXCLUSIVE`). **Ruling needed:** confirm the single-worker rule
still governs Phase 2 measurements, so `P2-2`'s A/B does not quietly become a
parallelism change.

### 2.4 `P2-3` contradicts a stated contract

`P2-3` proposes `locking_mode = EXCLUSIVE`, which the item itself notes must be
reconciled with the one-save-owner contract: "a read-only Store must not take it."
**Ruling needed:** is the contract amended, or is the item scoped to the write owner
only?

## 3. What Phase 2 does *not* need to redo

Available as-is from Phase 1, and usable by Phase 2's measurements without change:

* the frozen workload set, its `CONTRACT.md`, and the fresh-`--output` append-only
  discipline (`rounds/` is the working example);
* the probe client, rebuilt per tree by `collect.py`'s `B2` with sha256 recorded in
  `artifacts.txt` — the stale-client trap that invalidated one Phase 1 round is
  fixed in the driver;
* the vehicle extensions V1–V4 and the additive `M4`/`--case split` row, all
  documented as additive in `ROUND-README.md` corrections 3 and 4;
* the eight checks and the per-commit production-LOC discipline;
* the author-verification protocol, with 18 `verify-*.md` files as its worked form.

## 4. Entry checklist for the first Phase 2 item

1. §2.1–§2.4 answered on #178 and recorded in the plan.
2. A Phase 2 plan document in this directory — **there is none today**. Phase 1 had
   `phase1-implementation-plan-20260917.md`; Phase 2's eight items currently exist
   only as checklist lines on #178 with no consolidated plan, no anchors table and no
   landing order.
3. Each item's before-anchor identified on the frozen set, and its counter chosen
   **before** implementation — Phase 1's P1-3 prediction was refuted only because the
   prediction was written down first.
4. A receipt directory created per item, in the same append-only layout.
5. The `P2-`` items that touch `core/crates/layerfs-storage` need that workspace's
   checks in addition to `core/Cargo.toml` — the Phase 1 checks cover the whole core
   workspace, so this is already satisfied, but state it per item.

## 5. Explicitly out of scope for this handoff

Phase 2 implementation, the parked register (O4, O5, branch-row summaries, pack-BLOB
append rewrite, membership single-hash, persisted pool-index cursor), the
pending-ceiling default (P1-16 documents it; re-defaulting is an owner ruling), and
the two untracked Stage-6 documents. Nothing in this document changes a default, a
worker count, a timeout or a cache policy.

# Planning prompt — Phase 7 cluster 1 (storage)

> **Status:** Current planning checklist; no release candidate exists.
> Prompt for the agent that writes the cluster 1 implementation plan. Issue
> [#302](https://github.com/Ephemeral-AI-Lab/layerfs/issues/302), parent
> [#301](https://github.com/Ephemeral-AI-Lab/layerfs/issues/301).

---

You are writing the **implementation plan** for cluster 1 of LayerFS Phase 7.
This task produces a plan document, not product code. Another agent will
implement from your plan, so it has to be specific enough to follow without
re-deriving the design, and honest about what you verified versus what you
assumed.

## The situation

LayerFS is a content-addressed, layered filesystem for agent sandboxes. Its
replacement product lives under `core/`. Two experimental phases (5 and 6)
explored moving storage off a single host-owned SQLite Store; Phase 7 is the
real implementation and deliberately restarts from the last accepted source.

The owner has split Phase 7 into two clusters built in parallel:

- **Cluster 1 — storage (yours):** `layerfs-content` (C1), `layerfs-storage`
  (C2), `layerfs-history` (C5), plus two new engine crates, `layerfs-s3` and
  `layerfs-metadata`. It runs with a PostgreSQL container and a MinIO container
  and nothing else: no sandbox, no daemon, no FUSE mount.
- **Cluster 2 — sandbox (someone else's):** overlay, workspace, FUSE, daemon,
  bridge, sandbox, SDK. It integrates with your cluster only at Commit time.

## Where to read

- **Your worktree:** `/Users/yifanxu/.codex/worktrees/phase7-cluster1-storage/layerfs`, branch `codex/phase7-cluster1-storage`, created at the Phase 4.5 +
  Phase B commit `7edddbdb8e8512627aed0ed42533ef099d802384`. Work only there.
  Every claim you make about existing code must be checked against this commit
  and cite a file path. The `core/AGENTS.md` inside the worktree is the older
  version; the amended one named below governs.
- **Design packet:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/301/`
  (`README.md`, `01`–`06`). It is not committed yet, so read it from that
  absolute path. Start with `README.md`, then `01-architecture.md`,
  `04-storage-interfaces.md` and `06-tables.md`.
- **Rules:** root `AGENTS.md`, `core/AGENTS.md`,
  `core/benchmark/fs-bench-pro/AGENTS.md`, `docs/general/benchmark_rules.md`.
  The amended `core/AGENTS.md` (WAL and sync permitted; PostgreSQL and MinIO
  environment) is also uncommitted in the primary checkout at
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/AGENTS.md`; use that version.
- **Baseline evidence:** `core/docs/issues/286/` at the base commit, in
  particular `SEVEN-FAMILY-CHECKPOINT-20260930.md` (which names the Init and
  history rounds it reuses), the Init reports under `experiments/`
  (`20260929-init-r001.md`, `20260930-init-100000-v4-r041.md`,
  `20260930-init-10000-v4-r042.md`) and the history reports
  (`20260930-history-regression-r046.md`,
  `20260930-history-stride1-regression-r047.md`).

## Simplification is a goal, not a side effect

Phase 7 changes what the product stands on: **PostgreSQL and MinIO as global
storage, and embedded SQLite only inside the daemon.** The owner expects this to
remove a great deal of machinery, and the plan should go looking for it. A large
part of today's storage code exists to make one embedded, single-writer SQLite
Store do jobs that a database server and an object store now do themselves.

Treat every existing mechanism as something to justify, not something to port.
For each one, ask what problem it solved and whether that problem still exists.
Candidates to check against the source — these are leads, not conclusions:

- Writer coordination built for one SQLite file: save slots, the concurrent-write
  limit, hand-rolled allocation of pack and ordinal numbers. PostgreSQL has
  concurrent writers, row locks and sequences.
- Visibility of unpublished work: save identities, publication sequence and pack
  ceilings. With immutable objects registered only after their bytes are stored,
  a registered object is valid for everyone.
- Packs designed to be appended in place inside BLOB rows: the reserved directory
  region, incremental BLOB writes, the open pooled pack. A sealed immutable
  object needs none of that on the write path.
- SQLite-specific tuning and safety work: page-size selection, pragma profiles,
  busy and lock handling, paged cleanup of a local file.
- The host service that existed to own the Store: admission, transport and
  streaming code in `layerfs-server` around the C1/C2/C5 calls.

Prefer deleting to adapting, and one obvious mechanism to two clever ones. The
limits are fixed: canonical identities and formats, authentication of everything
read, bounded memory windows and the repository's measurement and source rules
stay. A simplification is real only if the product code is gone — moving code to
another crate is relocation and must be reported as such.

## Decided — build on these, do not reopen them

- One machine. PostgreSQL and MinIO each run in a local Docker container.
  Multi-machine deployment, remote endpoints and TLS are out of scope.
- PostgreSQL holds committed metadata bodies, locators and history. MinIO holds
  file-content packs only. Nothing else is stored in the bucket.
- Domain crates (C1, C2, C5) hold logic and define ports, and do no I/O.
  `layerfs-s3` implements C2's object-store port. `layerfs-metadata` owns the
  PostgreSQL schema and implements C2's locator/metadata port and C5's existing
  `HistoryCatalog`. Only a composition root names an engine.
- The engines know nothing about Workspace Commit, conflict or edits. Those are
  cluster 2 concepts and must not appear in this cluster's interfaces.
- Canonical identities, CDC, deduplication, FULL/PREFIX/STORED encoding and pack
  framing keep their existing behaviour. This is a storage-boundary change, not a
  format change.
- Objects are immutable: packs are sealed before upload and written with
  put-if-absent; locators are insert-if-absent and the first one wins.
- Ranged reads from MinIO are a nice-to-have, not part of this plan. Keep them
  possible; do not build them.
- Cluster 1 never depends on a cluster 2 crate.

## Open — your plan must resolve each, or put a precise question to the owner

The packet's README lists these with a recommendation. Decide where the source
lets you, and say what evidence decided it.

- **D10** — shape of C2's metadata port. Today C2's SQL is woven into
  `layerfs-storage/src/cas/` and `src/sqlite/`. Find which operations rely on
  sharing one SQL transaction and whether each can become one transactional unit
  across a network.
- **D12** — pooled-metadata packs are kept open and appended in place today.
  Decide how they become immutable, and what that costs.
- **D13** — where namespace Init/import lives once `layerfs-server` is retired.
  It must be reachable without a sandbox.
- **D5 / D11** — the S3 and PostgreSQL clients. A new dependency needs explicit
  owner approval; present the real dependency tree of each candidate.
- **D6** — pack size on an object store. Keep current limits unless counts say
  otherwise.
- The `saves` table and publication ceilings: confirm from `cas/` whether
  anything still needs a save identity.
- Coexistence: whether the C2 rework can be additive so the old host service
  keeps building until it is retired.

## What the plan must contain

Write these four sections, in this order.

**0. Architecture.** The components of this cluster, the single responsibility
of each, the ports between them with their exact operations, the dependency
edges, and the write and read paths end to end. State what each component must
not know. Show where it differs from the packet and why.

Include a **simplification ledger** in this section: a table with one row per
mechanism you remove — what it is, why the new stores make it unnecessary, the
files it lives in today, its production lines at the base (measured with
`python3 core/tools/production_loc.py --detail`, never estimated), and what, if
anything, replaces it. Follow it with a short list of what you deliberately
keep, and why.

**1. File and folder structure.** The full tree for every crate you touch or
add, one line per file saying what it owns. Aim for the smallest structure that
keeps responsibilities separate: split by responsibility, not by numbered parts;
no interface per algorithm; no placeholder modules. Every production file stays
under 1,000 physical lines and every `lib.rs`/`mod.rs` under 200, with product
code only under `src/` and tests under `tests/`. For each existing file, say
whether it is kept, changed, moved or deleted.

**2. Rollout plan.** An ordered list of steps where each step is the smallest
complete slice: what changes, what it depends on, the commands that verify it,
and its exit condition. The tree must build and its covering checks pass after
every step. Cover how the old path coexists and when it is removed, which shared
files (`core/Cargo.toml`, `Cargo.lock`, boundary and LOC tools) both clusters
will touch, and the per-commit production LOC comparison the repository
requires.

**3. Final verification and benchmark.** The owner's acceptance for this cluster
is: **meet the earlier baseline for namespace-init speed, and for the
deepseek-harness history schedules at stride 10, 3 and 1 in both speed and
storage compression.** Specify, for each of those selections, the exact
baseline receipt, the number and limit, the command, the cache contract and how
storage is accounted (MinIO object bytes plus PostgreSQL allocation, not payload
alone). Also specify the correctness verification that precedes any timing.

## What makes this hard

- **The speed baseline may not be eligible as it stands.** The Phase B
  checkpoint records family 1 and family 2 as functional PASS with numeric
  latency `INELIGIBLE` because cache state was unknown. Find out exactly what
  speed evidence exists. If there is no eligible baseline row, say so plainly and
  specify how a matched baseline arm is taken at `7edddbdb8` under a declared
  cache contract — do not quote an ineligible number as the bar.
- **Servers hold their own caches.** Root `AGENTS.md` §1 requires cache state to
  be declared and equal across arms. PostgreSQL and MinIO are long-lived
  processes with buffers; define the cold contract for them. Container start-up
  is setup and stays outside every timer.
- **Every metadata call is now a network round trip.** Init and the history
  schedules write and look up many objects. Count round trips and requests per
  operation and design the port so they are bounded; this is where a speed
  regression against an embedded Store would come from.
- **The pack format assumes appendable BLOB rows.** Read
  `layerfs-storage/src/pack/layout.rs` and `placement.rs` before deciding
  anything about sealing.
- **Storage compression is compared under an owner-approved tolerance.** Family 2
  passed with an up-to-10% allocation deviation. Carry the same accounting rule
  and say what the new stores add or remove.
- **Init has its own rules.** Release binaries only, and it is the one path
  allowed multiple construction workers. Everything else runs one construction
  worker.

## Constraints on how you work

- Plan only. Do not write or change product code.
- Do not invent numbers, sizes or estimates. Quote measured figures with their
  source file; mark anything you could not establish as unknown.
- Do not run benchmarks or repeat measurements to obtain numbers for the plan.
- Do not touch cluster 2 crates or plan work inside them. Where you need
  something from cluster 2 or it needs something from you, state it as an
  interface requirement.
- If the source contradicts the packet, the source wins: report the
  contradiction and adjust.

## What to hand back

One document, `core/docs/issues/302/IMPLEMENTATION-PLAN.md`, in your worktree,
with the repository's status banner and the four sections above. End it with:

- the decisions you made, each with the evidence that decided it;
- the questions only the owner can answer, each phrased so it can be answered in
  one line;
- a short list separating what you verified in source from what you inferred.

Keep it as short as completeness allows. A reader should be able to start
implementing step 1 from the document alone.

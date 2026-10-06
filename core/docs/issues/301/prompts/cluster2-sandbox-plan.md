# Planning prompt — Phase 7 cluster 2 (sandbox)

> **Status:** Current planning checklist; no release candidate exists.
> Prompt for the agent that writes the cluster 2 implementation plan. Issue
> [#303](https://github.com/Ephemeral-AI-Lab/layerfs/issues/303), parent
> [#301](https://github.com/Ephemeral-AI-Lab/layerfs/issues/301).

---

You are writing the **implementation plan** for cluster 2 of LayerFS Phase 7.
This task produces a plan document, not product code. Another agent will
implement from your plan, so it has to be specific enough to follow without
re-deriving the design, and honest about what you verified versus what you
assumed.

## The situation

LayerFS is a content-addressed, layered filesystem for agent sandboxes. Its
replacement product lives under `core/`. A command runs inside a sandbox
container against a FUSE mount; a daemon in that container serves the mount; a
Workspace is the committed base plus the changes made through the mount; a
Commit publishes those changes as an immutable new state.

Two experimental phases (5 and 6) explored this; Phase 7 is the real
implementation and deliberately restarts from the last accepted source. The
owner has split it into two clusters built in parallel:

- **Cluster 1 — storage (someone else's):** `layerfs-content` (C1),
  `layerfs-storage` (C2), `layerfs-history` (C5), and two engines,
  `layerfs-s3` (MinIO) and `layerfs-metadata` (PostgreSQL). No sandbox.
- **Cluster 2 — sandbox (yours):** `layerfs-overlay` (new), `layerfs-workspace`
  (complete rewrite), `layerfs-fuse`, `layerfs-daemon`, `layerfs-bridge`,
  `layerfs-sandbox`, `layerfs-api`.

Your cluster is independent of cluster 1 for mounting a Workspace and running
operations through FUSE. It depends on cluster 1 only in its last step, Commit,
which goes through `layerfs-storage`, `layerfs-s3`, `layerfs-metadata` and
`layerfs-history`. Reading unchanged files from a non-empty committed base is
part of that same integration step.

## Where to read

- **Your worktree:** `/Users/yifanxu/.codex/worktrees/phase7-cluster2-sandbox/layerfs`, branch `codex/phase7-cluster2-sandbox`, created at the Phase 4.5 +
  Phase B commit `7edddbdb8e8512627aed0ed42533ef099d802384`. Work only there.
  Every claim you make about existing code must be checked against this commit
  and cite a file path. The `core/AGENTS.md` inside the worktree is the older
  version; the amended one named below governs.
- **Design packet:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/301/`
  (`README.md`, `01`–`06`). It is not committed yet, so read it from that
  absolute path. Start with `README.md`, then `01-architecture.md`,
  `02-workspace-overlay.md`, `03-commit-workflow.md` and `06-tables.md`.
- **Rules:** root `AGENTS.md`, `core/AGENTS.md`,
  `core/benchmark/fs-bench-pro/AGENTS.md`, `docs/general/benchmark_rules.md`.
  The amended `core/AGENTS.md` is also uncommitted in the primary checkout at
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/AGENTS.md`; use that version.
- **Baseline evidence:** `core/docs/issues/286/` at the base commit, starting
  with `SEVEN-FAMILY-CHECKPOINT-20260930.md` and the per-family checkpoints it
  links, plus the deferred ledger in issue #276.
- **Background issues:** #298 (overlay idea), #299 (daemon-local execution and
  SQL work counts), #245, #248, #249, #256, #261 (limits this design is meant to
  remove).

## Simplification is a goal, not a side effect

Phase 7 changes what the product stands on: **PostgreSQL and MinIO as global
storage, and embedded SQLite inside the daemon for live Workspace state.** The
owner expects this to remove a great deal of machinery, and the plan should go
looking for it. Most of today's Workspace code exists to do, by hand, what SQLite
does natively, or to move bytes to a host that no longer needs to see them.

Treat every existing mechanism as something to justify, not something to port.
For each one, ask what problem it solved and whether that problem still exists.
Candidates to check against the source — these are leads, not conclusions:

- A hand-built storage engine for live state: authenticated pages, the B+
  ownership tree, the metadata arena, pieces, segments, cursors and reclamation
  under `layerfs-workspace/src/backing/`. SQLite already provides a B-tree, a
  pager, indexes and transactions.
- Resource accounting that exists because of that engine: page funds, completion
  reserves, refunds and ownership records.
- Custody of payload kept outside the database: source lifetimes, reference
  counts, pins and retirement queues. With payload and metadata in one
  transaction there is nothing to keep in step.
- Fixed capacities that came from fixed-size structures: limits on dirty
  identities, changed names, edit runs and directory population.
- Sending content to the host: prepared streams, payload and metadata contracts
  in `layerfs-bridge`, staged upload in `layerfs-workspace/src/commit/`, and base
  reads through bridge Inspect calls. Construction is local to the daemon now.
- Versioning schemes more general than needed: anything that keeps more than a
  frozen and an active state per key.

Prefer deleting to adapting, and one obvious mechanism to two clever ones. The
limits are fixed: POSIX behaviour the product already supports, exact capture,
no paused commands, bounded memory windows, the trust boundary and the
repository's measurement and source rules stay. A simplification is real only if
the product code is gone — moving code to another crate is relocation and must
be reported as such.

## Decided — build on these, do not reopen them

- The overlay is embedded SQLite owned by the daemon, one database per daemon,
  shared tables keyed by Workspace. It holds metadata **and** payload bytes;
  there are no private backing files.
- Rows are keyed `(workspace, object, generation)`. A mutation writes at the
  active generation and never touches a lower one. Capture is one statement that
  bumps the generation; install is one statement; retirement deletes folded rows
  in bounded batches. Within a generation a rewrite replaces the row, so only the
  latest payload is stored.
- Commands are never paused for a Commit. No transaction spans construction,
  encoding, a network request or command execution.
- `layerfs-overlay` knows SQLite and nothing about C1, C2, C5, MinIO or FUSE.
  `layerfs-workspace` knows no SQL. `layerfs-fuse` knows no storage. Only
  `layerfs-daemon` wires concrete engines. `layerfs-bridge` carries control
  messages only.
- The overlay is built together with the Workspace operations that use it, one
  operation at a time, not as a standalone component first.
- Mount and Exec are brought up on an empty committed base before any
  integration.
- One pending Commit per Workspace. One construction worker.
- Payload stays out of the kernel writeback cache: the mount keeps direct I/O as
  at the base, so an accepted write has reached the daemon.

## Open — your plan must resolve each, or put a precise question to the owner

The packet's README lists these with a recommendation. Decide where the source
lets you, and say what evidence decided it.

- **D3** — payload unit: fixed block grid or extents, and the block size. Check
  it against C1's edit input (`layerfs-content/src/file/`) so that block runs
  feed the existing edit and stream constructors without widening what C1 must
  re-chunk.
- **D4** — the overlay's SQLite settings (`02-workspace-overlay.md` §10). Confirm
  each is achievable with the pinned `rusqlite` and a bundled build for the
  static daemon image.
- **D7** — resolving an uncertain publication by exact identity lookup. This
  needs a rule change in `core/AGENTS.md`; state exactly what you need.
- **D9** — what happens to folded rows after install.
- The integration contract: precisely what you need from cluster 1 at Commit and
  for base reads (operations, inputs, outputs, error classes), written so the
  cluster 1 owner can confirm it early.
- What survives from the existing `layerfs-workspace/src/filesystem/` semantics
  and what is rewritten.
- Whether C1's deferred-edit allowance (`EDIT_DEFERRED_LIMIT`,
  `layerfs-content/src/file/edit/tree.rs`) limits any target workload.

## What the plan must contain

Write these four sections, in this order.

**0. Architecture.** The components of this cluster, the single responsibility
of each, the calls between them, the dependency edges, and the write, read and
Commit paths end to end, including the capture, install and retire transitions
and every failure outcome. State what each component must not know. Show where
it differs from the packet and why.

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
whether it is kept, changed, moved or deleted — the current
`layerfs-workspace` is about 27,000 production lines and most of it goes.

**2. Rollout plan.** An ordered list of steps where each step is the smallest
complete slice: what changes, what it depends on, the commands that verify it,
and its exit condition. Mark clearly the point up to which you need nothing from
cluster 1, and the steps that wait for it. Cover how the old Workspace, daemon
paths and host service coexist with the new ones and when each is removed, which
shared files both clusters will touch, and the per-commit production LOC
comparison the repository requires.

**3. Final verification and benchmark.** The owner's acceptance for this cluster
is **after integration with cluster 1: complete the seven `fs-bench-pro`
families** — `init_namespace`, `history_retention`, `workspace_write`,
`workspace_commit`, `workspace_namespace`, `workspace_mutations`,
`workspace_shell_package`. For each family, state its Phase B scope and
disposition from the checkpoint, what "complete" means here, the command, the
cache contract and the budget. Say which families cluster 1 already covers and
how their receipts are reused by identity rather than rerun. Also specify the
correctness verification that precedes any timing, including the deterministic
overlap witness for a Commit running while commands write.

## What makes this hard

- **Phase B is a functional baseline, not a numeric one.** The checkpoint records
  all seven families as PASS at a stated scope with numeric latency
  `INELIGIBLE`, and several items as owner-deferred or FAIL (#276). Do not
  relabel any of it. Say exactly what completing a family proves and what stays
  open.
- **Payload in SQLite has real costs.** In WAL mode bytes are written twice; the
  database file's pages land in the OS page cache, and root `AGENTS.md` §1
  forbids excusing file-size-proportional page-cache growth; one writer
  serializes every Workspace in the daemon. Plan the count-driven diagnostics
  (statements and transactions per operation, pages written per block, cgroup
  file cache) that decide these before any speed claim.
- **Copy-up can reach the base.** A partial write to a block held only in the
  committed base must read that block first. Before integration there is no
  base to read; design the seam so the empty-base bring-up is the real product
  path and not a test-only branch.
- **Capture must be exact under concurrent writers.** The design relies on one
  writing transaction at a time. Show how FUSE callbacks, handle state and the
  capture statement are ordered so no accepted write is split across generations.
- **Phase 6 measured about 40 SQL statements per write.** The target here is a
  small constant. Show the statement list for each hot operation.
- **The sandbox boundary.** Commands must not be able to read the overlay
  database or any credential. Carry the existing supervisor/command identity
  separation and say how it is re-proved.

## Constraints on how you work

- Plan only. Do not write or change product code.
- Do not invent numbers, sizes or estimates. Quote measured figures with their
  source file; mark anything you could not establish as unknown.
- Do not run benchmarks or repeat measurements to obtain numbers for the plan.
- Do not touch cluster 1 crates or plan work inside them. State what you need
  from them as an interface requirement.
- If the source contradicts the packet, the source wins: report the
  contradiction and adjust.

## What to hand back

One document, `core/docs/issues/303/IMPLEMENTATION-PLAN.md`, in your worktree,
with the repository's status banner and the four sections above. End it with:

- the decisions you made, each with the evidence that decided it;
- the questions only the owner can answer, each phrased so it can be answered in
  one line;
- the integration contract for cluster 1, on its own so it can be sent as is;
- a short list separating what you verified in source from what you inferred.

Keep it as short as completeness allows. A reader should be able to start
implementing step 1 from the document alone.

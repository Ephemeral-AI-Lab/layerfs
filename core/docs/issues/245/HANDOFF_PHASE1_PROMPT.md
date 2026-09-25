# #245 implementation and Phase 1 verification handoff prompt

> **Status:** Current planning checklist; no release candidate exists.

Copy the assignment below into a new Codex task. It is a proposed handoff, not
authorization to relabel old receipts or claim performance before qualifying a
new ordinary-shell measurement. The researched #243 **product source** is
`7f07b5dadd9bee33f431a243345ce40ed214bb1a`; the planning branch is
`codex/issue245-range-cow-plan` (draft
[PR #246](https://github.com/Ephemeral-AI-Lab/layerfs/pull/246)), stacked on
the #243 baseline branch (draft PR #244). Start from the planning branch's
current HEAD and record its exact SHA. Use a separate clean worktree and
implementation branch so the planning PR and historical evidence remain
reviewable.

## Assignment to the implementing agent

Implement and verify [issue #245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245)
against its focused Phase 1 ordinary-shell gate. Carry the task through
root-cause diagnosis, smallest shared-path fixes, scalable range-based COW
file indexing, coordinated bounded Commit streaming, and public behavior proof.
Keep the later many-package load cases as extension requirements, not
retroactive Phase 1 PASS claims. Do not stop after a unit prototype or a fast
but cache-ineligible raw timer. Report any unmet gate plainly.

### 1. Read these files before editing

Read in this order and resolve conflicts by the higher-level repository
rules. Do not rely on this prompt as a substitute for the documents.

1. `AGENTS.md`, `core/AGENTS.md`,
   `docs/general/benchmark_rules.md`,
   `docs/general/documentation-policy.md`,
   `core/benchmark/fs-bench-pro/AGENTS.md`, and
   `benchmark/fs-bench-pro/QUICKSTART.md` before changing benchmark or
   product files. Read `docs/general/release-policy.md` before any
   release-admission statement.
2. `core/docs/issues/245/README.md`,
   `ARCHITECTURE.md`, `IMPLEMENTATION_PLAN.md`,
   `VERIFICATION_PHASE1.md`, and `LOAD_BEARING_CASES.md`.
   The last file is deferred scope, not an instruction to run every proposed
   stress case in Phase 1.
3. `core/docs/issues/243/PHASE1_CONTRACT.md`,
   `TEST_WORKSPACE_AND_COMMANDS.md`, and
   `evidence/phase2-ordinary-shell-v1/REPORT.md`;
   `core/docs/benchmark/fs-bench-pro/exec2edit.md`;
   `core/docs/issues/232/SHELL_ROUTE_CORRECTION.md`,
   `evidence/posix-count-diagnostic/REPORT.md`, and
   `exec-fuse-edit-v2-baseline.md`. Preserve raw receipts and the historical
   ioctl campaign unchanged. The eleven large v2 failures **observed** a
   five-second silent Exec `Unknown`; the 8 MiB over-limit is a source/registry
   deduction, not an observed Capacity result.
4. Trace callers and invariants in
   `core/crates/layerfs-fuse/src/adapter.rs`. Under
   `core/crates/layerfs-workspace/src/`, read
   `filesystem/write.rs`, `read.rs`, `resize.rs`, `rename.rs`,
   `create.rs`, `remove.rs`; `overlay/pieces.rs`, `snapshot.rs`;
   `backing/metadata_index.rs`, `metadata_pages.rs`,
   `metadata_reclaim.rs`, `ownership.rs`, `payload.rs`; and
   `commit/lower.rs`, `save.rs`, `reconcile.rs`. Also read
   `core/crates/layerfs-bridge/src/contract/request.rs`,
   `core/crates/layerfs-server/src/service/save/content.rs`, and
   `core/crates/layerfs-content/src/file/edit/input.rs`.
   Inspect `core/benchmark/fs-bench-pro/registry/workspace-shell-package-v1.json`,
   `shell_package.py`,
   `core/crates/layerfs-api/sdk/examples/benchmark_shell.rs`, and
   `core/crates/layerfs-server/examples/verify_shell.rs` before planning
   a new sample.

The public SDK method is `WorkspaceApi::exec(command)`; “Workspace shell”
names the product workflow, not another method. It launches `/bin/sh -c`
inside the mounted Workspace. No LayerFS edit tool, shell-text classifier,
range ioctl, or direct Store mutation may stand in for ordinary POSIX/FUSE
writes.

### 2. Architecture and algorithm: before versus after

```text
CURRENT
  any shell command → POSIX → FUSE WRITE(offset, bytes)
    → own payload → load all P Base/Local/Zero pieces into a Vec
    → splice/resummarize all P → rebuild every fixed two-level piece page
    → publish private root                         O(P) per callback

  Commit → collect all pieces/edits
    → one EditFile request (≤256 changed intervals, ≤8 MiB existing-file
      replacement; ≤1,024 pieces in the private inode)
    → server eagerly buffers replacement → content construction
    → one Branch-head publication

TARGET
  same arbitrary command → same POSIX/FUSE callback
    → seek offset in a dynamic-height, length-indexed B+ tree-style sequence
    → split/replace affected K extents; COW affected leaf/ancestor paths
    → share untouched pages/payloads → publish private root

  Commit A → pin immutable private G1 root at a short ordering boundary
    → G2 sees G1 plus later shell writes on the same FUSE mount
    → cursor/stream G1's changed file and namespace spans with bounded memory
    → construct canonical root → publish one head B1
    → reconcile G2 onto B1 without losing later writes
  Commit B → pin G2; G3 may receive later writes → publish B2
```

Retain `Base` (canonical content), `Local` (private payload) and `Zero`
semantics. The tree is a **multiway paged sequence**, not an in-memory binary
node per piece or a conventional absolute-offset index. Leaves pack extents;
internal entries summarize subtree byte lengths so a structural prepend does
not rekey every suffix extent. Reuse the existing 4 KiB page arena and custody
model when correct. Validate any new persisted page format and ownership
edges; old private pages must not be silently decoded under the new format.
An ordinary shell insert still pays for every suffix byte its command sends.

### 3. Expected file ownership, layout, and LOC accounting

This is a provisional responsibility map, not scaffolding to create blindly.
Trace existing helpers first and keep the shortest coherent diff.

```text
core/crates/layerfs-workspace/src/
  backing/metadata_index.rs, metadata_pages.rs, ownership.rs,
          metadata_reclaim.rs
  backing/piece_tree/              proposed focused module if needed:
          mod.rs                    declarations only, <200 physical lines
          page.rs                   versioned length-indexed page format
          update.rs                 seek/split/join/COW path update
          cursor.rs                 bounded read and Commit iteration
  overlay/pieces.rs, snapshot.rs
  filesystem/write.rs, read.rs, resize.rs, rename.rs
  commit/lower.rs, save.rs, reconcile.rs

core/crates/layerfs-bridge/src/contract/request.rs
  contract/file_stream.rs          only if the stream ABI needs a focused file
core/crates/layerfs-server/src/service/save/content.rs
core/crates/layerfs-content/src/file/edit/input.rs

core/crates/*/tests/              external focused behavior tests
core/benchmark/fs-bench-pro/      new frozen ordinary-shell selection/runner
core/docs/architecture/          affected algorithm/format/bound documents
core/docs/issues/245/evidence/   fresh append-only diagnostic and run reports
```

The existing `write.rs` (870), `rename.rs` (926), and Bridge `request.rs`
(917) are close to the 999-physical-line production-file ceiling; split by
responsibility rather than appending a large implementation. `lib.rs` and
`mod.rs` remain declaration/delegation-only and below 200 physical lines.
Do not add dependencies when the existing page/index machinery suffices,
parallel file editors, or a test-only product branch.

At this planning identity, first-party production LOC is **120,780 combined**
(Core **55,363**, reference **65,417**) by
`python3 tools/production_loc.py --json`. A **rough, nonbinding**
implementation allowance is **+800 to +2,000 net Core production LOC** across
the file index, transport, reconciliation and shared-path fixes, offset by
removing flat-list code; reference production LOC should stay unchanged.
This is an estimate, not a budget or pass/fail gate. Record exact before,
after and signed delta for **every commit**, with scope and counting method,
from its first parent and final staged tree. Do not compress code, omit checks,
or move implementation to tools to meet the estimate.

### 4. Time, space, ownership, and failure complexity

Use `P` for final piece count, `K` for pieces touched by one callback,
`F` for page fanout, `D` for bytes genuinely supplied or shifted by the
command, and `H ≈ log_F P` for tree height.

| Path | Current cost | Target cost / bound |
| --- | --- | --- |
| Local WRITE index work | `O(P)` piece visits and full page rebuild per callback; repeated growing writes can be quadratic in callback count | `O(H + K)` piece/index work plus accepted-byte I/O; copy affected paths and rebalance locally |
| One read at offset | bounded lookup followed by requested bytes | `O(H + touched leaves)`; do not collect the file's pieces |
| Commit capture | metadata root pin | keep a short root/generation boundary, with no full-file or full-tree clone |
| File Commit | `O(P)` pieces and edits materialized; replacement eagerly buffered downstream | one ordered `O(P + D)` traversal/transfer with bounded cursor/frame memory, retaining canonical Base ranges |
| Retained backing | fixed count ceilings and page/payload reservations | charge physical Local bytes, metadata pages and generations to real disk/memory budgets; reclaim only after all roots/handles/unknown outcomes release custody |

The logical file ceiling remains 4 GiB unless a separate contract changes
it. The sandbox's current 1 GiB Workspace disk, 16 MiB Workspace memory,
and 512 MiB container memory budgets are real limits; account for cgroup
page cache and retained generations, not just process heap. The target
removes arbitrary per-file 1,024-piece/256-interval/8 MiB replay ceilings
only when **Workspace, Bridge, server and C1** all stream correctly. The
current server `Vec<Vec<u8>>` and C1's further edit cap must be addressed,
not hidden by a larger front-end quota.
Audit the current `PageRef` slot/epoch bound, candidate page reservations
and root-owner limit as the tree grows; a dynamic tree cannot silently move
the capacity wall into page allocation. Larger package installs also face
the separate 128-dirty-identity/128-changed-name and 32 KiB namespace
request bounds. Record them as later scale work rather than claiming Phase 1
proves that load.

The concurrent-Commit contract remains **unproven**: current
`reconcile_commit` can return `Busy` if the active revision/root changes
after successful Store publication. Preserve the frozen root, exact
before/after-cut visibility, old heads, definite/unknown outcome rules, and
reclaim safety. One Commit may be in flight; a second is sequential after the
first resolves. “Non-pausing” permits brief capture/reconcile ordering and
individual I/O waits, not a global lock for Store construction.

### 5. Command-agnostic FUSE routing

The FUSE adapter routes by syscall, **never by command name or text**:

```text
dd / cp / editor / package script / any process
  → Linux VFS
  → FUSE WRITE(offset, bytes) → one optimized Workspace write path
  → FUSE SETATTR(size)       → one resize path
  → FUSE CREATE/RENAME/UNLINK → namespace overlay paths
  → reads see each accepted mutation before Commit
```

Do not label explicit ioctl or cooperating-tool results as generic shell
results. Record actual FUSE callbacks and bytes; Status `write` aggregates
namespace mutations and cannot serve as an exact FUSE WRITE count. A command
that copies a suffix incurs that read/write traffic; the optimization removes
extra per-callback index work, not the bytes requested by the command.

### 6. Phase 1 implementation and verification target

Work in root-cause order:

1. From the retained #243 failed receipt, instrument a **labelled functional
   diagnostic** of root `package-lock.json.next` replacement. Capture the
   `WorkspaceError` variant/stage before FUSE maps it to `EIO`; compare
   the fresh-temp-over-untouched-canonical case at root and nested paths.
   Fix the shared rename/namespace cause and preserve old heads/open handles.
2. Keep #243's exact mixed refresh, 4 KiB overwrite, sixteen one-byte writes,
   and failed-command/no-Commit commands as correctness oracles. Diagnose the
   five-second silent Exec `Unknown` and separate ~5.5 s noncommitting
   container-stop/cleanup term by counts and event timing. Do not inflate the
   progress deadline or reclassify the eleven historical failures as observed
   Capacity.
3. On a clean, pinned **pre-index** source after the shared correctness fixes,
   freeze the new ordinary-shell case registry, exact commands, fixtures,
   cache contract, resource limits, receipt schema and full oracles. Include
   one 10 MiB middle shift for count growth and one formerly failing-size
   structural shift. Use ordinary POSIX shell utilities, never the historical
   LayerFS edit tool or ioctl. Collect one qualifying control attempt per
   selected case before optimization; if cold cache cannot be enforced,
   retain it as INELIGIBLE and make no speedup claim. The historical #243
   and #232 numbers are not numerical controls. Freeze any comparative
   latency target after the qualifying control and **before** candidate code
   or sampling.
4. Implement the local index and coordinated streaming Commit. Prove a fixed
   4 KiB write touches affected pieces and `O(H)` paths rather than
   collecting/rebuilding all `P` pieces. The retained 1/10 MiB count
   diagnostic (9/81 FUSE WRITEs, 51/3,363 old-piece loads, 10/144 rebuilt
   pages) is mechanism evidence, **not** a qualified speed arm. At the frozen
   changed-source identity, take one candidate attempt per case under the
   same enforced cache contract as its matched control. A changed command,
   fixture, harness or cache contract creates a new scenario, not a pair.
5. Prove G1 → G2 → G3 across two sequential Commits with ordinary mounted
   writes continuing during each Store construction. B1 must contain only
   pre-capture bytes; live G2 must include later writes; after reconciliation,
   B2 and live G3 must be exact. A Store success followed by an unexplained
   client `Busy` is not a PASS.
6. Run the independent full-tree Store/History verifier: exact paths, types,
   modes, lengths and hashes; old Commit roots; no publication after
   deliberate failed Exec; successful cleanup and bounded resources. Freeze a
   complete receipt schema with route, source/build/image/fixture/harness
   identities, field provenance, failure class, cache and campaign
   completeness. Missing counters stay UNAVAILABLE.

Observe the per-worktree measurement lock. One performance sample per case and
arm, no best-of or unchanged-arm resampling, one construction worker, sealed
builds, independent writable prepared-master copies, fresh append-only output
paths, and honest cold-cache qualification. The complete command is ≤15 s
normally; the existing mixed-refresh case has a declared ≤25 s exception.
Independent verification is under 10 s (the #243 selection froze 9 s).
No warm-cache credit, timeout/worker increase, shortened case, missing failed
cell, or historical receipt rewrite. If the FUSE backing cache remains
uncontrolled, report latency INELIGIBLE even when correctness passes.

At frozen final source, run the focused tests covering changed modules and
the owning Core checks once: `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --all-targets --locked -- -D warnings`,
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`,
`python3 core/tools/check_product_boundary.py`, and
`python3 -m unittest discover -s core/tools -p 'test_*.py'`. Diagnose a
failure from its output and source, apply the fix, then run its covering
commands once. Do not run the retired root preflight or claim CI green. Update the
affected `core/docs/architecture/` source pin, report every PASS/FAIL/
INELIGIBLE/NOT_RUN case, exact LOC delta per commit, reproduction command,
residual risk, and draft PR(s). #232's full 56 ordinary-shell shapes and the
deferred [load-bearing cases](LOAD_BEARING_CASES.md) remain open until
separately registered and proven.

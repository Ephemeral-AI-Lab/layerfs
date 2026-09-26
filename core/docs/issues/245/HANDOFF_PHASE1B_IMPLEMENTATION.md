# #245 Phase 1B handoff: implement the range COW index and the streaming Commit

> **Status:** Current handoff; the correctness round it follows is complete and
> merged into this branch, the index/transport/concurrency work is not started.

Copy the assignment below into a new Codex task. This is a **delta** handoff:
Phase 1A (root-cause repair and mounted diagnostics) is finished, committed and
pushed, and its receipts are retained. Do not repeat it. Read this document with
the [architecture](ARCHITECTURE.md), [implementation plan](IMPLEMENTATION_PLAN.md),
[Phase 1 verification](VERIFICATION_PHASE1.md) and
[Phase 1A report](evidence/phase1-ordinary-shell-repair/REPORT.md).

## Where the branch stands

| Item | Value |
| --- | --- |
| Branch | `codex/issue245-range-cow-plan`, draft [PR #247](https://github.com/Ephemeral-AI-Lab/layerfs/pull/247) |
| Phase 1A commits | `a0258cd6f` (+13 Core LOC) correctness fixes, `dd00c92e2` (+55) FUSE diagnostics, `167d01986` (0) evidence/docs |
| Planning identity | `d63379b95` — Core production LOC 55,363; combined 120,780 |
| Phase 1A final | Core 55,431 (+68); reference 65,417 unchanged |
| Done | three namespace/bind defects fixed; the four #243 correctness oracles and the two rename shapes re-verified on the mounted route with the sealed verifier |
| Not done | length-indexed piece pages, COW path update, cursor reads, streaming Commit, G1→G2→G3, frozen Phase 1 selection and its control attempt, the five-second `Exec Unknown` diagnosis |
| Every Phase 1A latency row | `INELIGIBLE` — host Store copy and container FUSE backing cache uncontrolled |

## Assignment to the implementing agent

Implement the architecture the #245 documents describe — a length-indexed,
dynamic-height, multiway paged extent sequence with copy-on-write path updates,
bounded cursors, and a coordinated streaming Commit — and then perform the
focused Phase 1 verification against the ordinary `WorkspaceApi::exec(command)`
route. Keep the deferred [load-bearing cases](LOAD_BEARING_CASES.md) as
extensions, not Phase 1 claims. Do not stop at a prototype, a counter-only
demonstration, or an ineligible timer. Report every unmet gate plainly.

### 1. Read before editing

Repository rules first, then the issue documents, then the code:

1. `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
   `docs/general/documentation-policy.md`, `core/benchmark/fs-bench-pro/AGENTS.md`,
   `benchmark/fs-bench-pro/QUICKSTART.md`; `docs/general/release-policy.md`
   before any release statement.
2. `core/docs/issues/245/`: this file, `ARCHITECTURE.md`,
   `IMPLEMENTATION_PLAN.md`, `VERIFICATION_PHASE1.md`,
   `evidence/phase1-ordinary-shell-repair/{REPORT.md,DIAGNOSTIC_CONTRACT.md}`,
   then `../243/PHASE1_CONTRACT.md`, `../243/TEST_WORKSPACE_AND_COMMANDS.md`,
   `../232/SHELL_ROUTE_CORRECTION.md`.
3. Source, in this order: `core/crates/layerfs-workspace/src/overlay/pieces.rs`,
   `backing/metadata_index.rs`, `backing/metadata_pages.rs`,
   `backing/metadata_build.rs`, `backing/ownership.rs`, `backing/payload.rs`,
   `backing/metadata_reclaim.rs`, `filesystem/write.rs`, `read.rs`, `resize.rs`,
   `commit/lower.rs`, `commit/source.rs`, `commit/save.rs`,
   `commit/reconcile.rs`, `overlay/snapshot.rs`; then
   `core/crates/layerfs-bridge/src/contract/request.rs`,
   `core/crates/layerfs-server/src/service/save/content.rs`,
   `core/crates/layerfs-content/src/file/edit/{input,tree,apply}.rs`.

### 2. What Phase 1A already established — do not redo it

- The `EIO` on `mv -f package-lock.json.next package-lock.json` was
  `WorkspaceError::Io`, not a capacity refusal, and it tracked **name length**,
  not path depth. Three defects were fixed: the fixed 17-byte key ceiling in
  `backing/metadata_build.rs`, `overlay/directories.rs::keep_name`'s cursor
  re-reading the cell it cleared, and `filesystem/create.rs` binding a name
  without clearing the removal record a kernel `UNLINK`+`CREATE` overwrite had
  written. Regression evidence is in the Phase 1A report; keep those rules.
- `LAYERFS_FUSE_TRACE` and `LAYERFS_FUSE_ERROR_DIAGNOSTIC` exist
  (`layerfs-fuse/src/trace.rs`, `replies.rs`). Use them; do not add a second
  trace mechanism. The error trace is how an `EIO` becomes a variant.
- The mounted functional route works: mixed refresh, 4 KiB overwrite, sixteen
  one-byte writes, deliberate failure, root lockfile replacement and nested
  long-name rename all pass with the sealed full-tree verifier.

### 3. What is still missing, at the exact code sites

| Gap | Where it is today |
| --- | --- |
| Absolute-offset piece keys; no subtree lengths | `overlay/pieces.rs` `Piece { start, … }`; `metadata_index.rs::build_pieces` writes leaves keyed by the piece's absolute start |
| Whole-list load per callback | `filesystem/write.rs` calls `arena.pieces(...)`, which collects all `P` pieces |
| Whole-list splice | `overlay/pieces.rs::splice` rebuilds the vector, caps 1,024 pieces / 256 edits / `MAX_REPLAY` 8 MiB |
| Whole-tree page rewrite | `filesystem/write.rs` calls `build_pieces`, which writes every leaf plus a branch |
| No cursor | reads use `Arena::piece_at` (bounded, unchanged) but Commit and lowering collect vectors |
| Commit collects everything | `commit/lower.rs::lower_file` re-collects all pieces and emits ≤ 256 `Edit` rows |
| Eager transport | `layerfs-bridge/src/contract/request.rs` 256-edit / 8 MiB validation; `layerfs-server/src/service/save/content.rs` materializes replacement buffers; C1 caps edits again |
| Successor gap | `commit/reconcile.rs` can return `Busy` after a successful Store publication |

### 4. Work packages, in order

**A. Page format (versioned length-indexed sequence).** Leaves pack extents
(`Base` | `Local` | `Zero`); internal pages hold child refs plus **subtree
logical byte lengths**, so a structural prepend moves the suffix logically
without rekeying it. Version the format explicitly and refuse to decode an old
absolute-offset page as a length-indexed page; retained private roots must
either be migrated or refused, never silently reinterpreted. Keep `MAX_FILE`
(4 GiB), height, fanout and arithmetic checked. Do not add dependencies.

**B. COW path update.** Seek by logical offset, split/replace only the affected
leaves, copy the affected ancestor path, rebalance/merge adjacent compatible
extents locally, and publish the candidate under the existing mutation ordering
(compare expected revision/root, refuse partial candidates). Target
`O(H + K)` index work per callback plus the accepted bytes; a large contiguous
replacement legitimately visits many extents.

**C. Cursor.** Ordered, bounded iteration for reads and for one Commit walk
without materializing a file's whole piece vector. Reads stay `O(H + touched
leaves)`.

**D. Transport coordinator.** Emit ordered base-retain and replacement spans
through bounded frames: Workspace lowering → Bridge request/frame → server save
→ C1 construction. Remove the 1,024-piece, 256-interval and 8 MiB replay
ceilings **only** together with that coherent path — never by moving the same
ceiling into the server, and never by raising one limit while the next layer
still buffers. Preserve one Branch-head publication per Commit, canonical `Base`
reuse, and definite/unknown outcome handling.

**E. Generation capture and reconciliation.** Keep `G1` immutable while later
`G2` writes continue on the same mount; after a successful publication reconcile
`G2` onto `B1` without losing, duplicating or reordering later writes, and
without an unexplained client `Busy`. Then prove `G3` with a second sequential
Commit. Preserve frozen-root custody: reclaim only after all roots, handles and
uncertain outcomes release it.

**F. Freeze and verify.** Freeze the ordinary-shell selection (registry, exact
commands, fixtures, limits, receipt schema, oracle) on the **pre-optimization**
source, collect one qualifying control attempt per case, then freeze any
comparative target, then take one candidate attempt per case on the
post-optimization source under the same enforced cache contract. Changed
command, fixture, harness or cache contract ⇒ new scenario, not a pair.

Each package's gate is its stop condition: A needs versioned read plus a refused
old-format decode; B needs focused write/resize/read tests for overlap, holes,
append, truncate, piece boundaries, old generations and rollback; C needs a
Commit walk that never materializes a whole file; D needs exact final bytes with
no unbounded `Vec<Vec<u8>>`; E needs the mounted three-generation observation; F
needs every registered cell reported.

### 5. Complexity contract

| Path | Today | Target |
| --- | --- | --- |
| Local WRITE index work | `O(P)` piece visits + every page rewritten per callback | `O(H + K)` index work + accepted bytes, path-copied |
| One read at offset | bounded lookup, then requested bytes | unchanged: `O(H + touched leaves)` |
| Commit capture | metadata root pin | unchanged: short root/generation boundary, no tree clone |
| File Commit | `O(P)` pieces materialized, replacement buffered eagerly | one ordered `O(P + D)` traversal with bounded cursor/frame memory, canonical `Base` retained |
| Retained backing | fixed count ceilings and reservations | physical Local bytes, pages and generations charged to real budgets; reclaim only after all custody releases |

`P` = final pieces, `K` = pieces touched by one callback, `F` = fanout,
`D` = bytes genuinely supplied or shifted, `H ≈ log_F P`. The logical file
ceiling stays 4 GiB unless a separate contract changes it. An ordinary shell
insert still pays for every suffix byte its command sends.

### 6. Operational playbook (learned in Phase 1A — use it)

The fastest real-behaviour loop is a **standalone mounted diagnostic**, ~1
minute per attempt, instead of a full `shell_package.py prepare`:

1. `cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked
   --offline --target aarch64-unknown-linux-musl -p layerfs-daemon` — ~6 s
   incremental, ~20 s cold, from a warm target directory.
2. Copy the daemon over the retained `...-prepared-02/image-context/layerfs-daemon`
   and `docker build -q` a new tag; keep `ENV LAYERFS_FUSE_TRACE=1` and
   `ENV LAYERFS_FUSE_ERROR_DIAGNOSTIC=1` in diagnostic images only.
3. `python3 core/docs/issues/245/evidence/phase1-ordinary-shell-repair/run_diagnostic.py`
   with `--prepared-root <worktree holding the retained #243 state>` to run one
   exact command on a fresh clone of a sealed master and verify it.
4. For a symbolized backtrace, build the **dev** profile for musl
   (`cargo +1.85.1 zigbuild --target aarch64-unknown-linux-musl -p
   layerfs-daemon`, 35 MB with debuginfo, ~17 s warm); the release binary is
   stripped and prints `<unknown>` frames. Remove any backtrace printing before
   committing.

Other traps worth knowing:

- macOS `cargo test --all-targets` compiles the Linux-gated
  `layerfs-workspace/tests/*` suites **out**; mounted and native-service
  behaviour must be verified through the container or the route harness.
- `cargo fmt` expands long calls and can push a file past the 999-line ceiling;
  always run `python3 core/tools/check_product_boundary.py` after formatting.
- Exact per-commit LOC snapshots: `git archive <parent> | tar -x -C <dir>` for
  before, `git checkout-index -a --prefix=<dir>/` for the staged tree, then
  `python3 tools/production_loc.py --root <dir> --json`.
- Remove stray `docker run` containers when a job is killed; they hold ports and
  disk.
- The retained prepared state lives in a separate worktree and may be pruned;
  if it is gone, re-prepare with `python3 core/benchmark/fs-bench-pro/shell_package.py
  prepare --output <fresh path>` (it refuses a dirty source).

### 7. Discipline that is not negotiable

- Route: `/bin/sh -c` in the mounted Workspace; the kernel's syscalls decide the
  callbacks. No edit tool, no range ioctl, no command-text classifier, no direct
  Store mutation, no cooperating-tool result relabelled as a shell result.
- Cache: a timed phase must pay for its own work from a declared state. If the
  FUSE backing cache cannot be enforced and checked equally for both arms,
  report latency `INELIGIBLE` even when correctness passes. No priming, no
  arm-specific invalidation, no warm-cache credit.
- Sampling: one attempt per case per arm, no best-of, no unchanged-arm rerun, no
  timeout or worker increase, no shortened case, no dropped failing cell, no
  historical receipt rewritten. One construction worker; `LAYERFS_CONSTRUCTION_WORKERS=1`.
- Budgets: complete command ≤ 15 s (declared exceptions ≤ 25 s, the mixed refresh
  already declares 25 s); independent verification < 10 s.
- Evidence: append-only paths, raw stdout/stderr, identities (source, build,
  image, fixture, harness), field provenance, and `UNAVAILABLE` for anything not
  measured — never zero by assumption.
- Do not run the retired root preflight and do not claim CI green.

### 8. Final reporting

At frozen final source run, once: `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 fmt
--manifest-path core/Cargo.toml --all --check`, `python3
core/tools/check_product_boundary.py`, `python3 -m unittest discover -s
core/tools -p 'test_*.py'`. Diagnose a red result from its output and source,
fix once, then re-run its covering commands.

Report: the per-commit production LOC before/after/delta with scope and counting
method; every registered case as PASS, FAIL, INELIGIBLE or NOT_RUN with its
receipt path; the exact commands to reproduce each row; updated
`core/docs/architecture/` pins for the changed format, algorithm or bound;
residual risk and every open gate, including the ones this handoff does not
close (#232's 56 shapes, the deferred load-bearing cases, and the many-package
namespace limits of 128 dirty identities / 128 changed names / 32 KiB).

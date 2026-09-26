# #245 Phase 1B continuation handoff: streaming Commit, generations, frozen proof

> **Status:** current handoff. Packages A–C are implemented, committed and gated
> on this branch; packages D, E and F are not started.

Copy the assignment below into a new task. This is a **delta** handoff. Phase 1A
(root-cause repair) and Phase 1B packages A–C (length-indexed COW index and
bounded cursor) are finished, committed and pushed, and their receipts are
retained. Do not repeat them. Read this with
[ARCHITECTURE.md](ARCHITECTURE.md), [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md),
[VERIFICATION_PHASE1.md](VERIFICATION_PHASE1.md),
[LOAD_BEARING_CASES.md](LOAD_BEARING_CASES.md),
[evidence/phase1-ordinary-shell-repair/REPORT.md](evidence/phase1-ordinary-shell-repair/REPORT.md),
[evidence/phase1b-extent-sequence/REPORT.md](evidence/phase1b-extent-sequence/REPORT.md),
and [the extent-sequence format pin](../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md).

## Where the branch stands

| Item | Value |
| --- | --- |
| Branch | `codex/issue245-range-cow-plan`, draft [PR #247](https://github.com/Ephemeral-AI-Lab/layerfs/pull/247) |
| Head | `1d774ee97` (implementation `466d6da6a`) |
| Phase 1A | `a0258cd6f` (+13) correctness fixes, `dd00c92e2` (+55) FUSE diagnostics, `167d01986` (0) evidence |
| Phase 1B A–C | `466d6da6a` production LOC 55,134 → 56,035 (**+901**); legacy 68,728 unchanged; combined 123,862 → 124,763 |
| Phase 1B evidence | `3c78910b9`, `1d774ee97` (delta 0 each) |
| Done | versioned length-indexed page format; path-copied splice; bounded cursor; 12 focused tests; the format/algorithm/bound pin |
| Not done | coordinated streaming transport (D); G1→G2→G3 continuous Commit (E); frozen ordinary-shell selection and its control (F); the five-second `Exec Unknown` and ~5.5 s cleanup diagnosis; #232's 56 shapes; the deferred load-bearing cases; the 128/128/32 KiB namespace ceilings |

## 1. Read before editing

Repository rules first, then the issue documents, then the code:

1. `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
   `docs/general/documentation-policy.md`, `core/benchmark/fs-bench-pro/AGENTS.md`,
   `benchmark/fs-bench-pro/QUICKSTART.md`; `docs/general/release-policy.md`
   before any release statement.
2. `core/docs/issues/245/`: this file, `ARCHITECTURE.md`,
   `IMPLEMENTATION_PLAN.md`, `VERIFICATION_PHASE1.md`, `LOAD_BEARING_CASES.md`,
   `evidence/phase1b-extent-sequence/{REPORT.md,IDENTITIES.txt}`, then
   `evidence/phase1-ordinary-shell-repair/{REPORT.md,DIAGNOSTIC_CONTRACT.md}`,
   then `../243/PHASE1_CONTRACT.md`, `../243/TEST_WORKSPACE_AND_COMMANDS.md`,
   `../232/SHELL_ROUTE_CORRECTION.md`.
3. Source, in this order for package D:
   `core/crates/layerfs-workspace/src/commit/{lower.rs,source.rs,save.rs}`,
   `core/crates/layerfs-workspace/src/backing/metadata_pieces.rs` (the cursor and
   the replacement producer),
   `core/crates/layerfs-bridge/src/contract/{request.rs,execution.rs}`,
   `core/crates/layerfs-server/src/service/save/content.rs`,
   `core/crates/layerfs-content/src/file/edit/{input,tree,apply,concat}.rs`.
   For package E: `core/crates/layerfs-workspace/src/commit/{reconcile.rs,completion.rs}`,
   `core/crates/layerfs-workspace/src/overlay/snapshot.rs`,
   `core/crates/layerfs-workspace/src/backing/metadata.rs` (`candidate`,
   `reconciliation_root`, `result_roots`).

## 2. What is already established — do not redo it

**Phase 1A.** Three namespace/bind defects were fixed and the four #243
correctness oracles plus two rename shapes were re-verified on the mounted route
with the sealed verifier. Keep those rules: the key-kind bound in
`backing/metadata_build.rs`, the `keep_name` removal-page cursor in
`overlay/directories.rs`, and clearing a removal record when a bind publishes a
name in `filesystem/create.rs`. `LAYERFS_FUSE_TRACE` and
`LAYERFS_FUSE_ERROR_DIAGNOSTIC` exist (`layerfs-fuse/src/{trace.rs,replies.rs}`);
use them and do not add a second trace mechanism.

**Phase 1B A–C.** The file index is now a length-indexed, dynamic-height,
multiway sequence in `backing/metadata_pieces.rs`:

- `PageKind` (`backing/metadata_pages.rs`) declares the body format at byte 49
  and the kind at byte 54. A page that does not declare the format its reader
  asks for is **refused**. There is no migration: a retained absolute-offset
  page is refused.
- A pieces leaf packs 32-byte records (`kind`, 24-bit length, source offset,
  payload identity, custody page); a pieces branch packs 16-byte child refs with
  each child's subtree logical length. Leaf capacity is 124 records, branch
  capacity 248 children, `MAX_EXTENT` is 2^24−1, level ceiling 7. An extent's
  logical start is **derived from its position**, never stored.
- `metadata_pieces::replace(store, old, splice, parts, window)` folds only the
  leaves the replaced interval touches, copies their ancestors, shares every
  other page by reference, splits straddling extents and merges adjacent
  compatible ones. `Parts::declare()` materializes the replacement extents.
- `metadata_pieces::Cursor` (and `Arena::cursor` / `Arena::piece_at`) is the
  bounded reader: one page path plus one leaf, with the operation deadline
  checked between page reads. `PieceStore` is the page-store seam; `RootOwner`
  is the production implementation, and an external test supplies an in-memory
  one.
- `Inode` records no longer carry an extent count. `inode.edits == 0` means the
  selected content is its own base; `inode.edits == u16::MAX` means a splice
  shared an untouched subtree so the exact count is unknown, and lowering derives
  it (and `commit/save.rs` therefore never reuses the base for such a version).

Gate and receipts: `crates/layerfs-workspace/tests/pieces_sequence.rs`
(12 tests) and `evidence/phase1b-extent-sequence/`.

## 3. What is still missing, at the exact code sites

| Gap | Where it is today |
| --- | --- |
| 256-edit ceiling on one request | `layerfs-bridge/src/contract/request.rs`, `Operation::EditFile` arm: `edits.len() > 256` |
| 8 MiB replay ceiling | same arm: `self.operation.input_length()? > MAX_REPLAY` |
| 1,024-piece request bound | `request.rs`, `bytes.div_ceil(1024).saturating_add(257)` |
| Materialized replacement buffers | `layerfs-server/src/service/save/content.rs`: `Replacements::new()` / `replacements.push(part)` |
| C1 caps edits again | `layerfs-content/src/file/edit/` (`input.rs`, `tree.rs`, `apply.rs`) |
| Whole-file lowering decision | `commit/lower.rs::lower_file` streams the cursor but its `FilePlan.edits` is still a bounded `Vec<Edit>` sent as one `EditFile` |
| Successor gap | `commit/reconcile.rs` can return `Busy` (around the root/revision compare) after a successful Store publication |
| Namespace frontier | `runtime/state.rs`: `dirty > 128`, `names > 128`, 32 KiB metadata request |

## 4. Work packages

**D. Transport coordinator.** Emit ordered base-retain and replacement spans
through bounded frames across all four boundaries — Workspace lowering → Bridge
request/frame → server save → C1 construction. Remove the 256-interval, 8 MiB
replay and 1,024-piece ceilings **only** together with that coherent path: never
by moving the same ceiling into the server, and never by raising one limit while
the next layer still buffers. Preserve one Branch-head publication per Commit,
canonical `Base` reuse, and definite/unknown outcome handling. **Gate:** large
replacement and fresh streams complete with exact final bytes and no unbounded
`Vec<Vec<u8>>`; the 4 GiB logical ceiling and real resource budgets stay
explicit. Start by proving the frame path is bounded *before* deleting a ceiling,
and delete a ceiling only in the commit that makes its layer streaming.

**E. Generation capture and reconciliation.** Keep `G1` immutable while later
`G2` writes continue on the same mount; after a successful publication reconcile
`G2` onto `B1` without losing, duplicating or reordering later writes, and
without an unexplained client `Busy`. Then prove `G3` with a second sequential
Commit. Preserve frozen-root custody: reclaim only after all roots, handles and
uncertain outcomes release it. **Gate:** the mounted three-generation
observation — pre-capture bytes in `B1`, post-capture bytes in live `G2`, then
exact `B2` and live `G3`, with both older heads still readable. Highest-value
first step: record the exact ordering point at which reconciliation reads the
active revision/root, and decide from that whether the fix is a successor
install or a retry-free reconcile, *before* editing.

**F. Freeze and verify.** Freeze the ordinary-shell selection (registry, exact
commands, fixtures, limits, receipt schema, oracle) on the **pre-optimization**
source, collect one qualifying control attempt per case, then freeze any
comparative target, then take one candidate attempt per case on the
post-optimization source under the same enforced cache contract. A changed
command, fixture, harness or cache contract means a new scenario, not a pair.
**Gate:** every registered cell is `PASS`, `FAIL`, `INELIGIBLE` or `NOT_RUN`.

## 5. Two evidence blockers you must solve first

Neither is a design blocker; both are prerequisites for any mounted or
end-to-end claim.

1. **The retained prepared state is gone.** `benchmark-results/fs-bench-pro/issue243-shell-package-v1-prepared-02/`
   (image, release `benchmark_shell`, release `verify_shell`, `benchmark_init`,
   three closed masters) no longer exists on this machine, so
   `evidence/phase1-ordinary-shell-repair/run_diagnostic.py` cannot run.
   Re-prepare once with `python3 core/benchmark/fs-bench-pro/shell_package.py
   prepare --output <fresh path>` (it refuses a dirty source), keep it outside
   any measured window, and record its `prepared.json` seal. The equivalent
   Docker images of the Phase 1A round still exist locally
   (`layerfs-shell-package-v1:issue245-fix7` is
   `sha256:26e081383867ec0e59276f38470bbb2002e09167843d7eacaaca13067ed429d1`),
   but the masters and the sealed binaries that go with them do not — check
   before assuming a re-prepare is avoidable.
2. **The Linux-gated suites need the live native service.**
   `crates/layerfs-workspace/tests/support/native_workspace.rs` requires
   `LAYERFS_ENDPOINT`, `LAYERFS_PRIVATE_KEY`, `LAYERFS_SERVER_KEY`,
   `LAYERFS_STAGE_TEST_ROOT` and `LAYERFS_STAGE_BRANCH`, and an attached local
   edit profile resolved through `Operation::HistoryQuery(GetBranch)` — a
   delivery that answers with `Unsupported` fails attach with
   `Service(Failure { code: InvalidInput })`. Either bring up the route harness
   the `*_route.py` drivers expect, or write the fixture deliberately. Do not
   weaken the product to make a fixture easier.

## 6. Operational playbook

- **Fastest real-behaviour loop (packages A–C style).** A component test that
  drives production traversal behind `PieceStore` runs in ~0.03 s:
  ```
  cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline \
      --target aarch64-unknown-linux-musl -p layerfs-workspace --test pieces_sequence
  docker run --rm -v "$PWD/core/target/aarch64-unknown-linux-musl/debug/deps:/d:ro" \
      alpine:3.22 /d/pieces_sequence-<hash> --test-threads=1
  ```
  The musl target directory is `core/target/aarch64-unknown-linux-musl` (not
  `target/`), and `zigbuild` is warm here: a full `-p layerfs-workspace --tests`
  build is seconds, not minutes. `timeout` does not exist on this host's `zsh`;
  use `perl -e 'alarm shift; exec @ARGV' <secs> <cmd>` when a container must be
  bounded.
- **Mounted route.** Build the release daemon
  (`cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked
  --offline --target aarch64-unknown-linux-musl -p layerfs-daemon`), copy it
  over the prepared `image-context/layerfs-daemon`, `docker build -q` a new tag,
  keep `ENV LAYERFS_FUSE_TRACE=1` and `ENV LAYERFS_FUSE_ERROR_DIAGNOSTIC=1` in
  diagnostic images only, then run `run_diagnostic.py`. For a symbolized
  backtrace build the **dev** profile for musl; the release binary is stripped.
  Remove any backtrace printing before committing.
- **Known host limitations, already diagnosed; do not re-investigate.**
  - `cargo +1.85.1 test --all-targets` on macOS compiles the Linux-gated
    `layerfs-workspace/tests/*` suites **out** (they are `#![cfg(target_os = "linux")]`
    or per-item gated). Mounted and native-service behaviour must be verified
    through the container or the route harness.
  - `zigbuild --workspace --all-targets` for musl **fails** linking
    `-lsqlite3` for `layerfs-history` test targets on this host. Build the
    package you need (`-p …--test …`) rather than the whole target set.
  - An external test can reach the frame codecs and the traversal through
    `pub mod backing` and the page-store seam, but it cannot build a
    `MetadataHost`/`Arena`/`RootOwner` fixture: `RootOwner` needs
    `MetadataHost::candidate`, which needs an `Arc<MetadataHost>` and a
    `Directory`. Use the in-memory `PieceStore` for traversal cases and the real
    `Workspace` or the route harness for anything above it.
- `cargo fmt` expands long calls and can push a file past the 999-line ceiling;
  always run `python3 core/tools/check_product_boundary.py` after formatting.
  `metadata_pieces.rs` is at 882 lines — split by responsibility before editing
  it heavily.
- Exact per-commit LOC snapshots: `git archive <parent> | tar -x -C <dir>` for
  before, `git checkout-index -a --prefix=<dir>/` for after (stage new files
  first), then `python3 core/tools/production_loc.py --root <dir> --json`.
- Remove stray `docker run` containers when a job is killed; they hold ports and
  disk. `alpine:3.22` (`5291449c3df7`) is present locally and is enough to run
  the musl test binaries.

## 7. Discipline that is not negotiable

- **Route:** `/bin/sh -c` in the mounted Workspace; the kernel's syscalls decide
  the callbacks. No edit tool, no range ioctl, no command-text classifier, no
  direct Store mutation, no cooperating-tool result relabelled as a shell result.
- **Cache:** a timed phase must pay for its own work from a declared state. If
  the FUSE backing cache cannot be enforced and checked equally for both arms,
  report latency `INELIGIBLE` even when correctness passes. No priming, no
  arm-specific invalidation, no warm-cache credit.
- **Sampling:** one attempt per case per arm, no best-of, no unchanged-arm
  rerun, no timeout or worker increase, no shortened case, no dropped failing
  cell, no historical receipt rewritten. One construction worker;
  `LAYERFS_CONSTRUCTION_WORKERS=1`.
- **Budgets:** complete command ≤ 15 s (declared exceptions ≤ 25 s, the mixed
  refresh already declares 25 s); independent verification < 10 s.
- **Evidence:** append-only paths, raw stdout/stderr, identities (source, build,
  image, fixture, harness), field provenance, and `UNAVAILABLE` for anything not
  measured — never zero by assumption.
- Do not run the retired root preflight and do not claim CI green.
- `inode.edits == u16::MAX` is a deliberate "exact count unknown" marker. If you
  touch `commit/save.rs` or `commit/lower.rs`, preserve it: a version whose
  splice shared an untouched subtree must be lowered, never reused.

## 8. Final reporting

At frozen final source run, once: `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 fmt
--manifest-path core/Cargo.toml --all --check`, `python3
core/tools/check_product_boundary.py`, `python3 -m unittest discover -s
core/tools -p 'test_*.py'`. Diagnose a red result from its output and source,
fix once, then re-run its covering commands.

Report: the per-commit production LOC before/after/delta with scope and counting
method; every registered case as `PASS`, `FAIL`, `INELIGIBLE` or `NOT_RUN` with
its receipt path; the exact commands to reproduce each row; updated
`core/docs/architecture/` pins for the changed format, algorithm or bound;
residual risk and every open gate, including the ones this handoff does not
close (#232's 56 shapes, the deferred load-bearing cases, and the namespace
limits of 128 dirty identities / 128 changed names / 32 KiB).

# #463 Layer split: migration spec

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

This is the work order for the agent that carries out
[#463](https://github.com/Ephemeral-AI-Lab/layerfs/issues/463), part of the
[#446](https://github.com/Ephemeral-AI-Lab/layerfs/issues/446) roadmap (proposal
D8). It splits today's `core/` into three layers: `core/`, a middle layer
(working name `engine/`) and `api/`. Every fact below was read at `c77281e49a`.
Re-check anything that `main` has changed since.

D8 is not decided yet. Do not start a step until its owner gate (below) is
recorded in #463 or in your handoff message. If a gate is open, stop and ask; do
not pick an answer yourself.

## 1. Outcome

| Layer | Crates | May depend on | Prod LOC at `c77281e49a` |
| --- | --- | --- | --- |
| `core/` | content, history, storage, telemetry | other core crates | 38,241 |
| `engine/` | bridge, daemon, fuse, overlay, persistence, project, sandbox, store-service, workspace | core, engine | 56,632 (includes the 378 moved in by step 2) |
| `api/` | layerfs-api, layerfs-node (`api/binding`), the JavaScript packages | anything below | 7,287 |

Total production LOC stays at 102,160 apart from the small edits in step 2.

The migration is done when:

- `check_product_boundary.py` assigns every crate to a layer and fails on any
  production or test edge that points to a higher layer;
- the four core crates have no engine or api crate in their dependency graph,
  including dev-dependencies;
- `layerfs-daemon` no longer depends on `layerfs-api`, by manifest or by
  `#[path]` include;
- the relocation commit changes no crate's production LOC and no dependency
  edge;
- the full check set passes from the new root manifest.

## 2. Owner gates

| Gate | Question | Blocks |
| --- | --- | --- |
| G1 | D8 is accepted as the direction | every step |
| G2 | May `layerfs-project` depend on `layerfs-bridge` for `provision` types? Or should those types move out of bridge, and to where? | step 2a |
| G3 | The middle layer's name. `backend` and `runtime` are taken by `@layerfs/backend` and `@layerfs/runtime`. | step 3 |
| G4 | When step 3 lands. The D8 comment suggests just before A9 or B1 starts. | step 3 |

Step 1 needs G1 only. It doesn't depend on the name: write the layer names as
data in one place so step 3 can rename the middle layer with a one-line change.

## 3. Rules that apply throughout

Read the root [AGENTS.md](../../../../AGENTS.md) and
[core/AGENTS.md](../../../AGENTS.md) first. The rules below are the ones this
migration hits most.

- **Worktree.** Work in your own worktree from local `main`, set up through
  [the parallel worktree launcher](../../../../docs/general/parallel-worktrees.md),
  on branch `issue463-layer-split`. Build and test with that worktree's
  registered Cargo home and target. Never touch another worktree.
- **Disk.** Run `python3 tools/cleanup.py check` at the start, before builds,
  after build batches, at least every 30 minutes, and at handoff. Below 100 GB
  free, follow the cleanup policy.
- **Toolchain.** Host cargo is `cargo +1.85.1`. Keep the repository ARM64 build
  flags in `.cargo/config.toml`.
- **Tests.** Every test command runs under an explicit timeout of at most 120 s.
  Build first with `--no-run`. A test that reaches the ceiling has failed as a
  hang: diagnose it, don't rerun it.
- **Moved tests.** Delete a test that is tricky to adapt rather than nursing it.
  Keep or move only cheap tests of surviving behaviour, and list deleted tests in
  the commit message. Never weaken an assertion.
- **Commit messages.** Every commit message carries
  `Production LOC: <before> -> <after> (delta <signed>)` with scope and method
  (§8).
- **No push.** Do not push, open a PR or merge unless the owner says so. One
  commit per step part (1, 2a, 2b, 3, 4), each building and checked on its own.
- **No CI wait.** CI reports; it never gates. Verify each step yourself (§7) and
  report exact checks and gaps.
- **No product changes.** Don't change behaviour, formats, dependencies or third-party
  code. fs-bench-pro stays frozen and broken.

## 4. Step 1: layer guard (tooling only)

Files: [core/tools/check_product_boundary.py](../../../../tools/check_product_boundary.py)
and [core/tools/test_check_product_boundary.py](../../../../tools/test_check_product_boundary.py).

Today the guard has:

- `ALLOWED_DEPENDENCIES`: an exact allowlist of first-party production edges per
  crate, read from `dependencies` and `build-dependencies` only;
- `DOMAIN_CRATES = {content, storage, history, project}`: crates whose source
  may not name persistence, overlay, workspace, store-service, fuse, daemon
  and similar crates (`ENGINES_AND_CLUSTER2`);
- `crate_name()` and `production_files()`, which assume every crate lives under
  `core/crates/`.

Add:

1. One `LAYER` map from every crate name to `core`, `engine` or `api`
   (`layerfs-node` is `api`), with the layer order `core < engine < api`. The
   guard fails on a crate missing from the map.
2. A production-edge check: an edge to a higher layer fails, on top of the
   existing allowlist.
3. A dev-dependency check: read `dev-dependencies` (including target tables) and
   fail on an edge to a higher layer. Allow exactly one named exception,
   `layerfs-daemon` → `layerfs-api`, with a comment that step 2b removes it.
4. A `#[path = "..."]` check in each crate's `tests/`: resolve the path and fail
   if it lands in a crate of a higher layer. Allow the same daemon → api
   exception for the three files listed in step 2b.
5. A core-crate check: no `rusqlite` in a core crate's manifest.

Leave `DOMAIN_CRATES` as it is in step 1; step 2a changes it.

Tests: cover each new rule with one passing and one failing case, including an
unknown crate, a higher-layer dev edge, the named exception and a `#[path]`
escape. Write paths through one helper so step 3 only changes the roots.

Checks: the guard on the real tree passes. The guard's self-tests pass, and so
does `python3 -B -m unittest discover -s core/tools -p 'test_*.py'`. Ruff format
both files.

## 5. Step 2: Init Store setup and cross-layer tests (product code)

### 2a. Move the Init Store setup into `layerfs-project` (after G2)

Scope: `c77281e49a`, `tools/production_loc.py --files`, only the directories
named below (partial listings).

```diff
  core/crates/layerfs-project/src/    [1719]   (partial: lib.rs and import/ only)
    lib.rs                               20   (edit: declares and exports store)
    import/                            [1699]   (unchanged)
+   store/                                —   new
+     mod.rs                              —   new
      init.rs                           187   (moved from layerfs-api/src/project/init.rs)
      install.rs                        147   (moved from layerfs-api/src/base/install.rs)
      install_types.rs                   44   (moved from layerfs-api/src/base/install_types.rs)
  core/crates/layerfs-api/src/         [1189]   (partial: lib.rs, base/, project/)
    lib.rs                               25   (edit: re-exports Init from layerfs-project)
    base/                               [958]
      control.rs                        289   (edit: install from layerfs-project)
      internal.rs                       130   (edit: install from layerfs-project)
      mod.rs                             10   (edit: drops the install modules)
-     install.rs                        147   (moved to layerfs-project/src/store/)
-     install_types.rs                   44   (moved to layerfs-project/src/store/)
    project/                            [206]
      mod.rs                              6   (edit: re-exports from layerfs-project)
-     init.rs                           187   (moved to layerfs-project/src/store/)
```

The listing shows only `base/` files that change; `base/` holds 10 files in
all. `api/binding/src/project.rs` (233) also changes so that it makes one Init
call into `layerfs-project`.

What to do:

- Move the three files with `git mv` and keep their bodies as they are. Change
  only `use` paths and visibility.
- Keep `layerfs-api`'s public names (`InitRequest`, `ProjectApi`,
  `SealedProject`, `InstallError` and the rest) as re-exports, so the binding
  and the JavaScript packages don't change behaviour.
- In `layerfs-project/Cargo.toml`, `layerfs-persistence` moves from
  dev-dependencies to dependencies. Add `layerfs-bridge` if G2 allows it.
  Otherwise move the `provision` types as G2 directs.
- In the guard, add those edges to `ALLOWED_DEPENDENCIES["layerfs-project"]`.
  Remove `layerfs-project` from `DOMAIN_CRATES`: it is an engine crate, and
  the exact allowlist still bounds it. Its self-tests change to match.
- Tests that check only the Store setup move from `layerfs-api/tests` to
  `layerfs-project/tests`. Tests that drive Init through the API stay where
  they are.

### 2b. Remove the daemon's upward test link

At `c77281e49a`, `layerfs-daemon` has `layerfs-api` as a dev-dependency.
These test files use `layerfs_api`:

- `mount_control.rs`
- `kernel_mount_routes.rs`
- `host_handoff.rs`
- `observed_application.rs`
- `host_install.rs`
- `kernel_application.rs`
- `r5_1_actual_daemon_commit.rs`
- `support/control_fixture.rs`
- `support/complete_fixture.rs`
- `support/host_install.rs`

Most uses are `layerfs_api::internal::workspace` (48) and `internal::install`
(10).

These three files include `layerfs-api` test support by path
(`tests/support/installed_store.rs` and `tests/support/sdk.rs`):

- `merge_report.rs`
- `concurrent_commits.rs`
- `lane_merge.rs`

For each file, in this order:

1. If it only needs to install a Store, switch it to the `layerfs-project`
   install from 2a.
2. If it is an end-to-end test through the SDK surface, move it to
   `layerfs-api/tests`. Move its daemon support with it, or depend on
   `layerfs-daemon` from there.
3. If neither is cheap, delete it, and name it and the behaviour it covered in
   the commit message. If that behaviour is still a valid contract, cover it with
   a test at its new home.

Then drop `layerfs-api` from the daemon's dev-dependencies and remove the guard
exception from step 1. Update [core/tools/ci_tests.py](../../../../tools/ci_tests.py)
for any renamed or moved test target, including its `SLOW` list.

Daemon and mount tests are Linux only: build and run them through the worktree's
Linux resources.

## 6. Step 3: relocation (after G3 and G4)

This commit only renames paths and fixes path strings. If any production
source line changes, other than a relative `include_str!` or `#[path]` string,
stop: that change belongs in its own commit.

Before you start, confirm that no open branch still edits `core/crates`. At
`c77281e49a`, five local branches did: `issue442-fs-mode`,
`fix/merge-lost-link-count`, `fix/merge-replaced-directory`, `pr424-log-cap`
and `claude/amazing-borg-cc92de`. Tell the owner which ones are still open. Do
not rebase or edit them yourself.

### 3.1 Moves

| From | To |
| --- | --- |
| `core/crates/layerfs-{bridge,daemon,fuse,overlay,persistence,project,sandbox,store-service,workspace}` | `engine/crates/…` |
| `core/crates/layerfs-api` | `api/layerfs-api` |
| `core/Cargo.toml`, `core/Cargo.lock` | repository root (no root `Cargo.toml` exists today) |
| `core/.config/nextest.toml` | `.config/nextest.toml` |
| `core/vendor/fuser-0.18.0` | `engine/vendor/fuser-0.18.0` |
| `core/patches/fuser-0.18.0` | `engine/patches/fuser-0.18.0` |
| `core/tools/*` | `tools/` (see 3.3) |
| `core/target` | `target/` (build output; not tracked) |

`core/docs/`, `core/README.md` and `core/AGENTS.md` stay for now (step 4).

### 3.2 Paths that break, found at `c77281e49a`

Cargo manifests:

- The root `Cargo.toml` gets members `core/crates/*`, `engine/crates/*`,
  `api/layerfs-api` and `api/binding`. Also update its `exclude` and its
  `[patch.crates-io] fuser` path, and the header comment, which still
  describes a reference workspace.
- Every moved crate's `{ path = "../layerfs-…" }` must point to the new place,
  for example `../../../core/crates/layerfs-content` from an engine crate.
- `api/binding/Cargo.toml` has 8 `../../core/crates/…` paths.

Test includes that cross crate folders:

- `api/layerfs-api/tests/sandbox_operations.rs`: `#[path]` to sandbox test
  support.
- `engine/crates/layerfs-project/tests/support/mod.rs`: `#[path]` to storage
  test support, which now lives in `core/crates`.
- `engine/crates/layerfs-persistence/tests/sqlite_ordinal_lookahead.rs`:
  `#[path]` to daemon test support. Both crates stay siblings in
  `engine/crates`, so check that it still resolves.
- `include_str!` inside a crate (persistence and overlay SQL) moves with the
  crate and needs no change.

Tools:

- `tools/worktree_build.py` and `tools/worktree_resources.py`: `core/Cargo.toml`,
  `core/Cargo.lock`, `core/crates`, `core/vendor`, `/work/core/target`,
  `macos_target`.
- `tools/test_worktree.py`, `tools/test_production_loc.py`,
  `tools/test_cleanup.py`, `tools/cleanup_discover.py`,
  `tools/decode_observations.py` and `tools/stage5_component_comparison.py`.
- `core/tools/*.py`: these use `Path(__file__).parents[1]` or `parents[2]` as
  roots, and both change when the files move to `tools/`.
- `tools/preflight.sh` is retired: change only its path strings, never revive
  it.

CI and configuration:

- `.github/workflows/ci.yml`: `--manifest-path core/Cargo.toml`, the `core/**`
  path filters (add `engine/**`, root `Cargo.*` and `.config/**`), and the
  `core/tools` commands.
- `.github/workflows/nightly.yml`: also `workspaces: core -> target`.
- `.github/workflows/format.yml`, `ruff.toml` (`include`), `.gitignore`
  (`/core/target-linux/`) and the comments in `.cargo/config.toml`.

JavaScript:

- `api/scripts/build-native.mjs` (target path), and the four
  `api/agent/sdk/test/verify/*_real.mjs` files.

Other:

- `core/crates/layerfs-overlay/tests/device_fixture.sh`.
- `core/crates/layerfs-telemetry/examples/*.rs` (run commands in comments).
- `core/crates/layerfs-daemon/tests/{fp2_negotiation_flags,r5_10_producer_profile}.rs`
  and `support/install_docker.rs`.

Re-run this search after the move and fix every live hit outside the history
docs:

```sh
git grep -nE 'core/(crates|Cargo\.(toml|lock)|target|vendor|patches|tools|\.config)|manifest-path core|workspaces: core' -- . ':!core/docs' ':!docs'
```

### 3.3 Tools merge

- `core/tools/production_loc.py` (152 lines) and `tools/production_loc.py`
  (545 lines, the pinned counter) both exist. Keep the root one. Before
  deleting the core copy, check whether anything calls it.
- The root counter's `core` scope is `core/crates`. Give it scopes `core`,
  `engine` and `api`. Keep recognising the old `core/crates/*` layout, so a
  pre-move tree still counts: assign every crate to its layer by crate name,
  not by folder.
- `check_product_boundary.py`, `check_fuser_integrity.py`, `ci_tests.py`,
  `phase7_services.py`, `sdk_telemetry_report.py`, `substitution-proof/` and
  their `test_*.py` files move to `tools/`. Their roots and crate globs follow
  the new layout, with one constant for the crate roots.
- The CI command `unittest discover -s core/tools` becomes `-s tools`. Check
  that root `tools/` tests and the moved ones run together cleanly.

### 3.4 Worktree resources

Registered worktrees record `core/target` paths. After the move:

1. Prepare your worktree's resources again for the new target path.
2. Release your old `core/target` cache through the cleanup script at handoff.
3. Do not migrate other worktrees' records. Tell the owner they need to
   prepare again after they rebase.

## 7. Verification per step

Run each command once per step at that step's final tree. Run tests per
package when the whole suite cannot fit in 120 s. All commands run from the
repository root.

Step 1:

```sh
python3 -B core/tools/check_product_boundary.py
python3 -B -m unittest discover -s core/tools -p 'test_*.py'
```

Steps 2a and 2b: the full core check set from
[core/AGENTS.md](../../../AGENTS.md#checks-and-completion), with focused test
runs for `layerfs-project`, `layerfs-api` and `layerfs-daemon` (Linux), plus
the binding build (`node api/scripts/build-native.mjs`).

Step 3, from the root manifest:

```sh
cargo +1.85.1 test --locked --all-targets --no-run
cargo +1.85.1 clippy --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --all -- --check
python3 -B tools/check_product_boundary.py
python3 -B tools/check_fuser_integrity.py
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

Then run tests per package under the 120 s ceiling, with the Linux daemon and
mount targets through the worktree's Linux resources. Then check that nothing
but paths changed:

- **Same graph.** `cargo metadata --no-deps` at the parent and at the new tree
  list the same packages, the same first-party edges (by name and kind) and the
  same features. Only the manifest paths differ.
- **Same LOC.** `tools/production_loc.py --files --json` at the parent and at
  the new tree give identical production LOC for every crate.
- **Only renames.** In `git diff -M --stat <parent>`, every moved file shows as
  a rename. Every edited file is on the list in 3.2 or 3.3.
- **Binaries build.** The Node binding and the daemon build for Linux through
  `tools/worktree.py`. The fuser integrity check passes at its new path.

Step 4: every local link and anchor in the changed docs resolves. Rust and
runtime checks don't apply to a docs-only change; say so in the report.

## 8. Production LOC per commit

Use `python3 tools/production_loc.py` from the commit's own tree. For step 3,
use the new counter, which recognises both layouts, on both the parent and the
staged tree.

| Commit | Expected line |
| --- | --- |
| 1 | `Production LOC: 102160 -> 102160 (delta 0)`, tooling only |
| 2a | the actual count: moved lines are unchanged; new `store/mod.rs` and re-export edits add a little |
| 2b | the actual count; test-only changes give delta 0 |
| 3 | `<n> -> <n> (delta 0)`, relocation, with per-layer subtotals for core, engine and api |
| 4 | unchanged total, delta 0, docs only |

Report the reference as `reference absent (retired R9c; last counted 65,417
at tree 498dd1917812)`. Don't call the move a simplification.

## 9. Step 4: docs

- Split [core/AGENTS.md](../../../AGENTS.md) into one guide per layer: what stays
  for `core/`, a new `engine/AGENTS.md` (daemon, FUSE, fuser patches, Store
  seal file control) and `api/` notes. Update the root
  [AGENTS.md](../../../../AGENTS.md) workstream table, links and check
  commands.
- Update `core/README.md`, the crate READMEs, `engine/patches/fuser-0.18.0/README.md`
  and the six `.agents/skills/layerfs-js-sdk` references that name `core/`
  paths.
- Fix `core/crates/…` links in maintained docs only (`docs/general/`, guides,
  current design indexes). About 7,600 references in 641 files under
  `core/docs` and `docs` are mostly dated records and historical receipts.
  Leave those unchanged: they cite the tree at their own commit.
- Follow the [documentation policy](../../../../docs/general/documentation-policy.md):
  one status banner, repository-relative links, verified before review.

## 10. Outside this repository (report, don't change)

- layerfs-benchmark `common/lib.sh` hard-codes `core/target/fs-bench-bin`.
- dsh-layerfs and magpie-layerfs pin a LayerFS commit. In dsh-layerfs, `core/`
  appears only in docs, but its next pin bump builds from the new root
  manifest.
- Open a follow-up for each only when the owner asks.

## 11. Stop and report when

- a gate is open for the step you are about to start;
- the graph or the per-crate LOC differs across step 3;
- a test hangs at the ceiling or fails for a reason you cannot trace to the move;
- a fix would change product behaviour, a format or a dependency version;
- free disk is below 100 GB and cleanup cannot recover 300 GB.

## 12. Handoff report

For each commit:

- its hash, its `Production LOC` line and the exact checks run, with results;
- deleted tests, and moved tests with their new homes;
- guard exceptions still open;
- branches the owner must rebase;
- worktree caches released;
- the free-disk reading.

Report gaps as gaps. A passing guard or a green CI run is not proof that the
migration is correct.

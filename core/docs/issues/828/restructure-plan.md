# #828 restructure: execution plan (owner-approved 2026-11-27)

This plan moves `layerfs_v2_0_0` to the folder and crate layout in the plan
mapping (`mapping-{core,engine,control,api}.json`), following the rules in
`layout-style.md`.

- **Base:** `layerfs_v2_0_0` at `3203f6acbf` (#835 merged). Its tree is
  byte-identical to the tree the mapping was built from: 240,846 production
  lines.

## Size

| | Files |
| --- | --- |
| Source files that move or get renamed | 1,271 |
| …of which need hand edits (split, merge or new) | 271 |
| Test files that move | 1,155 |
| SQL files that move | 210 |
| Other files that move | 60 |

**Crate changes:**

- Engine `layerfs-workspace` → `layerfs-vfs`, and `layerfs-mount-ports`
  merges into it.
- `layerfs-bridge` splits into `layerfs-protocol` and `layerfs-os`. The pieces
  only control uses go to home (`log/rotate.rs`) and Sandbox
  (`backend/namespaces/linux/runtime.rs`).
- Control `layerfs-platform` → machine, `layerfs-bucket` → store, and
  `layerfs-sandbox-backend` → sandbox.
- `layerfs-control` dissolves into inspect, machine, store, sandbox, home and
  api.
- New crates: `layerfs-inspect`, and the control-layer `layerfs-workspace`.
  The name is free once the engine crate is renamed `layerfs-vfs`.

## Owner rulings

1. The plan (mappings, current-tree CSVs, `build.py`, `layout-style.md`, the
   review reports and this file) is committed under `core/docs/issues/828/`
   as the first commit.
2. Merges into `layerfs_v2_0_0` are held during the run. If something lands
   anyway, regenerate the moves on the new tip; don't hand-merge renames.
3. One pull request into `layerfs_v2_0_0`. Each wave and agent keeps its own
   commits.
4. Construction `reference/` and `reference_index/` keep their names for now.
5. Content `compare/across.rs` keeps its name.
6. Sandbox `route/host/` → `route/machine/`, and the control-layer workspace
   crate's `host/` → `machine/`. The wire call `sandbox.host` keeps its name.
7. The nightly FUSE and mount tests are reported as a gap.
8. The fan-out uses background subagents (the Agent tool), driven by one
   session.

## Speed rules

- **One move tool.** A script outside the product reads mapping rows for a
  crate. It:
  - runs `git mv`;
  - regenerates `mod` declarations;
  - rewrites `crate::`/`super::` paths, `include_str!`/`include!` SQL paths,
    test `#[path]` attributes and `[[test]]` entries;
  - prints the production LOC line for the commit message.

  Use it everywhere; never hand-move files.
- **Public APIs stay frozen in wave 2.** A crate's public paths stay valid
  during its internal restructure. Where a public module path changes, the
  crate root keeps a temporary re-export. Each agent hands back an
  old → new public-path map, and wave 3 rewrites callers and deletes the
  re-exports before the pull request.
- **Compile only while working:** `cargo +1.85.1 check --locked -p <crate>
  --all-targets`. It compiles the tests, but nothing executes until wave 3.
- **Commits.** A pure-rename commit stays separate from its hand-edit commit.
  Every commit records `Production LOC: <before> -> <after> (delta …)`, split
  by core, engine, control and api, using `tools/production_loc.py`.
  Deleted tests are listed in the commit message (refactors delete tests that
  are tricky to adapt).

## Waves

**Wave 0 (driver, serial)**

- Make the worktree with `tools/worktree.py`, from `layerfs_v2_0_0`.
- Commit the plan docs.
- Build the move tool and prove it on one small crate (telemetry).
- Check disk with `python3 tools/cleanup.py check`.

**Wave 1 (driver, serial): crate-level changes**

- Make every crate-level change above. Move cross-crate files straight to
  their final paths (about 550 files).
- Rewrite callers' crate names mechanically.
- Include the fixes needed to compile:
  - `MachineUsers` gains reconcile, forget, holders and force-stop hooks,
    implemented in Sandbox's root `users.rs`;
  - machine `lifecycle/host.rs` and `status/worker.rs`;
  - the host worker server's move from api into store
    `admin/host_worker/`, splitting the host half out of
    `worker/connection.rs` and the host switch out of `fault.rs`.
- Update the root `Cargo.toml` members and the lockfile.
- Update `tools/production_loc.py` (it assigns layers by crate name),
  `check_product_boundary.py`, `ci_tests.py`, `linux_bundle.py`,
  `worktree_build.py` and their tests in the same commits.
- Done when `cargo check --workspace --all-targets` is green.

**Wave 2 (parallel subagents): restructure inside crates**

Each agent works in its own `tools/worktree.py` worktree from the wave-1 tip,
with a cloned warm target and about 2 build jobs. Run 5 at a time, heaviest
first, and check disk between batches.

| Agent | Crates |
| --- | --- |
| A | persistence (includes the 210 SQL moves) |
| B | store-service |
| C | store |
| D | machine, home, inspect |
| E | daemon, vfs |
| F | sandbox, control-layer workspace |
| G | overlay, fuse, nfs |
| H | content, history, telemetry |
| I | storage, project, os, protocol |
| J | api, cli, `api/` packages |

**What each agent does:**

- Touches only its own crates' directories.
- Uses the move tool, then does its crates' split, merge and new rows by hand.
- Keeps its public paths valid.
- Checks with `cargo check` only.
- Makes two commits, renames and then hand edits, each with LOC.
- Hands back:
  - its public-path map;
  - its deleted tests;
  - any mapping row it couldn't follow, with evidence.

**What the driver does with the results:**

- Check each agent's claims and diff.
- Re-brief the agent on any gap.
- Don't accept an agent whose crate doesn't compile.

**Wave 3 (driver): join and verify once**

1. Merge the agent branches. They touch disjoint files.
2. Apply the public-path maps to the callers and delete the temporary
   re-exports.
3. Rewrite path references in `docs/architecture` and the layer guides from
   the mapping, in the same commits as the code. Update the crate tables in
   `AGENTS.md` and the layer guides.
4. Grep audits for what the compiler can't see:
   - runtime paths (`CARGO_MANIFEST_DIR` fixtures);
   - SQL names registered as strings, and the `#818` visibility-guard
     registrations;
   - `#[path]`, `include_str!`;
   - the crate names in tools.
5. Run the checks once, at the final commit:
   - `cargo +1.85.1 check --workspace --all-targets --locked`;
   - `cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node
     --all-targets -- -D warnings`;
   - `cargo +1.85.1 fmt --all -- --check`;
   - `python3 -B tools/check_product_boundary.py`;
   - `python3 -B -m unittest discover -s tools -p 'test_*.py'`;
   - `cargo test --no-run`, then the fast lane from `tools/ci_tests.py`;
   - the slow-lane targets for hand-edited code: persistence acquisition,
     the store host worker, the machine and Sandbox users hooks, and the
     store `open/` split. Every test command runs under a 120 s wall cap;
     reaching it is a hang to diagnose, not to rerun.
6. Re-run the layout checker on the result:
   `python3 core/docs/issues/828/build.py core/docs/issues/828`, or the
   equivalent check that every planned folder exists and obeys the size
   rules. The two `examples/` folders (nfs, overlay) already fail at the base
   and are out of scope.
7. Open one pull request into `layerfs_v2_0_0`.

## Terminal pass

The run is done when all of these hold:

- The tree matches the mapping, every row applied or listed as a justified
  deviation.
- No temporary re-exports remain.
- Every check in wave 3 step 5 passes, or has a reported reason.
- The production LOC change is recorded per commit.
- The pull request is open with its check report and gaps (nightly FUSE and
  mount tests).

Don't merge it; the owner merges.

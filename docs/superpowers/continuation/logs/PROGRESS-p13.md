# PROGRESS — P13 folder-scoped client (api/agent + api/backend + exec cwd)

Environment: WSL2 Ubuntu on E: (cargo 1.85.1, node 22.22.1, python 3.12),
worktree at /mnt/e/layerfs-connect/worktrees/p13 (branch copy of layerfs).
All suites under explicit `timeout 120` (Rust build steps given 600s). No
mounted/real-E2E tests in this slice: /dev/fuse and Docker do not exist here,
so mounted `workspace.exec` (the actual bash run) is NOT RUN; the cwd
validation/joining it depends on is covered by pure Rust unit tests.

- Red-to-green: with the host worktree diff saved as /tmp/p13-red.patch and
  the production changes reverted in the host checkout, 13/14
  area-p13-scope scenarios FAIL for the intended reasons (scope accepted and
  ignored, out-of-scope paths dispatched, unprefixed Branch names, history
  answered, events/backend reachable); the 14th (unscoped call shapes) is a
  regression guard that passes both before and after. `exec_cwd` fails to
  compile on old code (`exec_directory` absent).
- JS area-p13-scope: PASS 14/14 scenarios (new).
- JS area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group
  + 1 budget traversal.
- JS area-p9-events: PASS 13 scenarios.
- JS area-p10-results: PASS 11 scenarios.
- JS area-p11-storage: PASS 5/5 scenarios.
- JS calls: PASS 66 cases.
- JS trace: PASS 9/9 cases (on, off, failure, layerfs-env, start, ids,
  no-content, store, daemon-host).
- JS exec_output (touched file): PASS `exec output: ok`.
- Rust layerfs-node exec_cwd: PASS 4/0 (new; no Docker, no FUSE).
- cargo +1.85.1 fmt --all -- --check: clean.
- cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node
  --all-targets -- -D warnings: clean (28.7s).
- cargo +1.85.1 clippy --locked -p layerfs-node --all-targets
  -- -D warnings: clean (40.6s; the binding crate is excluded from the
  workspace-wide run but was changed, so it was checked separately).
- check_product_boundary: PASS 970 files.
- tool self-tests: PASS 155/0.
- Rust area_p1_history: PASS 8/0.
- Rust area_p14_paging: PASS 8/0.
- Rust area_p7_attribution: PASS 11/0.
- Rust area_p11_statistics: PASS 3/0.
- Rust sdk_method_golden (umask 022): PASS 1/0 (no method shape moved; the
  new `cwd` is an optional internal binding input, not a public SDK field).
- Rust layerfs-bridge control_records: PASS 4/0.
- Rust layerfs-bridge observed_records: PASS 7/0.
- Not runnable here and outside the P13 matrix: init_refusals, reinit,
  binary_write (E2E-launcher suites; they read `LAYERFS_STORE_PATH`, which is
  absent here) and packaging (needs the explicit-resource-management Node
  flag). Each aborts before reaching any changed code, so this is
  environmental, not a regression.

## Round 2 — after the review fixes (append-only)

Review fixes now in the tree (not reverted): `scope.js` `changePaths()` makes
`fs.apply` validate `from` and `to` on `cp`/`mv` changes, not only `path`
(the binding's change parser accepts both shapes, so a cp/mv could otherwise
name either side outside the folder and still dispatch); the out-of-scope
sweep in `area-p13-scope.mjs` gained the four missing cp/mv cases
(inside→outside, outside→inside, mv both directions).

`api/backend/src/index.js`: P13 adds one line to `LayerFSBackend.of` — a
client carrying `scope` is refused with `LayerFSError("InvalidInput",
"out of scope: backend", null)`. Reason: `backend.store.fs.get_storage` and
`list_revisions` take `path`, and other backend calls return unfiltered
Workspace change paths and history/branch data, so a backend handle would
bypass the folder and Branch-namespace guards. Unscoped clients are
unchanged, which is why the backend suite still passes as before.

`events.js` now sends `events.subscribe` through `client.call` instead of
`invokeClient`; this does not change P10 recording, because the per-call
record is written by `invoke`/`invokeClient`'s sink, not by `client.call`.

- JS area-p13-scope (re-run): PASS 14/14 scenarios.
- JS area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group
  + 1 budget traversal.
- JS area-p9-events: PASS 13 scenarios.
- JS area-p10-results: PASS 11 scenarios.
- JS area-p11-storage: PASS 5/5 scenarios.
- JS calls: PASS 66 cases.
- JS trace: PASS 9/9 cases.
- JS exec_output: PASS.
- JS backend (surface, closed, comparable, get-log): PASS 4/4 invocations.
- Rust exec_cwd: built with `cargo test --no-run` (12.7s) and executed as
  `timeout 120 target/debug/deps/exec_cwd-0d7c185e20882f75 --nocapture`:
  PASS 4/0.
- cargo +1.85.1 fmt --all -- --check: clean.
- cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node
  --all-targets -- -D warnings: clean.
- cargo +1.85.1 clippy --locked -p layerfs-node --all-targets
  -- -D warnings: clean.
- check_product_boundary (`-B`): PASS 970 files.
- tool self-tests: PASS 155/0.

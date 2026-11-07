# project.export: PR #600 review, port and fixes

> **Status:** Current record for
> [layerfs-project#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53);
> branch `pr600-export-port`, not pushed.

[PR #600](https://github.com/Ephemeral-AI-Lab/layerfs/pull/600) by jiangkoumo
(git author `liuyuchen`) implements `project.export`. It was cut from
`layerfs_v2_0_0` at `b1802332b` and conflicted with the base by the time it
was reviewed (151 commits later, past the control split #601 and the typed
worker frames layerfs-project#109). This branch keeps the author's commits
unchanged, ports them onto `layerfs_v2_0_0` at `98ebfdfb96` in one merge,
and adds four fix commits on top.

## The author's change (PR head `c71f892159`)

| Commit | What |
| --- | --- |
| `608b9aba16` | `project.export(project, destination, { branch, on_progress })`, CLI `project export <store>/<project> <destination> [--branch] [--progress]`: one pinned Branch snapshot, one strict local writer into a hidden sibling staging folder renamed into place, a worker route that streams 2 MiB parts for SQLite on Docker, Lima and Apple Container, PostgreSQL in the caller's process, `prepare`/`export`/`finalize` progress, Store lease held throughout; tests and docs |
| `89b62130e7`, `15e3258e5e` | merges of the base at the time |
| `3b3cb179cb` | regenerated Node/Python typings, architecture docs |
| `bc6014b965` | 2.0.0 guide call count 61 -> 62 |
| `c71f892159` | symlink-alias and schema-v1-refusal tests |

Production LOC, counted with the counter in the author's tree
(`tools/production_loc.py` `9e7cf318`): `b1802332b` 141,113 ->
`c71f892159` 142,081 (delta **+968**), all in the API layer:
`control/project/export.rs` +914, `staged.rs` +15, worker client and
server +14, call table and family plumbing +25. The PR description's
140,847 -> 141,815 comes from an older counter; the delta is the same.

## The port (merge `abc46067d4`)

Only what the new layout required changed; behaviour is the author's.

- The exporter moved to `layerfs-store` and takes `&Path` + `Events`, a
  typed `ProjectExport`, and `OpenArgs`; its worker request is a new
  `ExportFrame` beside the other project frames.
- The base had rewritten the folder-copy writer for parallel Init parts
  (folders may exist, pieces land at any offset). Export depends on the
  author's strict writer to turn a collision (a case-insensitive host's
  `A` and `a`) into `Conflict` instead of merging or overwriting, so both
  modes stay: Init keeps the lenient path, export calls `write_exported`.
- The base already had 62 calls (`workspace.exec_cancel`), so with export
  the table has 63; tests and docs say 63.

Production LOC against the base: 143,873 -> 144,942 (delta +1,069).

## Fixes on top

| Commit | Finding | Change | Production LOC |
| --- | --- | --- | --- |
| `2ac0151aab` | the worker client polled a thread-local owned by export to stop a stream; after the split it cannot see that crate | the client's send path takes an event sink that may refuse an event; a refusal closes the connection and is the outcome; `Worker::call_until`; other callers unchanged | 144,942 -> 144,934 (-8) |
| `480715b8a1` | events fired at 200 ms, 4 MiB or 512 entries, whichever first, so the rate grew with disk speed; `finalize` came before the rename, so a failed publish showed `finalize` then a refusal | at most one `export` event per 200 ms besides the first and last; `finalize` only after the rename; three tests | 144,934 -> 144,920 (-14) |
| `b684270c4f` | `export.rs` at 980 physical lines, five jobs, at the 999-line ceiling | split into `export/{mod,route,destination,writer,walk}.rs`, no behaviour change | 144,920 -> 144,952 (+32, relocation) |
| `97fac452bd` | Apple Container's published port drops what it still holds for the host when the guest closes first; the worker closed right after its answer, so about 1 in 3 small-file exports from Apple Container lost the tail (`failed to fill whole buffer`) | the worker waits, at most `REQUEST_WAIT`, for the client's close after writing its answer; test `worker_frames::the_worker_waits_for_the_client_to_close_after_its_answer` | 144,952 -> 144,962 (+10) |

Layout at `b684270c4f`, `tools/production_loc.py --files` (`d3148db8`),
`control/crates/layerfs-store/src/project/` only, other files not shown:

```diff
  project/                            [1,437]  (partial: files this work touches)
    frame.rs                             159   (ExportFrame added)
    staged.rs                            221   (strict export mode added)
    tree.rs                              131   (Record is Copy)
+   export/                              [926]
+     mod.rs                               5   declarations only
+     route.rs                           293   call, routes, worker side
+     destination.rs                     178   destination checks, staging
+     writer.rs                          121   local writer, progress
+     walk.rs                            329   snapshot walk, part encoding
```

## Review findings withdrawn

- **setuid/setgid on export.** The review said export restores those bits.
  The content grammar cannot hold them: files and symlinks carry `0o777`,
  directories `0o1777`; Init and every Workspace write refuse more, and
  reads validate. A directory's sticky bit is the only special bit a
  folder can receive. Export keeps full fidelity; the test
  `snapshots_hold_no_setuid_and_a_sticky_directory_is_kept` pins this.
- **Commit-message LOC in `c71f892159`.** Its 142,081 is correct for the
  counter in that tree.

## Checks (macOS, `cargo +1.85.1`, load average 10–20)

| Check | Result |
| --- | --- |
| `cargo check --workspace --all-targets`; `-p layerfs-api --features postgresql --tests` | clean |
| `clippy --no-deps -D warnings` on layerfs-store, -machine, -control, -api | clean |
| `tools/check_product_boundary.py` | PASS |
| layerfs-api tests: `project_export`, `project_init_worker`, `calls_table`, `project_family`, `sandbox_records` | 11/11, 7/7, 15/15, 2/2, 2/2 |
| Node 24: `project_export.mjs` (native, CLI), `calls.mjs`, `package.mjs`, `events.mjs` | 17 pass, 1 skipped (exec, needs `LAYERFS_DAEMON`) |
| Python 3.14: `test_calls`, `test_stubs` | 7/7 (1 daemon skip), 8/8 (2 mypy skips) |
| Python 3.14: `test_project_export` | 6/8; the two `aio` failures are the base's callback race; with PR #599's `aio.py` applied temporarily, 8/8 |
| Worker tests at `97fac452bd`: layerfs-control `worker_frames`, `worker_hello`, `worker_lifecycle`, `worker_trace`, `worker_project_record`; layerfs-api `project_export`, `project_init_worker`, `store_transfer_crash`, `worker_mismatch`; Clippy on layerfs-control | 6/6, 7/7, 1/1, 2/2, 1/1; 11/11, 7/7, 4/4, 1/1; clean |
| Exports from native, Docker, Lima and Apple Container into a host folder (release build, sealed bundle) | at `4cd63f16dd` 7/8 exact, Apple Container E01 failed (the close-order bug); at `97fac452bd` 8/8 QUALIFIED, plus 12/12 small and 4/4 large untimed Apple Container exports exact. Reports in layerfs-benchmark `result/2026-11-08/fs-bench-pro/` (branch `fs-bench-pro-export`, not pushed) |

## Not run

| Item | Why |
| --- | --- |
| `postgres_connect` and any PostgreSQL export | no server or `LAYERFS_TEST_PG_URL` on this host; last run by the author at `608b9aba1` and `23fa36892` |
| Linux | macOS host only |
| Durable persistence profile | `NOT_RUN — disabled by owner until explicit reauthorization` |

## Open

- PR #599 must merge first for Python `aio` progress.
- Apple Container's truncated worker answers (about 36% of 9.4 MB exports
  in the author's runs, `store.pull` too) were the close-order bug fixed in
  `97fac452bd`. That fix touches every worker call, not only export, and
  may be split into its own PR.

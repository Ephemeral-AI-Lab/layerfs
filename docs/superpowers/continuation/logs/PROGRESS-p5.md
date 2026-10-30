# PROGRESS — P5 MCP server (area-p/p5, base abd6d57c8)

Environment: WSL Node v22.22.1, npm 11.11.1, cargo 1.85.1; worktree on /mnt/e. Every command had an explicit `timeout 120`. Exact dependencies: @modelcontextprotocol/server 2.4.0, zod 4.6.7.
- mcp-stdio-red-old-code: FAIL 6/6 as intended, `Cannot find module api/mcp/src/server.js`; diffs saved `/tmp/p5-red-tests.diff`, `/tmp/p5-red-tracked.diff`, `/tmp/p5-server-production.diff`.
- mcp-stdio-final: PASS 6/6 (handshake/50 strict tool schemas; delegation/refusals/Branch grammar; duplicate-open lifecycle; 43-case operation parity sweep; Workspace EOF cleanup; event notifications/cancellation).
- mcp-real-store: NOT RUN 0/1 — repository native build `--debug` with `CARGO_BUILD_JOBS=3` hit timeout 120 three times (exit 124) and produced neither `layerfs.node` nor `target/debug/liblayerfs_node.so`.
- area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group + 1 budget traversal.
- calls: PASS 66/66.
- sdk_method_golden (umask 022): NOT RUN 0/1 — locked offline `--no-run` build hit timeout 120 three times (exit 124), no test binary produced.
- cargo +1.85.1 fmt --all -- --check: PASS, clean.
- cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node --all-targets -- -D warnings: PASS (1m07s).
- python3 -B tools/check_product_boundary.py: PASS, 969 production files scanned.
- tool self-tests with cargo in PATH: PASS 155/155.
- dependency/diff/syntax checks: PASS npm ls exact pins, node --check, git diff --check.
- mounted/real-E2E and Docker: NOT RUN — /dev/fuse and Docker unavailable; only the plain real-Store scenario was in scope and is blocked above.
- Durable execution: NOT RUN — disabled by owner; no Durable profile invoked.

## Revision after independent review (2026-10-30, verdict REVISE)

Seven corrections applied to the P5 contract and the implementation: `fs.apply` now carries the SDK's seven change kinds (write/edit/mkdir/chmod/rm/cp/mv with modes); `backend.get_history` takes `since`, `sandbox.get_log` takes `lines` 1..2000 (default 200); `fs.read` returns the whole SDK answer and defaults `binary` false; `sandbox.create` takes only `image` and `fs.mkdir` no `recursive`; `branch.merge.units` accepts patterns or `{name,patterns}` groups; exact pins `@modelcontextprotocol/server` 2.4.0 / `zod` 4.6.7 (plus `@modelcontextprotocol/core` 2.4.0) recorded in `api/package-lock.json` with the same integrity digests as `tools/build-mcp/package-lock.json`; events wording covers cancellation, close and stale/error termination. Build-vs-test ceiling ruling applied: compilation gets a long budget, the produced binary runs under `timeout 120`.
- mcp-stdio-red-revision: FAIL 4/8 as intended (apply rejected mode/base64/edit/chmod/rm/cp/mv, `fs_read.binary` had no default, `get_history.since` rejected, merge unit groups rejected); test snapshot `/tmp/p5-red-stdio.test.js`.
- mcp-stdio-final (Windows node v25.8.2): PASS 8/8 (1.86s).
- mcp-stdio-final (WSL node v22.22.1): PASS 8/8 (50.3s under `timeout 120`).
- mcp-real-store (WSL): PASS 1/1 (3.8s) — supersedes the earlier NOT RUN. Addon built by `api/scripts/build-native.mjs --debug` with `CARGO_BUILD_JOBS=3` under a 600s build budget (8m05s, exit 0). Scratch root moved to ext4 with `LAYERFS_MCP_TEST_TMPDIR=$HOME` because the default temp directory is tmpfs and the SDK refuses direct Store access there (`Unsupported`); the scenario forks `work` from `main` because `main` is ReadOnly ("main changes only through publish").
- sdk_method_golden (umask 022): PASS 1/1 (0.30s) — supersedes the earlier NOT RUN. Built with `cargo +1.85.1 test -j 3 --locked -p layerfs-api --test sdk_method_golden --no-run` under a 600s build budget (3m04s); ran `timeout 120 target/debug/deps/sdk_method_golden-263c15b334c0ee8e`.
- area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group + 1 budget traversal. calls: PASS 66/66.
- cargo +1.85.1 fmt --all -- --check: PASS, clean. cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node --all-targets -- -D warnings: PASS (1m01s).
- python3 -B tools/check_product_boundary.py: PASS, 969 production files. Tool self-tests: PASS 155/155.
- node --check on server and tests, `git diff --check`, lockfile pin/integrity check: PASS.
- `mode` fields are forwarded unbounded (forwarded test asserts 4096 reaches the SDK) so the binding/SDK, not the MCP schema, decides whether a mode is storable; only negative modes are schema-invalid.
- mounted/FUSE and Docker: NOT RUN — unavailable. Durable execution: NOT RUN — disabled by owner.

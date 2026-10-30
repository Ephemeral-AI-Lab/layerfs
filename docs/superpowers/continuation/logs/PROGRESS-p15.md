# P15 progress: npm preparation ONLY

## Scope and rulings

- **Native addon scope ruling**: The native addon build (`layerfs.node`) is not a P15 deliverable; it belongs to the release/verification gate. Packaging tarballs contain JavaScript source only, and the native addon rule is explicitly documented (Linux uses external `LAYERFS_NATIVE`; macOS/Windows use the external runtime bundle).
- **CLI test execution ruling**: The full CLI suite (106 tests) cannot fit within the 120 s test ceiling in WSL in a single invocation due to /mnt/e 9p filesystem latency (~2.5-4 s per test). Per repository policy, the suite is split into 6 batches using Node's `--test-name-pattern`, each running strictly under `timeout 120`. All 106 tests pass on Linux (WSL).
- **Pre-existing Windows CLI path artifact**: On Windows host, `init: the command matches the SDK` fails with `path: 'E:\\fake\\src'` vs `'/fake/src'` (105 passed, 1 failed out of 106). This failure was reproduced identically on the base repository at HEAD (`/e/layerfs-connect/layerfs/api/cli/test/cli.test.mjs` in 12724 ms: 105 passed, 1 failed with identical diff). It is a pre-existing Windows path-representation artifact, not a P15 regression. On Linux (WSL), this exact test passes (`ok 2 - init: the command matches the SDK`, 1984 ms).
- **Removal of `api/cli/package-lock.json`**: `cli` was added to `api/package.json` workspaces. A package-local lockfile is redundant with the root workspace lockfile and fights workspace resolution. The offline tarball install in `prepare-publish.mjs` proves offline resolution works.
- **Dependency change `file:../agent` -> `1.1.0` in `api/cli/package.json`**: A `file:` dependency cannot be published to npm; changing to exact set version `1.1.0` is required for publication. npm workspace linking resolves exact matching versions locally without network access.
- **Publication boundary**: Hard boundary preserved. No npm publish or dry-run was executed; no npm token or account was created; no registry was contacted.

## Verification matrix

| Scope / Suite | Command | Exit Code | Result / Counts | Notes |
|---|---|---|---|---|
| Manifest metadata audit | `node api/scripts/prepare-publish.mjs` (static checks) | 0 | 4/4 packages valid | Non-private, MIT, exact version 1.1.0, repo `Ephemeral-AI-Lab/layerfs` |
| License equality | `sha256sum LICENSE api/{agent,backend,runtime,cli}/LICENSE` | 0 | 5/5 hashes identical | SHA-256 `e20a92efe4b92c0460bd0c475395166e37b6450ed410af155fa0ed99d479f676` |
| Workspace install | `npm --prefix api install --offline --ignore-scripts` | 0 | `up to date in 1s` | Links agent, backend, runtime, cli offline |
| Pre-publish pack/install gate | `node api/scripts/prepare-publish.mjs` | 0 | 4 packages packed & installed | agent 49, backend 18, runtime 27, cli 47 required files; 4 imports ok; installed `layerfs --version` (1.1.0) and `layerfs help` pass |
| CLI README command | `layerfs help` (via PATH from installed bin) | 0 | `layerfs: every @layerfs/agent call as one command` | Help text and usage verified |
| CLI WSL batch 1 (init, project, branch) | `timeout 120 node --test --test-name-pattern="^(every table\|init:\|project \|branch (fork\|list_commits\|diff\|merge\|publish))" api/cli/test/cli.test.mjs` | 0 | 13/13 pass (0 fail, 29.9s) | `init: the command matches the SDK` PASSES on Linux |
| CLI WSL batch 2 (fs commands) | `timeout 120 node --test --test-name-pattern="^fs " api/cli/test/cli.test.mjs` | 0 | 18/18 pass (0 fail, 55.5s) | All fs SDK-to-CLI parity cases pass |
| CLI WSL batch 3 (workspace, sandbox, P1, P7) | `timeout 120 node --test --test-name-pattern="^(workspace \|sandbox \|P[17] )" api/cli/test/cli.test.mjs` | 0 | 14/14 pass (0 fail, 46.0s) | Workspace, sandbox, P1, P7 parity cases pass |
| CLI WSL batch 4 (P8 cases) | `timeout 120 node --test --test-name-pattern="^P8 " api/cli/test/cli.test.mjs` | 0 | 22/22 pass (0 fail, 53.5s) | P8 ref/commit/layer/symlink/binary cases pass |
| CLI WSL batch 5 (usage errors) | `timeout 120 node --test --test-name-pattern="^usage error, nothing called:" api/cli/test/cli.test.mjs` | 0 | 22/22 pass (0 fail, 25.0s) | All 22 CLI syntax/usage error checks pass |
| CLI WSL batch 6 (defaults, flags, help, metadata) | `timeout 120 node --test --test-name-pattern="^(defaults:\|the client\|dashed\|init resolves\|usage errors\|apply with\|a missing\|--project\|after --\|bodies:\|a refusal\|exec --\|--text\|help\|there are\|docs/\|api/)" api/cli/test/cli.test.mjs` | 0 | 17/17 pass (0 fail, 72.5s) | Defaults, options, text mode, doc tables pass |
| CLI WSL total | Combined 6 batches | 0 | **106/106 pass (0 fail)** | All CLI tests pass under 120s ceilings on Linux |
| CLI Windows host | `node --test api/cli/test/cli.test.mjs` | 1 | 105/106 pass (1 fail, 10.3s) | Single failure is pre-existing Windows path representation (`E:\fake\src` vs `/fake/src`), verified at base HEAD |
| Agent calls | `node api/agent/test/calls.mjs` | 0 | PASS calls (66 cases) | Verified against fake-native stand-in |
| Agent area-p suite | `node api/agent/test/area-p.mjs` | 0 | PASS area-p | 241 cases + 2 P7 validation groups + 1 P8 validation group + 1 budget traversal |
| Agent area-p9 events | `node api/agent/test/area-p9-events.mjs` | 0 | PASS area-p9-events | 13 scenarios pass |
| Agent area-p10 results | `node api/agent/test/area-p10-results.mjs` | 0 | PASS area-p10-results | 11 scenarios pass |
| Agent area-p11 storage | `node api/agent/test/area-p11-storage.mjs` | 0 | PASS area-p11-storage | 5/5 scenarios pass |
| Agent exec output | `node api/agent/test/exec_output.mjs` | 0 | `exec output: ok` | Streaming output verification pass |
| Agent trace suite | `node api/agent/test/trace.mjs <case>` | 0 | PASS trace (9 cases) | on, off, failure, layerfs-env, start, ids, no-content, store, daemon-host |
| Backend suite | `node api/backend/test/backend.mjs <case>` | 0 | PASS backend (4 cases) | surface, closed, comparable, get-log |
| Runtime suite | `node api/runtime/test/runtime.mjs <case>` | 0 | PASS runtime (14 cases) | surface, home-mode, projects, init, state, events, leftovers, sandboxes, volumes, transport-order, macos-worker, worker-frames, docker-default, worker-docker |
| CLI doc tables | `node api/cli/test/doc-tables.mjs` | 0 | PASS | Doc tables in sync with CLI commands |
| Whitespace & diff check | `git diff --check` | 0 | No errors | Clean diff formatting |

## Summary of deliverables

1. **Manifests**: `@layerfs/agent`, `@layerfs/backend`, `@layerfs/runtime`, `@layerfs/cli` all non-private, license `MIT`, repository `https://github.com/Ephemeral-AI-Lab/layerfs.git` with correct `directory` entries, exact `1.1.0` dependencies on `@layerfs/agent`.
2. **README and LICENSE**: Untracked README and LICENSE created for all four packages; README commands verified executable; accurate native loading rules documented (JS-only tarballs; Linux external `LAYERFS_NATIVE`; macOS/Windows external sealed runtime bundle).
3. **Workspace linkage**: Root `api/package.json` workspaces list `agent`, `backend`, `runtime`, and `cli`. Redundant `api/cli/package-lock.json` removed.
4. **Pre-publish verification script**: `api/scripts/prepare-publish.mjs` packs each package offline, verifies tarball file lists against source and manifest requirements (asserting no `.node` files are included), installs all four from tarballs into a temporary directory, imports main entrypoints, and executes installed `layerfs` CLI binary for `--version` and `help`.
5. **Release policy**: `docs/general/release-policy.md` documents coordinated single-version scheme for all four packages and specific breaking-change boundaries for each package.
6. **Publication command for later approval (NOT executed)**: `npm --prefix api publish --workspaces --access public`.

# LayerFS Area P Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a reproducible Linux build/test environment, project-local build MCP and skills, source recovery points, and a sealed E2E carrier for Node, Python, CLI and MCP before Area P product code.

**Architecture:** Windows hosts editing and the build MCP; a dedicated Ubuntu WSL2 distribution runs the repository's existing `tools/worktree.py`; Docker Desktop supplies pinned Linux build/runtime/sandbox images. The build MCP is a typed, allowlisted adapter over that launcher, while the launcher is extended in place to seal Python/MCP payloads and execute allowlisted Python clients.

**Tech Stack:** Ubuntu WSL2, Docker Engine 29/API 1.52, Rust 1.85.1 in Docker, Node 24, Python 3, official `@modelcontextprotocol/server` 2.x + Zod 4, existing Python worktree tooling, Node test runner.

**Spec:** `docs/superpowers/specs/2026-10-28-area-p-design.md`

## Global Constraints

- Commit and push only to private `Ephemeral-AI-Lab/layerfs-area-p` on `area-p/p1-p15`; never commit or push to upstream LayerFS.
- Baseline is `582bc786f9e17a6993a147cc165e3e4a75366a9e`; local tag `area-p/baseline-582bc786` and `E:/layerfs-connect/backups/layerfs-area-p-baseline-582bc786.bundle` are the recovery points.
- Use Ubuntu WSL2 distribution name `Ubuntu`, launcher user `root`, checkout `/mnt/e/layerfs-connect/layerfs`, and resource state under `/root/.cache/layerfs-worktrees`.
- Exact base tags are `rust:1.85.1-bookworm` and `node:24.21.0-bookworm`; registration uses resolved immutable IDs and never pulls implicitly.
- Builds are `--locked --offline` after explicit `prepare`; no third-party source patches or global Docker prune.
- Real Stores use Disposable/WAL/synchronous=OFF only and live in per-run Docker volumes on Linux; Durable is NOT_RUN.
- Every test invocation has an explicit timeout <=120 seconds; build tests first and treat timeout as failure.
- Do not create a pre-push gate, arbitrary-shell MCP tool, automatic retry, automatic rollback, or another build framework.
- Each commit records production LOC before/after/delta using `python3 -B tools/production_loc.py --json`; tools/docs/tests normally leave product LOC unchanged.

## Review Focus

- **Wrong repository:** a routine push must not target upstream LayerFS — Task 1 asserts remote names/URLs and disabled upstream push URL.
- **Windows Store backing:** a run must not put SQLite on `/mnt/e` — Tasks 1 and 4 inspect launcher resource/run receipts and Store volume mounts.
- **MCP command injection:** arguments such as `--workspace`, shell metacharacters or another worktree must be refused — Task 2 tests exact argv arrays without a shell.
- **Cross-client artifact drift:** a Python/MCP payload absent from or altered within a sealed bundle must fail closed — Task 3 tests namespace/hash verification.
- **False E2E success:** a timed-out, unreleased or wrong-image run must not report PASS — Task 4 checks exit code, status and `released` in each receipt.

---

### Task 1: Linux Environment and Recovery Baseline

**Files:**
- Create: `tools/docker/area-p-runtime.Dockerfile`
- Create: `tools/docker/README.md`
- Modify: `.gitignore` only if the runtime recipe generates a local file not already ignored

**Interfaces:**
- Produces: local image `layerfs-area-p-runtime:node24-python3`, a printed `resources.json` path, and verified `origin`/`upstream` remotes consumed by Tasks 2–4.

- [ ] **Step 1: Prove the current recovery state**

Run:
```bash
git -C E:/layerfs-connect/layerfs status --short --branch
git -C E:/layerfs-connect/layerfs remote -v
git -C E:/layerfs-connect/layerfs bundle verify E:/layerfs-connect/backups/layerfs-area-p-baseline-582bc786.bundle
```
Expected: branch `area-p/p1-p15`, bundle contains baseline tag, `origin` is the private Area P repository, `upstream` is public LayerFS. Then run `git -C E:/layerfs-connect/layerfs remote set-url --push upstream DISABLED_NO_UPSTREAM_PUSH` and confirm no upstream push URL remains usable.

- [ ] **Step 2: Install and qualify the dedicated WSL control plane**

Run `wsl.exe --install -d Ubuntu --no-launch`. If Windows requires restart or interactive setup, stop this task and report that exact blocker; do not alter Podman/Docker utility distributions. After installation:
```bash
wsl.exe -d Ubuntu -u root -- sh -lc 'apt-get update && apt-get install -y --no-install-recommends python3 lsof git ca-certificates && python3 --version && docker version && test -d /mnt/e/layerfs-connect/layerfs'
```
Expected: Python 3, Docker Engine API >=1.48, and the checkout visible at the pinned path.

- [ ] **Step 3: Write the runtime recipe and its documentation**

`area-p-runtime.Dockerfile` starts `FROM node:24.21.0-bookworm` and installs only `git`, `procps`, `lsof`, `python3`, `python3-venv`, and `python3-pip` with apt lists removed. `tools/docker/README.md` records exact build/pull commands, the no-Durable rule, and that FUSE capability comes from the product's Docker configuration rather than an unqualified image claim.

- [ ] **Step 4: Pull/build and pin all image identities**

Run in Ubuntu:
```bash
docker pull rust:1.85.1-bookworm
docker pull node:24.21.0-bookworm
docker build --file /mnt/e/layerfs-connect/layerfs/tools/docker/area-p-runtime.Dockerfile --tag layerfs-area-p-runtime:node24-python3 /mnt/e/layerfs-connect/layerfs/tools/docker
docker image inspect rust:1.85.1-bookworm node:24.21.0-bookworm layerfs-area-p-runtime:node24-python3 --format '{{.RepoTags}} {{.Id}}'
```
Expected: all images local; registration subsequently uses these exact resolved IDs.

- [ ] **Step 5: Register and prepare private build resources**

Run:
```bash
python3 /mnt/e/layerfs-connect/layerfs/tools/worktree.py register --worktree /mnt/e/layerfs-connect/layerfs --root /root/.cache/layerfs-worktrees --owner area-p --build-image rust:1.85.1-bookworm --runtime-image layerfs-area-p-runtime:node24-python3 --sandbox-image layerfs-area-p-runtime:node24-python3
python3 /mnt/e/layerfs-connect/layerfs/tools/worktree.py prepare --resources <printed-resources.json> --platform linux
```
Expected: a registration whose source is the pinned checkout, Linux Cargo/target volumes are private, dependencies are fetched only in `prepare`, and the printed resource path is recorded in `.omc/ultragoal/plans/area-p-20261028/ledger.jsonl` evidence.

- [ ] **Step 6: Run existing cheap baselines under explicit timeouts**

Run in Ubuntu with `timeout 100` for each test command:
```bash
cd /mnt/e/layerfs-connect/layerfs
timeout 100 python3 -B -m unittest tools.test_worktree
timeout 100 docker run --rm --pull never -v /mnt/e/layerfs-connect/layerfs:/work:ro -w /work layerfs-area-p-runtime:node24-python3 node api/agent/test/calls.mjs
timeout 100 docker run --rm --pull never -v /mnt/e/layerfs-connect/layerfs:/work:ro -w /work layerfs-area-p-runtime:node24-python3 node --test api/cli/test/cli.test.mjs
```
Expected: all exit 0 within 100 seconds; otherwise retain output and diagnose before product work.

- [ ] **Step 7: Commit the environment recipe**

Run the production LOC counter before staging and against the staged tree, then commit only Task 1 files with `build: add Area P Linux runtime recipe`, the LOC comparison and `Co-Authored-By: Claude Code <noreply@anthropic.com>`. Push is a separate user-approved operation subject to the OMC Git guardrail.

### Task 2: Project Build MCP and Build/Rollback Skills

**Files:**
- Create: `tools/build-mcp/package.json`
- Create: `tools/build-mcp/package-lock.json`
- Create: `tools/build-mcp/src/commands.js`
- Create: `tools/build-mcp/src/server.js`
- Create: `tools/build-mcp/test/commands.test.js`
- Create: `tools/build-mcp/test/stdio.test.js`
- Create: `.mcp.json`
- Create: `.claude/skills/layerfs-build/SKILL.md`
- Create: `.claude/skills/layerfs-rollback/SKILL.md`

**Interfaces:**
- Consumes: Task 1 `resources.json` and pinned Ubuntu/checkout.
- Produces: MCP tools `layerfs_status`, `layerfs_prepare`, `layerfs_build`, `layerfs_cargo_test`, and `layerfs_checkpoint`; `createOperations({platform, resources, runner})` is the injectable command boundary.

- [ ] **Step 1: Write failing command-validation tests**

Test `createOperations` with an injected runner. Assert exact argv for status/prepare/build/cargo test; assert build defaults to Linux debug with jobs 2; assert cargo tests require `-p` and a named `--test` target and use `--timeout 90`; assert `--workspace`, `--manifest-path`, `--target-dir`, `fs-bench-pro`, shell metacharacters, alternate resources paths and timeout >100 are refused before the runner is called. Assert Windows commands use `wsl.exe -d Ubuntu -u root --cd /mnt/e/layerfs-connect/layerfs` and never a shell command string.

- [ ] **Step 2: Run the command tests and confirm failure**

Run `node --test tools/build-mcp/test/commands.test.js` under a 100-second parent timeout. Expected: FAIL because `src/commands.js` does not exist.

- [ ] **Step 3: Implement the typed command adapter**

`commands.js` exports `createOperations({platform = process.platform, resources, runner})`. Each operation returns `{status, stdout, stderr, receipt?}` from an injected spawn runner; no `exec`, shell string, arbitrary cwd, retry, Docker cleanup or Git mutation. `layerfs_checkpoint` runs only read-only Git commands and writes a timestamped binary diff plus status/revision manifest beneath `E:/layerfs-connect/backups` (or its Ubuntu-mounted equivalent), reporting untracked files separately rather than pretending they are in the diff.

- [ ] **Step 4: Implement and test the stdio MCP surface**

Use official `@modelcontextprotocol/server` 2.x and `zod` 4. `server.js` registers the five tools with strict schemas and uses `serveStdio`; stdout is protocol only and logs use stderr. `stdio.test.js` spawns the server, performs `initialize` and `tools/list`, asserts exactly the five tools, invokes invalid input and expects a tool error without a spawned build. Pin dependencies in the lockfile.

- [ ] **Step 5: Configure the MCP and write both skills**

`.mcp.json` starts the server with Windows Node and supplies `LAYERFS_BUILD_RESOURCES=/root/.cache/layerfs-worktrees/<registered-path>/resources.json` using the actual Task 1 path. The build skill documents Ubuntu execution, build-before-test, Disposable-only, timeout and receipt rules. The rollback skill documents baseline bundle verification, non-destructive checkpoints, exact-target inspection, and that restore/reset/clean/teardown require explicit approval.

- [ ] **Step 6: Run MCP and documentation checks**

Run both Node test files with `node --test` under a 100-second parent timeout. Run `git diff --check` and validate `.mcp.json`/package JSON parsing. Expected: PASS, five tools listed, no protocol logs on stdout.

- [ ] **Step 7: Commit the MCP and skills**

Record production LOC before/after and commit Task 2 files as `tools: add Area P build MCP and recovery skills` with the required attribution.

### Task 3: Seal Python and MCP E2E Payloads

**Files:**
- Modify: `tools/worktree.py:15-52`
- Modify: `tools/worktree_build.py:136-196`
- Modify: `tools/worktree_run.py:78-183`
- Modify: `tools/test_worktree.py` in the existing copy/verify/run test groups

**Interfaces:**
- Consumes: existing `copy_sdk(root, bundle)`, `verify_bundle(path)`, and launcher `arguments`.
- Produces: optional `api/python` and `api/mcp` payload copies when those directories exist, and `run` accepting only `node` or `python3` with a relative script contained in the sealed bundle.

- [ ] **Step 1: Add failing bundle and command tests**

Extend existing tests to create temporary `api/python` and `api/mcp` source trees. Assert their files are copied, included in the hash namespace, reject symlinks/path escape and fail verification when altered. Parameterize command validation: `node api/agent/test/...` and `python3 api/python/test/...` are accepted; absolute paths, `..`, `/bin/sh`, `python`, environment assignments and arbitrary host scripts are refused.

- [ ] **Step 2: Run the selected tests and confirm failure**

Run `timeout 100 python3 -B -m unittest tools.test_worktree` in Ubuntu. Expected: only the new payload/command cases FAIL; record the exact failures.

- [ ] **Step 3: Implement minimal payload and command validation changes**

Keep the existing bundle version and hash/ownership model. Add the two optional source directories without shipping caches/targets/symlinks. Validate interpreter (`node` or `python3`), exactly one relative bundle-contained script plus arguments, and retain all existing image, resource, timeout and per-run ownership checks. Do not add a generic executable selector.

- [ ] **Step 4: Run launcher self-tests and guards**

Run:
```bash
timeout 100 python3 -B -m unittest tools.test_worktree
timeout 100 python3 -B -m unittest discover -s tools -p 'test_*.py'
```
Expected: PASS within each 100-second ceiling. Diagnose any unrelated pre-existing failure separately rather than weakening its assertion.

- [ ] **Step 5: Commit the launcher extension**

Record production LOC before/after and commit Task 3 files as `test: seal Python and MCP E2E payloads` with the required attribution.

### Task 4: Prove the Sealed Linux Build and Real E2E

**Files:**
- Create: `docs/superpowers/evidence/2026-10-28-area-p-foundation.md`
- Modify: `docs/general/parallel-worktrees.md` only to document the new Python/MCP payload and `python3` command support

**Interfaces:**
- Consumes: Tasks 1–3 resources, MCP operations and sealed bundle behavior.
- Produces: one sealed Linux daemon/addon/SDK bundle and passing real Store/Workspace receipts used as the Area P starting gate.

- [ ] **Step 1: Build the sealed Linux bundle**

Run `python3 tools/worktree.py build --resources <resources.json> --platform linux --profile debug --jobs 2` in Ubuntu. Expected: a sealed bundle with matching daemon, addon and SDK source identity; no test timeout is applied to compilation, but the build is not left unattended.

- [ ] **Step 2: Run the existing real Store scenario**

Run `python3 tools/worktree.py run --resources <resources.json> --bundle <sealed-bundle> --timeout 90 -- node api/agent/test/verify/calls_real.mjs inner store`. Expected: process exit 0, receipt `status=passed`, `released=true`, and output reports all Store calls passed.

- [ ] **Step 3: Run the existing real Workspace scenario**

Run the same launcher with `inner workspace`. Expected: exit 0, `status=passed`, `released=true`, and actual mount/Exec/Commit/unmount output. On failure preserve the exact stopped containers, volume and receipt for diagnosis; no automatic retry or teardown.

- [ ] **Step 4: Run CLI agreement against real artifacts**

Run `node api/cli/test/verify/cli_real.mjs inner store` and then `inner workspace`, each as a separate <=90-second launcher call. Expected: SDK and CLI answers agree and both receipts pass/release cleanly.

- [ ] **Step 5: Record the evidence and starting gate**

The evidence document records exact source/image/bundle IDs, commands, output summaries, receipt paths, Store profile and any gaps. Explicitly state Python/MCP real scenario rows are carrier-qualified only until their product packages exist; do not claim P4/P5 complete. Run `git diff --check`.

- [ ] **Step 6: Commit the foundation evidence**

Record production LOC before/after and commit Task 4 files as `docs: record Area P foundation verification` with the required attribution. Checkpoint ultragoal G002 only after all applicable steps pass or record the exact blocker.

## Execution Handoff

After this foundation, write separate implementation plans for (1) P1/P2/P6/P14 and the shared history contract, (2) P7–P13 engine/API features, (3) P3 CLI, (4) P4 Python, (5) P5 MCP, and (6) P15/final cross-client release. R7 remains an external blocker for push/pull and must be shown explicitly in the final matrix.

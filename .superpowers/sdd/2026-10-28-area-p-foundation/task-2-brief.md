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

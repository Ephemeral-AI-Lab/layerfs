# LayerFS Area P implementation design

Date: 2026-10-28
Status: Approved for implementation with the repository and publishing constraints below
Baseline: `582bc786f9e17a6993a147cc165e3e4a75366a9e` from upstream `main`
Code repository: private `Ephemeral-AI-Lab/layerfs-area-p`; upstream `Ephemeral-AI-Lab/layerfs` is read-only
Tracking: `layerfs-connect#14`, P1/P3–P15 sub-issues, and `layerfs#484` for P2

## 1. Intent

Make the LayerFS API usable consistently from Node, Python, the CLI and MCP without weakening the filesystem, history or execution contracts. The immediate foundation is a reproducible development environment on the current Windows host, with Linux builds and real Store/FUSE tests running through the existing Docker Desktop engine. Product work starts only after that foundation and this design are approved.

The intended final demonstration is a shared behavioral matrix: each client mounts a Workspace, executes a command, explicitly commits, reads history, and pushes or pulls. P4 requires Python parity with the Node SDK; CLI and MCP expose the operations named by P3/P5 plus any explicitly shared contracts. P6 tests the overlapping surface rather than inventing CLI/MCP operations that their issues do not request. A client that cannot demonstrate a row because an upstream dependency is unfinished is reported as blocked, never simulated or marked complete.

## 2. Core understanding that constrains the design

The dependency direction is `core < engine < api`:

- **Content** owns canonical object identity, complete-file and filesystem construction, bounded logical reads and merge algorithms. It opens no database and delegates object consumption/readback through ports (`core/crates/layerfs-content/src/lib.rs`).
- **Storage** owns authenticated encoding, packs, deduplicated Save/Read flows and physical locators. It accepts finalized Content objects; it does not construct filesystem history (`core/crates/layerfs-storage/src/lib.rs`).
- **History** owns checked Branch/Commit/Layer/LayerStack identities and records, expected-head publication, fork/merge metadata and bounded queries (`core/crates/layerfs-history/src/lib.rs`).
- **Telemetry** owns bounded parent/child operation timing, not a public per-call API by itself (`core/crates/layerfs-telemetry/src/lib.rs`).
- **Engine** owns SQLite Persistence, Project/Store composition, Workspace/Overlay, FUSE, daemon, Sandbox, the public Rust `layerfs-api` surface and the Node addon.
- **API** contains the JavaScript packages only. The Node addon loads engine code; Python, CLI and MCP must reuse public behavior rather than create another Store implementation.

A Store is one global SQLite database holding immutable content-addressed objects and mutable Branch history. A Branch is a writable sequence of Commits; publishing to main adds a Layer. A Workspace is a complete writable view over an immutable committed root plus its own mutable overlay. Mount does not copy the whole tree; Commit captures the current Workspace-wide frontier, not merely files written by one registered Exec. Exec never implicitly commits or unmounts.

The current contract gives each mounted Workspace its own overlay SQLite file, copied from a prepared template at mount and deleted at terminal unmount. Older documents that describe one overlay per daemon or no Workspace database are historical where they conflict with current `AGENTS.md` and source.

Store calls run directly in the caller's process on Linux. Windows and macOS use the authenticated loopback worker route. Workspace calls go to the daemon; Store calls do not. No retired `layerfs-server`, second database, or host-mediated Linux Store route will be introduced.

All execution uses Disposable/WAL/synchronous=OFF only. Durable execution remains `NOT_RUN`. Operations are attempted once: no automatic retry, busy replay or guessed recovery after an uncertain result.

## 3. Goals

1. Establish a clean, reproducible Linux-first build and test environment before product code.
2. Add project-local build/rollback MCP and skill entrypoints that wrap existing ownership and timeout mechanisms rather than replacing them.
3. Implement P1–P15 through existing core/engine/API boundaries, sharing behavior and contract fixtures across clients.
4. Verify both cheap stand-in/contract behavior and real Linux Store/Docker/FUSE behavior.
5. Preserve exact evidence for unsupported, blocked and failed rows.
6. Prepare npm packages for P15, with actual publication separately approved.

## 4. Non-goals and anti-goals

- No PostgreSQL Store, remote object storage, cloud sandbox or implementation of Area R beyond what P3 explicitly consumes.
- No native Windows Store or FUSE implementation and no new general WSL distribution requirement for the initial environment.
- No standalone UI or demo bundle. No commit or push to upstream `Ephemeral-AI-Lab/layerfs`; all code commits and pushes target the private Area P repository. npm/PyPI publication occurs only after the relevant quality gates.
- No new abstraction that merely duplicates `tools/worktree.py`; no unrestricted shell exposed as a build MCP tool.
- No production fake clocks, test hooks in `src/`, inline tests, weakened assertions or automatic retries.
- No claim that CI green alone verifies a change; CI reports and does not gate.
- No silent implementation of unresolved D10/D11 or open R7 behavior under another name.

## 5. Development environment decision

### Recommended topology

Use **Windows as the editor, a dedicated Ubuntu WSL2 distribution as the orchestration host, and Docker Desktop Linux as the authoritative build/E2E host**. The existing launcher calls `os.uname()` and therefore cannot run under Windows Python; a dedicated WSL control plane also preserves the same Linux paths for sibling Docker containers.

- Windows supplies Git editing and the Claude/MCP host. Ubuntu WSL2 supplies Python 3, `lsof`, Docker CLI integration and a Linux home for launcher state.
- The exact `rust:1.85.1-bookworm` build image supplies the repository's required Rust toolchain. Neither Windows nor WSL needs a host Rust installation. Pulling this image is authorized as part of the requested environment setup.
- The exact `node:24.21.0-bookworm` image is the initial runtime and Sandbox base. A small Area P runtime derivative adds Python and test utilities for cross-client evidence; all three images are registered by immutable local image ID. The host's Node 25 is adequate for simple tooling but is not the compatibility baseline.
- The initial Sandbox uses the same Node 24 userland with the product test's existing Docker/FUSE capabilities. If the real scenario proves a missing userland requirement, the Area P image recipe records that requirement; no unqualified substitute image is selected silently.
- `tools/worktree.py` runs inside Ubuntu WSL2 and owns per-checkout Cargo homes/targets, sealed daemon+addon bundles, per-run Store volumes, container prefixes and retained receipts. Linux builds remain offline and locked after the explicit prepare step. Writable Store/build state stays on Linux ext4; the `/mnt/e` checkout is source input, not Store backing.
- Linux is the required platform for real Store and FUSE E2E. Windows worker compatibility is a later scoped check, not a substitute for Linux evidence.

This fits the repository better than modifying the existing Podman/Docker utility WSL distributions or adding an unsupported Windows launcher path. Docker Engine API 1.52 already satisfies the SDK's minimum. It also avoids a second native Windows build path that the product does not support.

### Bootstrap sequence

1. Verify the clean baseline and required Docker resources.
2. Pull/build the exact approved images and record immutable IDs.
3. Register this checkout with `tools/worktree.py` using an Area P owner and private state root outside the checkout.
4. Run the explicit network-enabled `prepare`, then locked/offline builds.
5. Run existing stand-in Node/CLI tests and tool self-tests.
6. Build the daemon/addon bundle and run the smallest real Store scenario, then the Workspace scenario.
7. Extend the sealed bundle/launcher, with owning tests, to carry the Python package and MCP server and to run allowlisted Python as well as Node client entrypoints. The cross-client runtime image includes both runtimes; this is infrastructure work, not a second E2E framework.
8. Only then start Area P product changes.

Every test invocation has an explicit wall timeout no greater than 120 seconds and builds first where applicable. A timeout is a failed/hung test, not a reason to wait indefinitely or rerun unchanged.

## 6. Build MCP and skills

Add a **project-local, stdio build MCP server** for this checkout, configured locally rather than globally. It will expose a small typed command surface around the existing launcher:

- inspect environment and current resource registration;
- prepare/build through the pinned resources;
- run an allowlisted package test or sealed E2E entrypoint with a bounded timeout;
- report sealed bundle/run receipts and retained failures;
- create a non-destructive source checkpoint and show rollback instructions.

It will not expose arbitrary shell execution, global Docker prune, another worktree's resources, dependency cache deletion, automatic retry, Git reset/clean, push or npm publish. Before implementation, consult the official MCP SDK documentation and use the maintained SDK rather than hand-rolling JSON-RPC.

Add project-local skills for:

- **LayerFS build/test:** platform routing, image/resource setup, build-before-test, timeout and evidence rules;
- **LayerFS rollback/recovery:** baseline verification, patch/checkpoint creation, retained Docker failure custody and explicit rollback commands.

The skills call the same existing launcher/MCP operations; they do not maintain a second build algorithm. If the MCP wrapper cannot preserve a launcher guarantee, that operation remains CLI-only rather than weakening the guarantee.

## 7. Rollback and source custody

The initial Git baseline is `582bc786f9e17a6993a147cc165e3e4a75366a9e`. The clone is shallow because the initial full clone failed in `index-pack` with an allocation error; the shallow clone completed successfully. This does not affect source verification, but no claim about full local history is made.

Before product edits:

- create an Area P feature branch without changing the baseline `main`;
- create a local baseline tag and an external Git bundle for recovery;
- push baseline and feature branches only to the private Area P `origin`; the public LayerFS remote is named `upstream` and has pushing disabled locally;
- keep generated Cargo/Docker state outside the source tree or in the repository's designated target path;
- save rollback instructions and the exact baseline outside the repository as well as in the ultragoal ledger.

No destructive rollback runs automatically. Any `git restore`, `git reset`, `git clean`, retained volume removal or failed-run teardown requires inspecting the exact target and explicit approval. The user has authorized commits and pushes to the private Area P repository and final package publication after verification. Each commit message must include the repository's reproducible production LOC before/after/delta calculation and the required Claude Code attribution. No commit or push is made to upstream LayerFS.

## 8. D10 and D11 recommendations

These are selected implementation routes under the user's approved plan; recording them as global owner rulings in D7 remains outside this assignment.

### D10 — Python SDK

Choose a **native Python binding over the public Rust API on Linux**, with the existing worker transport on macOS and Windows. The initial packaging target is a manylinux x86_64 wheel for Python 3.10–3.13, using an ABI3 subset if the binding can remain within it; local source/wheel tests come before any additional architecture. P4 includes the PyPI wheel requirement, while P15 remains the npm publication task.

A worker-only Linux SDK would move Store calls out of the caller's process and contradict the current Linux-only direct Store route. PyO3 is the likely implementation choice and is authorized as a new locked dependency if no existing crate supplies the binding; maturin may be used as build tooling after its official packaging documentation is checked. The macOS/Windows client implements the existing authenticated length-prefixed JSON worker frame protocol rather than inventing HTTP. Linux wheels and worker-client behavior share one contract suite, not two semantic implementations.

### D11 — MCP server

Choose **Node SDK + stdio first**.

Node already implements the public client and worker transports. Stdio fits local agent integration, avoids a second HTTP authentication/authorization surface, and allows P5 to reuse P1/P2 directly. HTTP is outside the initial scope unless a concrete remote-MCP requirement is approved later.

## 9. Implementation grouping and dependency order

### Foundation and shared contracts

Before each P1/P7–P14 coding slice, write its exact public method/argument/result/error/paging contract beside the P6 cases and have it independently reviewed. The existing one-line issues do not provide those details, so the builder may choose names that follow current SDK conventions; the reviewed contract, not an improvised implementation, freezes the fixture. This does not require a separate user approval round for routine naming.

- **P6** defines language-neutral cases, normalization and refusal assertions. Start with existing `api/agent/test/fixture/cases.mjs` and the SDK/CLI agreement tests; extend rather than creating an unrelated fixture model. Python receives the corresponding Node SDK calls; CLI/MCP contract coverage is limited to their documented operation surface plus the final five-row matrix.
- **P2** resolves all constructor/open/init options once, with environment fallback and no later environment rereads. It is independent of the history/storage work and can proceed in parallel after the foundation.
- **P14** first reproduces short `list_branches` pages and the Branch-name loss past 64, then fixes the owning Store/query path without returning misleading partial names.

### History and read surface

- **P1** adds the Node history surface from existing History/engine primitives: graph, Layers, merge parents, fork points, publish state, Branch-only commits and changed paths.
- **P7** threads author/agent and free labels through Commit writes and history reads, preserving Store format compatibility rules and exact unchanged-history behavior.
- **P8** exposes list/stat/read at a Commit or Layer with explicit binary handling; it must not mount a Workspace or mutate the Branch head.
- **P3** exposes P1 history and R7 push/pull through the CLI. History can proceed after P1; push/pull completion remains blocked while R7 (#42) and its R2/R3 prerequisites are open.

### Observation and scoping

- **P9** starts as a live typed stream with explicit connection-local ordering, cursor and disconnect semantics; it adds no persisted event log or Store schema change. A reconnect receives current state according to the reviewed contract and is not promised historical replay. Store-originated and daemon-originated events retain their source identity. If cross-reconnect durable replay is later required, it becomes a separately reviewed Store format change.
- **P10** maps existing invocation outcomes and telemetry into method, duration, error code and write results without adding fake clocks or changing operation semantics.
- **P11** extends `get_storage` with disk/WAL, raw/unique/compressed and per-Commit-kind byte accounting.
- **P12** provides cheap project-list summary data: size, Branch/Commit counts, main Layer and last Commit.
- **P13** is an option/wrapper on the existing client, mirrored by Python where applicable. It maps direct file paths beneath one project folder and Branch names beneath one namespace, and selects the scoped Branch for Workspace calls. A mounted Workspace still exposes the complete tree; scoped Exec starts in the configured folder, but LayerFS does not pretend that the mount is a filtered filesystem. Unscoped clients retain their existing behavior.

### Additional clients and release

- **P4** follows the approved D10 route and reuses P6.
- **P5** follows the approved D11 route and exposes files, Bash, history and available push/pull operations with explicit lifecycle cleanup.
- **P15** removes private/file-link publication blockers as required, validates package contents and versions, and performs npm publication only after separate approval.

Each implementation step updates the owning API/architecture documentation and adds cheap external tests before broad integration. New production files obey the 999-line ceiling; `lib.rs`/`mod.rs` obey the 200-line delegation ceiling.

## 10. Verification strategy

### Per-change checks

Select only the owning scope, but include all affected layers:

- Rust: format, locked build, warning-denying Clippy, external tests and boundary/fuser guards where applicable.
- Node/CLI: existing stand-in case tests, CLI/SDK agreement, packaging/trace/exec-output and new contract cases.
- Python: native unit/contract tests, wheel contents/import and worker transport tests according to platform.
- MCP: protocol initialize/list/call tests, argument/error mapping, stdio lifecycle and real client contract tests.
- Tools/MCP/skills: owning tool self-tests, allowlist/refusal tests, timeout/cancellation and receipt validation.

### Real Linux evidence

Use fresh isolated resources for each run. Run the existing real Store and Workspace scenarios before feature work and the affected scenarios after changes. The final matrix covers:

| Capability | Node | Python | CLI | MCP |
| --- | --- | --- | --- | --- |
| Mount | real Linux Workspace | real Linux Workspace | real Linux Workspace | real Linux Workspace through MCP |
| Run command | exit/stdout contract | same contract | same contract | same contract |
| Explicit commit | resulting Commit/history identity | same | same | same |
| Read history | graph/metadata contract | same | same | same |
| Push or pull | actual R7-backed result or explicitly blocked | same | same | same |

Unmount and client/server cleanup occur in `finally`/equivalent paths. Failed runs retain their exact containers, volumes, traces and receipts until diagnosed and explicitly released.

### Final quality gate

No story is complete on implementation alone. Final completion requires:

1. all applicable changed-scope checks passing;
2. independent code review with findings resolved;
3. ai-slop-cleaner and simplification pass without weakening behavior;
4. the cross-client evidence matrix and explicit blockers/gaps;
5. a clean final verifier report.

## 11. Resolved decisions and remaining external blocker

- Code lands only in private `Ephemeral-AI-Lab/layerfs-area-p`; upstream LayerFS receives no commits or pushes.
- Windows editing with a dedicated Ubuntu WSL2 control plane and Docker-Linux builds/real E2E; the exact base tags are `rust:1.85.1-bookworm` and `node:24.21.0-bookworm`, with immutable IDs recorded after pulling/building.
- D10 implementation route: native Rust-backed Python on Linux plus the existing framed worker protocol on macOS/Windows. Initial wheel target is manylinux x86_64, Python 3.10–3.13; P4 includes PyPI, P15 covers npm.
- D11 implementation route: Node SDK with stdio MCP first, using the official MCP TypeScript SDK.
- The launcher/bundle may be extended with tested Python/MCP payload and allowlisted Python execution so all client rows produce real Linux evidence.
- P9 is live-only without Store schema changes; P13 scopes direct paths, Branch names and Exec working directory without filtering the mounted tree.
- R7 (#42) and its prerequisites remain outside Area P. Implement P3/P5 history and other unblocked calls, but keep push/pull and the corresponding final matrix cells explicitly blocked unless the user later expands the assignment. No fake transport or partial success claim is allowed.
- The user authorized commits/pushes to the private Area P repository and final publication after quality gates. The OMC unattended Git guardrail may still require the user to execute push commands interactively; it will not be bypassed.

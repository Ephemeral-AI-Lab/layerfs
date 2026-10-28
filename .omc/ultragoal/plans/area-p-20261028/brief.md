# LayerFS Area P (P1–P15) implementation brief

Date: 2026-10-28
Source: Ephemeral-AI-Lab/layerfs-connect#14 and its sub-issues; P2 is Ephemeral-AI-Lab/layerfs#484.
Code repository: E:/layerfs-connect/layerfs
Baseline: main at 582bc786f9e17a6993a147cc165e3e4a75366a9e (shallow clone after full-clone index-pack OOM).

## Objective

Build Area P without weakening LayerFS's core contracts. Before product code, establish a reproducible Linux-first build/test environment on the current Windows + Docker Desktop host, dedicated project-local build MCP/skills where they add value beyond the existing tools/worktree.py launcher, and a clean Git rollback/checkpoint procedure. Verify changed scope directly; CI is informational, not a completion gate.

## Required outcome

Each of Node, Python, CLI and MCP can mount, run a command, commit, read history, and push or pull, subject to explicitly resolved upstream dependencies. All work uses Disposable/WAL/synchronous=OFF only; Durable execution remains disabled. Linux owns real Store/FUSE E2E; Windows uses the existing worker route. No retired server or host-mediated Store route.

## Task inventory

- P1 (#53): Node SDK history calls — commit graph with Layers, merge parents, fork points, publish state, branch commits to fork point, changed paths per commit.
- P2 (layerfs#484): explicit JS constructor/open/init configuration with environment fallback and lifetime-stable resolution.
- P3 (#54): CLI history, push and pull; blocked by P1 and R7 (#42), which is open and itself blocked by R2/R3.
- P4 (#55): Python SDK; blocked by D10 (#20), native Rust binding versus worker-only client.
- P5 (#56): MCP server; blocked by D11 (#21), Node versus Python and stdio versus HTTP.
- P6 (#57): contract tests shared by Node, Python, CLI and MCP.
- P7 (#58): commit author/agent and labels.
- P8 (#59): list/stat/read at any Commit or Layer, including binary flag.
- P9 (#60): event stream for commits, branches, workspaces, sandboxes and init progress.
- P10 (#61): per-call results API with method, duration, error code and write outcomes.
- P11 (#62): Store statistics building on get_storage.
- P12 (#63): Store summary for project lists.
- P13 (#64): folder-scoped client with isolated folder and Branch namespace.
- P14 (#65): fix list_branches short pages and list_workspaces Branch names beyond 64 (layerfs#480).
- P15 (#66): publish @layerfs/* packages to npm; outward-facing release requires separate explicit approval before publishing.

## Execution order and gates

1. Complete read-only recon and obtain approval for the full design, including D10/D11 choices or explicitly defer blocked implementation.
2. Pin the clean baseline and create the reproducible build/test/rollback tooling. Prove stand-in tests and the smallest real Linux Store/Workspace smoke path before feature work.
3. Implement shared Rust/API primitives and contract fixtures before duplicating behavior across SDKs.
4. Implement Node, Python, CLI and MCP in dependency order. Do not claim P3 push/pull complete while R7 is unavailable.
5. Run scoped tests with explicit <=120-second test ceilings, build before test, preserve failure evidence, and run real Linux E2E through isolated resources.
6. Finish with independent review, ai-slop-cleaner, full changed-scope verification, and the cross-client mount/exec/commit/history/push-or-pull evidence matrix. Publishing is not implied by local completion.

## Constraints

- Follow AGENTS.md plus core/engine/api guides and preserve core < engine < api layering.
- No new dependency where an existing crate/package provides the capability.
- One attempted operation; no automatic retry/replay.
- No automatic commits, pushes, issue edits, or npm publication without explicit user request.
- Preserve unrelated files and existing Docker/WSL state; use per-worktree caches and per-run Store/container ownership.

LayerFS workspaces for every agent [Magpie](https://usemagpie.ai) connects. Instead of each agent copying a project tree to work on it, agents share one durable, deduplicated history and work through stable filesystem paths: forking a branch for a parallel agent copies no file bytes, a checkpoint stores only what changed, and any agent can resolve, read and build on what the others committed.

## What it adds

Eight tools, served on Magpie's MCP server as `layerfs_` plus the tool name:

- `provision` — gives the agent a workspace for a project branch and answers the path to work in; the first provision imports the source directory the user chose, later ones answer the same path without copying the tree again.
- `checkpoint` — saves the workspace's state on its branch, storing only the blobs the store has not seen, and reports added / modified / deleted counts plus the byte accounting.
- `fork` — forks a branch for a parallel agent: a pointer at an existing checkpoint, ready immediately, 0 bytes copied.
- `publish` — publishes a branch's head as main's new Layer, guarded so a publish can only move main forward; `force: true` overwrites deliberately and reports what it replaced.
- `exec` — runs a shell command in a branch's workspace and checkpoints whatever it changed as one commit. Disabled by default; the user opts in.
- `history` — lists a branch's checkpoints, newest first, with each one's logical tree size and the bytes it actually stored.
- `resolve` — resolves a project, branch or file to a real filesystem path, its checkpoint and its content hash. An O(1) lookup, no copying.
- `status` — the backend in use, the projects and branches held, and the storage accounting.

The package also declares a Magpie page, LayerFS trajectory, which replays how the agents' worktrees grew: main as the green middle bough, each branch a bough of its own, a checkpoint a badge bearing its agent's own icon, a fork a split and a publish a graft back onto main. Since 0.8.0 the page replays the real trajectory, refreshed on each commit, not the bundled sample.

## How it works

Two backends implement the same model. The native backend is the real LayerFS through its JavaScript SDK, `@layerfs/agent`: every SDK call happens in a child process shipped in the package, so the plugin host never imports the SDK, and a checkpoint lands as one `fs.apply` commit in the engine's own store, with the engine's chunk-level dedup. The sim backend is a local content-addressed store (immutable blobs named by SHA-256, branches as manifest pointers) that runs anywhere; it is the default, and it is what the Magpie community registry package ships.

On a 184-file, 26.8 MB project with 6 parallel agents keeping 3 checkpoints each (sim backend): forking one more agent takes about 1 ms and copies 0 bytes (a plain copy takes 321 ms and 26.8 MB), and 18 checkpoints holding 509.4 MB of logical trees occupy 27.2 MB of store — an 18.7× dedup.

![test run](https://raw.githubusercontent.com/Mola-maker/magpie-layerfs/29b52fb98fc2e9c20fb9e5c3c85a05f80cd01754/docs/assets/tests.png)
![benchmark](https://raw.githubusercontent.com/Mola-maker/magpie-layerfs/29b52fb98fc2e9c20fb9e5c3c85a05f80cd01754/docs/assets/benchmark.png)

## You need

- Magpie, to host the plugin and serve its tools.
- For the native backend: Linux, a LayerFS checkout at the pinned commit with its native binding built (the install prompt covers the build; it needs a Rust toolchain), and Bun or Node to run the plugin's test suite.
- Nothing extra for the sim backend.

Safety defaults are set for a host that serves tools to any caller holding a gateway key: `provision` requires an explicit source directory and refuses Magpie's config directory, the credential stores and the plugin's own store as sources; secret-shaped files never enter the store from an import; the store is private (0700 directories, 0600 records); `exec` and the native backend are both opt-in.

## Limits

- Verified on Linux only. macOS and Windows are not tested by us.
- Binary files cannot round-trip through the LayerFS fs surface: writes accept bytes, but `fs.read` refuses binary content (upstream issue #417), so checkouts and checkpoints name the binary files they cannot move instead of dropping them.
- The native backend is off by default and needs `LAYERFS_SDK_PATH` set; the sim backend is what runs without it.
- The same plugin is listed in the Magpie community registry (`@magpie-community/tools-layerfs`, merged in PR #318) as a sim-only package at v0.7.1; the native backend ships from this repository.
- `LayerFS.init` refuses a source tree on overlayfs, and its error there is opaque; import from a regular filesystem.

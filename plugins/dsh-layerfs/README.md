`dsh-layerfs` is a plugin bundle for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (dsh). It changes no dsh code.

Agents in a dsh session work on a LayerFS project from bash, through the `layerfs` command or short Node scripts that use `@layerfs/agent`. They fork Branches, test them in Sandboxes, merge and publish, and the conversation gets **Work** and **History** tabs that draw what each agent did, subagents included.

![Wayfare's History at the end of a recorded run: main after 147 Commits, with each lane's Branches forking and merging around it](https://raw.githubusercontent.com/Ephemeral-AI-Lab/dsh-layerfs/baac1b92b97f0bd492002c285f6a13401b4f4c0b/demo/wayfare/assets/history-tree.png)

## What it adds

- **Minimalist preset.** "LayerFS × DSH minimalist": bash with background jobs, subagents and compaction, and no read, write or edit tools. Its persona carries an SDK cheat-sheet and the working rules: one Workspace per agent, only the main agent publishes, unmount every Workspace.
- **Project chip.** Beside the preset chip when you start a session: pick a project, start an empty one, or import a folder as a new project.
- **Work and History tabs.** Beside Chat and Trajectory: the agents' terminals, Branches and Sandboxes, played in step with the run; and the project's History, Files and Storage.
- **LayerFS page.** In the sidebar, one tab per project: History and Files side by side, a new session on the project, and Remove.
- **Scripted demo, on by default.** The LayerFS scripted model plays ten scripted agent runs (Tour, Parallel Branches, Conflicts, 5 agents, Shop, Depot, Crew and more) through the real dsh agent loop, with no language model or API key. Run them from the LayerFS page's Tests tab; turn `@dsh-layerfs/mock` off on dsh's Plugins page to hide it.

## How it works

dsh session (agents, subagents, bash) → `layerfs-host` (bash environment, records) → the `layerfs` command or `@layerfs/agent` → Store and Sandboxes.

Each finished LayerFS call becomes a record in the calling agent's own session, and the client plugin reads those records back to draw the tabs. On Linux the Host and the agents' scripts open the Store in their own process. On macOS a long-lived Linux container, the LayerFS home's worker, runs them; on Windows a worker in WSL2 does. Both come from `@layerfs/runtime`.

## Two recorded runs

| Run | Time | History | Store |
|---|---|---|---|
| Wayfare, a trip planner in Node 24 | 56 min, 8 agents | 147 Commits, 31 Branches | 1.21 MB on disk for 91.5 MB of snapshots, 27× dedupe |
| Quarry, a SQL database in Rust | 4 h 16 min, 8 agents | 913 Commits, 56 Branches | 8.90 GB on disk for 4.93 TB of snapshots, 166× dedupe |

Both runs, the video and a first-session tutorial are on the [LayerFS × DSH page](https://layerfs.ai/docs/integrations/dsh/).

## You need

- Node 22.19 or newer, and pnpm
- A built deepseek-harness checkout
- Docker, for Live projects: Docker Desktop on macOS, a Docker Engine inside WSL2 on Windows
- Network access for the first LayerFS build

## Limits

- Checked live in dsh web on macOS and Windows (WSL2); Linux has no live run yet.
- Only dsh web is checked. dsh desktop serves the same web client but was not tested.
- A session's project is fixed at its first bash call that uses it.
- Store calls need Linux and a local filesystem; on macOS and Windows the worker provides them. See [Platform and limits](https://layerfs.ai/docs/platform/).

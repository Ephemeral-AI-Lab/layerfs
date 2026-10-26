## What it adds

Mainline lets coding agents read and record the engineering judgment behind code:
goals, decisions, rejected approaches, validation and the Git commits carrying the
work. This adapter brings that existing workflow into LayerFS Workspaces.

- `mainline-layerfs run` executes Mainline in the task's Linux Workspace and returns
  its original output, exit code and LayerFS checkpoint.
- `mainline-layerfs launch` keeps a macOS or Windows worker alive while the agent
  runs, and supplies `LAYERFS_WORKER_ENDPOINT` to the child.
- Offline installation includes Mainline's skill, the integration skill, the pinned
  LayerFS SDK skill with its references, and the official Bash `layerfs` command.

## How it works

Each run opens the existing project, mounts the supplied task Branch in a known
Sandbox, executes one Mainline command, checkpoints the Workspace and attempts to
unmount it. The complete `.git` directory and `.ml-cache` travel with the task tree,
so Git refs, notes and local drafts survive the next fresh Workspace.

Mainline continues to use its existing CLI and Git storage format. Agents explicitly
choose when to start, append, commit, seal and publish, following Mainline's skill
and the user's authorization. A completed command's changes are checkpointed even
when its exit code is nonzero. A LayerFS refusal preserves its original error and
any cleanup errors; no operation is automatically retried or replayed.

Linux Store calls run in the calling process with its local Store mount. macOS and
Windows calls use the existing host's worker endpoint or the included runtime
launcher. On Windows the launcher owns the worker until the agent child exits.

## You need

- Node 20+, npm 10+ and Git; Go 1.22+ to run the default integration test.
- The pinned LayerFS checkout beside the Mainline checkout. The plugin lives under
  `plugins/layerfs/`; its install scripts verify exact source identity and one copy
  of the agent SDK without changing the LayerFS checkout.
- A configured LayerFS host, project and running Sandbox. The Sandbox image needs
  Linux Mainline, Git, Bash, coreutils and findutils. The plugin provides an example
  Dockerfile for extending an existing LayerFS image.
- A complete standalone Git repository in the project, a task Branch and a distinct
  Mainline actor identity for each independent agent. The adapter rejects external
  `.git` pointers, alternate object stores and symlinks inside Git metadata.

The pinned [Install instructions](https://github.com/mainline-org/mainline/blob/2c146816d4bc0d88c85138ed8f63bdcb128f6d38/plugins/layerfs/README.md#install)
cover dependencies, skills, worker settings, commands and the optional real-host
smoke test.

## Verification

The preview's 29 adapter tests pass without Docker, using public-client stand-ins,
real subprocesses and a freshly built Mainline CLI in temporary Git repositories.
They cover literal arguments and stdin, refusals and cleanup, worker routing and
lifetime, skill distribution, and draft/seal/actor-ref/notes persistence across new
Workspaces. Mainline's existing quick tests and hygiene checks also pass. The
[Node 20 CI run](https://github.com/mainline-org/mainline/actions/runs/40207555942)
checks the published plugin commit.

Real LayerFS Docker/FUSE execution and live workers were not exercised for this
preview: the build environment lacked a configured bundle, Store and worker
endpoint. Platform routing and worker ownership were checked with stand-ins.

## Limits

- One agent owns each task Branch and executes its commands sequentially. Independent
  LayerFS Branches do not participate in Mainline's linked-Git-worktree discovery.
- Shared sealed intents use Mainline's existing Git remote and refs/notes sync;
  unshared drafts remain in their task Branch. Combine code through the Git workflow;
  last-writer-wins merging of `.git` can lose refs or corrupt metadata.
- Inputs are limited to 64 KiB; each Mainline execution has a 110-second timeout
  and five-second termination grace. `--cwd` is rejected to retain the project root.
- A refused cleanup can leave a Workspace mounted for inspection. `Conflict`,
  `Busy`, `NoSuchSandbox`, `Capacity` and uncertain outcomes require caller judgment;
  the adapter does not force-commit, discard, restart a worker or replace a Sandbox.

Install Mainline for LayerFS {{version}} for me.

1. Check for Node 20+, npm 10+, Git and Go 1.22+. Choose two fresh sibling folders:
   `mainline` and `layerfs`. Preserve existing checkouts and local changes; if those
   names are already occupied, inspect them and ask before changing their contents.
2. Clone {{repository}} as `mainline` and check out exactly {{commit}}, detached.
   Clone https://github.com/Ephemeral-AI-Lab/layerfs as the sibling `layerfs` and check
   out exactly {{layerfs.commit}} ({{layerfs.version}}), detached. Verify both HEADs,
   that Mainline's `layerfs.json` contains the same LayerFS SHA, and both checkouts
   are clean. Do not substitute branches, latest releases or different commits.
3. Read `mainline/plugins/layerfs/README.md`, especially Install, host preparation,
   settings, refusal handling and the verification limits. Read the pinned LayerFS
   `api/README.md`, `api/how-to-use/agent.md`, `api/how-to-use/runtime.md`, and the
   plugin's `skills/mainline-layerfs/SKILL.md` and `skills/layerfs-js-sdk/SKILL.md`.
4. From the Mainline root run `npm --prefix plugins/layerfs ci --offline`. Its hooks
   must verify the pinned clean LayerFS checkout, the installed package sources and
   one shared agent SDK. Leave the LayerFS checkout unchanged. Run
   `node plugins/layerfs/bin/mainline-layerfs --help` and the installed official
   `layerfs` command's help. On Unix add `plugins/layerfs/bin` and
   `plugins/layerfs/node_modules/.bin` to this agent session's PATH; on Windows use
   Node for the adapter entry point and the official CLI's `.cmd` shim.
5. Install the three bundled skills into a fresh directory the agent can load:
   `npm --prefix plugins/layerfs run install-skills -- /absolute/path/to/fresh-skills`.
   Choose that path from the agent's actual configuration; preserve existing skills.
   Read the installed Mainline skill and follow its context and authorization rules.
6. Run `npm --prefix plugins/layerfs test`. These tests build the real Mainline CLI
   and use public-client stand-ins and temporary Git repositories. They require no
   Docker, Store, daemon or native addon. Report them as adapter tests, not proof of
   real LayerFS/FUSE execution.
7. Inspect the host's existing `LAYERFS_*` configuration. Ask for missing project,
   Sandbox, task Branch or deployment settings instead of guessing them. The host
   supplies the pinned bundle/native artifacts and a Sandbox image with Linux
   Mainline, Git, Bash, coreutils and findutils. Follow the README's Dockerfile steps
   only if an image extension is needed and the base image and architecture are known.
   Use an independent actor identity and a complete standalone Git repository.
8. On Linux use the configured local Store mount in the calling process. On macOS
   or Windows use the host's `LAYERFS_WORKER_ENDPOINT`, or use
   `mainline-layerfs launch --` with the agent executable and its arguments. The
   launcher maps the README's `LAYERFS_*` settings to the public runtime options,
   starts the worker once and keeps it alive until the agent exits. Agents call only
   the public agent SDK or the official `layerfs` command.
9. Once the host is configured, execute one explicit `mainline-layerfs run` preflight
   against the supplied project, Sandbox and task Branch, or the README's opt-in
   smoke command. Inspect Mainline's readiness and authority fields as well as the
   exit code, checkpoint and cleanup result. If the host is not ready, report which
   verification was not run and what settings are missing.

Report both pinned commits, commands run, results, installed skill paths and runtime
verification limits. If anything fails, stop and give me the exact error; never
retry or replay a LayerFS operation, force-commit, discard unresolved changes,
restart a worker or create a replacement Sandbox automatically.

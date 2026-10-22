# LayerFS × Claude Code

> **Status:** Current general guide. Plugin implementation candidate 0.1.0;
> Claude Code and real Docker/FUSE acceptance are pending. Not a released integration.

A native Claude Code plugin following the [DSH integration](../dsh/README.md):
agents edit through the original JavaScript SDK, run project commands in
Workspaces, and merge independent Branches. One Lead coordinates Workers and
publishes. Claude supplies Bash, background tasks, delegation and compaction.
LayerFS supplies files and history. There is no MCP server or new UI.

Custom Agent prompts define the tool sets. Plugin Hooks bind the real session and
Worker IDs to Branches, inject each Bash call's environment, and reject ordinary
file tools, unmanaged agents and Git worktree requests. A small SDK entry checks
ownership and records resources while calling the original SDK in the same
process. It does not proxy Store operations.

## Requirements

- Linux, or Claude Code inside WSL2; Node >=20 and Claude Code >=2.1.295.
  Declarations were checked against the official 2.1.307 documentation, not
  exercised in a Claude session. `onFailure: "block"` requires 2.1.295.
- A matching LayerFS source checkout, Linux native addon and daemon. Follow
  [the SDK build guide](../sdk/js/README.md#builds-and-compatibility) and
  [the DSH runtime build recipe](../dsh/README.md#1-install-layerfs).
  Build the addon and daemon together from the source recorded in
  [compatibility.json](compatibility.json). No public npm package or prebuilt
  runtime is assumed.
- A local Linux Docker Engine, FUSE, an existing local named volume, and a local
  runtime image ID with the project's toolchain. The SDK and daemon must share
  one Linux kernel. Remote Docker and the DSH macOS Store worker are outside
  this integration's scope.
- Direct read/write access to the volume's Store directory and LayerFS home.
  Docker's default volume directory often needs operator setup; Docker socket
  access alone does not grant Store access. Store must be on ext4/xfs/btrfs,
  including on WSL2; `/mnt/c` and `/mnt/d` are not supported Store locations.
- Disposable is the only Store profile. History is not power-loss durable.

The plugin is a development workflow for a trusted account, not a security
sandbox for hostile scripts. Arbitrary Bash retains the account's capabilities.
Claude permission policy and OS permissions still apply. The plugin does not
auto-approve Bash or choose a permission mode.

## Configure a project

Use a dedicated control directory, not a source checkout. Its config, session
bindings and scratch survive plugin updates and removal. Keep the Store and
LayerFS home outside the installed plugin cache and outside CLAUDE_PLUGIN_DATA.

```bash
mkdir -p /path/to/control/shop
cd /path/to/control/shop
cp /path/to/layerfs/claude/deployment.example.json ./deployment.json
```

Edit the absolute Linux paths, project, Docker endpoint, named volume and image
ID in deployment.json. Create the volume and build the image beforehand. Use
`docker volume inspect <volume>` to find its Mountpoint: storePath must be a file
inside that exact volume. For example, Docker may expose
`/var/lib/docker/volumes/layerfs-shop/_data/store.sqlite`. Use the real local image
ID from `docker image inspect --format '{{.Id}}' <image>`.

```bash
node /path/to/layerfs/claude/scripts/control.mjs configure ./deployment.json
# Explicit, one-time import; includes .git, ignored files and dependencies:
node /path/to/layerfs/claude/scripts/control.mjs import /absolute/project/source
node /path/to/layerfs/claude/scripts/control.mjs doctor
```

For an existing initialized project, select its original home, Store and volume
and skip import. Configure records SDK/native/daemon identities; doctor checks
the deployment and actually opens the Store in its own process. Neither command
starts Claude. If configure reports a missing prerequisite after writing the
configuration, correct the prerequisite and run doctor. To replace configuration,
preserve `.layerfs-claude/config.json` explicitly first; existing sessions remain
bound to their original configuration and must not silently move projects.

The image provides programs and tools. Committed dependencies and configuration
are part of LayerFS state; host credentials and configuration are not magically
copied into Workspaces.

## Load the official plugin format

For checkout development, launch from the configured control directory:

```bash
claude --plugin-dir /absolute/layerfs/claude --agent layerfs:lead
```

The repository also contains an official-format marketplace descriptor. Once
the branch is available in the selected repository, add that repository with
`/plugin marketplace add <owner>/layerfs` and install
`/plugin install layerfs@layerfs-plugins --scope local` in the control directory.
Use `claude --agent layerfs:lead` for installed use. The plugin settings select
the same agent by default, but the explicit flag makes the intended mode clear.
Enable this plugin only in LayerFS control directories: its Hooks reject
unbound sessions. It does not create AGENTS.md or CLAUDE.md.

Do not use `--worktree`, Agent worktree isolation or agent teams. All Workers
are `layerfs:worker`, with independent LayerFS Branches. Model selection and
permissions inherit the user's choices. If Claude's Bash sandbox is enabled,
its filesystem/network policy must permit the configured home, Store, plugin,
SDK and local Docker/daemon endpoints; the plugin does not disable it for you.

## Work normally

Describe the development task to the Lead. The original SDK Skill is preloaded
for the agents and also available through DSH_LAYERFS_SKILL. Its "Inside dsh"
setup applies here too. No Skill fork or npm install is required. Every Bash
call gets a fresh CC_LAYERFS_BRANCH, CC_LAYERFS_ROLE, SDK entry and trace step;
Worker identity is never stored in the shared shell environment file.

```bash
node "$DSH_LAYERFS_CLI" status
node "$DSH_LAYERFS_CLI" cat "$CC_LAYERFS_BRANCH" /README.md
node "$DSH_LAYERFS_CLI" run "$CC_LAYERFS_BRANCH" 'timeout 110 npm test'
node "$DSH_LAYERFS_CLI" resources
```

Use SDK scripts for more complex work. File and Branch methods are the original
SDK methods and return their original results. Fork/merge selects the returned
Branch for later Bash calls; use the returned branch in the same script. `use`
selects another owned Branch. Ordinary Bash runs SDK scripts or scratch commands;
only explicit workspace.exec enters the project.

`run` attempts Commit after both zero and nonzero command exit. It returns the
command's exit code, or 2 for a refusal. Conflict, Dirty, unknown outcomes and
failed cleanup preserve the Workspace. `--discard` is an explicit decision to
abandon this command's changes, never exception cleanup. UpToDate creates no
Commit; call receipts are separate from history.

Before publishing, merge current main into an integration Branch, run a meaningful
check with `run ... --validate`, then publish. The receipt binds validation to the
current Commit and integration's main version. SDK scripts may import
`recordValidation(lf, branch, description, run.exitCode, savedCommit)` after their
check and Workspace Commit. Pass the actual Commit result; a concurrent merge
or a later head change requires another check.
The plugin cannot judge whether a check is sufficient. Review generated changes
and repeat verification when a check changes what needs testing. Native
HeadMoved and overwrote information remains visible; external clients are not
part of the plugin's local publication lock.

## Long jobs and recovery

Keep long-lived Workspaces explicitly: import `retainWorkspace(ws, reason)` from
DSH_LAYERFS_SDK. Use the operator entry for process.start/list/cancel, then Commit
and unmount when done. CLI `keep`, `release`, `start`, `cancel`, `commit` and
`unmount` expose the same operations. TaskStop and termination of an outer Bash
are not proof that a daemon-side process stopped.

`resources` reports owned handles and unfinished/uncertain operations. The Lead
also sees its Workers' registered resources and unresolved calls. Its Stop
check covers those Workers as well. A Stop
Hook asks once for unresolved resources; it does not loop forever or discard
changes. SessionEnd only records termination. No automatic SDK retries or
uncertain-call replay occur. Trace is diagnostic, not the resource authority.

If a reply or process was lost, inspect Store history and operator/Workspace
status before deciding what happened. `recover <sandbox> <path> <evidence>`
registers an explicitly identified orphan only when its Branch belongs to the
caller. `acknowledge <record-file> <evidence>` records an inspected uncertain
outcome; it performs no project operation. The Lead can `clean-worker` a stopped
Worker's registered, unretained, idle and clean Workspace; dirty resources remain.
For an interrupted Branch fork, inspect the agent's fork receipt and actual
Branch: the plugin never guesses ownership from a matching name. Preserve the
records and repair the binding only after establishing the outcome.

Local record locks contain owner.json. A killed owner can leave a `.lock`
directory: establish that it is no longer active before removing that specific
lock. Records are small JSON files under `.layerfs-claude/sessions/`. Call files
hold the original command and deployment environment overrides; inherited
credentials are not snapshotted. Avoid secrets in command text and diagnostics.

Resume restores the recorded Branch. A forked conversation gets a new Branch.
Claude `/rewind` does not roll back SDK changes: use LayerFS history. If the
Agent definition is missing, Hook binding fails rather than allowing local
project edits. Disabling/removing the entire plugin also removes its Hooks;
that cannot be enforced by a plugin that is no longer loaded.

## Contribution checks

```bash
node claude/tools/sync-skill.mjs  # only when updating canonical Skill/SDK pins
node claude/tools/check.mjs
timeout 120 node --test claude/tests/*.test.mjs
```

The Node tests run the real JS SDK against a native-addon test double. They
exercise plugin workflow, not real SQLite, FUSE or Claude scheduling. No Claude
process is needed. Follow [TESTING.md](TESTING.md) for the owner's official
validator and real Lead + two Worker acceptance. See the
[design](../docs/roadmap/claude-code-plugin-design.md) for the intended boundaries.

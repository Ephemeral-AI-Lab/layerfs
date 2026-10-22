# Claude Code acceptance

> **Status:** Current general guide. Manual acceptance is pending; this checklist
> is not release evidence.

The implementation phase deliberately does not invoke Claude. Run this after
the local structure and Node behavior checks. Use a disposable project whose
source can be rebuilt; preserve failures and the actual Store/control records.

## Official loading and tool checks

1. Check `claude --version` is >=2.1.295. From the repository, run
   `claude plugin validate --strict ./claude`. Record the exact output: a manifest
   result does not establish that Agents, Skills or Hooks actually loaded.
2. Configure/import/doctor as in the README. Start from its control directory
   with `claude --plugin-dir /absolute/layerfs/claude --agent layerfs:lead --debug`.
   Check startup Hook context, `/agents`, `/skills` and `/hooks`. Confirm that
   layerfs:lead is the main agent, layerfs:worker is available, the upstream
   Skill is readable, and hooks reference this plugin root once each.
3. Ask the Lead to report its available tools. The Lead set is Bash, Skill,
   Agent(layerfs:worker), TaskStop, AskUserQuestion; Worker has Bash, Skill and
   TaskStop. Confirm ordinary Read/Edit/Write/Glob/Grep, LSP, PowerShell and
   worktree tools are absent. Verify actual tool calls and debug events, not
   only the model's claim. Check any main-agent Skill preload fallback reads
   DSH_LAYERFS_SKILL before SDK work.
4. In an isolated copy of the plugin, point the PreToolUse command at a missing
   script, make it crash, emit invalid JSON, and delay beyond its timeout, one
   case at a time. Ask for harmless Bash. Each case must block because of
   onFailure:block. Restore the plugin and restart between cases. Never test a
   broken policy Hook against a valuable project.
5. Resume with a missing Agent definition while the plugin Hooks remain loaded.
   Project tool calls must be denied. Disabling the whole plugin is outside its
   enforcement boundary. Starting in an unconfigured directory must show binding
   failure and deny subsequent project tools.

## Development and parallel workers

Use a tiny Node project with a test command and .gitignore. Ask:

> Read the LayerFS SDK Skill. Check or create /.lww with `%include .gitignore`.
> Delegate two independent changes to two layerfs:worker agents in parallel.
> Each Worker must inspect its assigned Branch, make one change, run the tests,
> and report its Branch, Commit and resource state. Integrate both results,
> merge current main, validate the integrated version and publish it. Report
> actual native results and leave no unretained Workspaces.

Inspect `.layerfs-claude/sessions/` and actual SDK branch/history results:

- One Lead and two real Worker IDs have separate Branches. Worker forks name
  the Lead's committed source version. Concurrent calls have distinct trace
  steps and the correct per-agent environment; shared CLAUDE_ENV_FILE has no
  Worker Branch/role. Try a stale inherited LAYERFS_STORE_PATH and verify it is
  replaced by the selected deployment.
- Both Worker changes appear in the integrated Branch and published main.
  branch.merge leaves its inputs unchanged and later calls use its result.
  The exact validated Commit is published; record native overwrote/HeadMoved
  results without suppressing them. Verify fs writes cannot target another
  agent's Branch and Workers cannot publish through the managed entry.
- Ordinary project commands run through Workspace. Install dependencies, Commit
  them, mount again and verify they exist without reinstalling. Confirm image
  tools and committed project config are available, without assuming host
  secrets were copied. Store calls remain local SDK calls, with no Store server.
- Compare call receipts to branch history: a no-change check can be UpToDate and
  leave no Commit. Record final Branch/head, main Layer, validation and remaining
  Workspaces/processes using native SDK/operator state.

## Failure, recovery and long jobs

1. Run a command that writes a file then exits 7. Confirm the CLI returns 7,
   commits the file and unmounts. Run a no-change command and confirm UpToDate.
2. Create an actual Workspace Commit conflict and a Branch merge conflict.
   Confirm native Conflict details survive, the dirty Workspace/input Branches
   remain available, and resolve through the canonical Skill. Dirty unmount must
   retain state. Explicit discard may affect only an owned Workspace.
3. Make a Store call return HeadMoved or Busy through actual contention where
   feasible. Inspect before choosing the next action. Confirm no automatic retry
   or whole-script replay. Kill an outer SDK process around a resource operation;
   inspect the pending/unknown receipt and daemon state, then explicitly recover
   and acknowledge the known outcome. Absence of trace is not proof of no effect.
4. Mount, retain and start a dev server with operator.process.start. In another
   Bash call query it, inspect/Commit the Workspace, cancel the process and
   unmount. Confirm TaskStop alone does not justify claiming the daemon job ended.
   Repeat with a Worker-owned resource; Lead cleanup must reject retained/dirty
   resources and can clean an explicitly stopped Worker's idle/clean resource.
5. Exit and resume the original session: same project and current Branch. Fork
   the conversation: different Branch. Change configuration mid-session: deny
   project operations. Upgrade/reload the plugin with the same SDK source and
   preserved external control/Store paths, then verify original bindings remain.
6. Exercise `/rewind`: conversation rewind must not be described as LayerFS code
   rollback. Advance main from another client after integration; publication must
   require new integration/validation or preserve native concurrency results.

Record the Claude version, plugin source, compatibility seal, deployment hashes,
prompt, actual hook inputs/debug events, SDK results, branch/head/main identities
and resource state. Keep credentials out of shared evidence. Report local Node
checks, official format validation and real end-to-end acceptance separately.

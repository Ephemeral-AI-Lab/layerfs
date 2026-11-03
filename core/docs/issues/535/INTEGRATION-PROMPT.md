# Handoff prompt: integrate #535 + #536, then LayerFS Connect (#537)

> **Status:** Handoff prompt, 2026-11-03. Give this file to the integration
> agent as its assignment. It follows [HANDOFF.md](HANDOFF.md) (#535 + #536
> finished, terminal pass PASS). It is not a product contract.

## Your assignment

Do these in order:

1. Check that `layerfs_v2_0_0` holds the finished #535 + #536 work. The lead
   merged and pushed it (Step A); you only verify it.
2. Build the LayerFS outputs Connect needs from the merged commit.
3. Back up Connect's LayerFS home before the new code converts it.
4. Carry out [#537](../537/README.md) in LayerFS Connect, the work order for
   Connect on the #535/#536 shape. Follow its §10 order and its §11 terminal
   pass. Read the corrections below first: #537 was written before the
   terminal pass and does not know what it settled.

Don't ask the owner routine questions. Use parallel subagents where steps are
independent. Stop and ask only at the points marked **Owner decision**.

## State at hand-over

- **Branch:** `issue535-connect` (local only), in the worktree
  `~/Ephemeral-AI-Lab/layerfs-issue535-connect`.
  - Final product commit: `32aec70a64`, production LOC 136,044.
  - The commits after it are docs and receipts.
  - The lead merged the branch into `layerfs_v2_0_0` on 2026-11-03, on top of
    two remote commits (`2ec16ad784` CLI banner, `bb53a84a7b` npm publish
    setting), and pushed it to `origin` (owner's instruction).
- **Read first:** [HANDOFF.md](HANDOFF.md) (open items, commit table),
  [TERMINAL-PASS.md](TERMINAL-PASS.md), [535 DECISIONS](DECISIONS.md),
  [536 DECISIONS](../536/DECISIONS.md) and [#537](../537/README.md).
- **`layerfs_v2_0_0` is checked out** in `~/Ephemeral-AI-Lab/layerfs-v2-merge`.
  - Its registration is `layerfs-v2-merge-7a9cf366f324`
    (`~/.cache/layerfs-worktrees/layerfs-v2-merge-7a9cf366f324/resources.json`).
  - The lead moved that checkout's untracked older copies of the 535, 536
    and 537 docs to
    `~/.cache/layerfs-worktrees/layerfs-v2-merge-7a9cf366f324/archive/untracked-docs-20261103/`.
  - Leave `core/docs/issues/527/README.untracked-draft-20261101.md` alone.
- **LayerFS Connect** is the private repo `~/Ephemeral-AI-Lab/layerfs-connect`,
  worked on directly on `main` (no worktree).
  - It links `"layerfs": "link:../layerfs-issue533-v2/api/node"`, the #533
    checkout, so it runs none of the #535/#536 changes.
  - Its LayerFS home is `~/.layerfs-connect/home-2`, a 2.0 home whose
    `layerfs.json` says `"layout":2`.
  - Its old 1.1.0 home `~/.layerfs-connect/home` stays as it is, and is never
    handed to LayerFS.
- **Builds the terminal pass used**, in the connect registration
  `~/.cache/layerfs-worktrees/layerfs-issue535-connect-7500a2864564/builds/`:
  - `k-macos/` was built from `54161c9e2d`, before the final fix, so don't use
    it for Connect.
  - `k3-bundle/2.0.0/linux-aarch64` is the sealed bundle `185b0240…`, with the
    Firecracker kit in `firecracker/`. The kit's original folder is deleted,
    so `k3-bundle` holds the only copy.
- **Lane worktrees still on disk:** `layerfs-issue535-verify` and
  `layerfs-issue535-C1`, with their registrations and `k2-bundle`.
  - They wait for a Docker Desktop restart, and the lead session removes them
    afterwards.
  - Don't touch them.

## Rules (binding)

- **No further pushing.** No push, PR, GitHub comment or tag in either repo
  without the owner's word. Never rewrite history, force anything or bypass
  hooks.
- **LayerFS commits** carry `Production LOC: <before> -> <after> (delta
  <signed>)`, measured with the pinned `tools/production_loc.py` on the parent
  and the staged tree (AGENTS.md "Production LOC comparison"). Every commit
  ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Connect commits** follow Connect's own conventions: read its `README.md`
  and `docs/`.
- **Never edit LayerFS source for Connect.** A missing SDK capability is a
  question for the owner (#537 §13), not a workaround.
- **Bounded commands.** No command runs longer than 2 minutes; longer work
  runs in the background.
  - Every test command gets a wall limit of at most 120 s, after a
    `--no-run` build on the Rust side.
  - Connect's `pnpm test:agents` and host tests run under the same 120 s
    limit; select subsets if the whole set does not fit.
- **Stores and retries.** Disposable Stores only. Durable is `NOT_RUN —
  disabled by owner until explicit reauthorization`. One attempt per
  operation: no retry or replay.
- **Hands off shared resources.** Don't touch these:
  - the `lfc-*` Lima VMs;
  - the containers `layerfs-postgres` (never use its credentials),
    `layerfs-minio`, `layerfs-worker`, `lfc-worker` and `mocktest-worker`;
  - the shared bundle `~/.cache/layerfs/bundle/2.0.0`, unless the owner
    decides otherwise (below).
- **PostgreSQL** servers run on the host only (owner D12), never in a VM.
  For tests, start your own throwaway `postgres:17` on the Mac.
- **Secrets.** Never put passwords or keys in receipts, commits or chat.
  `~/.layerfs-connect/storage.json` and the `*.env` files there hold
  secrets: don't print them.
- **Disk.** Run `python3 tools/cleanup.py check` at the start, every 30
  minutes and at the end.

## Step A: verify `layerfs_v2_0_0` (already merged and pushed)

1. `git fetch origin`, then check that `origin/layerfs_v2_0_0` equals the
   local `layerfs_v2_0_0` in `layerfs-v2-merge` and contains
   `issue535-connect` (`git merge-base --is-ancestor`).
2. If either check fails, stop and report; don't merge or push yourself.
3. Don't delete `issue535-connect` or the lane branches.

## Step B: LayerFS builds for Connect

Build in `layerfs-v2-merge` with its registration's Cargo home
(`cargo +1.85.1 --locked --offline`, `-j 6`), from the merged commit, and
record the commit and SHA-256 of every output.

- **macOS:** `layerfs-cli`, `layerfs-node`, `layerfs-python` and
  `layerfs-daemon`, each with its `postgresql` feature, the same set the
  terminal pass built:

  ```
  cargo +1.85.1 build --locked --offline -j 6 -p layerfs-cli -p layerfs-node -p layerfs-python -p layerfs-daemon --features layerfs-cli/postgresql,layerfs-node/postgresql,layerfs-python/postgresql,layerfs-daemon/postgresql
  ```

  - `api/node/src/native.js` lists where the package looks for `layerfs.node`
    (`target/debug/liblayerfs_node.dylib` is the addon). Place the addon on
    one of those paths inside the LayerFS checkout.
  - Connect finds the CLI at the linked checkout's `target/debug/layerfs` and
    the macOS daemon beside it (`host/config.js`).
- **Linux bundle:** run `python3 tools/linux_bundle.py` with `daemon`,
  `worker` and `helper`, then `seal --dest <fresh folder>`, each with
  `--resources <v2-merge resources.json> --timeout 300`.
  - Seal into a fresh folder under the registration's `builds/`. Docker
    Desktop failed twice to bind a re-created staging folder; a fresh `--dest`
    worked.
  - Then copy the Firecracker kit from `k3-bundle/2.0.0/linux-aarch64/firecracker/`
    into the new bundle's `firecracker/` (`cp -c`, keep `id_ed25519` at mode
    600).
- **Owner decision: where Connect gets the bundle.**
  - LayerFS looks at `~/.cache/layerfs/bundle/2.0.0/linux-aarch64/`, unless
    `LAYERFS_BUNDLE` (internal) names another folder
    (`api/crates/layerfs-control/src/machine/bundle.rs`).
  - That shared folder holds the older 2.0 bundle that other integrations
    use.
  - Ask whether to replace the shared bundle, or to run Connect's host with
    `LAYERFS_BUNDLE` set to the new bundle.
  - Until the owner answers, use `LAYERFS_BUNDLE` for your own tests only, and
    don't commit it into Connect.

## Step C: back up Connect's home before the conversion

The first #536 call on `home-2` converts it in place from layout 2 to
layout 3 (536 DECISIONS, F2 and F3). The conversion cannot be undone:

- it is refused `InUse` (with `detail.related`) while anything is live;
- workers delete their layout 2 folders on their first start from the new
  bundle.

1. Quit Connect and its host.
2. List what `home-2` records: machines, Stores, Sandboxes.
3. Copy `~/.layerfs-connect/home-2` to `~/.layerfs-connect/home-2.bak-<date>`
   (`cp -cR`, mode 0700). Record its file list and hashes.
4. Only then make the first call with the new build. Record its result.

## Step D: Connect, #537

Carry out #537 §10 steps 1–10. Step 1 relinks `layerfs` to
`link:../layerfs-v2-merge/api/node` and pins it in one commit (#537 §12).
Leave `layerfs-issue533-v2` in place.

### Corrections to #537 from the terminal pass

1. **Only side Branches can be mounted (owner D14).**
   - `workspace.mount(sandbox, "main")` is refused `InvalidInput`. Fork a
     Branch first with `branch.fork(project, name, "main")`, then mount that.
   - #537 §6 shows a Workspace on `main` (`task-42 (main)`), and §7's
     "Mount Workspace" and chat Workspaces need a Branch per mount. Design
     the fork into the flow, for example one Branch per chat (`chat-<id>`).
   - Connect's `tools/replay/convert.mjs:627` mounts whatever Branch a
     recording names, so a recording that mounts `main` now fails.
2. **Store calls during an operation.**
   - They are refused `Busy` during a push or pull.
   - They are refused `Unresolved` until the clearing call.
   - These are recorded spec differences (536 DECISIONS, "Differences between
     the spec and the code").
   - Check whether #537 §7's "reads from records (`project.get`) still render"
     holds for each read Connect makes, and design the banners to match. Don't
     assume reads pass.
3. **`workspace.exec` on a VM runs through the worker** (one connection).
   Don't design anything that expects it not to.
4. **Machine power is manual (#535 D11).** A Store call on a stopped machine
   is refused `Unavailable` and names `layerfs machine start`. That fits #537
   U3; show the refusal's text.
5. **PostgreSQL addresses (D12).**
   - Each machine's `reach` rewrites a loopback URL (`127.0.0.1`).
   - ssh refuses a loopback URL by design.
   - PostgreSQL Sandboxes run on every platform whose
     `supports.postgresql_sandbox` is true. Read it; don't hard-code it.
6. **Init (H11a):** `project.init` reads the source folder on the caller's
   host, also when the Store is on a VM.
7. **docker is built in on macOS** (no create).
   - `machine.remove` and `machine.disconnect` refused `InUse` now carry
     `detail.related` (`32aec70a64`).
   - `machine.list` shows docker's live state, the same as `machine.status`.
8. **Open items Connect will meet**, from HANDOFF.md:
   - The Node package leaves its Store leases at process exit. Connect's
     long-lived host should close its client on quit; doctor warns about
     leftovers.
   - After a transfer stopped at its last step, `store remove` deletes only
     the record machine's copy.
   - Disconnecting a PostgreSQL Store leaves records on worker roots. A later
     sqlite Store of that name pushed there is refused `Exists` on Init until
     a pull.
   - Push and pull send one worker request per 8 MiB. There is no single
     streamed connection, so progress comes in 8 MiB steps.
   - Don't work around these. Show them, and list them in your report.

### Connect checks

- After each step, run `pnpm typecheck`, then `pnpm test:agents` and the host
  tests, under the 120 s limit.
- Run the #537 §11 terminal pass once, on one Connect commit against one
  LayerFS build, and record it in `docs/live/<date>-535-ui.md` with
  screenshots.
- Item 6 needs a 2.0 (layout 2) home with a running Sandbox. Make a scratch
  home for it, not `home-2`.

## Done, and your report

Done means:

- `layerfs_v2_0_0` is verified to hold `issue535-connect`;
- the builds are recorded;
- `home-2` is backed up and converted;
- the #537 steps are committed on Connect `main`;
- the §11 pass is recorded;
- nothing is pushed.

Report to the owner, outcome first:

- the verified `layerfs_v2_0_0` head;
- build hashes;
- the bundle decision (or that it is waiting);
- Connect commits;
- the per-item §11 verdicts;
- deviations from #537;
- the open items Connect met;
- disk at start and end;
- anything left for the owner (a push of Connect, the shared bundle, #537 §13).

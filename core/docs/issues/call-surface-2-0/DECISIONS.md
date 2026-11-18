# Decisions applied at the spec freeze (2026-11-18)

The owner delegated the open rulings; the first (recommended) option of each is
decided. Full wording of each ruling and correction is in `rollout-body.html`
(sections "Rulings needed", "Corrections", "Correctness", "Simplicity and
cleanness", "Doctor page"). Every one below is binding on the frozen spec.

## Rulings

- **R1** No aliases for old call or option names, old paging included. No `layerfs doctor` alias.
- **R2** The fix for an outdated worker is `layerfs machine stop <m>` then `layerfs machine start <m>`,
  two commands behind one button; the button is off while Sandboxes run, with the count in its tip.
  `machine start` keeps refusing a mismatched worker.
- **R3** A check that could not be made: a `not_checked` entry carries either `blocked_by` or
  `reason` (`timeout`, `busy` or `lost`). A check that does not apply is left out. The page shows
  one "Could not check" signal with Check as its action.
- **R4** (owner) No doctor takes `--offline`. Opening the Doctor page runs no doctor: it reads
  `home get --with tree`. Records checks run inside `home doctor`; a records finding may name a
  thing at any depth.
- **R5** `layerfs backup doctor <bucket> --full` (not `--with write`). `--with` only ever adds a section.
- **R6** `layerfs home leftovers [--machine M]` is its own call (85 calls in all);
  `layerfs home prune <leftover>` removes one it listed. `home get` has no `--with leftovers`.
- **R7** `layerfs workspace cancel <workspace> [--exec E]`: every running command by default.
  `workspace.list` joins the agent profile. A worker that does not answer: the short form answers
  `{"unknown":true}` (decide the exact key in the row and state it).
- **R8** `project doctor --full` reads everything reachable from the head of each of the project's
  Branches (what a mount can reach). Streamed, bounded memory, no writes, no format change. An
  object shared by two projects is read once per project.
- **R9** `store doctor` on a PostgreSQL Store probes from each machine that has a Sandbox of that
  Store. `--machine M` stays for the command line.

## Corrections C1–C14

Apply each as written in `rollout-body.html`. In short: C1 keep `--discard`/`--force` on
`workspace unmount`, `--force` on `workspace commit`, `sandbox remove`, `machine stop`;
C2 `-m` on every call that makes a Commit, `mode` on `file write` and `file mkdir`; C3 rebuild
the "Today" column of the argument table from the code's own table and list every removed
option as dropped; C4 `machine prune <machine> [image] [--unused]`; C5 `machine update` keeps
`--cpus --memory --image --docker --wsl --daemon` (no `--disk`; resizing is not in 2.0);
C6 `machine load-image <machine> <image>` (a Docker reference copied from the host's Docker);
C7 comparing two different paths is dropped with `file.diff`; C8 merging into an existing
Branch is new work, and `--expected` on `branch publish` guards main's head; C9 exit codes 0, 2
and 125 with the refusal on stderr, a doctor exits 0 once its report is made, the refusal code
list completed (adds `InUse`, `Unresolved`, `InvalidInput`, `Unsupported`, `Dirty`, `Failed` to
`NotFound`, `Exists`, `Capacity`, `Conflict`, `Busy`, `Unavailable`, `Unknown`; check the code's
own list and use exactly its set); C10 Workspaces are in the home's records only on `native` and
for a Sandbox reached directly, so `home get --with tree`, `sandbox list` and `workspace list`
say so (a stopped machine's Workspaces are not listed; its Sandboxes answer `blocked`);
C11 one table for state and its companion: `busy` has `op`, `blocked` has `blocked_by`, `broken`
has `reason` and `fix`, `unknown` has `observed_at`, `ready` and `stopped` have none; Docker not
running is `broken` with `reason: engine_down`; every `get` example carries its `state`;
C12 Python spells the option `with_=["live"]`; C13 the partial results are stated: a
`sandbox create --mount` whose mount is refused keeps the Sandbox and names it in the refusal;
`store move` goes to or from this computer only; a lost reply on `machine start` or `stop` is
settled by `machine get --with live`; C14 progress and the shallow Store check are new work;
the project record check is not new (and its finding is a notice with a fix, not an error).

## Simplicity changes adopted

- `--with` is defined once: it adds one named section to an answer. Values: `live`, `stats`,
  `space`, `docker`, `platforms`, `tree`, `patch`. `storage` and `usage` fold into `stats`
  (`project list --with stats`, `sandbox get --with stats`). `project get` takes no `--with`.
  `machine stats` stays a call and keeps `--with space`. `write` is gone (R5), `leftovers` is gone (R6).
- Paging is one convention: `--limit N --cursor C` in, `{"items":[…],"next_cursor":"…"}` out.
  The three log calls take `--cursor` (no `--before`). `file glob` and `file grep` gain `--cursor`.
  Every list call either pages or says in its note why it is bounded. One default limit per kind:
  100 for lists, 200 for logs, 2000 lines for text windows.
- `store connect <store> <url> --auth A` on a Store's first connect; `store connect <store> <url>
  --machine M` adds one machine's own address and refuses `--auth` (a machine never has its own login).
- `backup list [--bucket B]`. `commit diff <project> <ref> [--from R]`: without `--from`, the
  changes since the fork (for a Branch) or since the parent (for a Commit).
- `sandbox.close` is renamed `sandbox.remove`.
- `home.get` answers `generation` (a number that moves when any record changes), not `changes`.
- No default machine recorded on a PostgreSQL Store: the machine is chosen at `sandbox create --machine M`; without it the Sandbox runs on this computer (`native`), as today.
- Examples obey the rules: one key set per call's long form, a `state` on every `get`, no `name`
  for an existing thing, no plural that is a number.

## Open decisions of version 17, decided

- A write to a mounted Branch is allowed; the Workspace's status answers `branch_moved:true`.
- Exit codes: see C9. `workspace exec` exits with the command's code and prints its result JSON on
  stdout; a refusal prints `{"error",…}` on stderr and exits 125.
- `file grep` takes a literal pattern; its help says literal.
- No Workspace file tools. No worker route for browsing a Store this computer cannot reach.
  Transaction-pooling proxies are documented, not detected. No login per machine.
- Blocked projects stay in `project list`, marked `blocked`.
- No raw-text mode for agent reads; JSON stays the default; `--text` stays as today.

## Totals after the freeze

85 calls in 10 families: project 8, file 14, sandbox 7, workspace 7, branch 6, commit 3, home 7,
machine 14, store 13, backup 6.

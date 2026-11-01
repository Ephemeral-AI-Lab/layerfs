# #533 handoff: LayerFS 2.0 on branch `issue533-v2`

> **Status:** Dated handoff, 2026-11-02; not release evidence. Work done under
> the owner's delegation (owner asleep): every gate and open decision took its
> recommended answer, recorded in [DECISIONS.md](DECISIONS.md). Nothing is
> pushed, no PR, no merge, no GitHub comment.

- **Worktree:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-issue533-v2`, branch
  `issue533-v2`, stacked on local `layerfs_v2_0_0` at `ddc7c5696b` (owner's
  choice, 2026-11-02; local only, not pushed). A later move of that branch
  comes in by a merge, never a rebase.
- **Terminal pass:** [TERMINAL-PASS.md](TERMINAL-PASS.md).
- **Steps:** 1–19 all have commits. Step 19 ran on this Mac only; its VM, ssh,
  docker-worker, Linux, R2 and Windows rows are NOT_RUN (reasons there).

## Commits and production LOC

Counter: `tools/production_loc.py`, combined core+engine+api (reference
absent: retired R9c; last counted 65,417 at tree 498dd1917812). "Actual"
recounts each commit's tree against its real parent here (`git archive` of
both). Where the commit message's line differs, the message was counted on
another parent or has a malformed line; the delta is the same in every case.

| Commit | Subject | Actual production LOC | Message's line |
| --- | --- | --- | --- |
| `51dc73b35d` | docs: the 2.0 spec, both proposals and the decisions log | 124168 -> 124168 (delta 0) | same |
| `a77d56250f` | docs: step 1, the AGENTS.md amendments of H §9.1 (G2) | 124168 -> 124168 (delta 0) | same |
| `e6b024d8b9` | tools: step 2.1, the LOC counter and layer guard learn the 2.0 crates | 124168 -> 124168 (delta 0) | same |
| `a1daa774bf` | node: step 2 prep, locks and ids through nix | 124168 -> 124173 (delta +5) | same |
| `dbe103fa1a` | api, node: step 2.2, the binding's host logic into layerfs-api | 124173 -> 124211 (delta +38) | same |
| `d2c7d0a1db` | api: step 2.3, the call table and its one dispatcher | 124211 -> 125372 (delta +1161) | **malformed**: only `combined 125372 production lines` |
| `69f57460c6` | api: step 3, layerfs.json and the layout version | 125372 -> 125515 (delta +143) | **malformed** (as above) |
| `6cde23aca1` | api: step 4, the home layout | 125515 -> 126112 (delta +597) | **malformed** (as above) |
| `f6193f64d5` | api: step 7, routes by records, leases, the worker protocol, branch/commit/file | 126112 -> 126961 (delta +849) | same |
| `7071cf6f3e` | api: step 8, sqlite Stores | 126961 -> 127591 (delta +630) | same |
| `9c13e7bb95` | sandbox: step 5.1, platform/ becomes backend/ (move) | 127591 -> 127590 (delta -1) | 124211 -> 124210 (cherry-picked; counted on its subagent parent) |
| `9e52bb9306` | sandbox: step 5.2, machine/ behind the machine feature | 127590 -> 130557 (delta +2967) | 124210 -> 127177 (as above) |
| `39f117abf0` | sandbox: step 5.3, tests for the machine layer | 130557 -> 130557 (delta 0) | 127177 -> 127177 (as above) |
| `139ff76051` | api: step 6, the machine family | 130557 -> 131395 (delta +838) | same |
| `b75629672b` | api: step 9, projects as `<store>/<project>`, Init as a live add | 131395 -> 131668 (delta +273) | same |
| `a744288f82` | api: step 10a, PostgreSQL connect, disconnect, status | 131668 -> 131821 (delta +153) | same |
| `40f584a9fe` | api: step 11, backups | 131821 -> 132509 (delta +688) | same |
| `68c6ce368c` | sandbox: O27, remove the Linux native pairing | 132509 -> 132484 (delta -25) | 131395 -> 131370 (cherry-picked) |
| `4f6975d9f5` | api: step 12, named Sandboxes and the workspace family | 132484 -> 132930 (delta +446) | same |
| `a8ea775b54` | api, sandbox: step 13, machines on demand, the worker's idle stop | 132930 -> 133002 (delta +72) | same |
| `b07cde28d5` | api: step 14, one package `layerfs` with nine families | 133002 -> 128341 (delta -4661) | same (api 4879 -> 107: 1.1.0 packages retired) |
| `39ae52158a` | cli: step 15, the `layerfs` binary and `layerfs worker` | 128341 -> 128865 (delta +524) | same |
| `6172866478` | api: the JSON call wrapper into the library (move-like refactor) | 128865 -> 128884 (delta +19) | same |
| `13e9fb2aaf` | python: step 16, the Python package over PyO3 | 128884 -> 128924 (delta +40) | same; plus 185 Python lines the counter does not count |
| `4437a7bf39` | persistence, project: step 10b, Init into a PostgreSQL Store | 128924 -> 130308 (delta +1384) | same (recounted when cherry-picked) |
| `c232269eb1` | tests: step 10b, PostgreSQL tests make their Store | 130308 -> 130308 (delta 0) | same |
| `d244cdb3af` | persistence, api: step 10c, remove the SQLite-to-PostgreSQL import | 130308 -> 129807 (delta -501) | same |
| `9b12a211b6` | api: step 10b in the library | 129807 -> 129996 (delta +189) | same |
| `6775d06789` | api, cli: step 17, settings removed, `layerfs import-env` | 129996 -> 130235 (delta +239) | same |
| `11551a8d4f` | test: the twelve-cell runner on the 2.0 package | 130235 -> 130235 (delta 0) | same |
| `707b534eb2` | docs: step 19, the terminal pass | 130235 -> 130235 (delta 0) | same |
| `73e38ecf27` | api: file.apply refuses the Store's own change field names | 130235 -> 130245 (delta +10) | same |
| `f7125ae090` | docs: step 18, the 2.0 manual, the SDK skills and the READMEs | 130245 -> 130245 (delta 0) | same |
| (this file) | docs: the handoff | 130245 -> 130245 (delta 0) | same |

Total: 124168 -> 130245 (delta +6077) across the branch, with the 1.1.0
JavaScript packages retired (-4772 in `api`) and the 2.0 library, machine
layer, command and bindings added in `engine`.

Step order differs from the README: 7 and 8 before 5 and 6 (step 5 ran in a
subagent worktree), and 10b/10c after 16 (a subagent worktree, cherry-picked).

## Decisions

All are in [DECISIONS.md](DECISIONS.md): G0–G16 and O1–O35 with the
recommended answer, the replaced recommendations (H §7.7 route, O27), and the
implementation choices per step. Read especially:

- the **correction** at its end: the store route keeps `docker` on Linux
  in-process, contrary to the H §7.7 row; unverified either way;
- Sandboxes on the `docker` machine and on PostgreSQL Stores are refused
  `Unsupported` (step 12);
- `import-env` answers commands for VM and ssh settings instead of acting.

## Tests deleted

- **O27** (`68c6ce368c`): `layerfs-sandbox` `tests/native_platform.rs` (2
  tests) and the Linux half of `native_provider` (listed in that commit).
- **Step 14** (`b07cde28d5`): `layerfs-node` `tests/native_admission.rs` (2),
  `native_connection_wait.rs` (1), `native_discovery.rs` (6),
  `native_local.rs` (12), `tests/support/` and `tests/README.md`; and the 63
  test files of the four removed JavaScript packages (names in the commit).
  The Linux behaviours they guarded (#463 discovery reclaim, #508 admission
  and connection wait) are **not re-covered** on the 2.0 entry point.
- **Step 10c** (subagent `1c05545ce4`, picked as `d244cdb3af`): its JS test
  edits fell with the packages; no Rust test was deleted.

## Not verified

- Every VM kind, ssh, and the docker machine's worker container: no sealed
  Linux bundle (O16) and no builder for one. The machine adapters have unit
  tests only.
- Anything on Linux: the namespaces backend, `remote_store`, the in-process
  `docker` route, the Linux worker; and Windows (O5).
- R2 backups (no account here).
- Release-build timings: the terminal pass used debug builds.
- One worker serving several Stores, machine boot and worker start times,
  the guest-to-host send speed (README §8).

## Known gaps found late (not fixed)

- The workspace version is still `1.1.0`: `layerfs --version` prints it and
  the bundle cache path is `bundle/1.1.0/`; the npm and Python packages say
  2.0.0.
- `file.read` refuses a binary file (`Binary`) and no 2.0 call reads its
  bytes back.
- `sandbox.list`/`sandbox.get` accept `page_size`/`page_pos` but return
  every item.
- Exec output over 1 MiB is spooled by the host code; no 2.0 call reads the
  spool back.
- `project.init` on a PostgreSQL Store answers `machine: "native"`.
- `backup.restore` onto another machine answers `Unsupported`; the spec says
  `InvalidInput`.
- A small Init emits no progress event.
- O6's other-named PostgreSQL schemas are not handled; only `layerfs`.
- `workspace exec --stream` and the Node/Python `stdout`/`stderr` callbacks
  get the output after the command ends, not live.
- Found while rewriting the skills (the skills document the code as it
  behaves): `workspace.commit` and `sandbox.close` ignore `force` (O31 not
  in the host close); a closed Sandbox's record stays until
  `sandbox.close(name)`, so re-creating the name is refused `Exists`;
  `file.grep` matches plain text, not a regular expression; `project.init`
  returns the project record, not `{project, branch, commit}`, and refuses a
  Store off `native` with `Unsupported` (S: `InvalidInput`);
  `branch.get`/`commit.get` refuse `NotFound`, not `NoSuch*`; a file read at
  a Commit does not check that the Commit is on the named Branch; a short
  Commit id to `commit.diff` comes back `NoSuchBranch`; refusal `detail`
  keys stay camelCase; `workspace.mount`'s `sandbox` field is the host's id,
  not the name; the `Capacity` message names `sandbox.list_workspaces`;
  `project.list` returns no `has_more`; S's handle objects (`ws.exec()`)
  are not built.
- No 2.0 call reads the 1.1.0 backend's reads: Store totals (canonical,
  stored, dedupe), history and comparable reads, revisions, Workspace
  changes and storage, processes, Sandbox logs, events; `commit.list main`
  is refused `NoSuchBranch`. Nothing writes a trace (the writer was in
  `@layerfs/agent`); `host/trace.rs` has no callers.
- Historical docs still link to the deleted skill pages
  `methods/fs-read.md` and `fs-write.md` (`core/docs/issues/334`, `352`,
  `373`, `428`); left as history.

## Cache releases for the cleanup script

All owner-released at handoff; none holds evidence (the receipts are
committed).

| Resource | Size | Note |
| --- | --- | --- |
| `layerfs-issue533-v2/target` | 11 GB | this worktree's macOS target; keep warm only if 2.0 work continues |
| `layerfs-issue533-v2-step5/target` (incl. `target/x86_64-unknown-linux-gnu`) | 5.4 GB | step-5 subagent worktree, branch `issue533-v2-o27`; its commits are cherry-picked |
| `layerfs-issue533-v2-pg/target` | 7.4 GB | step-10b/10c subagent worktree, branch `issue533-v2-pg`; cherry-picked |
| `~/.cache/layerfs-worktrees/layerfs-issue533-v2-{5ff20ac6a0aa,pg-550caaac6c6d,step-9d8c05ce2ded}` | ~1.1 GB | launcher state and receipts of the three worktrees |
| `$TMPDIR/lfs-sbx-{home,folder}-{51039,51054}` | small | leftovers of a failed step-12 run; their removal was refused by the auto-mode classifier, so they were left |
| containers `lfs533-pg-main` (stopped, `--rm`, so removed) and `lfs533-minio` (removed by `minio_service.sh stop`) | – | nothing left |

`python3 tools/cleanup.py check` at handoff: 168.3 GB free; no cleanup
needed.

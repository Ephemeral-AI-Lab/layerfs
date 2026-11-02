# #533 handoff: LayerFS 2.0 on branch `issue533-v2`

> **Status:** Dated handoff, 2026-11-02; not release evidence. Work done under
> the owner's delegation (owner asleep): every gate and open decision took its
> recommended answer, recorded in [DECISIONS.md](DECISIONS.md). Nothing is
> pushed, no PR, no merge, no GitHub comment.

- **Worktree:** `layerfs-issue533-v2` (beside the main checkout), branch
  `issue533-v2`, stacked on local `layerfs_v2_0_0` at `ddc7c5696b` (owner's
  choice, 2026-11-02; local only, not pushed). A later move of that branch
  comes in by a merge, never a rebase.
- **Terminal pass:** [TERMINAL-PASS.md](TERMINAL-PASS.md).
- **Steps:** 1–19 all have commits. **Step 19's pass condition is met at
  `6837bcfce2`:** the 12 #516 cells PASS on all seven machine kinds and on
  the two Linux rows (a Lima QEMU VM), 108 receipts. The pass found seven
  defects, each fixed with a test (DECISIONS, "Terminal pass fixes"). Still
  NOT_RUN in step 19: two Lima machines with two Stores each at once, the
  VM kinds with Docker stopped, R2; Windows is paused for the owner (P1-28).

## Commits and production LOC

Corrected after review P2-29 (2026-11-02). Every commit on `issue533-v2`
from its base `ddc7c5696b` to `4194d34001`, recounted with
`tools/production_loc.py --root` (the copy at `4194d34001`) on `git archive`
of the commit's real first parent and of the commit itself; combined
core+engine+api. Reference absent (retired R9c; last counted 65,417 at
tree 498dd1917812). The branch is linear (no merge commits). The recount
agrees with every commit message that has a `Production LOC:` line except
the four marked below, whose deltas agree and whose totals do not:
`9c13e7bb95`, `9e52bb9306` and `39f117abf0` were counted on the step-5
subagent branch (made on `dbe103fa1a`, 124211) and `68c6ce368c` on its
own subagent parent (`139ff76051`, 131395) before they were
cherry-picked here. `d2c7d0a1db`, `69f57460c6` and `6cde23aca1` carry no
`Production LOC:` line, only the counter's after total. History is not
rewritten; this table is the record.

| Commit | Subject | Production LOC (recounted) | Message's line |
| --- | --- | --- | --- |
| `51dc73b35d` | docs(533): the 2.0 spec, both proposals and the decisions log | 124168 -> 124168 (delta 0) | same |
| `a77d56250f` | docs(533): step 1, the AGENTS.md amendments of H §9.1 (G2) | 124168 -> 124168 (delta 0) | same |
| `e6b024d8b9` | tools(533): step 2.1, the LOC counter and layer guard learn the 2.0 crates | 124168 -> 124168 (delta 0) | same |
| `a1daa774bf` | node(533): step 2 prep, locks and ids through nix instead of raw FFI | 124168 -> 124173 (delta +5) | same |
| `dbe103fa1a` | api, node(533): step 2.2, move the binding's host logic into layerfs-api | 124173 -> 124211 (delta +38) | same |
| `d2c7d0a1db` | api(533): step 2.3, the call table and its one dispatcher | 124211 -> 125372 (delta +1161) | **none**: only the counter's `combined 125372 production lines` (the after total) |
| `69f57460c6` | api(533): step 3, layerfs.json and the layout version | 125372 -> 125515 (delta +143) | **none**: only the counter's `combined 125515 production lines` (the after total) |
| `6cde23aca1` | api(533): step 4, the home layout for Stores, backups and records | 125515 -> 126112 (delta +597) | **none**: only the counter's `combined 126112 production lines` (the after total) |
| `f6193f64d5` | api(533): step 7, routes by records, leases, the worker protocol and the branch/commit/file families | 126112 -> 126961 (delta +849) | same |
| `7071cf6f3e` | api(533): step 8, sqlite Stores: list, get, status, create, remove and move | 126961 -> 127591 (delta +630) | same |
| `9c13e7bb95` | sandbox(533): step 5.1, platform/ becomes backend/, the Docker client docker_engine/ | 127591 -> 127590 (delta -1) | **wrong totals** 124211 -> 124210, counted on its subagent branch before the cherry-pick; delta right |
| `9e52bb9306` | sandbox(533): step 5.2, machine/ behind the machine feature | 127590 -> 130557 (delta +2967) | **wrong totals** 124210 -> 127177, counted on its subagent branch before the cherry-pick; delta right |
| `39f117abf0` | sandbox(533): step 5.3, tests for the machine layer | 130557 -> 130557 (delta 0) | **wrong totals** 127177 -> 127177, counted on its subagent branch before the cherry-pick; delta right |
| `139ff76051` | api(533): step 6, the machine family over records, machines started on demand | 130557 -> 131395 (delta +838) | same |
| `b75629672b` | api(533): step 9, projects as `<store>/<project>` and Init as a live add (O7) | 131395 -> 131668 (delta +273) | same |
| `a744288f82` | api(533): step 10a, PostgreSQL Stores: connect, disconnect and status | 131668 -> 131821 (delta +153) | same |
| `40f584a9fe` | api(533): step 11, backups: buckets by name, create and restore through the caller | 131821 -> 132509 (delta +688) | same |
| `68c6ce368c` | sandbox(533): O27, remove the Linux native pairing | 132509 -> 132484 (delta -25) | **wrong totals** 131395 -> 131370, counted on its subagent branch before the cherry-pick; delta right |
| `4f6975d9f5` | api(533): step 12, named Sandboxes and the workspace family | 132484 -> 132930 (delta +446) | same |
| `a8ea775b54` | api, sandbox(533): step 13, machines on demand and the worker's idle stop | 132930 -> 133002 (delta +72) | same |
| `b07cde28d5` | api(533): step 14, one package `layerfs` with nine families | 133002 -> 128341 (delta -4661) | same |
| `39ae52158a` | cli(533): step 15, the `layerfs` binary and `layerfs worker` | 128341 -> 128865 (delta +524) | same |
| `6172866478` | api(533): the JSON call wrapper moves into the library for Node and Python | 128865 -> 128884 (delta +19) | same |
| `13e9fb2aaf` | python(533): step 16, the Python package over PyO3 | 128884 -> 128924 (delta +40) | same |
| `4437a7bf39` | persistence, project(533): step 10b, Init into a PostgreSQL Store | 128924 -> 130308 (delta +1384) | same |
| `c232269eb1` | tests(533): step 10b, PostgreSQL tests make their Store instead of importing it | 130308 -> 130308 (delta 0) | same |
| `d244cdb3af` | persistence, api(533): step 10c, remove the SQLite-to-PostgreSQL import | 130308 -> 129807 (delta -501) | same |
| `9b12a211b6` | api(533): step 10b in the library, connect makes the schema and Init writes into it | 129807 -> 129996 (delta +189) | same |
| `6775d06789` | api, cli(533): step 17, settings removed and `layerfs import-env` | 129996 -> 130235 (delta +239) | same |
| `11551a8d4f` | test(533): the twelve-cell runner on the 2.0 package | 130235 -> 130235 (delta 0) | same |
| `707b534eb2` | docs(533): step 19, the terminal pass on this Mac | 130235 -> 130235 (delta 0) | same |
| `73e38ecf27` | api(533): file.apply refuses the Store's own change field names | 130235 -> 130245 (delta +10) | same |
| `f7125ae090` | docs(533): step 18, the 2.0 manual, the SDK skills and the READMEs | 130245 -> 130245 (delta 0) | same |
| `d24961944e` | docs(533): the handoff | 130245 -> 130245 (delta 0) | same |
| `72eab2f1ee` | docs(533): the base is local layerfs_v2_0_0 at ddc7c5696b | 130245 -> 130245 (delta 0) | same |
| `a487409fe5` | api, sandbox(533): Sandboxes on the docker machine and of PostgreSQL Stores | 130245 -> 130548 (delta +303) | same |
| `b2ca5fceb5` | docs, test(533): the docker and PostgreSQL Sandboxes in the terminal pass | 130548 -> 130548 (delta 0) | same |
| `155e11f2fa` | api(533): a guest worker counts the Sandboxes it really holds (P0-5) | 130548 -> 130569 (delta +21) | same |
| `085376a7a8` | build(533): the workspace version is 2.0.0 | 130569 -> 130569 (delta 0) | same |
| `c3175b6eb9` | api(533): the Sandbox deploy settings move to family/deploy.rs (move) | 130569 -> 130576 (delta +7) | same |
| `6f2a43c4fd` | api(533): namespaces Sandboxes get their image archive and daemon (P0-4) | 130576 -> 130661 (delta +85) | same |
| `dfd444bd9b` | machine(533): a Firecracker host stays up, starts first, and resets its microVM (P0-6) | 130661 -> 130689 (delta +28) | same |
| `6263b95871` | tools(533): build and place the sealed Linux bundle, one bounded step per package (O16) | 130689 -> 130689 (delta 0) | same |
| `cc75955474` | sandbox(533): a native Sandbox finds its mounts by its real path | 130689 -> 130693 (delta +4) | same |
| `37f1bbd2b9` | api(533): Sandbox close, commit, output and records do what the spec says (P1-18, 20, 21, 22, 25) | 130693 -> 130810 (delta +117) | same |
| `539cc0e771` | machine(533): create and remove take the start lock; a removing machine is used by nothing (P1-9, P1-23, P2-32) | 130810 -> 130888 (delta +78) | same |
| `7cf2368a9d` | api, docs(533): native Sandbox text names deploy.localDir; start.lock in the home layout (P2-32) | 130888 -> 130888 (delta 0) | same |
| `6908c9a0af` | fix(533): a Store lease names its process by start time and boot (P1-27) | 130888 -> 130947 (delta +59) | same |
| `e375e333db` | fix(533): a lease and a Store move or remove never overlap (P0-1, P0-7, P1-9, P1-16) | 130947 -> 131027 (delta +80) | same |
| `75f83ec9b6` | fix(533): store status starts no machine and reports a down Store (P1-10) | 131027 -> 131054 (delta +27) | same |
| `9a4d9528e0` | fix(533): a missing Store or project keeps 1.1.0's codes (P1-25, store part) | 131054 -> 131054 (delta 0) | same |
| `73624c56b9` | fix(533): an open Store with a call running stays open; a worker's copy closes for a move (P0-2, P0-3) | 131054 -> 131081 (delta +27) | same |
| `d6c844eb33` | fix(533): store remove deletes an in-process machine's Store once (P1-16 follow-up) | 131081 -> 131081 (delta 0) | same |
| `bd8f90860b` | fix(api, 533): record writes from several threads at once (P1-11) | 131081 -> 131123 (delta +42) | same |
| `d1153d1776` | fix(api, 533): a new home is not a 1.1.0 home (P1-12) | 131123 -> 131131 (delta +8) | same |
| `92c2db72b3` | fix(api, 533): the 1.1.0 conversion places each Store's file (P1-13) | 131131 -> 131255 (delta +124) | same |
| `e6a5f38b68` | fix(api, 533): a worker's root is not a home; calls carry the home id (P2-32) | 131255 -> 131320 (delta +65) | same |
| `842b4058b9` | fix(533): backup restore refuses running Sandboxes and downloads before the lock | 131320 -> 131427 (delta +107) | same |
| `dfe87f9b68` | fix(533): backup create writes and checks layerfs-exported-from | 131427 -> 131399 (delta -28) | same |
| `943363f5b0` | fix(533): backup list compares the Store and the bucket copy | 131399 -> 131442 (delta +43) | same |
| `67a4723ea3` | fix(533): store connect refuses URL passwords and a second name for one database | 131442 -> 131546 (delta +104) | same |
| `88b033cfa9` | fix(533): commit.get pages to the end; NoSuchBranch and NoSuchCommit | 131546 -> 131559 (delta +13) | same |
| `797ecca2c7` | fix(533): a folder scope allows only its calls, bound to its project | 131559 -> 131662 (delta +103) | same |
| `e70c038586` | fix(533): worker frames check the secret first, with bounded reads | 131662 -> 131742 (delta +80) | same |
| `f67d5a9f6f` | fix(533): Node handlers cannot kill the process; events settle first | 131742 -> 131764 (delta +22) | same |
| `771823d728` | fix(533): Python stream; aio functions run on the loop, cancel stops them | 131764 -> 131764 (delta 0) | same |
| `0f7b2b652f` | fix(533): exec --stream exits 0 for a result with no exit code | 131764 -> 131766 (delta +2) | same |
| `94671c974c` | fix, test(533): live exec output in Node and Python; Python writes text | 131766 -> 131766 (delta 0) | same |
| `2793ee114c` | build(533): require the maturin that builds the wheel, 1.9.6 | 131766 -> 131766 (delta 0) | same |
| `4194d34001` | docs(533): step 19 passes only with 12/12 on all seven machine kinds (owner direction) | 131766 -> 131766 (delta 0) | same |
| `6f3ec06232` | tools(533): the bundle's Sandbox image is an uncompressed docker-archive named by its config digest | 131766 -> 131766 (delta 0) | same |
| `2f5cc7f180` | test(533): the cell runner can use one home per machine kind | 131766 -> 131766 (delta 0) | same |
| `005a30c93c` | fix(533): let namespaces Sandboxes pass the folders above their local root | 131766 -> 131782 (delta +16) | same |
| `44db721d95` | fix(533): a private home folder may let others pass through (o+x) | 131782 -> 131783 (delta +1) | same |
| `e1cfcde498` | fix(533): only a nested Lima host takes the bundle's firecracker/ folder | 131783 -> 131794 (delta +11) | same |
| `33a04f067d` | docs(533): receipts name the temp folder <tmp>, not this Mac's path (P2-30) | 131794 -> 131794 (delta 0) | same |
| `7b93496b00` | docs(533): test commands show their 120 s wall limit (P2-30) | 131794 -> 131794 (delta 0) | same |
| `6d39bf53eb` | docs(533): HANDOFF records every commit's production LOC and the deleted JS tests (P2-29) | 131794 -> 131794 (delta 0) | same |
| `b96288fe20` | docs(533): DECISIONS records the routes taken instead of G0, G4, step 5.2 and per-step branches; resize deferred (P2-31) | 131794 -> 131794 (delta 0) | same |
| `cfa795496e` | docs(533): the folder scope is { project, folder, branch? } with --scope-project | 131794 -> 131794 (delta 0) | same |
| `da3fa4a26c` | docs(533): backup list's seven states, exported-from and restore's InUse in the manual | 131794 -> 131794 (delta 0) | same |
| `8bbccbf45c` | docs(533): limitations drops what the review fixes built and the import-env item (P2-32) | 131794 -> 131794 (delta 0) | same |
| `89176fa936` | docs(533): the 2.0 manual follows the review fixes' behaviour | 131794 -> 131794 (delta 0) | same |
| `0fccb16cd0` | docs(533): the SDK skill's method pages follow the review fixes | 131794 -> 131794 (delta 0) | same |
| `4ce9fb2138` | docs(533): DECISIONS records each review finding's fix and the fixes' choices | 131794 -> 131794 (delta 0) | same |
| `0170995ce4` | fix(533): an idle worker stops apple-container and microsandbox machines | 131794 -> 131813 (delta +19) | same |
| `164e1297cb` | fix(533): a Firecracker microVM chooses its huge pages at each boot | 131813 -> 131815 (delta +2) | same |
| `b10dbcac8c` | fix(533): a restarted guest ends only the bundle's worker, never itself | 131815 -> 131816 (delta +1) | same |
| `4c740437cd` | fix(533): docker on Linux reaches its Stores through its worker container | 131816 -> 131767 (delta -49) | same |
| `6837bcfce2` | test(533): the cell runner takes its product commit from LFS533_PRODUCT | 131767 -> 131767 (delta 0) | same |

Total: 124168 -> 131767 (delta +7599) across the branch: 124168 -> 130548
(delta +6380) to `b2ca5fceb5`, the commit the review read beside
`707b534eb2`, then +1218 for the review fixes, then +1 for the terminal
pass fixes (`005a30c93c` … `4c740437cd`: +50, then -49 when `docker` on
Linux dropped its in-process adapter). Head recounted at `6837bcfce2` with
`tools/production_loc.py` on `git archive HEAD`: 131767. The 1.1.0 JavaScript
packages are retired in `b07cde28d5` (api 4879 -> 107); the 2.0 library,
machine layer, command and bindings are added in `engine`. The counter does
not count Python (`api/python`).

Step order differs from the README: 7 and 8 before 5 and 6 (step 5 ran in a
subagent worktree), and 10b/10c after 16 (a subagent worktree,
cherry-picked). The review fixes after `b2ca5fceb5` were made on four
fix branches (`issue533-v2-store`, `-home`, `-backups`, `-calls`) and
cherry-picked; each message names its source commit and recounts here.

**`d244cdb3af` (step 10c) left `tests/postgres_connect.rs` broken until the
next commit.** It removed the SQLite-to-PostgreSQL import
(`runtime.store.import_postgresql`) that the test still drove; its message
says so. `9b12a211b6` rewrote the test on Init into PostgreSQL and ran it
1/1 against a throwaway `postgres:17`. At `d244cdb3af` alone,
`--features postgresql` test runs of that file fail.

## Decisions

All are in [DECISIONS.md](DECISIONS.md): G0–G16 and O1–O35 with the
recommended answer, the replaced recommendations (H §7.7 route, O27, G0,
G4, step 5.2, the single branch, and G10's VM resize deferred), the
implementation choices per step, and the review fixes (one row per finding
P0-1 to P2-32; P1-28, the Windows cfg gates, is not done and waits on the
owner). Read especially:

- the **correction** at its end: the store route keeps `docker` on Linux
  in-process, contrary to the H §7.7 row; unverified either way;
- the step-12 deferral of Sandboxes on the `docker` machine and of
  PostgreSQL Stores is **closed** by the owner's instruction (built in
  `a487409fe5`; its own section at the end of DECISIONS);
- `import-env` answers commands for VM and ssh settings instead of acting.

## Tests deleted

- **O27** (`68c6ce368c`): `layerfs-sandbox` `tests/native_platform.rs` (2
  tests) and the Linux half of `native_provider` (listed in that commit).
- **Terminal pass** (`4c740437cd`): `layerfs-sandbox`
  `docker_on_linux_runs_in_process_and_starts_nothing` (the in-process
  route it checked is gone; `docker` on Linux is covered by the worker
  route's tests).
- **Step 14** (`b07cde28d5`): `layerfs-node` `tests/native_admission.rs` (2),
  `native_connection_wait.rs` (1), `native_discovery.rs` (6),
  `native_local.rs` (12), `tests/support/` and `tests/README.md`; and the 63
  test files of the four removed JavaScript packages (listed below).
  The Linux behaviours they guarded (#463 discovery reclaim, #508 admission
  and connection wait) are **not re-covered** on the 2.0 entry point.
- **Step 10c** (subagent `1c05545ce4`, picked as `d244cdb3af`): its JS test
  edits fell with the packages; no Rust test was deleted.

The 63 files under the four packages' `test/` folders that `b07cde28d5`
deleted (`git show --stat --diff-filter=D b07cde28d5`; the commit message
gives only the count). They include the tests' fixtures and the
`verify/` live scripts:

- `api/agent/test/` (@layerfs/agent, 39):
  - `binary_write.mjs`
  - `calls.mjs`
  - `exec_output.mjs`
  - `fixture/cases.mjs`
  - `fixture/fake-docker-init.mjs`
  - `fixture/fake-native.cjs`
  - `fixture/fake-worker.mjs`
  - `fixture/spool-native.cjs`
  - `fixture/website/.gitignore`
  - `fixture/website/.npmrc`
  - `fixture/website/README.md`
  - `fixture/website/app/layout.tsx`
  - `fixture/website/app/page.tsx`
  - `fixture/website/package-lock.json`
  - `fixture/website/package.json`
  - `fixture/website/packages/next/bin/next.js`
  - `fixture/website/packages/next/package.json`
  - `init_refusals.mjs`
  - `native_deployment.mjs`
  - `packaging.mjs`
  - `platform.mjs`
  - `reinit.mjs`
  - `trace.mjs`
  - `verify/calls_real.mjs`
  - `verify/concurrent_unmount_real.mjs`
  - `verify/container.mjs`
  - `verify/daemon_host_real.mjs`
  - `verify/direct_locking_real.mjs`
  - `verify/direct_refusal_real.mjs`
  - `verify/exec_mount_lookup_real.mjs`
  - `verify/exec_output_real.mjs`
  - `verify/fs_mode_real.mjs`
  - `verify/idle_close_real.mjs`
  - `verify/native_unmount_real.mjs`
  - `verify/reap_real.mjs`
  - `verify/sandbox_image_real.mjs`
  - `verify/sandbox_real.mjs`
  - `verify/scenario.mjs`
  - `verify/trace_real.mjs`
- `api/backend/test/` (@layerfs/backend, 2):
  - `backend.mjs`
  - `verify/backend_real.mjs`
- `api/cli/test/` (@layerfs/cli, 3):
  - `cli.test.mjs`
  - `doc-tables.mjs`
  - `verify/cli_real.mjs`
- `api/runtime/test/` (@layerfs/runtime, 19):
  - `fixture/fake-container.cjs`
  - `fixture/fake-lima.cjs`
  - `fixture/fake-msb.cjs`
  - `fixture/fake-ssh.cjs`
  - `runtime.mjs`
  - `verify/cells/cell.mjs`
  - `verify/cells/make.mjs`
  - `verify/cells/mixed.mjs`
  - `verify/cells/oldrec.mjs`
  - `verify/cells/run.mjs`
  - `verify/cells/workloads.mjs`
  - `verify/docker-shim/docker`
  - `verify/macos_real.mjs`
  - `verify/runtime_real.mjs`
  - `verify/ssh_faults.mjs`
  - `verify/stores_real.mjs`
  - `verify/vm_real.mjs`
  - `verify/worker_real.mjs`
  - `vm_placement.mjs`

## Not verified

- Now verified by the seven-kind pass at `6837bcfce2` (TERMINAL-PASS): every
  VM kind and ssh, the docker worker on macOS and Linux, the namespaces
  backend on Linux, the Linux worker, the Store moving native → machine →
  native, machines started by the first call, and (at `0170995ce4`) the idle
  stop. The earlier entries for them here no longer apply.
- Two Lima machines of different sizes with two Stores each, used at once;
  the VM kinds with Docker stopped (Docker is shared with other sessions).
- `remote_store` on Linux; Windows (O5, P1-28 paused).
- Machine boot and Init of a large project against the cell limits: `fc1`
  was warmed by a move before its cells (declared setup).
- R2 backups (no account here).
- Release-build timings: the terminal pass used debug builds.
- One worker serving several Stores, machine boot and worker start times,
  the guest-to-host send speed (README §8).

## Known gaps found late

Found before the review; those the review fixes closed say so (checked
against the fix commits' messages and code at `4194d34001`).

- **Fixed in `085376a7a8`:** the workspace version was still `1.1.0`
  (`layerfs --version`, the bundle cache path `bundle/1.1.0/`); it is 2.0.0.
- `file.read` refuses a binary file (`Binary`) and no 2.0 call reads its
  bytes back.
- **Fixed in `37f1bbd2b9`:** `sandbox.list`/`sandbox.get` accepted
  `page_size`/`page_pos` but returned every item; they page now.
- **Fixed in `37f1bbd2b9`:** exec output over 1 MiB was spooled with no 2.0
  call to read it; it now reaches the caller as output events and the reply
  says `streamed: true`.
- `project.init` on a PostgreSQL Store answers `machine: "native"`.
- No installer places the sealed Linux bundle in the user cache (O16).
  **Partly fixed in `6263b95871`:** the development tool
  `tools/linux_bundle.py` builds the bundle, writes its `bundle.json` and
  places it where LayerFS looks; the launcher's sealed build still writes
  `manifest.json`, and the earlier terminal pass hand-wrote a `bundle.json`
  in a private folder.
- The Node package loads the ignored copy
  `engine/crates/layerfs-node/layerfs.node`. Install a new build with
  `api/scripts/build-native.mjs` (staged copy, then rename): copying over
  the file in place makes macOS kill the next process that loads it.
- `backup.restore` onto another machine answers `Unsupported`; the spec says
  `InvalidInput`.
- A small Init emits no progress event.
- O6's other-named PostgreSQL schemas are not handled; only `layerfs`.
- `workspace exec --stream` and the Node/Python `stdout`/`stderr` callbacks
  get the output after the command ends, not live. (`stream` itself is now
  honoured by Node and Python, `f67d5a9f6f` and `771823d728`, and output
  over 1 MiB arrives as events, `37f1bbd2b9`; both still after the command
  ends.)
- Found while rewriting the skills (the skills document the code as it
  behaves): `workspace.commit` and `sandbox.close` ignored `force` (**fixed
  in `37f1bbd2b9`**, O31 in the host close); a closed Sandbox's record stays until
  `sandbox.close(name)`, so re-creating the name is refused `Exists`;
  `file.grep` matches plain text, not a regular expression; `project.init`
  returns the project record, not `{project, branch, commit}`, and refuses a
  Store off `native` with `Unsupported` (S: `InvalidInput`);
  `branch.get`/`commit.get` refused `NotFound`, not `NoSuch*` (**fixed in
  `88b033cfa9`**, and Store and project in `9a4d9528e0`); a file read at
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
committed). Refreshed after the seven-kind pass; the step-5 and pg subagent
worktrees and their targets listed before are already gone.

| Resource | Size | Note |
| --- | --- | --- |
| `layerfs-issue533-v2/target` | 22 GB | this worktree's macOS target; keep warm only if 2.0 work continues |
| Docker volumes `lfs-dev-layerfs-issue533-v2-5ff20ac6a0aa-target` and `-cargo` | 9.6 GB + 354 MB | this worktree's Linux target and Cargo home (launcher-owned), for the Linux daemon and bundle builds |
| `~/.cache/layerfs-worktrees/layerfs-issue533-v2-5ff20ac6a0aa` | 1.5 GB | launcher state, receipts and two older sealed builds (`build-linux-0c84a1ae…`, `38f27b92…`), both superseded |
| `~/.cache/layerfs/bundle/2.0.0/linux-aarch64` | 2.9 GB | the sealed bundle the seven-kind pass used (`4a77c75e…`, source `6837bcfce2`); its `firecracker/` folder is already removed. Keep it for further VM work; any later seal replaces it |
| `$TMPDIR/lfs-sbx-{home,folder}-{51039,51054}` | small | leftovers of a failed step-12 run; their removal was refused by the auto-mode classifier, so they were left |
| VMs, machines and containers of the pass | – | all removed (TERMINAL-PASS, "Cleanup") |

`python3 tools/cleanup.py check` at handoff: 297.7 GB free (after the
seven-kind pass and its cleanup); no cleanup needed.

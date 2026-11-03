# #535/#536 targeted rerun, lane R2: L2-D1 and the worker record path (apple-container, microsandbox)

> **Status:** Lane receipt, 2026-11-03 09:22–09:27 UTC. Not release evidence; the lead merges it into
> `core/docs/issues/535/TERMINAL-PASS.md`. Product commit `54161c9e2d` (production tree identical to `3e257b91c3`);
> no product change. Reruns only the checkpoints the L2-D1 fix (`17b61af46b`) affects (AGENTS.md §3.4); every other
> L2 result stays as recorded at `../../35ed671b6c/L2/`.

- **Builds (read only):** frozen macOS debug builds `k-macos/` (`PRODUCT` = `54161c9e2d0c…`; `SHA256SUMS` checked OK
  for `bin/layerfs`, `bin/layerfs-daemon`, `node/layerfs.node`, `python/src/layerfs/_native.abi3.so`); the Node
  package ran the D1 script and the cells. Sealed Linux bundle `k3-bundle/2.0.0/linux-aarch64` through
  `LAYERFS_BUNDLE` (`bundle.json` `hash` = `185b0240…035b0d`, as briefed). No Linux build of my own.
- **Machines**, made once in one shared 0700 scratch home (removed at the end), `setup.log`:
  `lfs535r2a` = `machine create … apple-container` (12.8 s; container CLI 1.6.1), `lfs535r2m` = `machine create …
  microsandbox` (34.5 s; msb 0.7.11). Two VMs at once, 2 CPUs / 4 GiB each. No PostgreSQL container (sqlite only).
- **Runner:** `d1/d1.mjs` (new, one attempt per call) for the D1 scenario; `scripts/cells.sh` (L2's, sqlite only:
  one cell per command, 115 s `perl alarm`, machine + cell-number locks) for the cells; `scripts/run.sh` (L2's) for
  CLI steps. Nothing ran concurrently in this lane (all steps sequential).
- **Disk** (`tools/cleanup.py check`): 225.44 GB free at start, 216.97 GB at the end (other lanes run alongside;
  this lane leaves no VM, volume or Store). **Load (1 min):** 7.29 at start, 6.11 at the end; 7.5–8.7 in the cells.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`. All Stores Disposable.

## Verdicts

| Item | Platform | Result | Evidence / note |
| --- | --- | --- | --- |
| L2-D1 rerun: native sqlite Store, `store push`, `project.init` through the worker (folder on the Mac), `sandbox.create`, mount forked `lane`, exec writes a file, Commit, `file.read` from the Mac | apple-container `lfs535r2a` | **PASS** | `d1/apple-container-d1.json` (attempt 2): Init `machine: lfs535r2a`, progress `check, send, send, install, record`; `sandbox.create` running; exec `exit_code 0`; `Committed`; `file.read lane /d1.txt` → `written in d1a2sb on lfs535r2a` |
| idem | microsandbox `lfs535r2m` | **PASS** | `d1/microsandbox-d1.json`: same steps, `Committed`, read back `written in d1msb on lfs535r2m` |
| `project.remove` refused `InUse` with `detail.related` while the Sandbox lives | apple-container | **PASS** | `InUse` "Sandbox d1a2sb of d1a2/p is running", `related [{kind: sandbox, name: d1a2sb}]` |
| idem | microsandbox | **PASS** | `InUse` "Sandbox d1msb of d1m/p is running", `related [{kind: sandbox, name: d1msb}]` |
| `project.remove` allowed after `sandbox.close` | apple-container | **PASS** | `{project: d1a2/p, stack: …, packs_kept: true}` |
| idem | microsandbox | **PASS** | `{project: d1m/p, stack: …, packs_kept: true}` |
| sqlite C01, C12 (Init natively, push, pull back) | apple-container | **2/2 PASS** | `apple-container-sqlite-C01.json`, `-C12.json` |
| idem | microsandbox | **2/2 PASS** | `microsandbox-sqlite-C01.json`, `-C12.json` |
| worker Store folders after the runs | both | clean | `worker-store-folders.log`: `/layerfs/data/store` empty on both VMs after every Store was pulled back |

**L2-D1 verdict: FIXED (PASS on apple-container and microsandbox).** The scenario that was refused `NoSuchProject: no
complete project manifest` at `35ed671b6c` (`../../35ed671b6c/L2/fixes/worker-init-then-sandbox.log`) now runs end to
end on both VM workers, and the changed worker record path does not disturb the native-Init-then-push cells.

### Script attempts (all kept)

- `d1/apple-container-d1-attempt1.json` (Store `d1a`): every product call up to and including `file.read` succeeded
  (`sandbox.create` running, `Committed`, read back `written in d1asb on lfs535r2a`), then my script threw passing
  `file.read`'s answer object (`{kind, text, …}`) to `Buffer.from`; the `project.remove` checks never ran, and its
  `finally` closed the Sandbox, pulled and removed the Store. A script defect, not a product one. Fixed (read `.text`)
  and run again as attempt 2 with Store `d1a2`.
- `d1/apple-container-d1.json` (attempt 2): all five checks PASS, but its overall `verdict` field says `FAIL`
  because the script counted 4 checks where there are 5 (script defect). Corrected to 5 before the microsandbox run,
  whose overall `verdict` is `PASS`. The receipt is kept unedited.

## Cells

| Cell | Result | timed exec (ms) | Commit (ms) | push (ms) | pull (ms) | load1 | start (UTC) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| apple-container-sqlite-C01 | PASS | 595.3 | 540.5 (Committed) | 88.9 | 172.3 | 8.7 | 09:25:42 |
| apple-container-sqlite-C12 | PASS | 330.0 | 242.7 (Committed) | 90.0 | 111.4 | 7.9 | 09:25:57 |
| microsandbox-sqlite-C01 | PASS | 702.1 | 603.6 (Committed) | 59.3 | 156.0 | 7.7 | 09:25:58 |
| microsandbox-sqlite-C12 | PASS | 425.0 | 299.5 (Committed) | 59.5 | 107.3 | 7.5 | 09:26:03 |

Timing is diagnostic; the verdict is PASS/FAIL.

## Product defects found

None.

## Resources

Created and removed: machines `lfs535r2a` (apple-container) and `lfs535r2m` (microsandbox) by `machine remove`
(afterwards `container list -a`, `container volume list`, `msb list`, `msb volume list` show nothing of this lane;
`cleanup.log`); every Store (`d1a`, `d1a2`, `d1m`, `c01`, `c12`) pulled back and removed (`store list` empty); the
shared home and lock folder in scratch. Left: nothing but this folder.

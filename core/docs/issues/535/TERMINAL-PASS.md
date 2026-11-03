# #535 + #536 combined terminal pass

> **Status:** Terminal pass record, 2026-11-03. Covers #535 §10 phase 8
> items 1–9, #536 §12 items 2–7 and the #536 §10b contract
> ([HANDOFF-2 §6](HANDOFF-2.md#6-combined-terminal-pass)). Receipts are under
> [`receipts/`](receipts/). Disposable Stores only. Durable: `NOT_RUN — disabled
> by owner until explicit reauthorization`. Nothing pushed.

## Verdict

**PASS.** Every #535 phase 8 item, #536 §12 item and the §10b contract pass at the final product commit `32aec70a64`, with three recorded spec differences (#535 item 6 R2; the §10b busy and unresolved rows) and only the NOT_RUN cases HANDOFF-2 §6 allows (Durable, Windows).

The pass ran three times on this Mac (arm64, 14 CPUs, Docker Desktop 29.5,
Lima 2.2.1, container CLI 1.6.1, msb), each time in three parallel lanes
(subagents in their own worktrees):

| Run | Product commit | Bundle | What it is |
| --- | --- | --- | --- |
| First | `fe6f85013b` | `a1f0d886…` | The full pass; its defects are fixed after it (table below) |
| Final | `35ed671b6c` | `764e4977…` | The full pass again on the fixed tree; found 2 new defects (L3 D-3, L2-D1) |
| Targeted rerun | `3e257b91c3` (receipts at `54161c9e2d`, same product tree) | `185b0240…` | Only the checkpoints those two fixes touch (AGENTS.md §3.4: reuse unaffected evidence, rerun the affected checkpoint) |
| Last case | `32aec70a64` | `185b0240…` | The final run's one §10b NOT_RUN case, run by the lead; it found and fixed `machine.remove` `InUse` without `related` |

**Final product commit: `32aec70a64`** (production LOC 136044). Every later
commit on the branch is receipts or docs. The final run's evidence holds for
`32aec70a64` except where the three later fixes reach: `3e257b91c3` changes only
the docker machine's readiness wait on a Linux Engine (Docker Desktop passes
the same port as before) and `17b61af46b` changes only how a worker merges a
project record that has no `created` (written only by Init through a worker);
`32aec70a64` changes only the `detail` of the caller's `machine.remove` and
`machine.disconnect` `InUse` refusal. Lanes R2 and R3 rerun the first two
paths, the lead checked the third live; the lead reran the full macOS check
set and test sweep at `3e257b91c3`, and the check set and the control and api
tests again at `32aec70a64`.

Binaries: debug builds (`builds/k-macos/`: CLI, macOS daemon, Node and Python
packages, `SHA256SUMS`), the sealed Linux bundle (`tools/linux_bundle.py
seal`) with the Firecracker kit added after sealing, and Linux builds made in
the lanes from the product commit (hashes in L3's and R3's RESULTS). Every
time in a receipt is diagnostic; verdicts are PASS/FAIL. One sample per cell;
a second attempt only after a diagnosis, both receipts kept.

PostgreSQL (owner D12): each lane ran its own throwaway `postgres:17` on the
Mac in Docker Desktop, published on `0.0.0.0`. "Loopback" cells use
`postgres://postgres@127.0.0.1:<port>/<cell>`, which each machine's `reach`
rewrites; "remote" cells use the Mac's LAN address `192.168.2.123:<port>`
dialled as given, standing in for a remote server (there is no second host).
ssh refuses a loopback URL by design (recorded once per run).

## Items (final run, with the targeted rerun)

| # | Item | Verdict | Evidence |
| --- | --- | --- | --- |
| 535-1 | sqlite cells on all seven platforms and the Linux rows | PASS | L1, L2, L3, R3 |
| 535-2 | PostgreSQL cells, loopback and remote, every enabled platform | PASS | L1, L2, L3, R3 |
| 535-3 | Many Stores per machine (native and one VM: 2 sqlite + 2 PostgreSQL, 4 Sandboxes at once) | PASS | L1 item 2 (native), L2 item 3 (microsandbox) |
| 535-4 | Refusals match `support.rs`, `SUPPORT.json` and `machine platforms` | PASS | L1 item 3 (macOS, 68 receipts), L3 item 4 (Linux: 7/7 rows, lima/apple-container/firecracker creates refused in the table's words) |
| 535-5 | Contracts (no `kind == postgresql` in api, no `LAYERFS_KERNEL` in code, `DaemonSetup` carries `DaemonStore`/`KernelRoute`, boundary guard) | PASS | L1 item 4 (`LAYERFS_KERNEL` only in historical Markdown) |
| 535-6 | Roundtrips: R1 one worker connection per worker-routed Store call | PASS | `LAYERFS_TRACE` worker-connection lines: L1 (docker), L2 (apple-container, with guest TCP counters agreeing), R3 (Linux docker) |
| 535-6 | Roundtrips: R2 no worker hop for `workspace.exec` on a VM | FAIL, recorded spec difference | exec is one `sandbox.host` connection to the worker (536 DECISIONS: exec stays on the worker); not counted against the pass |
| 535-7 | Binding parity (CLI, Node, Python) on native and one VM | PASS | L1 item 5 (native), L2 item 7 (apple-container), C01 C05 C09 C12 identical |
| 535-8 | Checks: the §11 set on the final commit | PASS | `receipts/3e257b91c3/lead/` (full set and sweep), `receipts/32aec70a64/lead/` (set, control and api again) |
| 535-9 | Machine power on every VM platform and ssh | PASS | L2 item 9 (apple-container, microsandbox, ssh), L3 item 9 (lima, firecracker); cells pass after start |
| 536-2 | Conversion from 2.0 (native and one VM; refused while a Sandbox runs; 12 cells after on both) | PASS | L1 item 6 (docker stands in for the VM: a 2.0 VM worker would need a 2.0 bundle in a VM; 24/24 after) |
| 536-3 | Intents (push, pull, sandbox.create killed at every phase; reads change nothing; the clearing call clears) | PASS | L1 item 7 (22 kills, 22/22 reads changed nothing) |
| 536-4 | Locks | PASS | L1 item 8 |
| 536-5 | Layout (no Sandbox record outside `sandbox/<name>/`, no `store/*/project/*/sandboxes/`, doctor clean) | PASS | L1 item 9, L2 (worker roots), R2 (`worker-store-folders.log`) |
| 536-6 | Size (`layerfs-api/src` holds only `calls/` and `lib.rs`; deleted 1.1 paths in the commit table) | PASS | L1 item 9; [HANDOFF.md](HANDOFF.md) commit table |
| 536-7 | Checks | PASS | as 535-8 |
| 536 §10b | Contract for UIs | PASS with two recorded spec differences | L1 item 10: 101 PASS, 3 FAIL (the busy-row and unresolved-row Store calls, recorded in 536 DECISIONS), 1 NOT_RUN (`related` on `machine.remove`: docker is built in); that case run by the lead at `32aec70a64` on microsandbox: PASS (`receipts/32aec70a64/lead/`) |

### Cells

| Platform | sqlite | PostgreSQL loopback | PostgreSQL remote | Lane |
| --- | --- | --- | --- | --- |
| native (macOS) | 12/12 | 12/12 | – | L1 |
| docker (Docker Desktop) | 12/12 | 12/12 | 12/12 | L1 |
| apple-container | 12/12 | 12/12 | 12/12 | L2 |
| microsandbox | 12/12 | 12/12 | 12/12 | L2 |
| ssh (Lima VM as the host) | 12/12 | refused by design | 12/12 | L2 |
| lima | 12/12 | 12/12 (192.168.5.2) | 12/12 | L3 |
| firecracker (nested in lima) | 12/12 | 12/12 (192.168.5.2) | 12/12 | L3 |
| Linux native | 12/12 | – | 12/12 | L3 |
| Linux docker | 12/12 | – | 12/12 | R3 (L3 was 0/24 at `35ed671b6c`: D-3) |

Every VM platform's sqlite cells push the Store out of `native` and pull it
back. Every PostgreSQL cell's Commit was looked up in its own database (L2: 55
found, 0 missing; C03 is `UpToDate`).

## Defects found and fixed

| Run | Defect | Fix |
| --- | --- | --- |
| First (K2) | microsandbox could not reach private addresses (D1) | `4b00569b55` (`--net public,private,host`) |
| First (K2) | a reconnected PostgreSQL Store name could reuse the old database's open Store (D2) | `2d607fc066` (open-Store key includes the binding) |
| First (K2, K1) | `store push` after a CLI `branch fork` refused for an unfinished WAL (D3) | `9fd05b8367` (the CLI closes its Stores) |
| First (K3) | Linux docker worker dialled the bridge gateway, which does not reach 127.0.0.1-published ports (D-1, 0/24) | `8b1474bd97` (`--network host`) |
| First (K3) | Linux `machine create` refusals not in the table's words (D-2) | `fbd4e90629` |
| First (K2) | no `LAYERFS_TRACE` in 2.0 (R1 not showable) | `d09b5deb60` (worker-connection trace) |
| First (K3) | a missing PostgreSQL database read as "no connection" | `cc216bf638` |
| First (K1, K2) | `workspace mount <sb> main` answered `NoSuchBranch` | `d47f8a3fc5` (owner D14: refused, only side Branches mount) |
| First (K3) | Linux `machine list` showed docker `ready` beside status `stopped` | `01bfb7ee93` |
| First (K1) | `store.status`/`store.list` changed the home; Init progress not §10b; `InUse` without `related` | lane F2, merged `3a58d861f6` |
| First (K1) | push/pull crash at install recorded as certain; crash at retire left both copies; worker roots kept project records | lane F1, merged `f250732c76` |
| Final (L3) | Linux `machine start docker` waited on port 7070 (D-3, 0/24 again) | `3e257b91c3` |
| Final (L2) | a project Inited through a worker could never get a Sandbox (L2-D1) | `17b61af46b` |
| Last case (lead) | `machine.remove`/`machine.disconnect` `InUse` had `detail: null` | `32aec70a64` |

Open, not fixed (536 DECISIONS open items): `store.disconnect` of a PostgreSQL
Store leaves records on worker roots (L2-D2); `store remove` deletes only the
record machine's copy after a transfer stopped at retire; the Node package
leaves its Store leases at exit (doctor warns; the next writer prunes); on
docker a readiness failure's log tail is empty.

## Not run

- Durable: disabled by owner until explicit reauthorization.
- Windows: no host.

## Receipts

- First run: [`receipts/fe6f85013b/`](receipts/fe6f85013b/) K1, K2, K3.
- Final run: [`receipts/35ed671b6c/`](receipts/35ed671b6c/) L1, L2, L3.
- Targeted rerun and lead checks: [`receipts/3e257b91c3/`](receipts/3e257b91c3/) R2, R3, lead.
- Last case and lead checks: [`receipts/32aec70a64/lead/`](receipts/32aec70a64/lead/).

Each lane folder has `RESULTS.md` (every item, cell and attempt, first errors,
machines, resources, disk and load), per-cell JSON receipts from
`api/node/test/verify/cells/cell.mjs` and its scripts.

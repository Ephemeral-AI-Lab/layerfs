# #535 + #536 handoff: connect layer and `layerfs-control`, through the terminal pass

> **Status:** Dated handoff, 2026-11-03; not release evidence or a product
> contract. Closes the work [HANDOFF-2](HANDOFF-2.md) assigned. Nothing is
> pushed, merged into `layerfs_v2_0_0` or commented on GitHub.

## Where it is

| | |
| --- | --- |
| Worktree | `~/Ephemeral-AI-Lab/layerfs-issue535-connect` (registration `layerfs-issue535-connect-7500a2864564`) |
| Branch | `issue535-connect` (local only) |
| Base | `layerfs_v2_0_0` at `58b8d1742b` |
| Final product commit | **`32aec70a64`** (production LOC 136044); later commits are receipts and docs |
| Branch head | the commit that adds this file (`git log -1 -- core/docs/issues/535/HANDOFF.md`) |
| Terminal pass | [TERMINAL-PASS.md](TERMINAL-PASS.md): **PASS** |

Production LOC: 131936 at `58b8d1742b`'s product tree as counted with the
pinned `tools/production_loc.py` → 136044 at `32aec70a64` (delta +4108).
Scope: first-party product source under core, engine, connect and api
(`tools/production_loc.py`, inline tests excluded); the reference product is
absent (retired R9c; last counted 65,417 at tree 498dd1917812).

## What passed

Every #535 phase 8 item (1–9) and #536 §12 item (2–7) passes, and the #536
§10b contract holds, with these recorded spec differences (536 DECISIONS,
"Differences between the spec and the code"), not counted against the pass:

- #535 item 6 R2: `workspace.exec` on a VM is one worker connection (exec
  stays on the worker).
- §10b busy row: Store calls are refused `Busy` during a push or pull.
- §10b unresolved row: Store calls are refused `Unresolved` until the
  clearing call.

## NOT_RUN

- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.
- Windows: no host (allowed by HANDOFF-2 §6).

## Decisions taken this round

Owner: D12 (PostgreSQL servers live on the host), D13 (G7 routes built for
apple-container, microsandbox, firecracker), D14 (only side Branches are
mounted), H11a (Init from a folder on the caller's host). Implementation
choices and the fix lanes' choices are in [535 DECISIONS](DECISIONS.md) and
[536 DECISIONS](../536/DECISIONS.md).

## Open items

From 535 and 536 DECISIONS "Open items", the ones the terminal pass touched:

- `store.disconnect` of a PostgreSQL Store leaves `store/<s>/` records on the
  worker roots of machines that ran its Sandboxes (L2-D2); a later sqlite Store
  of that name pushed there has `project init` refused `Exists` until a pull.
- `store remove` deletes only the record machine's copy after a transfer
  stopped at retire.
- The Node package leaves its Store leases at process exit (the CLI now closes
  its Stores); doctor warns and the next writer prunes them.
- A push or pull that clears an unresolved op in the opposite direction exits
  125 after clearing (it then refuses as documented), which reads like a
  failure to a caller.
- On docker, a worker readiness failure shows an empty log tail (the worker
  logs to `docker logs`).
- Push/pull `send` is one worker request per 8 MiB chunk; #535 R8's streamed
  single connection is not built.

## Commits

Every commit on `issue535-connect` since `58b8d1742b`, in order, with its
production LOC recounted from its first parent's tree and its own tree with
the pinned `tools/production_loc.py` (`scratchpad/recount.py`, cached by
product tree). Merge commits are compared with their first parent. Where a
message's LOC line differs from the recount, the recount is right; earlier
differences are also in the DECISIONS "Corrections to commit messages" tables.

| Commit | Subject | Before | After | Delta |
| --- | --- | --- | --- | --- |
| `0e12ca6a38` | docs(535): the spec and the handoff prompt | 131936 | 131936 | +0 |
| `451916521e` | docs(535): phase 0, the decisions log and the connect layer guides | 131936 | 131936 | +0 |
| `aacdd17433` | refactor(535): move layerfs-sandbox into connect/crates (1.1) | 131936 | 131936 | +0 |
| `4398b0fd41` | refactor(535): move the machine layer into connect/crates/layerfs-machine (1.2) | 131936 | 131955 | +19 |
| `7035f0c71b` | refactor(535): one folder per platform in layerfs-machine (1.3) | 131955 | 132087 | +132 |
| `77c59c524a` | refactor(535): Firecracker's host pieces in platforms/firecracker/host.rs (1.4) | 132087 | 132097 | +10 |
| `1f40eb9e92` | refactor(535): the machine Kind is Platform (1.5) | 132097 | 132099 | +2 |
| `0cada61acc` | refactor(535): layerfs-store-backup, the bucket drivers and Store backup in connect (1.6) | 132099 | 132099 | +0 |
| `236b7aceb2` | refactor(535): one link.rs for a Store's bucket link (1.6b) | 132099 | 132097 | -2 |
| `990e43f166` | refactor(535): layerfs-mount-ports, the route-neutral kernel ports under FUSE and NFS (1.7) | 132097 | 132093 | -4 |
| `0106ba8285` | refactor(535): split backend/native into local/ and backend/host_process/ (1.8) | 132093 | 132098 | +5 |
| `791d510b18` | refactor(535): the host-process backend's Rust names (1.9) | 132098 | 132100 | +2 |
| `72e7441731` | docs(535): phase 1 recorded in the decisions log | 132100 | 132100 | +0 |
| `7a35d4b655` | docs(535): the owner's revised spec (G3 without attach, section 9a) | 132100 | 132100 | +0 |
| `dffd241aee` | feat(535): DaemonSetup carries DaemonStore and KernelRoute; LAYERFS_KERNEL is gone (2a) | 132100 | 132274 | +174 |
| `c7e14c15a3` | feat(535): one StoreSelection and one Handles; OpenedStore::file(); no fake Store path (2b) | 132274 | 132399 | +125 |
| `3e4fb5d66c` | docs(535,536): interleaved order, #536 decisions, historical records restored (step A) | 132399 | 132399 | +0 |
| `48a50e4950` | fix(535): drop two imports 2b left unused under the postgresql feature | 132399 | 132397 | -2 |
| `86d4ec0652` | feat(535): StoreRef, a Store's kind decided once from its record (2c) | 132397 | 132487 | +90 |
| `42775643f9` | docs(535): phase 2 done; 2c choices and a commit-message correction | 132487 | 132487 | +0 |
| `56f6831a0e` | refactor(535): one Profile per platform; Platform reads it; options declared (B) | 132399 | 132638 | +239 |
| `1082648ca7` | refactor(535): sealed platform Adapter; machine calls and probe dispatch through it (B) | 132638 | 132869 | +231 |
| `f1f272777e` | docs(536): spec facts the code contradicts, and the step C order that builds | 132487 | 132487 | +0 |
| `0a71736fd0` | refactor(535): sealed BackendAdapter for what is fixed per Sandbox backend (B) | 132869 | 132962 | +93 |
| `633e53f8b1` | feat(535): one support table; connect/SUPPORT.json generated from it (B) | 132962 | 133191 | +229 |
| `1b74839960` | Merge lane B (#535 phase 3, connect side): Profile, Adapter, BackendAdapter, support table | 132487 | 133279 | +792 |
| `a6fea806de` | refactor(536): move the SDK Store-call facade into store-service methods/ | 132487 | 132488 | +1 |
| `53c326e0bc` | feat(535): machine.platforms, per-machine supports, refusals from the support table (B) | 133279 | 133332 | +53 |
| `c73fda2d57` | Merge lane C1 (#536 step 1.1): the SDK Store-call facade in store-service methods/ | 133332 | 133333 | +1 |
| `4a69bd6565` | docs(535,536): step B and C1 records; engine AGENTS names store-service methods/ | 133333 | 133333 | +0 |
| `23786fbd52` | refactor(535): PostgresUrl, the loopback-URL split desktop_url used | 133333 | 133364 | +31 |
| `17c17a188c` | refactor(536): move management code into layerfs-control (C2, crate move) | 133333 | 133369 | +36 |
| `bec9cc372c` | refactor(536): control common/: names, outcome, trace, json, context, home (C3) | 133369 | 133381 | +12 |
| `87bd0bfaff` | feat(535): Adapter::reach, the PostgreSQL URL a machine's daemons dial (5a) | 133364 | 133488 | +124 |
| `2f35e9d6cb` | feat(535): docker Sandbox containers on Linux get host.docker.internal (5a) | 133488 | 133495 | +7 |
| `7f51e79799` | refactor(536): control machine/: worker/ under machine, calls split per op (C4) | 133381 | 133456 | +75 |
| `510fd6400c` | feat(535): namespaces Sandboxes take binds; none for a PostgreSQL Store (5a) | 133495 | 133621 | +126 |
| `936fd2175b` | refactor(536): control store/, project/, backup/ by entity (C5) | 133456 | 133491 | +35 |
| `04fd094d02` | feat(535): support table enables PostgreSQL Sandboxes on native, docker, ssh (5a) | 133621 | 133617 | -4 |
| `ba00cddfea` | Merge lane G-connect commits 1-4 (#535 phase 5a, connect side): reach, binds, host-gateway | 133333 | 133621 | +288 |
| `3a0f71cc7f` | docs(535): G-connect merge record, held table flip, wrappers to switch | 133621 | 133621 | +0 |
| `2997ad6710` | refactor(536): control sandbox/, workspace/, daemon/ by entity (C6) | 133491 | 133509 | +18 |
| `210fb14a44` | test(536): control-internal tests move to layerfs-control (C7) | 133509 | 133509 | +0 |
| `4fd096335e` | Merge lane C (#536 step 1, C2-C7): layerfs-control, organised by entity | 133621 | 133797 | +176 |
| `72ed4cfbf7` | docs(536): step C done; api AGENTS names layerfs-control | 133797 | 133797 | +0 |
| `59979c4181` | feat(536): typed worker op project.remove replaces host.call/host.backend (D1) | 133797 | 133779 | -18 |
| `4e7e9cd1bd` | refactor(536): delete the 1.1 runtime.*/backend.* tables and what only they reach (D2) | 133779 | 132180 | -1599 |
| `45c5c9aa80` | refactor(536): delete the dead daemon/internal.rs wrappers (D3) | 132180 | 132161 | -19 |
| `5f255a90bf` | refactor(536): delete the test-only Sandbox launch facade and its examples (D4) | 132161 | 132043 | -118 |
| `26b453f38c` | Merge lane D (#536 step 2): typed project.remove; the 1.1 paths deleted | 133797 | 132043 | -1754 |
| `b2a1f0c796` | chore(536): guard the retired #536 paths; step D record | 132043 | 132043 | +0 |
| `9eed8f696e` | refactor(535): one executor; place::plan decides where a call runs (E1, R4) | 132043 | 132100 | +57 |
| `e8d330116e` | feat(535): no ping before a worker call; start only when nothing was sent (E2, R1) | 132100 | 132167 | +67 |
| `5fd14480df` | feat(535): no list, get or status call starts a machine (E3, P4) | 132167 | 132232 | +65 |
| `c6f3dc96d9` | feat(535): machine.start and machine.stop (E4, P1) | 132232 | 132323 | +91 |
| `9de6d4c77d` | feat(535): the worker writes a Store record only when it changed; built-ins once per home (E5, R5, R6) | 132323 | 132349 | +26 |
| `ae4d9de019` | refactor(535): one deploy resolver, run on the side that runs the call (E6, G8) | 132349 | 132311 | -38 |
| `aed6ad02a8` | feat(535): docker's default image is the bundle's, loaded once (E7, P5) | 132311 | 132411 | +100 |
| `30842a0560` | Merge lane E (#535 phase 4 in layerfs-control): one executor, machine power, no-start reads | 132043 | 132411 | +368 |
| `44623d029a` | docs(535): step E (phase 4) record | 132411 | 132411 | +0 |
| `5caf281e1c` | feat(536): typed names, checked once (F1a, §9.2) | 132411 | 132488 | +77 |
| `72ad49c7d0` | refactor(536): move machine/record.rs to machine/spec.rs (F1b, move) | 132488 | 132494 | +6 |
| `88fd08ef63` | refactor(536): move the bucket record helpers to backup/record.rs (F1c, move) | 132494 | 132501 | +7 |
| `5c7a26d7ce` | feat(536): typed Store, project and bucket records (F1d, §9.2) | 132501 | 132671 | +170 |
| `908d4a3af1` | feat(536): typed machine and Sandbox records, inventory (F1e, §9.2) | 132671 | 132808 | +137 |
| `b03e02feb1` | feat(536): state() per entity; store.list items carry state (F1f, §7.2, §10b) | 132808 | 132869 | +61 |
| `6d1ab2520b` | refactor(536): one function per call in the sandbox and backup families (F1g) | 132869 | 132874 | +5 |
| `810dc2e115` | refactor(536): split the store, backup and sandbox families per call (F1h, move) | 132874 | 133028 | +154 |
| `41a2a00525` | Merge lane F1 (#536 phase 3): typed names and records, inventory, state() | 132411 | 133028 | +617 |
| `ac606897eb` | docs(536): step F1 record and a merge-message correction | 133028 | 133028 | +0 |
| `9bdbebbc95` | refactor(536): seal the store-backup Driver trait (§9.4, G2) | 133028 | 133033 | +5 |
| `503dda0bef` | test(536): Sandbox backend conformance kit (step 9) | 132411 | 132411 | +0 |
| `c4b135989c` | test(536): platform conformance kit (step 9) and its README paragraph | 132411 | 132411 | +0 |
| `45b804329c` | fix(535): let a PostgreSQL Store take project.add again; observed test uses the fixture's Store locator | 133033 | 133033 | +0 |
| `b055085daf` | Merge issue535-kit: platform and Sandbox backend conformance kits (#536 step 9) | 133033 | 133033 | +0 |
| `78bd14c325` | fix(535): firecracker refuses a loopback Store URL; G7 reasons name the measured facts | 133033 | 133033 | +0 |
| `b7004e5547` | refactor(536): split home/migrate into migrate/{open,v110,volume} (F2a, move) | 133028 | 133037 | +9 |
| `3418e21068` | docs(535,536): G7 reach receipt; verify, kit and probe lane records | 133033 | 133033 | +0 |
| `fb31a90a88` | refactor(536): move the runtime-record reader out of sandbox/files.rs (F2b, move) | 133037 | 133052 | +15 |
| `1eb7919076` | feat(535): PostgreSQL Sandboxes run where their machine runs them, with a reach check (5a, api side) | 133033 | 133103 | +70 |
| `b5a66257c8` | test(535): store_routes expects Unavailable for a worker that refuses the secret (R1) | 133103 | 133103 | +0 |
| `c45e2b4ed1` | Merge issue535-G-flip: the support table enables PostgreSQL Sandboxes on native, docker and ssh (5a) | 133103 | 133099 | -4 |
| `88079bab6d` | refactor(535): delete the step-G compatibility wrappers (namespaces `store` option, docker::desktop_url) | 133099 | 133064 | -35 |
| `a90298f5bc` | docs(535): step G api side in the 2.0 manual, the SDK skill and DECISIONS | 133064 | 133064 | +0 |
| `6579bd7d95` | docs(535): record owner decision D11, manual machine lifecycle | 133064 | 133064 | +0 |
| `2632946760` | refactor(535): one place builds an ssh forward through a Lima instance | 133064 | 133069 | +5 |
| `db82b917e7` | feat(536): home layout 3: one folder per Sandbox, thin guest records, the layout 2 conversion (F2c) | 133052 | 133281 | +229 |
| `44891cec60` | feat(535): machine::forward carries a Sandbox daemon's port to the host (phase 6, G4) | 133069 | 133165 | +96 |
| `6704207235` | Merge issue535-connect (6579bd7d95, step G api side) into issue535-F2-layout | 133281 | 133317 | +36 |
| `444d12da21` | feat(536): Workspace names: mount { name }, <sandbox>:<name> in every workspace call (F2d) | 133317 | 133430 | +113 |
| `9ce7dd5533` | Merge lane F2 (#536 phase 4): home layout 3, the layout 2 conversion, Workspace names | 133064 | 133430 | +366 |
| `16b4c4a329` | docs(536): record step F2 (layout 3) and its differences from the spec | 133430 | 133430 | +0 |
| `a5ef188cf7` | feat(535): D11 part 1, a worker never stops by itself (no idle stop, no guest power-off) | 133430 | 133239 | -191 |
| `b6261beee5` | feat(535): D11 part 2, no call starts a machine; a stopped one is refused naming `layerfs machine start` | 133239 | 133159 | -80 |
| `c2a4808113` | feat(535): D11 part 3, a Sandbox daemon never closes itself; a guard keeps idle stop and on-demand start out | 133159 | 133082 | -77 |
| `a6e707de3f` | docs(535): D11 manual machine lifecycle in the manual, skills and architecture docs | 133082 | 133082 | +0 |
| `eeba429d7f` | Merge lane H-forward: machine::forward (phase 6, connect side) | 133082 | 133183 | +101 |
| `e306053c99` | feat(536): common/op.rs, the one in-flight intent, and per-entity locks (F3 base) | 133183 | 133340 | +157 |
| `35e8a36023` | docs(535): record the H merge, the F3 base and D11's supersession of #536 §10b | 133340 | 133340 | +0 |
| `7f8f049bf9` | feat(536): machine records hold one op intent instead of creating/removing | 133340 | 133405 | +65 |
| `5e6a2b6f62` | feat(536): the layout 2 conversion leaves no machine intent field | 133405 | 133424 | +19 |
| `c564260d55` | docs(536): machine op intent, Busy and Unresolved in the 2.0 manual and skill | 133424 | 133424 | +0 |
| `dc08ba6d98` | docs(536): MachineRecord::of's comment names the op, not a removal | 133424 | 133424 | +0 |
| `77aabd34d1` | feat(536): Sandbox records hold one op intent; state is derived (F3) | 133340 | 133445 | +105 |
| `aa2dfe25d1` | feat(536): Store records hold one `op` intent under the Store's lock (F3 store) | 133340 | 133413 | +73 |
| `e60d3b7f85` | Merge lane F3-machine: machine records hold one op intent | 133340 | 133424 | +84 |
| `5c737e62cf` | feat(536): PostgreSQL Store calls hold the Store's lease (F3 store) | 133413 | 133415 | +2 |
| `8fc6fb9b71` | docs(536): Store `op`, states and the Store lock in the manual and skills (F3 store) | 133415 | 133415 | +0 |
| `7a94ab1e94` | fix(536): the serving turn is serve.lock; the Sandbox lock guards its records (F3) | 133445 | 133449 | +4 |
| `5fe98cad09` | docs(536): Sandbox op intent, derived state and the Sandbox lock (F3) | 133449 | 133449 | +0 |
| `045f07faed` | Merge lane F3-store: Store records hold one op intent under the Store lock | 133424 | 133499 | +75 |
| `6f234c2148` | fix(536): a create whose claim write fails leaves the name free (F3) | 133449 | 133449 | +0 |
| `1b7f7858c5` | Merge lane F3-sandbox: Sandbox records hold one op intent; state is derived | 133499 | 133608 | +109 |
| `3ffb73cfa5` | docs(536): F3 in the error table, the home's lock rules and both DECISIONS | 133608 | 133608 | +0 |
| `aa1a992fcd` | feat(536): project.init into a sqlite Store on another machine runs in its worker (H11) | 133608 | 133676 | +68 |
| `face3ccf80` | docs(536): record the H11 folder choice and correct aa1a992fcd's per-file LOC split | 133676 | 133676 | +0 |
| `ca25b1b0ba` | feat(536): store.push and store.pull replace store.move (H1, §7.3) | 133608 | 133671 | +63 |
| `0b4b7223dc` | feat(536): the worker's hello says its protocol and bundle; another build is sent nothing | 133608 | 133734 | +126 |
| `2ebd2cbca6` | feat(536): push and pull check the target's room and name what holds a Store (§7.3, §10b, §11 item 7) | 133671 | 133727 | +56 |
| `bd0ea149c3` | feat(536): push and pull progress carries bytes only while sending (§10b) | 133727 | 133737 | +10 |
| `0fdb99b768` | Merge lane I-push: store.push and store.pull replace store.move | 133676 | 133805 | +129 |
| `af47630cef` | refactor(535): a live Sandbox's runtime may be absent; mount and status take their Store reads | 133608 | 133636 | +28 |
| `ad2c9cf351` | feat(536): layerfs doctor's report in layerfs-control (H7) | 133734 | 134341 | +607 |
| `23145ced3e` | feat(536): `layerfs doctor`, layerfs.doctor() in Node and Python (H7) | 134341 | 134416 | +75 |
| `d58d064b67` | test(536): the shell/package parity test expects per-process Store state; record push/pull choices | 133805 | 133805 | +0 |
| `859be8c875` | Merge lane I-doctor: worker hello version and bundle; layerfs doctor | 133805 | 134613 | +808 |
| `c4260f30a5` | docs(536): record the worker hello and doctor choices | 134613 | 134613 | +0 |
| `0c285e87a7` | feat(535): Workspace calls reach a forwarded Sandbox's daemon directly (G4, phase 6) | 133636 | 134071 | +435 |
| `5c8afa976b` | docs(535): direct Workspace calls on ssh, lima and docker (G4) | 134071 | 134071 | +0 |
| `c6adb78956` | Merge lane H-direct: Workspace calls reach a forwarded Sandbox's daemon directly (G4) | 134613 | 135076 | +463 |
| `4c4c7ed5f0` | fix(536): push and pull refuse a held Store before reaching any machine | 135076 | 135081 | +5 |
| `364cff6e2e` | docs(535): record step H (direct Workspace calls) and its spec differences | 135081 | 135081 | +0 |
| `7d965141d0` | feat(536): sandbox.create's machine is optional (#536 §10) | 135081 | 135087 | +6 |
| `104bcc2899` | feat(536): Busy and Unresolved carry detail {entity: {kind, name}, phase, op} | 135087 | 135106 | +19 |
| `1bc38f0b1e` | refactor(536): move PostgreSQL password references to common/secrets.rs | 135106 | 135113 | +7 |
| `3d818c634e` | refactor(536): one reader for worker.json and one for a Sandbox's daemon keys, in common/secrets.rs | 135113 | 135125 | +12 |
| `c41bd58668` | test(536): worker_dial's stand-in answers this build's hello; record step I choices | 135125 | 135125 | +0 |
| `4c59f4867a` | docs(535): record D12, PostgreSQL servers stay on the host | 135125 | 135125 | +0 |
| `1de4fd104b` | docs(536): record H11a, project.init reads the caller's folder | 135125 | 135125 | +0 |
| `83715db285` | feat(535): PostgreSQL reach routes for apple-container, microsandbox and firecracker (G7) | 135125 | 135144 | +19 |
| `b772e77f97` | test(536): two Sandbox tests no longer assume a macOS host | 135144 | 135144 | +0 |
| `e2f6365cce` | docs(535): record D13 (G7 routes), the verify sweep and the live lanes | 135144 | 135144 | +0 |
| `9641b6a851` | feat(536): project.init reads the caller's folder for a worker's Store (H11a) | 135125 | 135823 | +698 |
| `bda9334c9c` | docs(536): project.init copies the caller's folder to a worker (H11a) | 135823 | 135823 | +0 |
| `a9279a4044` | test(536): project_init_worker removes every temp folder it makes | 135823 | 135823 | +0 |
| `f5866453b0` | Merge lane H11a: project.init reads the caller's folder for a worker's Store | 135144 | 135842 | +698 |
| `10ffd1036a` | docs(535): layerfs-postgres now publishes on all addresses | 135842 | 135842 | +0 |
| `6d35b90711` | tools(535): build the bundle's guest worker with PostgreSQL | 135144 | 135144 | +0 |
| `9bf0159963` | docs(535): G7 live receipt, PostgreSQL Store Sandboxes on four VM platforms | 135144 | 135144 | +0 |
| `21d19753e7` | Merge lane LIVE-REACH: G7 live receipt and the worker's PostgreSQL build | 135842 | 135842 | +0 |
| `31619094f4` | fix(535): lima Sandboxes dial the host at 192.168.5.2, not its name | 135842 | 135842 | +0 |
| `e7a2cee390` | feat(535): PostgreSQL Sandboxes run on lima, apple-container, microsandbox and firecracker (G7) | 135842 | 135837 | -5 |
| `93fc8f4e5a` | docs(535): G7 rows on, live findings, cleanup and one LOC correction | 135837 | 135837 | +0 |
| `967f69840e` | feat(535): a failed PostgreSQL reach check says which machine and why | 135837 | 135887 | +50 |
| `fe6f85013b` | test(535): the cell runner runs a cell on a PostgreSQL Store | 135887 | 135887 | +0 |
| `8f805e3889` | test(535): K2 terminal-pass receipts for apple-container, microsandbox and ssh | 135887 | 135887 | +0 |
| `4b00569b55` | fix(535): microsandbox machines reach servers on private addresses | 135887 | 135887 | +0 |
| `f8ce572f42` | test(535): K1 receipts: macOS native/docker cells, many Stores, contracts, parity, intents, locks, 10b | 135887 | 135887 | +0 |
| `3cd94c7d99` | docs(535): K3 terminal-pass receipts, lima, firecracker and Linux rows | 135887 | 135887 | +0 |
| `2d607fc066` | fix(535): a reconnected PostgreSQL Store name never reuses the old database | 135887 | 135892 | +5 |
| `9fd05b8367` | fix(535): a `layerfs` CLI call closes its Stores before it exits | 135892 | 135893 | +1 |
| `8b1474bd97` | fix(535): the Linux docker worker shares the host network to reach its Sandboxes' daemons | 135893 | 135912 | +19 |
| `3fc5316955` | test(535): K1 receipts: conversion from 2.0, intents clearing, final layout, RESULTS | 135887 | 135887 | +0 |
| `fbd4e90629` | fix(535): machine create refuses a platform not on this host in the support table's words | 135912 | 135916 | +4 |
| `d09b5deb60` | feat(535): LAYERFS_TRACE records each connection to a machine's worker | 135916 | 135960 | +44 |
| `1b00ab69a2` | fix(536): store.status and store.list change nothing | 135960 | 135958 | -2 |
| `cc216bf638` | fix(535): a PostgreSQL server that refuses the session says so | 135960 | 135978 | +18 |
| `d47f8a3fc5` | fix(535): workspace.mount refuses main; only side Branches are mounted | 135978 | 135984 | +6 |
| `9c3e3747f2` | fix(536): project.init reports §10b progress on every route | 135958 | 135882 | -76 |
| `902e6fa3c6` | docs(535): D14, only side Branches are mounted (owner) | 135984 | 135984 | +0 |
| `01bfb7ee93` | fix(535): machine.list probes the built-in docker machine like machine.status | 135984 | 135984 | +0 |
| `7cb0051e77` | fix(536): project.remove InUse names the project's Sandboxes in detail.related | 135882 | 135882 | +0 |
| `459722d11f` | Merge issue535-K1: first terminal pass receipts at fe6f85013b (lane K1) | 135984 | 135984 | +0 |
| `ba3d13771f` | Merge issue535-K2: first terminal pass receipts at fe6f85013b (lane K2) | 135984 | 135984 | +0 |
| `573e4438e4` | Merge issue535-K3: first terminal pass receipts at fe6f85013b (lane K3) | 135984 | 135984 | +0 |
| `969392f63f` | fix(536): a push or pull is uncertain from install on, and a new one clears what it left | 135960 | 136052 | +92 |
| `e7f9ece00b` | fix(536): store.disconnect and backup.restore InUse fill detail.related | 135882 | 135856 | -26 |
| `3a58d861f6` | Merge issue535-F2: reads change nothing, §10b Init progress and InUse detail | 135984 | 135880 | -104 |
| `615c082b57` | fix(536): a retire that does not finish is recorded, reported and cleared by the next push or pull | 136052 | 136105 | +53 |
| `2d1ca57e0c` | fix(536): retiring a Store from a worker removes its whole folder there | 136105 | 136109 | +4 |
| `f250732c76` | Merge issue535-F1: push and pull recover from a crash at install or retire | 135880 | 136029 | +149 |
| `35ed671b6c` | test(535): workspace_names mounts a side Branch, not main | 136029 | 136029 | +0 |
| `963c71076b` | docs(535): record the first terminal pass, its fixes and the fix lanes' choices | 136029 | 136029 | +0 |
| `b7c1aae7d0` | test(535): L2 final terminal-pass receipts at 35ed671b6c (apple-container, microsandbox, ssh) | 136029 | 136029 | +0 |
| `7238c04bba` | test(535): terminal pass L3 receipts at 35ed671b6c (lima, firecracker, Linux) | 136029 | 136029 | +0 |
| `1201418d09` | test(535): L1 final terminal pass receipts at 35ed671b6c (macOS native, docker) | 136029 | 136029 | +0 |
| `17b61af46b` | fix(535): a project Inited through a worker keeps its manifest for Sandboxes | 136029 | 136037 | +8 |
| `3e257b91c3` | fix(535): the Linux docker worker is ready when it listens on its own port | 136037 | 136043 | +6 |
| `3d8555ecc4` | Merge issue535-L1: final terminal pass receipts at 35ed671b6c (lane L1) | 136043 | 136043 | +0 |
| `35f693f313` | Merge issue535-L2: final terminal pass receipts at 35ed671b6c (lane L2) | 136043 | 136043 | +0 |
| `54161c9e2d` | Merge issue535-L3: final terminal pass receipts at 35ed671b6c (lane L3) | 136043 | 136043 | +0 |
| `92886ae5b4` | docs(535): targeted rerun receipts for L2-D1 at 54161c9e2d (lane R2) | 136043 | 136043 | +0 |
| `3948620697` | docs(535): lead checks at the final product tree; L2-D2 open item | 136043 | 136043 | +0 |
| `4b24592bcc` | Merge issue535-R2: targeted rerun receipts for L2-D1 at 54161c9e2d | 136043 | 136043 | +0 |
| `7de8a8e7cc` | docs(535): targeted rerun R3 receipts: Linux docker at 54161c9e2d | 136043 | 136043 | +0 |
| `4bc7aa70c6` | Merge issue535-R3: targeted rerun receipts for Linux docker at 54161c9e2d | 136043 | 136043 | +0 |
| `32aec70a64` | fix(536): machine.remove and machine.disconnect InUse fill detail.related | 136043 | 136044 | +1 |
| `de3b5d3006` | docs(535): live check of machine.remove InUse related at 32aec70a64 | 136044 | 136044 | +0 |
| `f2b76d6db8` | docs(535): TERMINAL-PASS.md, the combined terminal pass passes at 32aec70a64 | 136044 | 136044 | +0 |

Three message lines differ from the recount (`990e43f166`, `3d818c634e`,
`e7a2cee390`); all three are already in the DECISIONS correction tables.

## Cleanup

Done on 2026-11-03 with `tools/cleanup.py` (plan, then apply; exact
selections, owner HANDOFF-2 step 5), except two containers noted below. Host
free space went from 224.3 GB to 246.2 GB (`tools/cleanup.py check`); the
tool's own measured net recovery was 15.6 GB, and the rest came from other
activity on the host during the cleanup.

Removed:

- the `layerfs-issue535-kit` worktree (branch `issue535-L1`, merged, clean),
- the C1 registration `layerfs-issue535-c1-f90330a1552e`,
- the generated Cargo caches of all three lane worktrees (`kit` and `C1`
  `target/debug`, `verify` `target/release`, and the whole `kit` target),
- the verify lane's Linux volumes `lfs-dev-layerfs-issue535-verify-417967c19d8d-{target,cargo}`,
- three exited verify-lane build containers (`cargo test --no-run`, exit 101).
  The tool removed the first with a recovery image
  (`layerfs-cleanup-recovery:20261103t095622z-a53b4f68cde2-8093a2531bec`, kept).
  It refused the other two on every fresh plan: it compares `docker inspect`
  `Mounts` as an ordered list, and Docker returns them in a different order
  on each inspect. Those two had no root-filesystem changes (`docker diff`:
  only the mount points); their logs were saved and they were removed with
  `docker rm`, keeping their volumes for the tool's volume pass.

Still present (blocked, about 81 GB): Docker Desktop's virtual machine
(`com.apple.Virtualization.VirtualMachine`, PID 40057, up 2 days) holds
3,501 stale read handles from its file sharing into these folders, so the
tool refuses them as open:

- worktree `layerfs-issue535-verify` (branch `issue535-R3`, merged, clean;
  `target/debug` about 69 GB),
- worktree `layerfs-issue535-C1` (branch `issue535-R2`, merged, clean; about
  0.7 GB of target left),
- registrations `layerfs-issue535-kit-6d8fa8af9006` (0.4 GB) and
  `layerfs-issue535-verify-417967c19d8d` (5.5 GB),
- `layerfs-issue535-connect-7500a2864564/builds/k2-bundle` (3.3 GB, the
  superseded final-run bundle).

They become removable with the same scopes after Docker Desktop restarts.
Restarting it was not done, because other owners' containers run there
(`layerfs-postgres`, `layerfs-minio`, `layerfs-worker`, `lfc-worker`,
`mocktest-worker`).

Kept on purpose:

- the connect worktree, its registration, its Cargo home and Linux volumes,
- `builds/k-macos` and `builds/k3-bundle` (the final-identity builds the
  receipts name),
- the lane branches (`issue535-K*`, `F*`, `L*`, `R*`; all merged).

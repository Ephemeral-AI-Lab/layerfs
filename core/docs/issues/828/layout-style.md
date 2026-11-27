# LayerFS file and folder layout style

> Status: owner style for the 2.0 layout refactor (#828), settled 2026-11-27.
> Applies to production source under each crate's `src/` and to `tests/`.
> Rules marked **(proposed)** are inferred from the decisions and still need
> the owner's confirmation; everything else is a ruling.

The goal: open any crate and find the same concern under the same name, in a
folder small enough to read at a glance, named for what its code does.

## 1. Shape

1. **Leaf folder: 3 to 12 files.** 20 is the hard ceiling. `mod.rs`, `lib.rs`
   and `main.rs` do not count.
   **Exception, the file family:** the file family's call folder is one flat
   folder, one file per call, in every layer, with no count limit (api
   `families/file/`, engine store-service `methods/file/`). The family grows
   call by call; splitting it only to stay under 12 would invent a grouping.
2. **Three or more files sharing a prefix become a folder** (`sandbox_*.rs` ×3
   → `sandbox/`), and the prefix leaves the file names.
3. **Branch folder: subfolders plus at most 4 loose files.** A folder holds
   either mostly files (a leaf) or mostly subfolders (a branch), not a mix.
4. **No thin folders.** A folder that would hold one or two files does not
   exist; those files join the nearest folder they belong to (a loose file in
   the parent, or a sibling folder).
5. **Wider, not deeper: at most 4 levels below the crate directory**
   (`src/a/b/c/file.rs`).
6. **No catch-all folders**: no `utils/`, `helpers/`, `common/`, `misc/`,
   `shared/`, `types/`, `other/`.
7. **Tests:** one binary per area at `tests/<area>/main.rs` with one module per
   topic; helpers in `tests/support/`; a test that needs its own process
   (process-global state, slow target) in `tests/standalone/` with a
   `[[test]]` entry. The 3–12 rule does not bind `support/` or `standalone/`.

## 2. Siblings

8. **No long row of unrelated subfolders.** Subfolders that sit side by side
   should be one group (variants of one thing, kinds of one thing, the
   entities of one crate) or a short list of clearly different roles. A long,
   mixed row is regrouped: by domain first, then by role.
9. **Siblings of one group get one design.** Find the pattern they share and
   give each the same inner layout and the same file names for the same job
   (every bucket provider has `auth.rs`, `settings.rs`, `url.rs`; every
   doctor kind has `doctor.rs`; both Store backends have `acquisition/`,
   `compact/`, `diagnostic/`, `metadata/`, `objects/`, `session/`,
   `upgrade/`). A sibling differs only where its code really differs, and
   the difference should be explainable in one line (PostgreSQL has a
   `server/`; SQLite has a file `lifecycle/`).
10. **A non-member does not sit among a group.** Something that is not a
    variant (shared runtime, setup, a support table) does not sit in the same
    row as the variants; it goes beside the group's parent or into a role
    folder.

## 3. Naming: semantics first

11. **Name a folder for what its files do or hold**, so a reader can predict
    the contents from the name alone. Prefer the project's own domain words
    (Store, Workspace, Sandbox, Commit, Branch, lane, overlay, call table).
12. **Never name by a word that fits everything.** Rejected by the owner:
    `calls/`, `call/`, `read/`, `surface/`, `monitor/` (when it sits beside
    another reads folder), `power.rs`, `support_rows.rs`, `entry.rs`,
    `unmount/` (daemon side). Accepted: `status/`, `lifecycle/`, `dispatch/`,
    `run/`, `jobs/`, `reply/`, `teardown/`, `server/`, `boot.rs`,
    `available.rs`, `with_users.rs`.
13. **No name that collides with another concept.** The daemon's `unmount/`
    became `teardown/` (Workspace unmount is a call); persistence `inspect/`
    became `server/` (`layerfs-inspect` is a crate).
14. **No folder repeats its crate's name; no file repeats its folder's name**
    (`layerfs-inspect/src/inspect/`, `route/route.rs`, `calls/calls.rs` are
    all wrong).
15. **A call's file is named after the call** (`create.rs`, `start.rs`,
    `stop.rs`, `remove.rs`), one call per file where calls are grouped. Work
    several calls share gets its own plain noun (`boot.rs`, `progress.rs`).
16. **File names are short nouns, not sentences.** Drop words the folder
    already says (`postgresql/postgresql_contract.rs` → `postgresql/contract.rs`;
    `machine/machine_target.rs` → `machine/doctor.rs`).
17. **Casing:** `snake_case` for Rust files and folders; Cargo binaries keep
    their crate-style names (`bin/layerfs-sandbox-helper/`).
18. **(proposed) Singular or plural.** A folder named for a concern or role is
    singular (`status/`, `lifecycle/`, `route/`, `record/`, `backend/`,
    `platform/`, `dispatch/`). A folder holding a set of like items, one per
    file or subfolder, is plural (`providers/`, `jobs/`, `checks/`). Do not
    mix the two for the same idea across crates.
19. **(proposed) Same idea, same word, everywhere.** One name per concept
    across crates and layers: drivers of a thing are `backend/` (Store,
    Sandbox) or `platform/` (machine) — never a third word; a crate's reads
    are `status/`; its existence/run changes are `lifecycle/`.

## 4. Organization

20. **Entity crates share one skeleton** (machine, Store, Sandbox, Workspace),
    each folder present only where the concern exists:
    - `status/`: how the thing is, without changing it: record reads (get,
      list, state words), live probes, its own stats;
    - `lifecycle/`: calls that change whether it exists or runs;
    - `record/`: its record, identity and op intent;
    - `route/`: how a call reaches it (worker client, host dispatch, daemon
      channel);
    - `backend/` or `platform/`: its drivers, one folder per variant;
    - parts only this entity has (Store `backup/`, Sandbox `image/`).
21. **Group by domain first, then by role** (`layerfs-store`: `admin/`,
    `data/`, `backend/`, `backup/`).
22. **Place code where its dependencies allow, proven by `use` lines.** The
    layer order is core < engine < control < api, and inside control
    home < machine < store < sandbox < workspace < inspect. Code that reads
    entities above its crate goes up (every doctor and cross-entity read is in
    `layerfs-inspect`, one folder per kind); checks flowing the other way go
    through a trait the lower crate declares (`MachineUsers`, `StoreUsers`).
23. **One concept, one place.** Backend-specific code lives in its backend
    (each Store backend's `diagnostic/`); neutral code stays neutral
    (persistence `diagnostic/check/`).
24. **A move across crates needs `use`-line proof.** A move inside a crate needs
    only the name and grouping rules.

## 5. Settled decisions (do not reopen)

- Control crates (Option A): `layerfs-home`, `layerfs-machine`,
  `layerfs-store`, `layerfs-sandbox`, `layerfs-workspace`, `layerfs-inspect`.
- Engine `layerfs-workspace` → `layerfs-vfs` (Workspace filesystem
  semantics under FUSE/NFS; was planned as `layerfs-workspace-fs`), with
  `base/`, `binding/`, `construction/{file,namespace}/`, `job/`,
  `operations/{file,kernel,namespace}/`, `ports/`; daemon `control/unmount/`
  → `control/teardown/`.
- `layerfs-inspect` holds the doctors (home, machine, store, sandbox, project,
  backup; `workspace.doctor` planned) and the cross-entity reads; folders
  `frame/`, `files/`, `records/`, `home/`, `machine/`, `store/`, `sandbox/`.
- `layerfs-store`: `admin/` (`record/`, `route/`, `status/`, `maintenance/`,
  `with_users.rs`), `data/` (`fs/`, `history/`, `project/`), `backend/`
  (`sqlite/`, `postgresql/`), `backup/` (`jobs/`, `bucket/`,
  `providers/{local_folder,r2,s3}/`, `s3_api/`, `copy/`).
- Machine `lifecycle/` (boot, create, start, stop, force_stop, remove,
  progress), `status/` (list, live, state), `platform/available.rs`.
- Sandbox `status/` and `lifecycle/`; api `dispatch/` (`answer/`, `guard/`,
  `run/`); home `reply/`.
- Persistence: each backend has `diagnostic/`; PostgreSQL `lane.rs` loose and
  `server/`; crate `diagnostic/` holds only neutral checks.
- Style review pass (2026-11-27, all four layers): api families under
  `families/` with `status/`/`lifecycle/`; Sandbox root regrouped (`route/`,
  `runtime/`, `exec/`, variants-only `backend/`); machine `platform/` holds
  only the four variants with one file set (`adapter`, `lifecycle`,
  `profile`, `status`), plus `route/`, `setup/`, `table/`; daemon
  `control/` → `workspace/`; store-service `read/` → `snapshot/` and a
  `workspace/` group; overlay `diagnostics/` dissolved; persistence
  `storage/` holds `blocks/` and `metadata/`; content `filesystem/` in
  `tree/`, `sorted/`, `update/`; content `admission/` → `verify/`; storage
  keeps `port/` (only `port/read/` → `port/pack_read/`).
- Open questions answered (2026-11-27): inspect `records/` folds into
  `home/`; inspect `frame/` → `doctor/`; content `sorted/` → `btree/` and
  `compare/sorted.rs` → `compare/btree.rs`; SQLite `objects/` `unit_` files
  stay loose (exception); api `file/change/` kept; bridge `control/`,
  `merge/rule/`, `reference_index/`, store `admin/`, `data/` and
  `admin/with_users.rs` kept; `workspace.doctor` filed as #829.
- Store calls match the api families in every layer (2026-11-27): api
  `families/{branch,commit,file,project}`; control store
  `data/{branch.rs,commit.rs,file/,project/}`; engine store-service
  `methods/{branch/,commit/,file/,project.rs}`, with files named after the
  2.0 calls. The engine's method-table strings keep their pre-2.0 names
  until a separate change renames them.
- Sandbox `running/` dissolved (2026-11-27): it mixed local daemon process
  plumbing with run-side records under a name too close to `runtime/`.
  `process/` (daemon, listener, search_path, streams, confined); `record/`
  gains discovery, reopen, sweep, unmount_guard; `lifecycle/registry.rs`.
  `runtime/` keeps the backend-neutral command types.
- Sandbox review 3 (2026-11-27, two reviewers, `use`-line checked):
  `runtime/sandbox.rs` → `backend/select.rs`; `confined.rs` (with the `os`
  alias) → `backend/` (revises the line above); `status/{ask,owner}.rs` →
  `route/inspect.rs`; `status/live.rs` → `layerfs-inspect` `machine/`;
  `channel/capacity.rs` → `status/`; `exec/record.rs` → `record/exec.rs`.
  Renames: `force_remove.rs`, `machine_stop.rs`, `lifecycle/serve.rs`,
  `exec/settle.rs`, `runtime/stop.rs`, `status/stats.rs`, `image/cache.rs`,
  `image/machine.rs` → `list.rs` + `prune.rs`.
- Store/machine/home review 3 (2026-11-27, five reviewers, `use`-line
  checked):
  - One word for op intent: `intent.rs` in every `record/` (home, Store
    admin, project).
  - A call folder's entry file is `run.rs` (project `delete/`, `export/`,
    `init/`).
  - Rule 1 applies in control too: Store `data/file/` holds one file per
    call (14 + `requests.rs`).
  - Store: `unknown.rs` → `data/`; `backup.connect` → `bucket/connect.rs`;
    `restored.rs` → project `record/`; diagnostic split into each backend;
    `jobs/heads.rs` merged into `list.rs`; `copy/{object,link}.rs`;
    renames `repair_mount_holders.rs`, `route/executor.rs`,
    `bucket/secret.rs`; project `status/` split `get.rs` + `list.rs`.
  - Machine: `record/` (`machine.rs`, `adapter_spec.rs`, `built_ins.rs`);
    `table/backend.rs` keeps only the enum (capabilities stay in Sandbox,
    which machine cannot use); `probe.rs` → `setup/` (closes the open
    question); `table/profile.rs`, `postgresql_url.rs`, `status/get.rs`,
    docker `image_cache.rs`/`unsupported.rs`, lima `name_limit.rs`,
    `setup/bundle_install.rs`.
  - Home: `op/` dissolved (`cancel.rs` at root); new `record/` (file,
    fields, generation, observed, intent, process, token); `disk/` keeps
    the home folder on disk; `lock/mutex.rs`, `log/lines.rs`,
    `reply/outcome.rs`.
- Engine review 4 (2026-11-27, seven reviewers, every engine crate but vfs,
  recursive, `use`-line checked; 318 rows):
  - Persistence: code both backends use leaves `sqlite/` (public compaction
    types → `gc/compact.rs`, neutral merge scratch → `storage/`,
    `backend/acquisition.rs`); SQLite's provider → `sqlite/objects/`;
    PostgreSQL `metadata/` split by table like SQLite; each backend's doctor
    entry is `diagnostic/run.rs`; `history/query/` → `history/statement/`
    (collided with core `query`); `diagnostic/scan.rs` → `check/store.rs`;
    `store/{open,seal}.rs` → `store/sqlite.rs`; `gc/epoch.rs` → `fence.rs`.
    Then (owner): persistence `store/` → `config/`, because `store/` beside
    `storage/` read as one name twice; `config/config.rs` →
    `postgresql_options.rs` (rule 14). Store-service and project keep
    `store/` (no `storage/` beside them).
  - Store-service: `history/lookup.rs` → root `lookup.rs`;
    `snapshot/named.rs`; `store/{open,handle}.rs`; `store/` and `operator/`
    kept. Redrawn the same day (owner, "overall they just look weird"):
    five root folders, each the api's word or a domain noun —
    `families/` (was `methods/`, plus `operator/` as the backend.store.*
    table, one `args.rs`), `history/`, `filesystem/` (was `snapshot/` reads
    plus `write/` → `change/`, with `named.rs` → `root.rs`,
    `file_facts.rs` → `facts.rs`, `change/changes.rs` → `run.rs`),
    `store/`, `workspace/`. Rejected: `snapshot` (means a SQLite read
    snapshot and the ruled-out compute snapshot), `files/` (fits
    everything, echoes `families/file/`). Tests follow: `tests/families/`,
    `tests/filesystem/`, `tests/change/`.
  - Daemon: `workspace/kernel/` → `workspace/fuse/` with the `Kernel` enum in
    `workspace/kernel.rs` (revises the settled daemon line: `fuse/` and
    `nfs/` are the two kernel routes); `application/diagnostics/` →
    `observed/`; `operator/` one file per call.
  - Overlay: `lifetime/` → `lifecycle/` + `custody/`; `open/state.rs`;
    `operation_record.rs` everywhere. Revised the same day (owner): the
    overlay is laid out around its generation cycle. `generation/` holds
    capture (seal the active rows without copying), `install.rs` (split out
    of capture), `failed.rs`, `tickets.rs`, and `captured/` (what Commit
    construction reads from a sealed generation: reader, cursor, runs,
    digest, names); `lifecycle/` dissolved. `database/` → `backing/` (the
    Workspace backing, AGENTS.md's word; store/ and storage/ mean the
    committed side), with `close.rs` and `workspace.rs`. `maintenance/` →
    `reclaim/` (`reclaim.rs` → `turn.rs`).
  - NFS/FUSE: NFS `protocol/` never names `Kernel`, `server/` dissolved into
    `kernel/`; FUSE `coherence/` folded into `request/`; same words as the
    rest of the stack (`mutation`, `attach`, `forced`).
  - Bridge `host/` → `system/` (runs inside the Sandbox, not host-side);
    project `import/run.rs`. Superseded the same day (owner) by the crate
    changes below.
  - Crates (owner, 2026-11-27): `layerfs-mount-ports` merges into
    `layerfs-vfs` (`ports.rs` → `ports/mount.rs`; `operations/` + `phase.rs`
    → `requests/`; no new dependency edge, its users already depend on vfs).
    `layerfs-bridge` splits: `layerfs-protocol` (how processes talk:
    `sdk.rs` = the SDK call shape + values, `daemon/` = was `control/`,
    `duplex/`, `provision/`, `wire.rs`, `initial_record.rs`) and `layerfs-os`
    (what this kernel reports: `process/`, `mount_table.rs`, `network.rs`).
    Pieces with only control users leave engine: log rotation →
    home `log/rotate.rs`; the namespaces runtime path → Sandbox
    `backend/namespaces/linux/runtime.rs`. Rejected: `layerfs-system`
    (says nothing). Home's loose log tests join `tests/log/`.
  - Validator: trees are built from every layer's rows (files may cross
    layers), and rule 7 exempts `tests/support/` and `tests/standalone/`
    from the whole 3–12 range.
  - Validator: rule 7 exempts `tests/support/` and `tests/standalone/` from
    the 3-file minimum.
- Content `filesystem/` (owner, 2026-11-27): `btree/` → `tree/pages/` (the
  shared copy-on-write page engine sits under `tree/` beside the kinds it
  serves: `attribute/`, `directory/`, `inode/`); `update/` →
  `construction/` (same word as `file/construction/`); `qualify/` →
  `topology/`; `compare/btree.rs` → `compare/pages.rs`; tests follow
  (`pages.rs`, `filesystem_pages_localized.rs`, `construction.rs`). This
  supersedes the earlier `sorted/` → `btree/` answer.
- Rebase on #818 + #835 (owner, 2026-11-27). The plan's base is now
  `layerfs_v2_0_0` `7325e66c7a` (#818 merged) plus #835 at `775ed5f487`
  (#819 host worker; the merge is staged in `layerfs-819-cli`). LOC are
  counted on that tree. Re-pin once #835 lands. Four subagent reviews, each
  claim checked against `use` lines:
  - #835's renames carry through: machine `platform/native/` → `platform/host/`,
    Sandbox `backend/native/` → `backend/process/`. The Sandbox root
    `process/` → `spawn/`, so `process` means one thing. `lifecycle/local.rs`
    → `boot.rs`. The process backend's own start leaves `spawn/daemon.rs` for
    `backend/process/instance/create.rs`, the twin of the namespaces
    `create.rs`. Sandbox `route/host/` → `route/machine/`, because "host" now
    means the host machine (the wire call keeps its name `sandbox.host`).
  - Host worker, no new crate (machine boot code calls the client, and the
    guest server serves Sandbox and inspect, so no one crate fits both):
    - The host worker server moves from api into store, under
      `admin/host_worker/`. It imports only store, machine, home and bridge.
      The move splits the host half out of `connection.rs` and the host
      switch out of `fault.rs`.
    - The same folder holds the Store side, a route not data (`call.rs`
      tries it first): read, write, retained, stream, postgresql, handoff,
      server, connection, write_queue, postgresql_queue. Queues are not
      "lanes"; that word means the publication lane.
    - Machine `route/{guest,host}/` hold the two worker clients; the host
      client splits into `client.rs` and `endpoint.rs`.
  - Upward-dependency fix (a defect in the plan before #835):
    - machine `with_users.rs` and #835's host-power code call store,
      inventory and Sandbox. `MachineUsers` gains reconcile, forget, holders
      and force-stop hooks, and Sandbox implements them in a root `users.rs`.
    - The host-power code is `lifecycle/host.rs`: fence, worker start and
      drain.
    - `machine.get`'s live probe is `status/worker.rs`; the doctor keeps the
      rest in inspect `machine/worker.rs`.
  - Store `admin/open/` (the Stores this process holds open; the home's own
    word, `store/<name>/open/`): `open.rs` splits into
    `cache`/`checkpoint`/`close`/`host`, plus `lease`, `service` and `args`.
    `route/mount_intents.rs` is split out of `service.rs`.
  - Home `secret/` (`keys`, `saved` = was `disk/secrets.rs`, `wipe`);
    `disk/native_names.rs` (was `home/legacy_host.rs`).
  - Storage `read/source/`: `Fetch`, its fields and its `impl` blocks (10
    files; "Source" is the crate's own trait word). provider, objects,
    prefetch and length stay loose.
  - Persistence: **reopened** `backend/acquisition.rs` → root `acquisition/`
    (`epoch`, `failure`, `payload`, `placement`; #818's placement is shared,
    and PostgreSQL imports SQLite's), beside the root `history/` and
    `storage/`. SQLite `child_windows.rs` → `placement.rs`. PostgreSQL
    `nonce()` → `session/attempt.rs`; pack statements →
    `objects/publish_flight.rs`. #818's PostgreSQL queries are grouped like
    the older ones: `acquisition/<name>.sql`, `history/branch/by_name.sql`,
    `history/node/`.
  - Store-service: `reply.rs` → `paged/`; `versions.rs` stays loose in
    `filesystem/`.
  - Tests added at #818/#835 join area folders. New ones: CLI `host_worker/`,
    `host_worker_writes/` and `host_worker_load/`; store-service
    `read_round_trips/`, `history_round_trips/`, `write_round_trips/` and
    `round_trips/`; persistence `postgresql_acquisition/`.
- Owner rulings before implementation (2026-11-27):
  - `layerfs_v2_0_0` `3203f6acbf` (#835 merged) is the implementation base.
    Its tree is identical to the plan's base.
  - Content `compare/across.rs` keeps its name: "versions" stays
    store-service's word (`filesystem/versions.rs`).
  - The control-layer workspace crate's `host/` → `machine/`, the same word
    as Sandbox `route/machine/`.
  - Construction `reference/` and `reference_index/` stay as named for now; a
    later rename is a one-line move.
  - Execution: the plan is committed under `core/docs/issues/828/`. Merges
    into `layerfs_v2_0_0` are held during the run, which ends in one pull
    request. Only `cargo check` runs while restructuring. Full checks run
    once at the end; the nightly FUSE and mount tests are reported as a gap.
    The fan-out uses background subagents. See `restructure-plan.md`.
- Store backup providers (owner, 2026-11-27, during implementation): keep
  one folder per provider: `local_folder/` (`driver.rs`), and `r2/` and
  `s3/` (`settings.rs`, `url.rs`). This is an accepted exception to the
  3-file minimum. Auth is shared today (`bucket/auth.rs`,
  `s3_api/sign.rs`), so the planned `bucket/provider.rs`, per-provider
  `auth.rs`/`checks.rs` and local-folder `url.rs`/`settings.rs` are not
  built; they arrive with the provider feature.
- Still open: construction `reference/` and `reference_index/` names
  (reference-count effects; "reference" also means overlay reference pieces).

## 6. How a proposal is written

- Layouts use the AGENTS.md diff listing: `+` new, `-` removed, space stays;
  every existing file with its production LOC from
  `tools/production_loc.py --files` at the stated commit; folders with
  bracketed subtotals; `—` plus `new`/`cond.` for files that do not exist.
- Every move or rename carries a one-line reason. A cross-crate move cites the
  `use` lines that allow it.
- A proposal states what it does not fix and why.
- Proposals change the plan only after the owner agrees; the issue and the
  layout page follow on "Update".

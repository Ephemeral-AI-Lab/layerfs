# layerfs-machine naming review (review 3, #828)

Scope: every folder and file under planned `control/crates/layerfs-machine/src/`
(rows of `nav/site/layout.json` whose `n` starts there; tree from
`nav/trees/tree-control.txt`). Old source read at `v2tree/control/crates/`
(`layerfs-machine`, `layerfs-platform`, `layerfs-sandbox-backend`, `layerfs-control`).
Planning only; nothing in the repository was touched.

## 1. Verdict

Most names are sound. The settled skeleton (`lifecycle/`, `status/`, `platform/`,
`route/`, `setup/`, `table/`, variant file set) holds up. I found four concrete
defects: `table/platform.rs` collides with the `platform/` folder; `machine.get` has
no file of its own; `setup/machine_spec.rs` repeats the crate name and is easy to
confuse with `setup/spec.rs`; `docker/images.rs` looks like the `machine.images`
call. Three smaller renames are clearly better. Everything else is fine as planned.

## 2. Proposed renames, ranked by value

| # | Planned | Proposed | Reason (evidence) |
|---|---|---|---|
| 1 | `table/platform.rs` | `table/profile.rs` | Rule 13 collision: `crate::table::platform` sits beside the `crate::platform` folder, so a reader looking for the variants or the `Platform` enum opens the wrong one. The file's header says "the one table that describes each [platform]: its `Profile`, one `PROFILE` per platform folder". `table/profile.rs` names the table and pairs with each variant's `profile.rs`, the same way `platform/adapter.rs` (trait) pairs with `<variant>/adapter.rs` (impl). |
| 2 | `status/list.rs` (holds `list` + `get`) | `status/list.rs` + `status/get.rs` | Rule 15: `machine.get` has no file named after it. Every sibling names it: api `families/machine/status/get.rs`, store `admin/status/get.rs`, Sandbox `status/get.rs`. Old `list.rs:193` `pub fn get` plus `settings()` (lines 193–268) move; the shared row builder (lines 19–150) stays in `list.rs`. This reopens the settled `status/ (list, live, state)` only by one file; the defect is concrete. |
| 3 | `setup/machine_spec.rs` | `setup/record_spec.rs` | Rules 14/16: `machine_` repeats the crate. Beside `setup/spec.rs` (the `Spec` types), a reader can't tell which file holds what. Its job is turning the record into a `Spec` (header: "A machine's records become the adapters' `Spec`": `spec()`, `provider_spec()`, `machine_of()`, `ensure_built_ins()`, `worker()`). Callers already import it as `record` (`create.rs:18`, `remove.rs:6`: `use super::spec::{self as record, …}`). |
| 4 | `platform/docker/images.rs` | `platform/docker/image_cache.rs` | Rule 13: `images.rs` reads as the `machine.images` call (api `families/machine/status/images.rs`), which lives in Sandbox `image/list.rs`. The file lists nothing. It holds one function, `cached_image()`, which tags the Sandbox image `layerfs-cache/sandbox:<hex>` in the Engine. That is the same idea and the same word as Sandbox `image/cache.rs` ("a machine's image cache"). |
| 5 | `platform/lima/limit.rs` | `platform/lima/name_limit.rs` | The file holds only `name_limit()`, which serves adapter method `name_limit` and returns `NameLimit` (`lima/adapter.rs:29-30`). Everywhere else, `limit(s)` means size limits (store `backup/bucket/limits.rs`, core `limits.rs` ×2). Sandbox hardening also uses "limits" for cgroup limits. |
| 6 | `setup/artifacts.rs` | `setup/bundle_install.rs` | Rule 19: one idea, two words. The sibling `bundle.rs` locates the sealed bundle; this file installs that bundle into the guest (header: "Per-file bundle installation…"; only pub fn `install(spec, guest, bundle)`, called from `worker.rs:134`). Two files sharing a prefix is under the three-file folder threshold. |
| 7 | `table/postgres_url.rs` | `table/postgresql_url.rs` | Rule 19: every other planned name says `postgresql`: store `backend/postgresql/`, `data/project/init/postgresql.rs`, engine `postgresql/` and `postgresql.rs`, and type `PostgresqlSupport` in `table/support.rs`. This is the only `postgres_` name in all four trees. |

## 3. Names checked and fine

- **Root:** `lib.rs`, `record.rs`, `update.rs` (= api `families/machine/update.rs`),
  `users.rs` (`MachineUsers`; matches store `users.rs`), `with_users.rs`
  (matches store `admin/with_users.rs`). There are four loose files, which is the limit.
- **`lifecycle/`:** `boot`, `create`, `start`, `stop`, `remove` match the api
  `lifecycle/` call names. `force_stop` parallels Sandbox `force_remove`.
  `progress` is shared phase events. All of these are settled.
- **`status/`:** `live.rs` and `state.rs` are fine, apart from #2.
- **`platform/`:** `adapter.rs`, `dispatch.rs` (the same pair as Sandbox
  `backend/adapter.rs` + `dispatch.rs`), `error.rs` (`MachineError`), and
  `available.rs` (settled).
- **Variant files:** `adapter`, `lifecycle`, `profile`, `status`. The overlap
  of `<variant>/lifecycle.rs` and `<variant>/status.rs` with the crate's
  `lifecycle/` and `status/` folders is **not a defect**. It is the same concept
  one layer down (rule 19), the paths are always qualified
  (`platform::lima::lifecycle`), and it mirrors trait/impl `adapter.rs`.
  `docker/cli.rs` and `socket.rs` are fine. `docker/windows.rs` is the cfg twin
  of the unix files; its OS name matches Sandbox `linux/` and `macos/`.
  `lima/template.rs` is fine.
- **`route/`:** `client`, `frame` (wire framing), `hello` (`worker.hello`) and
  `diagnostic` (`DiagnosticWorker`) are fine. The folder shares the word `route/`
  with Sandbox and store on purpose (rule 20 skeleton); that is not a collision.
  `runs_here.rs` is a verb phrase (rule 16), but every alternative is worse:
  - `direct` already means "Workspace calls straight to a forwarded daemon" in Sandbox and workspace `route/direct.rs`.
  - `executor` means command launcher in AGENTS.md.
  - `local` clashes with Sandbox `lifecycle/local.rs`.

  Keep it; it matches its public fn `runs_here`.
- **`setup/`:** `bundle`, `default_image`, `forward`, `guest`, `spec`, `vm`
  and `worker` are fine. `tool` matches `Tools`, `find_tool` and
  `tool:<cli>`. `facts` is a project word (engine has five `facts.rs`), so not
  clearly worse than an alternative.
- **`table/`:** `backend.rs` is the Sandbox `Backend` enum, the same word as
  Sandbox `backend/`, which is correct. `options`, `reach` (the #535 term),
  `removed` and `support` (the `SUPPORT.json` support table) are fine.
  `probe.rs` is left alone because its placement is still open. Note that the
  crate uses "probe" both for the host-tool probe (`Probe`) and the live machine
  probe (`status/live.rs` `PROBE`).

## 4. Cross-crate inconsistencies (outside this crate's files)

- **`get` file:** machine is the only entity without `status/get.rs` (see #2).
- **`live.rs`:** inspect `machine/live.rs` (from Sandbox `inspect/live.rs`)
  checks whether a machine's **Sandbox daemons** still run for `machine_stats`.
  `layerfs-machine` `status/live.rs` is the machine's own live read. The same
  path tail names two different reads. Consider renaming the inspect file
  (for example `daemons.rs`).
- **`frame`:** machine `route/frame.rs` is the wire frame (4-byte header +
  attachments). Sandbox `route/host/frame.rs` is the typed `sandbox.host` request
  encoding. One word covers two ideas. Inspect `frame/` was already renamed to
  `doctor/` for this kind of reason.
- **Test area:** the tests area is `tests/worker/` while the src folder is now
  `route/`. `tests/table/support_table.rs` repeats its folder (rule 14).
- **Type names (not files):** two `Endpoint` structs (`route/client.rs`,
  `setup/spec.rs`). Two `State` types: provider state in `platform/dispatch.rs`
  and the six-word machine state in `status/state.rs`.
- **error vs failure:** machine `platform/error.rs` and store
  `backup/bucket/error.rs` versus Sandbox `runtime/failure.rs` and
  `route/daemon/failure.rs`. I note it only; the meanings differ enough that no
  rename is proposed.

## Organization notes (not naming, one line each)

- `docker/status.rs` (old `unix.rs`) holds `run`, the docker CLI runner used by
  `images.rs` and `lifecycle.rs`. It belongs in `docker/cli.rs`.
- `docker/lifecycle.rs` holds `load_image`, which is image work, not lifecycle.
  It belongs beside `cached_image` (#4).
- `setup/record_spec.rs` (#3) is really record I/O (`save_running`,
  `save_unready`, `update`). If a third record file ever appears, it and
  `record.rs` should form `record/` (rule 20).

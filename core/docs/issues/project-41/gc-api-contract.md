# GC API contract (T11): four calls, results, refusals

> Draft for T11 of layerfs-project#41, derived **only** from the frozen `LayerFS-GC-spec.md` (the spec wins; `§N` cites it). No behavior beyond the spec is asserted; unknowns are listed in §6.

## 1. Calls and signatures

| Call | Arguments | Scope |
|---|---|---|
| `branch.remove(project, branch)` | project, Branch | one Branch |
| `project.delete(project)` | project | one project |
| `project.remove_file(project, path)` | project, absolute POSIX path | one project + path |
| `store.compact(name)` | Store | one Store |

Each takes only the resource identifier: no confirm, force, dry-run, stop-Sandboxes, Store, URL or volume option (§1). The
three removals are logical and reclaim nothing; `store.compact` is the only byte-reclaiming call (§1, §4). `project.remove` is
retired — an old client gets `InvalidInput` ("no call named `project.remove`"), never a silent hard delete (§4).
`branch.remove` of `main` is `ReservedName` (§4). `store.compact` picks `none`/`rebuild` by the 10% wholly-dead-pack rule; a
started rebuild completes even below 10% (§3, #42 item 41).

## 2. Success shapes

snake_case on every surface (§10); these fields are required:

- `branch.remove` → `{kind: "Removed", project, branch, removed_ms}` (§10).
- `project.delete` → `{kind: "Removed", project, deleted_ms, home_entry_removed}`; the worker frame is `{store, stack_id,
  deleted_ms?}`, its reply `{kind: "Removed", project, stack_id, deleted_ms}`, and the caller echoes `home_entry_removed` (§4,
  §10).
- `project.remove_file` → `{kind: "Removed"|"UpToDate", project, path, frontier_serial, removed_ms}` — the stored marker
  values. `UpToDate` iff no project node is above **this path's** marker frontier (missing = -1); a commit in another project
  does not make it `UpToDate` (§4.3).
- `store.compact` → `{kind: "Compacted", ...}` with §11's report: `path` (`none`|`rebuild`), `generation {before, after}`,
  `deadBytesBound`, `deadBytesRefined`, `logicalDiscarded`, `backendReusable`, `physicalShrink`, `keptWaste
  {deadInRetainedPacksBytes, deltaBases {bytes, references}, inodeBlocksBytes}`, `peakTempBytes`, `residue {bytes, artifacts[,
  reason]}` (§11).

Binding rules: `none` leaves the generation unchanged, `rebuild` is `G → G+1` (§3, §11). `estimate: bool` is top-level; a
capped field is `{value, cap}`; every zero field names a `zeroReason` from `empty-store`, `none-path`, `no-history`,
`no-waste`, `postgresql-in-place` (§11, S7). On `none`, `deadBytesRefined == deadBytesBound`; on PostgreSQL they are always
equal (no payload sweep) (§11). `home_entry_removed: false` is still success — the marker is the commit point and a same-home
retry is idempotent (§4). A repeat `branch.remove` is `NoSuchBranch` (a concurrent one is `Busy`, phase `"retire"`); a repeat
`project.delete` is §4's idempotent success (§4.4).

## 3. Refusals and detail shapes

| Code | GC trigger | `detail` |
|---|---|---|
| `MountsActive` | a mount/Sandbox of the Branch or project is in scope (§5) | `holders` (mounts/Sandboxes) |
| `Busy` | another user of the project/Store, a live op, a busy lane, a concurrent remove (§5, §4.4) | `{entity, phase, op}` (+ PG `pid`, `client_addr`, `backend_start`) |
| `Unresolved` | a dead owner left an uncertain op; observed by a non-owning call (§5.1, §10) | `{entity: {kind, name}, phase, op}` |
| `Removed` | a read/stat/merge hits a `remove_file` marker (§4.3, §10) | `{paths: [{path, frontier_serial}]}` |
| `NameTaken` | a create/fork/merge target name still held; `project.init` (§4.4, §10) | `{holder: {kind: branch\|project, name, state: live\|retired\|deleted}, source: store\|home}` |
| `StaleStore` | SQLite `(dev, ino)` differs from open; PG generation/fence mismatch (§10, §5.2) | unspecified |
| `Capacity` | a real `PersistenceError::Refused {status: "Capacity"}` (§10) | unspecified |
| `Unsupported` | default catalog body; PG transaction pooling for mounts/compaction (§10, §5.2) | unspecified |
| `ReservedName` | `branch.remove` of `main` (§4) | — |
| `Conflict` | sealed tuple changed at install; missing `project_epoch` row after cleanup (§10, §5) | unspecified |
| `NotFound`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchProject`, `NoSuchStore` | a hidden/collected resource, resolved through filtered statements (§4.2, §5) | — |
| `InvalidInput` | bad `remove_file` path, retired `project.remove`, symlink component (§4.3, §4) | — |
| `Exists` | a stray artifact at the staged compact name with no record (§9) | — |

`InUse` stays for non-GC calls only (§10). **snake_case rule:** success results and refusal details are snake_case; the
dispatcher snakes the detail (or the site builds it snake); a camelCase detail is a bug; the CLI prints the rendered map; §11
names are logical, not rendered (§10, §11). A busy checkpoint, a live owner and a reachable-but-unresponsive Sandbox stay
`Busy`; only a provably dead, uncertain op is `Unresolved` (§5.1, §7).

## 4. MCP, folder scope, CLI/SDK, `store.status`

- All four are **admin-profile** calls, refused under a folder scope with `out of scope: <call>`
  (`api/crates/layerfs-api/src/calls/scope.rs:33`; #42 item 39): a folder scope guards one folder and one Branch, and none of
  the four is a folder operation, so it is refused, never widened (§10).
- The admin classifier checks its admin list before the work families; the four join it and `project.remove` leaves both the
  call table and that list. MCP names follow the existing `project_remove` mapping (`project.delete` → `project_delete`); an
  old `project_remove` is `InvalidParams "unknown tool"`, a CLI usage error exits 2, the library reports `InvalidInput` (§10).
- The table becomes 76 (38 work / 38 admin): regenerate `gen-profiles`/`gen-coverage` and the hard-coded 73/38/35 counts;
  `whyOf`, the profile prose and `--profile admin` usage name the four calls, one `whyOf` line each; a precedence test covers
  `branch.remove` under `work` (§10).
- CLI/SDK surface: one command/one SDK method per call, ids only — `layerfs branch remove <project> <branch>`, `layerfs
  project delete <project>`, `layerfs project remove-file <project> <path>`, `layerfs store compact <store>` (the runbook
  already uses the last); results are the rendered snake_case map (§1, §10).
- `store.status` while compacting (§11): `op = {id, name, phase, started, uncertain}` from `gc_op` (PostgreSQL, Store-side) or
  `<store>.compact-op` (SQLite, the sidecar the spec calls the home op; a remote Store via `step_on`); `id` is the recorded
  `op_id` — 16 bytes, rendered as `op-<32 lowercase hex>` — not a row number or a fresh id; `state` is `busy`/`unresolved`;
  phases `mark`/`build`/`verify`/`install` (SQLite) and `mark`/`build`/`swap`/`cleanup` (PostgreSQL); there is no `PROGRESS`
  option (#42 item 40).

## 5. Implementation checklist (T11)

1. Call table: add the four, delete `project.remove`; ids-only arguments (§1, §10).
2. `HistoryError`: add `Removed` and `StaleStore` with explicit arms in `lookup.rs` before `_ => "Failed"`; `NameTaken` rides
   the boxed `NameTakenState` and `lookup.rs` projects `{holder, source}`; map `HistoryError::Unsupported` explicitly (§4.4,
   §10).
3. `HistoryCatalog`: `mark_*`, `retained_roots`, `get_retained_roots`, `remove_unreachable`; both providers implement them,
   default `Unsupported` (§10).
4. Dispatcher snakes every detail and result; builders carry §2's fields and §11's report with `estimate`, `{value, cap}`,
   `zeroReason` (§10, §11).
5. `remove_file`: normalize the absolute POSIX path (strip one leading `/`); reject empty, `/`, `//a`, `a//b`, `a/`, `./a`,
   `a/./b`, `.`/`..`, `\`, NUL, >4096 bytes, >256 components; subtree match only at a `/` boundary over bytes; frontier
   `next_history_serial - 1` read under the Store write lane in the marker transaction; `epoch` bump only on advance; per-call
   trace redaction with `trace_salt` (§4.3).
6. `project.delete`: worker frame, legacy identity save, sidecar `pending|marked|done`, retry order, `home_entry_removed` echo
   (§4); a `project.init` name collision is `NameTaken` (§10).
7. Capacity: add `StorageError::CapacityExceeded` to the predicate, keep PG 53100/53200/53400/54000, map 53300 to `Busy`, drop
   54001, name `SQLITE_FULL`/`SQLITE_TOOBIG`/`SQLITE_NOMEM`, explicit arm before `_ => Refused` (§10).
8. MCP/profile generators, `whyOf`, counts, precedence test, folder-scope refusal (§4).
9. `store.status` op projection and phases; `gc_op` (PG) / home op (SQLite) (§11).

## 6. Spec gaps and questions

1. Only `Removed`, `MountsActive`, `Busy`, `Unresolved` and `NameTaken` have a stated `detail`; `StaleStore`, `Capacity`,
   `Unsupported`, `Conflict` and `ReservedName` have none, and §10's "one detail shape" sentence covers only `Removed`.
2. `Unknown` is prose ("a lost reply is `Unknown`", §4; `project.list(store)` propagates `Busy`/`Unknown`, §4.2) but absent
   from §10's code table and has no detail shape.
3. §11 names fields camelCase (`deadBytesBound`) while §10 requires snake_case rendering; the mapping is implicit.
4. §4.3 refuses a merge `Removed` with `detail {paths: [...]}`, while the canonical detail is `{paths: [{path,
   frontier_serial}]}`; whether the merge echoes full elements is unstated.
5. Only `project.delete` has an idempotent retry contract (§4). For `branch.remove`/`remove_file` a lost reply "stays
   uncertain" (§1) with no re-answer specified.
6. `project.delete` returns `Removed` with `home_entry_removed: false`, where another home's stale entry may persist (hidden
   only by the §4.2 intersection); the result does not name that residue.

Note: the brief's main ref `4e65ebba…` (40 hex) is not a LayerFS ref; `branch get main` reports Layer `3216923362f73a85…`,
root `27872fc7…`; T11 forked `main` as resolved.

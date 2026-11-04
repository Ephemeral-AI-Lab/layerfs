# Call table

[Architecture](../../README.md) › [API](../README.md) › Call table

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The nine call families and the one dispatcher every front end goes through.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-api` |
| Source | [`api/crates/layerfs-api/src`](../../../../api/crates/layerfs-api/src), [`layerfs-control/src/calls`](../../../../api/crates/layerfs-control/src/calls) |
| Uses | [Control](../control/README.md) |
| Used by | [Front ends](../front-ends/README.md) |
| Flows | [A call, end to end](../../flows/call.md) |

`layerfs-api` holds the table's entries, one file per call; the machinery
under them (argument checks, folder scope, result keys and the dispatcher)
lives in `layerfs-control`'s `calls` module, and `layerfs-api` re-exports it
for the front ends ([`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs)).

## Topics

| Topic | What it covers |
| --- | --- |
| [Table](table.md) | `layerfs.table`, keys, scopes and checks. |
| [SDK](../sdk/README.md) | Every family and every call, one page per call (moved out of this component). |
| [Errors](errors.md) | The error catalog and uncertain outcomes. |

## The picture

```text
 layerfs CLI         Node addon            Python module
 (parse.rs)          (layerfs-node)        (layerfs-python)
     |                   |  JSON text          |  JSON text
     | SdkValue          v                     v
     |               layerfs_api::call_json(home, scope, name, args)
     |                   |   (calls/text.rs: parse, then answer as
     v                   v    {value} or {refused})
 layerfs_api::call / call_scoped(home, scope, name, args, events)
     |
     v
 dispatcher  calls/dispatch.rs  (layerfs-control)
   find --> check --> scope.apply --> home --> scope.owns
     |
     v
 Call.run  (layerfs-api/src/<family>/<call>.rs)
     |
     +--> project::family::run           project.*
     +--> store::calls::run              branch.*, commit.*, file.*
     +--> sandbox::family::sandbox       sandbox.*
     +--> workspace::calls::workspace    workspace.*
     +--> machine::<call>                machine.*
     +--> store::family::run / connect   store.*
     +--> backup::family::run            backup.*
     |
     v
 route: in process | machine's worker | Sandbox daemon
 (Routing and workers)
     |
     v
 Outcome = Result<SdkValue, SdkRefusal>  --> keys::snake --> front end
```

The routes are described in [Routing and workers](../routing/README.md); the
control side of each family in [Control: Families](../control/families.md).

## What it does

| Part | What a caller can rely on | Source |
| --- | --- | --- |
| Families | `project`, `branch`, `commit`, `file`, `sandbox`, `workspace`, `machine`, `store`, `backup`: 61 calls, listed in help order | [`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) `families` |
| Arguments | positionals in table order, then options; each has a snake_case name and a `Kind` | [`calls/table.rs`](../../../../api/crates/layerfs-control/src/calls/table.rs) |
| Checks | every argument checked against the entry before any effect | [`calls/check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs) |
| Scope | an optional folder scope guards and rewrites arguments before any effect | [`calls/scope.rs`](../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| Events | one stream per call: `{progress}`, worker events, `{stream, bytes}` of `workspace.exec` | [`common/context.rs`](../../../../api/crates/layerfs-control/src/common/context.rs) |
| Results | a value with snake_case keys, or a refusal `{code, message, detail}` | [`calls/keys.rs`](../../../../api/crates/layerfs-control/src/calls/keys.rs), [`common/outcome.rs`](../../../../api/crates/layerfs-control/src/common/outcome.rs) |
| Home | explicit, else `LAYERFS_HOME`, else `~/.layerfs`; must be absolute | [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) `home` |

Details: [Table](table.md) for kinds, keys, scopes and checks;
[Errors](errors.md) for codes.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every front end uses the same table and dispatcher. | The CLI calls `layerfs_api::call_scoped` ([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)); Node and Python call `call_json`, which calls the same `call_scoped` ([`calls/text.rs`](../../../../api/crates/layerfs-control/src/calls/text.rs)); the guest worker serves host operations, not table calls. |
| A call's name and arguments match across JS, Python and the CLI. | All three build from `Call` values: Node `table`, Python `TABLE` from `table_json`; the CLI parser and help read `layerfs_api::calls()`. |
| One file per call under its family, and nothing else in `src/`. | `the_source_is_one_file_per_call_under_its_family` in [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) |
| A call runs once; the dispatcher retries nothing. | [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) |
| Functions (`on_progress`, exec sinks) never cross into the library. | `check.rs` refuses a `Function` argument |
| Store calls run in the caller's process or the Store's worker; the daemon serves none. | [Routing table](../routing/routing-table.md) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Page size of a list | `page_size` 1 to 1000, default 50, for `sandbox` lists and a scoped `branch.list`; other lists take the Store engine's page rules | [`sandbox/family.rs`](../../../../api/crates/layerfs-control/src/sandbox/family.rs) `page`, [`store/calls.rs`](../../../../api/crates/layerfs-control/src/store/calls.rs) `branches_in` |
| `commit.get` | pages the Branch's Commits 1000 at a time to the end; an id prefix of 8 or more characters matches | `store/calls.rs` `commit_get` |
| A list page that does not move on | `Failed` "a list made no progress"; not asked again | `store/calls.rs` `page_of` |
| Worker frame | 512 MiB; larger is `TooLarge`, nothing sent | [`worker/frame.rs`](../../../../api/crates/layerfs-control/src/machine/worker/frame.rs) |
| Unknown call, unknown or ill-kinded argument, missing positional | `InvalidInput` before any effect | [`check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs), [`dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) |
| A call outside a folder scope | `InvalidInput` `out of scope: …` before any effect | [`scope.rs`](../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `machine.*` in a build without the `machine` feature | `Unsupported` before any effect | e.g. [`machine/create.rs`](../../../../api/crates/layerfs-api/src/machine/create.rs) |

## Demonstration

The commands below are written from source and were not run while writing
this page. They need a Unix host and no Docker or root.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) | `layerfs-api` | 61 calls in nine families; unknown calls and arguments refused before any effect; one test per family that a valid call reaches its family function and a wrong one is refused; JSON round trips; sizes and keys; a relative home refused; one source file per call. |
| [`calls_scope.rs`](../../../../api/crates/layerfs-api/tests/calls_scope.rs) | `layerfs-api` | A folder scope names its project, refuses every call it does not cover, needs a Sandbox of its project for `workspace` calls, and a scoped `branch.list` reads every page. |
| [`project_family.rs`](../../../../api/crates/layerfs-api/tests/project_family.rs) | `layerfs-api` | End to end on this host: `project.init` refused without a Store (`NotFound`), `store.create`, Init in process with `check`, `install`, `record` progress, file and Branch calls, a push to a stand-in worker on a `lima` machine and reads there, then `project.remove`. |

```text
 project_family:
 home(0700) --store.create--> shop (native)
            --project.init--> shop/api   progress: check, install, record
            --file/branch calls--> in process
            --store.push shop box--> stand-in worker (loopback, guest root)
            --file.read--> worker store.call
            --project.remove
```

| Test | Command | Needs |
| --- | --- | --- |
| `calls_table` | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | temporary folders only |
| `calls_scope` | `cargo +1.85.1 test --locked -p layerfs-api --test calls_scope` | one case changes with `--features machine` |
| `project_family` | `cargo +1.85.1 test --locked -p layerfs-api --test project_family` | Unix permissions; loopback TCP |

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`api/README.md`](../../../../api/README.md): the layer overview stays there; the table and dispatcher are covered in [Table](table.md) and this page.
- [`docs/versioned/2.0.0/calls.md`](../../../versioned/2.0.0/calls.md): the user's manual stays; the design is covered in [Table](table.md), the family pages and [Errors](errors.md).

---

← [Routing and workers: Daemon channel](../routing/daemon-channel.md) · [Up: API](../README.md) · [Call table: Table](table.md) →

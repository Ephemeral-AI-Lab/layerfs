# API

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The public surface: the `layerfs-api` call table with its one dispatcher, and the front ends (the `layerfs` command, Node, Python and a local MCP server) that expose its **70 calls in 10 families** with the same names and arguments.

| | |
| --- | --- |
| Layer | API (above [Control](../control/README.md), [Engine](../engine/README.md) and [Core](../core/README.md)) |
| Crates | `layerfs-api`, `layerfs-cli`, `layerfs-node`, `layerfs-python`; JavaScript packages under `api/` |
| Source | [`api/crates`](../../../api/crates), [`api/node`](../../../api/node), [`api/python`](../../../api/python), [`api/cli`](../../../api/cli), [`api/mcp`](../../../api/mcp), [`api/scripts`](../../../api/scripts) |
| Uses | [Control](../control/README.md) (home, machines, Stores, Sandboxes, backups), [Store calls](../engine/store-calls/README.md) |
| Used by | programs, shells and agents |
| Flows | [A call, end to end](../flows/call.md) |

## Picture

```text
 import layerfs from "@layerfs/node"  import layerfs     layerfs <family> <call>
 await layerfs.<family>.<call>(       layerfs.<family>.  <positionals> --options
   positionals..., { options })         <call>(..., k=v)
          |                                  |                   |
   layerfs-node (napi)              layerfs-python (pyo3)    layerfs-cli
          \___________ call_json (text) ____/                    |
                             |                       call_scoped (SdkValue)
                             v                                   v
           layerfs-api: table -> check -> scope -> home -> run once -> log
                             |
                             v
           control crates: in process | machine's worker | Sandbox daemon
```

## The layerfs-api crate

| Part | What it holds | Source |
| --- | --- | --- |
| `families()` | the ten families in help order: `project`, `branch`, `commit`, `file`, `sandbox`, `workspace`, `machine`, `store`, `backup`, `logs` | [`lib.rs`](../../../api/crates/layerfs-api/src/lib.rs) |
| `calls/` | the table types, argument checks, folder scope, dispatcher, argv rendering, key casing, text form, platform policy and call context | [`calls/`](../../../api/crates/layerfs-api/src/calls), [Calls](calls/README.md) |
| `<family>/` | one file per call, `<family>/<call>.rs`; the family's `mod.rs` lists them in help order. Each `run` builds one typed request and calls one control-crate function | [`src/`](../../../api/crates/layerfs-api/src), [SDK](sdk/README.md) |
| `doctor/` | explicit diagnostics outside the call table (`layerfs doctor`) | [`doctor/mod.rs`](../../../api/crates/layerfs-api/src/doctor/mod.rs) |
| Entry points | `calls()`, `find`, `table_json`, `call`, `call_scoped`, `call_json` | [`lib.rs`](../../../api/crates/layerfs-api/src/lib.rs) |
| Re-exports | the control-crate paths the front ends need (`home`, `worker`, `daemon`, `ProjectApi`, `WorkspaceApi`, doctor), so front ends reach control through this crate | [`lib.rs`](../../../api/crates/layerfs-api/src/lib.rs) |
| Features | `machine` (machine calls; host builds only, not the guest worker), `postgresql` (PostgreSQL Stores), `tls` (`https://` remotes); the latter two are on in the sealed Linux worker build | [`Cargo.toml`](../../../api/crates/layerfs-api/Cargo.toml) |

## Calling conventions

These hold for every call in every front end; the [SDK](sdk/README.md) pages
give each call's own arguments, answer and errors.

| Convention | Rule | Source |
| --- | --- | --- |
| Names | `family.call`, snake_case in every front end (`branch.get_commit`, `workspace.exec_cancel`); the CLI writes `layerfs branch get-commit` and options as `--page-size` | [`calls/table.rs`](../../../api/crates/layerfs-api/src/calls/table.rs), [`layerfs-cli/src/parse.rs`](../../../api/crates/layerfs-cli/src/parse.rs) |
| Positionals | required, in table order, from the outside in: owner, Branch, path or new name, other inputs, an optional Commit last | [`calls/table.rs`](../../../api/crates/layerfs-api/src/calls/table.rs) |
| Options | named and optional: a trailing object (JS), keywords (Python), `--flags` (CLI); an unknown or misspelled one is `InvalidInput` before any effect | [`calls/check.rs`](../../../api/crates/layerfs-api/src/calls/check.rs) |
| Kinds | `Name`, `Project` (`<store>/<project>`), `Text`, `Int`, `Bool`, `Json`, `Content` (text or bytes), `Size` (bytes, or `64K`, `512M`, `8G` in powers of 1024), `Function` (stays in the front end) | [Calls](calls/README.md) |
| Answers | a map with snake_case keys, usually with a `kind` naming what happened (`Created`, `UpToDate`, `Committed`, ...); bytes that are not UTF-8 are `{ base64 }` (Node `Buffer`, Python `bytes`) | [`calls/keys.rs`](../../../api/crates/layerfs-api/src/calls/keys.rs), [`calls/text.rs`](../../../api/crates/layerfs-api/src/calls/text.rs) |
| Refusals | `{ code, message, detail }`: Node throws `LayerFSError`, Python raises `layerfs.LayerFSError`, the CLI prints it on stderr and exits 125 (2 for a wrong command line) | [Errors and events](calls/errors-and-events.md) |
| Uncertainty | `Unknown`, and `Failed` with `attempted: true`, mean the outcome is not known; `Unresolved` blocks later calls on that entity. Read state with `get`, `list` or `status` before acting again | [Uncertain outcomes](calls/errors-and-events.md#uncertain-outcomes) |
| One attempt | a call runs once; nothing retries, refreshes or replays it | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Paging | lists that page take `page_size` and `page_pos` and answer `{ items, has_more, next_page_pos }`; pass `next_page_pos` back as `page_pos`. Diffs add `paths_only` and a `cursor`; `logs.list` uses `cursor` and `limit` | [`calls/table.rs`](../../../api/crates/layerfs-api/src/calls/table.rs) `PAGE`, `DIFF` |
| Events | `on_progress` (nine calls) and `workspace.exec`'s `stdout`/`stderr` functions receive events before the call answers; an event never means the call finished | [Errors and events](calls/errors-and-events.md) |
| Home | explicit, else `LAYERFS_HOME`, else `$HOME/.layerfs`; absolute | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home` |
| Scope | a client may be bound to one project, folder and Branch namespace: out-of-scope calls are `InvalidInput`, paths are relative to the folder and Branch names to the namespace | [`calls/scope.rs`](../../../api/crates/layerfs-api/src/calls/scope.rs), [folder scope](calls/README.md#folder-scope) |
| Call log | every call except `logs.*` writes one line to `<home>/trace/calls.jsonl` when it ends (not with `LAYERFS_LOGS=0`) | [the call log](calls/README.md#the-call-log) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every front end exposes exactly the table's calls and arguments | [`coverage.mjs`](../../../api/node/test/coverage.mjs) over the generated [coverage matrix](calls/coverage.md) |
| A call is changed in one place, `layerfs-api/src/<family>/<call>.rs`; front-end functions, types and stubs follow the table | [`calls_table.rs`](../../../api/crates/layerfs-api/tests/calls_table.rs), [`gen-node-types.mjs`](../../../api/scripts/gen-node-types.mjs), [`gen_stubs.py`](../../../api/python/tools/gen_stubs.py) |
| Arguments are checked, scoped and platform-checked before the home is opened or anything happens | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Dependencies point down: `core < engine < control < api`, test code included | [api guide](../../../api/AGENTS.md) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Unknown call or argument, wrong kind, out of scope | `InvalidInput`, before any effect | [`calls/check.rs`](../../../api/crates/layerfs-api/src/calls/check.rs) |
| New work on a removed platform | `Unsupported`, before the home is opened | [`calls/platform_policy.rs`](../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| Where a Store call runs | the caller's process, or the Store machine's worker; no daemon serves a Store call | [Control](../control/README.md) |
| Persistence | Disposable/WAL/synchronous=OFF is the only Store profile; Durable execution is `NOT_RUN — disabled by owner until explicit reauthorization` | [store family](sdk/store.md) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`calls_table.rs`](../../../api/crates/layerfs-api/tests/calls_table.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | any Unix host | 70 calls in ten families; one source file per call; refusals before any effect |
| [`coverage.mjs`](../../../api/node/test/coverage.mjs) | `node --test api/node/test/coverage.mjs` | macOS or Linux, with the built addon, Python module and CLI | every front end matches the table |
| [`file-history.mjs`](../../../api/node/examples/file-history.mjs) | `node api/node/examples/file-history.mjs` | macOS or Linux, with the built addon | a Store, Init, file writes and history reads |

## Topics

| Topic | What it covers |
| --- | --- |
| [Calls](calls/README.md) | the table, checks, folder scope, dispatcher and call log; [errors and events](calls/errors-and-events.md); the [coverage matrix](calls/coverage.md) |
| [SDK](sdk/README.md) | every call, one page per family, with JavaScript, Python and CLI forms |
| [Front ends](front-ends/README.md) | the `layerfs` command and packaging; the Node and Python [bindings](front-ends/bindings.md); the [MCP server](front-ends/mcp.md) |

The [api guide](../../../api/AGENTS.md) holds the binding rules for code in
this layer; these pages explain the design.

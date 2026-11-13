# Calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The call table and the one dispatcher every front end goes through: what a call is, how its arguments are checked, scoped and run once, and how it is logged.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-api` |
| Source | [`layerfs-api/src/calls`](../../../../api/crates/layerfs-api/src/calls): `table.rs`, `check.rs`, `scope.rs`, `keys.rs`, `dispatch.rs`, `render.rs`, `text.rs`, `platform_policy.rs`, `context.rs`; [`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) |
| Uses | [Control](../../control/README.md) (home, machines, Stores, Sandboxes, backups), [Home](../../control/home/README.md) (migration, call log), [Store calls](../../engine/store-calls/README.md) |
| Used by | [Front ends](../front-ends/README.md), [SDK](../sdk/README.md) |
| Flows | [A call, end to end](../../flows/call.md) |

## Picture

```text
 layerfs CLI         Node addon            Python module
 (parse.rs)          (layerfs-node)        (layerfs-python)
     |                   | JSON text           | JSON text
     | SdkValue          v                     v
     |               call_json(home, scope, name, args, client)
     |                   |  text.rs: parse; answer {value} | {refused}
     v                   v
 call_scoped(home, scope, name, args, events, client)    dispatch.rs
   take log_tag --> find --> check --> render argv --> scope.apply
   --> home --> platform_policy --> migrate::open --> ensure_built_ins
   --> scope.owns --> Call.run(Context{home, events}) --> keys::snake
   --> one line in <home>/trace/calls.jsonl
     |
     v
 <family>/<call>.rs  run: one typed request, one function of a control
 crate (layerfs-store, -sandbox, -machine, -control, -home)
     |
     v
 in process | machine's worker | Sandbox daemon     (see Control)
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Table | ten `Family { name, list }`; a `Call` is `{ family, call, args, options, help, run }`, an `Arg` `{ name, kind, optional }`; 70 calls, help order | [`table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs), [`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) `families` |
| JSON form | `table_json()`: `[{ family, call, args: [{ name, kind, optional }], options, help }]`; Node's `table`, Python's `TABLE`; the CLI parser and help read the same `Call` values | `table.rs` `table_json` |
| Shared options | `PAGE` (`page_size`, `page_pos`), `DIFF` (`paths_only`, `page_size`, `page_pos`, `cursor`), `MESSAGE` (`message`), `PROGRESS` (`on_progress`) | `table.rs` |
| Kinds | below | `table.rs` `Kind` |
| Checks | every argument against the entry, before any effect; sizes become bytes; result in table order | [`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| Scope | an optional folder scope guards and rewrites arguments before the home is opened | [`scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| Home | explicit, else `LAYERFS_HOME`, else `$HOME/.layerfs`; must be absolute | [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home` |
| Platform policy | new work on a machine of a removed platform is `Unsupported` before the home is opened, in every build; reads and teardown keep their routes | [`platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| Built-ins | `machine/native/` and `machine/docker/` exist from the first call (builds with the `machine` feature), checked once per process per home | `dispatch.rs` |
| Context | the running call's home and its one event stream: `{ progress }`, a worker's events as they came, `{ stream, bytes }` | [`context.rs`](../../../../api/crates/layerfs-api/src/calls/context.rs), [`events.rs`](../../../../control/crates/layerfs-home/src/events.rs) |
| Keys | result keys become snake_case before they leave the library; only identifier-like keys starting lowercase change, so a path or name key never does (`exitCode` → `exit_code`, `/src/main.rs` stays) | [`keys.rs`](../../../../api/crates/layerfs-api/src/calls/keys.rs) |
| Text form | `call_json` takes and answers JSON text, bytes as `{ "base64": … }`; the answer is `{ value }` or `{ refused: { code, message, detail } }`; arguments or a scope that do not parse are refused and logged as the dispatcher would | [`text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs) |
| Scoped `branch.get` | the answer's `name` is made relative to the namespace again | `dispatch.rs` |
| Step trace | with `LAYERFS_TRACE` and `LAYERFS_TRACE_STEP` set, the call's steps are timed | [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) `stepped` |

| Kind | Accepts | Shell form |
| --- | --- | --- |
| `Name` | a machine, Store, bucket, Branch or Sandbox name, or a Commit id | word |
| `Project` | `<store>/<project>`, checked by `names::project` | word |
| `Text` | a path, URL or free text | word |
| `Int` | a whole number | word |
| `Bool` | true or false | flag |
| `Json` | any JSON value | JSON text on stdin or `--from` |
| `Content` | text or bytes | stdin or `--from` |
| `Size` | bytes, or text `512`, `64K`, `512M`, `8G` (powers of 1024) | word |
| `Function` | `on_progress`, `stdout`, `stderr`; stays in the front end | `--progress`, `--stream` |

Positionals go from the outside in: owner, Branch, path or new name, other
inputs, an optional Commit last. A shell option is the name with `-` for `_`.

### Folder scope

```text
 scope { project: "<store>/<project>", folder: "/abs/path", branch?: ns }
 Scope::parse   unknown field, bad project, non-canonical folder, or a
                branch that is not a history name --> InvalidInput
 Scope::apply   commit.*, branch diff/commits/get_commit,
                file.revision           --> out of scope: history
                file.*                  --> project must match; every path
                                            inside folder (each change of
                                            apply too); branch, commit,
                                            left_*, right_* composed
                branch list             --> project must match; namespace
                branch get/publish/fork/merge --> names composed <ns>.<name>
                workspace.mount         --> only "main", as <ns>.main
                workspace.exec          --> cd '.<folder>' && <command>
                workspace commit/unmount/status/exec_cancel --> unchanged
                anything else           --> out of scope: <family.call>
 Scope::owns    workspace.*: the Sandbox's record names the scope's
                project, else out of scope: sandbox
```

A reference that is a 66-character lowercase hex id stays as given (a Commit
or Layer id); any other value is a Branch name composed as `<ns>.<name>`, and
a name that cannot be composed, or a scope without `branch`, is
`out of scope: branch`. A scope guards call arguments, not processes:
`workspace.exec` under a scope is a `cd` prefix, and the command can reach
anywhere in the mounted tree.

### The call log

Every call made through `call_scoped` (and `call_json`, which uses it) ends
with one line in `<home>/trace/calls.jsonl`, except the `logs` calls and with
`LAYERFS_LOGS=0`
([`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs),
[`render.rs`](../../../../api/crates/layerfs-api/src/calls/render.rs)):

| Field | Meaning |
| --- | --- |
| `v`, `at`, `ms` | version 1; start time (ms since the epoch); duration in ms |
| `client` | `cli`, `node`, `python` or `rust` (`layerfs_api::call`) |
| `argv` | the CLI words that make the same call, from its entry and checked arguments before the scope rewrites them: positionals while present, then `--name value` (a true Bool as a bare flag), then `--scope`, `--scope-project`, `--scope-branch`; a call refused before its arguments were checked logs `[family, call]` |
| `outcome` | `ok`, or the refusal code |
| `kind`, `commit`, `layer` | the result's `kind` and the version it names, for writes that save (`file.*` writes, `workspace.commit`), `branch.fork`, `branch.merge`, `branch.publish` |
| `tag` | a top-level `log_tag` argument (taken out before the check; no entry declares it), else `LAYERFS_LOGS_TAG`; at most 128 bytes |

Values are cut to 200 characters. Content, JSON, and the `Text` arguments
`command`, `message`, `old`, `new`, `password` and `url` are logged only as
`<N bytes>` (`REDACTED`). The line is written after the outcome and never
changes it. Reading and clearing it are the [`logs`](../sdk/logs.md) calls.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every front end goes through the same table and dispatcher | the CLI calls `call_scoped` ([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)); Node and Python call `call_json`, which calls `call_scoped` ([`text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs)) |
| One call, one log line, from its caller; a machine's worker serves host operations, not table calls | [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| A call runs once; nothing retries | `dispatch.rs` |
| No effect before every argument and the scope are checked | `check.rs`, `scope.rs` (applied before `home`) |
| A function never crosses into the library | `check.rs` refuses a `Function` argument as unknown |
| `help` is the one line in each `CALL` entry; nothing else writes help | [`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs) |
| One file per call under its family, nothing else beside `calls/` and `doctor/` | `the_source_is_one_file_per_call_under_its_family` in [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Arguments not a map (null is none) | `InvalidInput` "arguments must be named fields" | [`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| Unknown call | `InvalidInput` "no call named `x`" | [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Unknown argument, or a function passed in | `InvalidInput` "unknown argument `x`" | `check.rs` |
| Missing or null required positional | `InvalidInput` "`x` is required" | `check.rs` |
| Wrong kind; bad size text or negative size | `InvalidInput` "`x` must be …"; "a size such as 512M or 8G" | `check.rs` |
| Outside the folder scope | `InvalidInput` `out of scope: history \| path \| branch \| project \| sandbox \| <family.call>` | [`scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| Relative home, or neither `LAYERFS_HOME` nor `HOME` | `InvalidInput` | `dispatch.rs` `home` |
| New work on a removed platform | `Unsupported` with the removal reason | [`platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `machine.*` in a build without the `machine` feature | `Unsupported` before any effect | e.g. [`machine/create.rs`](../../../../api/crates/layerfs-api/src/machine/create.rs) |
| A scope that covers no Branch namespace | every `file` and `branch` call is `out of scope: branch`; only `workspace` calls other than `mount` pass | `scope.rs` |
| Ids under a scope | a 66-hex Commit or Layer id is taken as given, so a scoped client can read or fork from any id of its project it knows; a Workspace's Branch is not checked, only its Sandbox's project | `scope.rs` `reference`, `owns` |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | any Unix host, temporary folders | ten families and 70 calls; unknown calls and arguments refused before any effect; each family reached; JSON round trips; sizes and keys; a relative home refused; one source file per call |
| [`calls_scope.rs`](../../../../api/crates/layerfs-api/tests/calls_scope.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls_scope` | any Unix host | a scope names its project, refuses every call it does not cover, needs a Sandbox of its project, and a scoped `branch.list` reads every page |
| [`scope_references.rs`](../../../../api/crates/layerfs-api/tests/scope_references.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test scope_references` | any Unix host | fork and publish references outside the namespace refused before any effect; the JSON form refuses the same |
| [`logs.rs`](../../../../api/crates/layerfs-api/tests/logs.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test logs` | any Unix host | one line per call, rendering and redaction, `logs.list` and `logs.reset` |
| [`disabled_platforms.rs`](../../../../api/crates/layerfs-api/tests/disabled_platforms.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test disabled_platforms` | any Unix host | the platform policy refuses new work and keeps reads and teardown |

## Pages

| Page | What it covers |
| --- | --- |
| [Errors and events](errors-and-events.md) | the refusal catalog, uncertain outcomes, refusal order, progress events and `workspace.exec` output |
| [Coverage](coverage.md) | which front end has which call (generated), what is outside the table, intended differences, cross-front-end checks |

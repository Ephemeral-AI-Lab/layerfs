# Calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The call table and the one dispatcher every front end goes through: what a call is, how its arguments are checked, scoped and run once, and how it is logged.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-api` |
| Source | [`layerfs-api/src/dispatch`](../../../../api/crates/layerfs-api/src/dispatch): `table.rs`, `form.rs`, `shape.rs`, `refusal.rs`, `check.rs`, `scope.rs`, `keys.rs`, `dispatch.rs`, `render.rs`, `text.rs`, `platform_policy.rs`, `context.rs`; [`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) |
| Uses | [Control](../../control/README.md) (home, machines, Stores, Sandboxes, backups), [Home](../../control/home/README.md) (opening the home, call log), [Store calls](../../engine/store-calls/README.md) |
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
   take log_tag, detail, with --> find --> check --> render argv --> scope.apply
   --> home --> platform_policy --> home::open --> ensure_built_ins
       (skipped for a call whose entry has opens false: home.init, home.get
       and the six doctors)
   --> scope.owns --> Call.run(Context{home, asked, events}) --> keys::snake
   --> shape (its short or long form, and the sections asked)
   --> one line in <home>/trace/calls.jsonl --> refusal::public (13 codes)
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
| Table | ten `Family { name, list }`; a `Call` is `{ family, call, args, options, help, run, quiet, opens, form, with, paging, profile, scope, policy, saves }`, an `Arg` `{ name, kind, optional, redact, shell, repeat, required }`; 83 calls, help order. Everything the machinery needs to know about one call is on its entry: no file under `calls/` lists call names | [`table.rs`](../../../../api/crates/layerfs-api/src/dispatch/table.rs), [`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) `families` |
| JSON form | `table_json()`: `[{ family, call, args: [{ name, kind, optional, shell?, repeat?, required? }], options, help, form }]` (an option that must be given is `required: true` and `optional: false`); every call's `form` is `{ short: { always, sometimes }, long, parts?, with: { <section>: { where, keys, form? } }, paging, progress, quiet, profile }`; Node's `table`, Python's `TABLE`; the CLI parser and help read the same `Call` values | `table.rs` `table_json` |
| Forms | every entry declares its answer ([forms](#forms)); the dispatcher shapes the one full map its run function returns into the short or the long form | [`form.rs`](../../../../api/crates/layerfs-api/src/dispatch/answer/form.rs), [`shape.rs`](../../../../api/crates/layerfs-api/src/dispatch/answer/shape.rs) |
| Global options | `log_tag`, `detail` (Bool) and `with` (a list of section names, or one comma-separated text) are taken out of the arguments before the check, on every call; no entry declares them. A `with` name the entry does not list is `InvalidInput`, naming the ones it takes | `dispatch.rs` `take_tag`, `take_asked` |
| Refusals | a refusal leaves the dispatcher with one of thirteen public codes; the raised code stays in `detail.code` and in the call log ([errors](errors-and-events.md#refusals)) | [`refusal.rs`](../../../../api/crates/layerfs-api/src/dispatch/answer/refusal.rs), [`outcome.rs`](../../../../control/crates/layerfs-home/src/reply/outcome.rs) `CODES`, `FOLDS` |
| Shared options | `MESSAGE` (`message`, redacted) and `PROGRESS` (`on_progress`) are the two entries use; each list spells its own `limit` and redacted `cursor`. `CURSOR`, `WINDOW` and `FILES` are still defined but no entry uses them, and neither does `PAGE`, the 1.x paging (was `page_size`, `page_pos`; removed with no alias) | `table.rs` |
| Kinds | below | `table.rs` `Kind` |
| Checks | every argument against the entry, before any effect; sizes become bytes; result in table order | [`arguments.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/arguments.rs) |
| Scope | an optional folder scope guards and rewrites arguments before the home is opened; an entry says how a scope treats it (`Call.scope`, a `Rule`: `Outside` by default, `History`, `File(paths)`, `FileChanges`, `Branch*`, `Workspace*`) | [`scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs) |
| Home | explicit, else `LAYERFS_HOME`, else `$HOME/.layerfs`; must be absolute | [`call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs) `home` |
| Platform policy | new work on a machine of a removed platform is `Unsupported` before the home is opened, in every build; reads and teardown keep their routes; an entry says where its machine is found (`Call.policy`, a `Policy`: `None` by default, `Machine`, `Store`, `Project`, `Sandbox`, …) | [`platform_policy.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/platform_policy.rs) |
| Built-ins | `machine/macos-host/` (macOS) or `machine/linux-host/` (Linux) and `machine/docker/` exist from the first call (builds with the `machine` feature), checked once per process per home | `dispatch.rs` |
| Context | the running call's home, what the caller asked of the answer (`context.detail()`, `context.with("tree")`) and its one event stream: `{ progress }`, a worker's events as they came, `{ stream, bytes }` | [`context.rs`](../../../../api/crates/layerfs-api/src/dispatch/context.rs), [`events.rs`](../../../../control/crates/layerfs-home/src/reply/events.rs) |
| Keys | result keys become snake_case before they leave the library; only identifier-like keys starting lowercase change, so a path or name key never does (`exitCode` → `exit_code`, `/src/main.rs` stays) | [`keys.rs`](../../../../api/crates/layerfs-api/src/dispatch/answer/keys.rs) |
| Text form | `call_json` takes and answers JSON text, bytes as `{ "base64": … }`; the answer is `{ value }` or `{ refused: { error, message, fix?, …fields, detail? } }` (the refusal in the form the call asked); arguments or a scope that do not parse are refused and logged as the dispatcher would | [`call_json.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call_json.rs) |
| Scoped `branch.get` | the answer's `name` is made relative to the namespace again (the entry's `Rule::BranchGet`) | `scope.rs` `answer` |
| Step trace | with `LAYERFS_TRACE` and `LAYERFS_TRACE_STEP` set, the call's steps are timed | [`trace.rs`](../../../../control/crates/layerfs-home/src/log/trace.rs) `stepped` |

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
 Scope::apply   commit.*, file.history  --> out of scope: history
                file.* (others)         --> project must match; every path
                                            (path, from, to; each change of
                                            apply too) inside folder; a
                                            left-off glob/grep path is the
                                            folder; branch composed, at and
                                            expected never
                branch list             --> project must match; namespace
                branch get/publish/fork/merge --> names composed <ns>.<name>
                                            (fork's from defaults to <ns>.main)
                workspace.mount         --> only "main", as <ns>.main
                workspace.exec          --> cd '.<folder>' && <command>
                workspace commit/unmount/status/cancel --> unchanged
                anything else (branch.remove, workspace.list, every
                home, project, sandbox, machine, store and backup
                call)                   --> out of scope: <family.call>
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
with one line in `<home>/trace/calls.jsonl`, except a call whose entry is
`quiet` (`home.get`, `home.log`, `home.clear_log`, `sandbox.files`,
`sandbox.log`, `machine.stats`, `machine.files`, `machine.log`: a client
polls them, and a line per poll would bury the calls that changed something) and with `LAYERFS_LOGS=0`
([`call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs),
[`call_log.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call_log.rs)):

| Field | Meaning |
| --- | --- |
| `v`, `at`, `ms` | version 1; start time (ms since the epoch); duration in ms |
| `client` | `cli`, `node`, `python` or `rust` (`layerfs_api::call`) |
| `argv` | the CLI words that make the same call, from its entry and checked arguments before the scope rewrites them: positionals while present, then `--name value` (a true Bool as a bare flag), then `--detail` and `--with a,b` when given, then `--scope`, `--scope-project`, `--scope-branch`; a call refused before its arguments were checked logs `[family, call]` |
| `outcome` | `ok`, or the refusal code as it was raised (before it is folded to its public code) |
| `kind`, `commit`, `layer` | the result's word and the version it names: the answer's `outcome` and `commit`, and, where the entry's `saves` says the call saves (`file` writes, `workspace.commit`, `branch.fork`, `branch.merge`, `branch.publish`), its version |
| `tag` | a top-level `log_tag` argument (taken out before the check; no entry declares it), else `LAYERFS_LOGS_TAG`; at most 128 bytes |

Values are cut to 200 characters. Content, JSON, and an argument whose entry
flags it `redact` (`Arg.redact`: `opt("message", Text).redact()` for free
text, a credential or a URL; `.redact_path()` for a path) are logged only as
`<N bytes>`, so a `project.hide_path` or `file` path never enters the log
(#41 §4.3). Today that is free text (`workspace.exec`'s `command`, `cwd`,
`env` and `stdin`; `message`; `file.edit`'s `old_string` and `new_string`;
`pattern`, `include` and `exclude`; `branch.merge`'s `units`; every
`cursor`), credentials and URLs (`password`, `secret_key`, `url`) and every
path argument of `file`, `commit.diff`, `project.hide_path`,
`workspace.status`, `sandbox.files` and `machine.files` (`path`, and
`from` and `to` of `file.mv` and `file.cp`). A path counts its normalized form,
one leading `/` stripped (`/a` and `a` are both `<1 bytes>`); a glob or grep
`pattern` is not a path and counts its own bytes. A refusal contributes
only its code: its message and `detail` (which may echo the path, as `Removed`
does) are never logged. The line is written after the outcome and never
changes it. Reading and clearing it are `home.log` and `home.clear_log`
([home](../sdk/home.md)).

### Forms

The call surface 2.0 ([specification](../../../../core/docs/issues/call-surface-2-0/README.md))
gives every call one form of answer, declared on its entry as a `Form`. All
83 entries declare one. `home.log`'s entry, from
[`home/log.rs`](../../../../api/crates/layerfs-api/src/families/home/log.rs):

```rust
const ROW: Form = Form::of(
    &["call", "outcome", "logged_at"],          // short keys always there
    &["project", "error"],                      // short keys there when news
    &["call", "outcome", "logged_at", "project", "error", "duration_ms", "client", "argv", "commit", "tag"],
);
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor", "skipped"])
    .parts(&[("items", Part::Rows(&ROW))]);

pub(crate) const CALL: Call = call("home", "log", &[],
    &[/* call, project, client, tag, outcome, since */ opt("limit", Int), opt("cursor", Text).redact()],
    "the calls made against this home, newest first", run)
    .quiet()
    .form(&FORM)
    .paged(Paging::Cursor);
```

| Declared | Meaning | Source |
| --- | --- | --- |
| `Form { always, sometimes, long, parts, usual }` | the keys of one map: a call's top level, an item, a section, a tree node. `long` is complete and ordered; every short key is one. `parts` are the keys whose value has its own form (`Part::Rows(&Form)`, `Part::Scalars(word)`); `usual` names a `sometimes` value that is not news (a Store's `engine: sqlite`) | `form.rs` |
| `.with(&[Section::own(name, keys), …])` | the sections `--with` takes: `own` answers under its name, `item` puts its keys into each item, `top` puts them at the top level; `.shaped(&Form)` gives an `own` section its form | `form.rs` |
| `.paged(Paging::Cursor)` | a list: `limit` and `cursor` in, `items` and `next_cursor` out (15 calls) | `form.rs` |
| `.paged(Paging::Line)` | a line window: `line` and `limit` in (`file.read`, `file.blame`) | `form.rs` |
| `.agent()` | the `agent` profile (the MCP `work` bucket); `full` (`admin`) is the default | `form.rs`, [MCP](../front-ends/mcp.md) |
| `PROGRESS` among the options | the call sends progress events; `Call::progress()` | `table.rs` |

A run function returns one full map and never two forms: it reads
`context.with("tree")` to decide what to read, and `context.detail()` only
if the long form costs a read of its own. `shape::shape` makes the answer:

| Form | Rule |
| --- | --- |
| long (`detail`) | every declared key, in order, null when the run function gave none; each asked section; `ok` true at the top when not given |
| short | the long form projected onto the short keys: an `always` key is kept unless null, a `sometimes` key only when it is news (not null, false, empty, a `state` of `ready`, or its `usual` value); maps without their own form lose their null, false and empty fields; `ok` is dropped when anything else remains, and an answer with nothing left is `{"ok":true}` |
| sections | present only when asked, in both forms; what a section that was not asked for would add is left out silently, so a run function that has it at no cost may answer it anyway |
| a key the entry does not declare | a bug in the run function: a panic naming the call and the key in a debug build, dropped in a release build |

States, refusal fixes and progress come from `layerfs-home`, so control code
writes them the same way: `State::broken(Reason::Missing, Some("layerfs home
init")).write(value)` sets `state`, `reason` and `fix`
([`state.rs`](../../../../control/crates/layerfs-home/src/reply/state.rs));
`machine_state.blocks("machine", name)` gives what needs that machine its
`blocked` state, naming the root of the block; `with_fix(refusal, "layerfs
…")` and `with_next(refusal, "commit", head)` attach a refusal's fix line and
the flat fields its next call needs (`outcome.rs`);
`events.phase("send", Some(done), Some(total), extra)` sends one
`{ phase, done, total }` event ([`events.rs`](../../../../control/crates/layerfs-home/src/reply/events.rs)).

The contract test
[`contract.rs`](../../../../api/crates/layerfs-api/tests/calls/contract.rs) reads
the specification's `calls.json`: every call there is built with the row's
arguments, forms, sections, paging and profile, and the table holds no call
the specification does not name. Its exception file
[`contract_pending.json`](../../../../api/crates/layerfs-api/tests/calls/contract_pending.json)
is empty (`calls`, `old_names`, `not_built` and `spec_sync`); the test's
`PENDING_MUST_BE_EMPTY` switch is still `false`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every front end goes through the same table and dispatcher | the CLI calls `call_scoped` ([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)); Node and Python call `call_json`, which calls `call_scoped` ([`call_json.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call_json.rs)) |
| One call, one log line, from its caller; a machine's worker serves host operations, not table calls | [`call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs) |
| A call runs once; nothing retries | `dispatch.rs` |
| No effect before every argument and the scope are checked | `check.rs`, `scope.rs` (applied before `home`) |
| A function never crosses into the library | `check.rs` refuses a `Function` argument as unknown |
| `help` is the one line in each `CALL` entry; nothing else writes help | [`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs) |
| One file per call under its family, nothing else beside `calls/` (the doctors' shared answer is `calls/doctor/`) | `the_source_is_one_file_per_call_under_its_family` in [`table.rs`](../../../../api/crates/layerfs-api/tests/calls/table.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Arguments not a map (null is none) | `InvalidInput` "arguments must be named fields" | [`arguments.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/arguments.rs) |
| Unknown call | `InvalidInput` "no call named `x`" | [`call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs) |
| Unknown argument, or a function passed in | `InvalidInput` "unknown argument `x`" | `check.rs` |
| Missing or null required positional | `InvalidInput` "`x` is required" | `check.rs` |
| Wrong kind; bad size text or negative size | `InvalidInput` "`x` must be …"; "a size such as 512M or 8G" | `check.rs` |
| Outside the folder scope | `InvalidInput` `out of scope: history \| path \| branch \| project \| sandbox \| <family.call>` | [`scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs) |
| Relative home, or neither `LAYERFS_HOME` nor `HOME` | `InvalidInput` | `dispatch.rs` `home` |
| New work on a removed platform | `Unsupported` with the removal reason | [`platform_policy.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/platform_policy.rs) |
| `machine.*` in a build without the `machine` feature | `Unsupported` before any effect | e.g. [`lifecycle/create.rs`](../../../../api/crates/layerfs-api/src/families/machine/lifecycle/create.rs) |
| A scope that covers no Branch namespace | every `file` and `branch` call is `out of scope: branch`; only `workspace` calls other than `mount` pass | `scope.rs` |
| Ids under a scope | a 66-hex Commit or Layer id is taken as given, so a scoped client can read or fork from any id of its project it knows; a Workspace's Branch is not checked, only its Sandbox's project | `scope.rs` `reference`, `owns` |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`table.rs`](../../../../api/crates/layerfs-api/tests/calls/table.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls table` | any Unix host, temporary folders | ten families and 83 calls; unknown calls and arguments refused before any effect; each family reached; JSON round trips; sizes and keys; a relative home refused; one source file per call |
| [`allowed.rs`](../../../../api/crates/layerfs-api/tests/scope/allowed.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test scope allowed` | any Unix host | a scope names its project, refuses every call it does not cover, needs a Sandbox of its project, and a scoped `branch.list` reads every page |
| [`references.rs`](../../../../api/crates/layerfs-api/tests/scope/references.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test scope references` | any Unix host | fork and publish references outside the namespace refused before any effect; the JSON form refuses the same |
| [`log.rs`](../../../../api/crates/layerfs-api/tests/calls/log.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls log` | any Unix host | one line per call, rendering and redaction, `home.log` and `home.clear_log` |
| [`contract.rs`](../../../../api/crates/layerfs-api/tests/calls/contract.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls contract` | any Unix host; reads `core/docs/issues/call-surface-2-0/calls.json` | every specified call built as its row, nothing pending; every row's long example shapes to its long and short keys; codes, fold, states and the progress event; calls against a real home |
| [`disabled_platforms.rs`](../../../../api/crates/layerfs-api/tests/calls/disabled_platforms.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls disabled_platforms` | any Unix host | the platform policy refuses new work and keeps reads and teardown |

## Pages

| Page | What it covers |
| --- | --- |
| [Errors and events](errors-and-events.md) | the refusal catalog, uncertain outcomes, refusal order, progress events and `workspace.exec` output |
| [Coverage](coverage.md) | which front end has which call (generated), what is outside the table, intended differences, cross-front-end checks |

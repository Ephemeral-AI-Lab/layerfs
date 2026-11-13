# #659 Call log: one line per call, `layerfs logs list|reset`

> **Status:** Proposal; target LayerFS 2.0 (`layerfs_v2_0_0`); not a released contract.
> Issue: [#659](https://github.com/Ephemeral-AI-Lab/layerfs/issues/659).
> Written 2026-11-12 from read-only reviews of `layerfs_v2_0_0` at
> `be317c0456` (call dispatch, call table, CLI, Node and Python front ends,
> `layerfs-home` traces) and of its first client, the dsh-layerfs plugin.
> Re-read `calls/dispatch.rs` and `layerfs-home/src/trace.rs` before starting.

## 1. Outcome

A LayerFS home keeps a history of the calls made against it. Each public
call appends one JSON line to `<home>/trace/calls.jsonl`, saying which
call ran, written as CLI words whichever front end made it, how it ended,
when, for how long, and from which client. `layerfs logs list` reads the
history back and `layerfs logs reset` clears it; Node and Python get the
same two calls from the table.

Done means: §4–§7 are implemented, the pages in §9 are updated, and the
tests in §10 pass on Linux and macOS.

## 2. Settled decisions

| # | Decision |
| --- | --- |
| L1 | One line per public call: every call that goes through `calls::dispatch::call_scoped`. Worker requests (`worker_server::run`), the parts a call sends internally (`project.stage`) and commands outside the table (`help`, `--version`, `worker`, `import-env`, `doctor`) write no line. |
| L2 | The calling process writes the line, once, when the call ends. Lines are in completion order; a running call has no line yet and a killed process writes none. |
| L3 | The file is `<home>/trace/calls.jsonl` (`paths::trace_dir`, reserved since 1.1.0; O26 names `trace/` as the feed). It rotates once to `calls.1.jsonl` past 16 MiB. `log/` is never read or written by this feature. |
| L4 | On by default. `LAYERFS_LOGS=0` turns it off for the process. `LAYERFS_TRACE` and `LAYERFS_TRACE_STEP` keep their 2.0 meaning (worker-connection and step lines) and are independent of this log. |
| L5 | The call is recorded as `argv`, the CLI words that would make the same call, rendered from the call table (§5). There is no separate `op` field: it is `argv[0]` and `argv[1]`. |
| L6 | `outcome` is `"ok"` or the refusal code. LayerFS has no third class: an uncoded failure is already the code `"Failed"` (`layerfs-home/src/outcome.rs`). Refusal messages and `detail` are never logged. |
| L7 | From a result, only `kind` and the saved version it names (`commit` or `layer`, §4.2) are logged. No other result value, output or event. |
| L8 | `client` is `cli`, `node`, `python`, or `rust` for direct library calls (`layerfs_api::call`, tests, embedders). Front ends pass it to dispatch (§8). |
| L9 | A caller may attach an opaque `tag` (at most 128 bytes) to correlate lines with its own records: `LAYERFS_LOGS_TAG` in the environment, or a per-call `log_tag` argument that overrides it (§8). LayerFS never interprets it. |
| L10 | `logs.list` and `logs.reset` are table calls in a new `logs` family, so the shell parser, help, Node and Python get them from the table. They write no line. |
| L11 | A line that cannot be written changes no outcome and is not retried. |

## 3. Facts at the base

`layerfs_v2_0_0` at `be317c0456`.

- **One entry point.** The CLI calls `layerfs_api::call_scoped`
  (`api/crates/layerfs-cli/src/run.rs:200-201`); Node
  (`api/crates/layerfs-node/src/call.rs:114`) and Python
  (`api/crates/layerfs-python/src/call.rs:38`) call `layerfs_api::call_json`,
  which parses JSON and calls `call_scoped` (`calls/text.rs:11-27`).
  `call_scoped` already wraps every call in `trace::stepped`
  (`calls/dispatch.rs:52-63`).
- **The worker does not re-enter dispatch.** Worker requests run through
  `worker_server::run`, not `call_scoped`; the `dispatch.rs:1-3` comment
  saying the guest worker "makes exactly this call" is out of date. One call
  therefore writes one line.
- **The table describes the CLI.** `Call { family, call, args, options }`
  with `Arg { name, kind, optional }` and `Kind = Name | Project | Text | Int
  | Bool | Json | Content | Size | Function` (`calls/table.rs:17-66`). Shell
  options are the option name with `-` for `_`; Bool options are bare flags;
  Content and Json are read from stdin or `--from` (`layerfs-cli/src/parse.rs`).
  The folder scope is `--scope <folder>` with `--scope-project` and
  `--scope-branch` (`parse.rs:25-27`, `:127-129`).
- **Arguments are rewritten after checking.** `scoped` checks the arguments
  against the table, then `scope.apply` rewrites them, for example
  prefixing `workspace.exec`'s command with `cd '<folder>' &&`
  (`calls/scope.rs:235-237`).
- **Client kind is not known at dispatch.** No front end identifies itself;
  Node and Python share `call_json`.
- **Nothing writes `trace/`.** `paths::trace_dir` is `home/trace`
  (`layerfs-home/src/home/paths.rs:94-95`); only the 1.1.0 migration touches
  it. `docs/versioned/2.0.0/migration.md:202` says so. `LAYERFS_TRACE`
  appends with `O_APPEND`, mode 0600, best effort
  (`layerfs-home/src/trace.rs:227-238`).
- **The 1.1.0 call line** carried `method`, an argument summary ("cut to 200
  characters; contents only as byte counts"), `start`, `ms`, `ok`, `code`,
  `kind`, `commit`, `layer`
  (`.agents/skills/layerfs-sdk-observe/references/tracing.md`). This design
  keeps its redaction rule and drops the daemon telemetry.

## 4. The line

```json
{"v":1,"at":1793000004120,"ms":212,"client":"node","argv":["file","write","demo/app","main","src/index.ts","<5321 bytes>"],"outcome":"ok","kind":"Committed","commit":"c0ffee…","tag":"dsh:s_4d2a:a91e"}
{"v":1,"at":1793000005000,"ms":4,"client":"cli","argv":["branch","merge","demo/app","feat","main"],"outcome":"Conflict"}
{"v":1,"at":1793000006000,"ms":30012,"client":"python","argv":["workspace","exec","ws1","<48 bytes>"],"outcome":"Failed"}
{"v":1,"at":1793000007000,"ms":9,"client":"rust","argv":["store","list"],"outcome":"ok"}
```

(Commit and Layer ids are 66 hex characters; shortened here.)

### 4.1 Fields

| Field | Always | Meaning |
| --- | --- | --- |
| `v` | yes | Line format, `1`. |
| `at` | yes | Wall-clock start, milliseconds since the Unix epoch. |
| `ms` | yes | Duration, measured on a monotonic clock. |
| `client` | yes | `cli`, `node`, `python` or `rust` (L8). |
| `argv` | yes | The call as CLI words (§5). |
| `outcome` | yes | `"ok"` or the refusal code (L6). |
| `kind` | no | The result's top-level `kind` when it has one (`Committed`, `UpToDate`, `Forked`, `Merged`, `Added`, `NoChanges`, `Mounted`, …). |
| `commit` | no | The Commit the result saved or names (§4.2). |
| `layer` | no | The Layer the result published or names (§4.2). |
| `tag` | no | The caller's tag (L9). |

A line is one `write` of at most 4 KiB; §5 keeps it under that.

### 4.2 Saved versions

Only these results add `commit` or `layer`; every other line has neither.

| Call | Result `kind` | Logged |
| --- | --- | --- |
| `file.write`, `file.edit`, `file.apply`, `file.rm`, `file.mkdir`, `file.mv`, `file.cp`, `workspace.commit` | `Committed` | `commit` = `commit.id`, else `head` |
| the same | `UpToDate` | `commit` = `head`, else `layer` = `base` |
| `branch.fork` | `Forked` | `commit` = `head`, else `layer` = `base` |
| `branch.merge` | `Merged` | `commit` = `head` |
| `branch.publish` | `Added` | `layer` = `published_layer.id` |
| `branch.publish` | `UpToDate` | `layer` = `layer` |
| `branch.publish` | `NoChanges` | `layer` = `head_layer` |

`kind` tells a new version (`Committed`, `Added`, a non-fast-forward
`Merged`) from an existing one (`UpToDate`, `NoChanges`, `Forked`). A value
that is not a 66-hex id is not logged.

## 5. Rendering and redaction

`argv` is built from the call's table entry and its **checked arguments,
before `scope.apply`**, so it shows what the caller asked for.

1. `family`, then `call` with `-` for `_`.
2. Each positional in table order, while present.
3. Each present option as `--name value` (`-` for `_`); a Bool option as
   `--name` when true and nothing when false.
4. A scoped call adds `--scope <folder>` and, when set, `--scope-project`
   and `--scope-branch`.

Values by argument kind:

| Kind | Rendered |
| --- | --- |
| `Name`, `Project`, `Int`, `Bool`, `Size` | as given |
| `Text` | as given, cut to 200 characters with `…` |
| `Text` named `command`, `message`, `old`, `new`, `password` or `url` | `<N bytes>` |
| `Content`, `Json` | `<N bytes>` (encoded length) |
| `Function` | left out |

The named `Text` arguments carry free text or credentials: exec command
text, commit messages, edit strings, Store passwords and connection URLs
that may embed `user:pass@`. The list lives in one constant in
`calls/render.rs`; a new call with such an argument adds its name there.

A call refused **before** its arguments are checked (unknown call, bad
input) logs `argv` as `[family, call]` only, with the refusal code.

## 6. Writing, rotation and reset

- **Append.** Open `calls.jsonl` with `O_APPEND | O_CREAT`, mode 0600, write
  the line once, close. Nothing is held open, so a long-lived Node or
  Python process never writes into a rotated or reset file. `trace/` is
  created 0700 on first write.
- **Rotation.** After an append, if the file is over 16 MiB, take a
  non-blocking `flock` on `trace/calls.lock`; the holder renames
  `calls.jsonl` to `calls.1.jsonl`, replacing an older one. A writer that
  does not get the lock skips rotation.
- **Reset.** Take the same lock (blocking), truncate `calls.jsonl` in place
  and remove `calls.1.jsonl`. A line written during reset lands before or
  after it; no line is torn.
- **Off.** With `LAYERFS_LOGS=0` nothing is opened.

## 7. `logs.list` and `logs.reset`

A new `logs` family (`api/crates/layerfs-api/src/logs/`). Both calls run in
the caller's process against the home, never start a machine, and write no
line (L10).

### 7.1 `logs.list`

```bash
layerfs logs list --project demo/app --op file.write --limit 50
```

| Option | Kind | Meaning |
| --- | --- | --- |
| `op` | Text | `family` or `family.call` (`file`, `file.write`). |
| `project` | Project | Lines whose `argv` names this project. |
| `client` | Name | `cli`, `node`, `python` or `rust`. |
| `tag` | Text | Exact tag, or a prefix ending in `:` (`dsh:s_4d2a:`). |
| `outcome` | Text | `ok`, or a refusal code. |
| `since` | Int | Lines with `at` at or after this time (ms). |
| `cursor` | Text | `next` from a previous page. |
| `limit` | Int | Default 100, at most 1000. |

Answer, newest first, across `calls.jsonl` then `calls.1.jsonl`:

```json
{"lines":[{"v":1,"at":1793000004120,"…":"…"}],"next":"opaque","has_more":true,"skipped":0}
```

`next` is opaque. After a rotation or reset a cursor is stale: the call
refuses `InvalidInput` and the caller lists from the start. A line that does
not parse (a torn tail after a crash) is counted in `skipped`, not returned.
The CLI prints the answer as JSON, like every call.

### 7.2 `logs.reset`

```bash
layerfs logs reset
```

```json
{"kind":"Reset","removed":1342}
```

`removed` is the number of lines in both files before the reset.

## 8. Client kind and tag

- **Client.** `call_scoped` and `call_json` take a `client: Client`
  (`Cli | Node | Python | Rust`). The CLI passes `Cli` (`run.rs:201`), the
  Node binding `Node` (`layerfs-node/src/call.rs:114`), the Python binding
  `Python` (`layerfs-python/src/call.rs:38`); `layerfs_api::call` passes
  `Rust`. The public `call` signature is unchanged, so existing tests keep
  compiling.
- **Tag.** Dispatch takes a top-level `log_tag` Text argument out of the
  arguments before checking (as `trace::take_scope` does for `observe`), so
  no table entry declares it. Otherwise it reads `LAYERFS_LOGS_TAG`. Values
  over 128 bytes or with control characters are dropped. The CLI has no
  flag for it; a caller sets the environment per process. Node and Python
  pass `log_tag` with any call's arguments; an environment variable applies
  to the whole process, which is wrong for a long-lived SDK host serving
  several callers.

## 9. Changes by file

| File | Change |
| --- | --- |
| `control/crates/layerfs-home/src/home/paths.rs` | `calls_file(home)`, `calls_lock(home)` under `trace_dir`. |
| `control/crates/layerfs-home/src/logs.rs` (new) | `append`, rotation, `read(filter, cursor, limit)`, `reset` (§6, §7). |
| `control/crates/layerfs-home/src/lib.rs` | `pub mod logs`. |
| `api/crates/layerfs-api/src/calls/render.rs` (new) | `argv` from the table entry and checked arguments; redaction (§5); saved versions (§4.2). |
| `api/crates/layerfs-api/src/calls/dispatch.rs` | `call_scoped(…, client)`: take `log_tag`, time the call, render before `scope.apply`, append after the outcome unless the family is `logs` or `LAYERFS_LOGS=0`. Fix the `:1-3` comment about the guest worker. |
| `api/crates/layerfs-api/src/calls/text.rs` | `call_json(…, client)`. |
| `api/crates/layerfs-api/src/lib.rs` | `Client`; `call` passes `Rust`; register the `logs` family. |
| `api/crates/layerfs-api/src/logs/{mod.rs,list.rs,reset.rs}` (new) | The two calls. |
| `api/crates/layerfs-cli/src/run.rs` | Pass `Client::Cli`. |
| `api/crates/layerfs-node/src/call.rs`, `api/node/src/index.js` | Pass `Client::Node`; types for `logs.list`, `logs.reset` and `log_tag`. |
| `api/crates/layerfs-python/src/call.rs`, `api/python/src/layerfs/_client.py` | Pass `Client::Python`; `log_tag=`. |
| `docs/architecture/control/management/home-and-records.md` | `trace/calls.jsonl`, rotation, reset (the Traces section). |
| `docs/architecture/api/call-table/table.md`, `front-ends/{cli,node,python}.md` | The `logs` family, `client`, `log_tag`. |
| `docs/versioned/2.0.0/migration.md` | Lines 202 and 220: `trace/` now holds the call log. |
| `.agents/skills/layerfs-sdk-observe/references/tracing.md` | The call log next to the worker-connection and step lines. |

## 10. Tests

- `api/crates/layerfs-api/tests/logs.rs` (new):
  - one line per call, for a success, a refusal before checking and a
    refusal after it;
  - `argv` rendering: positionals, options, Bool flags, scope options,
    rendered before `scope.apply`;
  - redaction: `Content` and `Json` as byte counts, `Text` cut at 200,
    each named `Text` argument as a byte count;
  - saved versions for each row of §4.2, and none for a read;
  - `client` is `rust` through `layerfs_api::call`;
  - `tag` from `LAYERFS_LOGS_TAG`, overridden by `log_tag`; an oversized
    tag dropped; `log_tag` never reaches the table check;
  - `logs.list` and `logs.reset` write no line; `LAYERFS_LOGS=0` writes
    nothing;
  - eight threads appending at once give whole lines only;
  - rotation past 16 MiB (pre-filled file) and reset of both files;
  - `logs.list` filters, paging, a stale cursor, a torn last line counted
    in `skipped`.
- A worker-routed call (`native` Store on a Linux machine) writes exactly
  one line, from the caller.
- CLI: `client` is `cli`; `layerfs logs list` and `layerfs logs reset`
  answer as in §7; usage errors (exit 2) write nothing.
- Node and Python: `client` is `node` / `python`; `log_tag` lands in `tag`.

## 11. Out of scope

- Daemon, worker and Sandbox internals; individual file writes made by a
  `workspace.exec` command (only the call is logged).
- A subscription or stream of lines (O26: files are the feed).
- Shipping lines off the machine, retention beyond one rotation, or a
  per-project log.
- Changing what `LAYERFS_TRACE` writes.

## 12. Open questions

| # | Question |
| --- | --- |
| Q1 | Status and list polling dominate the file (a host polling Store health every 10 s writes 8,640 lines a day). Add a read/write bit to `Call` so `logs.list --writes` can hide reads, or log only mutating calls by default? |
| Q2 | Should Node and Python also take a client-level default `log_tag`, with the per-call argument overriding it? |
| Q3 | For `url`, strip only `user:pass@` and the query instead of the whole value? |

## 13. First client: dsh-layerfs

The DeepSeek Harness plugin drives LayerFS by spawning the CLI for every
call and wraps agent shells' `layerfs` in a managed entry point. With this
log it:

- sets `LAYERFS_LOGS_TAG=dsh:<sessionId>:<receiptId>` on each spawn, with
  one new receipt id per call;
- keeps its own session receipts (`{receipt, origin, sessionId, turn,
  toolCallId, versions}`) inside the session, because the log is per
  machine and can be reset while a session moves between machines;
- binds each checkpoint to the receipt of the call that saved it, and
  reads `logs.list --tag dsh:<sessionId>:` to show a session's calls;
- shows the home's history on a Logs page with `logs.list` and
  `logs.reset`.

This replaces the plugin's own CLI-output parsing and receipt spool.

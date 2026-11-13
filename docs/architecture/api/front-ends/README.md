# Front ends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `layerfs` command, the Node and Python bindings and the MCP server: each turns its own input into one call of the table, makes it once, and shows the answer, refusal and events in its own form.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-cli`, `layerfs-node`, `layerfs-python`; packages `@layerfs/cli`, `@layerfs/node`, `layerfs` (Python), `@layerfs/mcp` |
| Source | [`api/crates/layerfs-cli/src`](../../../../api/crates/layerfs-cli/src), [`api/cli`](../../../../api/cli), [`api/scripts`](../../../../api/scripts); bindings and MCP on their pages |
| Uses | [Calls](../calls/README.md) |
| Used by | shells, programs, MCP clients |
| Flows | [A call, end to end](../../flows/call.md) |

## Picture

```text
  shell words           JS function call         Python function call
       |                      |                         |
  layerfs-cli          api/node index.js         api/python _client.py
  parse.rs, run.rs     bind(): split args        _bind(): split args
       |               layerfs-node (napi)       layerfs-python (pyo3)
       | SdkValue             | JSON text               | JSON text
       v                      v                         v
  call_scoped          call_json                 call_json   (layerfs-api)
       |                      |                         |
  stdout JSON / exit   value | LayerFSError      value | LayerFSError
  --progress --stream  on_progress, stdout,      on_progress, stdout,
  --events             stderr functions          stderr functions

  MCP server (api/mcp) ---> Node binding, one tool per call
```

Every front end builds its functions or sub-commands from the same table
([`layerfs_api::families`](../../../../api/crates/layerfs-api/src/lib.rs), or
its JSON form `table_json`) and makes exactly one library call per function
call. Functions given as options stay in the front end, which feeds them the
call's events.

## What it does

| Front end | Turns its input into | Library entry | Answer | Source |
| --- | --- | --- | --- | --- |
| `layerfs` command | positionals in table order, then `--options`; content and JSON positionals from stdin or `--from` | `call_scoped` | one JSON line on stdout; a refusal as JSON on stderr, exit 125 | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs), [`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs) |
| Node | positionals, then an options object | `call_json` | a resolved value, or a thrown `LayerFSError` | [bindings](bindings.md#layerfs-node) |
| Python | positionals; optional positionals and options as keywords; `layerfs.aio` as coroutines | `call_json` | a value, or a raised `LayerFSError` | [bindings](bindings.md#layerfs-python) |
| MCP | one tool per call in a launch profile | the Node binding | a JSON text block, or `isError` | [MCP](mcp.md) |

Entries outside the table: `doctor` in every front end (report-only,
[Doctor](../../control/doctor/README.md)); `close` in Node and Python
(`close_idle(0)`: close this process's idle Stores and drop their leases; the
CLI does it after every call); in the CLI only, `worker`, `import-env`,
`help` and `--version`.

### The layerfs command

```text
layerfs <family> <call> <positionals...> [--options] [switches]
  none | help | -h | --help  -> help::print                 (exit 0 or 2)
  --version [--json]         -> version / capabilities      (no home)
  help --json                -> {calls, commands, switches} (no home)
  worker                     -> worker::main                (outside table)
  import-env                 -> home::import_env            (outside table)
  doctor [target ...]        -> doctor::main                (outside table)
  <family> <call>            -> find -> parse -> Scope::parse
                                -> call_scoped once -> close_idle(0)
                                -> print result or refusal
```

Every word must be UTF-8 ([`main.rs`](../../../../api/crates/layerfs-cli/src/main.rs));
one that is not is exit 2 before anything runs.
[`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs) reads one call's
words against its entry:

| Word | Becomes |
| --- | --- |
| a word not starting with `-` (or `-` itself) | the next positional, in table order; the count must lie between the required and the total, else exit 2 |
| `--` | every later word is a positional |
| `--<name> <value>`, `--<name>=<value>` | option `<name>` with `-` read as `_`, parsed by its kind (`Int`, `Bool` `true`/`false`, `Json`, `Size` such as `8G`, else text) |
| `--<bool-option>` alone | `true`; `--force=false` gives false |
| `--<text-argument>-from <file>` | that optional `Text` option read from the file |
| `-m <text>` | the `message` option; exit 2 when the call has none |
| content or JSON positional | read from stdin, or `--from <file>`; text when UTF-8, else bytes (JSON must be UTF-8); not counted on the line |
| any other `--x` | exit 2, listing the call's options |

Values are only shaped here; the library checks them again
([`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs)). An
unknown call suggests the closest one in its family
([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)).

| Switch | Calls | Effect |
| --- | --- | --- |
| `--text` | all | readable output: a result's `text` field as is, a list of records as an aligned table, else `key  value` lines; timestamps (`created`, `modified`, `at`) as UTC dates and byte fields in binary units ([`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs)) |
| `--progress` | the nine with `on_progress` | a status line on a terminal stderr, else one JSON line per event on stderr; `LAYERFS_PROGRESS_JSON` forces JSON lines |
| `--stream` | `workspace exec` | output events to stdout and stderr; no result JSON; exit with the command's `exit_code` (1 when it does not fit 0 to 255, 0 when there is none) |
| `--events` | `workspace exec` | framed NDJSON on stdout, then one `{result}` or `{refusal}` ([`framed.rs`](../../../../api/crates/layerfs-cli/src/framed.rs)); exclusive with `--stream` and `--text` |
| `--scope <folder>`, `--scope-project`, `--scope-branch` | all | a folder scope; `--scope` needs `--scope-project`; a bad scope is a refusal (125), the flags alone are exit 2 |

`layerfs help [<family> [<call>]]` and `layerfs <family> <call> -h` print
usage lines built from the entry ([`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs)):
`<name>` required, `[<name>]` optional, `[--name <name>]` per option (`[--name]`
for a `Bool`, `[--progress]` for `on_progress`). The banner
([`banner.rs`](../../../../api/crates/layerfs-cli/src/banner.rs)) prints only
when stdout is a terminal; `NO_COLOR` turns colour off. `--version --json`
gives `{ version, commit, dirty, features }` and `help --json` gives
`{ calls, commands, switches }` with `calls` exactly `table_json`
([`capabilities.rs`](../../../../api/crates/layerfs-cli/src/capabilities.rs)).

`layerfs worker` serves a machine's worker
([`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs)): it reads
`LAYERFS_WORKER_LISTEN`, `LAYERFS_WORKER_SECRET` (a file holding the secret)
and, for tests, `LAYERFS_WORKER_ROOT` (default `/layerfs/data`); a missing
setting exits 125. The protocol is in
[workers and reach](../../control/machines/workers-and-reach.md).

Each table call appends one call-log line with `client: "cli"`;
`LAYERFS_LOGS_TAG` sets its tag and `LAYERFS_LOGS=0` turns the log off
([`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs)).
A wrong command line, `help`, `--version`, `worker`, `import-env` and `doctor`
write none.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Front ends add no behaviour beyond argument and output shape: each function is built from the table and calls the library once | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs), [`index.js`](../../../../api/node/src/index.js) `bind`, [`_client.py`](../../../../api/python/src/layerfs/_client.py) `_bind`; the [coverage matrix](../calls/coverage.md) |
| No call, argument or result name is written in a front end | [api guide](../../../../api/AGENTS.md); generated types and stubs |
| Nothing is retried in a front end | [`calls/text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs), [`framed.rs`](../../../../api/crates/layerfs-cli/src/framed.rs) |
| A wrong call is refused the same way everywhere: the checks are the library's | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs); compared by `shell.mjs` and `test_calls.py` |
| Functions (`on_progress`, `stdout`, `stderr`) never cross into the library | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| LayerFS reads `LAYERFS_*` settings only and names no plugin | [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home`, [`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |
| The CLI closes the Stores a call opened before it exits, so a sqlite Store leaves no write-ahead log | `close_idle(Duration::ZERO)` in [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| CLI exit codes | 0 result; 2 wrong command line (`layerfs: <message>` on stderr), nothing called; 125 refusal (`{"code","message","detail"}` on stderr) | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `USAGE`, `REFUSED` |
| stdout cannot be written after the call | 125; the call was made and its effects stay; nothing is repeated | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs), [`framed.rs`](../../../../api/crates/layerfs-cli/src/framed.rs) |
| stderr cannot be written | the line is lost; the exit stays the outcome's | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) `diagnostic` |
| `--stream`: a command exiting 2 or 125 | same exit as a usage error or refusal; stderr tells them apart | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |
| Cargo features | Node: `machine` always, `postgresql`/`tls` opt-in; Python and CLI: `machine` by default; without `postgresql`/`tls` such a call is `Unsupported` | the three crates' `Cargo.toml` |
| Durable Stores | `NOT_RUN — disabled by owner until explicit reauthorization`; every build runs Disposable/WAL only | [store family](../sdk/store.md) |

## Packaging and release

| Artifact | Built by | Contents | Platform |
| --- | --- | --- | --- |
| `@layerfs/cli` 2.0.0-rc.1 ([`api/cli`](../../../../api/cli/package.json)) | `cargo +1.85.1 build --release --locked -p layerfs-cli`, copy to `api/cli/bin/layerfs`, `npm pack` (`prepack` refuses without the binary) | `bin/layerfs`, `LICENSE`, `README.md`, `package.json` | `os` linux, `cpu` x64, `libc` glibc; Node 22 or later |
| CLI tarball | [`build-cli-dist.mjs`](../../../../api/scripts/build-cli-dist.mjs) `<output-dir>`: release build with paths remapped, then `layerfs-<version>-x86_64-unknown-linux-gnu.tar.gz` and `SHA256SUMS` | `layerfs`, `LICENSE`, `README.md` | runs on linux x64 only; the Cargo target must stay in the worktree |
| `@layerfs/node` 2.0.0-rc.1 ([`api/node`](../../../../api/node/package.json)) | [`build-native.mjs`](../../../../api/scripts/build-native.mjs) (release by default), `npm pack` in `api/node` (`prepack` refuses without `layerfs.node`) | `src/` (with `index.d.ts`), `layerfs.node`, `LICENSE`, `README.md`, `package.json` | linux, x64, glibc; Node 22 or later |
| `layerfs` Python 2.0.0 ([`pyproject.toml`](../../../../api/python/pyproject.toml)) | `cd api/python && maturin build --locked` | `layerfs` with `_native` (abi3), stubs, `LICENSE` | Python 3.10 or later; not on PyPI |
| `@layerfs/mcp` 2.0.0 | not packed | | private |

Both npm packages publish under the `rc` tag with public access
(`npm publish --tag rc --access public`); npm also sets `latest` on a
package's first publish. npm checks the libc family, not its version: a glibc
older than the one the binary was built against installs and then fails to
load. Publishing, logging in and rollback are the owner's steps; no token or
`.npmrc` belongs in the repository, a command line or a log. A published
version is never reused: roll back by `npm deprecate`, by moving a dist-tag,
or by `npm unpublish` within npm's policy.

## Demonstration

Build first (`--no-run`), then each under a 120 s wall limit.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`exit_codes.rs`](../../../../api/crates/layerfs-cli/tests/exit_codes.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test exit_codes` | Unix | 0, 2 and 125 as above |
| [`stream.rs`](../../../../api/crates/layerfs-cli/tests/stream.rs), [`stream_framed.rs`](../../../../api/crates/layerfs-cli/tests/stream_framed.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test stream --test stream_framed` | Unix; the live half needs `LAYERFS_DAEMON`, else `NOT_RUN` | `--stream` passes output and the command's exit; `--events` frames |
| [`progress.rs`](../../../../api/crates/layerfs-cli/tests/progress.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test progress` | Unix | progress lines on stderr; refusal before any event; closed stderr |
| [`capabilities.rs`](../../../../api/crates/layerfs-cli/tests/capabilities.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test capabilities` | Unix | `--version --json`, `help --json` without a home |
| [`package.mjs`](../../../../api/cli/test/package.mjs) | `node --test api/cli/test/package.mjs` | any | the CLI package's licence, platform fields and file list |

## Pages

| Page | What it covers |
| --- | --- |
| [bindings](bindings.md) | `layerfs-node` with `api/node`, and `layerfs-python` with `api/python`: loading, values, events, types, tests |
| [mcp](mcp.md) | the private stdio MCP server: launch, export allowlist, profiles, schemas |

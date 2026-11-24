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
  --detail --with      stderr functions          stderr functions

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

Entries outside the table: `close` in Node and Python
(`layerfs_api::close()`: close this process's idle Stores and drop their
leases, then end its idle Sandbox daemon sessions inside 250 ms; the CLI
calls it after every call); in the CLI only, `worker`, `help` and
`--version`. The six doctors are table calls of their families
([Doctors](../../control/doctor/README.md)); the 1.x `doctor` entry outside
the table, in every front end, was removed with no alias.

### The layerfs command

```text
layerfs <family> <call> <positionals...> [--options] [switches]
  none | help | -h | --help  -> help::print                 (exit 0 or 2)
  --version [--json]         -> version / capabilities      (no home)
  help --json                -> {calls, commands, switches} (no home)
  worker                     -> worker::main                (outside table)
  <family> <call>            -> find -> parse -> Scope::parse
                                -> call_scoped once -> layerfs_api::close()
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
| `-m <text>` | the `message` option (nine calls); exit 2 when the call has none |
| `--detail` | the long form (every call) |
| `--with <a,b>` | the named sections (the ten calls that list some); a name the call does not list is exit 2 |
| a secret option | read from stdin with its own flag only: `store connect --password-stdin`, `backup connect --key-stdin` |
| content or JSON positional (`file.write`, `file.apply`) | read from stdin, or `--from <file>`; text when UTF-8, else bytes (JSON must be UTF-8); not counted on the line |
| any other `--x` | exit 2, listing the call's options |

Values are only shaped here; the library checks them again
([`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs)). An
unknown call suggests the closest one in its family
([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)).

| Switch | Calls | Effect |
| --- | --- | --- |
| `--text` | all | readable output: a result's `text` field as is, a list of records as an aligned table, else `key  value` lines; timestamps (`created`, `modified`, `at`) as UTC dates and byte fields in binary units ([`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs)) |
| `--progress` | the 19 with `on_progress` | a status line on a terminal stderr, else one JSON line per event on stderr; `LAYERFS_PROGRESS_JSON` forces JSON lines |
| `--stream` | `workspace exec` | output events to stdout and stderr; no result JSON. With or without it, `workspace exec` exits with the command's `exit_code` (1 when it does not fit 0 to 255, 0 when there is none) |
| `--scope <folder>`, `--scope-project`, `--scope-branch` | all | a folder scope; `--scope` needs `--scope-project`; a bad scope is a refusal (125), the flags alone are exit 2 |

`layerfs help [<family> [<call>]]` and `layerfs <family> <call> -h` print
usage lines built from the entry ([`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs)):
`<name>` required, `[name]` optional (as the rows write it), `[--name <name>]` per option (`[--name]`
for a `Bool`, `[--progress]` for `on_progress`). The banner
([`banner.rs`](../../../../api/crates/layerfs-cli/src/banner.rs)) prints only
when stdout is a terminal; `NO_COLOR` turns colour off. `--version --json`
gives `{ version, commit, dirty, features }` (`features`: `postgresql`, always
on because the engine is built into every artifact; the Cargo features
`machine` and `tls` when built in; and `sandbox-runtime` when namespaces
Sandboxes run on the bundle's runtime layer, R5) and `help --json` gives
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
A wrong command line, `help`, `--version`, `worker` and the
eight `quiet` calls write none.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Front ends add no behaviour beyond argument and output shape: each function is built from the table and calls the library once | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs), [`index.js`](../../../../api/node/src/index.js) `bind`, [`_client.py`](../../../../api/python/src/layerfs/_client.py) `_bind`; the [coverage matrix](../calls/coverage.md) |
| No call, argument or result name is written in a front end | [api guide](../../../../api/AGENTS.md); generated types and stubs |
| Nothing is retried in a front end | [`calls/text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs), [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |
| A wrong call is refused the same way everywhere: the checks are the library's | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs); compared by `shell.mjs` and `test_calls.py` |
| Functions (`on_progress`, `stdout`, `stderr`) never cross into the library | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| LayerFS reads `LAYERFS_*` settings only and names no plugin | [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home`, [`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |
| The CLI closes the Stores a call opened before it exits, so a sqlite Store leaves no write-ahead log | `layerfs_api::close()` in [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs), which runs `close_idle(Duration::ZERO)` ([`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs)) |
| A one-shot process ends its idle Sandbox daemon sessions before it exits, so its exit is not a dropped connection in the daemon's log (layerfs-project#181). Best effort inside 250 ms in all (`SESSION_END`), sent once, after the call's result is ready; a lent or failed connection, or a daemon that does not answer in time, is dropped as before. The daemon's side is in [the daemon channel](../../control/sandboxes/daemon-channel-and-exec.md) | `end_idle_sessions` in [`registry.rs`](../../../../control/crates/layerfs-sandbox/src/registry.rs), `Controls::end_idle` in [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| CLI exit codes | 0 result (`workspace exec`: the command's code); 2 wrong command line (`layerfs: <message>` on stderr), nothing called; 125 refusal (`{"error","message","fix",…}` on stderr, with `detail` under `--detail`) | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `USAGE`, `REFUSED` |
| stdout cannot be written after the call | 125; the call was made and its effects stay; nothing is repeated | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) |
| stderr cannot be written | the line is lost; the exit stays the outcome's | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) `diagnostic` |
| `workspace exec`: a command exiting 2 or 125 | same exit as a usage error or refusal; stderr tells them apart | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |
| Cargo features | Node: `machine` always, `tls` opt-in; Python and CLI: `machine` by default; without `tls` an `https://` remote is `Unsupported`. PostgreSQL is always built | the three crates' `Cargo.toml` |
| Durable Stores | `NOT_RUN — disabled by owner until explicit reauthorization`; a sqlite Store runs WAL with `synchronous=NORMAL` (record label `normal`) | [store family](../sdk/store.md) |

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
| [`stream.rs`](../../../../api/crates/layerfs-cli/tests/stream.rs), [`stream_exit.rs`](../../../../api/crates/layerfs-cli/tests/stream_exit.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test stream --test stream_exit` | Unix; `stream.rs`'s live half needs `LAYERFS_DAEMON`, else `NOT_RUN`; `stream_exit.rs` uses a stand-in worker | `--stream` passes output; `workspace exec` exits with the command's code with or without it |
| [`doctor.rs`](../../../../api/crates/layerfs-cli/tests/doctor.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test doctor` | Unix | `layerfs home doctor` prints one report and changes nothing; `home leftovers` and `home prune` exit 2 |
| [`progress.rs`](../../../../api/crates/layerfs-cli/tests/progress.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test progress` | Unix | progress lines on stderr; refusal before any event; closed stderr |
| [`capabilities.rs`](../../../../api/crates/layerfs-cli/tests/capabilities.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test capabilities` | Unix | `--version --json`, `help --json` without a home |
| [`package.mjs`](../../../../api/cli/test/package.mjs) | `node --test api/cli/test/package.mjs` | any | the CLI package's licence, platform fields and file list |

## Pages

| Page | What it covers |
| --- | --- |
| [bindings](bindings.md) | `layerfs-node` with `api/node`, and `layerfs-python` with `api/python`: loading, values, events, types, tests |
| [mcp](mcp.md) | the private stdio MCP server: launch, export allowlist, profiles, schemas |

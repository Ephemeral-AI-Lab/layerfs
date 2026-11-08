# CLI

[Architecture](../../README.md) › [API](../README.md) › [Front ends](README.md) › CLI

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `layerfs` command.

| | |
| --- | --- |
| Component | [Front ends](README.md) |
| Source | [`api/crates/layerfs-cli/src`](../../../../api/crates/layerfs-cli/src) |

## Parsing

```text
layerfs <family> <call> <positionals...> [--options] [switches]
   |
   v  run.rs main: first word
   +-- none | help | -h | --help --> help::print          (exit 0 or 2)
   +-- --version ------------------> "layerfs <version>"  (exit 0)
   +-- worker (alone) -------------> worker::main          (outside table)
   +-- import-env (alone) ---------> home::import_env      (outside table)
   +-- doctor [target ...] [--text] -> layerfs_api::doctor / doctor_with_options (outside table)
   +-- <family> <call> ------------> layerfs_api::find("family.call")
                                        |
                                        v
                            parse.rs parse(call, words, stdin)
                                        |  Usage error --> exit 2
                                        v
                            run.rs one: Scope::parse, then
                            layerfs_api::call_scoped(None, scope,
                                  name, args, &mut send)  -- once
                                        |
                                        v
                            close_idle(0); print result or refusal
```

Every word must be UTF-8: `main.rs` reads the command line with `args_os`,
and a word that is not UTF-8 is exit 2 (`layerfs: argument <n> is not valid
UTF-8: "<escaped>"`) before anything runs. A path that is not UTF-8 cannot be
named on the command line, because the calls take text.

`parse` reads one call's words against its table entry
([`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs)):

| Word | Becomes | Rule |
| --- | --- | --- |
| a word not starting with `-` (or `-` itself) | the next positional, in table order | count must lie between the required and the total number of on-line positionals, else exit 2 |
| `--` | every later word is a positional | |
| `--<name> <value>` or `--<name>=<value>` | option `<name>` with `-` read as `_` (`--start-line` → `start_line`) | parsed by the option's `Kind`: `Int` a whole number, `Bool` `true`/`false`, `Json` JSON text, `Size` a number or text such as `8G`, the rest text |
| `--<bool-option>` alone | `true` | `--force=false` gives false |
| `--<text-option>-from <file>` | option `<name>` read from the file | only for an optional `Text` option |
| `-m <text>` | the `message` option | exit 2 when the call has none |
| content or JSON positional (`file write` content, `file apply` changes) | read from stdin, or from `--from <file>` | not counted on the line; JSON must be UTF-8 JSON |
| a function option (`on_progress`, `stdout`, `stderr`) | never an option | replaced by `--progress` and `--stream` |
| any other `--x` | exit 2 | `unknown option` |

Content read from stdin or a file is text when it is UTF-8, else bytes
(JSON must be UTF-8: other bytes are exit 2). stdin is read to its end only
when a content or JSON positional needs it, and not at all when `--from` is
given; a stdin that is closed or empty is empty content, so `file write`
writes an empty file and `file apply` is exit 2 (empty is not JSON). A
`--from` file that cannot be read is exit 2.
Values are only shaped here; the library checks them again
([`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs)),
so a value the shell accepts can still be refused `InvalidInput`.

| Switch | Calls | Effect |
| --- | --- | --- |
| `--text` | all | readable output instead of JSON |
| `--progress` | calls with `on_progress` | each `{progress}` event as one JSON line on stderr |
| `--events` | calls with output events (`workspace exec`) | NDJSON progress and output on stdout, then exactly one `{result}` or `{refusal}`; exclusive with `--stream` and `--text` |
| `--stream` | calls with a `stream` option (`workspace exec`) | sets `stream: true`; output events go to stdout and stderr; exit with the command's code |
| `--scope <folder>` | all | a folder scope; needs `--scope-project <store>/<project>` |
| `--scope-project`, `--scope-branch` | with `--scope` | the scope's project and Branch namespace; alone they are exit 2 |

The scope is checked by `Scope::parse` before the call; a bad scope is a
refusal (exit 125), not a usage error. Scopes are described on
[Table](../call-table/table.md#scopes).

## Help

```text
layerfs help                  banner (TTY only), usage line, every family
                              with its calls, then doctor, import-env,
                              worker and help
layerfs help <family>         each call's usage line and help text
layerfs help <family> <call>  one call
layerfs <family> <call> -h    the same as the line above
layerfs help doctor           legacy usage and the explicit target commands
layerfs doctor -h             the same (also --help); exit 0
```

The usage line is built from the entry
([`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs) `usage`):
required positionals as `<name>`, optional ones as `[<name>]`, each option as
`[--name <name>]` (a `Bool` as `[--name]`, `on_progress` as `[--progress]`,
`stdout`/`stderr` left out), and `[--from <file>] (content from stdin)`
when a positional is content or JSON. The one-line text is the entry's
`help`. An unknown topic is exit 2; `import-env` and `worker` have no help
topic (a word after either is read as a family name, so `import-env --help`
is exit 2). The banner
([`banner.rs`](../../../../api/crates/layerfs-cli/src/banner.rs)) prints
only when stdout is a terminal; `NO_COLOR` turns its colours off.
`help` and `--version` need no home.

## Machine-readable discovery

`layerfs --version` keeps its `layerfs <version>` first line.
`layerfs --version --json` prints `{version, commit, dirty, features}`: the
full source commit and dirty state are embedded at build time, and features
lists enabled `machine`, `postgresql` and `tls` features. A dirty build is
not identified by the commit alone. Both forms need no home.

`layerfs help --json` prints `{calls, commands, switches}` in one JSON line.
`calls` is exactly `layerfs_api::table_json`; `commands` lists the outside-table
commands by shell name with positional arguments and options, including every
Doctor target, `import-env` and `worker`. `switches` lists each front-end switch
and the API call names it applies to, including help/end-of-options markers
and the generated `--<text-argument>-from` spellings. Table option names use underscores;
CLI spellings use hyphens. Discovery neither opens nor creates a home.

## Output

| Outcome | Printed | Exit | Source |
| --- | --- | --- | --- |
| success | the result as one line of JSON on stdout | 0 | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) `result` |
| success with `--text` | a result's `text` field as is; a list of records as an aligned table of their scalar fields (upper-case headers, nested values left out); else `key  value` lines | 0 | `readable`, `table` |
| refusal | `{"code","message","detail"}` on stderr | 125 | `refusal` |
| wrong command line | `layerfs: <message>` on stderr | 2 | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `wrong` |
| stdout cannot be written while printing (reader gone, disk full) | nothing more; the call was already made and its effects stay | 125 | `print`; help and `--version` print the same way |
| `doctor` | the legacy report as JSON, or with `--text` findings and fixes; explicit target commands also show outcomes and incomplete coverage | 0 for a produced report, including faults; usage 2, refusal 125 | `findings`, explicit `checks` and `complete` |

In `--text` mode, scalar fields defined by the call table or docs as epoch-millisecond timestamps (`created`, `modified`, `at`) are rendered as `YYYY-MM-DD HH:MM:SS` UTC. Fields defined as byte counts (`size`, `bytes`, `part_size`, `memory`, `disk`, `free`, `free_disk`, `wal_bytes`, `incoming_bytes`, `stdout_bytes`, `stderr_bytes`, `logical`, `new_canonical`, `new_stored`, `new_metadata`) are rendered in human units (B, KiB, MiB, GiB, TiB, PiB). When stdout is a terminal, table headers are displayed in bold colour (reset afterwards, respecting `NO_COLOR`). Help section titles (`USAGE`, `COMMANDS`) are similarly highlighted on terminals.

When a user makes a mistake on the command line:
- An unknown call suggests the closest existing call in that family (or closest family) when one is close (e.g. `did you mean layerfs store get?`).
- An unknown option lists the options accepted by that call.
The existing error message remains the first line, and exit code remains 2.

A stdout that is closed before the process starts (`>&-`) is not an error:
the Rust runtime reopens it on `/dev/null`, so the call is made, nothing is
shown and the exit is 0. A stderr that cannot be written loses its line
(`output::diagnostic`) and the exit code stays the outcome's own, 2 or 125.
No path of the CLI panics on a closed or failing standard stream.

After every table call, before printing, the command runs
`layerfs_api::close_idle(Duration::ZERO)`: the Stores the call opened are
closed, so a SQLite Store leaves no write-ahead log behind
([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)).

## Framed exec output

`workspace exec --events` writes ordered NDJSON to stdout: `{progress: ...}`,
`{stream: "stdout"|"stderr", bytes: ...}`, then exactly one `{result: ...}`
or `{refusal: {code,message,detail}}`. Bytes are text when UTF-8, otherwise
`{base64: ...}`. Spooled streams are delivered in pages of at most 1 MiB,
without dropping output above the inline window. Output delivery follows command
completion; this mode does not promise live interleaving between stdout and stderr.
Progress is included without requiring `--progress`. Each frame is flushed.

Exit 0 means a result, even when the command exited 2 or 125; 125 means a
refusal. Usage remains `layerfs: ...` on stderr with exit 2. `--events` is
exclusive with `--stream` and `--text`. If stdout fails, delivery stops and
exit is 125; effects remain and there may be no final frame. A consumer must
require both a complete final frame and process completion. No operation is replayed.

## Streams

```text
library event                  --progress      --stream       neither
{progress: v}               -> stderr line      (dropped)     (dropped)
{stream:"stdout", bytes}    -> (dropped)        stdout        (dropped)
{stream:"stderr", bytes}    -> (dropped)        stderr        (dropped)
```

With `--progress`, if stderr is connected to an interactive terminal, the command prints one updating in-place status line displaying the phase, done/total progress, and transfer rate, clearing it upon completion. When stderr is not a terminal (e.g. piped to another command), it prints raw JSON lines byte-for-byte. If a script needs JSON lines on a terminal, setting the environment variable `LAYERFS_PROGRESS_JSON=1` forces JSON lines output on stderr even on a TTY.

With `--stream` the result JSON is not printed. The exit code is the
result's `exit_code` when it fits 0 to 255, 1 when it does not, 0 when the
result has none; 125 still means a refusal. Without `--stream`, exec
output stays in the result JSON and the exit is 0.

So with `--stream` a command that exits 2 or 125 has the same exit as a
wrong command line or a refusal. What tells them apart is stderr: a wrong
command line is one `layerfs: <message>` line and no output, a refusal is
one `{"code",...}` JSON line, and the command's own output is whatever it
wrote. A stdout whose reader has gone does not change the exit: the stream
sink ignores write errors and the command's code is returned.

`layerfs worker` is the machine's worker, outside the table
([`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs)). It reads
`LAYERFS_WORKER_LISTEN` (address), `LAYERFS_WORKER_SECRET` (a file holding
the secret) and, for tests, `LAYERFS_WORKER_ROOT` (default
`/layerfs/data`), then serves until ended; a missing setting exits 125.
Its protocol is on [Workers](../../control/routing/workers.md).

## Related

- [Front ends](README.md): the three front ends side by side.
- [Table](../call-table/table.md): the entries the parser and help read.
- [Errors](../call-table/errors.md#error-kinds): the codes a refusal carries.
- [Workers](../../control/routing/workers.md): what `layerfs worker` serves.
- [Doctor](../../control/management/doctor.md): what `layerfs doctor` reports.

---

← [API: Front ends](README.md) · [Up: Front ends](README.md) · [Front ends: Node](node.md) →

`store stats <name>` exposes [Store-wide counters](../sdk/store/stats.md) through the call table.

`project stats <store>/<project> [<branch>] [<commit>]` reports [pinned saved-tree counts](../sdk/project/stats.md).

`file revision` and pinned `branch diff` accept `--continuation TOKEN`. Repeat
the saved IDs and query options, omit `--page`, and read the returned next
page in a fresh process. Mismatched tokens are `InvalidInput`. `commit diff`
also accepts full saved Layer ids, retaining its path cursor and scalable plan.

After argument and platform validation, table calls initialize a missing home,
including reads such as `store list`. An empty/unset `LAYERFS_HOME` falls back
to `~/.layerfs`. Help, version and Doctor observation commands do not initialize
a missing home. A contract receipt records the final build's observed behavior.

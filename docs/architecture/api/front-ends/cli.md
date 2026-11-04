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
   +-- doctor [--text] ------------> layerfs_api::doctor   (outside table)
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

`parse` reads one call's words against its table entry
([`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs)):

| Word | Becomes | Rule |
| --- | --- | --- |
| a word not starting with `-` (or `-` itself) | the next positional, in table order | count must lie between the required and the total number of on-line positionals, else exit 2 |
| `--` | every later word is a positional | |
| `--<name> <value>` or `--<name>=<value>` | option `<name>` with `-` read as `_` (`--start-line` → `start_line`) | parsed by the option's `Kind`: `Int` a whole number, `Bool` `true`/`false`, `Json` JSON text, `Size` a number or text such as `8G`, the rest text |
| `--<bool-option>` alone | `true` | `--nested=false` gives false |
| `--<text-option>-from <file>` | option `<name>` read from the file | only for an optional `Text` option |
| `-m <text>` | the `message` option | exit 2 when the call has none |
| content or JSON positional (`file write` content, `file apply` changes) | read from stdin, or from `--from <file>` | not counted on the line; JSON must be UTF-8 JSON |
| a function option (`on_progress`, `stdout`, `stderr`) | never an option | replaced by `--progress` and `--stream` |
| any other `--x` | exit 2 | `unknown option` |

Content read from stdin or a file is text when it is UTF-8, else bytes.
Values are only shaped here; the library checks them again
([`check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs)),
so a value the shell accepts can still be refused `InvalidInput`.

| Switch | Calls | Effect |
| --- | --- | --- |
| `--text` | all | readable output instead of JSON |
| `--progress` | calls with `on_progress` | each `{progress}` event as one JSON line on stderr |
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
```

The usage line is built from the entry
([`help.rs`](../../../../api/crates/layerfs-cli/src/help.rs) `usage`):
required positionals as `<name>`, optional ones as `[<name>]`, each option as
`[--name <name>]` (a `Bool` as `[--name]`, `on_progress` as `[--progress]`,
`stdout`/`stderr` left out), and `[--from <file>] (content from stdin)`
when a positional is content or JSON. The one-line text is the entry's
`help`. An unknown topic is exit 2. The banner
([`banner.rs`](../../../../api/crates/layerfs-cli/src/banner.rs)) prints
only when stdout is a terminal; `NO_COLOR` turns its colours off.
`help` and `--version` need no home.

## Output

| Outcome | Printed | Exit | Source |
| --- | --- | --- | --- |
| success | the result as one line of JSON on stdout | 0 | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) `result` |
| success with `--text` | a result's `text` field as is; a list of records as an aligned table of their scalar fields (upper-case headers, nested values left out); else `key  value` lines | 0 | `readable`, `table` |
| refusal | `{"code","message","detail"}` on stderr | 125 | `refusal` |
| wrong command line | `layerfs: <message>` on stderr | 2 | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `wrong` |
| stdout closed while printing | nothing more | 125 | `print` |
| `doctor` | the report as JSON, or with `--text` a count line and one finding per entry with its `fix` | 0 whatever it finds | `findings` |

After every table call, before printing, the command runs
`layerfs_api::close_idle(Duration::ZERO)`: the Stores the call opened are
closed, so a SQLite Store leaves no write-ahead log behind
([`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)).

## Streams

```text
library event                  --progress      --stream       neither
{progress: v}               -> stderr line      (dropped)     (dropped)
{stream:"stdout", bytes}    -> (dropped)        stdout        (dropped)
{stream:"stderr", bytes}    -> (dropped)        stderr        (dropped)
```

With `--stream` the result JSON is not printed. The exit code is the
result's `exit_code` when it fits 0 to 255, 1 when it does not, 0 when the
result has none; 125 still means a refusal. Without `--stream`, exec
output stays in the result JSON and the exit is 0.

`layerfs worker` is the machine's worker, outside the table
([`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs)). It reads
`LAYERFS_WORKER_LISTEN` (address), `LAYERFS_WORKER_SECRET` (a file holding
the secret) and, for tests, `LAYERFS_WORKER_ROOT` (default
`/layerfs/data`), then serves until ended; a missing setting exits 125.
Its protocol is on [Workers](../routing/workers.md).

## Related

- [Front ends](README.md): the three front ends side by side.
- [Table](../call-table/table.md): the entries the parser and help read.
- [Errors](../call-table/errors.md#error-kinds): the codes a refusal carries.
- [Workers](../routing/workers.md): what `layerfs worker` serves.
- [Doctor](../control/doctor.md): what `layerfs doctor` reports.

---

← [API: Front ends](README.md) · [Up: Front ends](README.md) · [Front ends: Node](node.md) →

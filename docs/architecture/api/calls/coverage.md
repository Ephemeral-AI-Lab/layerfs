# Coverage

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source. The matrix is generated and checked against the front ends.

Which front end has which call, what lives outside the call table, and where the front ends differ on purpose, within [Calls](README.md).

| | |
| --- | --- |
| Source | [`api/scripts/coverage-doc.mjs`](../../../../api/scripts/coverage-doc.mjs), [`node/test/drift/coverage.test.mjs`](../../../../api/node/test/drift/coverage.test.mjs), [`node/test/contract/front-ends.test.mjs`](../../../../api/node/test/contract/front-ends.test.mjs), [`api/node/test/contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs) |

## The matrix

```text
 Node table ----+---> Node bound functions ---------+
 (the addon)    +---> Python TABLE, sync, aio ------+--> render: one row per
                +---> `layerfs help <family>` ------+    call, problems listed
                +---> MCP tools and profiles -------+
                                                    v
                         coverage-doc.mjs  (rewrite) | --check (exit 1)
```

One row per call of the Node package's `table`. A cell is `yes` when that
front end has the call and agrees with the Node table, else `NO` with the
reason listed under "Front-end problems":

| Column | Has the call when |
| --- | --- |
| Node | the package binds a function for it |
| Python | `layerfs.<family>.<call>` exists and its `layerfs.TABLE` entry equals the Node entry |
| Python `aio` | `layerfs.aio.<family>.<call>` is a coroutine function |
| CLI | `layerfs help <family>` prints its usage line with the table's arguments, without content and JSON positionals (read from stdin or `--from`), with `on_progress` as `--progress` and without the output functions |
| MCP | the generated tool's schema and description match the table and its profile lists it; `admin` marks a call only `--profile admin` lists |

`node api/scripts/coverage-doc.mjs` rewrites the block between the markers;
`--check` exits 1 when the block is not what the front ends give, or when a
front end disagrees with the Node table. Both need the built addon, the Python
module (`LAYERFS_PYTHON`, default `python3`, with `api/python/src` on its
path) and the built command (`LAYERFS_CLI`, default `target/debug/layerfs`,
or under `CARGO_TARGET_DIR`); a missing one is an error, never an empty
column. [`coverage.test.mjs`](../../../../api/node/test/drift/coverage.test.mjs) runs that
check and proves a missing, renamed or extra call is reported.

<!-- BEGIN GENERATED: api/scripts/coverage-doc.mjs -->

84 calls in 10 families, counted from the Node table. Calls present: Node 84, Python 84, Python `aio` 84, CLI 84, MCP 84 (17 in `work`, 67 admin only).

Argument names that are Python keywords: from.

Front-end problems: none.

| Call | Positionals (`?` optional) | Options (`()` a function) | Node | Python | Python `aio` | CLI | MCP |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `home.init` | - | - | yes | yes | yes | yes | admin |
| `home.get` | - | - | yes | yes | yes | yes | admin |
| `home.log` | - | call, project, client, tag, outcome, since, limit, cursor | yes | yes | yes | yes | admin |
| `home.clear_log` | - | - | yes | yes | yes | yes | admin |
| `home.doctor` | - | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `home.move` | to | - | yes | yes | yes | yes | admin |
| `project.init` | project, folder | timeout, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `project.list` | - | store, limit, cursor | yes | yes | yes | yes | admin |
| `project.get` | project | - | yes | yes | yes | yes | admin |
| `project.stats` | project | branch, at | yes | yes | yes | yes | admin |
| `project.doctor` | project | full, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `project.export` | project, destination | branch, at, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `project.delete` | project | - | yes | yes | yes | yes | admin |
| `project.hide_path` | project, path | - | yes | yes | yes | yes | admin |
| `branch.list` | project | prefix, exclude_prefix, limit, cursor | yes | yes | yes | yes | admin |
| `branch.get` | project, branch | - | yes | yes | yes | yes | admin |
| `branch.fork` | project, name, ?from | at | yes | yes | yes | yes | yes |
| `branch.merge` | project, branch, from | into, units, expected, message | yes | yes | yes | yes | admin |
| `branch.publish` | project, branch | at, expected, force | yes | yes | yes | yes | admin |
| `branch.remove` | project, branch | - | yes | yes | yes | yes | admin |
| `commit.list` | project, branch | at, limit, cursor | yes | yes | yes | yes | yes |
| `commit.get` | project, commit | branch | yes | yes | yes | yes | admin |
| `commit.diff` | project, ref | from, path, limit, cursor | yes | yes | yes | yes | yes |
| `file.read` | project, branch, path | at, line, limit | yes | yes | yes | yes | yes |
| `file.stat` | project, branch, path | at | yes | yes | yes | yes | admin |
| `file.list` | project, branch, path | at, limit, cursor | yes | yes | yes | yes | yes |
| `file.glob` | project, branch, pattern, ?path | exclude, at, limit, cursor | yes | yes | yes | yes | yes |
| `file.grep` | project, branch, pattern, ?path | include, exclude, ignore_case, at, limit, cursor | yes | yes | yes | yes | yes |
| `file.history` | project, branch, path | at, traversal, limit, cursor | yes | yes | yes | yes | admin |
| `file.blame` | project, branch, path | at, line, limit | yes | yes | yes | yes | admin |
| `file.write` | project, branch, path, content | mode, expected, message | yes | yes | yes | yes (stdin/--from) | yes |
| `file.edit` | project, branch, path | old_string, new_string, replace_all, expected, message | yes | yes | yes | yes | yes |
| `file.rm` | project, branch, path | recursive, expected, message | yes | yes | yes | yes | yes |
| `file.mkdir` | project, branch, path | mode, expected, message | yes | yes | yes | yes | admin |
| `file.mv` | project, branch, from, to | expected, message | yes | yes | yes | yes | yes |
| `file.cp` | project, branch, from, to | expected, message | yes | yes | yes | yes | admin |
| `file.apply` | project, branch, changes | expected, message | yes | yes | yes | yes (stdin/--from) | admin |
| `sandbox.create` | project, name | machine, image, mount, on_progress() | yes | yes | yes | yes (--progress) | yes |
| `sandbox.list` | - | project, machine, limit, cursor | yes | yes | yes | yes | admin |
| `sandbox.get` | sandbox | - | yes | yes | yes | yes | admin |
| `sandbox.remove` | sandbox | force, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `sandbox.files` | sandbox, ?path | depth, limit, cursor, byte_offset, byte_length | yes | yes | yes | yes | admin |
| `sandbox.log` | sandbox | limit, cursor | yes | yes | yes | yes | admin |
| `sandbox.doctor` | sandbox | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `workspace.list` | - | sandbox, limit, cursor | yes | yes | yes | yes | yes |
| `workspace.mount` | sandbox, branch | name | yes | yes | yes | yes | admin |
| `workspace.exec` | workspace, command | cwd, env, stdin, stream, stdout(), stderr(), on_progress() | yes | yes | yes | yes (--progress, --stream) | yes |
| `workspace.cancel` | workspace | exec | yes | yes | yes | yes | yes |
| `workspace.commit` | workspace | message, expected, force | yes | yes | yes | yes | yes |
| `workspace.unmount` | workspace | discard, force | yes | yes | yes | yes | admin |
| `workspace.status` | workspace | path | yes | yes | yes | yes | yes |
| `machine.list` | - | - | yes | yes | yes | yes | admin |
| `machine.get` | machine | - | yes | yes | yes | yes | admin |
| `machine.images` | machine | - | yes | yes | yes | yes | admin |
| `machine.load_image` | machine, image | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `machine.prune` | machine, ?image | unused | yes | yes | yes | yes | admin |
| `machine.stats` | machine | - | yes | yes | yes | yes | admin |
| `machine.files` | machine, ?path | depth, limit, cursor, byte_offset, byte_length | yes | yes | yes | yes | admin |
| `machine.log` | machine | limit, cursor | yes | yes | yes | yes | admin |
| `machine.start` | machine | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `machine.stop` | machine | force, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `machine.create` | name, platform | cpus, memory, disk, image, os_image, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `machine.remove` | machine | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `machine.update` | machine | cpus, memory, image, docker, wsl, daemon | yes | yes | yes | yes | admin |
| `machine.doctor` | machine | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.list` | - | machine | yes | yes | yes | yes | admin |
| `store.get` | store | - | yes | yes | yes | yes | admin |
| `store.create` | name | machine | yes | yes | yes | yes | admin |
| `store.connect` | store, url | user, password, synchronous_commit, dry_run | yes | yes | yes | yes | admin |
| `store.disconnect` | store | - | yes | yes | yes | yes | admin |
| `store.attach` | store | machine, dry_run | yes | yes | yes | yes | admin |
| `store.detach` | store | machine | yes | yes | yes | yes | admin |
| `store.move` | store, machine | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.remove` | store | - | yes | yes | yes | yes | admin |
| `store.compact` | store | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.upgrade` | store | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.repair_mount_holders` | project, branch, action | op, legacy, force, operator, limit, cursor | yes | yes | yes | yes | admin |
| `store.doctor` | store | machine, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `backup.connect` | name, url | access_key, secret_key | yes | yes | yes | yes | admin |
| `backup.disconnect` | bucket | - | yes | yes | yes | yes | admin |
| `backup.list` | - | bucket | yes | yes | yes | yes | admin |
| `backup.create` | store, bucket | file, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `backup.restore` | bucket, file, store | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `backup.doctor` | bucket | full | yes | yes | yes | yes | admin |

<!-- END GENERATED -->

## Outside the table

These are not calls and are not in the matrix.

| Helper | Node | Python | CLI |
| --- | --- | --- | --- |
| the table | `table`, `families` | `TABLE`, `FAMILIES` | `layerfs help [<family> [<call>]]`, `layerfs help --json` |
| a client with a LayerFS root and folder scope | `client({ root, scope })`, frozen | `client(root=, scope=)` | `--scope`, `--scope-project`, `--scope-branch`; the root is `LAYERFS_ROOT` only |
| the module-level default client | `layerfs.<family>` | `layerfs.<family>`, `layerfs.aio.<family>` | — |
| `close` | `close()` | `layerfs.close()` | none: the command closes its idle Stores after every call |
| the error | `LayerFSError { code, message, fix, detail }` and the next-call fields | `LayerFSError` (`code`, also `error`; `message`, `fix`, `fields`, `detail`) | JSON `{ error, message, fix, … }` on stderr (`detail` with `--detail`), exit 125 |
| `--text`, `--from`, `--NAME-from`, `-m`, `--password-stdin`, `--key-stdin` | — | — | readable output; content, JSON and text options from a file; `-m` is `--message`; a secret from stdin |
| `layerfs worker` | — | — | serve a machine's worker |

## Where the front ends differ on purpose

| Difference | Why |
| --- | --- |
| The CLI reads a content or JSON positional (`file.write` content, `file.apply` changes) from stdin or `--from`, and an optional text option `--x` also from `--x-from <file>`; UTF-8 stdin is text, anything else bytes | a shell word cannot carry binary data or a document |
| A wrong command line is exit 2 with plain text on stderr (unknown option, wrong positional count, a word not of its kind, unreadable JSON, a whole number beyond signed 64 bits); the packages refuse the same input `InvalidInput`. A library refusal is exit 125 with the packages' `{ error, message, fix, … }` | the command line is parsed against the table before the library is called |
| The CLI has no `null` and no typed values: an omitted argument is omitted, a word is read by its kind, and a positional cannot be left out before one that is given | words are text |
| Whole numbers: Node takes a safe-integer `number` or a `bigint`; Python any `int`; the CLI a signed 64-bit decimal. The library reads signed 64 bits; the engine accepts line numbers, limits and modes only as safe integers | JavaScript numbers are doubles |
| Python refuses a float, `2.0` included ("only whole numbers are accepted"); Node and the CLI accept `2.0` as `2` | the types differ |
| Function options exist in Node and Python only, never cross into the library, and are `--progress` and `--stream` on the command line | a callback cannot cross a process |
| Python takes optional positionals and options as keywords, spelling `with` and `from` as `with_` and `from_`; Node takes a trailing options object | idiom; `with` and `from` are Python keywords |
| Python `aio` runs the synchronous call on a thread; cancelling the coroutine does not stop the call | the native call cannot be cancelled |
| `layerfs home doctor` can warn of a stale lease of an ended process that Node and Python do not see | the command is one process per call; the in-process front ends keep their lease alive |
| A refusal a front end makes itself is worded by that front end; its `error` code is the same | each front end builds its own JSON text |

## What is checked across the front ends

[`front-ends.test.mjs`](../../../../api/node/test/contract/front-ends.test.mjs) runs the case list
in [`contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs)
through Node, Python, Python `aio` and the command, each in its own fresh
temporary home (a SQLite Store `shop` on the host machine, project `shop/api`, Branch
`agent`). Answers are compared step by step after ids, times, inode numbers,
stored sizes and home paths are normalized, and every case also asserts its
expected value; a refusal is compared by `code`, key set and, unless the
wording is the front end's own, `message` and `detail`.

| Group | What it shows |
| --- | --- |
| `setup` | `store.create` (profile `normal`), `project.init`, `branch.fork` |
| `args` | a missing positional; defaults of `file.read`, `branch.list` and the Commit message; omitted equals `null`; a line window, a `limit` and `cursor` page, `mode` |
| `unknown` | an unknown option, an extra positional and a non-function handler are refused before any effect (state listed before and after is equal) |
| `validation` | `InvalidInput` (a bad name, path or kind, a write to `main`, a bad cursor), `NotFound` (no project, Branch or path; the raised `NoSuchPath` kept as `detail.code`), `Exists` (a fork onto a taken name); `file.apply` stops at the first refused change with nothing committed (`detail.index`); JSON nested deeper than 128 |
| `values` | all 256 byte values round-trip as `Binary`; Unicode in content and paths; empty content and message; nested JSON with integers beyond 2^53 kept exactly; `{ "base64" }` as bytes; a lone surrogate refused before the call |
| `ints` | `file.read`'s `line` 2^53-1 exact; 2^53 up to 2^63-1 refused by the engine as not a safe integer; 2^63 refused before it (`number out of range`; the command exits 2) |
| `flow` | fork, write, merge, reads of the merged Branch, `commit.diff`, `commit.list`, `branch.get`, `project.get`, `project.list`, `store.list`, `home.doctor`: equal answers |
| `export` | `project.export` of the default and an explicit Branch; a non-empty destination, an unknown option and an absent Branch refused |

Both tests skip with the reason when a front end is absent; a skipped front
end is not counted as agreeing.

| Behaviour all front ends share | Detail |
| --- | --- |
| `file.apply` with `[]` | commits nothing and answers `{ unchanged: true }` |
| a refused removal inside `file.apply` | `NotFound` with `detail.index`, the change it stopped at |
| engine argument refusals | say the value must be a safe integer (a `line` past 2^53-1); result keys are snake_case throughout |
| a one-key `{ "base64": <text> }` map | is bytes when the text is base64; inside `changes` a non-base64 one is refused |

## Build and run

Linux or macOS host, from the repository root; temporary homes need a
filesystem with SQLite locks (`TMPDIR` on ext4, xfs or btrfs). Each test
command runs under a wall limit of at most 120 s.

```text
cargo +1.85.1 build --locked --offline -p layerfs-cli -p layerfs-node
node api/scripts/native-addon.mjs --debug          # api/node/layerfs.node
(cd api/python && maturin develop --locked --offline)
export LAYERFS_PYTHON=<python with layerfs importable>   # default python3
node api/scripts/coverage-doc.mjs                  # rewrite the matrix
node api/scripts/coverage-doc.mjs --check          # fail when it is stale
cd api/node
perl -e 'alarm 120; exec @ARGV' node --test test/drift/coverage.test.mjs test/contract/front-ends.test.mjs
```

`LAYERFS_CONTRACT_REPORT=<file>` writes each case's normalized answer per
front end.

## Related

- [Calls](README.md): the table the matrix is generated from.
- [Front ends](../front-ends/README.md), [bindings](../front-ends/bindings.md), [MCP](../front-ends/mcp.md).

# Coverage

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source. The matrix is generated and checked against the front ends.

Which front end has which call, what lives outside the call table, and where the front ends differ on purpose, within [Calls](README.md).

| | |
| --- | --- |
| Source | [`api/scripts/gen-coverage.mjs`](../../../../api/scripts/gen-coverage.mjs), [`api/node/test/coverage.mjs`](../../../../api/node/test/coverage.mjs), [`api/node/test/contract.mjs`](../../../../api/node/test/contract.mjs), [`api/node/test/contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs) |

## The matrix

```text
 Node table ----+---> Node bound functions ---------+
 (the addon)    +---> Python TABLE, sync, aio ------+--> render: one row per
                +---> `layerfs help <family>` ------+    call, problems listed
                +---> MCP tools and profiles -------+
                                                    v
                         gen-coverage.mjs  (rewrite) | --check (exit 1)
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

`node api/scripts/gen-coverage.mjs` rewrites the block between the markers;
`--check` exits 1 when the block is not what the front ends give, or when a
front end disagrees with the Node table. Both need the built addon, the Python
module (`LAYERFS_PYTHON`, default `python3`, with `api/python/src` on its
path) and the built command (`LAYERFS_CLI`, default `target/debug/layerfs`,
or under `CARGO_TARGET_DIR`); a missing one is an error, never an empty
column. [`coverage.mjs`](../../../../api/node/test/coverage.mjs) runs that
check and proves a missing, renamed or extra call is reported.

<!-- BEGIN GENERATED: api/scripts/gen-coverage.mjs -->

61 calls in 9 families, counted from the Node table. Calls present: Node 61, Python 61, Python `aio` 61, CLI 61, MCP 61 (34 in `work`, 27 admin only).

Argument names that are Python keywords: none.

Front-end problems: none.

| Call | Positionals (`?` optional) | Options (`()` a function) | Node | Python | Python `aio` | CLI | MCP |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `project.init` | project, folder | timeout, on_progress() | yes | yes | yes | yes (--progress) | admin |
| `project.list` | ?store | page_size, page_pos | yes | yes | yes | yes | yes |
| `project.get` | project | - | yes | yes | yes | yes | yes |
| `project.export` | project, destination | branch, on_progress() | yes | yes | yes | yes (--progress) | yes |
| `project.remove` | project | - | yes | yes | yes | yes | admin |
| `branch.list` | project | page_size, page_pos | yes | yes | yes | yes | yes |
| `branch.get` | project, branch | - | yes | yes | yes | yes | yes |
| `branch.fork` | project, name, from_branch, ?from_commit | - | yes | yes | yes | yes | yes |
| `branch.merge` | project, name, a, b | message, units | yes | yes | yes | yes | yes |
| `branch.publish` | project, branch, ?commit | - | yes | yes | yes | yes | yes |
| `branch.diff` | project, from_branch, to_branch | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes | yes |
| `commit.list` | project, branch | page_size, page_pos | yes | yes | yes | yes | yes |
| `commit.get` | project, branch, commit | - | yes | yes | yes | yes | yes |
| `commit.diff` | project, from_commit, to_commit | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes | yes |
| `file.read` | project, branch, path, ?commit | start_line, end_line | yes | yes | yes | yes | yes |
| `file.stat` | project, branch, path, ?commit | - | yes | yes | yes | yes | yes |
| `file.list` | project, branch, path, ?commit | page_size, page_pos | yes | yes | yes | yes | yes |
| `file.glob` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes | yes |
| `file.grep` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes | yes |
| `file.blame` | project, branch, path, ?commit | from_line | yes | yes | yes | yes | yes |
| `file.diff` | project, left_branch, left_path, right_branch, right_path | left_commit, right_commit | yes | yes | yes | yes | yes |
| `file.write` | project, branch, path, content | message, mode | yes | yes | yes | yes (stdin/--from) | yes |
| `file.edit` | project, branch, path | old, new, message | yes | yes | yes | yes | yes |
| `file.rm` | project, branch, path | message | yes | yes | yes | yes | yes |
| `file.mkdir` | project, branch, path | message, mode | yes | yes | yes | yes | yes |
| `file.mv` | project, branch, from_path, to_path | message | yes | yes | yes | yes | yes |
| `file.cp` | project, branch, from_path, to_path | message | yes | yes | yes | yes | yes |
| `file.apply` | project, branch, changes | message | yes | yes | yes | yes (stdin/--from) | yes |
| `sandbox.create` | project, name, ?machine | image | yes | yes | yes | yes | admin |
| `sandbox.list` | ?project | page_size, page_pos | yes | yes | yes | yes | yes |
| `sandbox.get` | name | page_size, page_pos | yes | yes | yes | yes | yes |
| `sandbox.close` | name | force | yes | yes | yes | yes | admin |
| `workspace.mount` | sandbox, branch | name | yes | yes | yes | yes | yes |
| `workspace.exec` | ws, command | message, stream, stdout(), stderr(), exec_id, on_progress() | yes | yes | yes | yes (--progress, --stream) | yes |
| `workspace.exec_cancel` | ws, exec_id | grace_ms | yes | yes | yes | yes | yes |
| `workspace.commit` | ws | message, force | yes | yes | yes | yes | yes |
| `workspace.unmount` | ws | discard, force | yes | yes | yes | yes | yes |
| `workspace.status` | ws | - | yes | yes | yes | yes | yes |
| `machine.list` | - | - | yes | yes | yes | yes | admin |
| `machine.platforms` | - | - | yes | yes | yes | yes | admin |
| `machine.get` | name | - | yes | yes | yes | yes | admin |
| `machine.status` | name | - | yes | yes | yes | yes | admin |
| `machine.start` | name | - | yes | yes | yes | yes | admin |
| `machine.stop` | name | - | yes | yes | yes | yes | admin |
| `machine.create` | name, platform | cpus, memory, disk, image | yes | yes | yes | yes | admin |
| `machine.remove` | name | - | yes | yes | yes | yes | admin |
| `machine.update` | name | cpus, memory, image, docker, wsl, daemon | yes | yes | yes | yes | admin |
| `store.list` | - | - | yes | yes | yes | yes | admin |
| `store.get` | name | - | yes | yes | yes | yes | admin |
| `store.status` | name | - | yes | yes | yes | yes | admin |
| `store.create` | name | - | yes | yes | yes | yes | admin |
| `store.connect` | name, url | password, synchronous_commit | yes | yes | yes | yes | admin |
| `store.attach` | name, machine | url, password | yes | yes | yes | yes | admin |
| `store.detach` | name, machine | - | yes | yes | yes | yes | admin |
| `store.push` | name, machine | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.pull` | name | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `store.remove` | name | - | yes | yes | yes | yes | admin |
| `store.disconnect` | name | - | yes | yes | yes | yes | admin |
| `backup.connect` | name, url | key, region, part_size | yes | yes | yes | yes | admin |
| `backup.disconnect` | name | - | yes | yes | yes | yes | admin |
| `backup.list` | ?bucket | - | yes | yes | yes | yes | admin |
| `backup.create` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) | admin |
| `backup.restore` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) | admin |

<!-- END GENERATED -->

## Outside the table

These are not calls and are not in the matrix.

| Helper | Node | Python | CLI |
| --- | --- | --- | --- |
| the table | `table`, `families` | `TABLE`, `FAMILIES` | `layerfs help [<family> [<call>]]`, `layerfs help --json` |
| a client with home and folder scope | `client({ home, scope })`, frozen | `client(home=, scope=)` | `--scope`, `--scope-project`, `--scope-branch`; the home is `LAYERFS_HOME` only |
| the module-level default client | `layerfs.<family>` | `layerfs.<family>`, `layerfs.aio.<family>` | — |
| `doctor` | `doctor({ home, ...options })` | `layerfs.doctor(home, ...)` | `layerfs doctor [--text]` ([Doctor](../../control/doctor/README.md)) |
| `close` | `close()` | `layerfs.close()` | none: the command closes its idle Stores after every call |
| the error | `LayerFSError { code, message, detail }` | `LayerFSError` with the same three | JSON `{ code, message, detail }` on stderr, exit 125 |
| `--text`, `--from`, `--NAME-from`, `-m` | — | — | readable output; content, JSON and text options from a file; `-m` is `--message` |
| `layerfs worker`, `layerfs import-env` | — | — | serve a machine's worker; write 1.1.0 settings as records, once |

## Where the front ends differ on purpose

| Difference | Why |
| --- | --- |
| The CLI reads a content or JSON positional (`file.write` content, `file.apply` changes) from stdin or `--from`, and an optional text option `--x` also from `--x-from <file>`; UTF-8 stdin is text, anything else bytes | a shell word cannot carry binary data or a document |
| A wrong command line is exit 2 with plain text on stderr (unknown option, wrong positional count, a word not of its kind, unreadable JSON, a whole number beyond signed 64 bits); the packages refuse the same input `InvalidInput`. A library refusal is exit 125 with the packages' `{ code, message, detail }` | the command line is parsed against the table before the library is called |
| The CLI has no `null` and no typed values: an omitted argument is omitted, a word is read by its kind, and a positional cannot be left out before one that is given | words are text |
| Whole numbers: Node takes a safe-integer `number` or a `bigint`; Python any `int`; the CLI a signed 64-bit decimal. The library reads signed 64 bits; the engine accepts line numbers, page sizes and modes only as safe integers | JavaScript numbers are doubles |
| Python refuses a float, `2.0` included ("only whole numbers are accepted"); Node and the CLI accept `2.0` as `2` | the types differ |
| Function options exist in Node and Python only, never cross into the library, and are `--progress` and `--stream` on the command line | a callback cannot cross a process |
| Python takes positionals and options as keywords; Node takes a trailing options object. No argument name is a Python keyword (generated above) | idiom |
| Python `aio` runs the synchronous call on a thread; cancelling the coroutine does not stop the call | the native call cannot be cancelled |
| `layerfs doctor` can warn of a stale lease of an ended process that Node and Python do not see | the command is one process per call; the in-process front ends keep their lease alive |
| A refusal a front end makes itself is worded by that front end; its `code` is the same | each front end builds its own JSON text |

## What is checked across the front ends

[`contract.mjs`](../../../../api/node/test/contract.mjs) runs the case list
in [`contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs)
through Node, Python, Python `aio` and the command, each in its own fresh
Disposable home (a native SQLite Store `shop`, project `shop/api`, Branch
`agent`). Answers are compared step by step after ids, times, inode numbers,
stored sizes and home paths are normalized, and every case also asserts its
expected value; a refusal is compared by `code`, key set and, unless the
wording is the front end's own, `message` and `detail`.

| Group | What it shows |
| --- | --- |
| `setup` | `store.create` (profile `disposable`), `project.init`, `branch.fork` |
| `args` | a missing positional; defaults of `file.read`, `branch.list` and the Commit message; omitted equals `null`; a window, a page, `mode` |
| `unknown` | an unknown option, an extra positional and a non-function handler are refused before any effect (state listed before and after is equal) |
| `validation` | `InvalidInput`, `NoSuchStore`, `NoSuchBranch`, `NoSuchPath`, `ReadOnly`, `NameTaken` with their detail; `file.apply` stops at the first refused change with nothing committed (`detail.index`); JSON nested deeper than 128 |
| `values` | all 256 byte values round-trip as `Binary`; Unicode in content and paths; empty content and message; nested JSON with integers beyond 2^53 kept exactly; `{ "base64" }` as bytes; a lone surrogate refused before the call |
| `ints` | `start_line` 2^53-1 exact; 2^53 up to 2^63-1 refused by the engine as not a safe integer; 2^63 refused before it (`number out of range`; the command exits 2) |
| `flow` | fork, write, merge, reads of the merged Branch, `branch.diff`, `commit.list`, `branch.get`, `project.get`, `project.list`, `store.list`, `doctor`: equal answers |

Both tests skip with the reason when a front end is absent; a skipped front
end is not counted as agreeing.

| Behaviour all front ends share | Detail |
| --- | --- |
| `file.apply` with `[]` | commits nothing and answers `UpToDate` (`head` null on a Branch with no Commit) |
| a refused removal | `NoSuchPath` with `detail.index`; the message is the engine error's text (`Content(PathNotFound)`) |
| engine argument refusals | name the Store method's camelCase field (`startLine must be a positive safe integer`, `pageSize must be in 1..=1000`); result keys are snake_case throughout |
| a one-key `{ "base64": <text> }` map | is bytes when the text is base64; inside `changes` a non-base64 one is refused |

## Build and run

Linux or macOS host, from the repository root; temporary homes need a
filesystem with SQLite locks (`TMPDIR` on ext4, xfs or btrfs). Each test
command runs under a wall limit of at most 120 s.

```text
cargo +1.85.1 build --locked --offline -p layerfs-cli -p layerfs-node
node api/scripts/build-native.mjs --debug          # api/node/layerfs.node
(cd api/python && maturin develop --locked --offline)
export LAYERFS_PYTHON=<python with layerfs importable>   # default python3
node api/scripts/gen-coverage.mjs                  # rewrite the matrix
node api/scripts/gen-coverage.mjs --check          # fail when it is stale
cd api/node
perl -e 'alarm 120; exec @ARGV' node --test test/coverage.mjs test/contract.mjs
```

`LAYERFS_CONTRACT_REPORT=<file>` writes each case's normalized answer per
front end.

## Related

- [Calls](README.md): the table the matrix is generated from.
- [Front ends](../front-ends/README.md), [bindings](../front-ends/bindings.md), [MCP](../front-ends/mcp.md).

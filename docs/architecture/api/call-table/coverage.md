# Coverage

[Architecture](../../README.md) › [API](../README.md) › [Call table](README.md) › Coverage

> **Status:** Current architecture guide for LayerFS 2.0.0; the matrix is generated and checked against the front ends.

Which front end has which call, what lives outside the table, where the front ends differ on purpose, what is checked across them and what is not.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Source | [`api/scripts/gen-coverage.mjs`](../../../../api/scripts/gen-coverage.mjs), [`api/node/test/contract.mjs`](../../../../api/node/test/contract.mjs), [`api/node/test/contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs) |
| Uses | [Table](table.md), [Errors](errors.md) |
| Used by | [Front ends](../front-ends/README.md) |

## The matrix

One row per call of the table, counted by the generator from the Node package's `table`. A cell says
`yes` when that front end has the call and it agrees with the Node table:

- **Node** is the package's bound function.
- **Python** is `layerfs.<family>.<call>` and its entry in `layerfs.TABLE` equals the Node entry.
- **Python `aio`** is `layerfs.aio.<family>.<call>` as a coroutine function.
- **CLI** is the usage line `layerfs help <family>` prints. It must be the table's arguments without
  the content and JSON positionals (read from stdin or `--from`), with `on_progress` as `--progress`,
  and without the output functions (`--stream` is a plain option of `workspace.exec`).

A cell that is not `yes` is `NO`, and the reason is listed under "Front-end problems". The matrix is
rewritten by `node api/scripts/gen-coverage.mjs` and `--check` fails when it is stale;
`api/node/test/coverage.mjs` runs that check and also proves that a missing, renamed or extra call is
reported.

<!-- BEGIN GENERATED: api/scripts/gen-coverage.mjs -->

61 calls in 9 families, counted from the Node table. Calls present: Node 61, Python 61, Python `aio` 61, CLI 61.

Argument names that are Python keywords: none.

Front-end problems: none.

| Call | Positionals (`?` optional) | Options (`()` a function) | Node | Python | Python `aio` | CLI |
| --- | --- | --- | --- | --- | --- | --- |
| `project.init` | project, folder | timeout, on_progress() | yes | yes | yes | yes (--progress) |
| `project.list` | ?store | page_size, page_pos | yes | yes | yes | yes |
| `project.get` | project | - | yes | yes | yes | yes |
| `project.export` | project, destination | branch, on_progress() | yes | yes | yes | yes (--progress) |
| `project.remove` | project | - | yes | yes | yes | yes |
| `branch.list` | project | page_size, page_pos | yes | yes | yes | yes |
| `branch.get` | project, branch | - | yes | yes | yes | yes |
| `branch.fork` | project, name, from_branch, ?from_commit | - | yes | yes | yes | yes |
| `branch.merge` | project, name, a, b | message, units | yes | yes | yes | yes |
| `branch.publish` | project, branch, ?commit | - | yes | yes | yes | yes |
| `branch.diff` | project, from_branch, to_branch | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes |
| `commit.list` | project, branch | page_size, page_pos | yes | yes | yes | yes |
| `commit.get` | project, branch, commit | - | yes | yes | yes | yes |
| `commit.diff` | project, from_commit, to_commit | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes |
| `file.read` | project, branch, path, ?commit | start_line, end_line | yes | yes | yes | yes |
| `file.stat` | project, branch, path, ?commit | - | yes | yes | yes | yes |
| `file.list` | project, branch, path, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.glob` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.grep` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.blame` | project, branch, path, ?commit | from_line | yes | yes | yes | yes |
| `file.diff` | project, left_branch, left_path, right_branch, right_path | left_commit, right_commit | yes | yes | yes | yes |
| `file.write` | project, branch, path, content | message, mode | yes | yes | yes | yes (stdin/--from) |
| `file.edit` | project, branch, path | old, new, message | yes | yes | yes | yes |
| `file.rm` | project, branch, path | message | yes | yes | yes | yes |
| `file.mkdir` | project, branch, path | message, mode | yes | yes | yes | yes |
| `file.mv` | project, branch, from_path, to_path | message | yes | yes | yes | yes |
| `file.cp` | project, branch, from_path, to_path | message | yes | yes | yes | yes |
| `file.apply` | project, branch, changes | message | yes | yes | yes | yes (stdin/--from) |
| `sandbox.create` | project, name, ?machine | image | yes | yes | yes | yes |
| `sandbox.list` | ?project | page_size, page_pos | yes | yes | yes | yes |
| `sandbox.get` | name | page_size, page_pos | yes | yes | yes | yes |
| `sandbox.close` | name | force | yes | yes | yes | yes |
| `workspace.mount` | sandbox, branch | name | yes | yes | yes | yes |
| `workspace.exec` | ws, command | message, stream, stdout(), stderr(), exec_id, on_progress() | yes | yes | yes | yes (--progress, --stream) |
| `workspace.exec_cancel` | ws, exec_id | grace_ms | yes | yes | yes | yes |
| `workspace.commit` | ws | message, force | yes | yes | yes | yes |
| `workspace.unmount` | ws | discard, force | yes | yes | yes | yes |
| `workspace.status` | ws | - | yes | yes | yes | yes |
| `machine.list` | - | - | yes | yes | yes | yes |
| `machine.platforms` | - | - | yes | yes | yes | yes |
| `machine.get` | name | - | yes | yes | yes | yes |
| `machine.status` | name | - | yes | yes | yes | yes |
| `machine.start` | name | - | yes | yes | yes | yes |
| `machine.stop` | name | - | yes | yes | yes | yes |
| `machine.create` | name, platform | cpus, memory, disk, image | yes | yes | yes | yes |
| `machine.remove` | name | - | yes | yes | yes | yes |
| `machine.update` | name | cpus, memory, image, docker, wsl, daemon | yes | yes | yes | yes |
| `store.list` | - | - | yes | yes | yes | yes |
| `store.get` | name | - | yes | yes | yes | yes |
| `store.status` | name | - | yes | yes | yes | yes |
| `store.create` | name | - | yes | yes | yes | yes |
| `store.connect` | name, url | password, synchronous_commit | yes | yes | yes | yes |
| `store.push` | name, machine | on_progress() | yes | yes | yes | yes (--progress) |
| `store.pull` | name | on_progress() | yes | yes | yes | yes (--progress) |
| `store.remove` | name | - | yes | yes | yes | yes |
| `store.disconnect` | name | - | yes | yes | yes | yes |
| `backup.connect` | name, url | key, region, part_size | yes | yes | yes | yes |
| `backup.disconnect` | name | - | yes | yes | yes | yes |
| `backup.list` | ?bucket | - | yes | yes | yes | yes |
| `backup.create` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) |
| `backup.restore` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) |

<!-- END GENERATED -->

## Outside the table

These are not calls and are not in the matrix. Python means the package `layerfs` (and `layerfs.aio`
for coroutines).

| Helper | Node | Python | CLI | Checked |
| --- | --- | --- | --- | --- |
| `table`, `families` / `TABLE`, `FAMILIES` | yes | yes | `layerfs help [<family> [<call>]]` | the matrix |
| Client with home and folder scope | `client({ home, scope })`, frozen | `client(home=, scope=)` | `--scope`, `--scope-project`, `--scope-branch`; the home is `LAYERFS_HOME` only | scope: [#33](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/33), not here |
| Module-level default client | `layerfs.<family>` | `layerfs.<family>`, `layerfs.aio.<family>` | n/a | no |
| `doctor` | `doctor({ home })` | `layerfs.doctor(home)` | `layerfs doctor [--text]` | one report on a clean home, equal in all front ends (case `doctor`); reliability is the control plane's ([#23](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/23)) |
| `close` | `close()` | `layerfs.close()` | none: the command closes its idle Stores after every call | `api/node/test/close.mjs`, `api/python/test/test_doctor.py`; not in the case list |
| Error type | `LayerFSError{code,message,detail}` | `LayerFSError` with the same three | JSON `{code,message,detail}` on stderr, exit 125 | every refusal of the case list |
| `--text` | n/a | n/a | readable result instead of JSON | `contract: the command's --text ...` |
| `--from`, `--NAME-from` | n/a | n/a | content, JSON and optional text options from a file | `contract: the command reads content from --from ...` |
| `-m` | n/a | n/a | short for `--message` | no |
| `--progress`, `--stream` | `on_progress`, `stdout`, `stderr` functions | the same as keywords | stderr JSON lines; the command's output and exit code | **NOT RUN**: events and exec output are [#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37)'s |
| `layerfs worker` | n/a | n/a | serves a machine's worker | **NOT RUN**: needs a machine |
| `layerfs import-env` | n/a | n/a | writes 1.1.0 settings as records, once | **NOT RUN** |
| `layerfs help`, `--version` | n/a | n/a | yes | the matrix reads `help` |

**MCP** is shelved (O32: dropped from 2.0): no column, no row, no work here.

## Where the front ends differ on purpose

Each is checked by a case of the list, named in the last column, and is not a gap.

| Difference | Why | Case |
| --- | --- | --- |
| `layerfs doctor` can report a warning `Store x holds a stale lease of process N, which ended` that Node and Python do not | the command is one process per call, so an earlier call leaves the lease of a process that has ended (the next call on that Store removes it); the in-process front ends keep their own lease alive. `ok` is still `true` | the `doctor` case of the `flow` group sets exactly that warning aside and compares every other finding |
| The CLI reads a content or JSON positional (`file.write` content, `file.apply` changes) from stdin or `--from`, and an optional text option `--x` also from `--x-from <file>`. Stdin that is UTF-8 is text, anything else is bytes. | a shell word cannot carry binary data or a document | `write bytes`, `apply nested`, `--from` test |
| A wrong command line is **exit 2** and plain text on stderr, no JSON: unknown option, wrong number of positionals, a word that is not of its kind (`--force=yes`), JSON that cannot be read, a whole number beyond a signed 64-bit one. The packages refuse the same input with `InvalidInput`. A call the library refuses is **exit 125** with the same `{code,message,detail}` as the packages. | the command line is parsed against the table before the library is called; help says "Exit 0: done; 2: wrong command line; 125: refused" | `write unknown option`, `missing path`, `wrong kind text for bool`, `line i64 max + 1`, `deep JSON` |
| The CLI has no `null` and no typed values: an omitted positional or option is omitted, a word is read by the argument's kind (`--page-size=10` is a whole number). A positional cannot be left out before one that is given. | words are text | `read null commit`, `wrong kind text for int` (packages only) |
| Whole numbers: Node takes a `number` that is a safe integer or a `bigint`; Python takes any `int`; the CLI takes a signed 64-bit decimal. The library reads whole numbers as signed 64-bit, and the engine accepts line numbers, page sizes and modes only as safe integers. | JavaScript numbers are doubles | `ints` group below |
| Python refuses a float, `2.0` included (`only whole numbers are accepted`); JavaScript has no `2.0` apart from `2`, so Node and the CLI accept it. | the types differ | `whole float` |
| Function options (`on_progress`, `stdout`, `stderr`) exist in Node and Python, never cross into the library, and are `--progress` and `--stream` on the command line. | a callback cannot cross a process | [Table](table.md) |
| Python takes the table's positionals and options as keywords too; Node takes a trailing options object. No argument name of the table is a Python keyword (generated above). | idiom | matrix |
| Python `aio` runs the synchronous call on a thread; cancelling the coroutine does not stop the call. | the native call cannot be cancelled | `api/python/src/layerfs/aio.py` |
| The wording of a refusal made by a front end itself differs: Node `options must be an object` where Python says `takes 4 positional arguments`; a number's position (`at byte N`) is a position in the JSON text that front end built. The `code` is the same. | each front end builds its own JSON text | `stat extra positional`, `whole numbers only` |

## What is checked across the front ends

`api/node/test/contract.mjs` runs the case list in
[`contract/cases.mjs`](../../../../api/node/test/contract/cases.mjs) through Node, Python (sync), Python
`aio` and the command. Each front end gets its own fresh Disposable home (a native SQLite Store `shop`
with project `shop/api` made from a folder holding one README, and a Branch `agent`), so the cases are
the same real operations in the same order, and the answers are compared step by step after ids, times,
inode numbers, stored sizes and home paths are normalized. Every case asserts what it must be, not only
that the front ends agree. A refusal is compared by `code`, by the key set `{code, message, detail}` and,
unless the wording is the front end's own, by `message` and `detail`.

| Group | Cases | What it shows |
| --- | --- | --- |
| `setup` | 3 | `store.create` (profile `disposable`), `project.init`, `branch.fork` |
| `args` | 14 | a missing required positional; defaults of `file.read`, `branch.list` and the Commit message; omitted equals `null` (positional and options); a window, a page, `mode` |
| `unknown` | 17 | an unknown option, an extra positional and a function option that is not a function are refused **before any effect**: Commits, Branches, Stores and projects are listed before and after and are equal, and the file, Branch and Store were never made |
| `validation` | 21 | `InvalidInput`, `NoSuchStore`, `NoSuchBranch`, `NoSuchPath`, `ReadOnly`, `NameTaken` with their detail; a value of the wrong kind; `file.apply` refuses a non-list, an unknown op, and stops at the first refused change with nothing committed (`detail.index`); JSON nested deeper than 128 |
| `values` | 30 | all 256 byte values written and read back as `Binary`; UTF-8 bytes read back as text; Unicode (astral, combining, NFC against NFD sizes 2 and 3, `U+2028`, CRLF) in content and in a path; empty content, empty message; nested JSON text with `null`, empty strings and arrays, and an integer beyond 2^53 kept exactly as text; `{"base64"}` as bytes (valid) and as a refusal (not base64); a lone surrogate refused before the call |
| `ints` | 14 | `start_line` 3 and 2^53-1 exact; 2^53, 2^62+1 and 2^63-1 refused by the engine as `startLine must be a positive safe integer` (2^63-1 reaching it unchanged shows no conversion through a double); 2^63 refused (`number out of range`, the command: exit 2); a `number` beyond a safe integer refused by Node; `page_size` and `mode` the same; nothing written by the refused ones |
| `flow` | 11 | `branch.fork`, `file.write`, `branch.merge`, reads of the merged Branch, `branch.diff`, `commit.list`, `branch.get`, `project.get`, `project.list`, `store.list`, `doctor`: equal answers |

110 steps. Outcomes: Node 58 values and 52 refusals; Python and `aio` 57 and 53 (`whole float`); the
command 56 values, 30 refusals, 12 wrong command lines and 12 steps it cannot say (`null`, a wrong kind,
a lone surrogate), skipped for it and run in the others. Result names: every key of every value Node
returned matches `^[a-z][a-z0-9_]*$`.

Run (after building, see below): `node --test test/contract.mjs test/coverage.mjs` in `api/node`; both
skip with the reason when a front end is absent, and a skipped front end is not counted as agreeing.

### NOT RUN

| What | Why |
| --- | --- |
| `Unknown` and `Unresolved` outcomes without replay | no real operation without a daemon, a fault or a second process produces one here; their shape and the "read before writing again" rule are in [Errors](errors.md) |
| `sandbox.*`, `workspace.*`, `machine.*`, `store.connect/push/pull/disconnect`, `backup.*` | need a daemon, a mount, a machine, a bucket or another machine; the matrix checks only that they exist; that the command and the packages refuse every call alike for an unknown project or name is `api/node/test/shell.mjs` and `api/python/test/test_calls.py` |
| a result whole number beyond 2^53 | no call on a temp Store returns one; the decode side is covered by `api/node/test/values.mjs` and `api/python/test/test_values.py` |
| binary or non-UTF-8 `workspace.exec` output, `bytes` events | needs a Workspace; events are [#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37)'s |
| folder scope across the front ends | [#33](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/33) |
| PostgreSQL, TLS (`postgresql`, `tls` features are off by default in all three), other platforms | not built here; Linux x86_64 only |

## Known gaps

None of these is in front-end code, so none is fixed here. They are in the engine and the control
library, which this work does not change. Commands ran against the debug command built from this tree,
in a temporary Disposable home; the log is `logs/contract-gaps.log` of the work directory.

1. **`file.apply` with `[]` commits nothing and answers `UpToDate`; the page says a nonempty list.**
   `echo '[]' | layerfs file apply shop/api agent` prints
   `{"kind":"UpToDate","branch":"agent","head":null,"root":"67aa…"}`, exit 0, in every front end (case
   `apply empty list`). [file.apply](../sdk/file/apply.md) lists `changes` as "a nonempty list". Either
   the page or the call is right; the owner picks. `head` is `null` on a Branch with no Commit.
2. **A refused removal reads `Content(PathNotFound)`.** `layerfs file rm shop/api agent /nope` and a
   `file.apply` whose second change is `{"op":"rm","path":"/nope"}` both print
   `{"code":"NoSuchPath","message":"Content(PathNotFound)","detail":{"index":N}}`, exit 125. The code and
   `detail.index` are right; the message is the engine error's `Display`
   (`engine/crates/layerfs-store-service/src/write/mutation.rs:64-70`, `e.to_string()`), where
   `file.stat` says `/nope does not exist at agent@…`.
3. **Engine refusals name arguments in the engine's camelCase, not the table's snake_case.**
   `--start-line=4611686018427387905` gives `startLine must be a positive safe integer`;
   `--page-size 0` gives `pageSize must be in 1..=1000`. The table's names are `start_line` and
   `page_size` (the renaming is the call's, documented in each [SDK page](../sdk/README.md)). Result
   keys are snake_case throughout (checked); only these messages are not.
4. **A one-key `{"base64": <text>}` map is bytes when the text is base64** (F7). Verified, consistent in
   all front ends, and harmless today: the only calls that take a JSON document store it as `changes`,
   where `content` must be text or bytes, and a one-key map whose text is not base64 is refused
   (`apply base64 not base64`, `detail.index` 0). It is a documented reserved shape, not a loss.

Not gaps, verified: Node `bigint` is exact (2^63-1 reaches the library unchanged); a `number` beyond a
safe integer and a lone surrogate are refused before the call; Python refuses a float and a lone
surrogate; every refusal has exactly `code`, `message`, `detail`; a refused call left no state.

## Build and run

Linux or macOS host, from the repository root; temporary homes need a filesystem with SQLite locks
(`TMPDIR` on ext4, xfs or btrfs).

```text
cargo +1.85.1 build --locked --offline -p layerfs-cli -p layerfs-node
node api/scripts/build-native.mjs --debug          # api/node/layerfs.node
(cd api/python && maturin develop --locked --offline)
export LAYERFS_PYTHON=<python with layerfs importable>   # default python3
node api/scripts/gen-coverage.mjs                  # rewrite the matrix
node api/scripts/gen-coverage.mjs --check          # fail when it is stale
(cd api/node && node --test test/coverage.mjs test/contract.mjs)
```

`LAYERFS_CLI` names another command binary; `CARGO_TARGET_DIR` moves the default one.
`LAYERFS_CONTRACT_REPORT=<file>` writes each case's normalized answer per front end.

---

← [Call table: Errors](errors.md) · [Up: Call table](README.md)

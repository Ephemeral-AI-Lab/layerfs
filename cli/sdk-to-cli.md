# From the SDK to the `layerfs` command

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract. The command is implemented on the #428 branch, in
> `cli/`, over `@layerfs/agent`.

`layerfs` runs one `@layerfs/agent` call per command, so an agent whose only
tool is Bash can use LayerFS without writing a Node script for each call.

It is only a wrapper. Each command opens the project with `LayerFS.open`,
makes exactly one SDK call through the SDK's public API, prints the result and
closes the client. It adds no capability, check or retry of its own: the SDK
checks the inputs and makes its one attempt, and the command reports what it
answered. Settings come from the environment exactly as for the SDK, and the
same platform rules apply (Store calls run in this process on Linux, through
the DSH worker on macOS and Windows, and fail `Unsupported` anywhere else).

## Install and run

The command is the `cli/` folder of a LayerFS checkout; it isn't published to
npm. It needs Node 20 or newer and the SDK's native addon (`npm run
build:native` in `sdk/js`, or `LAYERFS_NATIVE`).

```bash
npm --prefix "$LAYERFS_REPO/sdk/js" install
npm --prefix "$LAYERFS_REPO/cli" install
"$LAYERFS_REPO/cli/src/layerfs.js" help
```

The package's bin is `layerfs`; link it onto `PATH` (for example `npm link` in
`cli/`) to type `layerfs` alone. A short session:

```bash
export LAYERFS_PROJECT=shop                        # or --project shop on each command
layerfs branch fork cart
printf '# Shop\n' | layerfs fs write cart /README.md -m readme
layerfs sandbox create                             # {"sandbox":"1","image":"sha256:…"}
layerfs workspace mount 1 cart                     # {…,"ws":"1:/workspaces/3"}
layerfs workspace exec 1:/workspaces/3 "npm test" --stream
layerfs workspace commit 1:/workspaces/3 -m "step 1: npm test"
layerfs workspace unmount 1:/workspaces/3
layerfs sandbox close 1
```

## How a call becomes a command

- **Names.** A call `<family>.<call>` is `layerfs <family> <call>`, with the
  SDK's names (`branch list_commits`); a dashed name is the same call
  (`branch list-commits`). `LayerFS.init` is `layerfs init`. `LayerFS.open`
  and `close` happen inside every command; the project is `--project <name>`,
  else `LAYERFS_PROJECT`, else `DSH_LAYERFS_PROJECT`.
- **Positional arguments** are the SDK's, in the SDK's order. An optional one
  that is left out is not sent, so the SDK applies its own default (`fs list
  main` lists `/`, `branch fork cart` forks from main).
- **Options** are the SDK's remaining arguments as flags: `--page-size <n>`,
  `--start-line <n>`, `--hunks`, `--image <digest>`. A value is `--name value`
  or `--name=value`; `-m <text>` is the Commit message, the only short option.
  A whole number must be digits only, `--units` is JSON, a flag takes no value,
  and an option given twice or not known is refused. `--` ends the options, so
  `layerfs workspace exec 1:/workspaces/3 -- --version` runs `--version`.
- **A Workspace** is written `<sandbox>:<path>`, such as `1:/workspaces/3`.
  `workspace mount` prints it as `ws` beside the SDK's `{ sandbox, path, image }`.
- **Bodies.** `fs write` reads the file's bytes from stdin, or from `--from
  <file>`, and sends them exactly. `fs apply` reads a JSON array of changes the
  same way. `fs edit` takes `--old <text>` or `--old-from <file>`, and `--new`
  or `--new-from`. Input is read before anything opens.
- **Global options**, anywhere before `--`: `--project <name>`, `--text`
  (readable output instead of JSON), `-h`/`--help`, `--version`.
- **exec.** `workspace exec` prints the SDK's result as JSON, with the
  command's `exitCode` inside, and exits 0. With `--stream` it writes the
  command's stdout and stderr through as they arrive, prints nothing else and
  exits with the command's own code.

## Output and exit codes

| What | Where |
| --- | --- |
| The call's result | stdout, one line of JSON; `--text` prints it as readable `key: value` lines. Bytes print as `{ "base64": … }` |
| A refusal or failure | stderr, `{"code", "message", "detail"}` as the SDK's `LayerFSError` gave them; `--text` prints `Code: message` |
| A wrong command line | stderr, `layerfs: <what is wrong>` and the command's usage line |

| Exit | Meaning |
| --- | --- |
| 0 | The call succeeded |
| 2 | The command line is wrong (unknown command, option or argument, a bad value or input, no project); nothing was opened or called |
| 125 | LayerFS refused or failed the call. The SDK's own input checks land here too, because they run inside the call |
| the command's code | `workspace exec --stream` only; 125 still means LayerFS failed |

## Every call

Each command is one file, `cli/src/<family>/<call>.js`, beside the SDK's own
`sdk/js/packages/agent/src/<family>/<call>.js` (`layerfs init` is
`cli/src/project/init.js`, where the SDK keeps Init). Each family's `index.js`
lists its commands in help order, and `cli/src/calls.js` assembles them into
the call table. The table below is written from it by
`node cli/test/doc-tables.mjs --write`; `cli/test/cli.test.mjs` fails when the
table differs, and when a family's files stop matching the SDK's.

<!-- calls:start -->
| SDK call | Command |
| --- | --- |
| `LayerFS.init(name, folder, timeout?)` | `layerfs init <name> <source-dir> [--timeout <n>]` |
| `project.list_branches` | `layerfs project list_branches [--page-size <n>] [--page-pos <n>]` |
| `project.list_sandboxes` | `layerfs project list_sandboxes [--page-size <n>] [--page-pos <n>]` |
| `branch.fork` | `layerfs branch fork <name> [<from-ref>]` |
| `branch.list_commits` | `layerfs branch list_commits <branch> [--page-size <n>] [--page-pos <n>]` |
| `branch.diff_commits` | `layerfs branch diff_commits <from-commit> <to-commit> [--page-size <n>] [--page-pos <n>] [--cursor <text>] [--hunks]` |
| `branch.diff_branches` | `layerfs branch diff_branches <from-branch> <to-branch> [--page-size <n>] [--page-pos <n>] [--cursor <text>] [--hunks]` |
| `branch.merge` | `layerfs branch merge <a> <b> <name> [-m <text>] [--units <json>]` |
| `branch.publish` | `layerfs branch publish <branch> <commit>` |
| `fs.read` | `layerfs fs read <ref> <path> [--start-line <n>] [--end-line <n>]` |
| `fs.list` | `layerfs fs list <ref> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.stat` | `layerfs fs stat <ref> <path>` |
| `fs.grep` | `layerfs fs grep <ref> <pattern> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.glob` | `layerfs fs glob <ref> <pattern> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.diff_files` | `layerfs fs diff_files <left-ref> <left-path> <right-ref> <right-path>` |
| `fs.blame` | `layerfs fs blame <ref> <path> [--from-line <n>]` |
| `fs.write` | `layerfs fs write <branch> <path> [-m <text>] [--from <file>]` |
| `fs.edit` | `layerfs fs edit <branch> <path> [-m <text>] [--old <text>] [--old-from <file>] [--new <text>] [--new-from <file>]` |
| `fs.apply` | `layerfs fs apply <branch> [-m <text>] [--from <file>]` |
| `fs.mkdir` | `layerfs fs mkdir <branch> <path> [-m <text>]` |
| `fs.rm` | `layerfs fs rm <branch> <path> [-m <text>]` |
| `fs.cp` | `layerfs fs cp <branch> <from-path> <to-path> [-m <text>]` |
| `fs.mv` | `layerfs fs mv <branch> <from-path> <to-path> [-m <text>]` |
| `workspace.mount` | `layerfs workspace mount <sandbox> <branch>` |
| `workspace.exec` | `layerfs workspace exec <ws> <command> [-m <text>] [--stream]` |
| `workspace.commit` | `layerfs workspace commit <ws> [-m <text>] [--force]` |
| `workspace.unmount` | `layerfs workspace unmount <ws> [--discard]` |
| `sandbox.create` | `layerfs sandbox create [--image <text>]` |
| `sandbox.list_workspaces` | `layerfs sandbox list_workspaces <sandbox> [--page-size <n>] [--page-pos <n>]` |
| `sandbox.close` | `layerfs sandbox close <sandbox>` |
<!-- calls:end -->

## What the command leaves out

- **`@layerfs/backend` and `@layerfs/runtime`.** Their calls serve integration
  hosts. A separate admin command for the runtime (`layerfs-admin`) is a
  possible follow-up; it isn't built.
- **Short forms.** The command has only the SDK calls above. The older `lfs`
  command in `@layerfs/runtime` (`status`, `log`, `ls`, `cat`, `grep`, `diff`,
  `fork`, `write`, `sandbox`, `sandboxes`, `run`) is unchanged, and dsh-layerfs
  still gives it to agents as `DSH_LAYERFS_CLI`.

## Cost per command

Each command is a new Node process. Before its one call it loads the SDK and
the native addon and opens the project's Store (`LayerFS.open` also makes one
`project.list_branches` call to check the project exists); a Workspace call
also connects to the Sandbox's daemon. An SDK script pays this once for many
calls. This cost has not been measured. For a loop of many calls, a script is
the cheaper route until it is.

## Tests

```bash
npm --prefix cli install
node --test cli/test/cli.test.mjs
```

The tests run the real `@layerfs/agent` JavaScript over a stand-in native
binding (`cli/test/fixture/fake-native.cjs`) that records every call, so they
need no Docker, Store or daemon. They check that every table entry makes its
SDK call with the right arguments, plus parsing, bodies, output and exit codes,
and that `cli/src` has a command file for every SDK call file.

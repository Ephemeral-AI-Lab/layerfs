# LayerFS CLI calls (2.0)

> **Status:** Research; informative and not a product contract.

Extracted 2026-11-14 from the call table on `origin/layerfs_v2_0_0` at `8cfb6e2734`
(`api/crates/layerfs-api/src/<family>/<call>.rs`, one `CALL` entry per file). The
CLI, the Node SDK and the Python SDK are all generated from this table, so
`layerfs <family> <call>` here is `layerfs.<family>.<call>` in the SDKs. Usage lines
follow the CLI's own `help` rules: `_` becomes `-`, content comes from stdin or
`--from`. 73 calls in 10 families, plus 5 CLI-only commands.

Common to every call: `--text` (human output instead of JSON). Exit codes: `0` done,
`2` wrong command line, `125` refused (`{code, message, detail}` on stderr). The home
is `LAYERFS_HOME`, else `~/.layerfs`. A project is named `<store>/<project>`.

## store (12)

Stores: the one database (SQLite file or PostgreSQL schema).

| Command | What it does |
| --- | --- |
| `layerfs store attach <name> <machine> [--url <url>] [--password <password>]` | let a machine run Sandboxes of a PostgreSQL Store, after checking it reaches the server |
| `layerfs store connect <name> <url> [--password <password>] [--synchronous-commit <synchronous-commit>]` | attach a PostgreSQL database as a Store |
| `layerfs store create <name>` | an empty sqlite Store on native |
| `layerfs store detach <name> <machine>` | stop a machine running Sandboxes of a PostgreSQL Store |
| `layerfs store disconnect <name>` | forget a PostgreSQL Store; its data stays |
| `layerfs store get <name>` | a Store's record |
| `layerfs store list` | every Store, from the home's records |
| `layerfs store pull <name> [--progress]` | bring a sqlite Store from its machine to native |
| `layerfs store push <name> <machine> [--progress]` | send a sqlite Store from native to a machine |
| `layerfs store remove <name>` | delete a sqlite Store with every project in it |
| `layerfs store stats <name>` | Store-wide publication and allocation counters |
| `layerfs store status <name>` | a Store's live state |

## project (6)

Projects inside a Store.

| Command | What it does |
| --- | --- |
| `layerfs project export <project> <destination> [--branch <branch>] [--progress]` | export a project's snapshot from its Store to a local folder |
| `layerfs project get <project>` | a project's record: its Store, kind and machine |
| `layerfs project init <project> <folder> [--timeout <timeout>] [--progress]` | import a host folder as a new project into an existing Store |
| `layerfs project list [<store>] [--page-size <page-size>] [--page-pos <page-pos>]` | every project in the home, or one Store's |
| `layerfs project remove <project>` | remove one project from its Store |
| `layerfs project stats <project> [<branch>] [<commit>]` | entry counts of one pinned saved tree |

## branch (8)

Branches: fork, merge, publish, diff.

| Command | What it does |
| --- | --- |
| `layerfs branch commits <project> <branch> [--page <page>] [--page-size <page-size>] [--at-commit <at-commit>] [--include-storage]` | saved branch commits or main publications, newest first |
| `layerfs branch diff <project> <from-branch> <to-branch> [--paths-only] [--page-size <page-size>] [--page-pos <page-pos>] [--cursor <cursor>] [--page <page>] [--from-commit <from-commit>] [--to-commit <to-commit>] [--path-prefix <path-prefix>] [--include-attribution] [--continuation <continuation>]` | files that differ between two Branches' heads |
| `layerfs branch fork <project> <name> <from-branch> [<from-commit>]` | a new Branch from another Branch's head or Commit |
| `layerfs branch get <project> <branch>` | one Branch and its head Commit |
| `layerfs branch get-commit <project> <branch> <commit>` | one saved Commit or main publication |
| `layerfs branch list <project> [--page-size <page-size>] [--page-pos <page-pos>] [--page <page>]` | a project's Branches |
| `layerfs branch merge <project> <name> <a> <b> [--message <message>] [--units <units>]` | merge two Branches into a new one |
| `layerfs branch publish <project> <branch> [<commit>]` | a Branch's head or Commit as main's next Layer |

## file (15)

File calls: run in the caller's process, no Sandbox; each write is one Commit.

| Command | What it does |
| --- | --- |
| `layerfs file apply <project> <branch> [--message <message>] [--from <file>]` | apply a list of changes; one Commit |
| `layerfs file blame <project> <branch> <path> [<commit>] [--from-line <from-line>] [--line-count <line-count>]` | the Commit of each line |
| `layerfs file cp <project> <branch> <from-path> <to-path> [--message <message>]` | copy a path; one Commit |
| `layerfs file diff <project> <left-branch> <left-path> <right-branch> <right-path> [--left-commit <left-commit>] [--right-commit <right-commit>] [--include-attribution] [--allow-missing] [--page <page>] [--page-size <page-size>]` | the difference between two files |
| `layerfs file edit <project> <branch> <path> [--old <old>] [--new <new>] [--message <message>]` | replace one unique text in a file; one Commit |
| `layerfs file glob <project> <branch> <path> <pattern> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]` | paths under `path` matching a glob |
| `layerfs file grep <project> <branch> <path> <pattern> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]` | lines under `path` matching a regular expression |
| `layerfs file list <project> <branch> <path> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]` | a folder's entries |
| `layerfs file mkdir <project> <branch> <path> [--message <message>] [--mode <mode>]` | make a folder; one Commit |
| `layerfs file mv <project> <branch> <from-path> <to-path> [--message <message>]` | move a path; one Commit |
| `layerfs file read <project> <branch> <path> [<commit>] [--start-line <start-line>] [--end-line <end-line>]` | read a file |
| `layerfs file revision <project> <branch> <path> [--at-commit <at-commit>] [--traversal <traversal>] [--page <page>] [--page-size <page-size>] [--continuation <continuation>]` | saved changes to a literal path |
| `layerfs file rm <project> <branch> <path> [--message <message>]` | remove a path; one Commit |
| `layerfs file stat <project> <branch> <path> [<commit>]` | a path's kind, size and mode |
| `layerfs file write <project> <branch> <path> [--message <message>] [--mode <mode>] [--from <file>]` | write a file; one Commit |

## commit (3)

Commits.

| Command | What it does |
| --- | --- |
| `layerfs commit diff <project> <from-commit> <to-commit> [--paths-only] [--page-size <page-size>] [--page-pos <page-pos>] [--cursor <cursor>]` | files that differ between two Commits |
| `layerfs commit get <project> <branch> <commit>` | one Commit of a Branch |
| `layerfs commit list <project> <branch> [--page-size <page-size>] [--page-pos <page-pos>]` | a Branch's Commits, newest first |

## sandbox (4)

Sandboxes: long-lived daemon plus isolation.

| Command | What it does |
| --- | --- |
| `layerfs sandbox close <name> [--force]` | stop a Sandbox |
| `layerfs sandbox create <project> <name> [<machine>] [--image <image>] [--progress]` | start a named Sandbox of a project on a machine (default: its sqlite Store's, or native) |
| `layerfs sandbox get <name> [--page-size <page-size>] [--page-pos <page-pos>]` | a Sandbox and its Workspaces |
| `layerfs sandbox list [<project>] [--page-size <page-size>] [--page-pos <page-pos>]` | every Sandbox in the home, or one project's |

## workspace (6)

Workspaces: per-call mount, run, commit, unmount.

| Command | What it does |
| --- | --- |
| `layerfs workspace commit <ws> [--message <message>] [--force]` | commit a Workspace's changes to its Branch |
| `layerfs workspace exec <ws> <command> [--message <message>] [--stream] [--exec-id <exec-id>] [--progress] [--events]` | run one bash command in a Workspace |
| `layerfs workspace exec-cancel <ws> <exec-id> [--grace-ms <grace-ms>]` | end a running command: SIGTERM, then SIGKILL after grace_ms |
| `layerfs workspace mount <sandbox> <branch> [--name <name>]` | mount a Branch in a Sandbox |
| `layerfs workspace status <ws>` | a Workspace's changes since its base |
| `layerfs workspace unmount <ws> [--discard] [--force]` | end a Workspace |

## machine (12)

Machines: where Sandboxes run (native, docker, lima, apple-container).

| Command | What it does |
| --- | --- |
| `layerfs machine create <name> <platform> [--cpus <cpus>] [--memory <memory>] [--disk <disk>] [--image <image>] [--os-image <os-image>] [--progress]` | create an enabled VM machine (apple-container, lima) |
| `layerfs machine get <name>` | a machine's record |
| `layerfs machine images <name>` | the Sandbox images a machine holds, and the host's Docker images it can copy |
| `layerfs machine list` | every machine, with what it supports |
| `layerfs machine load <name> <image> [--progress]` | copy an image from the host's Docker to a machine without creating a Sandbox |
| `layerfs machine platforms` | the platforms this host and build support, and what each is missing |
| `layerfs machine prune <name> [<image>] [--unused]` | remove a Sandbox image from a machine, or every unused one |
| `layerfs machine remove <name>` | delete a VM machine and its disk; one of a removed platform is only forgotten |
| `layerfs machine start <name> [--progress]` | start a stopped machine and its worker; a running one answers at once |
| `layerfs machine status <name>` | a machine's live state, free disk and tool version |
| `layerfs machine stop <name> [--force]` | stop a machine's worker and VM; its disks stay |
| `layerfs machine update <name> [--cpus <cpus>] [--memory <memory>] [--image <image>] [--docker <docker>] [--wsl <wsl>] [--daemon <daemon>]` | resize a VM, or set a built-in machine's image, Docker socket and daemon |

## backup (5)

Backups to an R2 or MinIO bucket.

| Command | What it does |
| --- | --- |
| `layerfs backup connect <name> <url> [--key <key>] [--region <region>] [--part-size <part-size>]` | connect an r2 or MinIO bucket |
| `layerfs backup create <store> <bucket> [<file>] [--progress]` | upload a sqlite Store to a bucket |
| `layerfs backup disconnect <name>` | forget a bucket and this home's links to it |
| `layerfs backup list [<bucket>]` | every Store file on the connected buckets |
| `layerfs backup restore <store> <bucket> [<file>] [--progress]` | download a backup as a sqlite Store on native |

## logs (2)

The home's call log.

| Command | What it does |
| --- | --- |
| `layerfs logs list [--op <op>] [--project <project>] [--client <client>] [--tag <tag>] [--outcome <outcome>] [--since <since>] [--cursor <cursor>] [--limit <limit>]` | the calls made against this home, newest first |
| `layerfs logs reset` | clear this home's call log |

## CLI-only commands (5)

| Command | What it does |
| --- | --- |
| `layerfs help [<family> [<call>]]` | this list; `layerfs help --json` prints the call table |
| `layerfs --version` | the build's version |
| `layerfs doctor [--quick \| --write-test] [--timeout <duration>] [--text]` | report what is wrong in the home, changing nothing |
| `layerfs import-env` | write this environment's 1.1.0 settings as records, once |
| `layerfs worker` | serve this machine's worker (inside a machine) |

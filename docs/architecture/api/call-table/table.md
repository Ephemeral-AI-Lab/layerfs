# Table

[Architecture](../../README.md) › [API](../README.md) › [Call table](README.md) › Table

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs.table`, keys, scopes and checks.

| | |
| --- | --- |
| Component | [Call table](README.md) |
| Source | [`api/crates/layerfs-api/src/calls`](../../../../api/crates/layerfs-api/src/calls), [`api/crates/layerfs-api/src/lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs), [`calls/table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs), [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs), [`calls/keys.rs`](../../../../api/crates/layerfs-api/src/calls/keys.rs), [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |

## layerfs.table

```text
 layerfs-api/src/lib.rs   families() -> &[Family; 9]
   Family { name: "file", list: file::calls }
        |
        v
 layerfs-api/src/file/mod.rs   calls() -> &[read::CALL, stat::CALL, ...]
        |                          (help order)
        v
 layerfs-api/src/file/read.rs  const CALL: Call = call("file", "read",
                                 args, options, help, run)
        |
        v
 Call { family, call, args: &[Arg], options: &[Arg], help, run: Run }
   Arg { name, kind: Kind, optional }
   Run = fn(&mut Context, &SdkValue) -> Outcome
        |
        +--> table_json(): [{family, call, args:[{name,kind,optional}],
        |                    options, help}]
        |      Node: `table` (also on the default export)
        |      Python: layerfs.TABLE
        +--> CLI parser and help read the same Call values
```

The table is code: one file per call under its family folder in
`layerfs-api` (owner decision D15), each holding the call's `CALL` entry and
the `run` function that hands it to `layerfs-control`
([`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs),
[`table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs)).
There are 61 calls in nine families, including
[`project.export`](../sdk/project/export.md). `layerfs.table` is the JSON form front
ends build their functions from (`table_json`); `layerfs doctor` and
`layerfs worker` sit outside it.

| Family | Calls, in help order | Count |
| --- | --- | --- |
| [project](../sdk/project/README.md) | init, list, get, export, remove | 5 |
| [branch](../sdk/branch/README.md) | list, get, fork, merge, publish, diff | 6 |
| [commit](../sdk/commit/README.md) | list, get, diff | 3 |
| [file](../sdk/file/README.md) | read, stat, list, glob, grep, blame, diff, write, edit, rm, mkdir, mv, cp, apply | 14 |
| [sandbox](../sdk/sandbox/README.md) | create, list, get, close | 4 |
| [workspace](../sdk/workspace/README.md) | mount, exec, exec_cancel, commit, unmount, status | 6 |
| [machine](../sdk/machine/README.md) | list, platforms, get, status, start, stop, create, remove, update | 9 |
| [store](../sdk/store/README.md) | list, get, status, create, connect, push, pull, remove, disconnect | 9 |
| [backup](../sdk/backup/README.md) | connect, disconnect, list, create, restore | 5 |

`project.export(project: Project, destination: Text, { branch: Name = "main",
on_progress: Function })` adds one project-level call. It accepts no `commit`
option, and a folder scope refuses it. Its CLI form is
`layerfs project export <store>/<project> <destination> [--branch <name>] [--progress] [--text]`.

| Kind | Accepts | Shell form | Source |
| --- | --- | --- | --- |
| `Name` | a machine, Store, bucket, Branch or Sandbox name, or a Commit id | word | `table.rs` |
| `Project` | `<store>/<project>`, checked by `names::project` | word | [`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| `Text` | a path or URL | word | `table.rs` |
| `Int` | a whole number | word | `table.rs` |
| `Bool` | true or false | flag | `table.rs` |
| `Json` | any JSON value | JSON text (stdin or `--from`) | `table.rs` |
| `Content` | text or bytes | stdin or `--from` | `table.rs` |
| `Size` | bytes, or text `512`, `64K`, `512M`, `8G` (powers of 1024) | word | `check.rs` `size` |
| `Function` | `on_progress`, `stdout`, `stderr`; stays in the front end | none (`--progress`, `--stream`) | `table.rs` |

Shared option sets in [`table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs):
`PAGE` (`page_size`, `page_pos`), `DIFF` (`paths_only`, `page_size`,
`page_pos`, `cursor`), `MESSAGE` (`message`), `PROGRESS` (`on_progress`).
Positionals go from the outside in: owner, Branch, path or new name, other
inputs, an optional Commit last (D7).

## Keys

| Rule | Example | Source |
| --- | --- | --- |
| Argument names are snake_case and the same in every front end | `start_line`, `from_path` | `table.rs` |
| A shell option is the name with `-` for `_` | `--start-line` | `table.rs`, [`layerfs-cli/src/parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs) |
| Result keys become snake_case before they leave the library | `exitCode` → `exit_code` | [`keys.rs`](../../../../api/crates/layerfs-api/src/calls/keys.rs) `snake`, applied in `dispatch.rs` |
| Only identifier-like keys starting with a lowercase letter change; a key holding a path or name never does | `/src/main.rs` stays | `keys.rs` `snake_name` |
| The Store methods still take 1.1.0 camelCase names; the `branch`, `commit`, `file` families rename on the way in | `start_line` → `startLine`, `old` → `oldText`, `from_path` → `from` | [`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs) `engine_args`, `renamed` |

## Scopes

```text
 scope { project: "<store>/<project>", folder: "/abs/path", branch?: ns }
   |
   v
 Scope::parse  --> InvalidInput on unknown field, bad project, non-canonical
   |               folder, branch not a history name
   v
 Scope::apply(name, args)       before the home is opened
   commit.*, branch.diff        --> out of scope: history
   file.*                       --> project must match; every path inside
                                    folder; branch/commit names composed
   branch list/get/fork/        --> project must match; names composed as
   merge/publish                    "<ns>.<name>"; list filtered to ns;
                                    fork from_commit and publish commit
                                    like file.* commit (below)
   workspace.mount              --> only "main", mounted as "<ns>.main"
   workspace.exec               --> command prefixed: cd '.<folder>' && ...
   workspace commit/unmount/    --> unchanged
   status
   anything else                --> out of scope: <family.call>
   |
   v
 Scope::owns(home, name, args)  workspace.*: the Sandbox's record names
                                the scope's project, else out of scope:
                                sandbox
```

A folder scope (O32) is a client option of every front end; it guards the
arguments of a call, not processes: `workspace.exec` is only a `cd` prefix,
and a command can still reach anywhere in the mounted tree
([`scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).
A scoped `branch.get` answer is renamed back relative to the namespace in
the dispatcher ([`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs)).

A Branch reference is a Branch name, composed as `<ns>.<name>`, or a
66-character hex id, left as it is. `file` calls apply this to `branch`,
`commit`, `left_*` and `right_*`; `branch.fork` to `from_commit`
(`from_branch` and `name` are composed) and `branch.publish` to `commit`.
Any other value is treated as a Branch name and composed into the namespace;
the Store then decides whether that Branch exists (`InvalidInput`,
`NoSuchBranch`). A name that cannot be composed, or a scope without `branch`,
is `out of scope: branch` before any effect. Tests:
[`scope_references.rs`](../../../../api/crates/layerfs-api/tests/scope_references.rs).

Not confined; the owner has not decided whether they should be:

| Gap | What happens today |
| --- | --- |
| Commit and Layer ids | a 66-character hex id passes as given wherever a reference is taken, so a scoped client can read or fork from a Commit or Layer of any Branch of its project if it knows the id (the Store resolves it by identity; reading another project through an id is not verified) |
| An id in `branch` | a 66-character hex id placed in a `file` call's `branch` field is accepted as a Commit id |
| `workspace.*` Branches | `Scope::owns` checks only that the Sandbox belongs to the scope's project; the Branch a Workspace was mounted on is not checked, so `workspace.commit` on a Workspace of a Branch outside the namespace writes to that Branch |
| Scope without `branch` | the option is optional, but every `file` and `branch` call has a `branch` argument and is refused (`out of scope: branch`); only `workspace.exec`, `commit`, `unmount` and `status` pass, so it is of little use |

The scope's nature, not a gap: it checks call arguments only. It does not
confine the process `workspace.exec` starts.

## Checks

```text
 call_scoped(families, home?, scope?, name, args, events)
   1 find(name)                    no call named ... (InvalidInput)
   2 check(spec, args)             names, required, kinds; sizes -> bytes
   3 scope.apply                   (when scoped)
   4 home(explicit | LAYERFS_HOME | $HOME/.layerfs), must be absolute
   5 migrate::open                 refusals noted by note_failure
   6 machine::ensure_built_ins     native, docker (feature "machine")
   7 scope.owns                    (when scoped)
   8 (spec.run)(Context{home, events}, args)
   9 keys::snake(value)
```

| Check | Refusal | Source |
| --- | --- | --- |
| Arguments not a map (null is no arguments) | `InvalidInput` "arguments must be named fields" | [`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| Unknown name, or a `Function` argument reaching the library | `InvalidInput` "unknown argument" | `check.rs` |
| Required positional missing or null | `InvalidInput` "`x` is required" | `check.rs` |
| Value of the wrong kind | `InvalidInput` "`x` must be …" | `check.rs` `kind`, `describe` |
| `Size` text that does not parse, or a negative number | `InvalidInput` "a size such as 512M or 8G" | `check.rs` `size` |

Checked arguments are kept in the table's order. The call then runs once
with a `Context` that holds the home and the one event stream: progress
(`{progress}`), worker events, and command output (`{stream, bytes}`)
([`events.rs`](../../../../control/crates/layerfs-home/src/events.rs)).
Nothing in the dispatcher retries.

## Help text

| Help | Built from | Source |
| --- | --- | --- |
| `layerfs help` | each family's call names, plus `doctor`, `import-env`, `worker`, `help` | [`layerfs-cli/src/help.rs`](../../../../api/crates/layerfs-cli/src/help.rs) `print` |
| `layerfs help <family>` | each call's usage line and its `help` text | `help.rs` |
| `layerfs help <family> <call>` | that call's usage line and `help` | `help.rs` |
| Usage line | `<arg>` per positional, `[<arg>]` when optional; `[--opt <opt>]`, `[--flag]` for `Bool`, `[--progress]` for `on_progress`; `Content` and `Json` positionals become `[--from <file>] (content from stdin)` | `help.rs` `usage`, `option` |
| Node and Python | the same `help` text in each `table` entry | `table_json` |

The `help` string is the one line in each call's `CALL` entry; nothing else
writes help, so the table and help cannot disagree.

## Related

- [Call table](README.md): the dispatcher in the picture.
- [Errors](errors.md#refusals): refusals before any effect.
- [Front ends: CLI](../front-ends/cli.md): parsing against the table.
- [Front ends: Node](../front-ends/node.md), [Python](../front-ends/python.md): functions built from `table_json`.
- [Control: Families](../../control/management/families.md): what each `run` reaches.

---

← [API: Call table](README.md) · [Up: Call table](README.md) · [Call table: Errors](errors.md) →

# Merge rules: `/.lww`

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Read from
> `core/crates/layerfs-content/src/merge/lww.rs` and the compare-and-merge contract on
> `main`; the Commit case is covered by verification case B10.

## Treat it as your `.gitignore`

In the normal case `/.lww` is one line:

```
# /.lww
%include .gitignore
```

That makes every `.gitignore` pattern a merge rule. Dependencies, build output, caches
and logs then take the newer version whole instead of conflicting line by line, which
is what you want for files a tool regenerates.

**Add more lines only in the extreme case:** a file Git tracks that is still produced
by a tool, not written by hand, and that keeps conflicting. Lockfiles
(`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`) and checked-in generated code
(`src/generated/`) are the usual examples:

```
# /.lww
%include .gitignore
package-lock.json
```

**Never add source files.** Last-writer-wins silently drops the other side's change; a
source conflict should surface so you read both sides and resolve it.

If the project has no `.gitignore` yet, create one first, with the usual entries for
its stack (`node_modules/`, `dist/`, `build/`, `coverage/`, `.cache/`, `*.log` for Node).
Do this in the project's first step, before any parallel work.

**One difference from `.gitignore` to keep in mind:** LayerFS still stores and commits
those files. `/.lww` changes only how they merge, never whether they're saved.

## Where it applies

LayerFS merges in exactly two places, and `/.lww` applies to both:

| Merge | When it happens | Which `/.lww` is read | "ours" / "theirs" |
| --- | --- | --- | --- |
| **Commit merge** | `workspace.commit` when the Branch moved since the mount | The one on the Branch's current head | ours = the Workspace, theirs = the head |
| **Branch merge** | `branch.merge(a, b, name)` | The one on `a` | ours = `a`, theirs = `b` |

Nothing else merges, so nothing else reads it: `fs.*` writes are refused with
`HeadMoved` if the Branch moved during the call, and `branch.publish` replaces `main`'s
view. A path only one side changed takes that side's change, with or without a rule. A
change to `/.lww` takes effect once it's committed on the Branch the merge reads.

## How a path is decided

For each path both sides changed differently:

1. The **last** matching line decides. A matching `!` line means "no rule".
2. With a rule, the side with the **newer modification time** is kept whole; on a tie,
   ours. A side that deleted the path counts with its nearest surviving parent folder's
   modification time, so a newer deletion can win.
3. Without a rule, the usual merge runs: text files merge line by line, and overlapping
   changes become marker blocks and a `Conflict`
   (see [resolving conflicts](conflicts.md)).

Every rule decision is reported in the result's `lww` list, on success and inside a
`Conflict`'s `detail` alike:

| Field | Meaning |
| --- | --- |
| `path` | The path decided |
| `kept` | `"ours"` or `"theirs"` |
| `deleted` | `true` when the kept side had deleted the path |
| `rule` | The deciding line as written, including lines from an included file |

## Syntax

The same as `.gitignore`, with one addition, `%include`:

| Pattern | Meaning |
| --- | --- |
| `# text` | A comment |
| `node_modules/` | A name at any depth; a trailing `/` is ignored. A folder match covers everything below it |
| `/dist` or `config/local.json` | Contains a `/`: anchored at the project root |
| `*.log` | `*` matches within one name; `?` matches one character |
| `logs/**/*.txt` | `**` spans folders |
| `!keep.log` | Negates: a later matching `!` line means "no rule" for that path |
| `%include .gitignore` | Reads another file in place, by its path from the project root; a missing file is skipped (at most 16 files in total) |

Only the root `/.lww` is read. A `.gitignore` in a subfolder counts only if you
`%include` it.

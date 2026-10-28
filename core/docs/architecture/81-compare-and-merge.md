# Compare and merge of immutable roots

> **Status:** Content modules implemented for #327 step 4; merge-on-commit wired
> in the daemon in step 6. Branch merge, diffs and blame wire it in later steps.
> No timing, storage or resident-memory claim has been measured.

Content owns two modules over checked immutable filesystem roots:
[`compare/`](../../crates/layerfs-content/src/compare/mod.rs) answers what
changed between two roots, and
[`merge/`](../../crates/layerfs-content/src/merge/mod.rs) merges two roots
against their common ancestor. Both only read canonical objects and emit through
the caller's `FinalizedConsumer`. Neither opens a database or allocates serials
itself.

## Compare

| Function | Answer | Work |
| --- | --- | --- |
| `sorted::difference` | keys whose value differs between two sorted trees | page-skipping: a page id equal on both sides is never read |
| `inode_changes` | serials whose kind, content or metadata differ | one inode-table difference; the reference count is ignored |
| `binding_changes` | names whose binding differs in two directory contents | one directory difference |
| `locate` | first path of each wanted serial | breadth-first listing, stops when all are named or at a listing limit |
| `root_changes` | path-level Added/Removed/Modified with both sides' facts | the above, plus the subtree under a rebound directory |
| `line_diff`, `match_lines` | the simulator's line difference | LCS table bounded by `MAXIMUM_DIFF_CELLS`; beyond it the common prefix and suffix are kept and the middle is one block |

| `inode_differences` | serials whose stored value differs, reference counts included | one inode-table difference (#352) |
| `new_bindings` | `(child, parent, name)` bindings one root holds that another lacks | the inode difference, then each changed directory's binding difference or new directory's listing (#352) |

**Change walk, #352 (2026-10-18).** `branch.diff_commits` and
`branch.diff_branches` no longer walk both whole trees. Content's
[`compare/changes.rs`](../../crates/layerfs-content/src/compare/changes.rs)
`ChangeWalk` yields both roots' paths in component order with both sides'
entries and descends only through `enter`. `ChangeWalk::plan` takes the changed
serials from `inode_differences` and their paths from the caller's
`BindingHints` port, each hint checked against the tree it is used for, up to
the root; it enters a directory only when its listing changed, a changed inode
lies below it, or its identity differs between the sides, and in a directory
kept on both sides visits only the changed names and those paths. Work follows
the changed entries times their path depth. `ChangeWalk::full` lists both trees
completely. A stale or missing hint cannot change the answer: an inode with
fewer checked bindings than its reference count, or more than
`MAXIMUM_CHANGED` (65,536) changed inodes, makes `plan` answer `None`, and the
caller uses the full walk. Since #364 item 4 the SDK adapter's plan cache
(`read/plans.rs`) keeps that `None` like a plan, so the later pages of the same
two roots do not plan again, and a resumed full walk, like a planned one, starts
each directory on the cursor's path at the cursor's name (one lookup per side)
instead of its first name. layerfs-store-service `read/compare.rs` (first the
daemon's, then store-service's `sdk/compare.rs`) keeps only the SDK adapter:
reply budget and paging, and the History catalog behind
`BindingHints` (`HistoryCatalog::bindings_of`, the Store's `binding_hint`
table). The answer, order and paging equal the earlier full walk (oracle test
`history_diff_oracle`). Every saved tree's bindings are recorded where it is
produced: host Init records its whole tree, and a daemon Commit or an SDK merge
records the bindings its root adds over the history root it was built on
(`new_bindings`), in write transactions of at most 4,096 rows. Merge's own
`locate` is unchanged.

**Content flag, #352 F-073 (2026-10-20).** A file's content root depends on
how it was built, not only on its bytes (CAS handbook §2). A localized edit
keeps its predecessor's extents, so a file edited to other bytes and back can
hold the same bytes under another root. A differing root therefore does not
decide a change. Two regular files with different roots and the same length
go through Content's
[`compare/bytes.rs`](../../crates/layerfs-content/src/compare/bytes.rs)
`same_file_bytes`. It skips each extent both files take from the same chunk at
the same offset, compares the rest in 256 KiB windows from each side, and
stops at the first window that differs. `read/compare.rs` uses it in three
places:
- the `content` flag of a `diff_commits` or `diff_branches` item;
- whether a same-inode pair whose metadata is unchanged is listed at all;
- `fs.diff_files`'s `contentEqual` for files that are not text.

Merge uses `compare::same_content`, the same check on inode values, in two
places:
- the three-way decision (`Decider::same`): sides with equal bytes did not
  both change a file;
- merge-unit classification (`units.rs` `changed`).

In the publication lane, a bounded merge (`MergeInput::bound`) compares bytes
only for files of at most `MAXIMUM_TEXT_BYTES`, so the hold does not grow with
a file's size. Larger files are compared by root there.

Text is a regular file of at most `MAXIMUM_TEXT_BYTES` (1 MiB) that is valid
UTF-8 with no NUL byte. Lines are the `\n`-separated pieces, as the simulator
splits them.

## Merge

`merge_roots(reader, consumer, input, fresh, records, timing)` merges `ours`
and `theirs` against `base` by inode serial (#355 M1). It walks one side's
changes, `from`, and builds the result on the other, `into`, with one
`update_filesystem_streamed_backed` call, the same canonical producer as every
other root. `from` is ours for a Branch merge and for a Workspace Commit's
first stage, and theirs for the in-lane step, so that step walks only what
landed after the first stage. File bytes it writes (merged text or marker
text) go through `construct_bytes` into the same consumer first.

1. **Changes by serial.** `inode_changes(base, from)` lists each serial whose
   value `from` changed. `into`'s value for each is read by point lookup, 256
   at a time. A serial `into` holds as `from` does needs nothing. A serial
   `into` holds as the base does takes `from`'s value, metadata included. The
   rest changed on both sides. There is no cap on their number; a caller's
   `bound` (the in-lane Commit step) refuses past it, never as a partial
   merge.
2. **Names.** A directory `from` changed is merged by name.
   `binding_changes` of its base and `from` listings gives each name whose
   binding `from` changed, and `into`'s listing gives the third binding. A
   name `into` binds as `from` does needs nothing. A name `into` left as in
   the base takes `from`'s binding: an add, a move away, a deletion or a
   replacement. Two different bindings collide, and so does a replacement
   of a directory whose names the other side changed, whichever side made
   it: `into`'s is found at the name, `from`'s at the directory's unchanged
   path once `into` is known to lack the directory. Two directories new to
   the other side at one name are merged by name into `into`'s. Otherwise
   the entries decide.
3. **Decisions.** A file changed on both sides, or two entries colliding at
   a name, is decided with the simulator's rules. An entry is its kind plus,
   for a file or symlink, its content. Two directories are the same entry,
   and their children decide. So a directory one side only added to or
   removed from is the base's entry: a file or symlink the other side put
   at its name wins in either direction, with no `/.lww` decision, and the
   new names below the directory go with it.

   | Case | Result |
   | --- | --- |
   | ours = theirs, or ours = base | theirs |
   | theirs = base | ours |
   | a `/.lww` rule matches | the side with the newer mtime, ours on a tie; a deletion takes its nearest surviving parent's mtime |
   | both text files | `merge3`; differing chunks become marker blocks (`content`, or `add/add` without a base) |
   | both files, one not text | ours, unmarked (`binary`) |
   | one side deleted a file | the kept text inside a deletion marker block (`modify/delete`, `delete/modify`) |
   | anything else | ours if present, else theirs (`type`) |

   Metadata merges by value, mode and mtime separately (#352 F-052): a field
   only one side changed takes that side's value, so a chmod survives an edit
   of the same file on the other side. A field both sides changed takes the
   decided entry's (theirs for a directory). Modes changed differently on
   both sides of a file are a `mode` conflict; the decided entry's mode is
   kept.
4. **Structure.** Moves follow the serial, so an edit inside a directory the
   other side moved lands at the new path.

   | Case | Result |
   | --- | --- |
   | a file renamed differently on both sides | both names, bound to the one serial (two links) |
   | a directory renamed differently on both sides | `type` conflict; `from`'s move is cancelled |
   | crossed moves that would put a directory inside itself | `type` conflict; `from`'s move is cancelled |
   | `from` deleted a directory below which `into` changed | the directory survives with what `into` keeps; a deleted file against an edit is `modify/delete` |
   | `from` bound a serial `into` deleted | resurrected with `from`'s subtree; a file `from` edited there is decided as a deletion against an edit (`modify/delete`), as in the case above |
   | `from` changed a serial `into` deleted | decided as a deletion against a change; a kept file is placed at `from`'s path, its missing parents revived |
   | one side changed a directory's names, the other replaced it at the same path | decided at the path in either direction: a new directory there takes the changed directory's names; otherwise the entries decide (step 3), so a file or symlink there wins; a lost directory leaves its new serials unbound, a kept one stays whole |
   | a serial new on `from` that loses a collision | left unbound, with the new subtree below it; a file `from` hard-linked at other names only once every link is lost, else it stays at the names the merge keeps |
   | a file new on `from`, hard-linked at other names, whose name is decided with written bytes (merged text or markers) | the bytes are a new file at that name; the other names keep the file and its own bytes, in a Branch merge and in the root a conflicted Commit installs |

5. **Paths.** Only a serial that needs a path is named: a decision (for its
   conflict report and `/.lww` matching), a deletion against a change below
   it, and a revived parent. Paths come from recorded bindings when the
   Catalog has them, else from `locate`.
6. **Build.** Every directory whose bindings change gets its whole listing
   rewritten; link counts are derived from the changed bindings, and a serial
   left with none is released. A directory new to `into` is built from its
   bindings. A file value keeps its serial in three cases: the serial is new to
   `into` and `from` binds it at no other name, `into` already holds that
   value, or the value changes in place at the serial's only binding.
   Otherwise the caller's `fresh` allocator supplies a new serial, so one
   serial never carries two values: a new file `from` also binds at other
   names stays at those names with its own value, and the written value is a
   fresh file at the decided name. Link counts follow the bindings the merge
   keeps, for the file and for the fresh serial alike.

**Working state (#355 (d), M1).** Memory does not grow with the merge. The
serials deferred until moves are known are one run of 60 KiB chunks read once;
the subtree stacks, the point marks of the structural cases and the build's
plan rows (changed names per parent, values, fresh and assigned serials) are
records in the caller's `IndexedConstructionBacking`, the global Store's merge
scratch rows. Up to 8 MiB of them is held in memory first, and a consumed
chunk is released at once. Without records (the in-lane step) the same state
is a resident map, and the caller's `bound` limits it.

The `/.lww` rules are gitignore-style, with `%include <file>`. They are read
from the side `RulesFrom` names: theirs for a Workspace Commit, ours for a
Branch merge.

**Merge units (#352 F-057).** A Branch merge may name `units`: groups of
patterns in the `/.lww` syntax (`Units::new` admits them; a malformed
pattern or an empty or repeated name is refused). A unit covers what its
patterns match and everything below; a path two units match belongs to the
first listed; the root is in none. Units run before `merge_roots`, which is
unchanged:

1. **Classify** (`unit_changes`). For each side, a walk of the base against
   that side (`ChangeWalk`, planned from recorded bindings, else full) counts
   per unit the paths it changed and their latest mtime. Added, deleted, a
   new kind, new file or symlink content and new metadata are changes; a new
   serial with the same kind, content and metadata is not, so a relink is
   none. A deletion takes its nearest surviving parent's mtime. The state is
   one count and one time per unit and side.
2. **Decide** (`decide_units`). A unit both sides changed is kept from the
   side with the later write, ours on a tie, and reported. A unit one side
   changed merges as before.
3. **Revert** (`revert_units`). A side that lost a unit has its changes
   there reverted to the base: the same walk binds, at each path of a lost
   unit, the base's entry with the base's value, through the merge plan's
   records and one backed update of that side. A base directory the side
   lacks is rebuilt from its base listing; one the side moved out of the
   unit is rebuilt under a fresh serial beside the move. A restored file the
   side also binds outside the unit keeps one serial with the base's value
   (a cross link).
4. **Merge.** `merge_roots` runs on the reverted roots. Inside a decided unit
   only the winner changed anything, so no text merge, conflict or `/.lww`
   rule applies there; outside units nothing changes. Without units, or when
   no unit is decided, the inputs and the result are exactly those of a
   merge without units.

The result lists the decided units (`unit`, `kept`, `keptTime` as RFC 3339
UTC, `changedOurs`, `changedTheirs`, `dropped`, `crossLinks`) in a 16 KiB
window with `unitsTruncated`; the merge Commit's record keeps the unit list,
outside Commit identity, and `branch.list_commits` shows it as `units`.
Workspace Commit merges take no units.

The same matcher serves `fs.glob`. A `**/` segment at a name boundary matches
zero or more complete directory names, so `src/**/*.js` includes both
`/src/cart.js` and `/src/web/cart.js`. A single `*` stays within one name.
Consecutive `**/` segments retain that boundary: `src/**/**/index.js` also
matches `/src/index.js` without requiring a directory for either segment.

`paths_with_markers(reader, root, paths, timing)` reports which of the named
paths are text files that still hold a complete marker block (an opening
`<<<<<<< ` line, a later `=======` line and a later `>>>>>>> ` line).

## Limits

- Locating a path without recorded bindings lists directories breadth-first
  from the root. Its cost grows with the directories above the serial, not
  only with the change. Only the serials that need a path pay it.
- A hard-linked file is reported at its first path only.
- Merge units: a lost unit's revert writes its own reverted root into the
  merge's Save, and those objects stay there when the merged root does not
  use them. Each side's classification and revert walk the whole trees when
  the side changed more than `MAXIMUM_CHANGED` inodes or bindings are
  missing.
- Merged text and marker files take the kind and metadata of the side they
  came from, with each one-sided metadata change applied.

## Merge-on-commit

[`store/commit.rs`](../../crates/layerfs-daemon/src/store/commit.rs) and
[`store/commit_merge.rs`](../../crates/layerfs-daemon/src/store/commit_merge.rs)
run it inside the Commit's one Save, after the captured candidate is built.

1. A Workspace with a pending conflict is refused while any file that
   conflicted still holds a complete marker block. The reply is `Conflict` with
   `markers: true`. Nothing is published, and the failed capture is resolved as
   for any definite nonpublication.
2. The Branch snapshot is read again. If neither its head nor its base Layer
   moved, or `force` is set, the candidate is published as before. A forced
   Commit over a moved head keeps the captured parent and reports the displaced
   head.
3. Otherwise `merge_roots` merges ours (the candidate) into theirs (the current
   head's root). The base is the history root the Workspace is bound to. Theirs'
   `/.lww` applies, and the labels are `workspace` and `<branch>@<head>`.
   Serials come from the Store's allocator in reservations of 64.
4. **Clean:** the merged root is published against the head it merged with and
   installed as the Workspace's base. The reply is `MergeCommitted` with
   `from`, `head` and the `lww` decisions. If the head moved again in between,
   the publication is `HeadMoved` and nothing merges again.
5. **Conflict:** nothing is published. The merged root, with its marker files,
   is installed as the Workspace's *local* base, and the Workspace's history
   binding moves to the head. The conflicted paths are kept with it. The reply
   is `Conflict` with `head`, `base`, the conflicts with their hunks, the `lww`
   decisions and the paths the merge changed in the Workspace (at most 4096).
   The next Commit merges against the head it now follows.
6. **Kernel caches.** A clean or conflicted merge installs a root other than
   the one the Workspace showed, so the kernel may hold entries, attributes and
   pages that the install changed. Before answering, the control Commit walks
   the two roots with `inode_changes` and sends `inval_inode` for every inode
   the shown root had and the installed root changed or dropped, and
   `inval_entry` for every name rebound in a directory both roots hold
   ([`control/invalidate.rs`](../../crates/layerfs-daemon/src/control/invalidate.rs),
   [`session/invalidate.rs`](../../crates/layerfs-fuse/src/session/invalidate.rs)).
   The walk streams and has no cap. An entry the kernel does not cache answers
   ENOENT, which counts as done. A failed notification turns the known answer
   into a failure after it (`Failure::After`); nothing is resent. A process
   that still holds a descriptor or working directory on a replaced inode gets
   a failed request, which is retained as any other and makes the mount
   terminal. Found by VERIFICATION B10: without the step, a directory both sides
   created (`.next`) stayed cached under the Workspace's own serial and failed
   with ENOENT and EIO after the install.

The SPEC asks for the merge to be written into the Workspace overlay. This
implementation instead installs the merged root as the local base, which
retires the captured rows and keeps later rows on top. The Workspace view is
the same. The difference is that `status` reports the merged root as the local
base while the binding names the head.

### Control wire

`Request::Commit` carries `CommitControl { token, force }`. The new replies are
`MergeCommitted(MergedCommit)` and `Conflict(ConflictReport)`. A control record
may now fill one Noise transport plaintext (`u16::MAX - 16` bytes, up from
8192). A conflict report writes conflicts, hunk lines, decisions and written
paths while they fit, and sets `truncated` when it leaves some out. Paths and
lines longer than 4096 bytes are clipped at a character boundary.


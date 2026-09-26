# #237: compact record fresh-builder prototype

> **Status: Proposal; target LayerFS 0.1.7; not a released contract.**
> Source base `443635b7d` retains the measured direct-inode treatment.
> This plan is frozen before changing C1 or Service for this prototype.

## Selected boundary

Keep the existing native source scan, four file constructors, C5 contiguous
reservation, three sequential C2 Saves and public SDK call. The file Save
publishes all file roots as today. During the prerequisite Save, write one
compact record per `PreparedEntry` to an operation-owned, byte-capped run:
parent ordinal, name, typed kind, file/symlink content root and portable
metadata root. No file payload is spooled. The record order is the scanner's
existing breadth-first, per-directory byte-sorted order; its ordinal plus
the C5 reservation start reproduces the current serials.

C1 gets a fresh-native-only entry point over that replayable run. Before
emitting any tree object it validates every record: root row, representable
serial range, name grammar/order, nondecreasing parent groups, every
non-root parent ordinal earlier than the child and naming a directory.
Those facts prove a connected acyclic tree with one binding per non-root
entry. They are stricter than generic `FilesystemInput`, which stays in use
for pathless Init and Workspace updates. Directory construction replays
each parent group through the existing sorted page engine, including the
existing canonical empty-directory branch. It records directory root IDs
in a second ordered run, then merges those with a replay of inode records
and uses the existing sorted inode engine. It emits the same root profile.

The Service owns the input run, its byte charge and checked cleanup. C1
owns only bounded cursors/record windows; caller-supplied `FileBacking`
owns the small directory-root run and is released before root emission.
The initial macOS diagnostic must prevent a recently written input run
from flattering the measured C1 read phase and must record spool disk and
resident page-cache bytes. A cache-served spool, an unbounded file or a
new refusal of the existing 4,097-child directory is a failed treatment.

## Gates

First prove exact Core root and object-ID equality for a fixed source,
scope and C5 reservation against the current generic builder, plus empty,
nested, wide and invalid record cases. Preserve complete validation before
tree emission, C2 abort/unknown ownership and all current source checks.
Only then freeze one real SDK memory arm against an identity-matched
control, with a fresh Store/source, full reopened oracle, Store geometry,
scratch/page-cache custody and overlapping Workspace Commit progress.
The declared cache state determines whether raw public timing can support
any speed claim; no improvement is assumed. The registered #236 debug Init
and #229 sparse-history gates remain separate.

# Independent file and processing custody

> **Status:** Implemented S6 checkpoint after `1775fdf98`; S6 remains in progress.
> Physical admission/headroom and exact whiteout simplification are not delivered
> by this checkpoint. Native reference/output wiring remains S8.

Schema v11 adds nonrecycled engine owner IDs, exact open/file-read/captured-reader/
operation records, file reference counts and one independent orphan domain. The
[capabilities](../../crates/layerfs-overlay/src/contract/custody.rs) bind engine,
namespace and actual SQL ownership. Copying a token does not acquire another
reference. Every use and release checks its full identity. Caller request keys
allow observation of original retained custody after a lost reply; absence never
permits replay. Reusing a request after an exact release mints a different owner,
and the earlier token cannot affect it. Caller-owned primitive lease/source IDs
still require caller lifetime uniqueness; they are not minted capabilities.

[Open/read ownership](../../crates/layerfs-overlay/src/lifetime/file_owners.rs)
retains each descriptor separately from the short base source and independently
owned read window. A current source stays owned across canonical demand and
consumer output. File-read sources use a separate SQL key domain from ordinary
caller sources. They cannot be released through the ordinary source-release API.
Known install waits for those short sources, while a long open does not pin the
namespace generation chain. Open facts come from the authenticated base outside
SQL, then current mutable rows decide inside the owner job.

At last unlink of a regular file, one short transaction publishes the positive namespace tombstone
and, when open/lookup/read custody exists, a negative-domain inode plus exact old
immutable root and lower-domain bounds. No payload is copied. Later descriptor
writes/truncates use this independent domain. Ordinary serial writes to a removed
inode refuse; `WriteOpen`/`SetOpenAttributes` validate the exact writable handle
in every Workspace round and again in the atomic apply. Metadata evaluation sees
the current orphan rather than the namespace tombstone.

[Orphan maintenance](../../crates/layerfs-overlay/src/maintenance/orphan.rs)
incorporates one effective lower cell at a time, newest domain first. It preserves
new upper validity/epochs/cutoffs and advances a lower reference only after its
cells finish. Namespace fold/retirement skips independently referenced source
rows. Releasing a skipped source schedules targeted cleanup without restarting a
namespace cursor. The read view has at most three layers during migration and one
afterwards, independent of Commit count. Namespace layers stay at most two through
failure readiness. Layer queries return at most four rows to detect an invalid
composition invariant without growing resident input or silently dropping data.

Unowned removed payload and steps are automatically deleted in bounded live turns.
Their positive metadata tombstone remains until installed, so serial lookup cannot
fall through to a still-existing base inode. Failed composition does not copy
payload into an unlinked namespace row. Last independent open/lookup/read release
wakes held orphan cleanup. Existing read windows survive descriptor close and
logical Workspace close; terminal deletion waits for their exact release.

[Captured readers](../../crates/layerfs-overlay/src/lifetime/captured_reader.rs)
store the original generation, root and installed floor. Known install can advance
while a sealed reader keeps its old input. Definite failure composition parks while
that reader exists, then resumes on exact release. A read plan carries the retained
root to Workspace, which rebinds immutable demand outside SQL. Neither current
installed floor nor a newer root is substituted into sealed input.

[Processing custody](../../crates/layerfs-overlay/src/lifetime/operation.rs)
uses nonrecycled engine IDs and independent backed `owned_scratch`. Exact release
transfers scratch into bounded maintenance; old cleanup cannot address a newer
operation. Existing processing reads remain valid after logical close, while new
scratch mutation refuses. The trusted legacy operation/scratch primitives retain
their caller-ID contract and separate table. No Bash/process exit implies release.

[Typed daemon jobs](../../crates/layerfs-daemon/src/overlay/commands.rs) expose
acquire/retained/use/release transitions and account bounded input/reply capacity.
[Workspace read ports](../../crates/layerfs-workspace/src/ports/files.rs) compose
file and captured windows through the real owner, with provider I/O outside it.
Actual owning templates have [strategy diagnostics](../../crates/layerfs-overlay/src/diagnostics/lifetime_plan.rs);
whole-operation foreground counters and automatic work are separately attributed.
The [checkpoint checks](../issues/307/checks/s6-orphans/) retain successful/failing
runs and arithmetic. These are correctness/work diagnostics, not latency, resident
memory, physical device headroom or sustained-rate qualification.

Remaining S6 ownership work includes non-file lookup custody and a symlink read
view that survives failed composition. The earlier one-creation-cell readlink
path is still a carried gap. Neither regular-file proofs nor raw generic leases
complete those behaviors. Physical reservation/headroom and exact whiteout
base facts also remain required before the S6 exit.

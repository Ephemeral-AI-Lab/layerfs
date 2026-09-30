# Component namespace and active Workspace authority

> **Status:** Research; source-backed implementation description, not a release
> contract or performance qualification. Written against implementation source
> `671f4a46f8f354cf764d3d1555529b73a5c939e0`, tree
> `ed5d82c664c9720a49d609cfbbb43d632adf890e`. This docs-only handoff advances
> the earlier construction-parent pin to the implemented source. File hashes are recorded in
> [the #284 manifest](../issues/284/PORT-MANIFEST.json) and final handoff.

The construction foundation is T0 `05eb5c14849f1b874383fd1600ba9288f103ad93`.
The physical-mechanism donor is P0
`11a864fc133844cae7a4247b1f243d84d5763b10`. The design source is
`f9f9abb37332e23d1968a5ed204ff70e3e66d783`; #284's implementation-only owner
instruction holds all of its benchmark evaluation steps as Phase B.

## Commit preparation observation

The #286 source in this commit after `ab3212381` adds
`Workspace::commit_with_progress`. Existing `commit` delegates with observation
disabled; explicit Stage uses the same disabled observer. Both active and legacy
preparation call the observer on the existing construction thread after each
non-directory inode has known saved content/metadata and persisted local
bookkeeping. An observer error fails that one attempt through the existing typed
submission/Commit failure path, retaining known saved roots without reissuing any
construction or canonical command. Captured selection, generations, memory
charges, physical fund, C1/C5 calls and deadline remain the same. The daemon uses
this production observer for authenticated control-session progress; it changes
no content/metadata/history format or construction algorithm.

## Namespace and selection

Ordinary SDK Exec runs the supplied command under `/bin/sh` in the real FUSE
mount. FUSE callbacks resolve a stable parent serial and one checked component,
or use a stable file handle. Command text and fixture identity do not select
an alternate content route.

`runtime/state.rs` retains T0's charged `NodeName`, parent, attached and retained
fields. `runtime/ancestry.rs` walks the complete attached resident chain, bounded
by resident population to detect corruption rather than by a fixed namespace
component count. Collection retains the complete ancestor closure of a held
attached directory in both LocalEdit and ReadOnly. Node, View, directory handle
and private held lease entry carry no aggregate namespace path.

New LocalEdit attaches one `ActiveBacking`. All content and namespace mutation
entrypoints require that authority; the old namespace mutation backend has been
removed. `namespace_view.rs` selects an immutable filesystem Base and an
`ActiveSnapshot`. Active I/N records take precedence, with canonical identity
fallback for an unchanged component. A charged parent/component memo caches
immutable canonical facts within the same baseline. A frozen selection never
requires authorization from a later live ancestry graph.

`active_rename.rs` changes the source and destination N keys and affected I/D
keys. It prepares one charged component owner, checks generation/baseline/revision
and the complete attached parent chains under State, refuses cycles before active
publication, and installs only the moved resident Node edge. Replacement detaches
its old directory identity; hard links retain shared regular-file identity.
Frozen directory names retain their selected view, while `..` uses the live or
last parent edge. No descendant enumeration or resident path rewrite occurs.
Actual index/page I/O and ancestor work remain paid; this is not constant total
rename or Commit work. Direct canonical LogicalPath, backing/mount paths and an
individual pathname syscall retain their separate limits.

## Physical records and completion

`backing/active/` reuses P0's authenticated 4096-byte pack/index/hot pages,
I/N/D/E/P/R/L records, extents, HotRef selection, inverse references, pin cohorts,
retirement and checked unlink algorithms. Index/hot framing is v2; pack framing
is v1. The legacy metadata arena serves saved observations and completion
bookkeeping, not a second live mutation authority.

`State.base` is a filesystem root. An active inode's `I.base` is a file-content
root whose source coordinates select its Base extents. Selected active origins
precede canonical refresh; when no active inode exists, stale facts refresh by
`Inspect::InodeAttributes`, never a reconstructed path.

The first dirty publication owns the existing 208*4096-byte completion reserve
and the separately charged 256-byte fund object. Capture transfers that same
fund to immutable G1; live G2's first dirty publication admits its own fund. A
clean capture without a live fund explicitly admits one. Failed preparation can
retain unused precharged credit. Partial allocation returns unused credit to the
same fund; finishing releases unused reservation. Refund for allocated pages
requires exact identity, actual blocks and successful unlink. Selected, foreign,
failed or uncertain physical owners remain charged.

`commit/active.rs` prepares captured content and metadata, preserving the existing
O(E) per-file extent/descriptor tradeoff and bounded grouped pack source. Known
file roots enter saved observation custody before another metadata call. The
internal v2 upload resolves backward/repeated Base runs locally in the Service;
no recursive daemon ReadFile occurs while upload holds the session mutex and no
result-sized Base byte spool is introduced. Local and Zero inputs still pay for
actual transmitted/constructed bytes and canonical C1/C2 work.

`active_reconcile.rs` installs only the captured set after a checked canonical
outcome is saved. A matching regular revision collapses to the saved Base.
The pre-admitted reconciliation patch takes ownership of deletion key buffers;
its immutable preparation rows release their emptied lists and charges before
ordered/index scratch admission. Keys are moved rather than cloned, and all
patch bytes remain charged before allocation/publication. This avoids retaining
a second complete key list during local C5 completion without raising Budget or
changing the atomic patch, selected snapshots, physical fund or failure custody.
Intervening G2 with a nonzero Base retains its original coordinates; a fresh
absent Base follows P0's adoption rule. Symlink target extents and directory
bindings do not undergo regular-file collapse. One-identity resident updates use
`node_index`. C2 and History formats remain unchanged, and complete C1 alias/cycle
validation remains separate work; canonical parent indexing stays deferred.

The adaptation removes dirty declarations for fresh directories/symlinks whose
last binding disappears before capture, retaining their selected private facts
for held owners. Replacing such a fresh identity follows the same rule. A later
non-directory rename advances the I generation/revision with its new D key.
Every namespace publication carries post-publication cleanup error together with
its accepted revision and receipt; an error does not undo publication. Namespace
mutations explicitly release their temporary old selection while the receipt is
still owned, so final-pin retirement errors cannot disappear into View Drop.

## Compatibility, public owners and custody

V1 SaveFile opcode 20 remains. Authenticated capability 28 returns typed
`Response::FileSaveCapabilities { version: 2 }`; LocalEdit accepts exactly v2
before dependent inspection or mutation. SaveFile v2 opcode 29 remains internal.
Lease opcodes 21–27 are additive. Entry wire facts remain serial/kind/size/
references/mode/mtime; selected context stays in the lease header. Requests carry
token plus issued serial/component/range. Canonical content selection is private.

The charged lease registry admits 32 leases, binds issued identities to tokens,
and registers the root entry atomically with its pin and charge. Root entry
mode/mtime come from the selected live version at pin time, not the immutable
attachment defaults. Read/list/target
and response admission retain their existing bounds. Checked release runs the
selecting retirement path; uncertain release is never replayed. The daemon uses
one lifecycle slot and one serialized authenticated session. Native read timeout
is Deadline; uncertain mutation/release is Unknown. Conditional shared-Host
metadata admission permits only its existing scoped metadata seam. No second
connection or construction worker is added.

Known canonical/local-C5 failure retains the outcome and selector. Explicit
native same-selector resume performs eligible checked candidate repair and local
installation only. Public SDK Commit does not resume a retained Submission.
Unknown outcomes retain pins, credit, exact selectors and failed custody.
Unmount, clean close and Sandbox deletion remain distinct.

Clean close refuses handles, lookups, leases and external payload pins before
stopping. Admission excludes only the active authority's exact owned payload Arc,
not arbitrary references; this allows retained symlink payloads to drain while
client or unknown pins still refuse. Physical release remains identity/block
checked. Workspace durability is unchanged: no fsync, WAL or crash-recovery
promise. Default 8 MiB Budget, quotas, deadlines, integer limits, one canonical
construction worker and Init's existing exception are unchanged.

## Verification boundary

Phase A uses small external correctness checks, owning Core checks and live SDK/
FUSE component traversal. Its report lists failures and unrun routes explicitly.
All Phase B scaling, timing, lowering/headroom and full qualification campaigns
remain ON HOLD/NOT_RUN. Historical numeric rows remain INELIGIBLE; earlier control
`48b51e874a41b3e1e6c6661e145316df8b408f07` remains NOT_RUN. Host memory qualification
is #283; the 32 KiB pinned-read Io observation, #248 streaming/progress and >65,535
runs, #256 many-file scope and #276 parent indexing remain open limitations.

## C5 page credit admission after the prepaid floor

The #286 source in this commit after0b2cf37aa admits an active C5 page credit
through the captured completion fund and the same configured physical quota.
The208-page prepaid escrow is unchanged. If a requested page credit exceeds
the fund's remaining reservation, the difference is explicitly reserved from
MetadataHost before allocating the page and then deducted once. This is one
admission path, not a failed take followed by fallback/retry. Quota refusal
leaves the fund unchanged and returns the existing typed local failure; known
canonical outcome and any partial candidate/refunds remain in their existing
custody. Page give/recycle and fund finish/drop return both original and
additional credits to the same quota; no configured quota or memory bound is
raised. It removes an internal fixed completion-size refusal for larger
candidates, without changing the whole-frontier memory strategy, canonical
format, C5 history or construction worker count. Runtime capacity diagnostics
record requested/prior/additional credits.

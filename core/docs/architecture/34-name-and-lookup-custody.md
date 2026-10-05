# Name inheritance and non-file lookup custody

> **Status:** Implemented S6 checkpoint after `cae3d43ed`; physical reservation,
> cleanup headroom and device/resource exits remain unfinished. Native request,
> FORGET/cache/invalidation adaptation remains S8.

Schema v12 adds [minted lookup owners](../../crates/layerfs-overlay/src/lifetime/lookup.rs)
and one lower-binding boolean per dentry. Lookup tokens bind exact nonrecycled
engine/namespace/owner/serial identities; request-key observation returns original
retained custody after a lost reply and never authorizes acquisition replay.
Root zero reference count is canonical root metadata. Only a semantically removed
non-root final inode enters non-file orphan state; normal root attribute/parent
updates never detach the root. Workspace marks that exact removal in `Changes`.
The primitive caller-ID lease API retains its lifetime-uniqueness precondition.

Removed directory/symlink metadata uses the existing independent domain. A short
lookup-read window retains separate file-reader/source references so lookup release
or logical close cannot remove the state while its consumer continues. Readlink
now [composes effective layers](../../crates/layerfs-workspace/src/workspace/view.rs)
and uses the original immutable root for inherited target bytes. It no longer
seeks only the original creation generation after failed composition moved the
cell. The symlink format still bounds its target to one 4096-byte window.

[Captured-reader metadata pages](../../crates/layerfs-overlay/src/lifetime/captured_reader.rs)
remain independently usable after install or definite failure, with exact original
generation/root/floor and fixed keyset windows. Captured symlink reads use the same
retained-root emitter as live/request reads. They do not substitute a current base
or release the capture's external/history obligations. Unknown outcomes retain
original custody under the existing caller/runtime contracts.

Each dentry's `inherited` bit describes the immediately lower view. On first
mutation an existing lower local binding decides; otherwise an authenticated base
fact decides. Rewrites retain the active row's original bit. [Workspace jobs](../../crates/layerfs-workspace/src/mutation/job.rs)
obtain a missing immutable fact outside SQL and revalidate mutable parent/name
facts in their publishing owner round. Stored facts avoid repeated base demand.
A known install makes the sealed lower view the immutable base, so no whole-name
rewrite is required and an existing active bit remains correct.

[Failed composition](../../crates/layerfs-overlay/src/lifetime/composition.rs)
handles one lower name per turn. The newest upper final binding survives and its
bit advances to the removed lower row's own bit. A whiteout with a false bit then
has nothing beneath it and is deleted, with exact dirty-name accounting. Necessary
whiteouts over existing base names survive. Lower captured rows are not rewritten
while their exact reader holds the domain. These are indexed point/keyset jobs,
without a resident namespace mirror or whole-tree pass on a foreground mutation.

[Retained checks](../issues/307/checks/s6-names/) include failed compile/fixture/lint
runs and their repairs. Public canonical proofs cover linked targets through 20
failed captures, directory/symlink owners, request custody beyond lookup release,
logical close, redundant/necessary whiteouts and sealed metadata. The actual daemon
proof uses a content-constructed root and prepared actor installs: six successful
installs and six definite failures preserve removed directory attributes and an
inherited symlink target, then automatic terminal cleanup follows last release.
These are scoped correctness/work diagnostics, not FUSE, latency, RSS or physical
headroom qualification. Per-Workspace/whole-system buffer and kernel custody remain
later integration work; a copied capability does not acquire a reference.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.

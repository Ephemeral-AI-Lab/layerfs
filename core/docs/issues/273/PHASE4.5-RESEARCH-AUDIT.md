# #273 phase 4.5 research and source audit

> **Status:** Source-derived design review at `79eca5ddd1b8d6eb1f84a53f5e2486a83bac0d27`;
> not benchmark evidence or an implemented v2 contract.

Three independent subagents audited the ordinary mounted WRITE path,
architecture, resource bounds and concurrency ownership. They performed
read-only source research; no build/test/performance sample ran for this
audit. The [implementation proposal](PHASE4.5-IMPLEMENTATION-SPEC.md) is the
prospective authority for #273 hot backing; historical observations remain
in the [checkpoint-4 log](CHECKPOINT4-LOG.md).

## 1. Findings that determine the design

| Finding | Source at reviewed identity | Consequence |
| --- | --- | --- |
| Child stores exact maximum and physical PageRef | [keyed.rs](../../../crates/layerfs-workspace/src/backing/active/keyed.rs), `Child`/codec | Caching a leaf still copies ancestors; selected indirection/fixed fences required for structural target |
| Four I walks around one tiny WRITE | [original.rs](../../../crates/layerfs-workspace/src/filesystem/original.rs), [active_file.rs](../../../crates/layerfs-workspace/src/filesystem/active_file.rs), [generation.rs](../../../crates/layerfs-workspace/src/backing/active/generation.rs), `inode_updates` | Validate selected cursor facts and return acknowledged attrs; specialize end to end |
| D, E, shared P, R/L owner state also changes | [extents.rs](../../../crates/layerfs-workspace/src/backing/active/extents.rs), [generation.rs](../../../crates/layerfs-workspace/src/backing/active/generation.rs) | One candidate covers all affected families; E-only shortcut insufficient |
| Each node read decodes; prepare re-walks height and rereads replaced births | [index.rs](../../../crates/layerfs-workspace/src/backing/active/index.rs), `node`/`height`/publication | Charged selected cache and carried height/birth facts; count actual reads beyond prepare |
| WRITE scans resident Nodes for one attr update | [active_file.rs](../../../crates/layerfs-workspace/src/filesystem/active_file.rs), final publication | Reuse `node_index`/validated retained slot; otherwise O(V) CPU remains |
| Index maintain twice per WRITE; pinned retired prefix repeatedly scanned | [index.rs](../../../crates/layerfs-workspace/src/backing/active/index.rs), `maintain`; [generation.rs](../../../crates/layerfs-workspace/src/backing/active/generation.rs), `write_with` reached by `write_tiny_file` | Selecting-pin cohorts/zero-owner events; count inspections, not only page fetches |
| PageStore is charged BTreeMap | [pages.rs](../../../crates/layerfs-workspace/src/backing/active/pages.rs) | O(log A) custody CPU and O(A) RAM remain; full CPU O(1) needs extra work |
| Tiny FUSE input first creates p-* | [adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs), `write`; [payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs) | Count constant temporary input floor; borrowed-byte API deferred |
| #248 edits inside inherited content | [registered v1 cases](ACTIVE-FORMAT-AND-EVALUATION-v1.md#registered-public-evaluation) | Track Base/Zero suffix/source offset and two partitions, not EOF alone |
| floor assumes one preceding child suffices | [index.rs](../../../crates/layerfs-workspace/src/backing/active/index.rs), predecessor lookup | New fixed fences/deletion need earlier-partition search or empty-child pruning |
| Commit materializes final extents | [commit/active.rs](../../../crates/layerfs-workspace/src/commit/active.rs), `scan_extents` | O(E_f) scratch remains; cache is not constant total RAM |
| Cumulative reconcile maps/early lifetime clones not fully precharged | [commit/active.rs](../../../crates/layerfs-workspace/src/commit/active.rs), `reconcile`; [lifetime.rs](../../../crates/layerfs-workspace/src/backing/active/lifetime.rs), `publish_reconcile` | Reserve cumulative entries/old+new overlap before allocation |
| Reconcile holds state lock over changed extents/index preparation | [commit/active.rs](../../../crates/layerfs-workspace/src/commit/active.rs), `reconcile` | Prepare frozen facts off gate where valid; retain affected-work cost; no size-independent callback claim |

## 2. Before-#273 source audit

The exact frozen control `48b51e874a41b3e1e6c6661e145316df8b408f07`
already splices path-locally. Its `filesystem/write.rs` creates RootOwner/
payload custody, invokes length-indexed `metadata_pieces::replace`, then
updates/seals keyed I/D. `backing/binary_plus_tree/extent/splice.rs` shares
untouched children without descending. Keyed update copies reached paths
and decodes/sorts ownership edge lists. Routine old-root release retires
unheld old payloads; it does not retain all historical same-byte writes.

For old extent/keyed heights H_e/H_k, K affected extents, and copied-page
edge counts F_p, a qualified WRITE bound is:

```text
O(H_e + H_k + K + input_bytes)
  + O(sum over copied pages of F_p*log F_p)
  + actual ownership-ledger / reclamation work
```

With fixed page capacity, narrow edits visit O(H_e+H_k) pages. Separated or
EOF writes produce Theta(W) final Local records, giving approximately
O(W*log_B W) structural work. Repeating one-byte replacement at the same
location has constant final spans and O(W) path work for fixed namespace.
For the narrow separated/append workload, source does not show a per-WRITE
whole-tree scan or support a sustained O(W^2) claim. Arbitrary overlap still
pays K-dependent work.

The frozen control's `commit/upload.rs`, `commit/source.rs` and extent
cursor stream one descriptor/replacement with an eight-level frame stack,
one decoded leaf and fixed I/O window. Its per-file Workspace upload scratch
is O(h+fixed leaf/window). Checkpoint 4 instead materializes O(E_f) extents
and 24*E_f descriptor bytes. This is a RAM scaling regression retained and
qualified in this hot-WRITE scope, distinct from C1/Store work or the growing
physical/payload registries.

Tiny old allocations reserve 4,096 bytes each. Separated live slots retain
that charge: `4,096*L + metadata/ownership + pins/candidate/cleanup slack`.
Repeated overwrite removes the prior Local owner; after maintenance with
no pins, retained tiny payload space can be O(1). Packing changes the one-byte
payload term to `4,096*ceil(L/80)`, plus active index/dead-slot/retention
overhead. It reduces a constant while separated-write space stays linear.

Historical 18,751,488 B reported charge came from product
`933b3457c916519f458137e4c4f19662c8e9228e`, not a physical sample of the frozen
control. It decomposes as 16,777,216 payload + 1,974,272 metadata bytes.
That receipt is a point-in-run charged counter, not independent st_blocks;
16 product files changed by `48b51e874`. Do not present it and later
checkpoint-4 physical counts as a matched source-pinned speed/space pair.

## 3. Resource wording and main-lane ownership

At the reviewed source, `NODE_LIMIT=256` is a charged allocation chunk in
[runtime/state.rs](../../../crates/layerfs-workspace/src/runtime/state.rs),
not a resident count cap. Active reconcile returns before the legacy
128-successor-dirty check in
[commit/reconcile.rs](../../../crates/layerfs-workspace/src/commit/reconcile.rs).
Active dirty/name frontier admission uses the Host budget. The v1 prospective
wording does not prove enforcement of either count.

The main lane's [#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264)
owns component-relative mounted namespace and charged retained ancestry.
Its [design at ef3a31048](https://github.com/Ephemeral-AI-Lab/layerfs/blob/ef3a31048/core/docs/issues/245/PHASE4_5_IDENTITY_RELATIVE_NAMESPACE.md)
already identifies the node-chunk distinction. Its
[corrected implementation at c51b5c982](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c51b5c982/core/docs/issues/245/PHASE4_5_IMPLEMENTATION.md)
removes rename/remove resident scans and preserves independent namespace
counts. Its posted scope leaves 128 handles, private level/slot, span/C1
ceilings and C1 full-base/effective-subtree validation as separate work.
No posted receipt there proves a broader 128-dirty resolution; inspect what
actually lands under the owner's main-lane resource assignment.

**Owner merge order:** complete #273 first, then merge the main lane.
Do not duplicate #264 or block this lane on integrating it. Later integration
overlaps active namespace callers, `original.rs`, Node state, Budget and
capture/reconcile. Stable inode/view identities support independent hot-path
development; importing only legacy #264 callers would not prove active
caller ancestry correctness. Run affected combined proofs at integration.

Actual current caps include 128 handles, 32 captures, <=160 selecting
revisions and eight index levels. Default Host Budget is 8 MiB. v1's
64-index/8-pack cache is still prospective. Raw 288 KiB excludes decoded
allocations, cursors, physical registry, retirement and operation scratch;
RSS/cgroup/file-cache domains require their own evidence.

## 4. Alternatives reviewed

| Alternative | Assessment |
| --- | --- |
| Cache leaf/spine only | Reduces reads; physical refs/exact maxima still copy ancestors. Insufficient for structural target. |
| Fixed batch then ordinary immutable splice | O(W*h/B) for fixed batch B. Cannot move its paid work into capture/Commit. |
| In-place acknowledged pages | Breaks frozen selections/failure atomicity. Rejected. |
| Separate prefix/frontier forests and locator/inode tables | More roots/resolvers/normalization protocols than a bounded selected directory. Rejected for this scope. |
| Tagged HotRef + fixed fences + selected directory | Selected: one ordered resolver/family pool; replacing physical leaf leaves ancestors unchanged. |
| Vec physical epoch slots | Plausible stdlib constant lookup, but requires reuse/high-water/collision/custody proof. Defer; report O(log A). |
| Full streaming Commit | Separate from hot WRITE. Close reservation gap and retain explicit O(E_f) scratch. |
| Normalize all hot nodes at Commit | Adds unrelated work and violates incremental owner rule. Rejected; affected closure only. |

## 5. Safety and amortization

The directory is a versioned resolver with exact selected ownership. Current
Hot nodes obey reachable ancestor closure. Old views use old directories.
I/E/P/R leaves and directory publish together; shared cursors cannot publish
competing copies. Hot epoch mismatch refuses, with no guessed Cold fallback.

Selective Hot-to-Cold normalization keeps the child physical page selected;
removing its directory entry does not justify retiring/unlinking that page.
Ordinary unpinned cleanup remains triggering WRITE work before its typed
outcome/reply; pinned release work belongs to the final-pin event.

Regular pack rollover must advance admitted P/R frontiers. Classifying each
new logical pack page as O(h) cold admission would leave O(W*h/80) for one-
byte writes and invalidate the sequence structural target. Count its carries.

Concrete split counterexample: full-left/short-right can place the advancing
edit before a cold suffix in a full left page, forcing another immediate
split. Prove balanced byte occupancy/max record width and both Base-suffix
directions before summing geometric carry costs.

Selecting cohorts eliminate repeated unrelated retired sweeps. Owners may
migrate through multiple cohorts. Failed unlink when a cohort disappears
must retain explicit charged custody. Removing a descriptor is not a refund.

The proposal qualifies constant structural hot operations with admission,
generic overlap, compaction, pin-release and custody terms. It establishes
no full CPU O(1), constant total Commit RAM, unlimited scale under 8 MiB,
zero kernel blocking, or measured speedup.

## 6. Process continuity and future concurrency

The owner requires running processes and incremental backing through Commit.
Existing native Stage proof blocks SaveFile while G2 edits/read succeed,
then checks G1 and continuing G2/alias bytes. Extend it to a mounted process
spanning Commit. Preserve exact views/held handles/custody without process
pause, Exec drain, remount, restart or full backing reconstruction.

Concurrent Exec is separate. Current control/SDK session serialization,
daemon process-group/cancel ownership, metadata consultation, projection
permit/reply ordering, one Host submission, Store write admission, branch
conflicts and shutdown drains need a future design. Changing one mutex does
not implement it. Hot keys must not depend on Exec identity; permits remain
held through reply, notifications outside locks, and Commit uses no Exec-drain
shortcut. Ordinary filesystem serialization remains explicitly accounted.

## 7. Primary research and limitations

- Driscoll, Sarnak, Sleator and Tarjan,
  [Making Data Structures Persistent (1989)](https://www.cs.cmu.edu/~sleator/papers/Persistence.htm):
  techniques retain old linked-structure versions with controlled overhead.
  They support explicit version selection, not LayerFS quota, authentication,
  failure atomicity or selecting-pin release proofs.
- Hinze and Paterson,
  [Finger trees: a simple general-purpose data structure (2006)](https://www.staff.city.ac.uk/~ross/papers/FingerTree.html):
  persistent end operations can be amortized constant. The authors' correction
  explains the role of laziness. It does not prove an eager disk B-tree;
  LayerFS requires its own byte occupancy, carry, publication and count proof.

No dependency from these papers is proposed. Reuse Rust containers, Budget,
physical custody, verified I/O and native Service. Prospective resource
numbers are targets to prove, not benchmark observations.

## 8. Final draft review corrections

The three reviewers checked the prospective draft independently. Corrections
include regular P/R rollover without cold search, cleanup before typed reply,
preserved physical birth/ownership on Hot/Cold conversion, nonempty indexed
partitions, a positive single-inode hot support profile, post-Commit hot
counts, explicit upload RAM regression and checkpoint-5-before-main-merge
sequencing. Conservative candidate bounds are <=98 for split-only and <=227
for a restricted merged admission/normalization event; their exclusions,
memory/slot precharging and two-pages-per-node proof obligations are explicit.
This review closes the proposal audit, not any implementation/proof gate.

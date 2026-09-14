# #130 minimality review of our own proposal

Read-only adversarial review, 2026-09-14. Basis: pinned `issue130.md`, the parent review `README.md`, and product HEAD `8d6352128248d2598af20641127371f983d66a4e`. Other agents are editing shared source; this report changes no product file, issue or shared document and runs no build, benchmark or resource-sensitive test. The `ponytail-review` skill was applied for complexity findings. Correctness conditions below constrain possible simplifications; this is not a substitute for the independent rules/correctness audit.

## Verdict

**#130 is directionally compatible with rule §11, but the full proposal is not established as the smallest implementation.** It explicitly preserves capture, ownership, mmap, publication and prior repairs; it also permits dropping exploratory representation ideas (`issue130.md:114–117`). That is appropriate. However, its supporting README promotes a collection of optional mechanisms into mandatory execution steps. Lazy construction, a resident/spill pager, byte-aware trees, virtual lower identities, primary sequence summaries, zero/one/many aliases and compact tiny/whole sources are several independently substantial changes. Their combination is an architecture menu, not evidence that every mechanism is needed.

Rule §11 states: “Choose the smallest implementation satisfying these rules” (`overlay-snapshot-rule.md:385`). The smallest *first cut* should remove demonstrated eager work through existing interfaces and then evaluate the remaining cost. It cannot be claimed to complete the fresh-lifetime objective until the applicable evidence shows that; nor should the full architecture menu become an extra requirement if fewer changes suffice.

## Findings in ponytail-review format

Paths without a prefix refer to the pinned files in this review directory or its parent. Line numbers were read directly; no implementation line savings are inferred from prose.

- `../README.md:L578: yagni:` remove the unconditional shared resident/spill pager from the first-cut checklist; keep the existing disk Index and make a pager a subsequent, separately justified extension only if remaining D1/D10 metadata I/O requires it. A resident/disk-location switch introduces eviction receipts, dirty owner-cache state, read-pin/eviction races and aggregate cache scheduling; the 106-byte correspondence and unchanged-candidate shortcuts need none of them.
- `../README.md:L576: yagni:` remove mandatory tiny-payload conversion from the first compact-record change; keep ordinary Payload tokens and the repaired interval allocator until tiny-create/append evidence identifies the remaining cost. Tiny bytes require source-lifetime, storage-threshold promotion, append/join, quota-class and old-reader behavior; a singleton correspondence needs only a bounded tagged value. The issue itself allows less invasive selection at `issue130.md:114–117`, so the README should not overrule that choice.
- `issue130.md:L99: delete:` delete dual-change-index replacement from the required first-cut inventory, retaining it only as a later alternative; use the current captured latest-change range scan. `issue130.md:213–238` already explains that the existing indexes support changed-only C2. Sequence summaries replace two rows but add new subtree fields, deletion markers, retirement discovery and a different cursor order; their lower record count does not establish lower total implementation cost.
- `../README.md:L115: yagni:` defer a second canonical-ID representation and legacy adapter; retain the exact bounded canonical-to-live mapping and make the existing base-file record compact first. Full lower-ID virtualization adds root/scope/allocation-domain cases and hidden-hardlink handling. At current `host_overlay.rs:244–254`, lookup already checks the existing canonical map before allocating identity; the expensive range materialization at `:168–175` can be reduced independently.
- `issue130.md:L101: shrink:` split “cookie, alias and source” optimization into independent decisions; cookie-in-binding is a bounded row merge, whereas zero/one/many aliases and compact sources each require promotion state machines. Retain the current reverse-alias and interval indexes initially. Combining all three under one inventory-reduction step conceals their different complexity and proof obligations.
- `../README.md:L140: yagni:` if a pager is later selected, extend the existing Index PageId/catalog/root owner rather than add a second upper-map/pager abstraction; reuse its root tickets, exact header receipts, bounded reclaim and physical relocation. A separate RAM map converted into a disk index, new generic storage trait family, or second ownership registry adds mechanisms without supplying new semantics and would make capture/spill harder to verify.
- `../README.md:L582: delete:` remove “integrate default public dispatch, prove supported-surface capture and C1/C2” as new #130 implementation work; treat these as already delivered #124/#125 prerequisites that selected optimizations must preserve and reverify only when affected. #130's pinned scheduling at `issue130.md:5–16` starts after their verified closure. Requiring the same unfinished migration again is stale scope, not a simplification.
- `issue130.md:L273: shrink:` keep continuation requirements as preservation criteria linked to their authoritative rules and retained proof identities, rather than another independent implementation specification; the preceding sparse-root explanation and the governing documents already define the same live/snapshot/Commit behavior. Keep the necessary checks and counterexamples; reduce duplicate normative text that can drift.

These findings do **not** recommend removing V1, a retained-stage receipt, a regression test, a quota check, or source custody. They distinguish preservation obligations from implementing an unnecessary second mechanism.

## The smallest recommended first cut

This sequence applies only after #130's declared prerequisite campaign has actually finished. Reinspect the delivered product first; source below is a current seam, not a promise that it will remain unchanged then.

### 1. Remove empty-path construction and unused-resource work

Use the existing owned snapshot and coordinator/publication path. The narrow first no-op condition is a **valid supported-surface capture with captured logical sequence equal to the predecessor's covered sequence**, plus the normal matching canonical/branch context. It is not arbitrary “equal bytes” after dirty edits: new origins can require new correspondence even when the Store outcome is UpToDate. Let the same coordinator perform expected-head/base validation, stage/publication and authoritative receipt resolution; do not create a second no-op publication protocol.

Current `snapshot_candidate.rs:393–428` captures changes and acquires construction capacity/file plans even when no new changes exist. Current `commit_attempt.rs:177–210` already checks candidate coverage/correspondence and routes through `WorkspacePublicationAttempt` and `commit_workspace_candidate_retained`. The smaller implementation supplies a valid unchanged prepared candidate through that route, with the normal ownership needed by staging, instead of teaching every lifecycle layer a shortcut.

Defer the **unused Payload arenas/private index** and empty construction journals until actual use. Keep their existing constructors and error semantics behind a small lazy owner; no new factory framework is needed. Resource admission precedes file creation/growth and remains held by surviving readers/snapshots. A root release ticket must reserve its eventual queue space before returning an owned root; lazy allocation cannot make Drop fallible.

**Do not promise zero private FDs for Begin from this change alone.** `HostOverlay::new` imports the root, and the current main Index writes it. `overlay_index.rs:303–304` creates its data/catalog files; keeping that backend means keeping that root-index cost. Root/base-view adaptation or a resident pager is additional work. The lower-complexity first cut can remove unused payload/journal resources while retaining the main root Index.

Similarly, do not delete `ff7098929`'s reader-cache charge. If the actual cache and owner tables remain preallocated/reserved, they must remain charged. Demand-sized admission is useful only when actual capacity grows under the same bound; changing the reported reservation alone is not an optimization.

### 2. Inline singleton correspondence; reduce simple range construction without rewriting the tree

The clearest representation win is the 106-byte singleton correspondence inside the existing parent row. Current `correspondence.rs:283–297` creates a header tree with links to offset/origin trees. Use a bounded tagged singleton value through the same Description read/planning interface, and retain the general indexed description for fragmented inputs. Avoid first building both singleton index trees and only compressing them afterward; that would save retained pages but keep the construction I/O.

For range nodes, first remove repeated private path construction: `overlay_ranges.rs:588–606` sequentially installs META, PROVENANCE, optional child links and BACKING into a new tiny root. A narrow bounded leaf-construction/batch primitive in the existing Index can prepare these entries and their ownership once. This does not require changing range traversal or the host protocol. It saves intermediate pages; it does **not** remove the final page-per-range cost, and should be described honestly.

Next choose one compact base/singleton range representation if the remaining cost warrants it. Reuse existing compact-shape predicates and cursor/planner semantics, not the old frozen Workspace or its unbounded resident maps. Merely embedding a new range in the existing 128-byte inode and overflowing to a private page can erase the expected space benefit; review its actual encoded size before claiming success. Do not simultaneously replace the entire treap, redesign the allocator, and add a storage hierarchy to obtain the singleton win.

### 3. Evaluate the residual cost before selecting another mechanism

Use existing applicable clean Commit, tiny-create, read and localized-edit boundaries with a fresh identity, existing correctness evidence and the measurement rules. D0/D1/D10 distinguish fixed work from changed-state work; they do not invent a new numerical gate. Retain unaffected proofs from the prerequisite campaign. A single green component test does not establish that #130 is complete.

If tree density is the remaining cost, byte-aware packed pages plus bounded multi-key private preparation are a direct continuation of the existing Index. This adds split/merge and ownership-bound verification but not an entirely new pager. If repeated uncached header/body I/O remains the cost, consider a bounded cache or pager within that same Index. A disk-backed immutable read cache is a smaller starting point than changing every page to be initially RAM-only; it cannot eliminate write I/O, so measure its actual contribution instead of presenting it as equivalent.

If tiny arena padding remains important, select exactly one owned-token small-storage mechanism and preserve the general interval path on promotion. Do not simultaneously implement inline token bytes, shared-block liveness, compact whole sources and source-ID virtualization as independent alternatives. Their source-tag/append/ownership interactions must be proven for the mechanism actually selected.

## Necessary optimization versus optional machinery

| Proposal | Judgment for the first cut | Existing mechanism to keep/reuse |
|---|---|---|
| Fresh identity with existing Store/daemon/container service reuse | Preserve; no mutable Workspace pool | Existing runtime/service ownership and mount route |
| Unchanged-candidate construction bypass | Select narrow sequence-equals-coverage case | Existing capture, coordinator, stage and V4 receipts |
| Lazy unused Payload/journal allocation | Select where there is no owned input yet | Existing constructors, admission permits, cleanup |
| 106-byte singleton correspondence | Select | Existing Description/planner and parent Index value |
| One bounded prepared leaf instead of 3–5 sets | Select narrow implementation if not already delivered | Existing Index allocation/ownership transaction |
| Compact base/singleton range | Next direct target; verify its actual encoded layout | Existing compact shape logic and read/planner interface |
| Byte-aware pages | Direct structural option after the singleton changes | Current Index, roots, catalog, reclaim |
| RAM-first shared pager | Conditional substantial extension | Current PageId owner and catalog; no second map/backend |
| Dual change-index removal | Defer; current scan already meets required locality | Current latest-change indexes and covered cleanup |
| Cookie embedded in binding | Optional bounded row merge, separate from alias work | Keep numeric cookie order and state |
| Zero/one/many aliases | Defer until exact benefit is shown | Existing reverse map and canonical mapping |
| Lower-ID virtualization | Defer | Existing collision-free bounded mapping; compact base value |
| Tiny/shared/whole-source promotions | Choose one later if needed, not a required bundle | Repaired Payload token/interval/append/reclamation contracts |

## Things the minimality pass rejects as false simplifications

- Reverting the resident-token or catalog-reclamation repair to avoid bookkeeping. That restores an observed defect, not a lean implementation.
- A host-only generation shortcut before valid kernel-visible capture. This is not a valid no-op proof for dirty mmap.
- An unbounded resident map, whole-map conversion at a cache threshold, first-write clone after snapshot, or one epoch retaining every later update. Fewer lines do not satisfy the ownership/capture contracts.
- Reusing one mutable Workspace identity, implicit fsync, writer finish, remount, or checkpoint reset. These change the requested semantics.
- Removing dual change indexes without replacing deletion/coverage discovery, or dropping alias/cookie maps without preserving the corresponding operations. Deletion of a structure alone is not a complexity reduction if it leaves required behavior unimplemented.
- Calling `mmap`, a new database, or platform snapshots a native replacement without proving this repository's exact owned-root and generic FUSE contracts. No such proof follows from the native feature's name.

## Evidence and conclusion

This is a review of our proposal, not an implementation benchmark. The pinned issue's conditional language and rule-preservation clauses are sound design intent. The smallest implementation has not been demonstrated because selected changes have not yet been implemented and measured under the prerequisite/custody contract. The README should reflect the same optionality as the issue and should not prescribe every explored state machine.

Use the small cut above, preserve all mandatory #124/#125 outcomes, then add only a mechanism justified by the remaining work and its measured cost. A shared pager may eventually be necessary to approach the desired cost shape; present it as a proved next choice when that evidence exists, not as the definition of “minimal.”

`net: not quantified — this is an unimplemented proposal, not a code diff; no lines-saved measure is invented.`

# #190 — version comparison corrections and improvement priorities

> Status: Research; informative and not a product contract.

Follow-up source inspection for the owner's diagram request. No product changes or performance runs. Historical source is `7fab1027a`; current product source is `9f35c49ad`. `git diff 7fab1027a HEAD` is empty for legacy `crates/layerfs-content/src/tree/batch.rs` and `crates/layerfs-workspace/src/changes.rs`, so the local legacy line references below address the recorded historical implementation.

## Shared behavior must not be drawn as a new regression

Legacy `crates/layerfs-content/src/tree/batch.rs:680–710` fetches and checks every immediate child of an entered branch before recursively calling edit. Its unchanged-subtree shortcut is at line 609. Replacement `core/crates/layerfs-content/src/filesystem/sorted/merge.rs:239–278` has the same order, with shortcut at line 180. **Sibling acquisition before pruning is present in both versions.** Its existence in the replacement is not evidence that this behavior was introduced by the replacement or explains the cross-generation slowdown. Input shape, tree shape, validation, provider cost and repeated demands could change its effect; those differences remain unmeasured.

## Legacy namespace timer is not all legacy tree work

Legacy `changes.rs:742` iterates dirty workspace nodes; `:791` updates directory content inside that loop. The enclosing loop is charged to Content at line 850. Namespace at lines 851–877 covers subsequent reference application and inode completion. Therefore old `namespace_ns = 342,355,542` cannot be directly compared with the core's full build/update envelope `23,520,347,667` as a tree-algorithm slowdown ratio.

Legacy uses a bounded FrontierInodes pending/spill representation: `changes.rs:2486–2496` consults pending and spilled values before base reads; `:3093–3107` batches reference base lookups; `:3025` applies sorted inode deltas. It also has singleton base lookups (`:2508–2514`), so drawing all legacy lookup work as perfectly batched would be false. Core likewise already batches validation prefetch (`filesystem/validate.rs:462–485`) and reference lookups; its per-directory `lookup_base` callers remain singleton paths (`filesystem/update.rs:203,214,288,441–450`).

## Proposed improvements, not measured fixes

1. First isolate validation/directory/reference/inode time and count physical read waves/pages separately from logical demands. No projected seconds saved.
2. Batch the existing singleton directory-parent demands, deduplicate within the declared batch bound, and retain the authenticated base records through their later reuse in the same immutable-base operation. Use existing lookup_many. Any cross-phase retention must remain within the resource contract; a cache cannot silently bypass visibility or validation. Assert equivalent roots/errors plus reduced wave/page counts on a fixed same-parent/shared-ancestor case.
3. Reuse validation work only when the exact base root, effective edits and validation assumptions match. Per-start cycle checking cannot simply be deleted or combined without preserving cycle/alias detection.
4. Consider lazy untouched-subtree summaries only after the trust/format prerequisites are explicit. Current branch WireEntry carries key, child ID and optional leaf value (`sorted/format.rs:65–71`); Node::existing needs child count/bytes/size/items (`sorted/page.rs:135–143`). The engine validates parent-child summaries and recomputes parent totals (`sorted/merge.rs:266,296–297`). Moving the shortcut above the read is not a valid one-line optimization. A persistent summary representation could require a format/contract ruling; an operation-local memo helps only where authenticated facts have already been acquired within the timed operation.
5. Streaming construction/admission is a later consideration. Legacy already pipelines file output into admission, while this harness first constructs objects and then saves. No additional construction workers are proposed, no cache warming or shifting product work out of timers is permitted, and stream dependency/order/read visibility must be proved before such a change.

The strongest low-scope candidate is bounded batching/reuse of repeated base-record demands. The strongest justified current claim remains the measured tree envelope, not a quantified regression mechanism. The original investigation and manifests remain archival snapshots; this addendum preserves new comparative findings explicitly.

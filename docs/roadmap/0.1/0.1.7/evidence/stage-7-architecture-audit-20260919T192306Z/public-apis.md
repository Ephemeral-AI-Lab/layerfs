# Stage 7 source audit: C1/C2 public APIs and independent replacement

> **Status:** Research; informative and not a product contract.

Reviewed frozen source snapshot `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z`, manifest SHA-256 `c2f6034c534853919833c9f59758a17e74ea6331f41835b4267adf883bb74ff8`, captured against HEAD `66bce8378`. This is a source review of that snapshot, including captured working changes; it is not a qualification of subsequent Stage 6 edits. References below are repository-relative and line numbers refer to that snapshot. No build, test, benchmark, swap experiment, or repository write was performed.

## Verdict

**The existing boundaries support integration design, but independent revision substitution is UNPROVEN.** C1 accepts authenticated canonical-object providers and finalized-object consumers and does not depend on C2. C2 provides the concrete `StoreProvider` and `SaveHandoff` bridges. Public functions can remain unchanged while chunk search, tree navigation, delta candidate selection, batching and packing implementations change, provided canonical and persisted-format contracts remain compatible. No plugin registry, dynamic loader, or extra trait hierarchy is needed to get this property.

C1-only substitution is structurally feasible, but C2 and the integration must resolve exactly the same C1 package instance and compatible canonical grammar. C2-only substitution is structurally feasible for callers using the Store facade, but persisted database/pack compatibility must be checked independently. Swapping both together is the easiest source composition, not proof that each can be replaced independently. Actual baseline/C1-only/C2-only/combined unchanged-consumer runs remain NOT_RUN.

## API map for co-design pair #179

Recommended integration surface means the smallest existing surface to depend on, not a claim that other `pub` symbols are inaccessible. Import `layerfs_content` for logical content/filesystem work, `layerfs_storage` for physical persistence, and `layerfs_telemetry::timer` for scopes.

| Caller job | Existing public route | What the caller owns / result |
|---|---|---|
| Open/create the object store | `Store::create(path, StoragePolicy, TimingScope) -> StorageResult<Store>`; `Store::open(path, TimingScope)` | C2 owns physical encoding, SQL and visibility. Creation does not migrate an existing Store. `store.policy()` supplies its persisted construction policy via `.construction()`; `store.capacities()` declares C2 bounds. |
| Construct a complete file | `construct_bytes(policy, &capacities, &[u8], &mut dyn FinalizedConsumer, scope)` or `construct_stream<R: Read>(policy, &capacities, source, consumer, scope)` | C1 selects canonical file representation and emits child objects before their root; returns `ConstructedFile { root, logical_len, counters }`. Source must be stable. `construct_stream` bounds its cutoff probe; caller need not collect the full file. |
| Offer prior-version correspondence during full reconstruction | `construct_bytes_with_predecessor(..., Option<PredecessorBase>, consumer, scope)`; `PredecessorBase::new(&provider, earlier_root)` | Caller supplies an actual known earlier root. Optional hint preserves canonical output; C1 walks mapping correspondence. This API supports byte slices, not the generic streaming entry point. Whole-file/cross-role predecessor cases are explicitly limited. |
| Apply known file edits | `apply_edits(policy, &capacities, &provider, EditRequest { root, edits: &EditStream, source: &dyn EditSource }, &mut consumer, scope)` | Caller supplies final edit sequence and replacement bytes. `EditStream::new(base_len, Vec<Edit>)` validates **current-result coordinates** and bounds. `Replacements` is a convenience in-memory source; implement `EditSource` for bounded external payload access. No-op preserves base ID; construction success is not save success. |
| Read file bytes | `read_range(&provider, root, Range<u64>, &mut dyn Write, scope)`; `read_all_bounded(&provider, root, maximum, sink, scope)` | Reads logical bytes without inspecting chunk or pack layout. For repeated access to one immutable file, `FileView::open` holds its classified root and exposes a range read. Sink ownership/backpressure stays with caller. |
| Resolve/stat/list/readlink filesystem paths | `FilesystemRead::new(&provider, FilesystemRootId)` then `.resolve`, `.stat`, `.list(path, after, max_entries, max_bytes)`, `.lookup_names`, `.lookup_inodes`, `.readlink`, `.read_portable`, `.read_attribute`, `.attribute_keys` | C1 owns canonical path/tree interpretation. Resolve returns an inode value; regular-file `content_root` then feeds `read_range`. `PathNotFound` differs from absent physical objects and provider failure. |
| Build/update a filesystem | `FilesystemObjects::new(&provider, &mut consumer)` + `build_filesystem` / `update_filesystem` (`*_timed` variants under `layerfs_content::filesystem`) | `FilesystemInput` carries optional base, allocation scope, root serial, sorted final directory bindings, sorted final inode values, fresh serial declarations and resource budget. Caller (#179/#180) owns stable inode allocation and change accumulation; C1 derives reference counts. |
| Attributes/symlinks | `layerfs_content::filesystem::attributes::{emit_value, build_attribute_tree, apply_patches, AttributeKey, AttributePatch, PortableMetadata}`; `filesystem::symlink::{SymlinkTarget, emit_symlink}` | These emit finalized canonical objects through `FilesystemObjects`. Inode values carry resulting metadata/content roots. |
| Give filesystem reference ordering scratch | `filesystem::references::{OrderingBacking, OrderingRun, FileBacking}`; `FileBacking::with_capacity(directory, bytes)` | Optional explicit resource capability, separate from CAS storage; operations fail if declared memory/resources cannot cover their work. Backing release is checked before operation success. C1 includes this concrete file-backed utility, so broad “C1 never opens any file” crate prose is inaccurate for this opt-in implementation. |
| Persist C1 output | `store.begin_save(scope) -> SaveOperation`; `SaveHandoff::new(&mut save)` implements `FinalizedConsumer`; after C1 returns inspect `handoff.take_failure()`, then `save.finish(scope) -> SaveOutcome` | Original C2 failure is retained behind `ContentError::OutputRejected`; do not discard it. `accept` transfers ownership and may flush a bounded wave; only `finish` acknowledges publication. Explicit `abort(scope)` reports cleanup, dropping unfinished save attempts best-effort cleanup. |
| Read/presence-check canonical objects directly | `store.read_batch(&[ObjectId], scope)` and `store.contains(&[ObjectId], scope)` | For consumers that need canonical objects, not logical file interpretation. Returned bytes are authenticated; batch cardinality/order and capacity limits matter. |
| Present Store reads to C1 | `StoreProvider::new(&store)` implements `AuthenticatedObjects` | Keep one provider per logical operation to reuse its connection/decode session; `StoreProvider` is explicitly `!Sync`. Store itself is shared/Sync. Provider rereads publication ceiling per wave; it is not a pinned multi-wave snapshot API. |
| Read objects accepted inside the current unfinished save | `SaveOperation::read_batch(&mut self, ids, scope)` | Includes pending accepted objects and seals demanded unfinished groups as necessary; these are not visible to `StoreProvider` until finish. There is no ready public same-save C1 provider bridge. |

Primary definitions: `layerfs-content/src/lib.rs:21`, `file/content.rs:207`, `file/edit/apply.rs:28`, `file/read.rs:19`, `file/view.rs:31`, `filesystem/read.rs:74`, `filesystem/update.rs:67`, `filesystem/input.rs:152`, `filesystem/objects.rs:44`; `layerfs-storage/src/cas/store.rs:174,279,300,344,383,424,451,579,619` and `cas/provider.rs:64` (all under `core/crates/`).

## Real boundary and call sequence

```
#179 accumulator + stable bytes/edits + authorized immutable roots
  -> C1 construct/edit/filesystem APIs
      reads -> AuthenticatedObjects -> StoreProvider -> C2 authenticated read
      output -> FinalizedConsumer -> SaveHandoff -> SaveOperation::accept
  -> check retained handoff error / C1 result
  -> SaveOperation::finish -> SaveOutcome
  -> external owner publishes/records its filesystem or history root
  -> Store::open -> StoreProvider -> C1 logical read
```

C2 saving objects does not install a Workspace branch head, history record, mount state, or authorization decision. Pair #180 and #181 retain those responsibilities. `AuthenticatedObjects` promises digest identity; the word authenticated here is not a tenant authorization check. Finalized output owns bytes/role/direct references and bounded optional predecessor hints. Direct dependencies must be available to C2 in the operation's eligible stored/current batch state; use C1's normal child-first emission instead of manually reshuffling objects. Do not publish the new root externally before successful `finish`.

Source: `object/access.rs:27`, `object/output.rs:110,213`, `cas/dependencies.rs:112`, `cas/store.rs:424,619`, `error.rs:1`.

**Borrowing clarification:** `Store::begin_save(&self)` returns an owned `SaveOperation`, so `StoreProvider::new(&store)` can read a published immutable base while `SaveHandoff` exclusively borrows the new operation for writes. These are different handles with different visibility. It does not require two Stores or unsafe borrowing. Conversely, directly borrowing the same `SaveOperation` as both C1 reader and sink is not supported: same-save reads need `&mut SaveOperation`, and `SaveHandoff` already owns that mutable borrow. After handoff use ends, `.operation()` can provide explicit sequential reads, but cannot serve C1 callbacks at the same time. Pair #179 should specify whether it needs chained edits against an uncommitted C1 root; do not accidentally add a commit between edits merely to work around this ownership gap.

**This does not block the current filesystem update pipeline.** `filesystem/update.rs:149–380` validates/reads the supplied base, builds each directory against its old root, carries newly emitted directory IDs in the local `contents` map, merges typed final inode values against `base_table`, and finally emits the filesystem root. It does not reload those new directory roots through the provider. The sorted engine keeps its unfinished edge pages private (`sorted/finish.rs:1`, `sorted/merge.rs:57`), and the inode updater explicitly avoids an encode/store/reread cycle (`inode/update.rs:1`). File `apply_edits` similarly keeps intermediate mapping drafts in `EditObjects` (`file/edit/tree.rs:109,160`) until final emission. Thus an accumulator that collects final changes against one published base, calls C1 once, then finishes C2 fits the present APIs. The missing bridge matters only if #179 chooses to start a **second C1 operation using the first operation's uncommitted root**, or expose uncommitted roots to C1 logical reads. It is a design constraint pending that choice, not an established current-path correctness blocker.

Errors are distinct: missing physical object, missing logical path, provider failure, output refusal, unsupported policy, capacity refusal, writer ownership failure, cleanup failure, unknown write outcome. `UnknownOutcome` must not be retried/resubmitted; `Drop` cannot report a cleanup failure to the caller. Async cancellation and thread scheduling belong outside these synchronous APIs. `OrderingBacking::release` has a checked operation completion contract, separate from Store save completion.

## Concrete flexibility findings, ordered by impact

1. **P1 — Persisted-data substitution is not generally drop-in.** Snapshot `policy.rs:33` and `sql/schema.sql:14` select schema version **6**; `sqlite/schema.rs:28` requires `content_signatures`, and `Store::open` eagerly loads the derived candidate index (`cas/store.rs:201`). Exact schema identity/table validation refuses older stores; there is no automatic migration. This is a real example where optimization changed persisted schema while high-level Store signatures stayed unchanged. Minimal remedy: classify every candidate as algorithm-only or format-changing, publish supported revision/schema combinations, and require two-way baseline/candidate reopen tests for a drop-in claim. If upgrade is wanted, an explicit migration is a separate design. Do not weaken fail-closed validation.
2. **P2, conditional design constraint — No production same-save C1 read/write bridge.** Published-base edit and new output compose; chaining against an accepted-but-unpublished root lacks an `AuthenticatedObjects` bridge. Evidence: `StoreProvider` reads published Store state; `SaveOperation::read_batch` requires `&mut self`; `SaveHandoff` holds `&mut SaveOperation`; C1 `apply_edits`/`FilesystemObjects` require reader and consumer concurrently. Impact depends on #179's accumulator boundary, so this is an integration capability decision, not a proven bug. Minimal remedy: first pin whether #179 needs it; if it does, provide one narrowly scoped C2 session adapter implementing the existing provider/sink contracts with explicit serialization/visibility and retained errors, and an external chained-edit test. Avoid exposing SQL or inserting intermediate publication to fake support.
3. **P2 — Public exposure is much wider than the integration facade.** C2 exposes `sqlite::{connection,lookup,write,schema,pool,cleanup}`, `pack` and `encoding`; concrete connection/locator/transaction functions are callable. `StorageError::Engine(rusqlite::Error)` leaks backend type even through the facade. C1 publicly exports `PageCache`, `RangeCursor`, `ReferenceReducer`, edit/tree machinery and canonical codec types. Existing external tests intentionally use low-level symbols, e.g. `tests/visibility.rs`, `tests/physical_formats.rs`; those are not evidence #179 needs those symbols. Callers that adopt these types will enlarge required source compatibility and make optimization harder. No existing #179 integration consumer was found or claimed here; this is actual public exposure and future coupling risk, not evidence of an existing runtime dependency. Minimal remedy: designate facade versus advanced canonical/physical implementation surface now, inspect external callers before visibility changes, avoid leaking raw backend error matching into integration. Restrict implementation items only where production consumers do not need them; no new registry needed.
4. **P2 — C1-only replacement needs one Rust type identity throughout the graph.** C2 Cargo.toml has `layerfs-content = { path = "../layerfs-content" }`, while integration will also name C1. Simply pointing integration to another worktree but leaving C2's sibling path untouched creates distinct package instances and nominally distinct `ObjectId`, `FinalizedObject`, traits and errors. Minimal remedy: a reproducible assembly procedure selects one C1 source path for both C2 and caller, records lock/dependency changes, and proves unchanged caller source. This is source/dependency selection, not binary hot swapping or permission to patch third-party crates.
5. **P2 — C2 has legitimate canonical-format coupling, which must be explicit.** It imports chunk payload codecs/magic (`encoding/full.rs:11`), reconstructs chunk/whole-file canonical envelopes (`encoding/decode.rs:11`), and pools inode leaves using C1's exact leaf grammar/constants (`cas/pool_lane.rs:65`, `encoding/pool/leaf.rs:15`). These are canonical contracts, not private C1 chunking algorithm calls. C1 algorithms can change within the frozen canonical profile; new grammar/roles cannot be treated as interchangeable. Minimal remedy: document this compact format ABI inventory, preserve codec fixtures, and test old/new object reconstruction and digest equality. No need to extract a new shared crate solely for appearances.
6. **P2 — Provider failure detail and filesystem telemetry do not compose fully.** `StoreProvider` maps non-content storage failures to `ContentError::ProviderFailure { what: &'static str }` (`cas/provider.rs:21`), losing original engine/locator detail; unlike SaveHandoff it retains no typed cause. `FilesystemRead`/`FilesystemObjects` use unscoped provider methods (`filesystem/read.rs:86`, `filesystem/objects.rs:108`), while file APIs also have some unscoped acquisition sites; C2 work can therefore lack named timing children despite the available scoped bridge. Minimum remedy: decide which error context and timing depth integration requires, then retain cause / thread coarse scope through existing boundary, with one external error-classification/timing check. Do not add per-object telemetry nodes.
7. **P3 — Reader capacity agreement is implicit rather than negotiated.** `AuthenticatedObjects` advertises no capabilities; `filesystem/objects.rs:24` hardcodes 4096 demands and asserts C2 has the same bound. Current concrete pair aligns, but a replacement provider with a smaller supported batch receives refused requests. Minimal remedy: make the supported batch/resource profile an explicit integration precondition and test it; add capability negotiation only if a second real provider needs a different ceiling.

## Example wiring (source-derived, not executed)

```rust
use layerfs_content::{apply_edits, EditRequest, ObjectId};
use layerfs_storage::{SaveHandoff, StorageError, Store, StoreProvider};
use layerfs_telemetry::timer::Timing;

fn edit_and_save(store: &Store, request: EditRequest<'_>) -> Result<ObjectId, StorageError> {
    let policy = store.policy().construction();
    Timing::disabled("integration.edit", |scope| {
        let provider = StoreProvider::new(store); // reads the published base
        let mut save = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut save);
        let result = apply_edits(
            policy, &policy.capacities(), &provider, request,
            &mut handoff, scope.child("content.edit"),
        );
        if let Some(error) = handoff.take_failure() {
            return Err(error);
        }
        let root = result?.root;
        save.finish(scope.child("storage.finish"))?;
        Ok(root)
    }).0
}
```

The `?` failure path drops the unfinished save with best-effort cleanup, as current examples do; callers requiring observed abort/cleanup errors must explicitly call `abort` after the handoff borrow ends and preserve both errors. This example is not a complete runtime publication/cancellation policy.

## Existing runnable evidence and missing proof

Reuse `core/crates/layerfs-storage/tests/core_pipeline.rs:21` for streaming construction -> SaveHandoff -> finish -> StoreProvider logical read; its cases include reopen. `tests/edit_pipeline.rs` covers edit -> save -> reopen/readback, though several tests collect objects first instead of using a Store-backed provider during edit. `tests/filesystem_pipeline.rs:698` exercises integrated filesystem handoff and timing on/off, but uses the prepared in-memory bag as the base provider. `tests/visibility.rs` checks incomplete-save publication exclusion; `tests/provider_errors.rs` checks production error classification. Existing tests are candidates for the unchanged consumer proof, not a proof themselves.

Commands to run later on an isolated assembled source selection, after Stage 6 resource-sensitive work permits it:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test core_pipeline --test edit_pipeline --test filesystem_pipeline --test visibility --test provider_errors
```

Repeat only for the required baseline / C1-only candidate / C2-only candidate / combined candidate compositions, with the SAME consumer source and explicit source/dependency identities. A dedicated persisted fixture must cross revisions in both directions; ordinary same-revision test temp directories do not establish that. No current report cell is PROVEN by this review. Mark incompatible schema combinations INCOMPATIBLE; mark absent real algorithm candidate or unrun checks NOT_RUN. Follow full required core checks when any product code is changed, and separate all performance claims from these correctness/source-compatibility checks.

## Clarification appended 2026-09-20: directory input granularity

The co-design sequencing review confirmed the exact `filesystem/input.rs:3–9`
contract: a directory update supplies the final binding of **each name it changes**,
sorted and unique. It does not require the caller to resend unchanged directory
entries. Read "sorted final directory bindings" above in that changed-name sense.
The earlier conversational explanation and #179's "complete final bindings per
changed directory" wording overstated the required input. This clarification
changes neither the recorded source snapshot nor any test/measurement result.

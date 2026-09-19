# Co-design ordering review: history, trust, and acknowledgement

> **Status:** Research; informative and not a product contract.

Read-only review. Issue inputs are the frozen JSON bodies in `/tmp/layerfs-design-order.l4IJqs/`. Relevant body-file SHA-256 pins: #179 `05877b7fae78cdbc7be75729578879a49e7c36cbd7daf3fcb63a993e0a87be25`; #180 `ce211d942764b98c0fac11db78091b73260a97a7d37defa6ac8adbb96ea466f2`; #181 `dd6c497d4e4b2cd10486aad864bfa743b2e28963129e597e0de2f0f006404449`. Local source/doc references are read-only observations of the shared checkout; concurrent Stage 6 work may advance them. No builds, tests, benchmarks, repository edits or GitHub mutations performed.

## Recommendation

Do **not** execute pair numbers as a whole-issue sequence `#179 -> #180 -> #181`, or reverse that into another whole-issue chain. Keep the three design tracks open together, with a small shared boundary milestone first and specific freeze gates afterward. #181 already requires a directional tenant decision before #180 freezes its schema; that is the correct dependency size.

The proposal index currently contradicts that dependency shape: `core/docs/architecture/proposal/README.md:75-96` labels pair 1 → pair 2 → pair 3 as strict, then requires pair 3 tenancy input before pair 2 freezes. Replace the strict full-pair ordering with the milestone dependencies below; keep the useful principle that final framing consumes agreed runtime/history operations.

Stage 7 supplies the reviewed C1/C2 contract and records its outstanding replacement proof. Preparing the pair decisions can proceed without inventing completed Stage 7 evidence; implementation should depend on closure/disposition of the contract findings that affect its slice, consistent with the existing owner sequencing. Stage 6 finalization does not have to be disturbed or rerun for this design work.

## The smallest required ordering

1. **Shared boundary checkpoint — #179/#180/#181, consuming #172's review.** Write one compact decision record for: operation vocabulary and the owner of each success acknowledgement; captured immutable root plus expected logical head/generation; who allocates scope/serials; directional trust domain/tenant-to-Store routing; where C1 executes and what a logical operation carries across a link; cancellation/unknown-outcome disposition. This is a boundary agreement, not completion of three designs.
2. **Parallel design work.** #179 develops callback mapping, accumulator ownership and bounded flush/backpressure. #180 develops history identity, stage/discard and conditional publication. #181 develops authorization checks and concrete direct/stream operation framing. Each works against the agreed boundary, marking unresolved details explicitly.
3. **Freeze gates.** History schema freezes only after tenancy/identity scope and metadata placement/publication decisions; protocol freezes only after operation/acknowledgement vocabulary and auth routing; runtime flush contract freezes only after the distinction between saved state and logical Commit plus unknown-outcome behavior. Full sibling issues need not close.
4. **Implementation slices after design acceptance.** Start with a direct local root-save/readback slice without history claims, then one explicitly conditional history path, then a stream-link failure slice. Multi-writer optimization, richer reconciliation, GC and speculative tenancy mechanisms are not prerequisites for the first slice.

## What must be decided before the freezes

| Decision | Producing pair | Consumers / exact gate |
| --- | --- | --- |
| Meaning of accepted writes, flush completion, saved root, staged Commit and Add/merge | Shared: #179 defines overlay/flush; #180 defines logical history; #181 defines delivery/failure | Both protocol/status freeze and runtime flush acceptance; prevents storage success being called a logical Commit |
| Trusted principal, authorization granularity, tenant-to-Store routing, permitted cross-project dedup | #181 | #180 identity/schema freeze and #179 admission boundary; decide direction early, not every future tenant feature |
| Scope/serial allocator owner and exposed-serial nonreuse, including discard and ordinary restart policy | #180 with #181 scope input | Filesystem mutation slice; C1 explicitly does not allocate or establish lifetime uniqueness |
| Expected head/base/generation and what a stale result means | #180, consumed by #179/#181 | Any logical Commit or Add request. Explicit stale refusal; no queue silently promotes old roots |
| Location of history metadata and composition of C2 save with stage/branch publication | #180 with C2 boundary review | Schema freeze and acknowledgement claim. Cannot add history tables to the existing Store unnoticed or assume C2 exposes an outer transaction |
| Whether C1 runs at consumer or owner, and exact batch/resource ownership across link | #179/#181 | Protocol freeze. Existing documents disagree between logical-only link and logical ops plus object batches |
| Unknown outcome, cancellation, bounded backpressure, cleanup ownership | Shared | Any mutating stream protocol. Backpressure before execution is allowed; replay of failed work is not |

A directional initial trust choice can be concrete and narrow—for example one trust domain per owner connection and explicit Store routing—without implementing a general capability-token service. The choice remains a design decision, not something this audit adopts on the owner's behalf. Scope-tagged roots and content hashes alone are not authorization.

## Safe initial end-to-end path before full history

An initial **local, single-save, root-based integration slice** can exercise:

`bounded accumulator -> C1 construct/edit/filesystem operation -> SaveHandoff -> C2 SaveOperation::finish -> retained root -> StoreProvider -> C1 logical readback`

The caller already holds an explicit immutable base root and maintains the next successful root for this slice. Filesystem mutation still needs a minimal allocator contract: one authority, an explicit allocation scope, and no reuse of exposed serials. If that is not yet agreed, start with file-content construct/edit/read operations or a read-only mount over a supplied filesystem root instead of fabricating allocator semantics.

This path can validate batching, output ownership, visibility, readback and actual callback/accumulator behavior without defining commits/layers/branches or pretending `SaveOutcome` is history. A first local prototype can retain its root handle in the running caller; making it restart-discoverable is a separate metadata ownership contract. It is not a complete workspace product or release qualification.

**The boundary is exact:**

| Result/state | What it can mean | What it cannot mean |
| --- | --- | --- |
| FUSE write acknowledged | Runtime accepted the declared bytes/change under its explicit ownership/bounds contract | C2 saved it, history moved, or crash durability |
| Flush saved / C2 `SaveOutcome` | C2 finished required object writes and retained-pack publication under its current profile; caller has a root it can read | A commit/stage/branch/history record exists |
| Logical Commit/stage acknowledged | #180's future stage identity/root/expected-base relation was accepted under its defined atomicity boundary | Branch head moved unless that operation explicitly owns the move; crash durability |
| Add/merge acknowledged | Conditional publication succeeded against the exact expected head, according to the selected history model | Any automatic incorporation of intervening changes or reconciliation |
| Acknowledgement lost | Failure with unknown outcome; preserve ambiguity and affected resources | Implicit rollback, safe resend, or proof that nothing happened |

The present C2 API makes this distinction observable: `Store::begin_save` returns a save operation; `SaveOperation::finish` returns `SaveOutcome` only after owner completion, and does not receive a Workspace, branch, commit or history transaction. `SaveOutcome.commits` counts SQLite transaction acknowledgements, not logical Commit entities. Sources: `core/crates/layerfs-storage/src/cas/store.rs:28-46,278-295,423-439`; `cas/lifecycle.rs:1-8`; `cas/finish.rs:1-23`.

## The metadata/publication seam must be resolved early

Current schema validation explicitly rejects extra tables (`core/crates/layerfs-storage/src/sqlite/schema.rs:127-137`), and `Store::open` adopts a specific schema identity and persisted policy. #180 cannot simply append `workspace_stages`, `branches` or allocator tables to that file and continue using unchanged C2 open validation. Nor does high-level `SaveOperation::finish` expose a transaction callback into which #180 can insert those rows.

The minimal decision is whether history owns a separate persistence surface and publishes only references to already-successful saved roots, or whether a supported shared transaction/schema contract is genuinely required. The first approach naturally admits saved but unreferenced objects after a failed history publication; no GC means those bytes remain. The second is an explicit C2 boundary change that must preserve replaceability and failure semantics. This audit does not choose a redesign or authorize reaching into public low-level SQL modules as an integration shortcut.

Regardless of placement, authorize logical stage/branch operations at the owner boundary and never acknowledge publication of a root whose required object save did not succeed. A lost logical publication acknowledgement remains unknown even when content already exists. That is why object dedup cannot settle protocol replay semantics.

## Contradictions and stale wording to amend before implementation

1. **Automatic rebase/retry is incorrectly presented as the runtime path.** #180's settled table says the queue unit is "rebase then merge"; its Boundary says rebase silently resolves overlaps. `02-init-commit-and-concurrency.md:481-497` specifies `HeadMoved` looping with backoff; `:536-538` presents silent rebase as the known gap. Higher-priority owner direction is explicit: stale captured base/head is rejected with no automatic merge/rebase (`docs/roadmap/0.1/0.1.7/README.md:43-55`) and failed work is not retried (`:61-75`). Mark that queue sketch historical/deferred and remove it from v0.1.7 acceptance. A later, separately requested operation is not automatically authorized replay.
2. **Crash durability is accidentally reopened.** #180 asks whether stage/allocation is durable; #179 says acknowledgement "must survive connection loss." Preserve ordinary runtime atomicity and state what a successful response proves, but do not turn those questions into new sync/WAL/recovery work. Connection-loss ambiguity cannot be eliminated by wording. Owner ruling at roadmap `:93-96` and `core/AGENTS.md:149-161` controls.
3. **Fresh flush is not proven replay-safe by content addressing.** #181 calls it safe before requesting a ruling. Identical object IDs permit CAS reuse; they do not make allocator exposure, staging rows, branch CAS, authorization or a logical operation idempotent. Replace that assertion with the already settled no-resend rule: unknown acknowledgement is failure; a separately requested authoritative inspection may establish persistence. No automatic poll/reflush/retry.
4. **Different branch rows do not mean no Store contention.** #180 and the proposal claim different-branch commits never contend. They avoid the same logical head CAS target, but share SQLite writer ownership and C2 resources. `cas/lifecycle.rs:3-8,40-52` makes this explicit. Specify branch-head contention separately from physical writer contention. Keep multi-writer prerequisites separate and conditional on implementing concurrent writers.
5. **The proposal mixes reference and replacement implementation.** It labels stage/merge "holds today" while core has no history, and describes a parallel private prepare design that its header says is not implemented. Label those statements as reference behavior/proposed integration, not current C2 promises. Its four-table count, 128 KiB per-writer candidate cache and no persisted-signature update assertions are stale against current C2's five-table/schema-6 Store-owned index. Fix only current descriptive pointers; retain historical receipts.
6. **Logical-only link versus object-batch link is unresolved.** #181's settled table says the link carries logical operations, not objects; the proposal sends logical ops plus canonical batches and puts C1 in the consumer. A batch payload inside one bounded logical operation is not necessarily one-message-per-object, but the execution placement and actual contract must be explicitly selected before framing freezes.
7. **Integrity is not authorization.** #181's "no transport-level MAC is needed" is not justified solely by rehashing content; the expected identity must itself be trusted, and mutation requests/principal binding need authentication/integrity. For a first trusted in-process local deployment, the local trust boundary can avoid a remote wire-security mechanism entirely. For remote/untrusted peers, preserve the proposed authenticated stream choice (e.g. its TCP+TLS option) and let trust design specify guarantees. Do not add a custom cryptographic protocol merely to resolve the wording.
8. **Filesystem request shape needs precision.** #179 says "complete final bindings per changed directory"; C1 says the final binding of **each name it changes**, sorted and unique (`filesystem/input.rs:3-9`). It does not require resending every unchanged name. This affects accumulator size and wire operation shape; fix before those bounds are specified.

## Exact narrow issue amendments proposed

**Proposal index amendment:** replace “Why this order is strict” and its whole-pair chain with “Parallel design; explicit freeze dependencies.” Move the directional tenant/auth decision into the shared initial milestone, show runtime operation and history semantics as mutually informing drafts, and make protocol framing depend on the agreed operation/acknowledgement contract. Remove the stale four-table assertion; link a pinned schema inventory instead.

**Add to all three Sequencing sections:**

> These are parallel co-design tracks, not an implementation order implied by pair number. Before schema/protocol/runtime-contract freeze, jointly record the operation and acknowledgement vocabulary, immutable base/expected-head inputs, allocation ownership, directional tenant/Store routing, authorization boundary, and C1 execution placement. Track each as a bounded dependency; no sibling issue must close in full for independent design work to proceed. Implementation consumes the reviewed C1/C2 contracts and records unresolved Stage 7 blockers explicitly.

**#180 acceptance additions/replacements:**

> Freeze history schema only after #181's directional trust/tenant routing and identity-scope decision, and after the metadata-placement/publication contract with C2 is explicit. Current C2 rejects extra tables and provides no high-level shared history transaction hook. State saved-root, staged Commit and conditional Add acknowledgements separately; record saved-but-unreferenced objects on later publication failure. Define stale captured base/head as an explicit refusal, with no automatic rebase, merge, backoff retry or replay. Define exposed-serial lifetime/nonreuse and discard behavior without adding a crash-durability promise.

Also replace the runtime rebase-queue entry and silent-overlap passage with the explicit owner deferral; change different-branches-never-contend to different branches have independent head CAS targets but share physical writer resources; replace outdated table counts with a source-pinned link. Correct the document pointer from `01-init-commit-and-concurrency` to `core/docs/architecture/proposal/02-init-commit-and-concurrency.md` and the "four phases ①–⑤" typo.

**#181 acceptance additions/replacements:**

> Supply the directional tenant/Store routing, authorization scope and identity namespace decision to #180 before its schema freeze; token/key lifecycle implementation need not block unrelated history design. Freeze mutation/status framing only after #179/#180 agree save/flush/stage/merge and unknown-outcome semantics. A lost acknowledgement is failure with unknown outcome: no automatic resend, fresh-overlay reflush or polling; separately requested authoritative inspection may establish persistence. Content IDs establish object integrity/reuse, not authorization or whole-operation replay safety.

**#179 acceptance additions/replacements:**

> Distinguish accepted overlay writes, storage flush/save completion, logical Commit/stage and Add/merge. Define the exact captured input generation and ownership of unacknowledged overlay data; connection loss reports an unknown outcome where appropriate and does not trigger replay. A first direct root-save/readback slice may precede full history but makes no logical Commit or restart-discovery claim. Filesystem directory input supplies sorted final values for changed names, not all names in the directory.

## Deferred versus necessary

Must decide now: names and meaning of success, failure and stale state; base/head generation ownership; allocator responsibility; directional trust/tenant topology; metadata location and atomicity boundary; logical-operation shape and C1 placement. These are prerequisites to coherent interfaces, even for the smallest path.

Can remain deferred: full conflict inspection/reconciliation (#164), automatic rebase/queue retry (not allowed in current scope), GC, concurrent writers/per-save publication redesign when not implementing concurrent writers, capability-token infrastructure if a narrower trust choice suffices, every deployment combination, and performance claims. None should be silently reintroduced to make the three design issues appear sequentially complete.

No runtime or concurrency claims were tested in this review. The recommendation is a source-backed ordering/design correction, not implementation or acceptance evidence.

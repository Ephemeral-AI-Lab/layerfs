# Commit cost review for a fresh Workspace per agent tool call

Read-only source/retained-evidence review, 2026-09-14. No product changes, builds, tests, benchmark invocations or timing experiments were performed. This report analyzes the new component implementation against the old public path; it does not claim the component has matched v0.1.5. Shared HostOverlay/Index/Payload implementation work was active during review; the Commit/correspondence files listed at the end were unchanged by this reviewer.

## Recommendation and ordering

Keep a fresh public Workspace/root lease/session per tool call. Share physical services only where their existing ownership permits; do not keep one mutable logical Workspace across unrelated calls to hide Begin/End. On the Commit side, the smallest useful sequence is:

1. Restore a proven-unchanged candidate shortcut inside the new builder, preserving actual Store staging, conditional publication and V4 resolution.
2. Put empty/singleton predecessor descriptions directly in the existing correspondence row; retain the paged representation for larger descriptions.
3. Use the existing bounded small-case allowance for captured changed keys before spilling to its existing index; avoid four scans and repeated empty-file journals on tiny cases.
4. Correct real construction resource admission and its first-row SQLite spill before considering worker/concurrency tuning. Keep the canonical builder unchanged.
5. Treat tagged tiny payload storage as a representation optimization only after its bounded append promotion and ownership contract are explicit. It is not a reason to change CDC/FULL/DELTA selection.

Steps1–3 remove concrete work from one-file/no-op attempts. There is no supported millisecond or percentage forecast for them. The required V1 acquisition proof, full public dispatch integration and benchmark campaign remain prerequisites to a performance claim.

## What v0.1.5 actually measured

Source of truth: `git show v0.1.5:release-notes/0.1.5/benchmark-closeout.md`, tag commit `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`, and its `benchmark-performance.csv`. That release record reused the #120 campaign; no benchmark was run merely to generate the release document. Fresh rows below used binary `c55daf13e372331a5ab6dbd465ece351a55923831c45864325ac46b1508fa295`, source `1ff1f2dddeb60493953311de316fa5bec4634a1a`, macOS arm64 Store/SDK and Linux Docker daemon/FUSE/workload. These are single complete samples, seed1, under their recorded cache contract. They are not distributions or matched current-versus-v0.1.5 pairs. The closeout's v0.1.3 ratios have an undeclared reference cache profile and cannot establish a paired claim.

Exact public-call boundaries from retained #120 `perf.jsonl` records (milliseconds, exact nanosecond conversion):

| Case | Begin/create | Exec/read | SDK edit | Commit | Visibility query | End | Registered timer |
|---|---:|---:|---:|---:|---:|---:|---:|
| `workspace-clean-commit-1-compact-v2` (50files/1MiB) |6.730750|—|—|1.501458|0.096459|2.166500|pure_call_sum=10.495167|
| `workspace-clean-commit-10-compact-v2` (500files/10MiB) |7.868500|—|—|1.488042|0.086250|2.428958|pure_call_sum=11.871750|
| `workspace-clean-commit-100-mixed-v4` (2000files/100MiB) |10.372541|—|—|1.762958|0.146167|3.188084|pure_call_sum=15.469750|
| `workspace-clean-commit-500-mixed-v4` (5000files/500MiB) |13.143875|—|—|1.634417|0.087750|2.929041|pure_call_sum=17.795083|
| `payload-random-read-1-compact-v2` |10.824458|4.188334|—|2.179416|0.094583|3.173625|pure_call_sum=20.460416|
| `overwrite-head-4k-on-1mib-ops-1` |11.843000|—|1.894000|9.053833|outside edit timer|3.071208|edit_commit=10.947833|

Raw paths are `benchmark-results/host-store/issue120/performance/<family>/<case>/perf.jsonl`; clean cases are in `workspace_change_locality`, random-read in `payload_create_read`, and the SDK overwrite in `edit_length_preserving`.

The generic runner times `create_workspace_session`, each selected exec/edit and Commit, one visibility query, and Clean End separately and sums those named calls (`benchmark/fs-bench-pro/src/workspace_bench.rs:1547,1873,1940,1960`). The SDK helper uses t0..t3 for edit+Commit, then t3..t4 for End (`src/sdk_file_edit.rs:36–84`); Begin is separately recorded. Do not compare the 10.50ms clean chain with a new inner Commit timer, or the 10.95ms SDK edit+Commit with a new full tool-call lifetime.

For clean1, Begin's attach subreceipt is5.625375ms, including mount readiness5.406791ms,10 snapshot database calls/10rows/4968bytes and no Store-wide scans; no per-Begin Docker engine call is recorded. The outer command581.574375ms, preparation670.521167ms and cleanup386.553750ms have different boundaries and are not interchangeable with Begin/Commit/End. A fresh Workspace can use the already established daemon/container service; fresh logical isolation need not imply launching Docker per call.

The SDK overwrite's old Commit includes pause fence1.224084ms, content2.126917ms, checkpoint2.332792ms and publication0.103875ms. Those are attribution fields of this old sample, not independent removable latency forecasts. The new design must remove freeze/checkpoint semantics while accounting for its new snapshot/ownership work.

Large-namespace and repeated-work context:

- `namespace-100000`: Init4.397542208s, verified cold source0/125169 resident pages, separately recorded acquisition25.942s and complete envelope31.615s in the closeout. This is100000-file Init, **not Begin or Commit over an existing namespace**. No matched100000-file no-op/Begin figure is supplied by this closeout.
- `workspace-distributed-sdk-edit-100-mixed-v4`:213.086041ms public-call sum. The500-tier0.66610s row is explicitly reused from another treatment; do not label it fresh candidate evidence.
- `dedup-history-unrelated-500-mixed-v2`:16.106808892s public-call sum for500 exec/Commit iterations within one Workspace. Retained exec sum7.650944528s and Commit sum8.441419157s, with Begin10.715416ms, End3.625833ms and visibility0.103958ms. This is not500 fresh Workspace lifetimes. Preserve the closeout FAIL/waiver: generic collection sample status PASS does not erase the missed<15s criterion.
- K32000 and full157 are retained historical evidence with different timing/source contracts. The closeout supplies no measured fresh-Workspace-per-call million-operation result. No such number is inferred here.

## New path: concrete avoidable work

### A. A clean attempt still takes the entire private builder

The old public lifecycle has `generation==0` → empty `ObjectBuffer` candidate and Store admission (`v0.1.5:crates/layerfs-workspace/src/lifecycle.rs`, same shortcut remains in current `lifecycle.rs:119`). In the primary reviewed commit, the new `snapshot_candidate.rs:376–611` always captures changed keys, acquires the96MiB/128FD construction envelope, creates a comparison reader, makes a task journal, initializes result/frontier/reference machinery and finishes a private ObjectBuffer. Even zero tasks reach final `reachable_from`: it inserts the external root into a seen set before discovering no candidate-owned object. With the reviewed private index4MiB minus fixed SQLite cache4MiB, this causes a first-row private SQLite spill.

Smallest change: after the **supported-surface acquisition** succeeds, if captured logical sequence equals the already applied canonical correspondence coverage, create an empty candidate for that exact canonical predecessor and reuse the metadata-only correspondence. Skip CapturedChanges, task/results, origin descriptions,96MiB construction admission and private reachability SQLite. Reuse the existing empty ObjectBuffer path/shared Store admission, not a new encoder or a forged `UpToDate` result. Give any actually retained admission buffers their real bounded owner. Cached inode import/physical backing substitution may change root identity without changing logical sequence, so a root pointer comparison is not the correct predicate. Conversely host sequence equality is insufficient until V1 makes the capture complete for dirty mappings.

The shortcut cannot classify dirty-net-zero changes from sequence equality: build those through existing canonical machinery and let the resulting root/Store decide. It must also never use live `snapshot.base_root` when C1 is the current canonical comparison root.

V4 behavior that must remain identical:

| Condition | Required behavior |
|---|---|
| Captured state proven equal to canonicalR; expected head/base still match | StageR and return Store-authoritative UpToDate; apply exact coverage then acknowledge receipt. |
| CandidateR unchanged but new canonical base differs | Preserve existing Store decision: same content can still create a commit with changed base identity. No unconditional UpToDate shortcut. |
| Head/base moves after capture | Shared conditional transaction rejects publication; exact stage remains available. No success inferred from equal bytes. |
| Created reply lost | Resolve retained exact receipt/stage; no new snapshot or builder. |
| UpToDate reply lost | Resolve its retained receipt although stage was deleted; missing stage never proves nonpublication. |
| Coverage/context installation or acknowledgement fails | Retain exact pending attempt/receipt; apply before ack; next request resolves it before capturing another boundary. |
| Two attempts share candidate root but have different covered sequence | They are different receipt tuples; never reuse prior coverage. |

This is a small change in builder entry and existing no-op/retry tests, not a Store publication refactor. Keep `commit_attempt.rs:92–291` and Store `workspace.rs:452–725` as the authoritative lifecycle.

### B. Singleton file descriptions cost three private pages

For a one-span nonzero file, `Description::build` makes an offset index leaf and an origin index leaf. Each descriptor performs floor, next-neighbor validation, origin insertion and offset insertion. `persist` creates a third header/link leaf (`correspondence.rs:188–297`). Each leaf is4096bytes. The three pages are additional to the existing shared per-node/reverse/pending indexes. This is structural allocation arithmetic, not measured physical RSS or runtime: N singleton files entail3N such live description pages (12288N bytes) before shared-index overhead and transient COW copies.

Preferred compact representation: store a singleton directly in the existing `CORRESPONDENCE_NODE` value. Its maximum fields are canonical inode32 + file content root32 + length8 + Origin24 + origin-offset8 + tag/kind2 =106bytes. Node identity is already in the key; singleton logical offset is0 and span length equals file length. This fits the current Index128byte inline-value allowance. Empty/zero forms are smaller. It eliminates all three extra description pages for this case rather than merely combining them into one dedicated page.

Use one Description API with inline and paged storage variants: constructor/normalization, cursor, origin predecessor/range lookup, persistence and restore. Keep existing two-pass anchor/zero-gap planning unchanged against those accessors. Fragmented descriptions promote to the existing paged indexes. No `Arc` pointer becomes an Origin, no content hash becomes lineage, and no raw range/payload root is retained by published correspondence. Promotion/demotion are metadata representation changes.

Adversarial checks: singleton→split→singleton; equal-content fresh Origin remains replacement; shifted and partial origin coordinates; zero insertion/deletion around nonzero anchors; empty-to-nonempty; hardlink aliases keep one inode identity; persisted reopen/ownership after old description drops; C1 payload reclamation while C2 still maps exact canonical spans. The existing `correspondence` and actual C1/C2 candidate tests are the oracles, not a parallel algorithm/test universe.

### C. Small captured sets are copied into another index and scanned four times

`CapturedChanges::capture` reads already bounded latest changes by sequence, then writes an ephemeral sorted node/binding index (`snapshot_candidate.rs:182–304`). A node entails get-before-set; a binding adds its parent and a separate binding row. The resulting node set is traversed for maintenance union, optional removed-small predecessor discovery, file planning and namespace construction. This is four full changed-node traversals; per-file old canonical records are also looked up for planning and namespace application. It stays proportional to changed state, not the whole namespace, but a one-inode tool call pays disproportionate setup.

Minimum representation improvement: keep a fixed admitted small collection (at most64 records **and a byte cap for names**) sorted/deduplicated in memory, then spill into the same existing Index if either cap is exceeded. Reuse CapturedNodes/bindings iterators for both variants. Include type/deletion facts needed by predecessor discovery so file-only changes do not rescan all nodes looking for directories. Do not add a third persistent change index to every ordinary mutation just to make Commit cheaper. Do not enumerate all bindings in a dirty parent: C2 must use only captured binding keys after coverage.

Within one captured immutable input, reuse the file-plan predecessor record in namespace construction rather than doing a second Store lookup merely to compare it with itself. Keep canonical node identity and captured record validation; the fixed324byte journal slot already stores that predecessor. For zero files, omit its task/results journal entirely. For one file, any synchronous delivery optimization must use the shared finalized-output sink/encoder and preserve bounded output; do not block a bounded channel with no consumer or create a second small-file encoding path. Thread/queue tuning is secondary until attribution identifies it as material.

### D. Keep necessary ordering; remove the accidental resource gate

The attempt mutex is per Workspace and serializes only Commit/maintenance context updates; ordinary Host operations do not acquire it. It is required. The new builder already fixes an old cost/contract problem: canonical output is built privately before acquiring Store-global WorkspaceAdmission, so construction across Workspaces need not serialize on the Store writer.

However the reviewed96MiB reservation under aggregate128MiB admits only one default builder even without that lock. Capacity fixes remain real correctness work. Use traced simultaneously live owner envelopes and exact charged scratch, not a notional smaller number chosen to pass concurrency. Private4MiB seen plus4MiB SQLite cache spilling at the first row is an avoidable concrete cost. Scoped lower cache/index limits can retain existing spill machinery; released default Store policies and public max_final_delta/CDC/FULL/DELTA limits stay intact.

Do not remove stage transactions, head/root/base checks, receipt retention or post-coverage acknowledgement as a cheap no-op optimization. Do not run canonical substitution across every completed file merely to End a fresh one-tool Workspace: once attempts/readers are resolved and owners release, the private arenas can retire normally. Optional substitution serves long-lived active Workspace capacity/C2 reuse, not a mandatory checkpoint before End.

## Adversarial review of tagged <=64byte ordinary payloads

This is viable as a single payload representation only with explicit ownership and promotion details; it is not unconditionally approved by this review.

- Keep ordinary writes as `Source::Payload { token }` with `inline_charge=false`. `OwnedPiece::inline` currently means SDK-inline quota and must not be used as an ordinary-write shortcut. Preserve `write_from` versus `write_inline_from` spool classification, Source origin/highwater and builder descriptor kind Temporary. The canonical builder sees the same reader and byte sequence and retains exactly the same small-file/CDC/FULL/DELTA rules.
- Store the small source bytes once behind the stable token/source interface, preferably in one tagged source row. Duplicating bytes in every subrange token would duplicate raw ownership/charges; independently dropping source tokens must not strand a derived reader. Retaining an entire<=64byte source for a one-byte subrange is within the existing<=4KiB retained-slice ceiling, but ordinary spool accounting must continue to charge it until the last source owner releases. Index page bytes remain physical metadata; do not report them as zero storage.
- Current fresh extent ownership writes six logical catalog rows: TOKEN,SOURCE,LOCATION,PHYSICAL,COVER,COVER_REVERSE, plus at least one4096byte arena block. An inline source can avoid the arena and location/coverage rows if source/token reference counts fully cover its fixed tiny allocation. It does not eliminate metadata writes: source and token rows still need owned publication and quota accounting. The six-row count is source-derived, not a measured write-amplification number.
- **Append promotion is the critical counterexample.** Returning `None` before input once the inline source reaches64bytes is individually safe, but repeated17byte appends would create a new source/lineage/piece roughly every three calls. That eventually hits the unchanged8193-piece gate; the previous source-frontier implementation coalesces those appends. Require one bounded promotion that copies at most the old64byte prefix into arena-backed ownership, preserves old tokens/readers, never reuses Origin coordinates, and keeps sustained appends coalescing. A bounded equivalent-backing replacement of the last range may require an explicit append result variant; do not silently consume input and then discover join incompatibility. A split inline-prefix/arena-tail source is more complex and is not the minimum design unless its benefit is proven.
- Prove failed promotion leaves the old source readable and all prospective physical/metadata charges retained; an old source reader or C1 snapshot must not be overwritten. Truncated-prefix append must reject nonfrontier extension before reading input. SDK-inline remains nonextendable. Partial ranges, joins and canonical substitution keep the same logical Origin/coordinates.

Start with compact singleton correspondence and clean Commit, whose changes do not introduce this payload-promotion state machine. If tiny payload promotion would require a new general storage hierarchy, defer it; no estimate shows it is necessary to approach the retained baseline yet.

## Existing targets and evidence required

Use current family entrypoints only after they invoke the delivered snapshot path. Do not use `shared/v016_matrix.py` or #122-owned cases. The exact36-row #122 manifest does not contain the inherited cases below; reconcile successors at any actual campaign freeze.

| Change | Existing matched public targets / correctness obligations |
|---|---|
| Clean shortcut / fresh lifetime | all4 `workspace-clean-commit-*`; `payload-random-read-1-compact-v2`; retained no-op/lost-Created/lost-UpToDate/head-conflict/coordinator tests. Report Begin, Commit, End, public-call sum and actual command window separately. |
| Small correspondence / tiny payloads | `overwrite-head-4k-on-1mib-ops-1`, corresponding middle/tail and500MiB controls; `tiny-bulk-create-1-compact-v2`; `workspace-distributed-sdk-edit-1-compact-v2`; exact C1/C2, origin/zero-anchor, quota/promotion/reader retention tests. These4KiB workload rows are scale controls; no v0.1.5<=64byte standalone performance match was found. |
| Changed-key/index work | `workspace-fixed-move-1-compact-v2`, `namespace-subtree-relocate-delete-1-compact-v2`, distributed100/500 and directory construction tiers; assert untouched namespace is not scanned and exact changed binding suffix is used. |
| Large preceding work / repeated attempts | `dedup-history-unrelated-500-mixed-v2` with its retained existing criterion; inherited repeated-publication proof; C1-large/C2-one-byte component oracles. A500-iteration single-Workspace history is not a substitute for500 fresh sessions. |
| Shared construction changes | affected `init_namespace` and canonical builder tests;100000-file cold Init remains its own contract and may not be used as a Begin baseline. |

No current component measurement in this review is a fully public benchmark comparison. The reviewed `lifecycle.rs` still routes through the legacy freeze/checkpoint machinery, while HostRuntime/CommitCoordinator are separate component entrypoints; a fast run that never reaches them proves nothing about their performance. Matched measurements must bind delivered product/entrypoint, unchanged workload and timer, fixture/cache contract, harness and image. Preserve old failures/WARNs, use the existing repetition/custody contracts, and report absent values as absent.

## Source anchors and custody

Primary reviewed source: immutable commit `3f04d4146ebe508a240076e4540de5123b9142d1` (initial HEAD). During review another execution advanced HEAD to `ff7098929eda07d91fbfc2d1072b286231203325`, adding persisted-token/resident-owner separation and charging the ordinary SnapshotReader cache. A later read-only diff observed active changes in candidate_capacity.rs, snapshot_candidate.rs and Store scratch.rs/spill.rs: these add explicit Workspace scratch placement (`acquire_in`/`with_directory`) and its checks. They address placement, but at that observed point do not change96MiB admission,4MiB index/cache behavior, the unconditional candidate route or per-file description pages. No claim here treats the changing working tree as atomically sealed or those ongoing changes as verified. Relevant original-source anchors are `commit_attempt.rs:92–291`, `snapshot_candidate.rs:182–304,376–611,640–744,1212`, `correspondence.rs:188–324,603`, `candidate_capacity.rs:13–69`, `objects.rs::reachable_from/construct_files`, Store `workspace.rs:452–725`, and `overlay_index.rs::INLINE_BYTES` (128). Source line numbers are review anchors and may move under subsequent edits. No source files were changed.

Immutable primary-source hashes (computed from git objects, not a working-tree/build seal):

- `crates/layerfs-workspace/src/commit_attempt.rs`: SHA-256 `bf5dbd7046b35484300c10367d2b22f4e84493d584c8bdada740667b27076143`.
- `crates/layerfs-workspace/src/snapshot_candidate.rs`: SHA-256 `861d30c4277befd57378e03baf8438b34299c6cf513350acc65403414a267d97`.
- `crates/layerfs-workspace/src/correspondence.rs`: SHA-256 `15e1d267950819bc5742e2520b5f604697339e72eae04f3d378ddb2936e8fe95`.
- `crates/layerfs-workspace/src/candidate_capacity.rs`: SHA-256 `20a823e404cd9be4a5c0752f91a081a90e9df33ec4022f2e927a58712a71dd6d`.
- `crates/layerfs-layerstack-store/src/workspace.rs`: SHA-256 `a538c2a66e1b5f72f36d73171d122b8e682c1837089ba00f9be3f61bfc3663bd`.

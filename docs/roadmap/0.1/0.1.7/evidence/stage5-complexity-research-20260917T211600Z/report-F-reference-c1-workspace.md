# Report F — Reference C1 (crates/layerfs-content) and workspace composition (crates/layerfs-workspace): algorithm inventory and update-pipeline complexity

Static source reading only. No builds, no runs, no measurements, no performance claims.
Complexities below are code-reading inferences, not observations.

## 0. Tree identity and files read

- HEAD: `5e45897dd9f56e7f563aada031a68070a438fb93`, `git status --porcelain` empty (clean).
- Method: `git rev-parse HEAD` + full-file reads of the modules below; no test execution.

Files read (production code; tests in the same files were skimmed only for bounds):

| Area | File | ~Lines |
| --- | --- | --- |
| CDC | `crates/layerfs-content/src/file/cdc/gear.rs`, `cdc/mod.rs` | 519, 133 |
| Rope | `.../file/rope/build.rs`, `edit.rs`, `read.rs`, `state.rs`, `validate.rs`, `diff.rs`, `mod.rs` | 367, 904, 515, 245, 132, 403, 16 |
| Extents/codec | `.../file/extent.rs`, `extent_codec.rs`, `content.rs` | 161, 267, 300 |
| Trees | `.../tree/batch.rs` (signatures + engine entry points), `tree/inode/table.rs` (reconcile merge) | 2863, 2015 |
| Filesystem | `.../filesystem/resolve.rs`, `apply.rs` (signatures) | 88, 1411 |
| Workspace | `crates/layerfs-workspace/src/changes.rs` (production ≈ lines 1–3430), `lifecycle.rs` (commit paths), `file_io.rs`, `capture.rs`, `reconcile.rs`, `snapshot_input.rs` (signatures), `cow_tree.rs` (signatures) | 5890, 1980, 1859, 214, 443, 395, 1027 |
| Docs | `docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md` §7 | — |
| Core (ORIENTATION ONLY) | `core/crates/layerfs-content/src/filesystem/update.rs` (header), module listing | 498 |

Out of scope and **not read**: `crates/layerfs-layerstack-store/*` internals (`construct_workspace_files`,
`ObjectBuffer`, `commit_workspace_candidate`, `SnapshotReader` caching, SQL). Store-side trip counts are
UNKNOWN beyond the call sites cited.

## 1. Variables and bound classes

- n = dirty nodes in the change set; f = dirty files (f ⊆ n); e = extents (chunks) in one file mapping;
  B = logical bytes of one file; d = delta entries of one directory; R = reference-journal rows (binding
  edges observed during directory merges); K = distinct frontier inode keys touched; p = path component
  depth; S = spilled frontier rows; W = construction workers (≤ 8); L = tree level (≤ 31);
  P = tree pages touched.
- **ENFORCED**: a hard limit that fails the operation when exceeded (error return).
- **STRUCTURAL**: bounded only by other enforced bounds (e.g. depth ≤ 31, entries ≤ 128), so a small constant.
- **UNKNOWN**: not derivable from the code read.

## 2. Algorithm inventory — reference C1 (layerfs-content)

| Algorithm | path:line | Time | Peak owned memory | I/O trips (per call) | Bound |
| --- | --- | --- | --- | --- | --- |
| FastCDC scan (two-byte GEAR, normalization shift) | `crates/layerfs-content/src/file/cdc/gear.rs:54-76`, region loop `gear.rs:85-103` | O(B), single pass | 32 KiB chunk vec (`gear.rs:60,108`) + 32 KiB read buffer (`gear.rs:60`) | reads source; emits via callback | chunk 8–32 KiB **ENFORCED** (`gear.rs:7-9`; min-skip `gear.rs:127-135`, max-cut `gear.rs:181-184`) |
| Streamed mapping build (CDC → extents → levels) | `crates/layerfs-content/src/file/rope/build.rs:108-133` (`scan_mapping`), flush `build.rs:185-207` | O(B) + O(e) | levels: ≤ 193 entries × 40 B × O(log₆₄ e) levels ≈ ≤ ~240 KiB (STREAM_FLUSH_AT=192, `build.rs:14`) | per chunk: 1 chunk-object encode+put + 1 payload put (`build.rs:115-120`); per node: 1 node put (`build.rs:284-285`) | node 64–128 entries / ≤ 8 KiB **ENFORCED** (`extent.rs:3-6`, `extent_codec.rs:109-112`); depth ≤ 31 **ENFORCED** (`extent.rs:5`) |
| Build finish (level collapse) | `build.rs:209-240` | O(e + L) | O(193·L) | ≤ 1 node put per level | **STRUCTURAL** (L ≤ 31) |
| Known-bytes build (< 8 KiB single chunk) | `build.rs:58-93` | O(B) | O(B) ≤ 8 KiB | 3 puts (payload, leaf, state) | **ENFORCED** by MINIMUM_CHUNK_BYTES branch `build.rs:62` |
| FileState framing | `extent_codec.rs:189-203` | O(1) | 93 B | 1 put | **ENFORCED** 93-byte fixed record (`extent_codec.rs:207-212`) |
| Small-content framing (whole file < 128 KiB) | `crates/layerfs-content/src/file/content.rs:69-78`, route `content.rs:206-243` | O(B) | ≤ 128 KiB prefix buffer (`content.rs:232-237`) | 1 payload put | SMALL_LIMIT 128 KiB **ENFORCED** (`content.rs:8,70-71`) |
| Rope replace (split + scan + concat + commit) | `crates/layerfs-content/src/file/rope/edit.rs:34-102` | split O(L) node loads (`edit.rs:415-492`); concat rebalance O(L) (`edit.rs:506-640`); total O(L + e_new) per replace | deferred structural objects ≤ 8 MiB−1 **ENFORCED** (`edit.rs:210,371-373`); prune threshold 4 MiB (`edit.rs:209,234-251`) | per replace: 1 state read (`edit.rs:50`), 2 splits + 2 concats each loading O(L) nodes (`edit.rs:81-84`, load `edit.rs:746-764`), commit re-puts reachable deferred nodes (`state.rs:174-212`), 1 state put (`edit.rs:99-101`) | depth ≤ 31 **STRUCTURAL**; deferred bytes **ENFORCED** |
| FileMutationBatch (multi-edit overlay) | `edit.rs:110-198` | sum of per-replace costs; prune = reachable walk O(nodes) when > 4 MiB (`edit.rs:234-298`) | ≤ 8 MiB−1 charged (`edit.rs:359-378`, charge `edit.rs:392-396`) | sealed nodes put eagerly (`edit.rs:225-232`); commit re-walks reachable set (`edit.rs:300-348`) | **ENFORCED** 8 MiB−1 |
| Coalesce adjacent extents | `crates/layerfs-content/src/file/rope/validate.rs:81-100` | O(k²) worst (Vec::remove shifts) but k ≤ 256 (two merged leaves) | O(k) | none | **STRUCTURAL** (k ≤ 2·MAX_ENTRIES = 256) |
| Logical range read | `crates/layerfs-content/src/file/rope/read.rs:41-51`, plan `read.rs:53-79`, descent `read.rs:206-301` | O(L + e_hit) node loads + O(bytes) | ≤ 127 selected extents ≈ ≤ 4 MiB (READ_BATCH_OBJECTS=127, `read.rs:11-13`) | payload reads batched 127-at-a-time via `get_authenticated_batch` (`read.rs:303-352`); 1 node read per visited node | batch ≤ 4 MiB **ENFORCED** (`read.rs:12-13` assert) |
| Read-all (bounded) | `read.rs:131-169` | O(L + e) | same | 1 read per node + payload batches | maximum arg is a caller bound; u64::MAX default unbounded — **UNKNOWN** per call |
| Extent visitation (construction correspondence) | `read.rs:171-203` + PredecessorCursor `read.rs:368-514` | forward frontier, subtrees skipped by summary (`read.rs:477-479`) | leaf iterator + stack O(L·128) | 1 node read per visited node | descriptor cap 4096 then cursor exhausts (`read.rs:427-430`) — **ENFORCED**; hints ≤ 4 payload ids (`read.rs:406-418`) **ENFORCED** |
| Structural diff (old vs new mapping) | `crates/layerfs-content/src/file/rope/diff.rs:14-67`, spans `diff.rs:270-334`, merge `diff.rs:357-403` | O(common-prefix skip): equal subtree ids stop descent (`diff.rs:172-177`); worst O(L + e) per changed region | 1-node caches ×2 (`diff.rs:51-52`) | node reads only where identities differ; no payload reads | **STRUCTURAL** |
| Whole-file validate | `read.rs:24-39` → `validate.rs:8-60` | O(L + e) | ancestors vec O(L) | node reads + payload-length batches of 64 (`validate.rs:21-48`) | **STRUCTURAL** |
| Cycle checks (`ancestors.contains`) | `read.rs:217`, `validate.rs:15`, `diff.rs:188` | O(L) per node, L ≤ 31 | O(L) | none | **STRUCTURAL** |
| Sorted directory merge (observed) | `crates/layerfs-content/src/tree/batch.rs:1364-1413` (`directory_apply_sorted_observed`) | single sorted pass over deltas merged into tree pages; O(d + P·log_P d) | scratch ≤ 4 MiB (SORTED_TREE_UPDATE_SCRATCH_BYTES, `batch.rs:12`), batches of 32 children (`batch.rs:21`) | 1 root read (`batch.rs:1372`), page reads/writes along affected paths | page 234 items/8 KiB (`batch.rs:13,16`) **STRUCTURAL**; scratch **ENFORCED** via lease narrowing (`batch.rs:237-239` comment) |
| Sorted inode-table apply | `batch.rs:1415-1430` | same engine over inode keys | same | page reads/writes | as above |
| Per-change compact apply (reconcile route only) | `crates/layerfs-content/src/tree/inode/table.rs:993-1003` | one full sorted apply (tree descent) **per changed key** | 4 MiB scratch each | O(tree path) reads + writes per key | **STRUCTURAL** per call; O(K) calls → O(K·path) total |
| Path resolve | `crates/layerfs-content/src/filesystem/resolve.rs:29-55` | O(p) lookups | O(1) | 1 namespace decode (`resolve.rs:35`) + per component: 1 directory lookup + 1 record lookup | **STRUCTURAL** |
| namespace() root decode | `resolve.rs:25-27` | O(1) | O(1) | 1 authenticated object read + decode | no cache — repeat cost is caller-side |

## 3. The workspace update pipeline (crates/layerfs-workspace) — phase by phase

Entry: `Workspace::commit` (`crates/layerfs-workspace/src/lifecycle.rs:36-140`) →
`build_candidate(Commit)` (`lifecycle.rs:114` → `changes.rs:331-339`) →
`CandidateInputs::build` (`changes.rs:637-889`). Phases are timed via
`note_commit_phase` (`changes.rs:3284-3289`). n/f/d/R/K as in §1.

| # | Phase | path:line | Passes over change set | Time | Peak owned memory | Trips (store / spool) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **CandidatePlan**: task plan | `changes.rs:682` (`inputs.prepare()`), impl `changes.rs:1364-1400`; per page `prepare_page` `changes.rs:1402-1539` | one pass over dirty nodes; per ≤128-file page: batched base inode-record lookups (`changes.rs:1417-1424`); per **new** file: path walk with batched directory lookups + record batches at each depth (`changes.rs:1453-1507`); removed-small catalogue discovery bounded (`changes.rs:1185-1269`; 4096 entries/1 MiB/8192 calls, `changes.rs:1166-1167,1284`) | O(n + Σp) batched | ≤ 4 MiB planning (`changes.rs:1370` limit clamp) | store: O(pages + depth-batches); spool: task journal write 324 B/file (`changes.rs:1059,1536`) |
| 2 | **Content**: file construction | `changes.rs:704-712` (`construct_workspace_files`), worker fn `produce_file` `changes.rs:1555-1658`; routing in `FrozenFile::build` `changes.rs:1814-1893` | one task per dirty file (f); workers ≤ 8 (`changes.rs:572-583`), forced to 1 when any predecessor exists (`changes.rs:685-688`) | per file: small → 1 payload build (`changes.rs:1601-1616`); new/complete → streamed CDC build (`changes.rs:1617-1621,1887`); edited base → `mutate_existing_file` O(edits) rope replaces (`changes.rs:1918-2007`) or full rebuild fallback (`changes.rs:1882`) | per worker: rope build state (§2) + deferred ≤ 8 MiB−1 | store: chunk/node puts per file; spool: worker result journal write 52–308 B/file (`changes.rs:1634-1642`) + task-index write per file (`changes.rs:1643-1651`); reads: base pieces via `ReadPlan` (`file_io.rs:227-281`) |
| 3 | **Dirty-node pass** (frontier assembly) | `changes.rs:742-848` | one pass over dirty set (n) | per node: `frontier_inode` O(1) hash/serial (`changes.rs:998-1040`); file result read-back from worker journal (`changes.rs:1697-1745`); directory delta → sorted merge O(d + P·log d) (`changes.rs:925-949`) with per-name fallback (`changes.rs:956-992`: ≤128 batches; per name 1 lookup + apply, `changes.rs:972-991`); old metadata read 1 store read/node (`changes.rs:808-812`); metadata cache build on change (`changes.rs:820-828`); `set_checkpoint` (`changes.rs:847`) | frontier pending map ≤ batch_size entries (clamp 1–128, `changes.rs:651-652`); filter ≤ 4 MiB (`changes.rs:2249-2257`) | store: 1 metadata read per dirty node + directory pages; spool: possible spill row writes 192 B/row + merges (phase 3c below); reference journal rows 65 B/edge (`changes.rs:76-101`) |
| 3c | Frontier spill (inside phase 3 when map fills) | `merge_pending` `changes.rs:2605-2669`, `write_batch` `changes.rs:2710-2741`, `merge_runs` `changes.rs:2746-2839`, `finalize` `changes.rs:2848-2877` | per batch flush: level merge cascade; level i consolidates runs; ≤ 64 levels **ENFORCED** (`changes.rs:2622-2624`) | tiered: each batch participates in ≤ log₂(K/batch) merges (comment `changes.rs:2858-2860`) → amortized O(S·log S) row I/O | merge buffers ≤ 16 KiB ×3 (`changes.rs:2247,2342-2358`) | spool: run writes/reads 192 B/row; run_row binary search reads 32 B/probe (`changes.rs:2384-2397`) |
| 4 | **Reference counting** (`apply_references`) | `changes.rs:854-860` → `changes.rs:3113-3184`; recursive release `changes.rs:3186-3281` | **two** sequential passes over the reference journal (additions then removals, `changes.rs:3127`) + per zero-ref directory a full descendant traversal (`changes.rs:3210-3279`) | O(R) + O(released descendants) | bounded by `budget` arithmetic (`changes.rs:3132-3134,3219-3239`) | store: batched base records per ≤128-key batch (`changes.rs:3093-3111`); directory pages per released dir; **namespace() root re-decode per base-record batch and per release iteration** (`changes.rs:3124,3201` → `resolve.rs:25-27`) |
| 5 | **Namespace/finish** (frontier → inode table) | `changes.rs:862-872` → `FrontierInodes::finish` `changes.rs:2935-3070` | one pass over frontier rows; encode each record → put → **write its ObjectId back into the spill row in place** (`changes.rs:2983-2998`), then one **full sequential re-read** of the spill feeding the sorted inode-table apply (`changes.rs:3017-3030`) | O(S + P·log K) | scratch ≤ tree_scratch ≤ 4 MiB (`changes.rs:660-662`) | store: 1 put per live record + inode-table page writes; spool: block read+write per row block + full re-read; fallback path re-decodes each record from the store (`changes.rs:3055-3059`) and applies **one tree apply per inode** (`changes.rs:3063`) |
| 6 | **Checkpoint journal** | `changes.rs:861,865-871`; validate `changes.rs:224-259` | one row per checkpointed node; `validate_record` per row reads back metadata (cache-hit or 1 decode) and symlink content (`changes.rs:231-244`) | O(n) | 104 B/row; byte limit n·104 **ENFORCED** (`changes.rs:166,181-185`) | spool: 104 B/node write; store: 1 metadata decode per distinct metadata root |
| 7 | **CandidateFinish** | `changes.rs:879-887` (`objects.finish` → BuiltRoot) | — | store-side | store-side | store internals **UNKNOWN** (not read) |
| 8 | Publication | `lifecycle.rs:125-132` (`commit_workspace_candidate`) | — | — | — | store internals **UNKNOWN** |
| 9 | **Checkpoint install** (post-publication) | `lifecycle.rs:252-320` | `checkpoint.visit` **twice** — validate (`lifecycle.rs:273-277`) then install (`lifecycle.rs:288-299`) — i.e. 2 more passes over every checkpointed node reading the 104-B spool journal | O(n) | O(1) rows in flight | spool: 2 × 104 B/node reads; store: 1 namespace decode (`lifecycle.rs:282-285`); install is live-tree work |
| 10 | Reconciliation route (conflicts only) | preview build `reconcile.rs:357-359`; fingerprint `reconcile.rs:371` → `changes.rs:423-473`; commit retry `lifecycle.rs:63-84` | `resolution_fingerprint` runs **both** a full base-namespace walk (`base_manifest`, `changes.rs:475-527`) **and** a full final-manifest walk (`changes.rs:529-563`), then streams whole affected files (`changes.rs:463-465`) | O(total namespace + affected bytes) | manifests charged against `max_final_delta_memory_bytes` (`changes.rs:519,553`) | store: per base entry a **from-root `resolve`** (`changes.rs:500-505`, O(p) lookups each) + whole-file reads |

Capture fast path (avoids phase 2 rebuilds): sequential appends to a new file stream into a background
object builder; non-sequential writes invalidate it (`crates/layerfs-workspace/src/capture.rs:49-129`).
Spool segments are 1 MiB (`crates/layerfs-workspace/src/file_io.rs:110`); remote windows fetch ≤ 1 MiB
(`crates/layerfs-workspace/src/snapshot_input.rs:72`).

## 4. Comparison notes (reference vs core) — core statements are ORIENTATION ONLY

Template: stage-5-report §7 "Concrete cuts" (`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:452-470`).

**CDC.** Reference: frozen two-byte GEAR FastCDC, streaming, 8/16/32 KiB bounds, 32 KiB owned buffers
(`gear.rs:7-9,54-76`). Core ORIENTATION: `core/crates/layerfs-content/src/file/` exists; content not
inventory-read here. No reference-side evidence of a differing chunk bound; the profile id is a content
hash of the frozen parameters (`gear.rs:18-38`), so any core change would change identities.

**Construction.** Reference: single-pass streamed build with level flush at 192 pending entries
(`build.rs:14,108-133`) and eager per-chunk payload puts (`build.rs:115-120`); deferred structural overlay
bounded 8 MiB−1 (`edit.rs:210`). Core ORIENTATION: stage-5 cut "sorted merge replaces per-name mutation"
targets tree construction, not chunking; chunk construction parity is a parallel-agent question.

**Edits (split/concat/coalesce).** Reference: rope replace = state read + 2 splits + 2 concats +
deferred commit (`edit.rs:50-101`), path work O(L) ≤ 31; FileMutationBatch amortizes multi-edit sequences
in one overlay with 4 MiB prune / 8 MiB−1 hard bound (`edit.rs:209-211`); coalesce is O(k²) but k ≤ 256
(`validate.rs:81-100`). Core ORIENTATION: not read; the reference's structural-edit semantics
(split/concat/coalesce over the same 64–128/31 profile) are the parity baseline.

**Logical read.** Reference: plan caches state+root node (`read.rs:53-79`), range reads descend by
partition_point (`read.rs:270-297`), payloads fetched in authenticated batches of 127 ≤ 4 MiB
(`read.rs:11-13,303-352`); every node load re-validates the node (`read.rs:753-762`). Core ORIENTATION:
`core/.../filesystem/read.rs` exists; per-read validation cost comparison is UNKNOWN here.

**Filesystem update.** Reference does, and the core's documented cuts correspond to:
1. *"Sorted merge replaces per-name mutation and deferred structural graphs"* — reference **has** the
   sorted merge as primary (`changes.rs:925-949`, engine `batch.rs:1364-1413`) but keeps per-name fallbacks:
   batched per-name apply after sorted-apply failure (`changes.rs:956-992`), and one-tree-apply-per-inode
   fallback in finish (`changes.rs:3050-3064`); the reconcile route applies one full sorted apply per
   changed key (`table.rs:993-1003`). Cut verified from the reference side: per-name/per-key mutation
   survives as fallback routes.
2. *"Optional base root replaces a provisional empty seed"* — reference materializes an empty-directory
   object per empty directory (`changes.rs:779-780`) and an empty-leaf FileState (`build.rs:38`,
   `edit.rs:87`). Verified: the seed object pattern is real.
3. *"Typed inode values replace encode → store → ID rewrite → reread"* — verified exactly:
   `changes.rs:2961-2968` encodes each record and `put_owned`s it, `changes.rs:2993` writes the returned
   ObjectId back into the 192-B spill row, and the fallback re-decodes the record from the store by id
   (`changes.rs:3055-3059`).
4. *"Synthetic inode-value identity removed"* — verified: reference inode rows are identified by
   content-addressed ObjectIds of encoded records everywhere (`changes.rs:2963-2968`; `table.rs:1178`).
5. *"Root/profile rereads removed"* — verified: `namespace()` re-decodes the root object on every call
   with no cache (`resolve.rs:25-27`), and the pipeline calls it per base-record batch, per release
   iteration, per finish, per resolve (`changes.rs:2508,3008,3124,3201`). Profile ids are OnceLock-cached
   (`extent_codec.rs:58-75`), so profile rereads were never the cost; root rereads are.
6. *"Exact sizing replaces trial clone/encode-for-fit"* — reference's batch engine narrows child batches
   against a live byte ledger and falls back to point reads when a child's worst case does not fit
   (`batch.rs:17-21,237-239`); this lease-narrowing is the trial-fit pattern being cut. Partially
   verified (engine comments); attribute-build specifics not read — UNKNOWN.
7. *"Decode-then-re-encode validation removed"* — reference re-validates every loaded node
   (`read.rs:753-762`) and every encoded node (`extent_codec.rs:77-78,185`); the cut's target
   (`object/inode_leaf.rs`) is core-side, reference counterpart is the per-load `validate` above.
   Reference-side verification is only partial — UNKNOWN whether the reference inode-table codec had the
   same decode-reread pattern.
8. *"Caller metadata readback removed"* — verified: phase 6 reads back metadata per checkpointed node to
   validate the handoff (`changes.rs:231-235`) and symlink content lengths (`changes.rs:239-244`).

## 5. Opportunity register (reference-side costs)

1. **O(namespace) per fingerprint (reconcile route)**: `base_manifest` walks the entire base namespace and
   resolves each entry from the root (`changes.rs:475-527`), `final_manifest` walks the entire final tree
   (`changes.rs:529-563`), plus whole affected files streamed into the digest (`changes.rs:463-465`).
2. **Per-entry from-root resolve**: `base_manifest` calls `filesystem::resolve` per entry although the
   listing already knows the parent (`changes.rs:498-505`) → O(n·p) lookups and O(n) namespace decodes.
3. **Repeated namespace root decodes in the hot pipeline** (no cache): `changes.rs:2508,3008,3124,3201`.
4. **Spill-row triple I/O**: frontier rows are written (`changes.rs:2727-2729`), later read+written in
   place to embed record ids (`changes.rs:2986-2997`), then read again fully for the sorted apply
   (`changes.rs:3013-3023`) — 3 passes plus merge-cascade reads over the same rows.
5. **Per-inode tree apply in the two fallback routes**: `changes.rs:3063` (finish fallback) and
   `table.rs:993-1003` (reconcile per-key apply) — O(K) full tree descents.
6. **Full both-sides file compare**: `file_matches` reads the entire final file and the entire base file
   into digests when piece shape cannot prove equality (`changes.rs:2034-2059`).
7. **Range compare reads base for every candidate replacement range**: `workspace_range_matches_base`
   reads final and base bytes in 64-KiB chunks to detect no-op rewrites (`changes.rs:2009-2032`), even
   when only one side changed.
8. **Uncached per-node old-metadata read**: one store read per dirty node (`changes.rs:808-812`), while
   new metadata is deduped through `PortableMetadataCache` (`changes.rs:820-828`).
9. **Worker-journal round trip per file**: results written to anonymous spool journals then read back one
   file at a time in the dirty-node pass (`changes.rs:1634-1642` vs `changes.rs:1697-1745`).
10. **Checkpoint double visit at install**: 2 full passes over the checkpoint journal after publication
    (`lifecycle.rs:273-299`).
11. **O(k²) coalesce** bounded by k ≤ 256 (`validate.rs:81-100`) — small, but quadratic in shape.
12. **Reconcile per-key sorted apply** (see 5) sits in `table.rs:993-1003`.

## 6. Round trips per update (architectural)

- **Base reads**: base inode records once per key (batched, `changes.rs:3093-3111`); base directory pages
  once per lookup path; base file content once per compared range (`changes.rs:2017-2029`) or fully
  (`changes.rs:2049-2058`); base metadata once per dirty node (`changes.rs:810`); base namespace root
  **once per `namespace()` call** — O(K + R + released pages) times (`resolve.rs:25-27` call sites above).
  The base tree as a whole is walked only in the reconcile/fingerprint route (§3 phase 10).
- **Re-encodes**: every changed inode record is encoded and stored, its id rewritten into its spill row,
  and (fallback only) re-decoded from the store (`changes.rs:2961-2998,3055-3059`).
- **Per-file store calls**: chunk payload + node puts per CDC chunk (`build.rs:115-120,284-285`); small
  files 1 put (`content.rs:211`); edited files defer structural nodes behind an 8-MiB overlay and re-put
  the reachable set at commit (`edit.rs:174-212,300-348`).
- **ID rewrites**: spill-row in-place id write-back (`changes.rs:2993-2996`) and task-index in-place
  location write-back (`changes.rs:1643-1651`).
- **Spool write+read-back**: task plan (324 B/file), worker result journals (52–308 B/file), reference
  journal (65 B/edge), frontier spill runs (192 B/row, ≥3 passes, §5.4), checkpoint journal (104 B/node,
  read twice at install). All are anonymous unlinked files (`changes.rs:40-51`).
- **Full-tree scans**: only in the reconcile route (`changes.rs:475-563`); normal commits never walk
  untouched subtrees (directory overlays carry the delta, `changes.rs:341-343` comment).

## 7. Honesty notes

- **UNKNOWN**: layerstack-store internals (phase 7–8 trips, SQL counts, `ObjectBuffer` write batching,
  SnapshotReader page caching) — the crate was not read; only call sites are cited.
- **UNKNOWN**: core-side definitive inventory (ORIENTATION ONLY statements above from
  `core/crates/layerfs-content/src/filesystem/update.rs:1-10` header and module listing).
- **UNKNOWN**: `cow_tree.rs`/`live_backing.rs` internals (live-tree per-op costs, install cost per node);
  only signatures were read.
- Memory bounds marked ENFORCED are code-reading inferences from explicit checks; nobody ran them.
- Doc-vs-code: AGENTS.md §8 says runs pin one construction worker; the code default remains
  `available_parallelism().min(8)` (`changes.rs:572-583`), and the code comment
  (`changes.rs:567-571`) documents this as the frozen released behavior with the experiment setting the
  env var — consistent, but the "one worker" property is a run policy, not a code default.
- `read_all` with `u64::MAX` maximum has no size bound of its own (`read.rs:131-137`); the caller's
  `maximum` is the only bound.
- The checkpoint byte limit is n·104 where n counts live nodes (`changes.rs:166`) — this is a scratch
  ceiling derived from the live set, ENFORCED by error, but it grows with the change set, not a constant.
- No contradiction found between the stage-5 cut table and the reference code; cuts 6 and 7 are only
  partially verifiable from the reference side (noted in §4).

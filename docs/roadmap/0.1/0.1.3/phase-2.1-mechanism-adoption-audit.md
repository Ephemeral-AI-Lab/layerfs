# Actual adoption of Phase 2.1 mechanisms by #38 and #39

Implementation references: [file layout and shared ownership](phase-2-shared-code-layout.md)
and [Workspace admission complexity/refactoring model](workspace-admission-complexity.md).

Date: 2026-09-05. Requested read-only investigation by the primary agent and
three subagents. No implementation, build, benchmark or correctness test was run.
This report refines the earlier implementation-order investigation: establish
and finish relevant mechanism transfers before assuming new POSIX work is the
first implementation required.

## Source and method

- Inspected checkout: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`.
- Inspected HEAD: `810bb3a589ac58d103483df34bb58ecfe0f0ddf4`.
- Compared product changes: base `4c9b14a6b489eb6de08d4bfd0d4a723745013ab4` to
  #40 implementation `95578a5e24ac15f38a07535dfdf1fcc9fee80065`.
- Authority: [six-piece specification](phase-2.1-shared-construction-staging-spec.md),
  especially lines 199–220 and the deferred-family boundary at 274–288.
- Traced callers from benchmark definitions through SDK/Workspace/native import
  into content construction, object admission and publication. Verified adoption
  from calls and implementations, not from helper names or completion claims.
- File references below are relative to the pinned integration checkout unless
  otherwise stated. Current-source performance remains unmeasured by this audit.

## Conclusion

#40 delivered substantive reuse: the shared eight-entry metadata cache, sorted
Workspace directory/inode updates, common checked insertion, larger carried
Workspace admission batches, staging and exact conditional publication. These
must not be reimplemented as family-specific optimizations.

The entire six-piece construction pipeline is not uniformly adopted. Native
initialization retains old initial-tree builders; Workspace retains complete
candidate buffering/spilling and later borrowed-byte admission. The owned-slab
producer/admission pipeline remains initializer-specific, and there is no
callable shared construction pool for Workspace to invoke.

Therefore remaining work includes both **adopting available helpers** and
**narrowly extracting/adapting existing initializer internals where reuse is not
yet possible**. This is not a reason to introduce another content engine or a
general public bulk interface.

## Mechanism matrix

“Native” below means the eligible direct initializer used by CAS/CDC input shapes;
fallback remains a distinct route. “Workspace” means ordinary modified Workspace
Commit, including #41/#44 and #39's mutating families. Clean Commit is separate.

| Mechanism | Native direct construction | Modified Workspace Commit | Assessment |
| --- | --- | --- | --- |
| Checked insertion | Direct admission calls the common checked helper | `admit_checked_objects` calls the same helper | Shared in #40; ordinary Workspace no longer does its old separate membership-plan pass |
| Eight-entry metadata cache | Uses `PortableMetadataCache` | Frontier and content-only builders use it | Shared in #40; unchanged roots still reused directly |
| Carried admission batches | 8,191-object / <4 MiB limits | Same limits; batch carried through reachable-object visitation | Real transfer in #40; previously Workspace count limit was 127 |
| Owned slabs and overlapping delivery | Owned 256 KiB / 512-object slabs, bounded queue, construction overlaps admission | Complete candidate first; visitor later copies borrowed bytes into a batch | Not transferred to Workspace |
| New sorted affected-page tree updater | Not called by initial directory/inode construction | Called by directory/inode mutation paths, with fallbacks and bounded batches | Workspace adoption is real; native adoption is missing |
| Canonical content builders / incremental extents | Existing rope/content builders | Existing rope/piece/edit builders | Reuse largely predates #40; no new CDC/hash algorithm was needed |
| Bounded producer execution and ownership | Existing scoped producer threads and one admission owner | Candidate loops remain serial; existing optional single-file capture thread; staging/ownership boundaries improved | Ownership transfer is real; shared multi-file producer pipeline is not available to Workspace |
| Stage and conditional publication | Native LayerStack publication has its own semantics | Three-column stage, conditional Commit/Branch publication, exact-result continuation | Adopted by Workspace; not a reason to force native import through Workspace |

## What changed materially in ordinary Workspace admission

`crates/layerfs-layerstack-store/src/objects.rs:16` changes the ordinary admission
count limit from 127 to 8,191. The byte cap remains 4 MiB minus one byte. This is
a 64.5-fold increase in the maximum object count per batch, **not** a measured
speedup or a promise that transactions are 64.5-fold fewer: bytes can limit first.

`src/workspace.rs:437` uses `admit_checked_objects` instead of its former
`plan_candidate` plus planned admission. New admission at `src/objects.rs:2985`
carries a batch across the candidate's reachable-object stream, flushes at the
count/byte limit or end, and commits each bounded insertion transaction.
`insert_checked_object_batch` at `:3325` retains exact collision authentication
and comparison. Its SQL is a cached per-object INSERT, not a new multirow engine.

Other Store APIs such as `commit_candidate` still exist and may use planned
admission. Do not claim every Store caller has changed simply because ordinary
Workspace Commit has changed.

Native fallback also retains separately planned insertion. The direct-path shared
checked helper does not establish universal insertion-path consolidation.

## The two concrete remaining transfer boundaries

### Native initialization does not use the new sorted tree updater

The results document says the initializer and Workspace call a shared cache/batch
updater through real callers. The cache portion is supported; the stronger
interpretation that both call the new sorted tree updater is not.

- `layerfs-layerstack-store/src/layerstack.rs:1411`, `:1444` and `:2250` call
  `build_initial_directory`.
- `layerfs-content/src/filesystem/apply.rs:366` implements that function with
  repeated `directory_insert` and `prune_to` calls.
- Initial inode finalization at `layerstack.rs:1478` calls
  `build_initial_inode_table_from_pairs`, whose implementation at
  `layerfs-content/src/tree/inode/table.rs:478` uses the older insertion builder.
- New `tree/batch.rs` sorted helpers are called by Workspace changes, not these
  native initial-tree functions.

This is a real call-routing difference, not proof that the older builder is slow
on every case. The narrow transfer candidate is empty-root initial construction
through existing sorted builders, with same-seed exact canonical parity, bounded
scratch and sparse-regression checks before replacement. Preserve fallback where
its supported domain has not transferred.

Native scheduling is independently shape-limited: flat directory tasks contain
512 files (`layerstack.rs:975`), so CAS's at-most-500 files form one task. CDC's
root reference plus variants directory produce two top-level tasks, with the
heavy directory processed as one task (`:995`, `:1100`). Record actual work/CPU
distribution before selecting a scheduling change; adding workers is not itself
a work-reduction mechanism.

### Workspace does not use direct owned-slab construction/admission

Current flow:

```text
build_candidate
  -> ObjectBuffer / DeferredObjectStore
  -> complete candidate and reachable-object order
  -> visit selected bytes
  -> authenticate + bytes.to_vec into admission batches
  -> checked admission
  -> stage and conditional publication
```

`DeferredObjectStore` spills after its 8 MiB candidate-memory limit
(`objects.rs:2010`); `ObjectBuffer::finish` establishes reachability (`:2430`).
`admit_checked_objects` borrows the completed store, authenticates objects and
copies bytes (`:3041–3055`). This is not the direct initializer's owned-object
producer queue and does not overlap construction with admission.

An existing optional single-new-file capture path can construct canonical content
during sequential writes using one thread and a one-slot channel
(`layerfs-workspace/src/capture.rs:48`, `:156`, `:195`). It predates #40 and
returns deferred objects for later admission. This is not an absent streaming
content builder, nor a shared multi-file direct-admission pipeline. Preserve and
measure this route rather than rebuilding content it has already captured.

The existing initializer slab/writer and admission types are Store-private.
`InitializationSegmentAdmission::new` explicitly requires an empty Store
(`objects.rs:2789`). Calling it unchanged for a live Workspace is invalid:
existing roots, reused objects, selected candidate closure and publication/error
semantics must remain supported.

Two different-sized transfer candidates should not be confused:

1. **Small owned-consumption reuse:** existing test-only
   `DeferredObjectStore::consume_prevalidated_pages` (`objects.rs:1818`) moves
   in-memory canonical vectors instead of copying them. A production adaptation
   could remove the in-memory admission copy while retaining complete-candidate
   construction and reachability. Spill reads still copy; this does not remove
   the spool or create a streaming pipeline.
2. **Larger direct-delivery transfer:** adapt existing owned slab, queue and
   checked admission machinery for stable selected Workspace output and a
   nonempty Store. Preserve read-your-writes during tree construction, final-root
   closure validation, old roots, bounded ownership and current failure limits.
   Extract only the required cross-crate boundary. Do not claim a shared worker
   pool already exists or add one before the measured producer work needs it.

Choose between these using candidate-copy/spill/readback counters and the real
input route. A small memory-only win does not justify claiming removal of a large
spilled candidate's write/reread.

## Family adoption and remaining applicability

| Family | Actual use of #40 | Relevant next adoption check |
| --- | --- | --- |
| #41 payload create | Modified Workspace construction/cache/sorted helpers as selected, enlarged checked admission and staging | Candidate bytes/copies/spill/admission; one large synced file is not small-file close batching |
| #42 cross-file CAS | Native direct cache/owned slabs/carried checked admission; old initial-tree builders | Sorted initial-tree transfer and actual single-directory task distribution |
| #43 CDC locality | Same native mechanisms; root-file-plus-directory direct eligibility broadened in #40 | Sorted initial-tree transfer; heavy variants directory task; preserve exact CDC output |
| #44 Workspace unique reuse | Same ordinary Workspace Commit mechanisms as #41 | Owned delivery and remaining construction/admission; per-file fsync remains live workload |
| #39 tiny bulk create/delete | Workspace frontier cache, sorted directory/inode batches, checked admission and stage | Create's candidate delivery; delete's reference-release and final-tree batching |
| #39 Workspace dense rewrite | Shared Workspace builders/admission, bounded frontier when required | Candidate delivery/spill and remaining bounded batch work; retain incremental sparse behavior |
| #39 subtree relocate/delete | Shared frontier tree/cache/admission and stage | Still performs per-reference release/traversal; no survivor-oriented final build has been established |
| #39 history | Modified Workspace Commit each cycle; distributed uses SDK edit, unrelated uses live writes | Per-cycle candidate delivery vs continuation; unchanged extents; preserve every historical Commit |
| #39 directory content scan | Read path does not construct content; clean Commit stages/retires existing root | Construction transfers do not optimize live scans; separate read/metadata work remains |
| #39 Git | Dirty final Commit uses Workspace mechanisms; commands use live filesystem | Apply relevant Commit transfers, but initial status/diff remain read/metadata investigation |

Workspace structural adoption is not equivalent to one global final-state build.
`FrontierInodes` carries at most 128 pending mutations before flushing
(`changes.rs:370`, `:1278`); repeated batches can revisit a page. Dense release
still walks removed directories with one-entry page requests (`:1344`). Existing
sorted helpers and pair/table builders are candidates for bounded broader reuse,
not grounds to remove identity/reference checks or rebuild sparse changes fully.

## Revised next-work order

1. Pin the implementation and repaired harness. Treat #40's narrow actual
   integrations as the starting point; reconcile the completion report's broader
   tree-updater language rather than assuming all six mechanisms transferred.
2. Use one current representative native import and one modified Workspace route
   to account for what #40 already improved: metadata misses, transaction sizes,
   membership work, candidate bytes, copies, spill/readback and tree visits.
   Reuse valid source-matched evidence. Do not rerun full families here.
3. Choose the smallest evidenced missing transfer: native sorted initial-tree
   construction, owned Workspace candidate consumption/delivery, or bounded
   broader tree updates. Native and Workspace transfers are separate branches
   of work; a shared implementation change is not automatically useful to both.
4. Validate #41/#44 after relevant Workspace transfers and #42/#43 after native
   transfers. Implement no family-specific replacement if the original curve
   already qualifies. Keep original seeds, workloads and resource boundaries.
5. Address remaining #39 live POSIX metadata/write/delete/read/Git costs using
   existing handlers. These lie outside the extracted construction mechanisms.
   Small confirmed fixes may proceed independently, but they must not substitute
   for investigating the intended #40 reuse first.

No quantitative family speedup is established by this audit. The enlarged
Workspace batches and removed ordinary membership-plan pass are concrete
improvements already in code; the owned delivery and native sorted-tree gaps are
concrete remaining adoption work. Both facts matter when planning #38/#39.

# Handoff: Stages 3–4 — physical encoding and localized file edits

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Complete [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) (Stage 3)
and [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) (Stage 4) under
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
Exact source paths, supporting tests and recommended per-file/directory LOC:
[stages-3-4-file-plan.md](stages-3-4-file-plan.md).
These two documents form the handoff; neither is a measured optimization claim.

**Current continuation:** the implementation remains partial, but pooling now exists.
Use the [three continuation prompts](stages-3-4-continuation-prompt.md): D's corrected
oracle and stored-tree algorithm, independent pooling coverage/qualification, then
final combined acceptance. The [completion details](stages-3-4-completion-handoff.md)
retain the proof obligations. The full criteria below remain unchanged.

## Copy/paste assignment

You are the implementation agent for Stages 3–4 in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Extend the real core built in Stages 0–2. Deliver the complete physical policy
and real localized edits through independently usable C1 and C2:

```text
immutable base + normalized edits + stable replacement ranges
                             |
                  C1: known-edit construction
             /                  |                  \
          small               large             transitions
      final whole object   reused extents +     final-size route
                           changed chunks
             \                  |                  /
                  finalized canonical objects
                    + bounded predecessor data
                             |
                    C2: exact CAS reuse
                             |
       eligible FULL/PREFIX | pooled metadata FULL/COPY-INSERT
                             |
             compression -> selected pack -> SQLite
                             |
             acknowledged completion -> reopen/readback
```

Implement both stages in one coordinated batch with separately verified checkpoints.
Keep C1 responsible for logical bytes/structure; C2 owns physical representation.
C1 must not call a delta codec or import SQLite. Neither component may require
Workspace/FUSE/daemon/history/checkpoint types or a particular future deployment.
Use ordinary functions, concrete internal types and existing narrow I/O seams.

The expected result is working code, external tests, real timing demonstrations,
LOC accounting and retained correctness/resource/performance evidence. Complete
both issue scopes; do not stop at a proposal or partial payload-delta demo.
Later filesystem-tree construction, Live Workspace COW, FUSE, host/daemon transport
and cloud implementation remain in Stages 5–7.

## 1. Start from the current foundation

Read repository [AGENTS.md](../../../../../AGENTS.md),
[core/AGENTS.md](../../../../../core/AGENTS.md), this handoff and its file plan.
Read [implementation report](stages-0-2-report.md), including amendments 8–9;
then trace the current code rather than assuming the original Stage 2 report
still describes its schema and failure behavior.

Planning HEAD is `5e8b8cbc2`; it follows `acefc3179` (publication watermark)
and `30f5d0633` (pending-group same-save reads and cross-wave duplicates).
Record the actual start commit/tree, relevant dirty/untracked files and source
identity. Preserve unrelated work and existing review/evidence artifacts.
Do not reset or indiscriminately stage them. Stages 0–2 are the foundation;
carry any still-applicable limits forward explicitly rather than calling them
fixed because an old report has a PASS.

Current candidate schema is **version 2, four tables, twenty columns**. Its
`retained_pack_ceiling` is essential ordinary-reader visibility state.
It is not crash durability or a logical checkpoint. Keep the post-review
visibility/ownership/cleanup fixes and their regression tests.

The algorithm/reference baseline is v0.1.6 source
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
Reference root crates remain isolated source/oracle inputs, never a candidate
dependency, source include, linked fallback binary or alternate runtime path.
The current Stage 2 core is a second, explicitly named baseline for incremental
costs; it lacks delta/edits and is not a feature-equivalent v0.1.6 substitute.

### Design inputs and precedence

Read:

- [File content](file-content.md): coordinates, no-op replay, transitions,
  exact COW partition/finality and the reference source map.
- [Physical encoding and packing](physical-encoding-and-packing.md):
  candidates, costs, prefix codec, pooling, index replacement and formats.
- [Policy and tables](content-storage-policy-and-tables.md):
  separate user policy, derived capacities and immutable format/resource bounds.
- [Finalized handoff](finalized-object-handoff.md), [I/O](content-io.md),
  [memory audit](content-io-memory-audit.md), [persistence](admission-and-persistence.md)
  and [canonical objects](canonical-objects.md).
- Existing telemetry README/USAGE and [timing specification](telemetry.md).
- [Benchmark rules](../../../../general/benchmark_rules.md),
  [benchmark AGENTS](../../../../../benchmark/AGENTS.md),
  [quick start](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
  [release](../../../../general/release-policy.md) and
  [documentation policy](../../../../general/documentation-policy.md).

Later owner rules and established correctness fixes supersede stale planning text.
In particular, **tools/preflight.sh is permanently retired**: do not run it,
restore it, or add an equivalent aggregate gate. Use the explicit affected-workspace
commands below. Historical receipts/reviews remain unchanged.

## 2. Hard invariants

- No retry, error-driven fallback, busy handler, fsync/fdatasync/sync_all/sync_data,
  WAL, added crash-durability machinery or third-party patch/fork/vendor edits.
  Keep MEMORY journal, synchronous OFF and zero busy timeout, with runtime SQL
  atomicity/abort. Unknown acknowledgement fails without replay or guessed deletion.
- A missing/ineligible advisory candidate or a successful losing delta trial may
  select FULL by policy. Failed acquisition, corruption, codec reset/encode/decode,
  allocation, SQL or ownership fails the operation; saved FULL is not error recovery.
- Preserve exact CAS comparison, canonical identity, frozen CDC, extent partition,
  dependency/chronology checks, retained visibility and successful stored versions.
  Direct logical references and selected physical dependencies are distinct.
- One writer owns a save across bounded batches/transactions. Preserve shared
  batches/packs across files; no per-file/object commit. Group sealing, construction
  completion, storage finish and acknowledged publication are different events.
- No generic payload spill/scratch, whole-file reconstruction fallback, input-sized
  candidate/extent/edit collector or per-object trace growth. Charge simultaneously
  live capacities, including blocked producer output and consumer-owned bytes.
- One construction worker except the existing future namespace-init exception.
  Preserve useful overlap or justify a replacement with matching evidence;
  a second worker cannot repair a failing performance gate.
- Product files <=999 physical lines; lib.rs/mod.rs <=200 and declarations/reexports/
  direct delegation only. External tests; no product fault-injection/test-only hooks.
  Ordinary error validation, assertions and useful aggregate receipts remain product.
- Reuse dependency pins and existing stdlib structures. No codec/plugin/transport
  registry, new monitoring subsystem or generic memory manager.
- No successful-version rollback or automatic format migration. Define any required
  schema/format compatibility change explicitly before persisting new data.

## 3. Execute in five checkpoints

### A. Freeze real policy/capacity and format support

Keep default T=128 KiB, whole-file depth 8 and chunk depth 4. Implement genuine
configuration with checked supported ranges, persisted policy and reopen agreement.
Separate threshold selection from maximum stored-object validity, chain-work budgets
and live-memory budgets. Raising T or a depth does not automatically raise the others.

Required representative cutoff tests are default 128 KiB, 256 KiB and 1 MiB; they
are correctness/capacity probes, not a tuning campaign or new defaults. If any cannot
be supported, give the exact format/resource blocker and report the unmet coverage;
do not quietly reduce the assignment to default-only support. Demonstrate meaningful
non-default values for both depth fields, including disabling prospective delta if
depth 0 is supported and an increased value. Test an increased cap at its reachable
boundary with byte budgets still enforced; accepting a field never used is failure.
Depth 50 is an optional higher-capacity probe, not a new mandatory default.

Audit construction, field/envelope validation, codec compress bounds/workspaces,
record lengths, pack targets, batch occupancy, chain eligibility and readers.
Keep default codec parameters. Plan a bounded singleton for an accepted record too
large for the normal pack target, preserving selected FULL/PREFIX representation.
Do not silently switch to CDC/RAW, lift memory ceilings or spill after a fit failure.

Write the accepted read/write/profile matrix before adding formats. Stage 2 handles
ordinary/native/compact whole-file lanes; Stage 3 adds real required delta/pooling
behavior. Preserve active v1/v2/v4 and implement required v6 physical metadata.
Identify separately any promised older-profile readers (v3/v5/whole-owner formats)
and their exact scope; no trial decoding or pretending the candidate opens old
reference Stores. If a capacity-aware grammar/schema revision is needed, make one
explicit supported-range contract, not a new version per numeric setting.
Preserve the publication watermark and exact schema validation. Do not blindly
copy the old four-table/19-column proposal over the current twenty-column schema.

### B. Complete payload DELTA and physical readback

Reuse the existing Zstandard raw-prefix codec and distinct role framings.

```text
missing object -> prepare compressed FULL alternative
              -> one eligible candidate under depth/work/memory policy?
                              |
                    at most one PREFIX trial
                              |
             compare actual framed representation cost
                         /             \
                       FULL           DELTA
```

WHOLE_FILE considers its explicit predecessor, then the bounded admitted-FULL
winner cache only when no eligible anchor is acquired. Preserve signature/ordering/
tie-break and Store lifetime. CHUNK uses the first supplied eligible-policy
candidate according to the reference's actual first-candidate rule; do not turn
four correspondence IDs into four trials. Preserve regular-file versus metadata
rope provenance and FULL-based native group membership.

Batch distinct initial candidate locations where independent. Share valid live
base data without forgiving logical work charges. Reconstruct dependencies
iteratively with bounded depth/encoded/decoded work and authenticate required
intermediates, roles, lengths, cycles and chronology. Enforce the same captured
read ceiling through dependency and cache reads. Required missing bases are errors.

Keep default selection/cost rules and candidate universes. Compare complete record
cost including base IDs, not frame lengths alone. Port actual selected-base columns/
checks/indexes and cleanup order. Preserve failed-sync/unknown-outcome invalidation;
do not leave a partly advanced cache usable after failure.

### C. Implement single edits and transitions

Consume an immutable base, declared final length, ordered normalized edits in
**current-result coordinates**, stable reopenable replacement ranges, explicit
policy, authenticated reads and a bounded finalized consumer.

Open/authenticate the base once per operation. Preserve original-base correspondence
needed for no-op tests and predecessor hints after insert/delete shifts. Empty edit
streams and eligible equal-byte replacements preserve exact roots. The no-op
comparison may read replacement bytes, then replay for construction after mismatch:
both passes and base reads stay charged; planned replay is not a retry.

- Small -> small: construct the final whole object, targeting one final allocation
  with explicit old/new overlap; physical delta does not remove hashing its bytes.
- Large -> large: split/reuse unchanged extents/subtrees, CDC replacement ranges,
  concatenate/coalesce and rebuild affected paths. Do not re-CDC all old content.
- Small -> large: stream retained bytes/replacements through the complete builder.
- Large -> small: assemble retained ranges into the final whole object, without
  reading discarded ranges solely for conversion; account physical group/base reads.
- Any -> empty: validate lengths/edits and return the defined empty representation.

At exactly T use chunked representation. For one edit operation select from declared
final length, verify accumulated length, and avoid intermediate representation changes.
Reject unsupported coordinates/overlaps explicitly; do not reorder/merge edit
segments because CDC segmentation and exact roots can change.

### D. Prove and implement the decoded multi-edit frontier

Replace the reference's two encoded structural overlays with one owned decoded
unfinished representation inside file/edit/. Preserve existing split/concat,
coalescing, fanout, half partition, root collapse and old-root immutability.

```text
v0.1.6 edit structure:
encoded draft nodes -> map lookup/clone -> decode -> prune -> encode final output

target:
existing subtree ID + owned decoded join boundaries
               -> split/concat while mutable
               -> final children first -> encode/hash once -> emit
```

Before final emission, prove no later admitted edit or either side of a join can
change that node. Include the 80-entry + 100-entry root join that repartitions to
90 + 90: individually valid leaves are not automatically final. Bound both boundary
paths, builder levels, node capacities and recursion as a function of supported
height/fanout, independent of total edit count/file bytes. Decoded entries may be
larger than encoded entries. Late failures must release ownership and fail C2 once.

Do not port both old overlays into core as a safety mode or emit speculative objects
and collect them later. If the proof is incomplete, keep #169 incomplete and report
the smallest unresolved case; a full rebuild is not an acceptable substitute.

### E. Complete physical metadata, packing and batch acceptance

Physical inode-value pooling belongs to #168 even though filesystem-tree algorithms
are Stage 5. Add only the checked C1 compact-leaf/value grammar needed for supplied
canonical inputs. Do not invent directory/workspace constructors to exercise C2.

Retain 73-byte value equality, first-encounter ordinals, exact chronology, up to 165
values/group, authenticated group digests and pooled FULL/COPY-INSERT DELTA.
Payload PREFIX and metadata COPY/INSERT are distinct existing codecs. Reconstruct
and authenticate each required canonical leaf through the pooled chain.

Build the exact value-group body once, hash it, then compress. Replace the temporary
SQL fingerprint accelerator with the specified bounded Store-owned
BTreeSet<(fingerprint, ordinal)> only with identical selection/window/failure
semantics and successful resource/cost proof. Keep 131,072 retained entries and
whole-group reset behavior; charge actual B-tree nodes/transients. No shrinking
the window, hash-equality shortcut, extra DB or SQL fallback.

Reuse Stage 2 placement-before-assembly. Extend to physical lanes, pooling and larger
singletons without incoming+merged duplicate assembly. Preserve existing locator
ordinals, base chronology, exact UPDATE cardinality and shared pack density.
An on-demand seal for same-save reads/duplicates is ordinary planned work; test its
packing cost and memory rather than retaining unlimited canonical payloads.

Review current Stage 2 hot paths while extending them: per-object clone in
cas/save.rs, repeated group reconstruction, provider connection/workspace churn,
and repeated payload-demand copies. Remove only demonstrated redundant work;
keep collision operands alive through their last required check.

## 4. Required external verification

The [file plan](stages-3-4-file-plan.md#5-supporting-files-and-external-tests)
names every starting test target. Tests exercise public production APIs and
independent oracle data. Freeze reference fixtures/separately sealed reference
execution; a candidate round-trip alone is not an exact-root oracle.

| Target(s) | Required cases and proof |
| --- | --- |
| C1 inode_leaf + object_identity | Exact existing leaf/value grammar and identities; malformed rows/order/counts/lengths/references rejected. |
| C1 edit_single | Overwrite/insert/delete/append; head/middle/tail; complete deletion; actual replacement EOF/length errors; unchanged base roots. |
| C1 edit_batch + edit_model | Current-result coordinates, adjacent segmentation, delete-only monotonicity, repeated length changes, overlap rejection, child-first output and no unreachable new output. Reference root/partition equivalence, not just equal bytes. |
| C1 edit_transitions + file_complete | At every accepted T: 0/1/T-1/T/T+1; small/large/empty conversions, known/unknown complete lengths, compressible and incompressible inputs, persisted reopen. |
| C1 edit_noop | Empty edit stream, equal replacement, long equal prefix then mismatch, shifted-coordinate applicability, bounded comparison and replay. |
| C1 edit_bounds + streaming | Join/root occupancy 63/64/127/128/129 and streaming 192/193; 80+100 join; height growth/collapse; both frontier sides; many files/edits; slow bounded sink; late source/sink failure; capacity accounting. |
| C1 file_read + edit_timing | Fragmented retained ranges, ordered repeated IDs, byte/count-bounded waves, base view reuse; real DB-free edit with supplied provider; enabled/disabled behavior and error reports. |
| C2 delta_payload | WHOLE_FILE and CHUNK FULL/PREFIX wins/losses; exact reuse first; explicit/cache candidates; native first-only; absent/ineligible optional candidates; actual codec/read failure never becomes FULL. |
| C2 delta_chains | Default and changed depth caps, each reachable boundary and first excluded edge, independent encoded/canonical/raw-work ceilings, corrupt/missing/cyclic/wrong-role dependencies, intermediate authentication and iterative memory. |
| C2 metadata_pool | Supplied canonical leaf -> pooled FULL/DELTA -> reopen/exact bytes; high/low value reuse, group size/ordinal boundaries, exact savings thresholds, catalogue/body/digest corruption and late cleanup. |
| C2 metadata_pool_index | Same smallest exact-match ordinal as reference, authentic fingerprint-collision fixtures where available, whole-group reset at 131,072, live Store versus reopen, failed synchronization invalidates once. No test-only public hash injection. State unreachable external collision coverage honestly. |
| C2 physical_formats + policy_capacity | Explicit recognized/rejected format matrix; no decoder probing; valid larger incompressible WHOLE_FILE singleton including 1-MiB cutoff case; checked allocation/frame/pack bounds; invalid persisted policy rejected before mutation. |
| C2 edit_pipeline | Real C1 edits -> C2 -> finish -> reopen -> logical readback; both delta roles and transitions; exact matching roots in standalone/integrated routes; no full-store scan or old-runtime dependency. |
| Existing C2 cas_reuse/pack_locator/visibility/persistence_failure | Repeated IDs within/across waves, unfinished-group reads, stable append locators, private early commits, retained watermark, selected bases/pool values under ceilings, late failure/one cleanup, unexpected SQL cardinality, BUSY/LOCKED failure without retries, quarantined unknown outcome. |
| Existing C2 memory_bounds/timing | All live lanes/base/full/delta/pack/SQL/index/report ownership; direct canonical singleton rather than only a CDC file; no payload staging files; standalone actual storage/codec scopes, bounded reports and original errors. |

Add focused tests for discovered gaps rather than hiding a missing obligation behind
a green target name. Failure cases use real malformed inputs and externally inducible
SQL/filesystem behavior, never product fault injection. If a required failure cannot
be induced, record the exact source proof and coverage gap; do not invent PASS.
Model/oracle collection is verification-only and its memory is reported separately.

### Commands

From repository root, use targeted tests during implementation:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_single --test edit_batch --test edit_transitions --test edit_noop --test edit_model --test edit_bounds --test edit_timing --test inode_leaf
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test delta_payload --test delta_chains --test metadata_pool --test metadata_pool_index --test physical_formats --test policy_capacity --test edit_pipeline
```

After the final relevant changes, run the affected-workspace checks individually:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

Use actual supported CLI syntax; fmt does not take --locked. Report commands,
toolchain identities, test counts and every failure/unrun check. The counter output
above is a working-tree report, not the required exact per-commit comparison.
No zero-test discovery or absent target counts as verification. Do not re-run
unchanged passing resource-sensitive cases without a stated relevant reason.
There is no CI or aggregate pre-push wrapper.

## 5. Demonstrate independent timing

Extend the existing real modes and add the file-plan example
`layerfs-storage/examples/measure_edits.rs`. Required new example interface:

```text
--mode c1|c2|pipeline
--case small|chunked|small-to-large|large-to-small|batch
--threshold-bytes N
--output FRESH_DIRECTORY
```

These flags are a requirement to implement, not a claim they exist today.
Use small deterministic fixtures with their identities/sizes reported. Reject an
existing output directory; preserve errors and JSON timing. Record base preparation,
timed boundaries, excludes, root/length and verified outcome. Required modes:

- C1: real edit with a supplied bounded authenticated provider and nonpersisting
  consumer; no C2 save. Demonstrate a DB-free provider case. Necessary base reads,
  no-op comparison, replay and consumer waiting remain in the named scope.
- C2: bounded supplied canonical targets/candidates -> actual save-to-ack and
  authenticated read; no timed C1 file construction. Do not collect a full file's
  objects to prepare this mode.
- Pipeline: real C1 edit and C2 save/finish/readback through the same bodies.
  Report edit-to-save-ack separately from verification/readback.

Example commands to run once the interface exists:

```sh
stage34_smoke="$(mktemp -d /tmp/layerfs-stage34.XXXXXX)"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode c1 --case small --threshold-bytes 131072 --output "$stage34_smoke/c1"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode c2 --case small --threshold-bytes 131072 --output "$stage34_smoke/c2"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case chunked --threshold-bytes 131072 --output "$stage34_smoke/pipeline"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case small-to-large --threshold-bytes 1048576 --output "$stage34_smoke/grow"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case large-to-small --threshold-bytes 1048576 --output "$stage34_smoke/shrink"
```

These are wiring/correctness smoke demonstrations, not performance evidence.
Preserve existing complete-file example modes as regression coverage.

Inject existing Timing scopes into actual work. Illustrative coarse steps:

```text
file.edit                         storage.save
  base.read                         membership
  compare/replay                    base.acquire
  construct                         encode (payload or pooled metadata)
  output / consumer wait            pack
                                    persistence / finish
```

A synchronous edit timer includes consumer work if its body calls the consumer.
Label inclusions; do not subtract overlapping spans to invent pure CPU time.
No per-object trace retention: preserve complete coarse timing under the existing
1024-node/32-level limits and disclose clipped detail. Timing disabled preserves
the same product operation/results/errors; the caller owns text/JSON output with
ordinary writes, no fsync. No new metrics collector or timer implementation.

## 6. Why this can improve on v0.1.6 — and what must prove it

The reference already has CDC, payload delta, localized edits, useful caches and
bounded streaming. Their existence is not a new improvement. The proposed
improvement is less surrounding work with unchanged successful semantics.

| v0.1.6 cost/source | Intended Stage 3–4 reduction | Evidence required |
| --- | --- | --- |
| Two encoded structural overlays in [rope/edit.rs](../../../../../crates/layerfs-content/src/file/rope/edit.rs) and [rope/state.rs](../../../../../crates/layerfs-content/src/file/rope/state.rs) | Owned decoded frontier; drop overwritten drafts and encode/hash only final nodes | Exact roots/partitions, nodes/bytes encoded and copied, no emitted garbage, simultaneous frontier memory and edit time |
| Repeated classification/root/retained-range work in [file/content.rs](../../../../../crates/layerfs-content/src/file/content.rs) | Known final-size dispatch, one base view, one final small-object allocation where possible | Source passes, plan/node reads, copied bytes, transition time and peak overlap |
| Native initial predecessor point probes in [objects/read.rs](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs) | Batch distinct independent first-candidate lookups and share live bases | Same candidate order/decisions, SQL/base-read counts and full save latency |
| Value wrappers -> compress -> clone -> decompress -> digest in [metadata_values.rs](../../../../../crates/layerfs-layerstack-store/src/objects/admission/metadata_values.rs) | Construct/hash the exact group body once, then compress | Identical group membership/digest/decoded values, codec/copy/hash work and save time |
| Disposable SQL fingerprint index in [objects/metadata.rs](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs) | Bounded standard-library ordered set with identical window/ordinal behavior | Same exact reuse and retained storage, total B-tree memory, cold/reopen sync/lookup time and removed temporary DB I/O |
| Incoming pack then cloned merged pack in [objects/admission.rs](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs) / [objects.rs](../../../../../crates/layerfs-layerstack-store/src/objects.rs) | Exact placement before one selected assembly | Assembly/copy counts, bytes rewritten, tail occupancy and total Store size; Stage 2 already implements part of this cut |
| RAW-singleton temporary-file write/read in reference admission | Budgeted consuming in-memory assembly with checked relocation | Largest supported incompressible case, no payload temp I/O, peak live canonical/pack/driver bytes and successful save/read |
| Duplicate demand copies/reconstruction | Shared bounded authenticated owners and grouped acquisition | Same demand order/cardinality/authentication, reads/decodes/copies and memory, including base dependencies |

Preserve time/space complexity of the reference on the same supported operation.
Large->large work follows required replacement scans, no-op comparisons, touched
mapping paths and unavoidable boundary work; do not claim unconditional O(edit
bytes) for every case. Small-file construction is O(final size). Small->large must
construct a chunked representation; large->small may read several physical groups/
bases for short retained ranges. Include those costs rather than hiding conversions.

Fewer LOC, smaller delta frames or a faster failed operation do not prove a win.
Do not credit already-absent full-file scans, all-edit payload spooling, later
Workspace/RPC cuts or additional workers to this batch.

### Required measured acceptance

Before benchmark implementation/collection, create and commit
`docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-verification.md`, linked
to #168/#169. Reuse existing harness mechanisms; add only the missing real component
entrypoint/fixture adapter, not a new framework. Freeze cases, exact successful
public surfaces, sizes/seeds, source/harness/oracle identities, cache/index state,
single-worker setting, acknowledgement, numerical limits and allowed claim.
Freeze baseline-derived limits before candidate optimization/collection.
Stage 6 broadens full-core qualification; it does not excuse this batch's own
correctness, storage, speed and resource gates.

Required families cover payload FULL/DELTA/new/reuse, localized single/multiple
edits/no-ops, both threshold transitions and supported overrides, physical metadata
low/high reuse and index turnover/reopen, pack boundaries/singletons, grouped
reads/dependency chains and failure cleanup. Use exact cases from an existing
qualified contract when available. Do not silently shrink workloads to fit budgets.

Only match v0.1.6 and candidate when public operation semantics, inputs, algorithm
profile, cache state, work/acknowledgement boundary and harness treatment match.
If the old public API cannot expose an equivalent C1/C2 operation, publish separate
non-comparative diagnostics and say the speedup is unproven. Do not compare a
Workspace pipeline with a core edit, or present new cutoff support as an old-profile
speedup. Source-derived savings are labelled structural evidence.

Report for every applicable case:

- Raw operation/component elapsed ns, whole-command wall, sample count, units,
  throughput byte basis and any CPU measurements with their actual attribution.
- Candidate/usable/FULL/DELTA/reuse counts, chain work, source/base/pack bytes read,
  query/transaction count, copies/hashes/codec trials and assemblies where observable.
- Total retained database/pack/value-group/index footprint, occupancy, written
  versus rewritten bytes and any temporary disk. A DB size delta is not write I/O.
- Allocation/lifetime ledger and measured relevant process/SQL/cache/index memory.
  Count simultaneous capacities and all framing lanes/Stores; SQLite cache_size
  is not a total memory cap. Separate sampled phase peaks from lifetime high-water.
- On/off timing equivalence and overhead with matching inputs/cache state.
- Source/product/harness/fixture identities, raw artifacts, verification outcome
  and every FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN item. Missing counters are unavailable,
  not fabricated zero.

Follow one sample per case/arm unless specifically approved otherwise; with n=1
do not claim statistical confidence. Use a fresh output per run and preserve all
attempts. Reuse fixture preparation/builds by the existing sealed mechanisms, never
warm timed work. Respect host-owned SQLite, measurement lock and command budgets
(ordinary <=15 s, declared exceptions <=25 s; verification <=60 s). No aggregate
preflight or concurrent resource-sensitive builds. Unsupported cold measurement
does not become a cold PASS. Append evidence to the appropriate current ledger.

If the ordered-set/frontier/singleton target loses a required gate, fix or revise
that implementation before acceptance. Do not ship a hidden old/new fallback,
drop the losing case, relax its limits or label the whole batch optimized.

## 7. Completion and issue closure

Create `stages-3-4-report.md` beside this handoff, and fresh evidence directories
under `docs/roadmap/0.1/0.1.7/evidence/`. Include:

1. Exact source/profile/schema/read-write compatibility and supported numeric ranges.
2. Actual file tree, per-file/directory estimates versus actual, physical caps and
   exact per-commit production LOC before/after/delta. Follow the companion file plan.
3. Separate #168 and #169 criterion tables with code/test/evidence for every item.
4. Actual standalone/integrated timer artifacts, including failures and exclusions.
5. v0.1.6 versus candidate mechanism/performance/storage/memory evidence, with
   unmatched/new capabilities explicitly separated and unproved claims named.
6. Bounded-memory ledger and FFI/codec ownership checks; corrected critical review
   findings and the specific regression checks. No universal safety claim from RSS.
7. What was removed, retained, relocated, deferred or remains incomplete, including
   the narrow C1 inode grammar versus later filesystem algorithms.

Close #168 and #169 only after their actual acceptance criteria have evidence.
A working file delta path does not complete metadata pooling; a working single
edit does not complete the multi-edit frontier or transitions. Keep #165 and
Stages 5–7 open. No reference retirement, release/tag/deployment or cloud support
claim. Report a genuine unresolved gate honestly rather than declaring partial
implementation complete.

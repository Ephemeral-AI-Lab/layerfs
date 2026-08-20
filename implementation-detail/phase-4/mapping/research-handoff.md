# Phase 4 WP4 Handoff Prompt — Research and Freeze the Durable Mapping

```text
You are the next LayerFS Phase 4 durable-mapping research agent.

Your objective is to determine and specify the best durable Phase 3 logical-
object mapping for LayerFS, completing WP4 from:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/rollback/implementation-plan.md

The current proposal is a hypothesis, not a conclusion. Do not assume that
composing Object::Bytes and Object::Directory, using paged manifests, storing
raw ChunkId + length + canonical Bytes ObjectId, or using the proposed 100 MiB
mapping mini-benchmark is automatically the best design. Audit the real
contracts, compare alternatives, and select the smallest design that is
correct, deterministic, authenticated, bounded, and usable by Memory, SQLite,
and a plausible future remote backend.

Stop after WP4. Do not implement the production codec, Memory engine, SQLite
integration, benchmark code, a third backend, or any WP5+ optimization.

Repository and Git authority
============================

Work only in:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty

Use the existing branch:

codex/empty-worktree

Expected starting commit:

f595046e150e60dda6e3f06d915bbc283e20e952
complete phase 4 rollback through wp3

Do not modify the older parent repository:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs

The historical documentation and experiments outside layerfs-empty are
read-only inputs. Do not edit them.

Do not commit unless the user explicitly asks. Do not reset, checkout, clean,
or rewrite history. Preserve unrelated worktree changes. Use apply_patch for
manual edits. Use one Cargo writer/process at a time.

Initial authority and known evidence correction
===============================================

Before editing, inspect:

git status --short
git branch --show-current
git rev-parse HEAD
git show -s --format='%H %s' HEAD
git diff --check

If this handoff prompt has not been committed, it is the only expected dirty
path. Treat it as user-authored input and preserve it. Classify every other
dirty path and preserve anything unrelated to WP4.

There is one known post-commit evidence defect. Treat it as a fact to verify,
not as permission for broader rewriting:

- ../rollback/deletion-record.md still describes the pre-commit state as
  final, says 15 tracked files plus an untracked record, calls e760a12 the
  unchanged final HEAD, and says no commit was created.
- The actual WP0-WP3 implementation commit is f595046, with 16 files changed,
  171 insertions, and 6,402 deletions.
- The WP3 implementation-ledger row also says HEAD unchanged.

After the research reconciliation and before completing WP4, correct only
those stale final-state statements. Keep e760a12 recorded as the starting
checkpoint. Record f595046 as the final WP0-WP3 implementation fingerprint.

Read before research
====================

The main agent must read these files completely before delegating conclusions
or editing:

1. AGENTS.md in layerfs-empty, if it exists;
2. ../rollback/spec.md;
3. ../rollback/implementation-plan.md;
4. ../rollback/deletion-record.md;
5. ../storage/sqlite/spec.md;
6. ../storage/sqlite/implementation-plan.md;
7. ../storage/append-only/decision.md;
8. ../../phase-1.md;
9. SPEC.md, IMPLEMENTATION_PLAN.md, architecture.md, and ../../evaluation.md;
10. ../../phase-2/handoff.md, ../../phase-2/findings.md, and ../../phase-2/opt-2-packed-cas.md;
11. knowledge/canonical-objects.md;
12. the complete current production path under crates/layerfs-core/src/;
13. crates/layerfs-engine/Cargo.toml and crates/layerfs-engine/src/lib.rs;
14. the relevant current evaluator path under tools/layerfs-eval/; and
15. every caller located by repository-wide searches for Object, ObjectId,
    ChunkId, LogicalFile, ChunkReference, TreeNode, Metadata, RootHandle,
    Delta, DeltaEntry, RootRecord, and DeltaRecord.

Also read the directly relevant architecture, storage, and failure material
under these read-only trees:

/Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment

Follow references only where they constrain canonical identity, CDC, COW,
root/delta semantics, bounded storage, reopen, range reads, or backend
compatibility. Historical material is diagnostic, not controlling authority.

Mandatory subagent research before editing
==========================================

After the main agent has read the controlling instructions, immediately launch
as many read-only subagents in parallel as the available slots permit. Use at
least three concurrently when capacity allows, then reuse or launch additional
agents until every workstream below is covered.

Subagents must not modify the worktree. They may inspect source, documents,
tests, and retained benchmark artifacts and run non-mutating analysis. Avoid
Cargo contention while audits are active.

Do not edit logical-persistence.md or any other file until all
research reports have returned and the main agent has explicitly reconciled
them.

Workstream A — exact semantic and caller inventory
--------------------------------------------------

Trace every field, invariant, constructor, mutation, consumer, and identity
for:

- Object, ObjectKind, ObjectReference, DirectoryEntry, and canonical codec;
- ObjectId, ChunkId, their domains, and streaming hash paths;
- LogicalFile and ordered ChunkReference values;
- Metadata, NodeKind, TreeNode, NodeId, RootHandle, and RootId;
- Mutation, MutationResult, Delta, and every DeltaEntry variant; and
- engine ObjectRecord, RootRecord, DeltaRecord, capture, reopen, and range APIs.

Produce a field-by-field persistence inventory with exact source locations.
Distinguish semantic data from derived, provisional, cached, process-local, or
engine-specific state. Identify any currently unstated invariant that would
make a durable format ambiguous.

Workstream B — Phase 1 composition audit
----------------------------------------

Audit the frozen Object::Bytes and Object::Directory encodings, limits,
canonical ordering, identity domain, streaming behavior, and authentication.

Determine whether those two object kinds can safely and boundedly express the
complete Phase 3 mapping. Do not presume that they can or cannot. Analyze at
least:

1. typed envelopes stored as Object::Bytes and linked by Directory objects;
2. typed envelopes represented entirely through Directory names/references;
3. a hybrid of Directory structure and versioned Bytes payloads;
4. a genuinely new ObjectKind; and
5. any simpler existing representation found in the codebase.

For each, identify canonical-byte implications, object-count/byte
amplification, collision or type-confusion risk, authentication boundaries,
range-read behavior, compatibility, and whether Phase 1 would have to change.

Workstream C — Phase 2 content and identity audit
-------------------------------------------------

Audit how FastCDC, raw ChunkId, chunk length, canonical Object::Bytes identity,
LogicalFile edits, reuse authentication, bounded rejoin, and range reads work.

Test the proposal that every durable file reference needs raw ChunkId, raw
length, and canonical Bytes ObjectId. Prove which fields are semantically
necessary, which are derivable only by reading payload bytes, and what would be
lost by omitting or duplicating them.

Analyze reference/page sizing, maximum counts, edit locality, streaming decode,
and exact cross-chunk range behavior. Reject a layout that requires a
source-sized manifest, an unbounded object-ID map, or full-file reconstruction
for a bounded range read.

Workstream D — Phase 3 tree/root/delta audit
-------------------------------------------

Audit every NodeKind, Metadata field, tree ordering rule, mutation, delta
variant, root/parent invariant, replay rule, and provisional identity path.

Determine exactly what must survive close/reopen to reconstruct the same Phase
3 value and apply/replay deltas safely. Look for fields embedded in TreeNode or
Delta that make naive recursive encoding cyclic, redundant, or unbounded.

Compare at least:

- recursive node envelopes;
- content-addressed child-node DAGs;
- separate file/directory manifests;
- root envelopes referring to one authenticated node; and
- delta encodings that embed state versus refer to before/after identities.

State how each design affects COW locality, stable identity, delta size,
closure traversal, replay, and future compatibility.

Workstream E — bounded layout and alternatives analysis
-------------------------------------------------------

Develop and compare concrete byte-layout alternatives. Include at least:

1. one object per complete file manifest;
2. fixed-capacity paged file manifests;
3. tree/fan-out manifest pages;
4. directory-as-page versus one directory object;
5. typed Bytes envelopes linked by canonical Directory objects;
6. a new first-class object kind; and
7. any smaller layout discovered from existing project patterns.

For each alternative report:

- exact lookup and range-read complexity;
- object and reference count amplification;
- bytes added per chunk reference and tree node;
- encoding/decoding allocation bounds;
- maximum object/page sizes;
- COW edit amplification;
- closure traversal and authentication cost;
- malformed/cycle/depth handling;
- forward-version behavior;
- Memory/SQLite/remote-store suitability; and
- whether it introduces speculative machinery or hidden compaction/indexing.

Do not choose a layout merely because it is elegant. Prefer the least format
surface that satisfies all current contracts with honest bounds.

Workstream F — correctness, security, and compatibility audit
-------------------------------------------------------------

Threat-model the candidate mappings for:

- type confusion and cross-domain identity reuse;
- non-canonical alternative encodings;
- reordered or duplicate references/entries;
- truncated, trailing, or oversized fields;
- integer and allocation overflow;
- cycles, excessive depth, and excessive closure work;
- mismatched raw ChunkId, length, and canonical Bytes ObjectId;
- root/parent/delta mismatch and replay against the wrong base;
- malformed data reachable only after reopen;
- future version ambiguity; and
- platform-, allocator-, Rust-layout-, or iteration-order dependence.

Map every failure to the existing typed error vocabulary where possible.
Identify any missing error distinction, but do not add production errors in
WP4. Explain whether unknown versions fail closed and how future readers avoid
mistaking a new encoding for the current version.

Workstream G — engine and remote-compatibility audit
----------------------------------------------------

Trace the current SQLite transaction, object, root, delta, reopen, and range
boundaries. Determine what a shared mapping must expose without coupling core
semantics to rusqlite or to InMemoryCas.

Evaluate the mapping against three authorities without implementing adapters:

- in-process Memory;
- local durable SQLite; and
- a plausible high-latency remote object/KV/database authority.

Identify which operations can be batched, which identities can be computed
client-side, what must be atomic, what can be retried idempotently, and what
requires authentication after fetch. Do not invent an engine trait, sync
protocol, remote cache, or third backend. This is a compatibility test of the
format, not authorization to build infrastructure.

Workstream H — golden vectors and mini-benchmark critique
---------------------------------------------------------

Design an independently checkable golden-vector method and critique the
proposed M4-1 100 MiB in-memory mapping round trip.

Determine whether one 100 MiB row is the smallest useful diagnostic, whether a
smaller structural case is required first, and whether the retained repetitive
fixture sufficiently exercises unique-object creation, repeated references,
paging, closure, and ranges. Compare reuse of existing evaluator fixtures with
creating any new fixture.

For the recommended mini-benchmark define:

- exact fixture and fingerprints;
- exact logical tree and create delta;
- included and excluded work;
- timer start and stop;
- warm-up and measured iterations;
- stage timings and work counters;
- memory observations;
- exact correctness self-gates;
- which existing evaluator helpers can be reused; and
- claims it authorizes and explicitly does not authorize.

The benchmark contract may accept, modify, or reject M4-1. It must not be
selected merely because it appears in this prompt. No new benchmark code or
new performance result belongs to WP4.

Mandatory research reconciliation
=================================

After all reports return, explicitly reconcile them before editing. Produce a
working decision record containing:

1. verified facts and exact source evidence;
2. assumptions in the current proposal that are false, ambiguous, or
   unproven;
3. correctness and bounded-resource blockers;
4. an alternatives comparison table;
5. the selected mapping approach and rejected alternatives;
6. why the selected approach is the smallest one that satisfies the contracts;
7. exact remaining questions, if any;
8. the chosen golden-vector derivation/checking method; and
9. the selected, modified, or rejected mini-benchmark contract.

If the reports expose a material conflict between controlling Phase 1, 2, or 3
contracts, stop and report it rather than inventing a compromise. If a mapping
choice is still materially underdetermined, leave WP4 pending and identify the
smallest experiment or user decision needed.

WP4 deliverable
===============

Only after reconciliation, create:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/mapping/logical-persistence.md

This is an exact format specification, not a conceptual architecture essay.
It must contain enough information for an independent implementation to
produce byte-identical results without reading Rust struct layout.

At minimum include:

1. authority, status, scope, non-goals, and compatibility promise;
2. terminology and separation of ChunkId, ObjectId, NodeId, RootId, and delta
   identity;
3. complete persisted-field inventory with source owners;
4. selected layout and evidence-backed rationale;
5. rejected alternatives and concise rejection reasons;
6. exact domain and version bytes;
7. exact integer widths, endianness, booleans/options, lengths, and tags;
8. exact file, directory, metadata, root, and delta byte grammars;
9. exact chunk-reference fields and canonical order;
10. exact page/fan-out format if paging is selected;
11. exact identity derivation for every persisted entity;
12. exact strong-edge graph and traversal order;
13. cycle, depth, object, reference, byte, and decoded-allocation bounds;
14. canonical encode and fail-closed decode rules;
15. exact typed malformed/overflow/identity/version error mapping;
16. reopen, reconstruction, and bounded range-read algorithms at the semantic
    level;
17. forward-version and unsupported-version rules;
18. golden success and failure vectors;
19. mini-benchmark contract; and
20. WP4 acceptance checklist and WP5 implementation boundary.

Exact-byte requirements
=======================

Do not use phrases such as "serialize the metadata" or "encode the delta"
without defining every byte. Use byte-offset or grammar tables for every
envelope. Specify whether lengths include headers, whether IDs are raw 32-byte
digests or another representation, and whether optional values use a tag.

For each persisted entity specify:

- field order;
- field width;
- allowed values;
- maximum value;
- canonical ordering;
- identity/hash domain;
- strong versus non-strong references; and
- exact rejection behavior.

Do not change the already frozen Phase 1 canonical bytes or hash domains. If
the research concludes that a new ObjectKind is necessary, the specification
must include a concrete proof that existing kinds cannot meet the contracts,
the exact Phase 1 compatibility impact, and why a subordinate Phase 4 record
has authority to request that change. Otherwise retain the existing kinds.

Golden-vector requirements
==========================

At minimum freeze:

- empty file;
- one-chunk file;
- multi-chunk file with different raw ChunkId and canonical Bytes ObjectId;
- empty directory;
- nested directory with deterministic ordering;
- every persisted metadata boundary;
- root with and without parent;
- one example of every delta operation;
- maximum valid reference page if paging exists;
- fragmented-input equivalence where relevant;
- truncation at every structural boundary;
- trailing bytes;
- reordered and duplicate entries/references;
- unknown version/tag;
- malformed IDs and identity mismatch;
- count/length/allocation overflow; and
- cycle/depth/reference-limit failures where representable.

For every successful vector include exact encoded bytes, expected BLAKE3
identity, reconstructed semantic value, and strong-edge list. Derive vectors
with an independently reviewable method. Do not let one unreviewed prototype
both define the format and certify its own outputs.

Mini-benchmark contract
=======================

WP4 defines the measurement contract but does not implement or run the new
mapping benchmark. The final specification must explain that distinction.

The benchmark should be the smallest row that can expose mapping CPU, copying,
hashing, metadata amplification, closure work, range behavior, and bounded
memory. It must use existing fixtures/helpers when they satisfy the research
decision. It must not create a new benchmark framework.

If the selected design retains a 100 MiB single-file mapping round trip, the
timer must include the agreed streaming source read, CDC, raw and canonical
identity work, object creation/reuse, mapping/root/delta construction, closure
validation, decode, full reconstruction, and exact ranges. Source generation
and fixture preflight remain outside. Record direct stage counters, one warm-up,
at least five measured iterations, median/spread, and explicit NotApplicable
durability/reopen labels for the Memory diagnostic.

The benchmark must never authorize a 200 or 300 MiB/s durable claim. It is an
unoptimized shared-mapping diagnostic. The full Memory/SQLite create/edit and
durability campaign remains WP8-WP9.

Non-negotiable constraints
==========================

Preserve:

- Phase 1 canonical-object bytes and identity;
- Phase 2 CDC profile, boundaries, and fragmentation independence;
- Phase 3 COW, root, and delta semantics;
- immutable authenticated CAS and authentication before reuse;
- exact bounded range reads;
- typed errors and checked arithmetic;
- atomic root/delta publication requirements;
- streaming source processing;
- explicit memory/reference bounds; and
- deterministic results independent of platform and Rust layout.

Do not add or implement:

- a production codec during WP4;
- a new engine abstraction, factory, or provider system;
- MemoryEngine or SQLite integration;
- another custom carrier, packed CAS, arena, or index;
- a third database or remote adapter;
- async, workers, queues, retries, pools, or caches;
- compaction, GC, migration, checkpoint, rollback, or materialization work;
- a benchmark implementation or new performance claim; or
- speculative configuration/version negotiation beyond the exact format rule.

Keep the specification narrow. Do not design a general filesystem or database
protocol when a deterministic logical-object mapping is sufficient.

Implementation-plan and evidence updates
========================================

After the mapping record passes the reconciled WP4 review:

1. correct the known final-state inaccuracies in
   ../rollback/deletion-record.md;
2. update the WP3 ledger fingerprint to f595046;
3. mark WP4 complete in the implementation ledger with the exact mapping
   specification fingerprint and review evidence; and
4. leave WP5 and every later work package pending.

If WP4 is blocked or materially underdetermined, do not mark it complete.

Verification
============

The expected WP4 diff is documentation-only. Inspect it directly:

git status --short
git diff --stat
git diff --check
git diff -- logical-persistence.md \
  ../rollback/deletion-record.md \
  ../rollback/implementation-plan.md

Confirm no Rust, Cargo, lockfile, database, evaluator, fixture, or retained
historical-result file changed. Confirm every local Markdown link resolves.

Run existing read-only tests only when needed to verify a claimed current
contract or golden dependency. Because WP4 is documentation-only, do not run a
broad Cargo wall merely for ceremony. If Cargo is used, use one process at a
time and report the exact nonzero test count.

Perform one independent read-only final audit after the source stops changing.
The auditor must check exact-byte completeness, identity-domain separation,
bounds, error coverage, golden-vector independence, and consistency with the
Phase 1/2/3 source contracts.

Required final report
=====================

Report:

- subagents launched and the workstream each covered;
- reconciled findings and disagreements;
- assumptions rejected or left unproven;
- alternatives considered and the decision table;
- selected mapping and why it is the smallest correct choice;
- exact domains, identities, bounds, traversal, and compatibility rule;
- golden-vector method and fingerprint;
- selected/modified/rejected mini-benchmark contract and why;
- documentation files created or corrected;
- exact starting and final Git/source fingerprints;
- exact verification commands and outcomes;
- any test not run and why;
- unresolved questions or WP5 risks;
- confirmation that no production code or benchmark was implemented; and
- whether WP4 is complete and ready for WP5.

Stop after WP4. Do not start WP5 even if the mapping appears straightforward.
```

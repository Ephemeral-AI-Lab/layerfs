# Bounded metadata reuse inside one independent proof

Prospective changed verifier; no original receipt promotion. Product unchanged.

History53 reference at16ca22090 finishes performance83.107392291s within170s but
proof times out9.502048166s. The shared verifier re-requests immutable metadata
for every root and looks up all file metadata even when `Reuse.lengths` already
contains that authenticated root. The latter rows are immediately skipped.

Add a verifier-only metadata reader, empty at one proof invocation start, with
2MiB canonical bytes/512entries and LRU eviction. Only namespace metadata walk
uses it; file content sampling and all source oracle comparisons use the original
reader. Miss bytes must come from the actual authenticated source and are hashed
against their requested ObjectId before admission. Bodies above the existing
8KiB inode-node cap are not retained. No oracle bytes or expected producer data
prefill this cache. Each state still walks the complete namespace and compares
all path/kind/size and every declared sampled digest, and C5 custody plus original
owner preservation remain. File metadata lookup excludes lengths already checked
under the same immutable ID, matching the existing skip in the validation loop.

This is an additional bounded verifier work buffer, not an increase in product
body/decoded/cache/queue/worker limits. Lifetime one entire proof command; no reuse
from prior command, setup, another arm or other phase. First acquisition is paid
inside that same proof command. Separate proof9.5s and product cold contracts are
unchanged. Digest/length reuse already exists in this proof; this adds metadata
reuse without skipping any oracle comparison. Sealed reference/candidate vehicles
share the exact module; new harness identity requires new matched qualification.

External helper tests cover mixed hits/misses/order/duplicates, identity/missing
refusal, fixed bounds/eviction and fresh invocation empty state. Compilation-only
argument/constant/trait import defects corrected from diagnostics; no performance
arm repeated. Focused owning checks and diagnostic measurements are recorded below.

Prospective one native-only53reference count diagnostic on retained original
closed Store/producer/census/metadata, source/database cold equally attested,
9.5s native/60s complete. No performance rerun, qualification, or old proof
promotion; per-state memo counters and read work attribute the mechanism. If it
fits, new ordinary paired selections still need the whole combined proof.


Focused checks:4external helper tests PASS (3contracts then one new short-batch
refusal); verifier example/helper test Clippy-Dwarnings PASS after removing
unnecessary non-Drop lifetime calls;8history facade/runner/provenance tests PASS.
Package formatting applied. Runtime product source is unchanged; no full-workspace
or new product-boundary verification claim. Freeze and execute diagnostic next.


First frozen count diagnostic atdc7f1726f retains DIAGNOSTIC_TIMEOUT:46/53states
complete,9.513163750s native,26.541444541s complete; source/database cold and owners
preserved PASS. Memo20854hits/39837misses/39325evictions, peak1092229B/512rows.
Row capacity binds below byte capacity. This scope covers all namespace metadata,
including numerous small direct-directory pages. Original failure unchanged;
native-only count timing does not qualify any ordinary proof.

Next changed scope keeps2MiB/512rows/8KiBentry bounds and empty invocation start:
only FilesystemRead root/inode-table navigation uses the memo. Direct directory
reads/listing use the original provider alongside file reads, reducing one-use
entries without expanding any cache or dropping checks. New labelled53native
reference count diagnostic will measure this admission-policy change, not retry
the unchanged failed scope. Product performance still unrun at the new harness.


Narrow-memo5250275fc diagnostic remainsTIMEOUT after48/53; original files preserved,
coldPASS. Memo21610hits/3733misses (~85%hits),561sourcecalls, peak2097152B/437rows.
State48 walk113.381667ms, length/sample acquisition261.798417ms with2900body
acquisitions/306886032requestedVFS bytes. Same limits, more useful reuse, but
file-root acquisition dominates the remaining gap. No ordinary proof promotion.

Third directed mechanism change: shared verifier file roots are ordered by stored
pack hints and split into cohorts of at most8pack hints (existing2MiB cache divided
by256KiB pack limit), still<=512IDs and existing16MiB estimated canonical bytes.
Every requested root and sampled byte remains; cohorting reduces cache churn in
the unchanged reference provider without modifying its product. Candidate hints
come from real locate rows; reference closed-census TSV adds MIN(stored pack_id)
as an advisory fourth field, source facts only. The source still chooses eligible
locators and authenticates canonical bytes. Hints never determine proof results.
The canonical ID/role/length census hash stays unchanged; hints must be positive.
Existing verified-length rows retain their pack hint (8bytes per existing row,
no extra rows); existing pending worklists also retain one hint per requested root.
Product buffers, memo2MiB/512/8KiB, workers and budgets unchanged.

Diagnostic re-exports actual closed-reference metadata at its new schema before
cold attestation, inside the complete diagnostic clock; native-only9.5s still
excludes census/export and cannot admit an ordinary combined proof. Qualifying
whole proof must later pay that work inside9.5s. No source-body prefill/cache
warming survives cold attestation. New single native53reference child declared
at the next frozen hint-cohort identity, not unchanged-scope retry. Example
Clippy and8focused history tests PASS; no full-workspace claim.


## Ordinary53pair atd8b1df33f

The third file-cohort native-only diagnostic timed out after52/53 with observers;
retain it as DIAGNOSTIC_TIMEOUT. It did not promote the original ordinary proof.
A new ordinary paired selection uses the changed verifier with tracing disabled
for the separate proof and pays full census/export/native/preservation work.

| Scope | Reference | Candidate Disposable |
| --- | ---: | ---: |
| Product lifecycle |68.721138333s|80.446455584s|
| Complete performance command |85.368419334s|96.684457584s|
| Independent combined proof |8.655362000s PASS|9.512004250s TIMEOUT|
| Acquisition |22.740203291s|22.737621787s|
| Construction |3.188177418s|3.072327962s|
| Filesystem |14.248526666s|10.423950462s|
| Save/custody |27.876083411s|43.393351043s|
| Allocation |65142784B|62611456B|

Ratio1.170621697129 timeFAIL (allowance75.5932521663s); combined jointINCOMPLETE
because candidate proofFAIL. Both complete performance commands within170s,
cold/root/canonical/storage/cleanup PASS;73447objects/589480854canonicalB match.
Full original manifests independently verified; no runtime pass promoted from
producer roots or census. Earlier reference proof timeout remains unchanged.

Observed product trace including setup/finalization: reference1250866statements/
55993679VM, candidate691365/19089631VM. Candidate native176104transactions with
1228writes =>174876reads. Payload returned7653915810B plus metadata pack returned
5621630117B; these counters have distinct overlapping work scopes, not device-byte
claims. Save/custody+15.517267632s is the largest product gap while filesystem is
faster. Source inspection identifies pack-body SHA256 both in SQLite objects_read
and C2 Fetch authentication on every acquisition. Further product work must retain
an authenticated boundary and public persistence contracts; no hash validation
may simply be deleted on trust. Instrument/centralize that work before another
changed-source pair.157/Durable NOT_RUN; prior17/Init passes stay pinned, no
all-seven current-artifact qualification. Goal ACTIVE.

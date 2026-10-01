# R1d-binding-sites delivery

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Parent: `420e6a2c0149326fe2c380ff0e4156eadd3d326d`.
> Named submilestone exits: COMPLETE in the direct Darwin provider scope below.
> Full R1 remains PARTIAL; R2–R7 and #288 qualification remain open.

The strict prepared Server path now uses combined paged exclusive binding sites
and monotone base facts. It replaces the resident site/name/target-parent/base
binding lists on this route. The ordinary wire-v1 request, canonical-v1 identities,
explicit legacy v1/v2 APIs, Store capacity, construction worker count and native
scratch allowance remain unchanged. SC-03/05/07/08 are affected.

[The frozen contract](R1D-BINDING-SITES-FREEZE.md) specifies the formats, authority,
phase order, admission, failure custody and file ownership. The changed source and
external tests are captured in [SOURCE-FREEZE](evidence/r1d-binding-sites/SOURCE-FREEZE.json):
64 files, path/NUL/bytes/NUL SHA256
`6c9cbdd48c5e0bd12d78136a9805e087af39973ff43d0afe1cf538c3ba97e793`.
The final all-Core test-input closure for owning lint/examples/fmt/guard is
`74ac357c0e136e6d5b4d3b4c95760ba2bbc22370198676832cf5389d85f9d5b6`.
The resulting actual commit belongs in the external issue checkpoint, rather
than a guessed self-referential SHA in this document.

## Delivered authority and work

One existing issuer is acquired by `SpoolPreparation` before native, Save or body
effects. Server transfers that preparation by value into the received spool.
Opaque `BindingSourceId` and issued Point28 bind parent, header descriptor and
binding ordinal to that source. Foreign points fail before provider access;
selected points validate the complete sparse block and adjacent boundaries.
Real spool access decodes at most16 names and does no name search. Source getter
failure/mismatch precedes selected ownership; later errors abandon the selected
scope once. A point does not grant whole-directory or input completion.

C1 table3 uses Source-associated SiteScope89, key25 and frame60. Birth records
close in source order against independently accumulated count, bytes, digest and
maximum. Closing consumes the producer and drops its pending allocation before
facts. Immutable HasBase and monotone SawBase/AnyLegalBase preserve restated-edge,
active-zero and uncertified-base OR semantics. Indexed parent projection restores
names through issued points in ordinal order. Final alias verification and known
retirement precede root/cycle decisions and canonical output. A later checker
failure still terminalizes the selection. Separate canonical-v2 identities and
certified namespace parents are R3 obligations.

C2 private schema3/Header200 binds the same source in native-binding-v3. Sites1
retire before roots2 in one admitted16MiB native file. Primary sites, unique birth
order and partial existing-parent indexes all count toward that allowance.
Birth/facts/final/retirement use bounded transactions with exact selected-record
validation. Final sealing revalidates immutable births in the same transaction
before hashing mutable flags. Retirement is acknowledged only after known COMMIT,
empty projections and native allocation observation. Unknown retains exact
pending rows or observations, scope/source, expected/proposed seals, continuation,
native identity and credit; it permits no resend, guessed adoption or cleanup.

Declared count/frame refusal now precedes native reservation, attempt creation,
queries and BEGIN. A local duplicate still requires decoding every selected
existing row before Duplicate rollback, so corruption is not hidden by that
shortcut. New source code contains no test hook, alternate benchmark route,
third-party modification, retry or expanded quota.

## Independent and provider proofs

228 distinct primary tests passed: Content141, Storage43 and Server44. Empty
process helpers, reused passing bodies and guard self-tests are excluded. Literal
point/frame/seal/membership/page vectors and independent alias facts cover source,
identity, tombstones, permutation, restatement, uncertified OR, duplicate priority,
active-zero and alias-before-cycle order. Actual new construction/update callers
match separately pinned canonical-v1 roots and reachable-object sets.

Real input with4,097 names follows points with at most16 decodes, two adjacent
checks and zero name probes. The129-site case records258 visits, two parent pages,
387 point resolutions and zero historical binding-row reads. These are narrow
implementation work diagnostics, not speed or whole-graph resource admission.

Native tests use Apple SQLite3.51.0, real files and real indexed query plans. The
original65,536 all-base sites under parent1 use3,425 pages. Independent review
identified SQLite's wider INTEGER representation; a distinct new case with parent
`i64::MAX` uses3,932 pages, leaves3,924 free pages after retirement and reuses the
same file for65,536 DirectoryRoots. Both retain allocated16,777,216 bytes and the
4,096-page ceiling. The logical Site60 maximum is3,932,160 bytes; roots63 maximum
is4,128,768 bytes. Native records are54 bytes and the128-record materialization
window is6,912 bytes. These numbers distinguish encoded admission from physical
B-tree costs and native/global heap/cache observations.

Five fresh owned-process lock barriers prove Unknown at birth, close, facts,
final seal and retirement. Actual SHARED/COMMIT lock coordination, not elapsed
sleep, establishes overlap. Server additionally proves real257 fresh/duplicate
bindings,129 stored-site permutation, surviving-parent refusal, prior Stage and
base-byte preservation, two occupied Save slots with catalog progress, and scratch
Unknown with both known Save abort and actual ownership-cleanup refusal. No roots
are adopted or accepted writes rolled back on a guess.

## Exact owning checks and retained red results

All commands ran from the owned repository root, using owned `core/target` and
repository-root ARMv8 AEAD flags. Every command, exit, source inventory and raw
output is append-only in the linked result/log files below.

- [01-storage-contract](evidence/r1d-binding-sites/01-storage-contract-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test construction_sites --test construction_claims --test construction_state -- --nocapture`.

- [02-content-contract](evidence/r1d-binding-sites/02-content-contract-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test binding_points --test binding_site_codecs --test binding_site_validation --test binding_site_reference --test binding_claim_validation --test binding_claim_codecs --test indexed_state --test validation_prefetch --test filesystem_binding_validation --test filesystem_binding_rows --test filesystem_bounds --test filesystem_hardlinks --test filesystem_reference --test filesystem_topology --test filesystem_updates -- --nocapture`.

- [03-storage-maximum-parent-width](evidence/r1d-binding-sites/03-storage-maximum-parent-width-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test construction_sites unix::maximum_width_all_base_sites_two_indexes_retire_then_maximum_roots_in_same_class -- --exact --nocapture`.

- [04-server-composition](evidence/r1d-binding-sites/04-server-composition-result.json): exit101. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test binding_claims --test prepared_bindings --test catalog_admission --test history -- --nocapture`.

- [05-server-catalog-header-correction](evidence/r1d-binding-sites/05-server-catalog-header-correction-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test catalog_admission allocator_finishes_with_two_real_save_slots_occupied_and_a_third_save_is_refused -- --exact --nocapture`.

- [06-server-scratch-header-correction](evidence/r1d-binding-sites/06-server-scratch-header-correction-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test catalog_admission scratch_unknown_preserves_publication_and_reports_owned_save_cleanup -- --exact --nocapture`.

- [07-server-unrun-targets](evidence/r1d-binding-sites/07-server-unrun-targets-result.json): exit0. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test prepared_bindings --test history -- --nocapture`.

- [08-owning-clippy](evidence/r1d-binding-sites/08-owning-clippy-result.json): exit101. `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-content -p layerfs-storage -p layerfs-server --all-targets -- -D warnings`.

- [09-owning-clippy-correction](evidence/r1d-binding-sites/09-owning-clippy-correction-result.json): exit0. `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-content -p layerfs-storage -p layerfs-server --all-targets -- -D warnings`.

- [10-owning-examples](evidence/r1d-binding-sites/10-owning-examples-result.json): exit0. `cargo +1.85.1 build --manifest-path core/Cargo.toml --locked -p layerfs-content -p layerfs-storage -p layerfs-server --examples`.

- [11-core-fmt](evidence/r1d-binding-sites/11-core-fmt-result.json): exit0. `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`.

- [12-product-boundary](evidence/r1d-binding-sites/12-product-boundary-result.json): exit0. `python3 core/tools/check_product_boundary.py`.

- [13-guard-selftests](evidence/r1d-binding-sites/13-guard-selftests-result.json): exit0. `python3 -m unittest discover -s core/tools -p test_check_product_boundary.py`.

04 failed because the external catalog observer still expected Header192,
while schema3 correctly returned Header200. Its blocked scratch child and unrun
prepared/history bodies remain explicit. Only the observer expectation and unused
product import were corrected;05/06 cover the two affected bodies and07 runs the
previously unrun targets. 08 Clippy rejected three non-owning external wrapper
drops; their removal uses ordinary NLL without changing assertions or ownership.
09 passes. Passing unchanged test bodies were reused. Original source bytes and
hashes are retained in [source captures](evidence/r1d-binding-sites/14-original-source-captures.json).
[Independent review](evidence/r1d-binding-sites/15-independent-review.json) records
admission-before-effects, corruption precedence and parent-width corrections.
The reviewer performed static review; provider bodies were executed by the root.

Owning examples were built without running workloads. Whole-Core formatting,
442-file production boundary and nine guard self-tests passed. Unchanged SDK and
counter tests are reused; required full Core tests/examples/lint and all tool
self-tests remain final R7 checks. No CI, retired preflight or benchmark ran.

## Remaining gates and qualification handoff

Graph seen/frontier/cycle/reachability, memo/unreachable, reference/touched/final
row and release populations remain resident. Repeated per-seed cycle walks need
algorithmic replacement rather than a disk copy. Sites Facts overlaps the alias
graph; its admission must account for their simultaneous owners. Later graph
phases can reuse the retired sites file, but this delivery does not grant that
reuse or enlarge the profile.

Global SQLite heap, native cache, RSS, physical containment, healthy protected
progress and eligible native Server startup remain incomplete. Apple hardlimit
readback0 still explicitly refuses native startup; the direct library proofs
above do not override that capability refusal. Linux and other real-provider
bodies remain unrun. The inherited receive/spool/multiple-cleanup custody gap
remains for R3/R4: ignored cleanup failure or early Drop does not prove complete
cleanup. Workspace v3, full Commit/G1/G2/READY, command/FUSE ownership, concurrency
and cutover remain R2–R7 gates.

#288 must qualify changed point, site, projection and retirement mechanisms from
this exact implemented source after its prerequisites pass. Existing receipts
retain their original source and verdicts. No campaign, new family, routine
Family2 rerun, #288 issue edit, speed PASS or release admission is claimed.

Next: select a complete paged graph-authority algorithm and explicit simultaneous
native/window admission from the source audits; freeze shared interfaces and
independent graph expectations before product edits.

## Production LOC

The exact first-parent/final-staged archive comparison is recorded in
[PRODUCTION-LOC](evidence/r1d-binding-sites/PRODUCTION-LOC.json). Exact comparison:

```text
Production LOC: 143756 -> 147464 (delta +3708)
Reference: 65417 -> 65417 (delta +0)
Core: 78339 -> 82047 (delta +3708)
Method: tools/production_loc.py blob a1cb6c064dd150dbc3aa19648a3748b5aeb741b5;
SHA256 0d798f53263c0f18636d96ccdfdc3728f501f53b066e90577d251859f1f117b2;
git archive first-parent/final staged tree crates core/crates, same scan/per_file;
production Rust + shipped SQL, excludes test-only/inline tests, external tests,
examples/fixtures/tools/docs/generated/third-party inputs.
```
 Reference code is preserved; new Core
site authority temporarily coexists with compatible legacy paths. This is an
implementation addition and selected-route replacement, not legacy retirement.

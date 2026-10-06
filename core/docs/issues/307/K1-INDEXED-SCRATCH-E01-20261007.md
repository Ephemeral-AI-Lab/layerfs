# Indexed scratch, lazy file-edit Plan and E01 receipts

> Status: committed locally as `889836c446507c726a53f0ccf1e4418bd4d0946f`;
> S7/S9/S10 remain incomplete and
> unchecked. First parent `d6d9ad5a3f3dcbc52559ef5ac37f8839d275161f`.
> Product source tree `f0b2facf66b3cb76489b91bbfaefdd1c7e55a08e`; final commit
> tree `18de8ef21f34523e918955a38a9ce675a410291a` matches the staged tree.
> [Committed source/count confirmation](checks/k1-lazy-indexed-20261007/47-committed-verification.json)
> and [separate tracker receipts](checks/k1-lazy-indexed-20261007/48-tracker-receipts.json)
> are retained by the next checkpoint; original proof/source pins remain.

This checkpoint implements the neutral local record provider needed by the
[K0/K1 proposal](K0-K1-BACKED-EDIT-DESIGN-20261007.md), the lazy small-result edit
Plan and the first external collector under the already committed
[E2 specification](../../../../docs/roadmap/0.1/0.1.7/cluster-two-e2-job-receipts-v1.md).
It does not implement the backed Content editor or captured Commit. The root
reference remains intact. No speed/storage treatment is sampled or requalified.

## Product changes and actual limits

Content now advances the stable validated edit Plan lazily when assembling a
WholeFile, reusing one RangeCursor. Replacement data can be consumed before later
rows; the first original source failure stops future rows and emits no final
object. Final cutoff, allocation, canonical roots and predecessor rules remain.
The state/codec/lifetime helpers move from tree.rs into objects.rs. Independent
source comparison found the tree algorithm byte-identical after the extraction;
this is relocation, not an algorithmic source-size simplification. Memory-profile
Draft/parent/detached/resolved/emitted collections and the 8 MiB deferred limit,
u32 keys and sparse byte scanning remain. The new public bodies cover early
replacement demand, first failure and a 3 MiB chunked input reduced to 2 bytes
through 4,097 deletion rows. Source establishes the one-cursor path; the third
body alone does not distinguish the earlier resident Plan.

Overlay schema15 adds indexed raw operation records in the existing daemon
SQLite database, keyed by the full namespace/operation/file-scope/kind/ObjectId.
Membership projects no BLOB, get returns one bounded record, guarded sorted
unique changes check every original expected value before effects, and the
first 64-key window excludes one retained root. Capacity-based 65,536-byte
admission has no 64-change cap. Operation release enqueues the indexed domain;
live and terminal cleanup use bounded metadata windows and exact full-key deletes.
Accounting triggers charge raw value rows/bytes. Daemon retains original typed
unattempted commands and attempted Completions with existing service credits.
[The source architecture](../../architecture/49-indexed-operation-scratch.md)
states exact custody and limits. Content has not supplied the raw draft codec or
adapter, so this provider alone removes no Content state cap.

## SQL count diagnostic

One fresh host operator diagnostic retained [source/binary/command identity](checks/k1-lazy-indexed-20261007/profile-source.json),
[original output](checks/k1-lazy-indexed-20261007/13-host-indexed-profile.stdout)
and [reviewed arithmetic](checks/k1-lazy-indexed-20261007/profile-counts-reviewed.json).
Cache was uncontrolled; there is no comparable speed arm, cold claim, residency
claim or numerical acceptance. Its complete wrapper wall was 1,334,710,333 ns.
The retained database is the ignored owning artifact under
`core/target/cluster2-307/k1-indexed-profile-20261007/overlay.db`.

| Scratch operation | Attempts / executions | Returned rows / BLOB bytes | VM steps | Bound bytes |
| --- | ---: | ---: | ---: | ---: |
| Contains | 1 / 1 | 1 / 0 | 20 | 64 |
| Get | 1 / 1 | 1 / 8,192 | 20 | 64 |
| First keys | 1 / 1 | 64 / 2,048 | 469 | 64 |
| Guarded update | 2 / 3 | 1 / 8,192 | 183 | 4,224 |

Update reports one direct change and three trigger-inclusive changes. Scratch
release counters are zero because lifecycle/reclamation work belongs to other
families. The 20 explicit maintenance turns reclaimed 1,090 actual scratch rows,
at most64 per turn, and4,096 raw value bytes in total. All-family maintenance
reports1,210 attempts/2,302 executions,1,130 returned rows,3,276 trigger-inclusive
changes/1,092 direct changes,43,600 returned BLOB bytes,62,320 value bytes,
141,909 VM steps and71,376 bound bytes. The two extra direct changes delete
control garbage rows; they are absent from reported scratch row counts.
The original derived note claiming otherwise remains retained separately.

Resources change scratch1,090/4,096 ->0/0 and operation owners1 ->0. Shared logical
file270,336 bytes, allocated/high-water268,701,696 bytes and66 database pages
remain; freelist2 ->19. Reserved tail268,431,360 and cleanup capacity134,217,728
bytes remain separate. This direct Overlay maintenance pump does not prove
automatic Daemon cleanup; the external Daemon bodies exercise that route.
Exact EQP/VM programs and runtime totals are retained. Indexed visited rows are
unobserved; fullscan=0 is not a substitute. No physical shrink, exact eligible
debt, phase cache, exclusive CPU or latency causality follows.

## E01 external collector

The example performs one actual `Owner::start_observed` with defaults, records
the original creation/profile/allocation/error outcome, takes two separate
post-ready diagnostics and explicitly stops the owner. It creates no Store,
Root, Route or dummy Workspace; jobs.jsonl is empty with explicit unrun status.
Global persistence is NOT_IN_SCOPE. The original DB artifact remains retained.
Recording uses bounded windows and preserves acknowledged prefixes/original
failures; no restart, truncation or replay. The process-local invocation and IDs
exist before startup. Source/base tree/current inventory, actual binary digest,
command inputs and observed applicability are bound in the external identity.

The independent validator checks exact nested fields, family totals, original
errors, exact binary/input paths and trimmed original identity bytes. Its PASS
means diagnostic receipt consistency only: uncontrolled cache, admission_eligible
false, qualification NOT_EVALUATED, all27 E1 samples NOT_RUN/count0. Docker input
path mapping remains UNAVAILABLE; a Docker collector cannot acquire consistency
PASS through guessed host/container aliases. Exact eligible observers, calibration,
physical I/O, residency, copies, queue-only peaks, eligible debt and isolated wait
remain implementation/registration work. E04/E05 are unimplemented.

## One actual E01 command and separate consistency proof

[The original diagnostic command](checks/k1-lazy-indexed-20261007/40-e01-native-original-command.json)
completed in1,108,598,167 ns under the15s stop. The
[separate read-only validator](checks/k1-lazy-indexed-20261007/41-e01-independent-validator.json)
completed in148,156,208 ns under9s and returned observation_consistency PASS,
qualification NOT_EVALUATED, admission_eligible false, E1 NOT_RUN/count0.
[Original output and metadata](checks/k1-lazy-indexed-20261007/diagnostic/50b1c7c2873592a4ae069b25bbc6497b408fb5c4f86f9c65adc1e4071ffb3aba/manifest.json)
and [derived index](checks/k1-lazy-indexed-20261007/e01-observations.json)
retain every actual stream/hash, original outcomes and scope.
This is the first original native-host diagnostic, not an eligible measurement.
No Linux/Docker E01 command ran; portable Rust tests are a separate check.

Owner start-to-ready reports7,758,250 ns; original creation7,464,417 ns overlaps
that span. The collector entry-through-stop-and-log prefix reports175,121,958 ns
and excludes final metadata encoding/writes, so it cannot replace the external
complete-command wall. Original startup observes one create/open, two connection
configuration calls and one cache configuration call,105 SQL attempts/executions,
88 returned rows/1,519 column bytes/0 BLOB bytes,2,078 VM steps,9 sorts and one
direct/trigger-inclusive changed row. Fullscan/autoindex/reprepare/bound bytes0
retain their actual source scope. SQL statement memory sample bytes412,080 sum
samples and are no process peak. One admitted range call requests268,435,456 bytes;
three allocation observations retain logical192,512/allocated and high-water
268,435,456/reserved tail268,242,944/cleanup capacity134,217,728 bytes.
The separate post-Stop artifact stat matches the retained one-link DB and its
192,512 logical/268,435,456 allocated bytes. Neither reservation/high-water nor
metadata proves phase residency or physical I/O. Both diagnostics endpoints
observe admitted/outstanding/credited/peak credits0, as no route/job was created.

Source identity remains first-parent HEAD/tree plus current inventory,
source_sealed false. Binary b9c96c8329c83f5bd1704779c2805f04cdd3e9b4218f9f665ba08a71be657828
(4,362,632 bytes) and immutable source copies are retained under ignored local
owned paths named by the original metadata. Preparation metadata/helper copies
are [retained with the report](checks/k1-lazy-indexed-20261007/e01-preparation-metadata/).
Native topology/toolchain were actually read; original build environment is
UNAVAILABLE in the wrapper, not inferred from the preparation process.
No global Store, Docker domain, root topology, observer calibration or E1 source
seal is substituted. Existing Init/history verdicts and all27 sample rows remain.

## Checks, failures and continuation

[Append-only raw receipts](checks/k1-lazy-indexed-20261007/) retain every command,
wall stop, original stdout/stderr and result. Host/Linux selected bodies include
81 Content,39 Overlay and18 Daemon bodies on each platform, plus7 E01 Rust bodies
on each platform,36 affected Workspace bodies on each platform, and37 Python
validator bodies. Host/Linux Content/Overlay/Daemon
all-target warning-denying Clippy and locked builds cover the active route.
The boundary guard scans691 production Rust/SQL files;41 tool self-tests pass.
No test timed out. The changed-schema real host/Linux proof passed both profiles in
78,525,374,041 ns (functional120s outer ceiling; unchanged40s consumer/45s
host profile bounds). Each profile delivered131 real host responses, checked
13 native metadata paths and7 regular-file names, preserved the original remote
refusal, observed automatic idle and terminal cleanup, and fenced262 receive
messages with live_messages0/credited_bytes0.
[Original proof/output](checks/k1-lazy-indexed-20261007/38-real-host-linux-final-proof.json)
and [retained Linux binary](checks/k1-lazy-indexed-20261007/linux-consumer-binary.json)
are distinct from the unchanged R4 receipts. Binary SHA256
9ad2a594cc424c1d14fa2d5f50a08cdd73e1bfebf3e98f44ce13c812ac158a0a
(71,081,264 bytes) is retained locally. This remains a small functional oracle,
not scale, full topology, crash/unknown recovery, physical shrinkage or
performance acceptance.

The first Daemon affected selection retained a schema14 expectation failure
(actual15) in owner.rs after8 unchanged owner bodies passed. Updating that exact
assertion and building first allowed its one failed body to pass; the later3
startup/3 upstream bodies were previously unrun. No product fallback occurred.
The first Daemon Clippy retained an existing scalar SHA256 range-loop warning
from the external harness module imported by the new example and test. A scoped
lint allowance on only that unchanged external import retains its code/identity;
product warning-denying checks remain. Original failure receipts are immutable. The full staged whitespace scan also
reported blank EOF lines in14 immutable original Cargo stdout files. Their
bytes remain unchanged; the authored source/docs/harness scope is checked with
raw stdout/stderr excluded. This is an artifact-format observation, not a
product test failure or hidden trimming of a receipt.
Review also corrected pre-start receipt IDs, command/input binding, exact nested
fields and original error equality before tests; the final copied-identity
negative preserves original input bytes while consistently rehashing an altered
copy. Those source-review findings are corrections, not invented runtime failures.

Ready work continues into the private fallible Content state interface, explicit
stored/draft identities, bounded raw mapping codec, neutral Workspace backing
adapter and captured sparse normalization. K2's streamed directory changes and
root-qualified topology provenance, native FUSE/control/Exec, Save/history/install
and E2/E3 qualification remain. No milestone completion, legacy retirement,
push/release/deployment or aggregate CI/preflight claim follows.

## Exact production source size

Production LOC: 162906 -> 163413 (delta +507).
Core97489 ->97996 (+507); reference65417 ->65417 (+0).
Content14561 ->14564 (+3), Daemon2296 ->2388 (+92), Overlay6567 ->6979 (+412).
Every other product subtotal is unchanged. The full totals include excluded
first-party predecessors. Content's state extraction is relocation; no old
reference or predecessor is retired, and no algorithmic shrink is claimed.

[Exact snapshot comparison](checks/k1-lazy-indexed-20261007/production-loc-source.json)
uses first parentd6d9ad5a3f3dcbc52559ef5ac37f8839d275161f and staged product
f0b2facf66b3cb76489b91bbfaefdd1c7e55a08e, with `git archive TREE crates core/crates`
and unchanged `tools/production_loc.py --root ARCHIVE --json`, SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Nonblank/noncomment Rust and shipped SQL count; tests/inline tests/examples/docs/
tools/harness/third-party do not. New E01 collector/validator artifacts are
external examples/harness/tests and therefore excluded. Final staged and committed
product roots/counts are confirmed with the same method.

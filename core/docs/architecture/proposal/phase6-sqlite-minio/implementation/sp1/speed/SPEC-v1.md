# SP1 component speed screen v1

> Status: Dated planning checkpoint; not release evidence or a product contract.

Owner authorization: "proceed with the 4", 2026-10-02, under #295/#293.
Starting source03ae2f0526182726c1977cfc9449eb392f8c05a3. This is an exploratory
component screen, independent of S2. It does not qualify SDK/Exec/FUSE, cloud,
whole-owner memory, retained-history strides or a released product speedup.

## Contract and selection

Use the existing core/benchmark/fs-bench-pro/runner.py, one canonical family
module sp1_components.py and a benchmark-only release example in phase6-live.
No second framework, product scenario branch or historical receipt relabel.
Publish this spec commit before implementation/collection. One observation per
registered row, fixed order below, no n3, distributions or unchanged retries.
All rows have admission_eligible=false. FAIL/INELIGIBLE/NOT_RUN remain visible.

| Order / row ID (prefix sp1-speed-v1-) | Investigation | Operation and immutable input |
| --- | --- | --- |
| 1 metadata-local | 1 metadata route | Fresh local SQL/C2 owner captures and reconstructs sealed pool0 canonical,3284B, from prepared closed metadata fixture |
| 2 metadata-native | 1 metadata route | Same semantic capture/read via authenticated P6SP1V2 native server on host; first connection/authentication included |
| 3 small-new-full | 2 small save; shared FULL-write control for3 | New Writer, offer sealed whole1 canonical96023B without predecessor into empty clone, finish_storage ACK |
| 4 small-exact-reuse | 2 exact reuse | Same owner initialization and offer sealed whole0 canonical96023B against its committed FULL clone, finish_storage ACK |
| 5 payload-prefix-write | 3 PREFIX write | Offer sealed whole1 with whole0 OriginalBase into committed FULL clone; existing selector must choose PREFIX |
| 6 payload-full-read | 3 FULL read | Fresh ReadOwner captures and reconstructs sealed whole0 through real MinIO+SQL |
| 7 payload-prefix-read | 3 PREFIX read | Same route for sealed whole1, requiring its retained whole0 base |
| 8 candidate-hydration | 4 SQL index | Walk8192 synthetic eligible ring rows in128-row pages; all rows refer to the same independently sealed FULL locator, stamps1..8192, slots stamp-1. This measures bounded SQL selection/paging, not normal writer candidate quality/population |
| 9 metadata-transfer | 4 SQL transfer | Authenticated native register_body(Metadata) of a deterministic legal Native C2 envelope composed of32 copies of the independently sealed pure-metadata Native group in strict-oracle-v2/domain-packs/metadata.sqlite pack3. No locators/ordinals/FS semantics are claimed for this synthetic body. Record exact length/digest at preparation,32groups and ceil(body_bytes/8192) page calls |

The nine unique rows cover four investigations. Row3 is a shared control;
references from investigations2/3 never cause another sample. Family aliases
select each investigation; sp1-components selects their unique ordered union.
FULL/PREFIX inputs are the unchanged old-producer-v1 sealed whole0/whole1 and
physical envelopes. Pool fixture uses unchanged old-producer-v1 pool0 and value
groups. Sealed canonicals are arguments to the C2 component API, not candidate
expected-output shortcuts or a substitute for C1/FUSE work.

Preparation acquires/validates these existing fixtures once, imports through
normal catalog/body/group/registration operations, and publishes closed masters:
empty, metadata, FULL, PREFIX and synthetic-ring. MinIO masters are isolated
immutable input buckets. Each sample receives an independent writable byte-copy
SQL clone; needed provider objects are copied to an independent sample bucket
without mutation/regeneration of the master. Record setup=clone, method, source
hash/size, provider copy calls/bytes and every reuse. No APFS/hard-link/cached
output, prior mutated sample, live owner or reader cache is reused.

## API, timing and observed work

operation_contract_id=sp1-component-speed-v1; operation_surface=experimental
strict component API; orchestration/mutation executor=host. Use actual
StrictCatalog/NativeCatalog, Writer, ReadOwner and existing C2 algorithms. MinIO
runs the same pinned ARM64 image/digest used by r005, one disk, HTTP,
512MiB/1CPU, host bind root, no provider/backend fallback. SQLite and native
server remain on the macOS host. No public SDK/FUSE authenticity claim.

Primary monotonic Instant operation_ns starts immediately before the named
operation (capture/read, Writer construction/offer/finish, full snapshot walk or
native body registration) and stops after its normal acknowledgement. Argument
construction, fixture preparation/clone, engine/server configuration, output,
provider statistics/digests, proof and cleanup are outside this inner timer and
inside separately reported command/lifecycle scopes where applicable. Writer
initialization/offer/finish have nested attribution; allocation/codec/index work
is paid inside initialization. Native first-connection/authentication is paid
inside the first request; no preconnect/warmup/expected-range priming.

Record wall_ns and getrusage CPU deltas (host producer and native server share
one process), source/canonical/body bytes, objects/exact reuse, FULL/PREFIX/trials,
private/group/pack work, all MinIO call/requested-byte counters, native action
counts/connect work, snapshot builds/slots/pages/VM/fullscan/sort, SQL sizes and
read decode counters. Missing metrics are null+reason, not zero. No benchmark
verification/reopen/body digest/expected-root comparison occurs inside timers;
intrinsic CAS identity and C2 authentication remain measured.

Initial writer/read fixture import is setup, not the observed operation. Scope
is component readiness, not C5/Exec/cleanup latency. Native/local route difference
is an intentional non-paired diagnostic; no product improvement ratio. FULL,
PREFIX, reuse and hydration are distinct operations, not interchangeable speed
arms. Report raw ns and ms; no MiB/s headline for these small fixed-cost inputs.

## Cache, limits and eligibility

Clone/hashing/copy finish before preconditioning. Reuse the existing generic
host residency instrument for complete SQL clone files where supported; record
self-check, pre/post nonfaulting whole-file residency and launch gap. Never read
or touch input pages to manufacture a cold state. Fresh engines/C2 owners have
no previously read object/candidate caches; schema-only setup is explicit.
Source-page invalidation cannot attest SQLite pager, MinIO process/VM/volume or
device caches. Therefore all file-backed numeric speed claims remain
INELIGIBLE unless every required cache domain is proven by retained evidence.
Unverified domains are not excuses for warm timing PASS. Deterministic memory
canonical arguments and new-output writes carry that precise input contract,
not a disk-throughput or cold claim. No cold/warm pooling.

Frozen diagnostic alert thresholds (not release targets): metadata rows20ms;
small/write rows250ms; payload reads100ms; hydration100ms; transfer250ms.
Report UNDER_ALERT/OVER_ALERT only beside cache/claim eligibility. No threshold
changes after observation. Complete sample command (required clone/provider
copy/server/product/receipt/cleanup) <=15s; proof <10s. Over-bound cells remain
FAIL/NOT_RUN with actual wall/reason; no worker/workload/timeout relaxation.
One construction producer, LAYERFS_CONSTRUCTION_WORKERS=1, current quotas.
A transport server thread performs SQL/RPC service only, not construction.
Build release --locked in the owned target, archive executable by SHA256,
record first-use/cached build wall separately. No local build overlaps a timed
phase; use the existing worktree-local lock.

## Prerequisites, proof, custody and reporting

Writable cloned masters require ordinary open_writable of a caller-qualified
known-closed catalog. It must refuse sidecars/unresolved capture/save custody,
validate markers/fingerprint before writes, retain every row, initialize current
TEMP state once and get a fresh epoch. Add an indexed unresolved-save check,
review schema version3/fingerprint; no recovery/migration/adoption or test hook.
This source change and exact profile are sealed before masters/samples.

Each row has a separate exact-identity proof: reconstruct relevant output
canonical and logical bytes against original independent fixtures; verify
expected FULL/PREFIX/reuse counters, native/provider route counts, body SHA/
framing, snapshot8192/65/one-build/indexed-page work, master immutability and
sample cleanup. Verifier is <10s, not included in operation timing. Reuse only
matching existing proofs explicitly; these new performance rows still need their
own persisted-output proof. Retain originals for failed/unknown results.

Use fresh owned outputs under benchmark-results/fs-bench-pro/, release binary
and complete compilation/dependency seal including phase6-live Rust+SQL,
manifests/lockfiles, Core inputs, root .cargo/config.toml/ARMv8 flags, harness,
workload/spec, oracle hashes, host/provider identities. Source before/after builds
must match. Record source commit/tree/dirty state, actual binary/image digests,
command, fields/availability, raw stdout/stderr, phase clocks/counters and all
failure/cleanup/custody statuses. Freeze supported exact preparation hashes
before sample use; no candidate-derived semantic oracle.

Report all9 rows and all4 investigation views from retained raw receipts. Keep
performance, proof, resource, cache and cleanup outcomes separate. Physical RSS,
cgroup phase peaks, disk/device reads and total provider allocation are
UNAVAILABLE unless actually instrumented; container512MiB is a quota, not peak.
No history/speedup/extrapolation/percentile/public SDK/memory admission claim.

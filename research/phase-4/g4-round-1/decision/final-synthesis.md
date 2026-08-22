# Phase 4 G4 Round-1 final synthesis

Disposition: **ROUND 1 COMPLETE / G4 PREREGISTRATION READY / NO CANDIDATE PROMOTED**

G3 remains `PASS / G4 READY`; G4 remains **UNSTARTED**. This research does not
implement a production candidate, run G4 acceptance, start G5, integrate VFS or
SDK, change a profile, modify sealed evidence, or commit.

## Decision in one paragraph

Keep Canonical-v2, the current 8/16/32-KiB FastCDC profile, fixed-radix mapping,
SQLite `FULL+DELETE`, and `cache_spill=2000` as the authoritative control. G4
should first refresh the exact proof-preserving logical reconstruction row,
then establish first/full native materialization by adding a bounded writer
sink to that same **batched** authenticated traversal and publishing through
G3's proven native old-or-new protocol. It should separately qualify G3's
same-open protected-seed clone/patch and measure a real full seed read. A
separate one-variable G4 repair may formally prove and remove the derivable
closure product from the shared verified stream. The highest-upside later
architecture is two representations: Canonical-v2 remains durable truth while
a capacity-bounded content-root native seed cache, under a stronger protection
domain, accelerates reads and clones. No current primitive preserves that seed
authority across a true broker restart, so restart still means discard or full
reauthentication/rebuild.

## 1. What G4 must measure first

The first timed row is **R0: the existing accepted complete authenticated
logical reconstruction**, under its exact frozen candidate-campaign custody. It
must retain:

- the expected namespace/file root and profile;
- complete mapping and canonical-object authentication;
- exact topology/order/length/role/error behavior;
- closure, ordered occurrence, and raw output evidence products;
- current 170-query/5,371-row S1-100 shape, including 83 batched leaf queries;
- direct CPU/RSS/Q/SQLite/storage counters; and
- warm versus fresh-process/warm-or-unknown labels.

This prevents a native candidate from winning by deleting proof work or
falling back to G3's approximately 5,371-query one-chunk-at-a-time traversal.

Next, run the unmodified G3 complete fallback once as **M0-control**, not an M0
candidate: it already authenticates and writes in one
traversal but omits accepted closure/occurrence outputs and loses leaf
batching. Then measure promotable M0, which uses the accepted batched walker
plus a bounded native sink and the exact G3 publication protocol. The control
is immutable and measured first; the candidate cannot overwrite or relabel it.
Both qualify one native file at the engine boundary only—not a directory,
workspace, VFS, SDK, or application checkout.

The baseline and candidate are separate binary identities. `M0-control` retains
the G3-v13 executable
`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
Its sealed measurement custody is HEAD
`d79f0e0e2582d1bc491410224fec2b6cef7482e9` plus the then-dirty frozen
four-file source set
`3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`;
clean `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` later committed those exact
source bytes but is not by itself the v13 measurement identity. The candidate
build and focused 1/10-MiB screen are a separate lock-held, cleanup-inclusive
<=120-second experiment; only afterward may the main campaign use both frozen
binaries without building inside its own <=120-second clock.

The prerequisite distinctions and matrix are frozen in the
[proposed G4 contract](../benchmark-contract/proposed-g4-contract.md).

## 2. Controlled-cold disposition

**Unavailable on the current Round-1 host.** Three research lanes shared an
interactive macOS/APFS machine; a machine-global cache purge was neither an
isolated nor an authorized ordinary row. Process restart and SQLite reopen do
not control the OS or device cache. Apple defines `F_NOCACHE` as changing cache
behavior for a descriptor; it does not prove eviction of already resident
ordinary-path SQLite, mapping, directory, or device-cache state
(<https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html>).

A later exclusive-host approximation may:

1. finish source/binary/fixture/base hashing and all preparation first;
2. close every relevant operand;
3. verify exclusive benchmark custody and acquire the lock;
4. successfully run `/usr/sbin/purge` outside the operation timer;
5. launch one no-warmup row immediately; and
6. label it `controlled-host-buffer-cold-approximation`, with device/controller
   cache and stable-media physical I/O still `Unavailable`.

Apple's own `purge(8)` documentation describes only an approximation of
initial-boot disk-buffer conditions
(<https://github.com/apple-oss-distributions/system_cmds/blob/main/purge/purge.8>).
If exclusivity or purge success is absent, retain an `Unavailable` row. Do not
substitute `fresh-process`, `warm-or-unknown`, or `uncached-fd`.

## 3. Best format-preserving reconstruction improvement

For the accepted path, the best candidate is a **shared verified-stream
contract**:

```text
materialize_verified_file(expected_root, path, sink)
  -> VerifiedFileSummary
```

The exact API name is not prescribed. Its required semantics are:

- pin one head/catalog generation and resolve against the exact expected root;
- authenticate every mapping and chunk object against its expected Object ID;
- preserve Canonical-v2 role, order, partition, cumulative extent, length,
  cycle, limit, and identity-before-grammar errors;
- emit each raw chunk to a private/caller sink only after that chunk is fully
  authenticated;
- return a typed root/profile/length/reference/generation summary;
- never grant publication authority after only a verified prefix; and
- make closure, occurrence sequence, and output digest explicit requested
  evidence products rather than accidental duplicated authority.

The first A/B removes **only** the separate closure product after a formal proof
that an authoritative Canonical-v2 root plus complete per-object
authentication already binds the same ordered graph. The raw output digest is
retained in that experiment. This is format-preserving for canonical data, but
any persisted receipt/result schema that exports closure must be explicitly
versioned.

Prediction, labeled as an upper bound:

```text
current warm reconstruction                         338.775916 ms
G2 closure-family gross ceiling                     -88.483070 ms
impossible full-ceiling floor                       250.292846 ms
impossible-floor throughput                         396.37 MiB/s
plausible hypothesis                                20–88 ms improvement
acceptance / stretch                                <=333 / <=300 ms
```

The 88.483-ms family is not guaranteed removable wall; component medians are
not additive into a synthetic median and formal/error/API proof may retain part
of the work. The underlying direct timers are disjoint **within each row**, and
that row's direct sum plus its own residual equals its parent; the published
component-wise medians may come from different rows. Kill the candidate below
5% adjacent improvement or on any authority/error/protected regression.

There is also a statically clear public-engine repair: the older
`Engine::read_object_range` performs two full incremental-BLOB passes, opens a
third BLOB for the range, and `load_object` later revalidates the owned bytes.
One acquisition can hash, remember grammar failure until identity completes,
retain the selected range, and preserve error precedence. That is a later
production-integration repair; it has no defensible gain against the accepted
borrowed/batched benchmark path.

## 4. Best format-preserving first-materialization improvement

Use the shared accepted batched traversal above with a bounded `Write` sink,
then apply the G3-v13 native publisher:

```text
expected root + complete accepted authentication/folds
  -> same-directory private O_EXCL/O_NOFOLLOW temp
  -> authenticated chunk writes, bounded buffer
  -> exact summary and output checks
  -> final metadata
  -> file data/metadata sync policy
  -> descriptor-relative no-follow atomic rename
  -> parent-directory sync
  -> target/prior reconciliation on ambiguous acknowledgement
  -> owned-temp cleanup
```

Unsupported clone, cross-volume `EXDEV`, `ENOTSUP`, count change, invalid
authority, external mutation, or clone identity failure converges on this same
writer without consuming the protected-seed permit.

Prediction:

```text
T_first_native_warm
  = 338.775916 ms accepted logical control
  + native write/sync/metadata/rename/dirsync/reconcile
  - actual measured overlap

400-ms acceptance permits net native overhead <= 61.224084 ms
333-ms stretch cannot be inferred from the current 338.775916-ms proof-heavy control
```

This equation is honest but not evidence that the objective will pass. Native
write/sync wall is unavailable, and G3's 1-MiB fallback cannot be linearly
extrapolated. M0 should be killed if warm 100 MiB exceeds 400 ms or any exact
semantic, resource, durability, cleanup, or protected gate fails.

## 5. Best disruptive architecture

The best cross-operation architecture is **Canonical-v2 durable truth plus a
capacity-bounded content-root verified native cache under stronger custody**.

```text
canonical truth
  SQLite head/receipt/profile
    -> Canonical-v2 root and immutable canonical objects

derived native plane
  key: authenticated store/profile + file content/root identity
  value: exact raw native seed
  authority: service/private protection domain + descriptor leases
  capacity: allocated-byte cap K + entry/fd/concurrency caps
  hit: read descriptor or whole-file clone + authenticated patch
  miss/corruption/restart: complete canonical reconstruction and rebuild
```

It is better than per-revision native projections because content-addressing
shares equal roots and a hard cap prevents history growth:

```text
native_cache_allocated(n) <= K
not n * file_size
```

For an illustrative `K=2 GiB`, ten distinct 100-MiB roots may consume about
1 GiB before filesystem-sharing details; 100 and 1,000 distinct roots plateau
at the cap through eviction. Fill/rebuild is `Theta(A+S)` plus native write and
readback; a full read hit is `Theta(S)`; a clone hit is filesystem metadata
span plus authenticated changed bytes. Cache authority does **not** establish
the canonical mapping/edit witness required by first edit after reopen, so it
does not currently reduce the retained approximately 154-ms first-edit path.
Foreground misses/revalidation and maintenance eviction/repair/rebuild each
need separate CPU/RSS/Q/storage/wall accounting; none is free background work.

The blocker is authority, not lookup design. No current LayerFS/Apple primitive
binds root identity to a named cache file across true broker restart while
excluding malicious same-UID substitution, rollback, or mutation through an
existing writable descriptor. An authenticated unlinked fd may be passed to an
already-live process and remains a capability while some holder survives; it
cannot be reopened after all holders exit. Restart therefore discards or fully
reauthenticates/rebuilds until a separate-UID/service-owned, rollback-resistant
design is proved.

The storage-research ladder starts with **bounded SQLite-resident authenticated
extent BLOBs**, preserving one SQLite/CAS durable authority while testing
coarser acquisition. Only if that lower bound is insufficient may a later
immutable external segment/value plane be considered with SQLite retaining the
sole catalog/head authority. The latter can at most attack the current
59.403771-ms BLOB-acquisition family and requires a complete segment-sync then
SQLite-commit, orphan-tail, migration, reader-generation, compaction, and GC
protocol. Neither belongs in DO-NOW G4; do not implement either writer from
query-count arithmetic.

## 6. Necessary, redundant, and uncertain full-byte/proof passes

| Work | Classification | Reason |
|---|---|---|
| Read every logical byte for first/full reconstruction with no trusted representation | **Necessary** | Operation returns/emits all `S` bytes; lower bound `Theta(S)` |
| Write every logical byte for portable first/full native output | **Necessary** | Clone is optional; complete fallback must create exact native bytes |
| Authenticate each fetched canonical object against expected Object ID | **Necessary now** | Current store/cache threat model does not otherwise authenticate stored bytes |
| Validate mapping/Bytes role, grammar, order, partition, cumulative ends, lengths, counts, cycles, and bounds | **Necessary** | Root identity does not make a malformed/misrouted decoder safe without validating the fetched object grammar |
| Authenticate every complete intersecting chunk for a range | **Necessary under current whole-object IDs** | Current range cost is `O(H+J+R)`, not merely `O(R)` |
| Native private-temp data/metadata durability, atomic rename, parent sync, ambiguity reconciliation | **Necessary for accepted durable native publication** | Old-or-new and cleanup contract; platform stable-media limits stay explicit |
| Separate closure hash over role/ID/length/full canonical bytes after every object/edge authenticated | **Derivable for byte authority; currently required evidence/API output** | Formal proof strongly suggests redundancy, but removal needs one-variable negative-case and schema/API review |
| Separate ordered `(length,ObjectId)` occurrence commitment | **Derivable; current exported evidence** | Canonical-v2 mapping already commits the same ordered occurrences; retained median only 0.408711 ms |
| Whole raw output fingerprint | **Required benchmark oracle; product requirement uncertain** | Current hash sink uses it as exact fixture evidence. A real caller must deliver bytes but may not require a second digest result. First A/B retains it. |
| Secondary Bytes decode/length pass | **Redundant but immaterial** | G2 median 0.141476 ms; not a lead candidate |
| SQLite/BLOB acquisition | **Necessary with current physical layout; constants replaceable** | Gross 59.403771-ms ceiling; physical segments could change it, not eliminate payload reads |
| G3 full parent/target source comparisons and exact patch-relation construction | **Benchmark mechanism qualification, not per-operation product work** | Production must replace with capture/transaction authority; cannot hide preparation if still used |
| G3 seed construction, native write, full readback/hash, unlink | **Necessary for current seed qualification; frequency/architecture uncertain** | Never charge zero or hide on cache miss/rebuild |
| Reauthenticate seed on every live same-open descriptor read | **Unnecessary within exact retained descriptor authority; necessary after authority loss** | Cross-process live fd handoff is not broker-restart persistence |
| Public engine's second BLOB grammar pass, third range open, and later load revalidation | **Redundant implementation work** | Can be fused while preserving identity-first errors; separate production repair |

## 7. Can trusted-seed full reads reach 2–3 GiB/s?

**Plausible only for a cache-hot, already-qualified live descriptor, and still
unproven.** Exact target arithmetic is:

```text
100 MiB / 50 ms = 2,000 MiB/s
100 MiB / 35 ms = 2,857.14 MiB/s
```

A bounded sequential descriptor read to a consuming sink can use <=1 MiB
application buffering and stay under the 20-MiB RSS objective. It still copies
all `S` bytes. An in-timer whole-output digest may defeat the target: G2's
single-thread raw-fingerprint family alone was 87.889943 ms on a different but
related path. G4 must therefore report product read delivery, optional digest,
and untimed independent oracle separately, without weakening byte exactness.

No persistent/fresh-process 2–3-GiB/s claim is currently admissible. Seed fill
and rebuild pay complete canonical reconstruction, native write/sync, full
readback/hash, and storage capacity. Broker restart loses the only proved fast
authority.

The retained G3 100-MiB one-byte number is strictly operation-local. Its raw
row reports `operation_total_ns=3,414,166`, but the entire child was 4.24 s
external real, 3.23 s user CPU, and 0.91 s system CPU. Seed and candidate temp
each had 104,857,600 logical/apparent/allocated bytes, and post-operation exact
verification read 104,857,600 bytes outside the operation timer. G4 and any
later cache must therefore keep three ledgers separate:

```text
fill/qualification = canonical build/proofs + seed write/sync/readback + permit
qualified hit      = authority check + clone/read + selected patch + publication
maintenance        = revalidation + eviction + corruption repair + rebuild
```

Only the middle line is represented by the 3.414166-ms operation wall.

## 8. Can first/cold native reach 200–300 MiB/s?

- **Warm first/full `>=250 MiB/s` / `<=400 ms`: plausible but unmeasured.** It
  requires net native overhead no more than 61.224084 ms over the retained
  proof-heavy logical control, or a separately accepted reconstruction repair.
- **Warm stretch `>=300 MiB/s` / `<=333 ms`: not reachable from the current
  338.775916-ms logical control by merely adding output.** It requires G4-R1 or
  another measured reconstruction reduction before/native overlap.
- **Controlled host-buffer-cold `>=200 MiB/s` / `<=500 ms`: unresolved.** The
  row is unavailable on this shared host. Under a later exclusive purge
  approximation, the blocker can be identified only by direct canonical
  acquisition/authentication, output write, and sync phase counters.

The known insignificant blocker is the 0.141-ms secondary decode. The likely
components are the 94.817-ms canonical authentication, 59.404-ms BLOB
acquisition, current evidence folds, actual destination writes, and sync/
publication. Do not infer physical I/O or add gross medians to predict cold.

## 9. Candidates that preserve wins together

| Candidate | Create | Edit/COW | Range | Reopen | Reconstruction | First native | Incremental native |
|---|---|---|---|---|---|---|---|
| G4-M0 batched native writer | unchanged, <=5% guard | unchanged | unchanged | unchanged | shared exact walker | establishes missing path | fallback only |
| G4-S1 retained protected seed | fill separate | current capture authority | selected chunks retained | no persistent claim | full seed-read row separate | clone/no-op | observed strong clone/patch, G4 pending |
| G4-R1 verified-stream proof repair | protect <=5% | protect <=5% | current route unchanged | unchanged | 20–88-ms hypothesis | lowers shared walker if accepted | no direct hit change |
| L-A1 bounded native cache | miss/admission may regress | canonical truth unchanged | seed optional, canonical fallback | authority blocker | hits can bypass store only under valid custody | clone hit | clone/patch hit |
| L-P1 SQLite extent then segment ladder | may win or lose; hard gate | append new objects, COW unchanged | extent/locator regression risk | catalog stays SQLite | acquisition hypothesis only | shared stream source | seed path separate |
| L-I1 direct VFS | unchanged | new write API deferred | primary upside | pinned handles | full read still linear | avoids checkout for partial access | cache optional |

Only the first three belong in G4. They remain separate variables/scoreboards;
do not stack all mechanisms into one arm.

## 10. Milestone routing

| Decision | Owner after Round 1 | Why |
|---|---|---|
| R0/R1/R2 logical reconstruction truth and cache labels | G4 | Needed before any optimization claim |
| G4-M0 first/full native writer and portable fallback | G4 | Missing acceptance path; format-preserving |
| G3 protected-seed clone/no-op/patch/fault qualification | G4 | Retained candidate is ready but only a mechanism screen |
| Trusted same-open seed full-read row | G4 | Separates read throughput from clone wall |
| One-variable closure-product authority A/B | G4 repair; move out if 120-s campaign cannot remain exact | Format-preserving contract question with large ceiling |
| Public engine multi-pass BLOB fusion | later production integration | Accepted benchmark already uses borrowed/batched path |
| Persistent/cross-process seed authority and bounded cache | later architecture / G5-A or integration | Requires new protection domain, restart, eviction, corruption, concurrency |
| SQLite-resident authenticated extent BLOB lower bound | later physical profile / G5-D research | Preserve one durable truth and test direct acquisition counters first |
| External immutable segment/value plane | only after SQLite extent evidence | Two-file crash/GC/migration work; only 59-ms gross ceiling |
| Count-changing mapping/prolly profile | G5-B | Current suffix behavior remains protected and already fast at 100 MiB |
| Concurrency/endurance/history/GC | G5-C | Single-operation G3/G4 does not establish it |
| VFS/projection/SDK/application | post-core integration | VFS/SDK are stubs; no current API to promote |
| Final scoreboard/evidence/limitations closure | G6 | G4 PASS alone does not complete Phase 4 |

The complete downstream dependency graph is in the
[post-G4 dependency map](../roadmap/post-g4-dependency-map.md).

## Ranked conclusion

### DO NOW / G4

1. **G4-M0:** batched proof-preserving verified stream into durable native temp
   with the portable fallback convergence.
2. **G4-S1:** same-open trusted full read plus unchanged G3-v13 clone/patch/
   fallback qualification.
3. **G4-R1:** formal proof and one-variable closure-product A/B in the shared
   verified-stream boundary.

### LATER PROFILE / ARCHITECTURE

1. **L-A1:** capacity-bounded content-root native cache under a stronger
   protection domain — the top disruptive architecture.
2. **L-P1:** SQLite-resident authenticated extent BLOBs first; external
   immutable segments only after a decisive lower-bound result.
3. **L-I1:** direct VFS range/sequential streaming over the shared engine
   primitive.

### REJECTED / DEFERRED

1. Reject mutable destination/seed receipts, path/inode/timestamps/watchers, or
   clone lineage as byte authority.
2. Reject restart/reopen/`F_NOCACHE`/shared-host state relabeled controlled
   cold.
3. Defer new Merkle/prolly/CDC profiles; reject per-chunk loose-file reflink and
   foreground compression/pack for the current workload/evidence.

Full 15-field records and kill rules are in the
[candidate matrix](candidate-matrix.md).

## G4 preregistration readiness and unresolved blockers

The decision package is ready for a separate preregistration author/reviewer,
not direct execution. The contract now supplies the missing controls:

- two scoreboards and 1/10/100-MiB matrix are fixed;
- warm, fresh-process, warm-or-unknown, controlled-cold/unavailable,
  empty-destination, protected-seed, and fallback classes are fixed;
- direct CPU/RSS/Q/storage/SQL/BLOB/filesystem/durability counters are fixed;
- performance objectives and <=5% protected gates are fixed;
- exact global fail-fast lock path
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/target/BENCHMARK_LOCK`;
- separate frozen control/candidate binary identities and a lock-held <=120-s
  candidate build plus 1/10-MiB screen before the no-build main campaign;
- exact 30-row one-shot chronology with `M0-control` before `M0-candidate`;
- measured-campaign equation/buckets totaling <=120 seconds, with workspace
  static validation separately timed and gated;
- primary/independent analysis and append-only repair rules are fixed; and
- production-integration boundary is explicit.

Unresolved facts to be measured or designed later:

- 100-MiB first/full native wall and its write/sync phase breakdown;
- true trusted-seed 100-MiB returned-byte wall with/without digest;
- an exclusive-host buffer-cache-cold approximation; true device-cold remains
  unavailable;
- formal authority/API/error proof for removing closure output;
- persistent cross-process/restart seed authority;
- native-cache hit rate/fill/rebuild/eviction/corruption/history;
- SQLite-resident extent lower bound, then only if warranted an external
  segment lower bound and two-file crash/GC/migration protocol; and
- VFS/projection, concurrency, endurance, and final Phase-4 closure.

## Primary external sources used in the decision

- SQLite incremental BLOB semantics:
  <https://www.sqlite.org/c3ref/blob_open.html>
- SQLite connection cache counters:
  <https://www.sqlite.org/c3ref/c_dbstatus_options.html>
- SQLite atomic-commit protocol:
  <https://www.sqlite.org/atomiccommit.html>
- rusqlite BLOB positional I/O:
  <https://docs.rs/rusqlite/0.40.2/rusqlite/blob/index.html>
- Apple APFS clone behavior:
  <https://developer.apple.com/documentation/foundation/about-apple-file-system>
- Apple clone/copy APIs:
  <https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/ToolsandAPIs/ToolsandAPIs.html>
- Venti content-addressed immutable blocks:
  <https://www.nokia.com/bell-labs/publications-and-media/publications/venti-a-new-approach-to-archival-storage/>
- WiscKey key/value separation and GC tradeoff:
  <https://www.usenix.org/conference/fast16/technical-sessions/presentation/lu>
- Git multi-pack index and replaceable physical locations:
  <https://git-scm.com/docs/multi-pack-index>
- Xet content-defined chunks and bounded aggregate retrieval:
  <https://huggingface.co/docs/xet/index>
- BLAKE3 tree specification and Bao verified-slice representation:
  <https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex>,
  <https://github.com/oconnor663/bao/blob/master/docs/spec.md>

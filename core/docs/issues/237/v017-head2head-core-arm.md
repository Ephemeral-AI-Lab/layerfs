# #237: Core arm of the common-source v0.1.6 comparison

> One preregistered diagnostic on the current integrated Core native Init. The
> public call completed, but the benchmark receipt is **INCOMPLETE** because a
> daemon telemetry event was lost. The full reopened readback passed separately.
> This is not a fully cold admission or a release performance result.

The common-source protocol is [v016-v017-common-source-prereg.md](v016-v017-common-source-prereg.md)
at root commit `60c941bbf`. This arm used Core base `7f2124ba0` with only
temporary aggregate instrumentation in three Service files; the exact
[instrumentation diff](evidence/v017-head2head-core/instrumentation.diff.gz)
is retained. Product seal `d9b17e338d61a33a8f907d07e2758b7ac83c9e246212dbef200b59f5a0472d74`,
harness seal `6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
and the release Service binary SHA-256
`c4e22ff40f8f363b4fa305be323ddc10f22ed164d5b1e36f3ed0dab4b3e6d131`
were pinned before timing. [build.json](evidence/v017-head2head-core/build.json)
and [receipt.json](evidence/v017-head2head-core/receipt.json) retain all build
and execution identities.

The shared seed-1 master had manifest SHA-256
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
Its sealed directory was linked into this isolated worktree **only for untimed
setup**. H3 made a distinct writable byte copy for this arm. Full hash,
invalidation, and nonfaulting whole-input checks found **0/27,503 resident
payload pages** across 300,000,000 B both at preflight and immediate recheck;
the recheck ended 6.820042 ms before the timer. The
[cold sidecars](evidence/v017-head2head-core/cold-launch.json) record the
method, source path and timing. Directory/inode metadata residency remained
unqualified, so the row keeps `source-cache-uncontrolled-v1` and cannot be
called fully cold. No OS-specific purge or external pack storage was used.

## Single public result and micro counts

| Observation | Core single run | Scope |
| --- | ---: | --- |
| Public daemon request through `StackCreated` | **1,380.218125 ms** (217.357 decimal MB/s) | One public call, 300 MB numerator |
| Complete performance command | 2,318.692459 ms | Process setup, call and cleanup; separate from public latency |
| Service `history.import_scan` | 45.427416 ms | Whole-directory names/metadata scan |
| Service `history.import_files` | 1,169.934333 ms | Receiver loop plus four concurrent producers |
| Receiver `SaveHandoff::accept` / `recv_timeout` | 720.450294 / 445.970804 ms | Nonoverlapping receiver actions; 3.513235 ms other loop work |
| Four-producer construction / object-send sums | 4,420.194964 / 2,593.985371 ms | Sums overlap workers and receiver, not caller-additive |
| Source `Read::read` calls / bytes / summed wall | 29,952 / 300,000,000 B / 1,129.334721 ms | Four-producer sum; logical returned bytes, not device-read bytes |
| File Save C2 SQL / COMMIT buckets | 138.887229 / 223.729665 ms | Disjoint within Save profile; 80 file-Save COMMITs |
| File Save pack write / object INSERT regions | 78.919545 / 65.525741 ms | Nested in SQL/placement work; do not add to parent buckets |
| Whole Core namespace / `build_filesystem` | 45.339792 / 8.811541 ms | Whole stage includes prereq/tree saves; inner construct overlaps |
| Catalog reserve / publication whole calls | 0.260375 / 0.243458 ms | Two separate C5 write transactions; COMMIT-only time unmeasured |

The receiver handled 24,562 object messages and 10,000 completion messages;
its 34,562 receive calls include waiting for the last producer completion.
The reference's slab consumer idle timer has a different message and tail
boundary. The Core file Save also spent **72.854333 ms dropping its SQLite
connection** after its named 4.406167-ms finish-save child and inside the
public operation. Summing disjoint top-level Core Service stages (begin Save,
scan, file loop, named finish, this drop, catalog reserve, whole namespace,
catalog publication) gives 1,340.547124 ms, only 0.365460 ms below the
sampled Service `HistoryCommand` span of 1,340.912584 ms. Its boundary differs
from the 1,380.218125-ms daemon public call; the difference must not be
assigned entirely to transport, especially with incomplete daemon telemetry.

| Core completed Save | Inserted / reused | COMMITs | COMMIT bucket | Object INSERT calls | Pack creates / appends |
| --- | ---: | ---: | ---: | ---: | ---: |
| File | 24,364 / 198 | 80 | 223.729665 ms | 1,408 | 1,257 / 1,037 |
| Metadata prerequisite | 11 / 3 | 4 | 0.535209 ms | 2 | 2 / 0 |
| Namespace tree | 308 / 0 | 21 | 2.305376 ms | 4 | 3 / 200 |
| **C2 total** | **24,683 / 201** | **105** | **226.570250 ms** | **1,414** | **1,262 / 1,237** |

Core C5 additionally makes two successful catalog write transactions, one
for inode reservation and one for LayerStack publication; this count is
source-derived from the public path and `HistoryCatalog::write`, not traced.
Thus there are **107 successful write COMMITs** across C2 and C5, while only
the C2 105 have exact SaveOutcome counters and a COMMIT-only timer. The file
Save also issued 67 presence queries, wrote 300,879,140 submitted pack bytes,
and inserted 24,364 objects. Per-statement counts for every other SQL shape
and actual owner pager spill/cache readings were **not measured** in this arm.

The closed [Store geometry](evidence/v017-head2head-core/store-geometry.json)
reports SQLite page size **4,096 B**, 24,683 object rows, 1,262 SQLite BLOB
packs, 330,825,728 B capacity and 305,969,540 B used. Content Store size is
333,651,968 B apparent / 335,609,856 B allocated. The separate
[History geometry](evidence/v017-head2head-core/history-geometry.json)
reports 4,096-B pages and 86,016 B apparent/allocated, giving a combined
333,737,984 B apparent / 335,695,872 B allocated. The Store and catalog SHA-256
values and retained private paths are in the [evidence manifest](evidence/v017-head2head-core/manifest.json).

The runner's service+daemon **lifecycle** CPU was 1.015206 s user + 0.864606 s
system. The sampled local Service `HistoryCommand` window counted 0.967597 s
user + 0.829099 s system and sampled 59,195,392 B maximum RSS in 14 samples;
boundary coverage was false. The daemon local window counted 0.000359 s user
+ 0.000784 s system. These windows are not an exact caller CPU sum, and no
physical device-read/write delta was recorded.

## Prospective next 10k experiment: bounded producer slabs

The common-source comparison observed 1,203 reference producer slabs versus
34,562 Core object/completion messages, while both use four construction
workers. The Core source currently has an eight-message channel and one
`Message::Object` per finalized object plus one `Message::Done` per file
([Core producer/receiver](../../../crates/layerfs-service/src/operation/import_native.rs)).
The reference demonstrates four queued slabs bounded by **256 KiB and 512
objects** ([constants](../../../../crates/layerfs-layerstack-store/src/objects.rs#L39-L48)
and [pipeline](../../../../crates/layerfs-layerstack-store/src/objects.rs#L372-L480)).
Those counts nominate a handoff experiment; they do not prove that the Core
receiver's 445.971-ms wait and the reference slab receiver's 47.914-ms idle
have identical boundaries or that their 398-ms difference is removable.

One prospective Core treatment would give each of the existing four workers
one partial slab, with at most **512 finalized objects, 512 completions and
256 KiB canonical payload bytes** per ordinary slab, and one `sync_channel`
with **four queued slab slots**. The byte test uses
`FinalizedObject::canonical_len()` ([object API](../../../crates/layerfs-content/src/object/output.rs#L96-L140));
the completion cap also bounds empty-file result metadata. An object larger
than 256 KiB takes an immediate **singleton** message after flushing the
worker's partial slab. Its size remains limited by the existing 16-MiB
canonical-object policy ([C2 policy](../../../crates/layerfs-storage/src/policy.rs#L113-L144));
no object is split or silently rejected merely for exceeding the slab target.
At most four ordinary queued slabs and four producer partial slabs own 2 MiB
of canonical payload, plus bounded completion records; singleton slots have
a separately declared 4 × 16-MiB queued upper bound, and four producers may
each hold one 16-MiB singleton while blocked. The pessimistic singleton bound
is therefore **128 MiB before C2 ownership**; the treatment must measure
peak memory and may need a stricter singleton admission permit. These limits must be
charged alongside C2's existing pending-wave ownership, not hidden behind a
`Vec` without byte accounting. Core's current `PendingBatch` already has the
same empty-singleton pattern for an object above its wave byte bound
([batch](../../../crates/layerfs-storage/src/cas/batch.rs#L13-L67)).

Carry each file completion in a slab after the objects it produced. On the
consumer side, call the existing `SaveHandoff::accept` **once per object in
the same order** and only then apply that slab's completions; drain and join
all producers on failure. This preserves C2's per-object semantics and avoids
ending the receiver loop before a worker flushes its last partial slab. The
candidate should count actual slab sends, payload bytes, producer block time,
receiver wait/accept time and unchanged C2 SaveOutcome work. It should be
measured once against an identity-matched Core control under the same cold
payload checks, 4-KiB SQLite pages, 128-KiB cutoff and full later readback.
No slab source change or timed treatment is part of this result.

## Status and proof

The [public perf record](evidence/v017-head2head-core/perf.jsonl) returned the
confirmed root `e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
The runner retained `INCOMPLETE` due solely to `daemon: telemetry loss`;
Service/daemon cleanup passed, and in-run full verification was `SKIPPED` as
preregistered. The [separate reopened verifier](evidence/v017-head2head-core/verification-separate.json)
ran once afterward on the same Store, History, root and manifest. It passed in
3.166599166 s, traversing 10,101 paths / 101 directories / 10,000 files and
checking all 300,000,000 B. It does not repair the lost telemetry event or
turn the timed receipt into admission evidence. There was no retry or second
performance sample.

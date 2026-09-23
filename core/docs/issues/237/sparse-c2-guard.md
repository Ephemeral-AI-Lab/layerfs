# #237 matched sparse-history C2 guard

> **Status:** Research; preregistered before either arm. This is a separate
> space and correctness diagnostic, not a 10k Init speed or release result.

## Fixed comparison

Run the existing `history-stride1` producer once per arm over
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, manifest SHA-256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`.
Its 157 selected states declare 904,143 path-states and 4,936,693,030
logical bytes. Use `LAYERFS_HISTORY_ADVISORY=1`,
`LAYERFS_HISTORY_DEPTH_LIMIT=7`, `LAYERFS_HISTORY_FULL_PRODUCER=0`,
`LAYERFS_HISTORY_CHUNK_PREDECESSORS=1`, and
`LAYERFS_CONSTRUCTION_WORKERS=1` for both. The corpus, producer, policy,
SQLite 4,096 B pages, verifier, and readback tools are identical. Each arm
creates a fresh Store and output directory; never reuse a previous Store or
sample to select a better number.
Unset `LAYERFS_HISTORY_ORDERED_PREDECESSORS`,
`LAYERFS_HISTORY_DECLARED_ONLY`,
`LAYERFS_HISTORY_SIMILARITY_CANDIDATES`,
`LAYERFS_HISTORY_FALLBACK_CANDIDATES`, and `LAYERFS_HISTORY_PHASES` in
both commands so inherited diagnostics cannot alter one arm's producer.
The prospective raw output directories are
`benchmark-results/fs-bench-pro-storage-content/issue237-sparse-c2-control`
and `benchmark-results/fs-bench-pro-storage-content/issue237-sparse-c2-candidate`.
They were absent at preregistration and are never reused.
The common [one-arm wrapper](evidence/sparse-c2-guard/run_arm.py) holds this
worktree's measurement lock, refuses a dirty source or existing arm output,
checks the corpus pin, and retains the binary/source hashes, exact command,
exit status and wall time. Invoke it once with `control`, then once with
`candidate` after the product-only change. Its wall time is diagnostic; it is
not the 10k native Init caller timer.

The source checkout began at `df9dbc94b`, which **already contains** C2 commit
`a5f484b14`. The control reverses exactly that commit's five production
files to their `433e657f4` contents; the candidate reapplies their exact
`a5f484b14` contents. Its external test and architecture document do not
participate in the arm difference. The shared [benchmark-only verifier
iteration fix](../../../benchmark/fs-bench-pro-storage-content/src/ops/history.rs)
and [external Store readback tool](../../../crates/layerfs-storage/examples/sparse_history_readback.rs)
are frozen before either arm.
Record the SHA-256 of the built binary, the five production files, the
benchmark source, the corpus manifest, and both command lines in raw custody.

## Checks after each closed Store

1. Reopen the Store through public `Store`/`StoreProvider` and read **every
   distinct stored canonical object**. Recompute every `ObjectId` from bytes.
   Require equal object row counts, distinct ID sets, canonical byte totals,
   and ordered `(ID, length, bytes)` digests between arms.
2. Reopen through the existing history verifier with
   `--verify-sample 1000000`, which selects every declared oracle path in
   each state. Require 157 equal root IDs and 904,143 compared paths, with
   zero mismatch/missing/unexpected counts. This proves expected-path
   presence, kind, size, and digest. It does **not** prove the absence of
   undeclared extra paths; retain the verifier's `INCOMPLETE` classification.
3. Read both closed SQLite files with `mode=ro`. Require
   `PRAGMA page_size=4096`; record Store SHA-256, apparent and allocated
   bytes, page/freelist counts, object and pack counts,
   `sum(length(data))` pack capacity, header-declared used bytes, slack,
   and per-pack versions. Do not compare either arm to #229's mismatched
   archived Store. A candidate increase in apparent bytes, allocated bytes,
   pack capacity, or slack is a physical-space regression and must be
   reported; any root/object/readback difference is a correctness failure.

The original history row exceeds the 15/25 s command budget. Its timing is
retained as a **diagnostic only** and the admission row stays `NOT_RUN`.
Verification and Store inspection happen after the operation, outside any
throughput timer. The path check can take much longer than the normal
verification budget; it is a separate diagnostic and cannot promote an
admission PASS. Retain failures and incomplete rows unchanged.

## Iteration record

Before either arm, the external readback example failed its first release build:
`rusqlite::Row::get` does not implement `FromSql` for `u64` (two sites), and
`Store::open` needs a pending child scope rather than the active scope reference
(one site). The existing storage tests use signed `i64` reads and
`scope.child("store")`; the tool was changed to that public pattern. Its next
locked release build passed. This was a tool failure, not a product failure.

One separate 1,000-byte `measure_ingest` smoke Store at
`benchmark-results/fs-bench-pro-storage-content/issue237-sparse-c2-tool-smoke/`
passed the reopened tool: 1 object row, 1 distinct authenticated canonical
object, 1,023 canonical bytes, ordered canonical digest
`02740c4ff9cc1d6a7bc13ed8825c8fe7a580cf14dcd845c9fe8f3f75120de272`,
one read connection. The independent
[geometry receipt](evidence/sparse-c2-guard/tool-smoke-geometry.json) records
4,096 B pages and the closed Store hash. This smoke is not either history arm.

### Control C0 — failed, retained

The one preregistered control at `a209149de` returned process exit 0 but
printed **`history-stride1 INCOMPLETE`** after 3.593 s complete-command wall.
Its first correctness gate is
`product error: Integrity("dependency encoded work")`; no state root was
published, so the 157-state workload did not complete. The trace also records
`g1.o3-pinned-counters INCOMPLETE` because no complete row existed. This is a
failed arm, not a completed sparse-space baseline. The independent read-only
[closed-Store geometry](evidence/sparse-c2-guard/control/geometry.json)
records 4,096 B pages, 6,742,016 B apparent/allocated, 25 packs, 6,553,600 B
capacity, 1,677,769 B assembled and 4,875,831 B slack. The partial Store
has six published saves and 1,556 object rows. A separate byte copy reopened
and [authenticated all 1,556 objects](evidence/sparse-c2-guard/control/readback.stdout)
(9,289,896 canonical B, ordered digest `2294b1c475af9dbe4d73e6df9286e6ccbad6f558016da535681f6b31d39e14c2`).
That does not repair the failed operation or prove all history paths.

The [raw control receipt](evidence/sparse-c2-guard/control/custody.json),
[trace](evidence/sparse-c2-guard/control/trace.jsonl), and
[phase record](evidence/sparse-c2-guard/control/phases-perf.json) remain
append-only. The original 6.4 MB SQLite file remains under the named private
control output; its SHA-256 is in geometry. There will be no unchanged-control
retry. The single candidate run may locate whether the C2 change affects the
failure, but cannot form a matched completed-space comparison with C0.

### Candidate C1 — same failure, retained

The candidate at `0136dc525` restored the five C2 product files byte-for-byte
from `a5f484b14` and rebuilt the release producer. The custody manifests show
that exactly those five file hashes changed; the history producer and one-arm
wrapper hashes stayed equal. The same corpus manifest and fixed environment
were used. Its one fresh run returned process exit 0 but printed
**`history-stride1 INCOMPLETE`** after 3.358 s complete-command wall, with the
same `Integrity("dependency encoded work")` gate and no state root. The
[candidate custody](evidence/sparse-c2-guard/candidate/custody.json),
[trace](evidence/sparse-c2-guard/candidate/trace.jsonl), and
[phase record](evidence/sparse-c2-guard/candidate/phases-perf.json) retain that
failure. The 0.235 s difference between two failed partial commands is **not**
a history or Init throughput result.

Both partial Stores have 4,096 B pages, 1,556 object rows, 25 packs,
6,742,016 B apparent/allocated, 6,553,600 B pack capacity,
1,677,769 B assembled and 4,875,831 B slack. The candidate's
[geometry](evidence/sparse-c2-guard/candidate/geometry.json) records Store
SHA-256 `edb5ff9b…e1cf2`, different from control `e0807fee…2e157bf`.
Both independent byte copies reopened, read and authenticated every one of
their 1,556 distinct canonical objects. The
[candidate ordered digest](evidence/sparse-c2-guard/candidate/readback.stdout)
equals control exactly:
`2294b1c475af9dbe4d73e6df9286e6ccbad6f558016da535681f6b31d39e14c2`.
This proves parity only for the prefix of history that completed. The planned
157-root and 904,143-path verifier was **NOT_RUN** because neither arm
published a completed state sequence.

### Shared failure mechanism and open gate

`Integrity("dependency encoded work")` has one source:
`encoding/delta/read.rs::Resolver::decode_at` rejects a charged base chain
above `CHAIN_ENCODED_LIMIT = 256 KiB`. The only product call to
`Resolver::resolve_dependency` is `encoding/delta/select.rs::acquire`; its `?`
propagates this refusal before selection's later work-budget check can choose
FULL. Every already-published object reads back as an ordinary requested
object, so the source points to a charged advisory-base acquisition during
the next save. This is an inference, not a captured call stack: the failed
trace does not name the offending object, state or call site. This is a
**blocking product-path finding**, not a verifier-only timeout. It was not
changed in this C2 batching experiment and neither arm was rerun.

The matched #229 sparse-history space/correctness gate remains **OPEN**:
neither arm completed 157 states, no complete Store geometry exists, and no
all-path readback was possible. The equal partial geometry cannot establish
that bounded group admission preserves sparse pack capacity or slack over the
complete corpus. A subsequent repair must use a new, preregistered scenario
identity with new one-shot arms; these failed receipts remain failures.

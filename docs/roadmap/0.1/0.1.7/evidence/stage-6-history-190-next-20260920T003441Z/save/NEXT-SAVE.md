# Next Store-admission investigation

Status: source research only; no new timing claim, product edit, build or run.
Read against HEAD `10b9d4a6cf9d88267d508cb010cc82950e080d77` and the current working tree.
Source references below are repository-relative `file:line` references.

The prior candidate's three public storage intervals total 10,660,492,501 ns
(stride10) and 22,255,980,923 ns (stride3), as supplied by the parent investigation.
Its recorded `SaveOutcome.commits` sums are 1,149 and 1,470. These are previously
retained diagnostics, not measurements performed by this squad. The totals do not
attribute time to compression, signatures, SQLite or transactions.

## Actual boundaries

```text
begin_save
  verify profile; BEGIN IMMEDIATE; inspect publication and pack cursors
  reload candidate index only when invalidated; create codec workspaces
accept(object)
  push into bounded pending batch
  only when batch full: flush_batch
    batched membership lookup + batched direct-reference presence lookup
    exact reuse comparison OR offer each missing object
      validate dependencies
      select representation
        inode leaf: pooled-value/index path
        other roles: full alternative; optional candidate/decode/prefix trial
      seal when group requires it
        frame/compress group
        assemble selected whole pack
        INSERT/UPDATE pack; INSERT object rows
        charge rows + canonical bytes + whole rewritten pack bytes
        maybe COMMIT + BEGIN IMMEDIATE
finish
  flush remaining pending batch (same work as accept flush)
  seal remaining groups
  flush newly admitted candidate-index entries
  advance publication ceiling; final COMMIT
```

Entry points: `core/crates/layerfs-storage/src/cas/store.rs:383`, `:413`, `:424`.
Batch bounds: `cas/batch.rs:48`. Membership/presence/reuse: `cas/save.rs:22`, `:33`,
`:43`, `:74`. Representation and seal: `cas/selection.rs:28`, `:93`;
`cas/placement.rs:108`. Acquisition/final acknowledgement: `cas/lifecycle.rs:30`,
`:129`, `:172` (all paths in this paragraph under the same storage `src/`).

`finish` is NOT just COMMIT time: it can encode, acquire bases, write packs and
insert rows for the last batch. Similarly individual `accept` latency alternates
between cheap buffering and real flushes. The driver `save_ns` envelope also
contains caller index maintenance (`core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:1972`),
so use the disjoint `storage.begin + storage.accept_loop + storage.finish` total
for this comparison and keep `harness.index` separate. The accept-loop interval
itself includes cloning objects and rebuilding caller advisory lists before
calling `accept` (`history.rs:1952`); it is not pure C2 CPU.

## Existing observability and its limits

| Observable | Exact scope | What it cannot establish |
| --- | --- | --- |
| `SaveOutcome.inserted/reused`, FULL/PREFIX records | New objects/exact reuse and selected record kinds | Encoding time or physical write volume |
| `packs_created`, `pack_appends` | Pack INSERT/whole-row UPDATE calls | Bytes rewritten or pages dirtied |
| `commits` | Acknowledged COMMIT calls, including final publication | Time spent in COMMIT; total BEGINs or journal bytes |
| `statements` | Only batched `objects` INSERT statements | Total SQL statement count; pack/index/transaction queries |
| `presence_queries` | Calls to paged presence lookup, not necessarily individual SQL pages | SQL time or every membership/location lookup |
| `delta` | FULL preparations, trials, selections and refusals | Signature calls/bytes, payload vs group codec CPU |
| `chain` | Objects, edges, encoded/canonical bytes, depth, group decodes during accumulated base resolution | Every eligibility/depth probe, physical I/O or pack-fetch count |
| `pool` | Leaves, reused/new values, groups, delta/full leaves and trials | Pooled index lookup CPU or SQL statements |
| `candidate_index_bytes()` | Fixed bounded in-memory index allocation | Signature/lookup/flush CPU |

Definitions: `core/crates/layerfs-storage/src/cas/store.rs:30`, `:547`, `:563`,
`:570`; `cas/owner.rs:31`; `encoding/delta/select.rs:31`;
`encoding/delta/read.rs:27`; `cas/pool_lane.rs:33`.

The public SaveOutcome does not expose transaction charged bytes or transaction
begin count (the internal owner carries the latter). Those quantities cannot be
reconstructed exactly from COMMIT count and final Store size. Final stored bytes
do not count repeated writes.

## Concrete source findings

### 1. Repeated full-payload signature scan: smallest candidate

`core/crates/layerfs-storage/src/encoding/delta/select.rs:298` computes
`signature(raw)` to search the content index after declared candidates fail.
If no eligible candidate results, `:312` computes the same signature again to
insert this object. An index-acquired candidate that fails its work budget or
loses its prefix trial also recomputes the signature at `:336` or `:369`.

The signature walks every 16-byte window of the payload
(`encoding/delta/candidates.rs:136` through `:159`). This is a repeated linear
scan of one payload, not a scan of the entire Store or similarity table.
`Candidates::find` itself probes a fixed number of signature references
(`candidates.rs:388`); `flush` writes only admissions since the previous flush,
bounded by ring size (`:353`). Neither supports an unbounded-index-scan claim.

Proposed one-variable change: retain a lazy `Option<[u64; 8]>` within `select`
and use it for both fallback search and any subsequent FULL admission. Keep the
no-depth branch and successful advisory-prefix path lazy: do not add hashing to
paths that currently avoid it. Reuse must cover all three downstream insertion
branches, not only the no-candidate branch. No cache across objects, extra heap
map, candidate policy change, dependency, codec change or signature-format change.

Correctness expectation: the same immutable bytes produce the same signature;
candidate order, selected representation, counters, emitted pack bytes and roots
must remain identical. Memory increment is one optional fixed-size signature per
active selection; no file-size-proportional buffer. Measured time saving: unknown.
The existing `no_candidate` total is NOT the exact number of duplicate scans:
roles and acquisition paths differ, and later rejection/loss paths also exist.

### 2. Whole-pack rewriting is real; its cost has not been isolated

`cas/placement.rs:125` invokes `select_many` with one group. Placement assembles
the complete open pack at `pack/placement.rs:179`; SQLite appending is actually
`UPDATE object_packs SET data = ?2` at `sqlite/write.rs:80`. Each append therefore
submits a new whole pack, and `cas/placement.rs:200` charges that whole byte length
to the transaction. This is deliberate honest accounting, not an arithmetic bug.

For k equal-size groups fitting one pack, repeatedly assembling prefixes copies
the group bodies 1+2+...+k times; this is bounded by pack limits, not an unbounded
whole-repository quadratic claim. k, rewritten bytes, CPU and actual disk traffic
have not been measured here. Current transaction cadence follows both declared
row and byte thresholds (`cas/lifecycle.rs:129`). Raising either threshold would
relax a bound rather than remove work.

`select_many` can already coalesce groups in a single call. Doing that at the
owner level requires preserving same-save reads, pending dependency availability,
member-row publication, ordinary group bounds, tail-memory accounting, pack cache
invalidation and failure cleanup. Deferred writes are therefore a larger lever,
not the initial patch. `write_pack` explicitly invalidates caches because appends
move group bodies (`cas/placement.rs:175`). Do not retain stale decoded pack layout.

### 3. Codec work must be separated from acquisition and indexing

Non-tree payloads first encode FULL (`encoding/delta/select.rs:270`), and eligible
candidates may additionally encode PREFIX (`:342`). This is necessary for the
current complete-size choice and cannot simply be removed while claiming the
same policy. Group compression is separate (`cas/placement.rs:118`). The actual
constants are PAYLOAD_LEVEL=3 and GROUP_LEVEL=19 (`encoding/codec.rs:94`, `:102`);
nearby comments referring to level 9 are stale evidence, not current execution.

## Smallest defensible measurement plan

1. Preserve caller begin/accept/finish/index boundaries. Capture existing public
   outcome counters before adding any new observer. An external CPU sampling
   diagnostic can distinguish signature, zstd, pack assembly and SQLite stacks
   without product hooks, but statistical samples cannot close exact wall-time
   arithmetic or stand in for phase CPU. Retain profiler overhead and sampling
   limitations; do not mix it into the unprofiled gate sample.
2. The current public APIs cannot produce an exact live save codec/index/SQL
   decomposition. If that exact decomposition is required, introduce genuine
   optional production telemetry, using the existing telemetry crate and bounded
   per-save aggregates. No test-only feature or per-object/per-syscall trace nodes
   (`cas/store.rs:375` explicitly explains that node-budget failure).
3. Necessary aggregate boundaries are: membership/presence lookup; signature and
   candidate lookup; eligibility/base reconstruction; payload FULL/PREFIX codec;
   pooled index/value processing; group codec; pack assembly; pack/object SQL;
   candidate-index flush; COMMIT/BEGIN. Record call counts and bytes beside elapsed
   time. Keep nested intervals separate or subtract them explicitly; elapsed time
   is not CPU time. Report uninstrumented overhead as a residual, not codec cost.
4. Public codec APIs (`encoding/codec.rs:241`, `:303`, `:386`) and public
   `candidates::signature` permit focused harness diagnostics without changing
   private levels or exposing new test hooks. A replay of those calls can explain
   an algorithm's isolated cost, but is not attribution of the original save:
   it must include actual role/prefix/population identity and its own cache state.

## One-variable experiment acceptance

First candidate is signature reuse, conditional on the coordinating squad's next
target decision. Retain the same corpus pins, state selection, object order,
advisories, levels, capacities, one construction worker and instrumentation in
both arms. Compare all roots, canonical readbacks, exact failure outcomes,
representation/pack/transaction counters, Store bytes and memory bounds. A focused
external test must exercise index miss, advisory hit, index hit and downstream
FULL outcomes through the real production API. Do not add a signature test hook.

Then one stride10 sample per arm, with fresh output, lock and declared machine/
cache state; confirm on stride3 only if correctness and work reduction hold.
Do not run stride1. Do not select a better repeat. Resolve/record complete-command
ceilings before execution; retain all failures and label unrun work NOT_RUN.
A successful semantics proof with no measurable latency gain is still a valid
result, but cannot resolve #190's time target.

No new performance experiment was run by this squad. Exact signature-call/byte
counts, pack rewritten bytes and codec/index/SQL phase times remain NOT_MEASURED.
No v0.1.6 transaction count is used as a direct comparator: the legacy pipeline
and phase boundaries differ. The inherited 11.370679212-second Commit sum remains
a historical tripwire, not a like-for-like codec or SQLite baseline.

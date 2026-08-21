# Phase 4 invariant and experiment matrix

This matrix prevents a performance idea from changing the question it claims
to answer. Sources are the current [algorithm specification](../../../implementation-detail/phase-4/algorithm/spec.md),
[complexity analysis](../../../implementation-detail/phase-4/algorithm/complexity-analysis.md),
[full-create plan](../../../implementation-detail/phase-4/wp4m/f-series/planning/full-create-plan.md),
and accepted F2/F4 reports.

## Semantic invariants

| Area | Must remain true | Why it is protected | Permissible research variable |
|---|---|---|---|
| Canonical object bytes | Exact framing, field ordering, limits, decoding, and typed malformed-input behavior | Bytes define stable identity and interoperability | Construction API and buffer ownership, if output is byte-identical |
| `ObjectId` | Domain-separated BLAKE3 of complete canonical bytes | Root, CAS, delta, and closure authority depend on it | How bytes are streamed to the same hasher; not the digest or domain |
| Raw `ChunkId` | Exact hash of raw chunk bytes under its frozen domain | CDC occurrence identity is distinct from canonical object identity | Shared input traversal if both exact outputs remain independently checked |
| CDC sequence | Frozen FastCDC 8/16/32-KiB profile, exact ordered boundaries, fragmentation independence | A boundary change changes chunk IDs, mappings, roots, deltas, and dedup behavior | Scanner implementation only when the exact sequence is preserved; a new CDC algorithm requires a versioned format/profile migration |
| CAS immutability | Same ID cannot authorize different kind, length, or bytes | Prevents forged reuse and semantic aliasing | Exact locator/index and operation-local proof shape |
| CAS reuse | Persisted incumbent is authenticated, not trusted by existence/key alone | A key or index is a locator, not integrity authority | A bounded receipt may remove repeat reads only if it binds store, epoch, locator/range, object ID, and transaction authority |
| File mapping | Ordered occurrence identity, cumulative summaries, canonical K/F partition, nonfinal fullness, minimal height | Exact ranges, deterministic roots, and COW proofs depend on the topology | A separately versioned mapping profile/data structure with migration and golden tests |
| Directory mapping | Canonical order, adjacency, duplicate rejection, bounded pages/indexes | Namespace meaning and deterministic identity | Page/index structure under equivalent semantics; count-changing policy with a new format if required |
| Root and delta | Root is exact canonical directory-wrapper identity; delta order/replay/parent-child semantics remain exact | They are the visible snapshot and transition authority | Internal construction path; never a second mutable source of truth |
| Closure | Complete strong-edge closure is recomputable and freshly authenticated at the required boundary | Prevents publication of dangling or forged graphs | Transaction-local composable proof that is single-use and independently checked after commit |
| Errors | Exact typed boundary errors, first failure, cleanup failure, and ambiguity provenance | Callers and recovery logic depend on them | Internal representation only if external identity and precedence stay exact |

## Durability invariants for the accepted SQLite profile

| Contract | Frozen behavior | What would require separate authorization |
|---|---|---|
| Execution | Synchronous caller thread | Worker, async, or pipelined publication |
| Writer | One writer transaction | A second transaction/database or hidden retry |
| Publication | One complete visible-head COMMIT | Split visibility or more than one durability boundary |
| SQLite mode | Rollback journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`, `mmap_size=0` | WAL, weaker sync, memory temp store, mmap, or a changed page profile |
| Atomicity | Object/root/delta/receipt/generation/authority become visible together | Separate carrier/database publication without a proven crash protocol |
| Ambiguous COMMIT | Fresh independent read-only reconciliation | Treating dispatch, return, or wall time as proof of outcome |
| Residue | No final journal/WAL/SHM for the accepted path | Persistent sidecars or new serialized optimization metadata |

A physical-profile or backend experiment may deliberately change a row above,
but then it is not an F2/F4 one-variable candidate. It needs its own version,
base image, crash model, migration, storage equations, and acceptance campaign.

## Resource invariants

| Resource | Hard rule | Evidence required |
|---|---|---|
| Mapping-owned memory `Q` | Sum every simultaneously live owned capacity exactly; checked arithmetic; terminal zero on every exit | Component equation, high water, cleanup/overflow tests |
| Asymptotic resident state | No source-sized vector, occurrence vector, visited set, cache, SQL text, or parameter list | Analytical bound plus 100/512-MiB slope |
| Fixed buffers/groups | Allowed only under a declared byte/count cap | Exact admission equation and cap test |
| RSS/footprint | Independent from `Q` | External process observation; never inferred from allocation accounting |
| Physical I/O | Never inferred from logical length, pager pages, RSS, `Q`, or wall time | VFS/syscall/filesystem/media observation, or `Unavailable` with reason |
| Durable bytes | Separate logical, apparent, and allocated main/journal/sidecar values | Pre/post endpoint snapshots and residue audit |
| Metadata | No new serialized optimization metadata by default | Format/profile amendment plus 100-GiB projection if proposed |

## Performance invariants

The 100-MiB durable timer is:

```text
source processing start
  -> source read + CDC
  -> canonical construction + CAS persistence + mapping
  -> complete pre-COMMIT qualification
  -> one SQLite COMMIT return
  -> measured state dropped
```

Post-COMMIT reopen, fresh scrub, reconstruction, and range verification are
separate protected phases. A candidate may not move work past COMMIT and call
that a durable win. The row-level equations, identities, object/byte work,
transaction/COMMIT count, storage, and terminal `Q=0` are hard gates.

## Lower bounds and known non-bounds

**Derived lower classes:** previously unseen full capture requires
`Theta(source bytes)` inspection; required raw and canonical identity domains
require complete coverage of their inputs; durable creation must persist the
new canonical information; full scrub/reconstruction remain linear in the
authenticated closure/output.

**Not lower bounds:** current statement count, SQLite page count, 4-KiB page
size, fixed K64/F64 ordinal grouping, one BLOB row per object, number of
canonical-buffer copies, and present index layout. Those are implementation or
format choices and are valid research targets when the comparison remains
honest.

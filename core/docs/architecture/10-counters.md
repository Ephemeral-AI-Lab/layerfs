# Counters and receipts

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; the two `15.6` counter readings corrected by #178 **C1**
(2026-09-18) and the `opens` counter added by #178 **V3** (2026-09-18) are marked
in place and carry their own commits, as is the `PoolCounters` home moved from
`cas/owner.rs` to `cas/pool_lane.rs` by #178 **P2-0** (2026-09-18) and the
`statements` counter added by #178 **V5** (2026-09-18) and the `group_decodes`
counter added by #178 **V6** (2026-09-18). Scope, method,
measurement status and upkeep are stated in the [index](README.md).

Chapter numbers are global to the set: this paper holds **chapter 15**.

---

## 15. Counters and receipts

### 15.1 Why this paper exists

Every C1 and C2 operation returns typed counters describing the work it actually
did. They are the only honest answer to "what did that cost", and they are what
Stage 6 ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) must
collect. Until now the set cited them ad hoc with no single inventory.

**Two rules govern everything below.**

1. **A counter reports work performed, not a durability or storage claim.** C1's
   own documentation says this of `DiscardingConsumer`: *"Its counters describe the
   emitted object set, not a stored one."* Acknowledged persistence belongs to the
   consumer.
2. **A counter that cannot fail an assertion is not evidence.** `SaveOutcome` has
   no `committed: bool`, and the source records why: *"a field that can only ever
   hold one value cannot fail an assertion, and one used to sit here doing exactly
   that."* Holding a `SaveOutcome` at all **is** the acknowledgement.

### 15.2 The inventory

#### C1 — file construction, edits and reads

| Type | Where | Fields |
| --- | --- | --- |
| `CdcCounters` | `file/cdc/gear.rs` | `bytes_scanned`, `chunks_emitted` |
| `EditCounters` | `file/edit/tree.rs` | `nodes_read`, `nodes_created`, `payloads_created`, `payload_bytes`, `peak_deferred_bytes` |
| `ReadCounters` | `file/mapping/read.rs` | `nodes_read`, `node_batches_read`, `max_node_batch`, `payload_ids_read`, `payload_batches_read`, `max_payload_batch`, `payload_bytes_read` |

#### C1 — filesystem

| Type | Where | Fields |
| --- | --- | --- |
| `SortedWork` | `filesystem/sorted/page.rs` | `pages_read`, `read_waves`, `pages_created`, `pages_reused`, `change_keys`, `untouched_subtrees`, `peak_scratch_bytes` |
| `ObjectWork` | `filesystem/objects.rs` | `objects_read`, `read_waves`, `bytes_read`, `objects_emitted`, `bytes_emitted` |
| `ValidationWork` | `filesystem/validate.rs` | `objects_read`, `read_waves`, `inode_demands`, `inode_pages_read`, `directory_pages_read`, `entries_examined` |
| `ReferenceWork` | `filesystem/references/reduce.rs` | `rows_touched`, `rows_spilled`, `base_records_read`, `base_waves`, `final_values`, `final_removals`, `serials_scanned`, `peak_pending`, `runs` |
| `MergeWork` | `filesystem/references/merge.rs` | `rows_written`, `rows_read`, `runs_created`, `merges`, `peak_level`, `peak_live_runs`, `peak_run_bytes` |
| `ReleaseWork` | `filesystem/references/release.rs` | `pages`, `entries`, `base_records`, `released`, `traversed_directories`, `peak_depth` |
| `FilesystemUpdateCounters` | `filesystem/update.rs` | all of the above, grouped, plus `bindings_added`, `bindings_removed`, `directory_updates`, `base_records_read` |
| `FilesystemReadWork` | `filesystem/read.rs` | `directory`, `inode`, `attributes` |

#### C2 — storage

| Type | Where | Fields |
| --- | --- | --- |
| `SaveOutcome` | `cas/store.rs` | `reused`, `inserted`, `packs_created`, `pack_appends`, `commits`, `statements`, `full_records`, `prefix_records`, `delta`, `chain`, `pool` |
| `DeltaCounters` | `encoding/delta/select.rs` | `prepared_full`, `trials`, `prefix_selected`, `full_losses`, `no_candidate`, `absent_candidates`, `ineligible_candidates`, `work_exceeded` |
| `ChainCounters` | `encoding/delta/read.rs` | `objects`, `edges`, `encoded_bytes`, `canonical_bytes`, `max_depth`, `group_decodes` |
| `PoolCounters` | `cas/pool_lane.rs` | `leaves`, `reused_values`, `new_values`, `groups`, `delta_leaves`, `full_leaves`, `trials`, `work_exceeded` |
| `StoreReadCounters` | `cas/store.rs` | `objects`, `packs_read`, `pages`, `ceiling`, `edges`, `max_depth`, `canonical_bytes`, `group_decodes`, `opens` |
| `CleanupReport` | `sqlite/cleanup.rs` | `objects`, `packs`, `pages` |

`OutcomeCounters` is the internal form `SaveOutcome` is built from; it carries
`transactions` and `commits` separately, which `SaveOutcome` collapses to `commits`.

`group_decodes` (added at #178 **V6**, 2026-09-18) counts the ordinary-lane group
**bodies** a read actually decompressed, charged in `encoding/decode.rs` where the
decompression happens and surfaced on `ChainCounters`, `ReadCounters`,
`StoreReadCounters` and - summed over an operation's waves - on
`StoreProvider::group_decodes()`. It is the counter that prices one decode per
**record** inside a group that is read once: `packs_read` charges one per pack per
wave and `objects` one per returned object, so neither can see it. Every other lane
decodes a per-record frame rather than a group body and charges nothing here.

The decoded-group cache (#178 **P2-4**, 2026-09-18) is what the counter then
measures: `encoding::GroupCache` retains decoded ordinary-lane bodies keyed by
`(pack, group)`, owned by the **read operation** (the pooled `ReadSession`), bounded
by `DECODED_GROUP_CACHE_BYTES` (512 KiB) with the pooled cache's wholesale release,
and dropped with the session. A hit charges no `group_decodes`, so after P2-4 the
counter reads the number of **distinct groups** an operation touched: on the frozen
pipeline readbacks that is 2 → 1. The cache is a reading of work, never a
visibility shortcut: the resolver checks the location's pack against the wave's
ceiling *before* consulting it, because the ceiling is re-read per wave while the
cache outlives a wave.

`statements` (added at #178 **V5**, 2026-09-18) counts the `INSERT` statements
issued for object rows - **statements, not rows**: a multi-row `INSERT` of `k` rows
is one statement, while `inserted` counts the rows. It is charged where the
statement is issued, from the writer's own report (`sqlite/write.rs`), and it
scopes to the `objects` insert alone: pack writes, value-group inserts and
transaction statements are already `pack_appends`/`packs_created`,
`pool.groups`, `transactions` and `commits`. A save that reuses everything issues
none. It exists so that an INSERT-batching change has a counter that moves while
`inserted` stays exactly the same.

### 15.3 The counters that make claims checkable

Most counters describe work. These four let a reader **falsify a claim** rather
than trust it.

```text
   CLAIM                          COUNTER                          WHAT IT SHOWS
   ─────                          ───────                          ─────────────
   "a localized edit does not     EditCounters.payload_bytes       new payload BYTES;
    rewrite the file"              + payloads_created               the rewrite cost
                                   + nodes_created

   "an update touches only the    SortedWork.pages_reused          stored pages whose
    changed path"                  + untouched_subtrees             bytes were reproduced
                                                                    exactly / referenced
                                                                    WITHOUT being read

   "FULL was chosen by policy,    DeltaCounters.{no_candidate,    four POLICY outcomes
    not because something          absent_candidates,               kept separate from
    failed"                        ineligible_candidates,           work_exceeded
                                   work_exceeded}
                                   and prefix_selected / full_losses

   "the read saw only              StoreReadCounters.ceiling        the visibility
    acknowledged packs"                                             watermark applied to
                                                                    every acquired location
```

Three of these are worth stating as the trap they avoid:

- **`EditCounters` reports all zeros on non-frontier paths.** Complete construction
  and a whole-file result hold no unfinished mapping node, so every field is zero
  there. That is the honest answer, not a missing measurement — but a reader who
  expects non-zero will misread it.
- **`SortedWork.pages_reused` and `untouched_subtrees` are different things.**
  `pages_reused` means a page was re-encoded and produced byte-identical output, so
  it was not emitted. `untouched_subtrees` means a subtree was referenced by
  identity and **never read**. Only the second is the COW claim.
- **`DeltaCounters` distinguishes four policy outcomes from one failure.** A codec,
  allocation or read failure is returned as an error and never becomes a
  representation; `work_exceeded` counts a *policy* refusal (the chain would exceed
  a fixed budget), which is not a failure. Pooling the two would make a
  no-fallback architecture look like it had a fallback.

### 15.4 The twelve 4,096s — a disambiguation

`4_096` appears twelve times in `core/crates/*/src`. **They are not one limit**, and
three of them are not counts at all.

| # | Constant | File | Bounds | Kind |
| ---: | --- | --- | --- | --- |
| 1 | `MAXIMUM_READ_DEMANDS` | content `filesystem/objects.rs` | ids per C1 read wave | count — **read batch** |
| 2 | `READ_OBJECT_LIMIT` | storage `policy.rs` | ids per C2 read wave | count — **read batch** |
| 3 | `MAXIMUM_WALK_ENTRIES` | content `filesystem/limits.rs` | entries per cycle walk | count |
| 4 | `MAXIMUM_EDITS_PER_OPERATION` | content `file/edit/input.rs` | edits per stream | count |
| 5 | `MAXIMUM_ATTRIBUTE_KEYS` | content `filesystem/limits.rs` | keys per listing | count |
| 6 | `DEFAULT_MAXIMUM_PENDING` | content `references/reduce.rs` | reducer pending rows | count |
| 7 | `DEPTH_CACHE_ENTRIES` | storage `delta/select.rs` | cached chain costs | count |
| 8 | `TABLE_BUCKETS` | storage `pool/delta.rs` | hash buckets | count |
| 9 | `SCAN_LIMIT` | content `references/backing.rs` | dir entries scanned | count |
| 10 | `MAXIMUM_PATH_BYTES` | content `filesystem/limits.rs` | path length | **bytes** |
| 11 | `MAXIMUM_SYMLINK_TARGET_BYTES` | content `filesystem/limits.rs` | symlink target | **bytes** |
| 12 | `SINGLETON_FRAMING_SLACK` | storage `policy.rs` | pack framing headroom | **bytes** |

**None of them is the write batch.** That is a different number, and it is smaller:

```text
   WRITE  (C2 save)                        READ  (C1 wave → C2)
   ────────────────                        ────────────────────
   BATCH_OBJECT_LIMIT            512       MAXIMUM_READ_DEMANDS   4,096
   BATCH_CANONICAL_BYTES_LIMIT   512 KiB   READ_OBJECT_LIMIT      4,096
   TRANSACTION_ROW_LIMIT       8,191
   TRANSACTION_CANONICAL_BYTES 4 MiB − 1
```

Two of the twelve are the read wave, and they are **deliberately matched across the
boundary** — `filesystem/objects.rs` states that "the storage side declares the
same figure, and the pool that feeds both is bounded by it". The other ten are
unrelated limits that happen to share a round binary number.

`MAXIMUM_PATH_BYTES = 4_096` means a 4,096-**byte** path, which has nothing to do
with batching; `filesystem_limits.rs` pins it that way
(`a_path_is_accepted_at_4096_bytes_and_refused_at_4097`).

### 15.5 Bounds a receipt must respect

| Bound | Value | Consequence for a receipt |
| --- | ---: | --- |
| `BATCH_OBJECT_LIMIT` | 512 | a wave drains every 512 objects, not at 4,096 |
| `BATCH_CANONICAL_BYTES_LIMIT` | 512 KiB | usually binds first: 512 average objects ≈ 512 KiB only at ~1 KiB each |
| `TRANSACTION_ROW_LIMIT` | 8,191 | one transaction spans many waves |
| `TRANSACTION_CANONICAL_BYTES_LIMIT` | 4 MiB − 1 | `2²²−1`, an encoding ceiling, not a round number |
| `MAXIMUM_WALK_ENTRIES` | 4,096 | charged **per walk**, not per operation |
| `READ_WAVE_OBJECTS` | 32 | mapping read waves; ≤ 1 MiB payloads |
| `EDIT_DEFERRED_LIMIT` | 8 MiB − 1 | binds by failing the operation, never by dropping state |
| `DEPENDENCY_PACK_CACHE_BYTES` | 4 MiB | released wholesale; costs reads, never correctness |

### 15.6 Counters that are easy to misread

| Counter | Looks like | Actually |
| --- | --- | --- |
| `ObjectWork.objects_emitted` | objects stored | objects **handed to the consumer**; with `DiscardingConsumer` nothing is stored |
| `SaveOutcome.reused` | a cache hit | an occurrence served by an exact existing row, **byte-compared** (`cas/membership.rs`) |
| `ChainCounters.objects` | objects requested | objects read **including dependencies** |
| `ChainCounters.max_depth` | a policy value | the longest chain *actually reconstructed* |
| `ReferenceWork.rows_touched` | inodes affected | work, not cardinality — "a serial registered by the caller and then observed once counts twice" |
| `ValidationWork.entries_examined` | tree size | entries **charged to the walk**, which is per-walk scoped |
| `PoolCounters.reused_values` | deduped values | values that reused an existing ordinal in the bounded window |
| `StoreReadCounters.ceiling` | a limit | the watermark **applied** to every acquired location |
| `StoreReadCounters.opens` | connections per operation | connections **this wave** opened (added at V3): one `read_batch` call is one wave and opens one connection, so an operation's connection lifetime is the **sum over the waves it issued** — `StoreProvider::connection_opens()` carries that sum, and P1-2 is the item that lowers it |
| `ObjectWork.read_waves` | one per object | one per **provider call**: a grouped demand of *n* objects is **one** wave (corrected at C1; it was charged twice) |
| `SortedWork.pages_read` | stored pages requested | pages **decoded**, including the children a grouped fetch returned (corrected at C1; batched decodes charged nothing) |

### 15.7 What counters cannot tell you

Named rather than left implied:

- **Wall time.** Counters report work, not duration. Duration comes only from
  `layerfs-telemetry`, and a disabled recording reads no clock.
- **Resident memory.** `peak_deferred_bytes`, `peak_scratch_bytes` and `peak_pending`
  are the operation's *own* declared charges. They are not process RSS, and they do
  not include allocator overhead, page cache or SQLite's page allocation.
- **Crash durability.** `SaveOutcome.commits` counts `COMMIT`s. The persistence
  profile is MEMORY journal with `synchronous = OFF` and no `fsync` anywhere, so a
  commit is an acknowledgement, not a durability guarantee.
- **Whether a superseded representation was reclaimed.** Nothing reclaims it, so no
  counter reports it. See [`08-representations.md` §13.3](08-representations.md#133-no-reclamation--the-economics-that-follow).
- **A PASS or a FAIL.** Every counter here is evidence, not a verdict. Stage 6 owns
  the limits a number is compared against.

### 15.8 Timer composition

`TimingReport` (`layerfs-telemetry`) is a separate axis and composes with all of the
above: counters say *what work*, the timer says *where the time went*. Two of its
properties matter for honest reporting.

```text
   NodeOutcome                       Completeness
   ───────────                       ────────────
   Ok       returned success         Disabled   no measurement; no root
   Error    returned an error        Complete   measured, every requested node
   Unknown  NEVER RETURNED           Clipped    measured, but missing detail
            (panic, or scope
             dropped mid-run)
```

`Unknown` exists so that a node which never completed cannot read as success: *"a
consumer that reads the outcome alone must not treat a node that never completed as
an operation that completed successfully."* And `Disabled` versus `Clipped` are
different states with different consequences, so *"a caller that checks only
`is_incomplete()` therefore never fails a disabled row."*

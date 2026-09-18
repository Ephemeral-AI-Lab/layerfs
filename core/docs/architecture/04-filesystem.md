# Filesystem trees

> **Status:** Research; informative and not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 5. Filesystem trees (C1)

### 5.1 The scoped root

`core/crates/layerfs-content/src/filesystem/root.rs` — a fixed 116-byte value:

```text
   0        8     10    11    12                    44                    76
   ┌────────┬──────┬─────┬─────┬────────────────────┬────────────────────┬─────────┐
   │LFS6FSR\0│ver u16│role │flags│    profile[32]     │     scope[32]      │serial u64│
   │  8 B   │ = 1  │ = 6 │ = 0 │                    │                    │         │
   └────────┴──────┴─────┴─────┴────────────────────┴────────────────────┴─────────┘
   76        84                                    116
   └─serial──┴──────────── inode_table[32] ─────────┘

   ROOT_VALUE_BYTES = 116      (excluding the LFSO envelope)
```

The frozen profile description, hashed into `profile_id()`:

```text
   "layerfs/namespace-profile/scoped-inline/v1\0"
   "scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5"
```

That single string states the profile's parameters, and `FilesystemRoot::decode`
rejects any other profile explicitly — "before any mutation, instead of being
silently re-interpreted or converted".

### 5.2 Scoped inode identity

`core/crates/layerfs-content/src/filesystem/identity.rs`

```text
   InodeIdentity = ( InodeScope , serial : u64 )

      InodeScope  = one ObjectId, stored in the FILESYSTEM ROOT
      serial      = stored in the INODE TABLE and in DIRECTORY BINDINGS
```

Serials from different scopes are not comparable, so the type keeps the scope next
to the serial in every public identity value. `MAXIMUM_INODE_SERIAL = i64::MAX`,
and serial `0` is refused.

**C1 never allocates.** The module documentation states the contract and its
enforcement plainly:

> New serials arrive from the caller's allocator, which owns uniqueness, scope
> separation and the lifecycle of an exposed serial: the reference allocator
> durably burns a reserved range even when the work that requested it fails, and a
> caller that reuses an exposed serial would silently reinterpret retained records.
> That precondition is **stated here, enforced where it can be** (no zero serial,
> in-range values) and **never hidden inside a store**.

`scope_for_seed(seed)` derives a scope as
`ObjectId::for_bytes("layerfs/inode-scope/v1\0" ‖ seed)`.

### 5.3 Two real page formats and exact size arithmetic

`core/crates/layerfs-content/src/filesystem/sorted/format.rs`

> Sizes are exact arithmetic (`44 + sum(row widths)`), **never trial encodings**: a
> candidate page is never cloned and encoded just to discover whether it fits.

```text
   canonical page = LFSO envelope (13 B) + node header (31 B) + rows
                    └──────────── EMPTY_PAGE_BYTES = 44 ────────────┘

   ┌──────────────────────┬─────────────────────┬──────────────────────────┐
   │                      │  DIRECTORY  (LFS6NSP)│  INODE      (LFS6INT)   │
   ├──────────────────────┼─────────────────────┼──────────────────────────┤
   │ leaf row             │ 2 + name + 8        │ 8 (serial) + 73 (value)  │
   │                      │ (len, name, serial) │ = 81 B                   │
   ├──────────────────────┼─────────────────────┼──────────────────────────┤
   │ branch row           │ 32  (child id)      │ 40 B                     │
   ├──────────────────────┼─────────────────────┼──────────────────────────┤
   │ leaf role / branch   │ 1 / 2               │ 7 / 8                    │
   └──────────────────────┴─────────────────────┴──────────────────────────┘

   NODE_VERSION = 1 for both;  NODE_HEADER_BYTES = 31
   DEFAULT_PAGE_ITEMS = 234   (reserved row cells before rows are appended)
```

The header carries `level`, a recorded subtree entry `count` and a recorded
subtree byte total. A single `Format` trait is implemented twice
(`decode` / `encode` / `width` / `heap_bytes` / `decode_scratch` / `page_items` /
`max_items` / `filled` / `fits` / `empty_allowed` / `role` / `magic`), so the
sorted engine is **one algorithm over two concrete formats** rather than two
copies.

Two trait methods exist as a pair and the source explains why they are not one:
`page_items` says when the **engine splits**, while `max_items` says what the
**canonical encoder may accept**. They differ because the reference's split
threshold admits **one row more than the page can hold**.

### 5.4 The sorted B+tree engine

`core/crates/layerfs-content/src/filesystem/sorted/page.rs`

```text
   a stored page  ──read──► authenticate ──► decode ──► check against root/non-root context
        │
        │  UNTOUCHED SUBTREES stay as their stored identity
        │  and their bytes are NEVER READ AGAIN
        ▼
   ┌────────────────────────────────────────────────────────────────────────┐
   │  what a localized update costs:                                        │
   │      the changed path  +  the siblings that PROVE THE PARTITION        │
   │  NOT the whole tree                                                    │
   └────────────────────────────────────────────────────────────────────────┘
```

An unfinished page this operation built stays **decoded and private** until the
merge proves it final. Children are read in bounded authenticated batches
(`BATCH_CHILDREN = 256`), and the accumulating right spine holds private pages.

**Filling and splitting read a running total, not a re-sum.** A page carries the
sum of its rows' encoded widths, maintained by the one funnel every row passes
through (`append_entry`), so the fill test after an append and the split point are
O(1) reads instead of an O(k) sum over every key — O(k) per page fill rather than
O(k²), with k bounded by the page ceiling (740 rows for a directory leaf). The
figure is a page's **row widths**; `Entry::bytes` is a different number (a
subtree's encoded bytes) and is not it.

The width is the widest real demand, not the demand ceiling: one branch page's
children are bounded by the directory format at 232 (1-byte names), so 256 covers
every legal page in one wave. A full-width reservation is `256 x (8,192 + 88)`
plus one decode slot — about 2.08 MiB against the 4 MiB operation lease — where
the 4,096-id demand ceiling would reserve 33.9 MiB and the narrowing loop would
clamp it on every call. A parent's batch is held while the merge descends into its
children, so an inner batch **narrows** to what the remaining lease affords
instead of refusing; `peak_scratch_bytes` rises with the width and stays inside
the declared lease.

`SortedWork` reports what one operation did:

| Counter | Meaning |
| --- | --- |
| `pages_read` | pages read, including batched ones |
| `read_waves` | canonical read waves issued |
| `pages_created` | pages this operation created and emitted |
| `pages_reused` | stored pages whose canonical bytes were reproduced **exactly** |
| `change_keys` | final-state change keys consumed |
| `untouched_subtrees` | unchanged subtrees referenced by identity **without being read** |
| `peak_scratch_bytes` | largest simultaneous scratch reservation |

`pages_reused` and `untouched_subtrees` are the two counters that make the
"localized, not whole-tree" claim checkable rather than asserted.

### 5.5 The reference reducer — the hardlink and ordering subsystem

This is the most intricate part of C1. It exists because a hardlink's link count
is a **global** property of an inode, while an operation only sees the directory
bindings it touched.

`core/crates/layerfs-content/src/filesystem/references/`

```text
   the sorted directory merge
        │  observes every ORIGINAL → FINAL binding edge it actually saw
        ▼
   ┌──────────────────────────────────────────────────────────────────────┐
   │  ReferenceReducer                                    reduce.rs       │
   │                                                                      │
   │    pending      : BTreeMap<u64 serial, Row>                          │
   │                   capped at DEFAULT_MAXIMUM_PENDING = 4_096          │
   │    declared_new : BTreeSet<u64 serial>                               │
   │                                                                      │
   │    on overflow ──► SPILL to runs                     runs.rs         │
   └──────────────────────────────────────────────────────────────────────┘
        │
        │  the two counting rules:
        │
        │    NEW inodes  → count from the bindings actually RETAINED
        │                  (so an alias added in two directories reaches the
        │                   right count with no journal replay)
        │
        │    EXISTING    → stored count + the SIGNED effect the merge observed
        │                  (so aliases outside the changed paths survive)
        │
        │    ORDERING RULE: additions are observed BEFORE removals are
        │    released — a move never drops an inode to a spurious zero
        ▼
   merge_runs ──► FinalRows                                  merge.rs
        │
        ▼
   release_zero_count: bounded page traversal of zeroed descendants
        │                                                   release.rs
        ▼
   inode table rebuilt from TYPED VALUES in ONE sorted pass
```

Base records are read **at the end**, in bounded waves
(`DEFAULT_BASE_BATCH = 32`), and **only for the serials whose effect rows need
them**. An inode whose count is unchanged and whose value was not supplied produces
**no row at all**.

Run storage is caller-supplied through `OrderingBacking`, with one concrete local
implementation over real files. The completion contract is explicit and is the
kind of thing a qualification pass has to check:

> An operation calls `OrderingBacking::release` **exactly once**, after every row
> consumer has closed its handles and **before it reports success**. A release that
> fails fails the operation. A caller that needs the resources kept past one
> operation owns them itself and passes a fresh backing to the next one.

Nothing here is free memory or free disk: one explicit account owns the bytes,
growth is reserved **before** it happens, obsolete runs give their bytes back when
dropped, and the finishing cleanup is **checked, not hidden in a destructor**.

**Tiers and their scans.** A spilled run lives in a tier (`levels[i]`), and each
tier keeps one buffered reader (`scans[i]`) whose cursor lets an ascending sweep
read that tier's rows exactly once. The two are index-parallel: `scans[i]` exists
only while `levels[i]` holds the run it scanned. A spill into level `k` replaces
the runs of tiers `[0, k]` and writes the merged run back into `k`, so it drops
**only those scans** — a tier above `k` keeps its run and therefore keeps its
cursor. Clearing every tier's scan on every spill is what made a lookup restart
from the front of a higher tier's run and re-read the rows its cursor had already
passed; the ascending-sweep property is what a receipt on this subsystem has to
show.

### 5.6 The operation boundary

`core/crates/layerfs-content/src/filesystem/objects.rs`

Every filesystem entry point takes one `FilesystemObjects` value **instead of a
store**:

```rust
FilesystemObjects { reader: &dyn AuthenticatedObjects,
                    consumer: &mut dyn FinalizedConsumer,
                    work: ObjectWork }
```

Reads go to the caller's authenticated provider exactly as file construction and
logical reads already do, and every finalized tree page moves to the caller's
bounded consumer. "Nothing here opens a database, a pack or a file, and nothing
retries: a refused object ends the operation."

`MAXIMUM_READ_DEMANDS = 4_096` bounds one wave on **both sides** — the storage side
declares the same figure — so a wave is always a bounded amount of provider and
decode work rather than an unchecked slice length. `read_batch` rejects an
over-large group *before* calling the provider and re-checks exact cardinality
after.

`update.rs` orchestrates the whole operation and returns `FilesystemResult` with
`FilesystemUpdateCounters` broken out across objects, directories, inodes,
references, release and validation. Completion here is **this operation's own
result**; acknowledged persistence is the consumer's.

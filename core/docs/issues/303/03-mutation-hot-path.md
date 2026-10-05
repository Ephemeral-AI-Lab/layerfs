# 03 — The mutation hot path

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. "Fast" is a design priority here, not a result: no number
> in this document is a measurement of the design. Statement and page figures
> are design targets to be confirmed by count. Claim labels are defined in the
> [entry point](README.md#claim-labels).

## 1. The path

[proposed design, implementing the owner requirement]

```text
                     FUSE mutation request
                              |
            lock Workspace.core                     waits; never refused for contention
                              |
            validate against the newest rows        0-2 point statements
                              |
            BEGIN
              update the affected overlay rows      a small constant number of statements
            COMMIT                                  in-memory journal; pages written once; no sync
                              |
            unlock
                              |
            reply                                   the kernel's own caches are already coherent

  Never on this path:
    an SQLite checkpoint                (there is no write-ahead log)
    canonical construction, CDC, delta selection, a Save, a history call
    a read of base payload
    a scan or copy of the file
    a replay of earlier edits
    a scan of the Workspace
    a file create, unlink, fsync or read-back verification
    a kernel notification
    a bridge call                       (two bounded exceptions, §6)
    reclamation                         (one bounded exception under disk pressure, §7)
```

Three operations that are easy to conflate, and are kept distinct:

| Operation | What it is | When |
| --- | --- | --- |
| SQL transaction commit | The atomic end of one mutating request's row changes | Every mutating request, before its reply |
| SQLite checkpoint | Copying a write-ahead log into the database file | **Never.** The overlay has no write-ahead log ([02 §2](02-base-overlay.md#2-database-placement-and-profile)) |
| Workspace Commit | Capture, canonical construction, Save, history transition, install | When the SDK asks, on its own thread ([04](04-concurrency-commit.md)) |

**Why one transaction per request and not a batch.** Several requests could
share one SQL transaction on the single connection, since later statements see
earlier uncommitted ones. That would cut page writes for repeated small writes.
It is not adopted: a batch whose `COMMIT` fails, for example on a full disk,
would lose operations that were already acknowledged. One transaction per
request keeps every acknowledged operation independent of every later one.
Under this profile a transaction's fixed cost is a few page writes with no sync
and no lock system call, so the batch is not needed to make the path short.

## 2. What disappears

[source-verified for the left column; proposed for the removal]

Paths are under `core/crates/` unless marked.

| On the acknowledgement path today | Source | In the replacement |
| --- | --- | --- |
| A new file per 4 KiB page: create, five `fstat`, `fallocate`, direct write, read back, two SHA-256 passes, compare | `layerfs-workspace/src/backing/active/pages.rs:447-698` | SQLite page writes |
| Unlink of every replaced page file | `layerfs-workspace/src/backing/active/pages.rs:749-824` | Pages return to the freelist |
| `index.maintain` twice, two reclamation passes and a compaction plan per mutation | `layerfs-workspace/src/backing/active/generation.rs:679-844` | Off the path (§7) |
| Host maintenance before every write over 128 bytes | `layerfs-workspace/src/backing/payload.rs:719` | Off the path |
| One `inval_inode` to the kernel per WRITE | `layerfs-workspace/src/filesystem/write.rs:398-406` | None |
| One upstream `ReserveInodes { count: 1 }` per create and mkdir | `layerfs-workspace/src/filesystem/active_create.rs:116-140` | In-memory range |
| An upstream negative lookup before create | `layerfs-workspace/src/filesystem/active_create.rs:73` | Cached base lookup |
| About twelve acquisitions of the Workspace mutex per write, one held across storage I/O | `layerfs-workspace/src/filesystem/active_file.rs:35-241` | One acquisition |
| `EBUSY` when another callback is in flight | `layerfs-workspace/src/runtime/coherence.rs:490-493` | A wait |
| A node-table scan on FORGET and RELEASE | `layerfs-workspace/src/runtime/state.rs:429-465` | A map update |
| A `format!` per callback for a disabled trace; a second copy of every READ | `layerfs-fuse/src/adapter.rs:251-255`; `layerfs-workspace/src/filesystem/read_origin.rs:31-45` | Not built |
| From the #305 prototype: a WAL `stat` after every write and an inline `PASSIVE` checkpoint | `core/experiment/real-tree/src/overlay/db.rs:97`, `:136-155` on `codex/phase7-experiment-305` | No write-ahead log |
| From the #305 prototype: `BEGIN` and `COMMIT` around every read | same file, `:106-113` | Reads are bare statements under the mutex |

## 3. Cost of representative operations

[proposed design; counts are targets]

"Statements" excludes `BEGIN` and `COMMIT`. "Hint" is the append hint of
[02 §8](02-base-overlay.md#8-caches-owner-key-visibility-invalidation). Every
row has the same lock (one hold of `Workspace.core` for the listed SQL), the
same transaction boundary (one, around the writes), the same maintenance (none,
except §7's pressure rule) and the same acknowledgement point (after `COMMIT`,
before any other work).

| Operation | FUSE requests | Base reads | Statements and rows | Bytes copied or rewritten | Grows with |
| --- | --- | --- | --- | --- | --- |
| 1-byte overwrite inside a base file, first touch | 1 WRITE | Base **attributes** if not cached (§6). No payload | inode seek; inode insert (`lower_len` = base size); 2 extent queries; extent insert (1 byte) | 1 byte of payload; about 3 pages | B-tree depth |
| Same byte again | 1 WRITE | none | inode seek; 2 extent queries; in-place BLOB write; inode update | 1 byte; 2 pages | nothing |
| 128 KiB sequential append, hint present | 1 WRITE | none | extent insert; inode update | 128 KiB once, about 33 pages | nothing |
| 100-byte append to a small tail | 1 WRITE | none | extent row rewrite (inline, under 4 KiB); inode update | under 4 KiB; 2 pages | nothing |
| Create an empty file | LOOKUP (miss), CREATE | A base miss for the name if the parent is a base directory, cached | name seek; dentry insert; inode insert; parent upsert | 3 small rows | nothing |
| Unlink a base file | 1 UNLINK | none beyond the kernel's earlier lookup | name seek; dentry whiteout; inode upsert; parent upsert | 3 small rows | nothing |
| Unlink a file created in this generation | 1 UNLINK | none | dentry delete; inode delete; `reclaim` insert if it had data; parent upsert | 3–4 small rows | nothing; no rows remain |
| Rename a base directory | 1 RENAME | Destination miss, cached | 2 name seeks; whiteout; dentry upsert to the same serial; 2 parent upserts | 4 small rows | **not** the subtree |
| Editor save: create temp, write, rename over | CREATE, WRITE, RENAME, plus FLUSH and RELEASE | One base miss | about 10 statements over three transactions | The file once | nothing |
| `O_TRUNC` of a 10 GiB overlay file | 1 SETATTR | none | inode seek; inode upsert; `reclaim` insert | 2 small rows | nothing on the path; reclamation later (§7) |
| Shrink a file to `n > 0` | 1 SETATTR | none | inode upsert; in-place zero of at most 128 KiB; delete of the extents beyond `n` | See §6 | The rows this request discards |
| Sparse write at 10 GiB | 1 WRITE | none | extent insert; inode upsert | The written bytes | nothing in the overlay; Commit pays for the hole |
| Write to a block the Commit captured | 1 WRITE | none | inode seek (finds the captured row); inode insert at the active generation; extent insert in a **new** stream | The written bytes | nothing; the captured stream is untouched |
| `mkdir -p a/b/c`, all new | 3 × (LOOKUP, MKDIR) | One base miss, for `a` | per directory: name seek; dentry insert; inode insert; parent upsert | small | nothing |
| chmod | 1 SETATTR | Base attributes on first touch | inode upsert | 1 small row | nothing |

A one-byte write runs five statements and dirties about three pages. It does
not construct, chunk, hash, publish or checkpoint anything.

## 4. Two traces

[proposed design]

**First write of one byte at offset 1,000,000 of a 5 MB base file.**

```text
 kernel -> WRITE(ino 4711, off 1000000, len 1)
 lock core
   SELECT newest inode row of 4711                      -> none
   base attributes of 4711                              -> attribute cache hit (the kernel looked it up)
   BEGIN
   INSERT inode (4711, A, size 5000000, lower_len 5000000, stream 9, born 0, …)
   SELECT last extent of stream 9 with off <= 1000000   -> none
   SELECT extents of stream 9 in (1000000, 1000001)     -> none
   INSERT extent (stream 9, off 1000000, 1 byte)
   COMMIT                                               -> 3 pages written, no sync
 unlock
 reply(1)
```

No base payload was read. A later read of `[999000, 1001000)` copies one byte
from the extent and asks the base for the two ranges around it.

**The 10,000th 100-byte append to a log file created in this Workspace.**

```text
 kernel -> WRITE(ino 90210, off 999900, len 100)
 lock core
   append hint for 90210: stream 12 ends at 999900, last extent inline with 2,300 bytes
   BEGIN
   UPDATE extent SET data = data || ?  WHERE id = ?     -> one leaf cell rewritten (2,400 bytes)
   UPDATE inode SET size = 1000000, mtime … WHERE ino = 90210 AND gen = A
   COMMIT                                               -> 2 pages written
 unlock
 reply(100)
```

The cost is the same as for the first append. Nothing counts appends.

## 5. Repeated edits

[proposed design]

```text
  write block X: A
  write block X: B          one extent row, replaced in place: the stream holds B
  --- Commit captures ---   the captured inode row points at stream s1 (holding B)
  write block X: C          a new inode row at the active generation, a new stream s2 holding C
                            s1 is not written again: no writer can name it
  --- install ---           the captured row is retired; s1 goes to reclaim; s2 holds C
```

| Pattern | Rows after N operations | Cost of operation N | Depends on earlier edits? |
| --- | --- | --- | --- |
| Thousands of tiny overwrites at scattered offsets of a base file | One small inline extent per distinct range | One insert, or one in-place write when the range was written before | No. Reads of a fragmented region cost in proportion to the extents in the range read, at most one per two bytes requested |
| Repeated appends | About `size / 3.9 KiB` rows for tiny appends; `size / 128 KiB` for large ones | One inline row rewrite, or one insert | No |
| Rewrites of one block | One | One in-place BLOB write, touching only the pages covering the range | No |
| Sparse writes across a large file | One extent per write | One insert | No. File size never enters |
| Truncate-to-zero and regrow cycles | The current contents, plus queued dead streams | Truncate: three statements. Regrow: ordinary writes | No. The dead streams are the backlog bounded in §7 |
| Concurrent Exec writes | — | Requests serialise on `Workspace.core`; each is one transaction | No |
| Mutations during Commit | At most one extra row per touched key, in the active generation | First touch of a captured inode copies its small inode row locally; no payload is copied | No |

There is no lifetime edit counter, no per-file extent cap, and no threshold
after which a file is rewritten whole. The bounds are resources: the disk quota
and the configured page cache.

**Costs that do grow, and why they are accepted.**

| Cost | Grows with | Justification |
| --- | --- | --- |
| Overlap handling in a write | The extents the written range intersects | They are the bytes the request itself replaces; bounded by the request size |
| Shrink to a non-zero size | The extents the request discards | Proportional to the operation's own effect; other inodes are not blocked (§6) |
| rmdir of a base directory | The base entries of that directory, once | Amortised constant per removed entry ([02 §6](02-base-overlay.md#6-names-and-inodes)) |
| Reading a scattered file | Extents in the range read | Inherent to scattered writes; Commit coalesces adjacent extents into runs |
| B-tree descent | The logarithm of the row count | — |

## 6. Necessary exceptions on the path

[proposed design]

| Exception | When | Bound |
| --- | --- | --- |
| Base **attributes** of an inode | The first mutation of a base inode in a generation needs its size, mode and mtime to write the first row | One `Attributes` item. The kernel looked the inode up before it could send the request, so this is a cache hit unless the entry was evicted. After the first touch the overlay row answers. Base **payload** is never read by a mutation |
| Base existence of a name | Create, rename and link check that the destination is absent | One cached base lookup when the parent is a base directory; none when the parent was created in the overlay |
| Serial range refill | A create finds the in-memory range empty | The range is refilled in the background when it falls below half; a synchronous `reserve_inodes` call happens only if creates outrun the refill. Ranges grow geometrically, and unused serials are never recycled |
| Directory emptiness | rmdir or rename over a base directory | One merged enumeration, stopping at the first visible child; the directory is marked busy |
| Large shrink | More than 64 extents beyond the new size | Further bounded transactions before the reply; the inode is marked busy; other inodes proceed between them |
| Multi-inode operations | rename, link, unlink | Always one transaction; at most four inode rows and two name rows |

## 7. Maintenance

[proposed design]

**There is no checkpoint to schedule.** With an in-memory rollback journal the
database file is the only file. Each transaction writes its dirty pages to it
once. There is no journal file and no write-ahead log to grow.

What needs maintaining is **garbage**: rows nothing can see.

| Garbage | Created by | Unit of work |
| --- | --- | --- |
| A dead stream | Truncate to zero; last close of an unlinked file; retirement of an inode row | Delete its extents, a fixed number of pages freed per step |
| A retired generation | Install | Keyset pass over `inode` and `dentry`: delete rows with `gen <= folded` and `pinned = 0`; push their streams to `reclaim` |

**Who does it.** One maintenance thread per daemon. It visits Workspaces in
round-robin order and performs **one bounded step** per visit: lock
`Workspace.core`, delete at most a fixed number of rows freeing at most a fixed
number of pages, unlock. A step is bounded in pages, not rows, because freeing
a 128 KiB row walks its overflow chain. A mutation can therefore wait behind at
most one step.

**Backlog bound.** `Workspace.core` keeps the count of garbage pages; it is
known when each unit is queued.

```text
  garbage pages <= high-water                    maintenance thread only
  garbage pages  > high-water                    every mutating request on THIS Workspace also
                                                 performs one reclamation step after its own
                                                 COMMIT and before unlocking
  a mutation would exceed the quota              reclamation runs first, until the request fits
  and garbage is queued                          or the queue is empty; then ENOSPC if it still
                                                 does not fit
```

The high-water mark is a configured fraction of the quota (or an absolute byte
value when no quota is set). A step frees more pages than a request can
allocate, so above the mark the backlog shrinks while writes continue. The
backlog can exceed the mark by at most what one operation discards at once
(one truncated file, or one retired generation). `ENOSPC` therefore means live
data exceeds the quota, never "garbage was not collected yet".

**Journal and database growth.** The database file grows to the high-water of
live pages plus backlog and never shrinks while open; freed pages are reused
first. It is unlinked at close. A Workspace that lives for one tool call never
runs maintenance at all.

**Stalls.**

| Source | Bound |
| --- | --- |
| A maintenance step | One step's page budget |
| A Commit construction read of captured rows | One keyset page or one extent ([04 §4](04-concurrency-commit.md#4-construction-from-the-captured-state)) |
| Capture, install | One statement each |
| The kernel throttling the daemon's `pwrite` when dirty page cache exceeds its thresholds | The device's writeback rate. This is a real resource bound on sustained payload writes for any design that writes to a file. It is why payload is written once and not twice |

**Isolation.** Garbage, the high-water mark, the quota and the pressure rule
are per Workspace. One Workspace's backlog never charges another's requests.
The maintenance thread's round-robin gives each Workspace one step per cycle
regardless of the others' backlogs.

**Under the write-ahead alternative (O-3).** The maintenance step also runs
`PRAGMA wal_checkpoint(PASSIVE)` when the log passes a configured size, with
`wal_autocheckpoint = 0` and `journal_size_limit` set to the same size. With
one connection no reader can hold a snapshot, so a passive checkpoint always
completes. It is driven by log size, never by an operation, and it runs on the
maintenance thread, never on a mutating request.

## 8. Proposed targets

[proposed design] These are proposals for the owner to freeze prospectively.
None is a result. Each names the measurement shape that would qualify it under
root `AGENTS.md` §1–§3 and `docs/general/benchmark_rules.md`. Count targets are
preferred because a count is reproducible across windows and a wall time is
not.

| # | Target | How it would be qualified |
| --- | --- | --- |
| T1 | A mutating request is acknowledged after exactly one overlay transaction and **zero** of: checkpoints, sync calls, file creates or unlinks, bridge calls (outside §6), kernel notifications, maintenance steps (outside §7's pressure rule) | Per-operation-class counters in the daemon, reported expected against observed, on 10,000 appends, 10,000 create/delete iterations, and an edit plus `git commit` |
| T2 | Statement ceilings per request: sequential WRITE 2 with the hint and 5 without; create, mkdir, symlink 4; unlink 4; rename 8; truncate to zero 3; chmod 2 | The same counters; a ceiling is a count gate |
| T3 | Statements and pages written per write are flat against the write index for 100, 512, 4,097, 10,240 and 100,000 writes to one file | The same counters, plotted against the index. Storage growth reported in bytes |
| T4 | No mutation is refused for contention | `EBUSY` count is zero with four Execs writing and with a writer running through a whole Commit |
| T5 | A mutation waits behind at most one bounded step: the lock-wait distribution during a Commit and during retirement has a ceiling set by one keyset page or one maintenance step | A lock-wait counter with its maximum, recorded per phase; reported raw |
| T6 | Overlay cost on top of the mount: on the same commands, Exec with the overlay is within a declared factor of Exec with a passthrough under the same mount profile | Matched arms, one sample each, cache state declared and enforced equally. The factor is frozen by the owner before the run; 1.25 is the proposal |
| T7 | Fixed cost of a tool call (open, mount, `true`, Commit of nothing, unmount, close) against the experiment's 100 ms reporting line | Phases reported separately with an event-resolution timer; a reporting line, not a gate, until the owner freezes one |
| T8 | Guest page cache attributable to the overlay is bounded independently of file size, if O-7 permits the mechanism | A phase-local cgroup domain from a reset cgroup; a lifetime peak cannot decide it |

No overlay measurement is admissible until the owner amends the hosting rule
(O-1).

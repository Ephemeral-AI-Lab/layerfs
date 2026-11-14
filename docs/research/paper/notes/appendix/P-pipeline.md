# P. From `write(2)` to stored bytes: the LayerFS storage pipeline (appendix notes)

Read-only research notes, 2026-11-14. Nothing was built, run, edited or checked out.

## Identities and citation conventions

- **Code (2.0):** `origin/layerfs_v2_0_0` @ `8cfb6e2734`. `v2:<path>` means
  `git -C ~/Ephemeral-AI-Lab/layerfs show origin/layerfs_v2_0_0:<path>`.
- **Benchmarks:** `LFB:` = `~/Ephemeral-AI-Lab/layerfs-benchmark` @ `e628da3`.
  - `OVN-COLD` = `LFB:comparision/ovnfs/rerun-2026-11-08-cold/report.md` (LayerFS
    `b9c4adcb`); its per-run receipts are
    `LFB:comparision/ovnfs/rerun-2026-11-08-cold/results/layerfs-<case>.run1/state.json`
    (`steps[].new_canonical/new_stored/new_metadata`, `probes[]`).
  - `MICRO` = `LFB:result/2026-11-06/fs-storage-micro/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`.
  - `QUICK` = `LFB:result/2026-11-06/fs-storage-quick/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`.
  - `HIST` = `LFB:result/2026-11-06/fs-storage-history/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`.
- **Sibling notes:** `notes:C`, `notes:B`, `notes:D`, `notes:E` =
  `docs/research/paper/notes/{C-core-storage,B-engine,D-speed,E-evaluation}.md`.
- **Labels.** **[measured]** = exact bytes or a reported timing with its source.
  **[constant]** = a value in source. **[inference]** = my reasoning from source,
  not stated in a doc or report. Wall times are diagnostics (INELIGIBLE) except the
  OVN-COLD commit column, which has an enforced guest-cache eviction (notes:E §2).
- **Byte terms** (MICRO "Raw vs stored"): *canonical* = the object bytes that are
  hashed; *new_stored* = new pack bytes; *new_metadata* = the metadata-domain subset
  of new_stored; *payload* = new_stored − new_metadata; *net* = Store file + WAL.

---

## 0. The pipeline at a glance

```text
 process write(2)                      (kernel writeback OFF: every WRITE reaches the daemon)
   |  FUSE WRITE <= 128 KiB (max_write)
   v
 [W1] recognition (writes >= 8 KiB on a file with chunked committed content):
      whole chunks equal to the file's own committed chunks -> WriteClaims
   v
 [W2] one owner job, one SQLite transaction in the Workspace's overlay file:
      fs_inode row (size, inherited_cutoff) + fs_data cells / dense runs
      + fs_data_ref pieces (claims, no bytes) + fs_truncate steps (shrinks)
      + fs_entry / fs_predecessor (namespace, rename-over)      -- reads no base payload
   v  ... more calls, any process ...
 [C1] Commit: capture = generation flip, copies no rows
 [C2] names pass -> changed-parent headers; same-path base files remembered (<= 65,536)
 [C3] values pass, one file at a time on one producer thread:
        fresh file + digest names a saved root   -> reuse root (relink)
        fresh file                               -> construct_runs_with_predecessor
        changed base file                        -> shapes pass -> edit script -> edges trim
                                                    -> apply_indexed_edits_view_backed
        mtime only                               -> base row kept
 [C4] Content: CDC (8/16/32 KiB GEAR) / whole-file object / extent-tree split+concat
      -> FinalizedObject (BLAKE3 id over canonical bytes)
 [S1] Save wave: duplicate? already stored (role+length)? -> reuse, no bytes
      new -> FULL zstd-3 | STORED (1 KiB probe) | PREFIX (one delta trial, strictly smaller)
 [S2] lane groups -> append-only 256 KiB packs (ordinary groups zstd-1) -> SQLite rows
 [P ] publication lane: head check, merge if moved, insert Commit, advance Branch
 [I ] install new base; RETIRE captured generation after the answer
```

Sources: overlay `v2:docs/architecture/engine/overlay/payload.md`; capture and
construction `v2:docs/architecture/engine/commit/README.md`,
`v2:docs/architecture/engine/commit/construction.md`; Content
`v2:docs/architecture/core/files/{README,edits}.md`; Save and packs
`v2:docs/architecture/core/physical-storage/{README,encoding}.md`; lane notes:B §3.

---

## 1. Capturing localized edits

### 1.1 Write side: what a `write(2)` leaves in the overlay

- **Kernel writeback is off**, so each `write(2)` is a FUSE WRITE that is published
  into the overlay before it returns (notes:D §4 item 4, citing
  `v2:docs/architecture/engine/kernel-routes/README.md`). One FUSE WRITE is at most
  128 KiB (`max_write`); one overlay write job is at most `WRITE_WINDOW` 256 KiB
  (notes:B §2 window table).
- **Cells and dense runs** [constant] (`v2:docs/architecture/engine/overlay/payload.md`
  "Runs and cells"; schema in `v2:engine/crates/layerfs-overlay/sql/schema.sql`
  `CREATE TABLE fs_data`):
  - A row is either **one 4 KiB cell** (1..4,096 B, trimmed to its last valid byte, with
    an optional per-byte `validity` mask) or a **dense run** of whole cells, at most
    32 KiB, never crossing a 32 KiB slot. Both rules are schema `CHECK`s.
  - The mask distinguishes a written zero from a hole; **holes cost no rows**.
  - A sequential write stores one dense row per slot.
  - A partial-cell write is "one point read and one upsert" of the overlay row
    (`payload.md` "Streams").
- **The write reads no base payload.** `Write`/`WriteOpen` "stores only its own
  bytes and reads no base payload; uncovered bytes stay inherited"
  (`v2:docs/architecture/engine/workspace/operations.md` "Files").
  - Consequence: a 100-byte overwrite into a committed 500 MiB file writes one cell row
    (two if it straddles a 4 KiB boundary) of ≤ 4,096 B data + ≤ 512 B mask, plus the
    inode's `fs_inode` row for this generation [inference from the codec in
    `v2:engine/crates/layerfs-overlay/src/payload/cells.rs` `Window::trim`: data is
    kept up to the last valid byte of the cell].
- **Shrinks** (`truncate`, `O_TRUNC`) are a "staircase" of `fs_truncate` steps: one
  touched row, one step row, a binary search; discarded cells are hidden at once and
  removed later by maintenance (`payload.md` "Layers").
- **Reference pieces** (`fs_data_ref`): `(generation, inode, file_offset) → (length,
  source root, source_offset)`; "file bytes [file_offset, +length) of this layer ==
  committed bytes of content root `source` from source_offset". A piece holds no
  bytes; `file_offset` is cell-aligned; ragged edges are stored as cells
  (`payload.md` "Pieces"; schema `CREATE TABLE fs_data_ref`).
- **Recognition creates pieces from ordinary writes**
  (`v2:engine/crates/layerfs-workspace/src/operations/file/recognize.rs` header and
  constants; `operations.md` "Files"):
  - Before its owner visit, a write ≥ `RECOGNIZE_BYTES` (one minimum chunk, 8 KiB)
    is scanned for CDC cut candidates; "a whole chunk of the write whose identity equals
    the committed chunk's is claimed". It reads the write's own bytes and the file's
    committed **mapping pages**, "no overlay row and no committed chunk".
  - Search window: 2 × 32 KiB of the request, starting up to 16 chunks back; ≤ 64
    extents, ≤ 8 hashes per search; a chunk split across two requests is carried by
    identity (forward or backward order) [constant].
  - Examination of a file stops after `UNMATCHED_BYTES` = 1 MiB of consecutive
    unclaimed writes [constant].
  - Only the file's **own** committed content is a source (gate `Never` when the file
    has no chunked committed content), so a brand-new inode is never recognized.
- **Append digests** (`payload.md` "Digests"; schema `fs_digest`): a file this
  Workspace creates and only appends to keeps a ~2 KiB BLAKE3 state; at last close
  `(inode, sequence, length, digest)` is pinned. Any non-append write ends it.
- **Rename-over** (`schema.sql` `fs_predecessor` comment; overlay README row
  "Predecessors"): `rename()` of a file over a file records `(moved → replaced)`; "a
  chain of replacements keeps one row, ending at the oldest file". No Store I/O
  (notes:C §6 "Rename-over").

### 1.2 Commit side: from rows to an edit description

- **Capture copies nothing.** Active generation G becomes captured, G+1 active
  (`v2:docs/architecture/engine/commit/README.md` row "Capture").
- **Per-file routing** (`construction.md` table "Inode row"; code
  `v2:engine/crates/layerfs-workspace/src/construction/captured/owner.rs`):
  - fresh file with a digest naming a saved root → reuse that root (relink);
  - fresh file → `construct_runs_with_predecessor` over captured runs, offered the
    rename predecessor or else the base file at the same path;
  - changed base file → `CapturedFileEdits::prepare` then
    `apply_indexed_edits_view_backed` over the retained `FileView`;
  - mtime-only change → base row kept.
- **Step 1: shapes-only pass** (`captured/normalize.rs` `prepare`; `captured/scan.rs`
  `Scan::shapes`): "a pass that learns only which bytes are local and never reads
  them". The overlay answers with indexed steps: `Gap(Inherited|Zero)`, `Local`,
  `Reference`, `Window`, `End` (`payload.md` "Streams";
  `v2:engine/crates/layerfs-overlay/src/payload/captured_runs.rs`: "Indexed forward
  metadata discovery", and "the next exclusive seek never revisits this physical
  row"). Unchanged regions are one `Gap(Inherited)` step each, so the pass costs
  O(rows touched), not O(file length) [inference from the seek design].
  - For a file **without pieces**, only `[0, min(base, final))` is visited; bytes past
    the surviving base are "one trailing insert whatever they hold" (`normalize.rs`
    comment).
  - For a file **with pieces**, the pass runs to the end, because a piece can name base
    bytes an earlier edit has not consumed.
- **Step 2: spans → `Edit`s** (`captured/script.rs`, one forward pass, constant state):
  - A span is *retained* if its bytes are the file's own base bytes at or after the
    base cursor: inherited bytes, or a piece whose root is this file's base root.
    Everything else (local cells, zeros, pieces of other roots, pieces behind the
    cursor) is *new bytes*.
  - New bytes accumulate until the next retained span, which emits
    `Edit::new(start, start + skipped_base_bytes, pending_new_bytes)`.
  - No pieces → the closing call is `overwrite`: every byte keeps its offset.
  - `finish(final_size)`: unvisited tail bytes are new; unconsumed base bytes are
    removed. This is how **append** (trailing insert) and **truncate** (trailing
    delete) are expressed.
  - Edits are written to indexed operation records in batches of 64, then sealed with
    a context record (`normalize.rs` `Writer`), so "no namespace-sized collection is
    held in memory" (`construction.md` "Records").
- **Step 3: edge trimming** (`captured/edges.rs`): only for scripted files (with
  pieces) whose length changed and whose edit both removes and inserts. It reads at
  most `WINDOW` = 2 × 32 KiB of base and of captured bytes at each edge, and retains
  the equal prefix/suffix as base mapping ("whole-chunk write recognition and
  whole-cell pieces leave ragged copied bytes literal"). Counters
  `edge_read_bytes`, `edge_retained_bytes` record it.
- **Step 4: `apply_indexed_edits_view_backed`**
  (`v2:core/crates/layerfs-content/src/file/edit/apply.rs`; `v2:docs/architecture/core/files/edits.md`):
  1. open base view (one root read);
  2. **compare pass**: if every replacement equals its base range (64 KiB windows, stop
     at first difference), return the base root unchanged (no objects);
  3. route by **result** length: Empty; WholeFile (< cutoff) — assemble kept ranges and
     replacements into one pre-sized object, base declared as explicit predecessor;
     Chunked with chunked base — `replace_chunked`; Chunked with whole-file base —
     rescan the whole result (`stream_combined`).
  4. `replace_chunked`, per edit: split stored extents at `start`, split again after
     `removed_len`, **discard the removed middle unread**, scan only the replacement
     through the normal CDC builder with an `Aligner` hint per new chunk, concatenate;
     pages off the two join paths keep their IDs and are never read; pages it emits are
     offered the base page at the same level and logical start (#92).
  - Edits use **current-result coordinates** and "are never reordered or merged; their
    boundaries decide the chunk layout" (`edits.md` "Edit model").
  - Bytes read: mapping pages on root-to-boundary paths; payloads only for the compare
    pass of same-length replacements (`edits.md` "Cost").

### 1.3 Representation of each change kind

| Change | Overlay rows at write time | Edit description at Commit | Content work |
| --- | --- | --- | --- |
| Overwrite in the middle | cells / dense runs over the written range; base bytes stay inherited | `Edit::overwrite(a, b)` (script `overwrite`) | split at a and b, scan only `b−a` bytes, Aligner hint = old overlapping chunk |
| Append | cells/dense runs past old EOF | `finish`: one trailing `Edit::new(base_len, base_len, n)` | scan only the appended bytes; old last chunk kept as is (the partition differs from fresh construction) |
| Truncate (shrink) | one `fs_truncate` step + cut of one row | `finish`: `Edit::new(new_len, base_len, 0)` | split + discard, no scan; e.g. DEL500 (cut 500 MiB of 1000 MiB) stored **8,397 B, all metadata** [measured, OVN-COLD run1 receipt; report 8.2 KB] |
| Extend / sparse hole | no rows (holes cost no rows) | gap `Zero` → new bytes delivered as `FileRun::Zero` | repeated-chunk path: interior zeros as one repeat count, identical pages reused (notes:C §2) |
| Written zeros | dense rows of zeros | a window whose bytes are all zero is handed out as `FileRun::Zero` (`scan.rs` `read`) | same zero path |
| Insert that shifts the tail (rewrite tail in place) | recognized whole chunks become pieces pointing at shifted base offsets; ragged edges are cells | script retains pieces of its own base; new bytes form one insert-like edit; edges trimmed | scan only the residue; see I10 below |
| Same inode `O_TRUNC` + full rewrite | shrink step, then pieces for recognized chunks + cells for the rest | as above | M06: 1-B insert at start of a 64 MiB file → 1,001 B stored (54 B payload) [measured, MICRO "M06"] |
| Rewrite with identical bytes | cells | edits whose replacement equals base | compare pass returns base root; no object |
| Rename-over (`tmp` → `f`) | new inode's rows + `fs_predecessor(new → old)` | none: a fresh file | **fresh construction** with predecessor (whole-file base, or Aligner chunk bases) |
| `rm f; write f` (recreate) | new inode at same path | none | fresh construction, offered the base file at that path (#88) |
| Copy to a new path (`cp`) | appends from 0, digest pinned at close | none | relink: chunked root reused via `file_digest`; small files rebuilt to the same object ID (`commit/README.md` row "Relink") |
| mtime only | inode row | none | base row kept (`construction.md`) |

### 1.4 When a file is constructed from scratch

- Any file the base does not hold under that serial: a new path, a temp file renamed
  over the target, a recreate. `construct_new` requires "no base file"; "a base-less
  file's only edit is one insert of its whole final length, and the run construction
  never reads that edit" (`captured/owner.rs` `construct_new` doc).
- Any **whole-file** result (< cutoff): the edit route assembles the whole object, so
  it reads every kept byte, at most the cutoff (`edits.md` "Cost").
- A whole-file base that grows to a chunked result: the whole result is rescanned
  (`stream_combined`, `apply.rs`).
- Scratch construction still gets storage help: CDC resynchronizes, so unchanged
  chunks get the same IDs and are reused by CAS; changed chunks are offered a delta
  base (rename predecessor or same-path base, via the Aligner; notes:C §4.3)
  [inference for the CAS part; measured overall effect F-063 −80 %, §6 below].

---

## 2. Small files (below the cutoff)

- **Representation** [constant] (`v2:core/crates/layerfs-content/src/contract/policy.rs`
  `representation`): length 0 → Empty (FileState over empty leaf); 1..cutoff−1 →
  one `WholeFile` object (`LFS5SML\0` v1); ≥ cutoff → chunked.
  `DEFAULT_SMALL_FILE_THRESHOLD_BYTES = 131_072`, minimum 131,072, maximum
  1,048,576; "the construction cutoff accepts any power of two from 128 KiB to 1 MiB"
  (policy.rs header). Persisted per Store in `store_policy` (notes:C §7.2).
- **Why 128 KiB.** No document or commit in the repository states a measured
  rationale (searched `docs/`, `core/docs/`, policy.rs history). Source facts that
  constrain it:
  - the whole-file zstd window log is 18 (256 KiB) up to a 256 KiB cutoff, else 20
    (`policy.rs` `whole_file_window_log`); a 256 KiB window holds a < 128 KiB base used
    as a PREFIX plus a < 128 KiB target [inference: this is why the window is 2 × cutoff];
  - chain budgets are 4 × and 2 × the cutoff (512 KiB canonical, 256 KiB encoded)
    (notes:C §4.4);
  - 128 KiB = 4 × the 32 KiB maximum chunk, 8 × the 16 KiB target; below it a file
    would be only a few chunks plus a 93 B FileState and at least one extent page
    [inference];
  - the overlay digest/relink path is only needed for chunked files, since a whole
    file is rebuilt to the same ID (`commit/README.md` "Relink").
- **Whole-file object cost** [constant]: canonical = raw + 23 B (13 B envelope +
  value header); the compact whole-file lane stores records "no length fields; raw =
  canonical − 23" (`v2:docs/architecture/core/physical-storage/README.md` lane table).
- **Lane and grouping**: whole-file records are grouped (#219 `a1faf957ed`: 9,444 →
  519 groups, +4 B per record) and the group is not compressed ("raw"); each
  record is already zstd-3 or STORED (physical-storage README lane table;
  notes:C §8.1).
- **Delta for small files**: the base comes from (notes:C §4.3)
  - in-place edit: `OriginalBase` (the edited base root);
  - rename-over (#352 F-063, extended to whole files by #88) and recreate-at-path
    (#88, ≤ 65,536 lookups/Commit): the replaced or same-path file's root is declared
    as explicit predecessor **without reading it** (`runs.rs` comment "A whole-file
    result is offered the base root as its explicit predecessor without any read
    (#88)");
  - otherwise the **similarity ring** (8,192 bottom-k signatures, 704 KiB, persisted).
- **Per-file overhead** [measured]:
  - Q1 (2,000 files 0–4 KiB, random): logical 4,103,985 B; canonical 4,356,403
    (+126 B/file); new_stored 4,132,331 (payload 4,115,113, metadata 17,218 =
    8.6 B/file); net 4,538,368, so **net − new_stored = 203 B/file** and total
    overhead ≈ 217 B/file on a 2,052 B mean file; raw ÷ stored **0.90×** SQLite,
    0.80× PostgreSQL (QUICK "Per-Commit attribution").
  - M01 (10,000 files 0–4 KiB): **2,261 B per file**, 182.5 B of it outside packs;
    raw ÷ stored **0.91×** SQLite, 0.83× PostgreSQL (MICRO "M01 tiny files").
  - What the outside-pack bytes are: one `object_location` row per object (32 B key
    + 5 integers + B-tree), a ~40.3 B inode value-block entry, binding hints, SQLite
    page slack; `file_digest` rows were dropped for small files by #91 (notes:C §7.5).
  - The content is incompressible (SHAKE-256), so compression cannot hide the
    overhead (MICRO "M01" workload line).

---

## 3. Small edits on big files

### 3.1 Worked example: 100-byte overwrite in a 500 MiB file (L500)

Workload: 500 MiB incompressible file, 5 Commits, each `open(r+b); seek; write(100 B)`
(`LFB:comparision/ovnfs/rerun-2026-11-08-cold/harness/largefile.py` `apply_to_file`).

1. **Write.** One FUSE WRITE of 100 B. Recognition is skipped (< 8 KiB). One owner
   job writes the inode's `fs_inode` row (size unchanged, `inherited_cutoff` = base
   length) and one `fs_data` cell row (two if the 100 B cross a 4 KiB boundary)
   carrying the 100 bytes and a validity mask; no base payload is read
   (§1.1).
2. **Capture.** Generation flip; no copy.
3. **Shapes pass.** Gap(Inherited) to the cell, Window with 100 local bytes,
   Gap(Inherited) to EOF: a handful of indexed steps for a 500 MiB file
   [inference, §1.2]. Script emits **one** `Edit::overwrite(off, off+100)`.
4. **Compare pass.** Reads the base bytes under the 100-byte range, i.e. the one
   stored chunk holding them (≤ 32 KiB, decoded whole) [inference on chunk
   granularity]; they differ.
5. **Extent tree.** Mean chunk ≈ 19.4 KB (ADD500 made 26,966 chunk trials for 500 MiB;
   the `edit_localized.rs` fixture has 432 extents in 8 MB, 1,293 in 24 MB), so the file
   has ≈ 27,000 extents, ≈ 211 full leaves of 128 entries, 2 level-1 branches and one
   root: path length 3 pages + FileState [inference from [constant] page bounds
   64–128 entries, `edits.md` "file/mapping"]. The split reads the 3 path pages; the
   extent holding `off` becomes left slice + right slice of the same stored chunk
   (offset/length change, payload ID kept).
6. **Re-chunk.** Only the 100 replacement bytes are scanned: one new 100-byte chunk
   (the EOF chunk of the replacement may be shorter than 8 KiB; `chunk_runs.rs`).
   Nothing else is re-chunked.
7. **Delta base.** The Aligner offers the old chunk overlapping the range
   (`align.rs` `offer`). One PREFIX trial; for random bytes it cannot be smaller, so
   the chunk is stored FULL/STORED [inference].
8. **Pages rebuilt.** Only the join path: the changed leaf and its ancestors plus a
   new 93 B FileState; each page offered its predecessor at the same level and logical
   start as a COPY/INSERT program base (#92). Then the inode table path (pooled inode
   leaf, InodeBranch program) and filesystem root. The parent directory is not
   rewritten: names bind serials (notes:B §3 "Reuse of unchanged subtrees").
9. **Bytes** [measured, OVN-COLD run1 receipt]: 5 Commits Σ new_canonical 128,346 B
   (25.7 KB/Commit), Σ new_stored **29,352 B = 5.9 KB/Commit**, of which metadata
   28,587 and payload **765 B (153 B per 100-byte overwrite)**. Store growth
   36,864 B over 5 Commits = **7.4 KB/Commit** (report: 7.4 KB; ovnfs 524.3 MB).
10. **Time** [measured, OVN-COLD]: cold Commit median 28.1 ms (25.8–38.9) vs ovnfs
   603.9 ms.

Same edit at other sizes [measured, OVN-COLD + run1 receipts]:

| Case | File | Commits | Σ new_stored | Σ metadata | Σ payload | Store growth/Commit (report) | Cold Commit (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| L10 | 10 MiB | 20 | 76,937 | 73,877 | 3,060 | 5.5 KB | 24.4 |
| L100 | 100 MiB | 10 | **43,402** | **41,872** | 1,530 | 5.3 KB | 24.5 |
| L500 | 500 MiB | 5 | 29,352 | 28,587 | 765 | 7.4 KB | 28.1 |
| I10 (insert 100 B, tail rewritten) | 10 MiB | 10 | 42,042 | 40,512 | 1,530 | 5.3 KB | 25.5 |
| A5 (append 16 KiB) | 5 MiB | 20 | 341,649 | 12,864 | 328,785 | 18.4 KB | 19.5 |

- Payload per 100-byte edit is **153 B at every file size**; the file size shows up
  only as about one more page level, as the unit test shows: one three-extent
  overwrite reads **3 pages on an 8 MB file and 4 on a 24 MB file, with the same 3
  payloads** (`v2:core/crates/layerfs-content/tests/edit_localized.rs` header table:
  mapping pages 5 vs 12, extents 432 vs 1,293, pages emitted 4 vs 5).
- Metadata per middle overwrite is 3.7–5.7 KB stored from ~17–26 KB canonical;
  per append it is ~0.6 KB. The reports do not attribute this difference by role
  [gap; inference: a middle edit rewrites a full 128-entry leaf and both slice entries,
  an append extends only the rightmost, partly filled leaf].
- **F-005 Aligner** (commit `48ad603b14`, 1 MiB file, range-4 fork): insert near the
  start 37,029 → 3,941 B (0/3 → 3/3 chunks stored as deltas); middle edit 25,649 →
  4,785 B [measured, commit message; notes:C §8.1].

### 3.2 Insert that shifts the rest (I10)

- The harness inserts by rewriting the tail in place: `seek(off); tail = read();
  seek(off); write(payload + tail)` (`largefile.py`). Every byte after `off` moves.
- Write side: the tail arrives as 128 KiB WRITEs; recognition finds whole committed
  chunks inside them (CDC cuts resynchronize after the insert) and records them as
  pieces with `source_offset = file_offset − 100`; ragged cell edges are stored as
  bytes (`recognize.rs`; `payload.md` "Pieces").
- Commit: pieces of the file's own base are retained spans; the residue becomes one
  edit; `edges.rs` authenticates ≤ 64 KiB at each edge and retains equal bytes.
- Result [measured]: **153 B of payload per Commit, identical to the overwrite**
  (I10 Σ payload 1,530 B over 10 Commits; L100 also 1,530 B), metadata 4.0 KB per
  Commit; ovnfs stores 10.5 MB per Commit (OVN-COLD).
- M06 shows the same for a full `O_TRUNC` rewrite of a 64 MiB random file: insert 1 B
  at start 1,001 B (payload 54), insert middle 4,771 B (183), overwrite 4 KiB 17,737 B
  (8,277), append 1 MiB 1,053,928 B (1,050,505), delete first byte 1,278 B (108)
  [measured, MICRO "M06"].

### 3.3 Append (A5, ADD500) and losing-base backoff

- A5 (5 MiB, +16 KiB per Commit, incompressible): ~17,020 B new_stored per Commit,
  ~16,440 payload (+0.3 % over 16,384) and ~583 B metadata; ovnfs 613 KB per Commit
  [measured, OVN-COLD + run1 receipt].
- ADD500 (append 500 MiB of random data): every appended chunk is offered the old
  last chunk (the Aligner's neighbour) and loses. Losing-base backoff
  (`82d22cdced`): after 8 consecutive losses against one base, chunks offered it are
  stored FULL without a trial, retried after gaps 1, 2, 4, … 2^16. Trials 26,966 → 22,
  Commit 7,877 → 5,493 ms, **stored bytes identical** (525,796,005) [measured, commit
  message, in-process median of 3; notes:C §8.3].
- `66b9a73c67`: a win saving < 1/8 counts as a loss: 100 MiB log append 1,540 → 977 ms
  for +1.04 % stored (explicit speed/storage trade) [measured].
- ADD500 stored 525,795,569 B (payload 524,853,792 for 524,288,000 raw, +0.108 %;
  metadata 941,777) vs ovnfs 1,048.6 MB, because ovnfs copies the old 500 MiB too
  [measured, OVN-COLD run1 receipt; notes:E §1c].

---

## 4. Deduplication at each level

| Level | Mechanism | Evidence |
| --- | --- | --- |
| Identical whole small files | same canonical bytes → same ObjectId; Save reuses a located object with same role and length, without read-back | `v2:core/crates/layerfs-storage/src/save/wave.rs`; notes:C §3 |
| Identical large files (copy) | append digest → `file_digest` → saved root reused, no construction | `commit/README.md` "Relink"; M02 below |
| Identical chunks | CDC partition is content-defined and frozen per Store (profile ID in every FileState); identical chunk = identical ID | notes:C §2 |
| Identical extent pages / zero runs | `push_repeated_chunk` emits complete identical pages once; 64 MiB of zeros → **76 B payload, 684 B metadata** | `edits.md` "Streaming builder"; MICRO "M07" |
| Unchanged subtrees | "A directory with no changes keeps its stored root"; untouched B+tree children keep IDs and are never read | `v2:docs/architecture/core/filesystem-trees/updates.md` (notes:B §3) |
| Unchanged extents of an edited file | split keeps stored chunk IDs as slices | `edits.md` "Plans" |
| Rewritten-but-equal bytes | compare pass returns the base root; `same_file_bytes` for diff/merge | `apply.rs`; notes:C §1.3 |
| Across Branches | Store-wide CAS; 50 identical Branches cost 5,407 B per Branch (H03) | HIST; notes:E §3c |
| Across projects | one Store for 20 projects: Σ H06 (20 Stores) ÷ H05 (1 Store) = **16.65×** SQLite, 16.83× PostgreSQL; later Inits into a shared Store cost 217–225 KB each vs 23.6 MB for the first | HIST; notes:E §3c |
| Copies of a tree | M02: 50 MB tree + 10 copies: exact dedup **11.90×**; the copy Commit (500 MB) stored **15,938 B, all metadata, 0 payload** | MICRO "M02" |

---

## 5. Compression

- **Payload records** [constant] (`v2:core/crates/layerfs-storage/src/encoding/codec.rs`,
  `full.rs`; notes:C §4.1): FULL = zstd level 3, chunk window log 20, whole-file
  window log 18 (≤ 256 KiB cutoff); STORED = raw when a payload > 1,024 B has a first
  1,024 B that compress by ≤ 1/32, or when the frame is not smaller than raw. PREFIX
  frames are never probed.
- **Ordinary lane groups** (tree and metadata pages, programs): zstd level 1, window
  log ≤ 16, kept only when smaller; chunk and whole-file lanes are raw because their
  records are already encoded (physical-storage README lane table).
- **Codec frozen (owner, 2026-11-06)** for the #84 storage round: no level, window,
  STORED threshold, dictionary, group or database compression change
  (`LFB:handoff-storage-optimization.md` box at top). The decision record
  (layerfs-project#93, now CLOSED, read with `gh issue view 93`) measured, offline on
  9,386 files < 128 KiB (61.7 MB): zstd-3 22.18 MB; zstd-6 −4.4 % at 2.6× compression
  CPU (109 vs 284 MB/s); zstd-19 −7.8 %; trained 112 KiB dictionary −14 % but needs a
  frame format change; 256 KiB group compression −22 % but must "decode 256 KiB to
  serve a 2 KB file"; random content (M01) gains nothing. The freeze kept per-object,
  random-access records and put the round's effort into base selection and metadata
  [inference about motive; the issue records the trade-offs, not a motive].
- **Measured M07** (MICRO "M07"): random 64 MiB → payload 67,181,294 B
  (1.0011 × raw: stored raw plus framing); zeros 64 MiB → 76 B; synthetic text
  64 MiB → 25,539,254 B vs whole-file `zstd -3` 20,412,198 B (**1.251×** worse,
  per-chunk frames lose cross-chunk context).
- NEW500 (random 500 MiB): payload 524,853,792 B for 524,288,000 raw (+0.108 %),
  metadata 942,288 B (0.18 %) [measured, OVN-COLD run1 receipt].

---

## 6. Delta encoding in the pipeline

- **One candidate, one trial, strictly smaller** (`select.rs`, notes:C §4.2):
  independent record first; then one base; PREFIX = `[1][base id 32 B][zstd-3 frame
  with the base payload as prefix]`; kept only if the record is strictly shorter.
- **Candidate source by case** (notes:C §4.3):

| Case | Base offered | Applies to |
| --- | --- | --- |
| In-place edit, small result | `OriginalBase` = edited root | whole file |
| In-place edit, chunked | Aligner: old chunk with most byte overlap, re-anchored at unchanged chunks | chunks |
| Rename-over (F-063, #88) | replaced file (from `fs_predecessor`) | chunks via Aligner; whole file as explicit predecessor |
| Recreate at path (#88) | base file at that path | both |
| New path, nothing declared | similarity ring (bottom-k sketch) | whole file only |
| Extent/directory/inode pages | page replaced or same level+position (#92, #360) | COPY/INSERT programs |

- **Chain limits** [constant]: depth 16 whole file, 8 chunk, 8 metadata; budgets 4T
  canonical / 2T encoded (512/256 KiB at T = 128 KiB). At a limit, **re-base** onto
  the chain's FULL root (#89) so the new object sits at depth 1 instead of storing
  FULL (notes:C §4.4, §8.2).
- **Measured** [MICRO; QUICK; notes:C §8]:
  - M03 in place / M04 rename-over / M05 new path (100 text files 16–64 KiB, 20
    Commits of 1 % line edits): delta + zstd **32.12× / 32.04× / 30.07×**; 20 edit
    Commits Σ 1,218,773 / 1,226,649 / 1,402,989 B.
  - Q2/Q3/Q4 (same shape, 25 files): Σ payload 436,551 / **436,551** / 449,377 B: a
    rename-over save now costs exactly what an in-place edit costs. Phase 0 → final:
    Q2 Σ new_stored −27.79 %, Q3 −45.61 %, Q4 −44.03 %; Q4 payload 808,429 → 449,377 B
    (#88); Q2 edit-Commit payload 290,078 → 119,925 B (#89).
  - F-063 rename-over chunk predecessor (`2de43d0a02`): git index churn 1,463,988 →
    297,095 B (**−80 %**), index chunks 120 FULL → 10 FULL + 110 PREFIX.
  - Cost of re-base: average final decode +22.5 %, max 520,575 B ≤ 524,288 budget (#89).

---

## 7. Pattern table

| Workload pattern | Mechanism that absorbs it | Measured result | Source |
| --- | --- | --- | --- |
| Copy of a tree | digest relink (chunked), same-ID rebuild (small), CAS | 10 copies of 50 MB: copy Commit 15,938 B, 0 payload; dedup 11.90× | MICRO M02 |
| Small text edit in place | whole-file `OriginalBase` PREFIX delta, re-base at limits | delta+zstd 32.12× (M03); 27.51× (Q2) | MICRO M03; QUICK |
| Rename-over save (small) | `fs_predecessor` → explicit whole-file predecessor (#88) | 32.04× (M04); Q3 payload = Q2 (436,551 B) | MICRO M04; QUICK |
| Rename-over save (large, e.g. git index) | full re-chunk + CAS for unchanged chunks + Aligner bases (F-063) | 1,463,988 → 297,095 B (−80 %) | commit `2de43d0a02` |
| New path per version | recreate-at-path (#88) or similarity ring | 30.07× (M05); Q4 payload −44.4 % | MICRO M05; layerfs-project#88 |
| Big file, small overwrite | cell row; one overwrite edit; split/slice; 1 new chunk; join-path pages | 100 B on 500 MiB: 153 B payload, 5.9 KB stored, 28.1 ms cold | OVN-COLD; run1 receipt |
| Big file, insert shifting tail | write recognition → pieces; edges trim; one small edit | 153 B payload/Commit, 4.0 KB metadata (I10) | OVN-COLD; run1 receipt |
| Big file, append | trailing insert edit; losing-base backoff | A5 17.0 KB/16 KiB append; ADD500 trials 26,966 → 22, bytes equal | OVN-COLD; `82d22cdced` |
| Big file, truncate | `fs_truncate` step; split + discard | DEL500 8,397 B, 0 payload | OVN-COLD run1 receipt |
| New random data | STORED after 1 KiB probe | +0.11 % payload framing (M07, NEW500) | MICRO M07; OVN-COLD |
| Zeros / sparse | holes cost no rows; zero runs; repeated pages | 64 MiB zeros → 76 B payload, 684 B metadata | MICRO M07 |
| SQLite DB churn | in-place page writes → overwrite edits; Aligner chunk deltas | 13.73× raw÷stored; 615,282 B per 1 % update Commit; Git pack 2.25× smaller | MICRO M08 |
| Tiny files | whole-file objects, compact lane, pooled metadata | 0.91× (M01), 0.90× (Q1): Store larger than raw | MICRO M01; QUICK Q1 |
| Many Branches | Store-wide CAS; Branch = pointer to root | 50 identical Branches: 5,407 B growth each (H03) | HIST |
| Many projects | one Store, shared CAS | 16.65× smaller than 20 Stores (H05 vs H06) | HIST |

---

## 8. Candid costs

1. **Tiny files cost more than their bytes.** ~200 B per file beyond the raw bytes
   (Q1 203 B outside packs; M01 2,261 B per 2 KB file); raw ÷ stored 0.90–0.91×
   SQLite, 0.80–0.83× PostgreSQL (QUICK; MICRO).
2. **Metadata dominates small Commits.** H01: 1,957 of 2,990 B new_stored per Commit
   (≈ 2/3) (HIST); S1–S4 "about two-thirds" (OVN-COLD); L100 41,872 of 43,402 B
   (96.5 %) (run1 receipt). Fitted per-operation costs: edit 414 B, new file 1,360 B,
   delete 1,111 B, rename 464 B (HIST; a least-squares diagnostic).
3. **New data is written twice.** Every acknowledged write goes into the overlay
   SQLite (32 KiB rows) and is read back at Commit, chunked, hashed, probed, packed and
   written to the Store (notes:D §3.4). NEW500 cold Commit 1,359 ms vs ovnfs 997 ms;
   write exec 2,669 vs 1,756 ms (OVN-COLD).
4. **Whole-file rewrites under a new inode are re-chunked whole.** Rename-over and
   new-path saves of a large file read and CDC-scan every byte at Commit; recognition
   only covers same-inode writes and gives up after 1 MiB unmatched. Storage stays
   small (CAS + Aligner), CPU does not. Every whole-file (< cutoff) edit reassembles
   the whole object (≤ 128 KiB).
5. **No GC or repack.** Published objects are never deleted; packs are immutable
   (SQLite triggers refuse UPDATE/DELETE); deltas chosen at Commit are permanent
   (notes:C §7.1, §9). Removing a 500 MiB file frees nothing (RM500 commit 286 B).
6. **Per-chunk compression loses context**: text 1.251× larger than whole-file
   zstd-3 (M07); Git packs are 10–40 % smaller on histories and 2.25× smaller on SQLite
   page churn (notes:E §4).
7. **Edits fragment the mapping and break root equality** [inference for fragmentation]:
   each overwrite leaves slices and a small new chunk, with no defragmentation; equal
   bytes may have different roots, so diff/merge compare bytes (`same_file_bytes`,
   notes:C §1.3).
8. **One sample per row**; storage bytes are exact but most rows are a single sample
   with 0–32 KiB page noise (notes:E caveats).

## 9. Gaps and inferences to resolve before publication

- The per-role split of the 3.7–5.7 KB metadata per large-file overwrite is not in any
  report; a traced run (`save_work` counters) would settle it.
- No measured rationale for the 128 KiB cutoff exists in the repository.
- Page counts for the 500 MiB file are derived (≈ 27,000 extents, 3 levels), not
  observed.
- The compare pass reading "one chunk" for a 100-byte range is inferred from chunk
  granularity, not from a counter.

---

## Appendix section (paper-ready, ~1,000 words)

### From `write(2)` to stored bytes

LayerFS turns filesystem calls into Store objects in two phases. At write time it
records *what changed*, cheaply and without reading committed data. At Commit time it
turns that record into the smallest set of new immutable objects it can find in
bounded work.

**Write time.** Kernel writeback is off, so every `write(2)` reaches the daemon and is
published into the Workspace's private overlay database before it returns. Bytes are
kept as 4 KiB cells (with a per-byte validity mask, so a written zero differs from a
hole) or as dense runs of up to 32 KiB. Holes cost no rows, a shrink is a single step
row, and a write never reads the committed bytes it partly covers. Two pieces of
provenance are kept beside the bytes. A *reference piece* says that a range equals
bytes of a committed file; write recognition creates one when a large write contains
whole chunks the file already had, as when a program rewrites a file in place. A
*predecessor* row records that `rename()` put one file over another, which is how
editors, Git and package managers save.

**Capture.** A Commit freezes the overlay generation without copying rows. For each
changed file, a first pass reads only row shapes: an unchanged region is one indexed
gap, so the pass grows with rows written, not file length. A one-pass script turns
the spans into an ordered edit list in result coordinates. Bytes inherited from the
file's own base, or pieces of it, are *retained*; everything else is new.
Overwrites, appends (one trailing insert), truncations (one trailing delete), holes
(zero runs) and shifted rewrites (pieces plus a small residue, with edges checked
against the base in 64 KiB windows) all come out in this one form.

**Construction.** Content applies the edits to the stored extent tree: it splits the
extents at each edit boundary, drops the replaced middle unread, chunks only the
replacement with FastCDC (8/16/32 KiB), and rebuilds pages only on the join paths.
Unchanged extents keep their chunk identities, possibly as slices. A file the base did
not hold (a new path, a renamed-over temporary file, a recreated path) is built from
scratch. Content-defined chunking resynchronizes, so its unchanged chunks are the same
objects as before. Files below a per-Store cutoff (128 KiB by default; any power of
two up to 1 MiB) are single whole-file objects.

**Storage.** Each object's identity is the BLAKE3 hash of its canonical bytes. The Save
reuses any object already stored, then gives each new payload one representation trial.
The independent record is zstd-3, or raw when a 1 KiB probe shows the data is
incompressible. At most one delta base is tried, and the delta is kept only if strictly
smaller. Bases come from filesystem events rather than search: the edited version, the
aligned old chunk, the renamed-over file, or the file previously at the same path. A
similarity sketch is the fallback for whole files. Chains are bounded by depth and by
read-work budgets, and a chain that reaches a bound is re-based onto its root. Records
are grouped into append-only 256 KiB packs inside the single Store database.

| Workload pattern | Absorbed by | Measured |
| --- | --- | --- |
| Copy of a tree | digest relink, CAS | copy Commit of 500 MB: 15.9 KB, no payload |
| Small text edit in place | whole-file PREFIX delta | delta+zstd 32.1× |
| Rename-over save, small file | predecessor recorded at `rename()` | 32.0×; same payload as in place |
| Rename-over save, large file | CAS + aligned chunk bases | git index −80 % |
| New path per version | same-path base, similarity ring | 30.1× |
| 100 B overwrite, 500 MiB file | cell row, one edit, one new chunk | 153 B payload, 5.9 KB per Commit |
| Insert shifting the tail | recognition → pieces | 153 B payload per Commit |
| Append | trailing insert, losing-base backoff | +0.3 % over appended bytes |
| Truncate 500 MiB | split and discard | 8.4 KB, metadata only |
| Random data | raw after probe | +0.11 % |
| 64 MiB of zeros | zero runs, repeated pages | 76 B payload |
| SQLite page churn | in-place edits, chunk deltas | 13.7× (Git 2.3× smaller) |
| Tiny files | whole-file objects, pooled metadata | 0.91× (Store larger than raw) |
| 20 projects | one Store, shared CAS | 16.7× smaller than 20 Stores |

**Example 1: a 100-byte overwrite in a 500 MiB file.** The write produces one cell row
in the overlay and reads nothing from the Store. At Commit, the shape pass finds one
local window between two inherited gaps and emits a single overwrite edit. The edit
reads the three mapping pages on the path to that offset (about 27,000 extents in a
three-level tree) and the one chunk under the range. It splits the chunk's extent into
two slices, scans the 100 bytes into one new chunk, and rebuilds only that leaf, its
ancestors and the 93-byte file state. Each rebuilt page is offered its predecessor as
a copy/insert program base, and the old overlapping chunk is offered as the chunk's
delta base (random data keeps it raw). The parent directory is untouched, because
directory entries bind inode serials rather than content. Measured over five Commits,
each one stored 153 B of payload and 5.7 KB of metadata. The Store grew 7.4 KB per
Commit, and a cold Commit took 28 ms. The same edit on 10 and 100 MiB files stores the
same 153 B; in a unit test, file size adds one page read per tree level (3 pages at
8 MB, 4 at 24 MB).

**Example 2: a rename-over save of a small file.** An editor writes `f.tmp` (a new
inode, say 40 KiB) and calls `rename(f.tmp, f)`. The overlay stores the new bytes as
dense runs, rebinds the name `f` to the new serial, and records the old serial as its
predecessor; no Store I/O happens. At Commit the new file has no base of its own, so it
is built from scratch as one whole-file object. The old file's root is declared as an
explicit delta base without being read. The Save tries one zstd frame with the old
version as prefix (a 256 KiB window covers both) and keeps it if smaller. The directory
page that maps `f` gets a copy/insert program against the page it replaces. Measured on
100 such files with 1 % line edits, 20 Commits stored about 613 B per file version
(delta+zstd 32.0×, equal to in-place editing at 32.1×). On the smaller Q-suite, the
rename-over and in-place payloads are byte-identical.

**Costs.** Tiny files cost about 200 B each beyond their bytes. Metadata is about
two-thirds of a typical small Commit. New data is written twice, once to the overlay
and again to the Store. A large file saved under a new inode is re-chunked in full,
though it stores little. Nothing is ever garbage-collected or repacked.

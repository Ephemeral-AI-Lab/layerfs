## 9. Five plain answers

**1. Resulting file/folder structure and production LOC change.**
C1 `layerfs-content`: 29 files, **2 336 → 4 463** (+2 127). C2 `layerfs-storage`: 38 Rust
files plus 1 runtime SQL file, **3 084 → 5 863** (+2 779, including 48 SQL LOC). Telemetry
unchanged at 732 across 7 files. **Core / replacement product 6 152 → 11 058 (+4 906) in
52 → 75 files**; reference unchanged at 65 417 in 193 files; combined 71 569 → 76 475.
C1 + C2 alone is 5 420 → 10 326, inside the plan's 8 720–14 230. New families:
`file/edit/` (8 files, 1 538), `encoding/delta/` (831), `encoding/pool/` (960),
`object/inode_leaf.rs` (307), `file/view.rs` (86), `sqlite/pool.rs` (118). Eight planned
files were never created — most importantly the `encoding/codec/` folder:
`encoding/codec.rs` was **retained and grown** from 367 to 508 rather than relocated, and
it holds the batch's only twelve `unsafe` sites. One unplanned file was added,
`file/edit/tree.rs` (701 production / 901 physical LOC), the stored-tree edit algorithm.
Only `file/edit/frontier.rs` and `pack/singleton.rs` are genuine responsibility merges;
`pack/read.rs` and `mapping/predecessor.rs` folded into existing files. No relocation
occurred. All hard caps hold: no file over 999 physical lines (largest 911), no
`lib.rs`/`mod.rs` over 200, and the boundary guard passes on 75 files.

**2. Are all applicable Stage 3 and Stage 4 criteria met?**
**No — both are INCOMPLETE.** Every implemented capability I checked is real and
externally tested: E1–E3 (pooling grammar, 131 072-entry window, pooled COPY/INSERT delta),
D1–D4 (sealed-reference stored-tree edits including the 80+100→90+90 repartition, a
4 096-edit bounded frontier, child-first finality), configurable 128 KiB–1 MiB cutoffs with
8/4/50 depths, payload FULL/PREFIX with one trial, pack lanes and explicit format dispatch,
transitions at every accepted cutoff, and the storage regressions. All 273 workspace tests
pass (exit 0), as do the handoff's two focused target sets and all five required example
modes; fmt, clippy `-D warnings`, the boundary guard, both tool suites and `git diff
--check` are clean. What is **not** met: #168 acceptance item 7 and #169 acceptance item 6
(footprint/memory/speed measured, "existing-or-better") have **no artifact at all**; #169's
read-amplification accounting is `NOT_RUN`; the 8 MiB frontier refusal and the timer-detail
completeness of the largest arm are unrun or clipped (57.46 % of that arm unattributed);
and no v0.1.6 comparison exists on any axis. G13 and G15 are recorded as PASS only by an
**owner waiver** whose own text says it "creates no evidence, it accepts its absence". I
would not have closed either issue on this evidence; the owner did, at `02:15:04Z` and
`02:15:07Z` on 2026-09-17, minutes before this round.

**3. What can be removed, merged or simplified without weakening the contract?**
Roughly **60–80 physical production lines**, plus several real work reductions, with no
loss of validation, exact CAS comparison, candidate quality, backpressure, atomicity,
visibility or cleanup. Highest value first: (i) `select_pooled` calls the batch index API
once per row — batching it removes ≈100 SQL point queries and ≈1.2 MiB of group clones per
100-row leaf; (ii) a fresh `PoolReader` per pooled-leaf resolution discards the wave's pack
and group caches, costing up to K-fold extra fetches and decodes; (iii) `delta::select`
re-hashes a canonical object whose `ObjectId` the caller already holds; (iv) the file state
is read and decoded a second time per chunked edit (inherited from v0.1.6 — do not credit
it as a new win) and `tree::read_state` can go entirely; (v) value-group bodies and pooled
leaves are each duplicated once; (vi) `plan_lane` encodes the same record twice on the
oversized path; and (vii) a set of one-to-four-line deletions (dead constants, a dead
assignment, an unreachable-false conjunct, a recomputed set, a double-copied batch read). I
explicitly reject removing the two `load_node(last,false)` probes (they are a partition
validation, not a duplicate read), `compare_replacements` (mandated planned work), eager
builder publication (it would emit unreachable objects) and the retained-tail pack rewrite
(locator stability depends on it).

**4. What statistics and source evidence establish speed, efficiency, bounded memory and
memory safety, and what remains unproven.**
**Established (measured):** correctness and footprint on real fixtures — 273 tests across
43 targets exit 0, the sealed-reference oracle agrees on nine cases including the 90+90
repartition, the frontier peak vector `[2208…3408]` against a no-release control
`[6308…129728]`, and the footprint row `raw=1048583 canonical=1052271 objects=60 packs=6
pack_bodies=1052818 largest_pack=259777 database=1204224` with no pack, payload or spool
file on disk. **Established (source-derived):** per-owner bounds, multiplicities,
lifetimes and release events; the fail-closed charge arithmetic; and the memory-safety
reading of all twelve `unsafe` sites, which are **sound as written** against the pinned
zstd 1.5.7 (static contexts refused rather than overrun; destinations allocated to the
declared raw length; Rust-side header validation before every decompress; no reachable
use-after-free on the prefix or reset paths). **Unproven:** *any* speed claim — the only
reference pair is n=1, in-memory and Store-free, with interleaved observations and two arms
that print different quantities; *any* storage-saving claim — the write boundaries are not
like-for-like (13 826 B against 47 357 B) and the two metrics are not comparable; *any*
peak-memory claim for this HEAD — the only instrument is an example whose sole receipt
predates the tip and which cannot see libzstd's or libsqlite3's heaps; p95 and variance;
cold-cache behaviour; the codec FFI under hostile input; 32-bit behaviour; and the host
SQLite's engine limits. Safe Rust (C1 and telemetry are `forbid(unsafe_code)`) and a low
RSS would **not** establish dependency safety for C2.

**5. What limits exist?**
File revisions: **no cap, and no revision or history owner at all** — retained roots are
unbounded in number, bounded only by a SQLite database size the product does not limit, and
no successful-version rollback exists. Delta depth: 8 whole-file / 4 chunk / 8 pooled by
default, each configurable 0…50 (0 disables that role's prospective delta) — these cap
*chain links*, not file revisions, and a longer history simply selects FULL. File size:
routing threshold 128 KiB by default (configurable only to the four powers of two up to
1 MiB), whole-file payload ≤ cutoff − 1, largest canonical object any role produces
1 048 598 B, format ceiling 16 EiB − 1 (the u64 length field), largest physically tested
file 2 097 151 B; **T is not a maximum file size**. Files per workspace: **not
established** — no Workspace owner exists; the verified underlying scale is 60 objects and
6 packs, and 512-object batches and 128-ID pages are batching, not a file cap. Directory
name, path, nesting and entry limits: **not established by Stages 3–4**; Stage 5/7 review
required. Workspace size: **the core cannot promise one** — no quota and no aggregate API;
per-pack limits are 262 144 B (16 781 312 B singleton) with no cap on pack count.
Store/database: ordinal space 4 294 967 295 pooled values per Store (the guard at that
boundary is currently defeated by a u32 truncation), 165 values per group, 512 objects and
4 MiB−1 per transaction, but **SQLite's own BLOB and database maxima are host-dependent and
unqualified** because the system library is linked. Pool/index: the 131 072-entry window is
an eviction bound, not a workspace cap. Edits: at most 4 096 per operation, frontier
charged against 8 MiB−1 (fail-closed, refusal unrun), one writer per Store with no retry,
and unbounded readers with engine-level contention semantics.

---


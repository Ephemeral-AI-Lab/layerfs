# G. Core algorithms (appendix pseudocode, derived from source)

Read-only notes, 2026-11-14. Nothing was built, run or edited.

- **Code identity:** `origin/layerfs_v2_0_0` @ `8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5`.
  Paths below are relative to the repo root at that ref (read with
  `git show origin/layerfs_v2_0_0:<path>`). `C:` = `core/crates/layerfs-content/src/`,
  `S:` = `core/crates/layerfs-storage/src/`, `E:` = `engine/crates/`.
- **Conventions.** Pseudocode follows the source's control flow and constants.
  `[S]` marks a simplification made in the pseudocode (error paths, counters,
  timing scopes, cache plumbing and overflow checks are omitted throughout and
  not marked individually). `[constant]` = value in source.
- Builds on `notes/C-core-storage.md` §2, §4, §6 and `notes/B-engine.md` §3, §8;
  corrections/precisions relative to those notes are flagged **[precision]**.

---

## A1. GEAR content-defined chunking with normalization and the zero-run shortcut

**Sources.** `C:file/cdc/gear.rs` (`Scanner::consume`, `scan_region`,
`process_pending_pair`, `emit`, `finish`, `profile_id`); `C:file/cdc/zeros.rs`
(`Scanner::zeros`); `C:file/mapping/repeated.rs` (`push_repeated_chunk_hinted`,
`repeat`, `uniform_node`); `C:file/construction/runs.rs`, `C:file/construction/chunk_runs.rs`;
`C:contract/policy.rs` (`MAX_MAPPING_ENTRIES = 128`).

**Constants [constant].** MIN = 8,192; TARGET = 16,384; MAX = 32,768; shift 2; seed 0.
`SMALL_MASK = 0xd903_0353_7000` (16 bits set), `LARGE_MASK = 0xd901_0353_0000`
(12 bits set); `SHIFTED_* = MASK << 1`. With one mask test per byte, the cut
probability per byte is 2^-16 in [8 KiB, 16 KiB) and 2^-12 in [16 KiB, 32 KiB)
(FastCDC normalization level 2 around a 2^14 target).

```text
state: chunk buf (cap MAX), hash ← SEED, pending ← ⊥        # persists across read() calls

CONSUME(bytes):
  if pending ≠ ⊥: PAIR(pending, bytes[0]); bytes ← bytes[1..]; pending ← ⊥
  while bytes ≠ ∅:
    if |chunk| < MIN:                                   # first 8 KiB copied, never hashed
      move min(MIN−|chunk|, |bytes|) bytes into chunk; if bytes = ∅: return
    for (lo, hi, smask, mask) in [(|chunk|, TARGET, SHIFTED_SMALL, SMALL),
                                  (max(|chunk|,TARGET), MAX, SHIFTED_LARGE, LARGE)]:
      for each whole pair (a, b) of bytes while position < hi:          # two bytes per step
        hash ← (hash << 2) + (GEAR[a] << 1);  if hash & smask = 0: CUT before a
        hash ←  hash + GEAR[b];               if hash & mask  = 0: CUT before b
    if CUT at e: append bytes[..e] to chunk; EMIT(); bytes ← bytes[e..]; continue
    append scanned pairs to chunk
    if |chunk| = MAX: EMIT(); continue                     # forced cut
    if one odd byte left: pending ← it                    # pair split by the read boundary
    return
PAIR(a, b): same two tests on (a, b), small masks iff |chunk| < TARGET; a cut emits
            the chunk ending before a (or before b), then the rest starts the next chunk
EMIT(): callback(chunk); chunk ← ∅; hash ← SEED
FINISH(): if pending: chunk += pending;  EMIT()             # EOF chunk may be < MIN

ZEROS(L):                                     # hole of L logical zero bytes
  head ← min(L, MAX); CONSUME(0^head)        # one MAX chunk flushes preceding non-zero state
  n ← (L − head) div MAX
  if n > 0: callback_repeated(0^MAX, n)      # state is periodic with period MAX
  CONSUME(0^((L − head) mod MAX))

PUSH_REPEATED(raw, n):                        # mapping side: one chunk object, n extents
  accept chunk object once; REPEAT(level 0, extent(raw), n)
REPEAT(level, entry, n):
  top up the level's unfinished page through the ordinary path until it is flushed
  pages ← ⌈(pending + n − flush_at) / 128⌉; tail ← (pending + n) − 128·pages
  keep `tail` copies pending; if pages > 0:
    emit ONE uniform page of 128 × entry   # stands for `pages` identical pages
    REPEAT(level+1, summary(page), pages)
```

`[S]` The two mask phases are written as a loop; the source runs a small-mask
`scan_region` over at most `(TARGET − |chunk|)/2` pairs, then a large-mask one up
to MAX. The cut index convention (the chunk ends *before* the byte whose test
fired) is as in `scan_region`.

**Complexity.** Time Θ(n): one table lookup, shift/add and mask test per hashed
byte; the first 8 KiB of each chunk is copied unhashed (so at most
n − 8 KiB·#chunks bytes are hashed). Memory O(MAX): one 32 KiB chunk buffer and
one 32 KiB read buffer; no whole-input retention. Read-buffer boundaries never
move a cut (the `pending` byte carries the half pair). Zero run of length L:
O(MAX) scanner work (head + tail) plus O(h · 128) mapping work, h =
⌈log₁₂₈(L / MAX)⌉ + O(1) — i.e. logarithmic in L, and exactly one chunk object
for the whole interior. The partition and root equal byte-by-byte scanning (the
source's stated invariant; "work depends on boundary pages and tree height, not
the repeat count").

**Standard vs LayerFS.** Standard: GEAR rolling hash, FastCDC two-mask
normalization, two-bytes-per-step rolling (FastCDC 2020). LayerFS-specific: frozen
profile digested into `profile_id()` and embedded in every FileState (a changed
profile fails reads); the zero-period argument (a zero stream's scanner state
repeats every 32 KiB including the odd pending byte and hash) that lets holes be
handed over as one repeat count; uniform-page reuse in the B+tree builder so a
hole of any length costs O(log L) objects.

---

## A2. Localized edit application on the extent tree, and the Aligner

**Sources.** `C:file/edit/apply.rs` (`apply_view`, `replace_chunked`,
`stream_combined`); `C:file/edit/tree.rs` (`split`, `concat`, `concat_inner`,
`coalesce`); `C:file/edit/compare.rs` (`compare_source`,
`COMPARE_WINDOW_BYTES = 64 KiB`); `C:file/mapping/align.rs` (`Aligner`,
`WINDOW = 64`); `C:file/mapping/extents.rs` (`Walk::next`);
`docs/architecture/core/files/edits.md`.

Edits are `(start, removed_len, replacement_len)` in current-result coordinates,
applied in ascending order. The extent tree is a B+tree of 64–128-entry pages
(leaf entry = chunk id, offset into the chunk, length: extents may be **slices**).

```text
APPLY_EDITS(base_view, edits, source):
  if edits = ∅: return base_view.root
  if COMPARE(base_view, edits, source) = Equal: return base_view.root   # 64 KiB windows, first diff stops
  case representation(final_len):                                       # by RESULT length vs cutoff
    WholeFile: assemble kept ranges + replacements into one pre-sized object
               hint explicit predecessor = base root (if different)    # "OriginalBase"
    Chunked, base chunked: return REPLACE_CHUNKED(base_state, edits)
    Chunked, base whole:   rescan base+replacements through the full builder (no hints)

REPLACE_CHUNKED(base, edits):
  T ← summary(base.mapping_root); A ← Aligner(base); shift ← 0
  for e in edits:
    (L, R0) ← SPLIT(T, e.start)                 # split at the edit start
    (X, R)  ← SPLIT(R0, e.removed_len)          # split at end of removed range
    discard X unread                            # removed middle never read or emitted
    M ← ⊥
    if e.replacement_len > 0:
      A.anchor(new = e.start, old = e.start − shift)
      M ← CDC+build(replacement bytes only,      # fresh Scanner per replacement
                    hint(off, len, id) = A.offer(e.start + off, len, id))
    shift += e.replacement_len − e.removed_len
    T ← CONCAT(CONCAT(L, M), R); release drafts not reachable from T
  emit unfinished pages reachable from T (children first), then FileState

SPLIT(node, k):   if k ∈ {0, size}: no read
  leaf: partition extents at k, cutting the straddling extent into two slices
  branch: find child c by cumulative end; recurse into c only;
          return (CONCAT(children before c, left(c)), CONCAT(right(c), children after c))
CONCAT(a, b):     equal levels: merge pages (leaves: coalesce adjacent slices of one chunk)
                  else descend only the taller side's boundary spine, rebuild that path
```

```text
ALIGNER (window of old extents (from, to, payload), ≤ 2·WINDOW = 128 held):
  anchor(new, old): shift ← new − old; boundary ← old
                    walk base forward to old (skipping whole subtrees before it)
                    neighbour ← old extent holding byte old−1; drop extents ending ≤ old
  offer(start, len, id):
    from ← max(0, start − shift); end ← from + min(len, MAX)   # a repeated run: first chunk only
    read ahead until walked ≥ end and window holds ≥ 64 extents
    if some old extent has payload = id:                        # unchanged chunk: re-anchor
      shift ← start − its.from; boundary ← its.to; neighbour ← id
      drop window up to it; return ⊥                            # exact reuse, no delta needed
    best ← argmax_old overlap([from,end), [old.from, old.to))   # ties: last in window order
    if overlap(best) > 0: return best.payload
    return first old extent with from ≥ boundary, else neighbour   # successor of last anchor
```

`[S]` Reference counting of draft pages (`release`, `settle`), the backed vs
memory draft store, and the page hints for mapping pages (#92) are omitted. The
tie-break "last in window order" follows Rust's `max_by_key` semantics (returns
the last maximal element); the source does not state a tie rule.

**Complexity.** Let h be the extent-tree height (O(log₆₄ E) for E extents), k
the number of edits and R the total replacement bytes. Each edit performs two
splits and two concatenations, each touching O(h) pages on one root-to-boundary
path (plus height-difference spines in CONCAT), so mapping pages read and pages
emitted are O(k·h); unread subtrees keep their IDs. Payload work is Θ(R) CDC
scanning; no removed or kept payload is read, except in the COMPARE pass (only
when a replacement has the same length as the range it replaces; a length change
is a difference by construction) and, by the Aligner, never (it reads mapping
pages only). The Aligner adds O(W) = O(128) per new chunk and O(pages spanned by
the window + h per anchor) page reads; memory O(W + one page). **Edit cost is
independent of file size**: measured in test `edit_localized.rs` (3 pages read on
an 8 MB file, 4 on 24 MB, same 3 payloads; see C-notes §6).

**Standard vs LayerFS.** Standard: rope/B+tree split-concat (persistent
sequence trees), CAS chunk reuse. LayerFS-specific: extents as slices of stored
chunks so edit boundaries never force re-chunking of neighbours (the price: the
root depends on edit history, hence A9); the Aligner as a delta-base oracle that
re-anchors on content-defined resynchronization (an inserted chunk shifts all
indices, so it matches by byte range, not index); the same Aligner is reused for
rename-over and recreate-at-path predecessors (A3).

**[precision]** CDC is *not* re-run across the edit boundary: the replacement is
chunked on its own (fresh scanner), and the two boundary chunks become slices.

---

## A3. Representation selection for one payload object

**Sources.** `S:encoding/delta/select.rs` (`select`, `trial_base`, `chain_root`,
`DepthCache::cost_of`, `LosingBases`, `eligible`, `acquire`); `S:encoding/full.rs`
(`encode_representation`); `S:encoding/codec.rs` (`payload_is_incompressible`,
`STORED_PROBE_BYTES = 1024`, saving 1/32, `PAYLOAD_LEVEL = 3`);
`S:encoding/delta/record.rs` (`prefix_cannot_win`, `PREFIX_FRAME_MIN = 14`);
`S:save/select.rs` (`offer`); `S:store/policy.rs` (`CHAIN_CANONICAL_CUTOFFS = 4`,
`CHAIN_ENCODED_CUTOFFS = 2`); `C:contract/policy.rs` (depth caps 16/8/8, max 50).
Candidate producers: `C:file/mapping/align.rs`; `C:file/construction/runs.rs`
(`construct_runs_with_predecessor`); `E:layerfs-workspace/src/construction/namespace/normalize.rs`
(`SamePath`, `SAME_PATH_HINTS = 65,536`, precedence at the values pass);
`E:layerfs-workspace/src/construction/captured/owner.rs` (`run_new`).

**Where candidates come from (content/engine side, before Storage).**

```text
per inode in Commit construction:
  changed base file           → edit route (A2): chunks get Aligner(base) hints;
                                whole-file result gets explicit predecessor = base root
  fresh file:
    pred ← rename-over predecessor recorded in the overlay        # F-063, first
           else same-path file the base tree held at its name     # #88, ≤ 65,536 lookups
    construct_runs_with_predecessor(pred):
       chunked result: each chunk ← Aligner(pred).offer(...)      # reads pred's mapping pages only
       whole-file result: explicit predecessor = pred root, no read
```

So a payload object reaches Storage with an *advisory list*: for a chunk at most
one Aligner-chosen base; for a whole file the declared predecessor(s).

```text
SELECT(id, canonical, role, advisory):               # role ∈ {Chunk, WholeFile}
  full ← INDEPENDENT(raw)                            # FULL (zstd-3) or STORED
  cap ← depth_cap(role)                              # WholeFile 16, Chunk 8
  if cap = 0: ring.insert(id) if whole file; return full
  if role = Chunk:
    c ← advisory[0]; if c ≠ ⊥ and not losing.admits(c): return full   # backoff skip
    cand ← c if ELIGIBLE(c) else ⊥                   # never the ring
  else:
    cand ← first ELIGIBLE(c) in advisory
    if cand = ⊥ and not prefix_cannot_win(|full|):   # base id 32 B + 14 B frame ≥ FULL?
      p ← ring.propose(id, signature(raw)); cand ← p if ELIGIBLE(p)   # A4
  if cand = ⊥: ring.insert(id) if whole file; return full
  cost ← chain_cost(cand)                            # depth, Σcanonical, Σencoded (cached walk)
  (base, cost) ← TRIAL_BASE(cand, cost, |canonical|, cap) or return full (+ring insert)
  if cost.canonical > 4T or cost.encoded > 2T: return full (+ring insert)
  base_bytes ← ACQUIRE(base)                         # A6, authenticated, charged to budgets
  if chain.canonical + |canonical| > 4T or chain.encoded + |canonical| > 2T: return full
  prefix ← zstd-3(raw, refPrefix = raw(base_bytes))  # exactly ONE trial
  if role = Chunk: losing.note(cand, won = (|full|−|prefix|)·8 ≥ |full|)
  if |prefix| < |full|:                              # strictly smaller; ties keep FULL
    depths.record(id, cost + 1); maybe admit to ring (A4); return prefix
  ring.insert(id) if whole file; return full

INDEPENDENT(raw):
  if |raw| > 1024 and |zstd3(raw[..1024])|·32 ≥ 1024·31: return STORED(raw)   # saves ≤ 1/32
  f ← zstd3(raw); return STORED(raw) if |f| ≥ |raw| else FULL(f)
ELIGIBLE(c): located(c) and role(c) = role and chain depth computable     # cap NOT checked here
TRIAL_BASE(c, cost, n, cap):                         # #89 re-base
  fits ← cost.canonical + n ≤ 4T and cost.encoded + n ≤ 2T
  if cost.depth = 0 or (fits and cost.depth < cap): return (c, cost)
  r ← chain_root(c)                                  # FULL record at bottom of c's chain
  return ⊥ if depth(r) ≥ cap else (r, cost(r))       # new object lands at depth 1
LOSING.note(b, won): won → forget b
                     else losses[b]++; if losses ≥ 8: skip[b] ← 2^min(losses−8, 16)
LOSING.admits(b):    if skip[b] > 0: skip[b]−−; return false; else true
```

T is the Store's whole-file cutoff (default 128 KiB → 4T = 512 KiB canonical,
2T = 256 KiB encoded). `[S]` The encoded-budget check uses `|canonical|` as the
new object's charge, as the source does (`select.rs`, "chained/encoded"). The
FULL record may be prepared early by `offer` (when a ring proposal is pending in
the same Save) and handed in; the decision is identical. `prefix_cannot_win`
only gates the ring lookup, not declared candidates.

**Complexity.** Per object: one 1 KiB probe plus at most one full zstd-3 pass
(Θ(n)); at most one chain-cost walk of ≤ cap+1 locator lookups (memoized in a
4,096-entry `DepthCache` dropped whole when full); re-base adds one more ≤ cap
walk; at most one base acquisition (≤ 4T decoded, ≤ 2T encoded bytes, A6) and
one prefix compression Θ(n + |base|). Whole files without a declared base add a
Θ(n) signature and an O(1) ring probe (A4). Memory: `LosingBases` ≤ 4,096
entries per Save (cleared whole when full).

**Standard vs LayerFS.** Standard: zstd with a reference prefix as a delta
codec (`ZSTD_CCtx_refPrefix`), delta chains with depth caps (as in Git packs),
"keep the delta only if smaller". LayerFS-specific: exactly one trial (no
search over candidates); candidates come from structural knowledge (Aligner,
rename-over, same-path) before content similarity; *re-base onto the chain root*
instead of falling back to FULL at the cap; work budgets in units of the cutoff
judged from stored sizes before reconstruction; per-Save losing-base backoff
with exponential retry; a probe that skips compression for incompressible data.

**[precision]** C-notes §4.2 lists the losing-base memo as "after 8 losses"; in
source the skip begins *at* the 8th consecutive loss with a gap of 1 offer,
doubling per further loss to 2^16. A win forgets the base. Note also that
`ELIGIBLE` accepts a candidate at the depth cap (it is re-based, not refused).

---

## A4. Similarity signatures and the candidate ring

**Source.** `S:encoding/delta/candidates.rs` (`signature`, `mix`, `fold`,
`Candidates::{find, propose, insert, admit, admitted_before_save, flush, read}`,
`SLOTS = 8,192`, `REFERENCES = 65,536`, `WINDOW = 16`, `ADMITTED_PER_SAVE = 1,024`,
`INDEX_BYTES = 704 KiB`); admission call sites in `S:encoding/delta/select.rs`.

```text
SIGNATURE(raw):                                    # bottom-8 over 16-byte windows
  if |raw| < 16: return [EMPTY]*8
  r ← poly-hash base 257 of raw[0..16]; sig ← [EMPTY]*8 (sorted)
  for each window start s:                         # Rabin–Karp roll
    r ← (r − raw[s−1]·257^15)·257 + raw[s+15]       (s > 0)
    h ← splitmix64_finalizer(r)
    if h < sig[7] and h ∉ sig: insert h into sorted sig, drop largest
  return sig
FOLD(sig) = [low 32 bits of each h]                # stored/compared form

ring: slots[8192] of (id, folded sig); refs[65536] of slot index (u16); next; stamp
INSERT(id, sig):  if sig[0] = EMPTY: return
  s ← next; clear refs that still point to s's old entry
  slots[s] ← (id, FOLD(sig)); for h in FOLD(sig): refs[h mod 65536] ← s   # newest wins the ref
  next ← (s+1) mod 8192; stamp++
FIND(target, sig):
  best ← ⊥
  for h in FOLD(sig): e ← slots[refs[h mod 65536]]
    skip if empty, e.id = target, or h ∉ e.sig      # ref collision check
    ov ← |FOLD(sig) ∩ e.sig|; age ← (next−1−slot) mod 8192
    if ov ≥ 2 and (ov > best.ov or (ov = best.ov and age < best.age)): best ← (ov, age, e.id)
  return best.id
PROPOSE(target, sig): f ← FIND; return admitted[f] if f was admitted in THIS Save else f
ADMISSION (from SELECT):
  FULL / STORED winner (no candidate, lost trial, budget refusal) → INSERT
  PREFIX winner whose base came from the ring AND that base was admitted before this Save
      → admitted[id] ← proposal; INSERT  (≤ 1,024 per Save)
  object stored against a declared (advisory) base → never inserted
```

Persistence: rows `(slot, stamp, id, 32-byte sig)` keyed by slot, upserted by
`flush` inside the publishing transaction; reload replays rows in stamp order so
reference-table collisions resolve as the live sequence did. Loaded lazily at the
first ring use in a Save.

**Complexity.** Signature Θ(n) with O(1) per window (8-element sorted insert).
FIND: ≤ 8 reference probes × 8×8 overlap compare = O(1). INSERT O(1). Memory
fixed at 704 KiB per Store handle regardless of object count.
`admitted_before_save` scans all 8,192 slots (O(SLOTS)), called only for a PREFIX
winner proposed by the ring.

**Standard vs LayerFS.** Standard: bottom-k min-hash resemblance sketch
(Broder), Rabin–Karp rolling hash, LSH-style reference table. LayerFS-specific:
fixed ring sized at the measured coverage knee (8,192 slots; 65,536 refs; 32-bit
folding with identical measured coverage — C-notes §4.3); only objects that found
no other base enter (self-limiting population); newest-entry tie-break so the
latest version of a lineage is proposed; the per-Save admission rule that never
proposes an object still pending in the same Save (avoids early pack seals).
A hash collision yields a false proposal only; A3's strictly-smaller rule makes
it harmless.

**[precision]** FIND requires an overlap of at least **2** of the 8 hashes.

---

## A5. Exact reuse in a Save wave and publication arbitration

**Sources.** `S:save/wave.rs` (`wave`, `admit`, `demand`); `S:save/select.rs`
(`offer`, `MissingDependency`); `S:save/publication.rs` (`acknowledge`);
`E:layerfs-persistence/src/backend/sqlite/objects/publish.rs` (`INSERT … ON
CONFLICT(object_id) DO NOTHING RETURNING object_id`); `S:store/policy.rs`
(`READ_OBJECT_LIMIT = 4,096`, `BATCH_OBJECT_LIMIT = 512`, `RECORD_COUNT_LIMIT = 8,191`).

```text
WAVE(objects):                                     # ≤ 512 objects or ~publication_bytes
  seen ← {}
  for slice in DEMAND_SLICES(objects):             # ids+refs+predecessors ≤ 4,096 per slice
    locate(slice ids)                              # one batched locator query
    for o in slice: ADMIT(o)
  flush ring signatures; register ready packs

ADMIT(o):
  if o.id ∈ seen:                                  # duplicate inside this wave
    require canonical(seen[o.id]) = canonical(o) and role equal, else Collision
    reused++; return
  seen[o.id] ← o
  if o.id pending in an unfinished pack of this Save: seal that pack now
  if loc ← location(o.id):                         # this Save's packer first, then Store
    require loc.role = o.role and loc.canonical_length = |o|, else Collision
    reused++; return                               # NOT read back (trust the content address)
  for r in o.references: require r pending, sealed here, or stored, else MissingDependency
  record ← SELECT(o) (A3); append to pack

PUBLISH(batch):                                    # reference-closed, topologically ordered
  returned ← INSERT object_location rows ON CONFLICT DO NOTHING RETURNING id
  lost ← batch ids − returned                      # another writer published them first
  for id in lost:
    expected ← resolve(id) from this Save's own pack
    actual   ← fresh authenticated read of the winner
    require actual = expected and winner.role = our role, else Collision
  count lost as reused, others as inserted
```

**Complexity.** Per wave of m objects: ⌈m·refs/4,096⌉ batched locator queries;
O(log m) per in-wave lookup (`BTreeMap`); exact reuse costs one locator row and
no payload read. Collision checks on `lost` read 2 objects per lost id, which
occurs only under concurrent writers. Publications split before 8,191 rows.

**Standard vs LayerFS.** Standard: content-addressed dedup, first-writer-wins
insert-if-absent. LayerFS-specific: reuse decided by locator (role + length)
without read-back (owner decision 2026-10-19, commit `abc3eb7721`; corruption is
caught at the first authenticated read); full byte comparison retained exactly
where two *new* copies meet (in-wave duplicate, lost publication race).

---

## A6. Delta-chain read

**Source.** `S:encoding/delta/read.rs` (`Resolver::resolve`, `resolve_dependency`,
`resolve_charged`, `base_of`, `decode_at`, `locator_key`).

```text
RESOLVE(id, charge_self):                          # charge_self: true when acquiring a base
  root ← location(id) visible under ceiling, else ObjectMissing / VisibilityCeiling
  [pooled inode leaves and program pages take their own readers — omitted]   [S]
  chain ← []; cur ← root
  loop:                                            # walk base-ward, iterative
    if cur.id ∈ chain: fail "dependency cycle"
    chain.push(cur)
    b ← base id named in cur's record (⊥ for FULL/STORED); if b = ⊥: break
    if |chain| > depth_cap(role): fail "dependency chain depth"
    cur' ← location(b); require cur'.role = cur.role ("dependency role")
    if store orders dependencies: require key(cur') < key(cur)   ("chronology")
    cur ← cur'
  canon ← ⊥
  for loc in reverse(chain):                       # decode root (FULL) first
    if loc ≠ requested or charge_self:
      canonical_work += loc.canonical_length; require ≤ 4T
      encoded_work   += stored width of loc's record; require ≤ 2T
    bytes ← decode(loc, base = raw_payload(canon))  # zstd with prefix = base raw payload
    require BLAKE3-id(bytes) = loc.id               # every element authenticated
    canon ← bytes
  return canon
```

**Complexity.** O(d) locator lookups and pack-body fetches for chain depth d ≤
cap; decode work bounded by the budgets (Σ canonical ≤ 4T, Σ encoded ≤ 2T, the
requested object itself uncharged on a plain read and bounded by the object
limit). Memory O(d) locators plus two decoded payloads at a time; pack bodies
go through a bounded cache shared by the walk and the decode pass.

**Standard vs LayerFS.** Standard: base-first delta reconstruction (Git pack
style). LayerFS-specific: every intermediate element is hash-authenticated
against its own locator (not only the final object), so a corrupt base is
caught at the element that is wrong; work budgets are enforced on read with the
same arithmetic the writer used (A3), so the writer never creates a chain the
reader would refuse.

---

## A7. Commit

**Sources.** `E:layerfs-store-service/src/store/commit.rs` (`admit_commit`,
`commit_admitted`); `.../store/commit_merge.rs` (`merge_candidate`,
`merge_in_lane`, `LANE_MERGE_PATHS = 1024`); `.../store/pipeline.rs`
(`QUEUE_SLOTS = 32`, `BATCH_BYTES = 256 KiB`, `BATCH_OBJECTS = 512`);
`docs/architecture/engine/commit/{README,construction}.md`.

```text
COMMIT(ws, force):
  CAS ws.committing false→true, else InFlight (Busy)
  snap ← bound Branch snapshot; st ← overlay state
  if st.dirty_inodes = 0 ∧ st.dirty_dirents = 0 ∧ no captured/consolidating gen
     ∧ st.base_root = bound root ∧ no pending conflict
     ∧ head(branch) and base layer unchanged since bind:
       return UpToDate                              # no capture, Save, lane or install
  G ← capture()                                     # active gen G sealed, G+1 active; no row copy
  save ← begin_save()
  spawn producer thread:                            # one construction producer
     construct(captured G over immutable base) → batches (≤512 obj / 256 KiB)
     → bounded channel (32 messages) → caller thread runs Save.accept (waves, A5/A3)
  candidate ← producer result
  refuse if a file listed in a pending merge still holds a complete marker block
  cur ← head snapshot
  root ← (cur moved ∧ ¬force) ? MERGE(base = bound root, ours = candidate, theirs = cur) : candidate
  save.finish(); count logical size; record name-binding hints; validate + prepare install
  ─── publication lane (admission turn + cross-process writer lock) ───
  if ¬force ∧ ¬conflicted:
    now ← head snapshot
    if now ≠ cur:                                    # head moved again while we worked
      root ← MERGE_bounded(base = cur.root, ours = root, theirs = now, ≤ 1,024 serials)
             in its own small Save; over the bound → LaneMerge, nothing published
  if conflicted: install merged tree WITH markers as local base; binding → head; publish nothing
  else: commit_branch(expected head/base/root, candidate)   # one write txn; may answer UpToDate
  ─── end lane ───
  install prepared base: wait base_readers = 0; base_root ← root; installed ← G; retire G
```

`[S]` The producer/Save pipeline also carries same-Save *read* requests on the
same queue (the producer flushes its batch, then waits). Failure custody, timing
and statistics are omitted. The lane is taken even for a conflicted Commit; it is
released at the end without a publish.

**Complexity.** Capture O(1) (generation flip). Construction and Save work is
proportional to the captured changes, not the namespace (indexed overlay records;
A2 for edited files, A1 for fresh files, unchanged subtrees keep IDs). In flight:
≤ 32 messages (~8–32 MiB). Lane hold: two head reads, an optional bounded merge
of ≤ 1,024 changed serials with byte comparison capped at 1 MiB per file (A8/A9),
validation only if the target changed, and one write transaction — independent of
tree size, history length and Branch count in the uncontended case.

**Standard vs LayerFS.** Standard: optimistic concurrency with compare-and-swap
on a branch head (Git's `update-ref` with expected old value), merge-on-push.
LayerFS-specific: generation-flip capture of whatever any process published (no
process/Exec identity); two-stage merge so the expensive merge runs outside the
single-writer lane and only the delta that landed meanwhile is merged inside it,
with a hard bound; conflicts become a local marker base rather than a failed
publish; one producer thread by policy (`LAYERFS_CONSTRUCTION_WORKERS=1`).

---

## A8. Three-way merge

**Sources.** `C:merge/entry.rs` (trivial cases, `compared` bound); `C:merge/serial.rs`
(`Ctx::run`, `changes`, `window`, `decide_change`, `WINDOW = 256`, `count`);
`C:merge/decide.rs` (`Decider::decide`, `same`, `text`, `time`); `C:merge/text.rs`
(`merge3`, `deletion_markers`, `has_markers`); `C:compare/text.rs`
(`MAXIMUM_TEXT_BYTES = 1 MiB`, `MAXIMUM_DIFF_CELLS = 2^20`, `diff_lines`,
`as_text`); `C:compare/tree.rs` (`inode_changes`);
`docs/architecture/core/compare-and-merge/merge.md`.

```text
MERGE(base, ours, theirs, from, into, bound):
  if ours = base or ours = theirs: return theirs; if theirs = base: return ours
  rules ← load /.lww from the rules side
  compared ← bound ? MAXIMUM_TEXT_BYTES : ∞
  for window of 256 serials s from inode_changes(base, from):   # page-skipping diff, serial order
    i_vals ← batch-read into(s) for the window
    for s: (b, f, i) ← values
      if i ≡ f: continue                       # both made the same change
      count++; if bound ∧ count > bound: fail (LaneMerge)
      if f is a directory: merge its changed names ("name points")
      elif f, i present ∧ i ≢ b: DECIDE(path(s), b, f, i)
      elif b, f present ∧ i absent: defer        # into deleted it; resolved by path later
      elif b, i present ∧ f absent ∧ i ≢ b: deleted_and_changed
      # i ≡ b: only `from` changed → take f in the emit pass
  second pass writes directory rows with decisions applied; build result on `into`

DECIDE(path, b, o, t):                          # o = ours, t = theirs (sides reordered)
  if SAME(o,t) or SAME(b,o): return theirs
  if SAME(b,t): return ours
  if .lww rule matches path: return newer mtime side (ours on tie; deleted → parent mtime)
  if o, t regular files:
    if base/ours/theirs all text ≤ 1 MiB (no NUL, UTF-8): return merge3(...)  # markers per hunk
    else: conflict binary; return ours
  if exactly one side present and non-directory: conflict modify/delete;
     return kept side wrapped in deletion markers if text, else kept side
  conflict type; return ours if present else theirs
SAME(x,y): both absent; or both directories; or same kind ∧ same_content(x,y, ≤ compared)  (A9)
MERGE3(b,o,t): match_lines(b,o), match_lines(b,t)  (LCS table if ≤ 2^20 cells,
  else prefix/suffix trim + unique-line anchoring); base lines matched on both
  sides split the text into chunks; per chunk: o=b → t; t=b or o=t → o;
  else marker block "<<<<<<< ours / ======= / >>>>>>> theirs" + hunk
```

`[S]` Directory name-point merging, structural corrections for moves (`places.rs`),
deferred deletions, revivals and field-wise metadata merge are summarized in one
line each. Conflict kind for text hunks is `content` with a base, `add/add` without.

**Complexity.** Driven by |Δ(base, from)|: the inode-table diff reads only pages
whose child IDs differ (O(Δ·h) pages); `into` values are read in batches of 256;
paths are named only where a rule needs one (bounded per-side path cache 2^16).
Text merge per file O(L_b·L_o + L_b·L_t) bounded by 2^20 diff cells per diff,
files ≤ 1 MiB. Byte comparisons A9. In the lane, decisions ≤ 1,024 and per-file
byte work ≤ 1 MiB, so the hold is independent of tree and file size.

**Standard vs LayerFS.** Standard: diff3-style line merge with Git-compatible
marker blocks; three-way decision table. LayerFS-specific: the walk is keyed by
inode *serial*, not path, so a move on one side and an edit on the other compose
(the edit lands at the new path); "same" means same bytes, not same root
(F-073); `.lww` gitignore-style last-writer-wins rules by mtime; an explicit
in-lane bound on both decisions and bytes compared.

---

## A9. `same_file_bytes`: byte equality that skips shared extents

**Source.** `C:compare/bytes.rs` (`same_file_bytes`, `same_file_bytes_within`,
`same_content`, `WINDOW = 256 KiB`); `C:file/mapping/extents.rs` (`Extent`:
logical offset, length, chunk id, chunk offset).

```text
SAME_FILE_BYTES(a, b, limit):
  if root(a) = root(b): return true
  open both views; if len(a) ≠ len(b) or len > limit: return false
  x, y ← extent cursors (⊥ for a whole-file side); p ← next(x); q ← next(y)
  from ← 0; at ← 0                            # [from, at) read-pending, not yet compared
  while at < len:
    end ← min(boundary(p, at), boundary(q, at), len)   # next extent start or end on either side
    if p = q (same chunk, chunk offset, length) and p.offset = at:   # shared extent
      if from < at and not BYTES_EQUAL(from, at): return false
      from ← end                                       # skip shared bytes unread
    elif end − from ≥ 256 KiB:
      if not BYTES_EQUAL(from, end): return false; from ← end
    at ← end; advance p / q past `at`
  return from ≥ len or BYTES_EQUAL(from, len)
BYTES_EQUAL(r): read both sides in 256 KiB windows, compare, stop at first difference
SAME_CONTENT(u, v, limit): kinds equal ∧ (roots equal ∨ (regular file ∧ SAME_FILE_BYTES(.., limit)))
```

**Complexity.** Equal roots O(1). Unequal lengths or over-limit: two root reads.
Otherwise O(E_a + E_b) extent-cursor steps (mapping pages) plus reads of only the
non-shared byte ranges, compared in 256 KiB windows with early exit; memory
O(256 KiB) per side. Shared extents (same chunk at same offset/length/position)
cost no payload read. With `limit = MAXIMUM_TEXT_BYTES` (1 MiB, the lane's bound)
the worst case is ≤ 1 MiB per side.

**Standard vs LayerFS.** Standard: chunked-file comparison by chunk-hash
equality. LayerFS-specific: needed *because* localized edits (A2) make the root
depend on edit history — equal bytes can have different roots (F-073: roots
`ee055170` vs `40ba3671`); slices make extent-level skipping exact only when
chunk, offset and position all match, so differing partitions fall back to byte
comparison of just those ranges.

---

## Corrections and cross-checks against existing notes

1. **CDC across edit boundaries (A2):** replacements are chunked with a fresh
   scanner; neighbours become slices. C-notes §6 is consistent; worth stating
   explicitly in the paper.
2. **Losing-base backoff (A3):** skipping starts at the 8th consecutive loss
   (skip 1 offer), then 2, 4, … up to 2^16; a win (saving ≥ 1/8) forgets the base.
3. **Ring proposal threshold (A4):** at least 2 of 8 folded hashes must overlap.
4. **Depth caps (A3/A6):** writer accepts a candidate at the cap and re-bases
   onto its chain root; reader refuses more than `cap` edges. Metadata cap is 8
   (program pages and pooled leaves, separate readers, not A3).
5. **Lane for conflicts (A7):** the lane is acquired even when the pre-lane merge
   conflicted; nothing is published and no head re-read occurs.
6. **Whole-file declared predecessor that is a chunked FileState** is
   role-ineligible (role must equal WholeFile), so such a file falls through to
   the similarity ring (A3 `ELIGIBLE`).

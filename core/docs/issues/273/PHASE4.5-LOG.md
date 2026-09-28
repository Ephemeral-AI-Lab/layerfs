# #273 phase 4.5 implementation log

> Append-only record of implementing the [phase 4.5
> proposal](PHASE4.5-IMPLEMENTATION-SPEC.md). Step status here is a source and
> functional statement, never a speed result or a release admission.
> Checkpoint 5 stays **NOT_RUN**.

## Step 4.5.1 — private index v2 format and selected resolver

Source commit `91c9c4938` ("feat(#273): select the phase-4.5 private index v2
resolver"), first parent `941613f65` (the phase-4.5.0 proposal). New
attachments select v2; no v1 index magic, name or page is produced, adopted or
migrated, and no checkpoint-4 receipt is relabelled.

Implemented grammar and behavior (spec §4.1, §4.2, §6.1):

- `page.rs`: index magic `LFSAIDX2` at version 2 for leaves and branches,
  directory magic `LFSAHOT2`, unchanged `LFSAPAK1` version-1 pack records.
  `pages.rs` names index pages `a-index-v2-<id>-<epoch>` and directory pages
  `a-hot-v2-<id>-<epoch>`, and `create_verified` returns the authenticated page
  so a selector decodes exactly the bytes a reader would see.
- `keyed.rs`: branch cell `fence_len:u16, fence, target`; 17-byte target
  `tag:u8, first:u64, epoch:u64` (0 cold page/epoch, 1 hot slot/reuse epoch,
  any other tag refused); only the final child may carry the inherited bound as
  a zero-length fence, and non-final fences must be nonempty and strictly
  increasing. Largest leaf cell 788 bytes, largest branch cell 291 bytes.
- `hot_directory.rs`: fixed `8 + 64*32 = 2,056`-byte body, records 64,
  32-byte entries (reuse epoch, physical page, level, kind, reserved), zero
  vacant/reserved bytes, level 0 exactly for leaves and 1..7 exactly for
  branches, occupancy checked before any entry is exposed.
- `resolve.rs`: every selected get/floor/scan descends by fence partition. A
  hot target resolves only through that view's directory with the requested
  reuse epoch, kind and level; an absent slot, mismatched epoch or superseded
  kind refuses instead of falling back to another page. A per-descent visited
  mask rejects a repeated hot slot, and the eight-level bound holds during
  descent.
- `splice.rs`: generic v2 mutation prunes vanished children, splits at the
  encoded midpoint so both halves of an overflowing node keep at least
  `BODY_BYTES/2 - max_cell` bytes (1,196 leaf / 1,693 branch), keeps a replaced
  hot node's slot and reuse epoch, and stages replacements hot whenever the
  predecessor was hot, so no cold edge can conceal a current hot target.
- `index.rs`: one selected root plus one optional charged directory copy are
  installed in a single publication; a replaced directory copy joins the
  revision-pinned retirement list.

Not implemented at this step, and not claimed: no route admits a hot cursor,
so every publication still selects an empty directory. The directory grammar
and hot resolution path are implemented and format-covered only. Eligible
tiny-WRITE admission, balanced-carry proofs, selective normalization,
selecting-pin retirement cohorts and the incremental G1/G2 reconcile precharge
are steps 4.5.2 and 4.5.3.

### Verification at `91c9c4938`

| Check | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace` (macOS host) | PASS |
| `cargo +1.85.1 zigbuild --release --target aarch64-unknown-linux-musl -p layerfs-workspace --tests`, then Docker `alpine:3.22` on an owned ext4 volume | `active_backing` 25/25 PASS; `readable` 18 PASS / 2 mounted-only ignored; `keyed_tree` 1; `attachment` 3; `backing_ownership` 6; `pieces_sequence` 36 PASS |
| `cargo +1.85.1 clippy --release --workspace --all-targets -- -D warnings` | PASS |
| `cargo +1.85.1 fmt --all -- --check` | PASS |
| `python3 core/tools/check_product_boundary.py` | PASS, 345 production Rust/SQL files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 9 PASS |
| 14 registered `stage_route.py` public cases at this source | PASS; receipts in [evidence/phase4.5/](evidence/phase4.5/) |

The route cases are functional selections with `performance_claim=false` and
`cache_claim=null`. Each reuses an independent byte copy of the closed
64 MiB fixture, runs one owned Linux `ext4` volume in `rust:1.85.1-bookworm`
with `--cpus=2` and `LAYERFS_CONSTRUCTION_WORKERS=1`, and records complete
command wall. No row is a latency PASS or a control comparison.

| Case | Checks | Complete wall | Selected observation |
| --- | ---: | ---: | --- |
| `active_repeated` | 3/3 PASS | 6.28 s | 4,097 same-offset writes; 2 pack fetches, 17 index fetches, one index write |
| `active_separated4096` | 3/3 PASS | 16.90 s | 4,096 separated one-byte writes; 1,843,200 B before, 20,480 B after Commit |
| `active_generation` | 2/2 PASS | 0.85 s | G1 Stage beside G2 live bytes; both Commits |
| `active_many_file` | 3/3 PASS | 4.22 s | 128 files; two shared pack pages before, zero after Commit |
| `active_mounted` | 2/2 PASS | 0.84 s | mounted FUSE write/append, unmount, Commit, canonical bytes |
| `active_split_slot` | 2/2 PASS | 0.84 s | packed-slot split final byte oracle and clean close |
| `active_retained32` | 3/3 PASS | 1.46 s | 32 pinned generations; 274,432 B retained, 12,288 B released |
| `active_mixed_compact` | 3/3 PASS | 3.78 s | G1/G2 mixed sealed pool; 4 pack fetches, 1,099 index fetches |
| `active_mutation_compact` | 2/2 PASS | 3.88 s | ordinary WRITE removes a mixed sealed source page |
| `active_quota_refusal` | 2/2 PASS | 0.93 s | refusal keeps acknowledged bytes and exact charge |
| `active_quick_controls` | 3/3 PASS | 16.16 s | clean Commit skips the retained old journal |
| `active_namespace` | 2/2 PASS | 0.84 s | fresh directory/link/symlink and successor rename/unlink Commit |
| `active_close` | 2/2 PASS | 0.84 s | fresh-name Commit and exact clean close |
| `active_payload_refund` | 3/3 PASS | 0.81 s | large payload owned before Commit, refunded after |

Nonpassing and unrun: no route case failed, but the 14 cases are the whole
registered set this step exercised. Cases that need the unwritten 4.5.2/4.5.3
sources (hot-cursor admission, carry counts, selective normalization, cohort
retirement and the post-Commit hot continuation), every benchmark selection,
the mounted-process-spanning-Commit continuity proof and checkpoint 5 are
NOT_RUN. No performance sample, RSS/cgroup bound, `st_blocks` equality claim
for the hot path or release admission exists at this step.

### Production LOC

`91c9c4938`: core 64,548 → 65,136 (delta +588); reference 65,417 → 65,417
(delta 0); combined 129,965 → 130,553 (delta +588). First parent `941613f65`
compared with the committed tree, `python3 tools/production_loc.py --json`,
unchanged scope/exclusions from the checkpoint-3-5 ledger.

This evidence commit is documentation and receipt storage only: production LOC
delta 0.

## Step 4.5.2 — source-derived implementation constraints (not implemented)

Recorded while implementing 4.5.1, from the same stable source. These are
read-only source findings for the next session; they change no product byte and
make no implementation claim.

1. **One index revision per ordinary WRITE is enforced by the caller.**
   `filesystem/active_file.rs::publish_active_file_mutation` rejects any
   publication whose revision is not the Workspace's next revision, so hot
   admission, selective normalization and the mutation must be merged into one
   candidate publication, exactly as spec §8.1 requires. A separate admission
   publication, or admitting only after the mutation returns, would fail that
   check; admission that cannot fit must fall back to the generic route in the
   same revision rather than publish twice.
2. **A retained hot binding needs a local validity rule, and the obvious
   predecessor check is not sufficient.** After a leaf split at fence `K0`, the
   left node keeps slot `s` (spec §4.3: replacing a logical node retains its
   HotRef epoch) and the right node takes a new slot. A binding for key `K`
   whose recorded predecessor stayed left passes a predecessor-only check even
   when `K >= K0`, which would insert `K` into the wrong node and break parent
   routing. The safe local rules are: (a) an *update* binding is valid only
   while its key is still present in the retained leaf; (b) an *insertion*
   binding whose recorded successor cell is still present is valid when the
   recorded immediate predecessor is still the cell immediately before `K`'s
   insertion point; (c) an insertion binding with **no** successor cell in the
   leaf (the append-at-right-edge case, and the split case above) is valid only
   while the retained slot's content version is unchanged. A charged per-slot
   content version in the incarnation state, bumped on every slot replacement
   and updated by the cursor's own publication, makes (c) cheap; without it a
   split can move the advancing frontier to a right sibling while leaving the
   predecessor behind.
3. **Heating the whole root-to-leaf path costs up to `h` slots per key.** Spec
   §4.3 requires every ancestor of a current hot node to be hot, so admission
   rewrites each path node (the leaf itself can be selected in place because
   its page is unchanged, but each ancestor's child cell must change from cold
   to hot, which rewrites that ancestor). At the eight-level ceiling, one
   inode's I/E/P/R paths can consume roughly 32 slots before shared-path
   savings, so a 64-slot directory supports about two maximum-height files.
   Admission must therefore be best-effort and compute its full affected path
   union before allocating; refusal falls back to the generic route and must
   never enlarge a declared bound.
4. **Slot reuse requires normalizing the parent edge first.** A directory slot
   cannot be repointed to a different logical node while any selected parent
   cell still names its old `(slot, epoch)`: resolution would refuse and the
   tree would become unreadable. Eviction therefore cannot simply reassign a
   slot; it must first make the parent's cell cold (spec §6.2), which is the
   normalization pass. A bounded "normalize the hot nodes no live cursor
   retains, then clear the directory" publication frees the whole table at once
   and is amortized against admissions; it must also preserve the physical
   page's birth/current interval, because writing `Cold(selected physical
   child)` is not a retirement.
5. **Selecting-pin cohorts can decide enqueue-versus-release in `O(log G)`.**
   `Index::frozen_between(birth, retire)` already answers "does any frozen
   revision select this page" from the `frozen` map, so publication can release
   an unpinned owner immediately or enqueue it under the *latest* selecting
   revision without scanning the retired vector. Replacing the `O(J)` scan in
   `Index::maintain` with cohorts keyed by that revision is a contained change
   and is required before the per-WRITE retirement term in spec §9 can be
   claimed.

## Steps 4.5.2 / 4.5.3 — bounded hot publication and affected lifetime work

This product revision continues `91c9c4938`; the evidence follow-up pins its
exact commit. Checkpoint 5 remains **NOT_RUN**. Public route receipts are
reissued only after this product revision is committed.

Implemented:

- Charged selected node copies, at most eight inode cursors / 64 Hot slots /
  1 MiB resident reservations. I/D facts and acknowledged inode attributes
  flow through the filesystem wrappers; one indexed Node update replaces
  the resident-node scan. Shared P/R bindings follow the live pack tail.
- EOF and advancing Base/Zero writes retain exact gaps/suffix/source offsets.
  A binding checks the selected root and every ancestor epoch/content version;
  its own leaf version is deliberately not a validity condition. Existing
  I/D/P keys must remain present and insertion keys must stay in [lower,fence).
- One merged leaf candidate per changed slot, a matching pack and directory,
  and one revision. Carries use recorded parent slots. Branch terminal cells
  inherit the parent upper bound, including after right-side deletion; byte
  balancing counts the actual encoded terminal body. Page staging checks
  kind/level and Hot child levels before exposing a candidate.
- Admission and necessary normalization share the generic mutation, with
  no second publication or candidate-error fallback. Closed siblings are Cold;
  shared Hot closure stays selected. Normalization preserves a child's physical
  birth/charge, and reassignment increments incarnation slot high water.
- Complete quota reservations precede eligible staging: ordinary 6/7 files,
  split-only <=98, restricted merged boundary <=227. Optional cold admission
  refuses at a slot/byte/quota bound and leaves the generic route available.
- Index/directory/pack retirement selects the latest pin in [birth,retire).
  An unpinned owner gets its exact release attempt in the triggering mutation;
  final pin release visits only its cohort. Failed release remains charged
  custody. Birth comes from PageStore custody rather than a page reread.
- Frozen G1 reconcile scans/saved facts are prepared off the state gate and
  rechecked against the live inode revision. Intervening G2 extents survive.
  Prepared rows, cumulative maps/ordered vectors and compaction/reconcile
  clones are precharged. Upload still has charged O(E_f) extent scratch;
  affected installation and legacy/payload maintenance remain real work.

Focused backing checks at the final product implementation (not speed rows):
12,288 interleaved EOF/Base/Zero writes, 12,285 hot writes, 11,316 ordinary
noncarry rows, maximum 6 new active candidate files on those ordinary rows,
1 physical path admission, 1,003 carries, 100 total boundary seeks,
370,820 node visits, 59,629 index writes (including 12,288 directory writes),
12,288 pack writes. Ordinary rows assert zero root seeks/index/pack fetches;
leaf/branch split minima were 1,815 / 1,951 B (limits 1,196 / 1,693).
The three-file final physical sum was 4,755,456 B, with 234,840 B hot resident
reservations; this is not the one-file 3 MiB selection or an RSS claim.
Both selecting-pin release orders pass without unrelated retirement sweeps.
The corrected eviction proof primes 32 files, pins that composite view, then
adds 32 files; slot 6 advances epoch 4 -> 8 while the old view keeps its bytes.
Host Budget refusal preserves revision, acknowledged bytes and allocation.

Retained implementation failures, never relabelled:

1. The first Clippy check reported the explicit `emit` range parameters as
   8/7 arguments; a local lint annotation retained the concrete inputs.
2. Linux backing attempt 1: 26 PASS / 4 FAIL, 13.122 s complete wall. The
   read-only page diagnostic exposed stale root bindings and a finite root
   terminal fence after right-side deletion. Fixed in the shared binding and
   branch codec/grouping paths; the original failed files/logs remain evidence.
3. Linux backing attempt 2: 29 PASS / 1 FAIL, 34.321 s complete wall. The
   epoch fixture's original slots remained shared, so its selected before/after
   pair did not witness reassignment. Its directory already showed epoch 4
   in a later slot. The focused initial fixture correction also failed (0.766 s).
   A prospective 32-file prime then 32-file eviction establishes the pair and
   passes in 1.079 s. No performance arm was rerun or selected from these tests.

The functional source still requires its source-pinned public proof/evidence
follow-up and owner review before checkpoint 5. No performance sample,
release admission, full CPU O(1), constant-RAM Commit or main-lane #264
integration is claimed here.

### Public regression found after the hot proofs

At `f83a186dd`, both added public hot cases passed, and the first reissued
original case (`active_repeated`) passed. `active_separated4096` then passed
its one-file 3 MiB private-backing check but failed C5 with `Capacity`, after
C1 had already committed. Its failed receipt and runtime are retained until
recorded cleanup; that Commit outcome must never be resent.

The source-derived reservation diagnostic counts 8,192 E deletions and
4,096 R deletions in this declared alternating-write case. Before unrelated
resident allocations, the prepared deletion keys, update map, outer ordered
vector, two publisher clones, inner ordered clone and index scratch reserved
10,624,386 B against the unchanged 8 MiB Budget. This is a reservation/duplicate
ownership defect, not permission to grow the Budget or shrink the case.
The fix transfers the already charged C5 map into the publisher and moves its
keys/values into one sorted candidate, while keeping old/new map-vector
capacity charged. Shared prune paths now precharge touched logical-ID sets,
returned owner vectors and P-key additions through publication. A focused
ordinary-API reconcile Budget-refusal test preserves the selected revision,
bytes and physical charge without allocating candidate pages.

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

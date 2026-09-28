# #273 phase 4.5 continuation handoff (prompt for the next agent)

> **Read this first, then the authority documents.** This is a handoff prompt,
> not a contract or a result. It records the state at source `dcbaf4884`
> (product source `91c9c4938`) so the next session can continue phase 4.5
> without re-deriving it. Nothing here is a benchmark, a speed claim or a
> release admission. Checkpoint 5 is **NOT_RUN**.

## 1. Identity and authority

- Worktree: `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs`;
  branch `codex/issue273-active-head`; origin
  `https://github.com/Ephemeral-AI-Lab/layerfs.git`; draft PR #274; issue #273.
  **One writer at a time in this worktree**; no second Cargo writer, no target
  directory outside it.
- Authority order: [implementation spec](PHASE4.5-IMPLEMENTATION-SPEC.md) →
  [research audit](PHASE4.5-RESEARCH-AUDIT.md) →
  [phase-4.5 log](PHASE4.5-LOG.md) → repository and `core/AGENTS.md` rules →
  [v1 format contract](ACTIVE-FORMAT-AND-EVALUATION-v1.md) (v1 index is
  retired for new attachments; pack pages stay v1).
- Scope is **hot path; concurrency constraints only**. #264/PR #269 owns
  mounted namespace and charged ancestry; do not reimplement it, do not add a
  256-node/128-dirty cap here, and do not block on merging it. Fully finish
  #273 before the main lane merges.

## 2. State at this handoff

| Step | State |
| --- | --- |
| 4.5.0 proposal | Done (`941613f65`) |
| 4.5.1 format/resolver | **Done and verified** (`91c9c4938`, evidence `8a704f40b`) |
| 4.5.2 hot publication | **Not started** |
| 4.5.3 boundary/lifetime | Only byte-balanced splits exist (in the generic splice). No carries proof, normalization, slot reuse, cohorts or reconcile precharge |
| 4.5.4 public proof | Format-level only: 25 backing tests + 14 registered `stage_route.py` cases at `91c9c4938` |
| 4.5.5 handoff | Architecture doc and v1 notice updated; source **not** frozen |
| Checkpoint 5 | **NOT_RUN** — no sample, no speed/space claim |

Implemented in this worktree (read these before editing):
`backing/active/page.rs` (v2 framing), `keyed.rs` (fences + tagged targets),
`hot_directory.rs` (fixed 64-slot table), `resolve.rs` (fence-partition
get/floor/scan; hot target must match epoch/kind/level), `splice.rs` (generic
v2 mutation, balanced splits, hot-in-place replacement), `index.rs` (one
selected root + one optional charged directory copy per publication).
`hot_cursor.rs`, `hot_path.rs` and `retirement.rs` **do not exist**.

## 3. Verification recipe (exact, from the repository root)

Host (macOS):

```sh
cargo +1.85.1 test   --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace
cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt    --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Linux-gated tests (cross build, then Docker on an owned volume):

```sh
cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked --offline \
  --target aarch64-unknown-linux-musl -p layerfs-workspace --tests
docker volume create layerfs-issue273-<label>
docker run --rm -v layerfs-issue273-<label>:/work \
  -v "$PWD/core/target/aarch64-unknown-linux-musl/release/deps/active_backing-<hash>:/tests/runner:ro" \
  -e TMPDIR=/work -e LAYERFS_ACTIVE_TEST_ROOT=/work alpine:3.22 /tests/runner --test-threads=1
```

Registered public route case (reuse the closed fixture; never regenerate it):

```sh
cargo +1.85.1 build --release --manifest-path core/Cargo.toml --locked --offline \
  -p layerfs-server -p layerfs-daemon -p layerfs-bridge --features layerfs-bridge/native --example public_key
LAYERFS_PROOF_PROFILE=release python3 core/crates/layerfs-workspace/tests/stage_route.py \
  --fixture core/target/issue273/checkpoint3-prepared-v1/result.json \
  --binaries core/target/release \
  --test-binary core/target/aarch64-unknown-linux-musl/release/deps/stage-<hash> \
  --case active_separated4096 --output core/target/issue273/<new-dir>
```

- `--output` must not exist; the driver pins `git rev-parse HEAD`, so **commit
  before issuing receipts** or the receipt names the wrong source.
- The 14 cases used at 4.5.1: `active_repeated`, `active_separated4096`,
  `active_generation`, `active_many_file`, `active_mounted`,
  `active_split_slot`, `active_retained32`, `active_mixed_compact`,
  `active_mutation_compact`, `active_quota_refusal`, `active_quick_controls`,
  `active_namespace`, `active_close`, `active_payload_refund`.
- These are functional rows (`performance_claim=false`, `cache_claim=null`);
  never quote them as speed. `tools/preflight.sh` is retired — do not run it,
  and never claim CI.
- Delete the Docker volume and any container you created when done.

## 4. Next step: 4.5.2, bounded hot EOF-append (recommended first cut)

Design already validated against the source; keep it this narrow first, then
extend. Everything not eligible falls back to the generic v2 route **in the
same revision**.

1. **One revision per ordinary WRITE.** `filesystem/active_file.rs` rejects a
   publication whose revision is not the Workspace's next revision, so
   admission, normalization and the mutation must be merged into one
   candidate. Never publish twice and never retry through another algorithm.
2. **Admission (`heat`) merged into the mutation.** In `Mutation`, walk from
   the mutation's current root to a key, converting each node on the path to a
   hot target: a leaf is selected in place in a new slot (its page is
   unchanged), a branch whose child changed is re-encoded into a new page and
   keeps/receives its slot, and a cold node receives a fresh slot. Return the
   new root when the root itself changed. At most `h` slots per key; admission
   is best-effort and must skip (not fail the write) when a slot, byte or
   height bound refuses.
3. **Cursor and bindings.** `HotCursor { inode, generation, selected inode
   revision, bindings: Vec<Binding> }`, at most 8 cursors, charged.
   `Binding { role, slot, epoch, lower, fence, ancestors: Vec<(slot, epoch,
   version)> }` with `role` in `Inode | Locator | Frontier | Inverse`. Record
   `lower`/`fence` from the parent cells at admission.
4. **Validation rules (derived; do not weaken them).**
   - every recorded **ancestor** `(slot, epoch, version)` must still match the
     incarnation state; a split changes a parent, which invalidates the
     binding. The leaf's own version is deliberately *not* required, so
     another file writing a different key into a shared leaf does not
     invalidate a cursor.
   - `Inode` and `Locator`: the key must still be **present** in the leaf.
   - `Frontier` and `Inverse`: an insertion, valid iff
     `lower <= key < fence`; with ancestors unchanged the leaf's routing range
     is unchanged, so the positional insert is exact.
   - the merged body must fit `BODY_BYTES`; otherwise refuse (a split is a
     boundary event for the generic route, which then re-admits).
   - a predecessor-only check is **not** sufficient: after a split the left
     node keeps the slot and a key at or above the new fence would be
     inserted into the wrong node. The ancestor-version check is what makes
     the positional rules safe.
5. **Publication.** Stage one new page per changed leaf (same slot and reuse
   epoch), one new directory page, install the selection with unchanged root
   and height, retire the replaced leaves and the old directory copy by
   revision, and publish the matching pack tail and revision. The pack tail
   page and the `P` locator stay in the same publication as today.
6. **Eligibility for the first cut.** `offset == selected EOF`, 1..128 bytes,
   the inode already dirty in this generation, `selected.storage == 2`, a
   valid cursor, all four bindings valid, no split, and no pack rollover
   (a new logical page makes the `P` key an insertion → generic + re-admit).
   The `#248` advancing-edit case (advance inside inherited Base/Zero
   coverage) needs the retained frontier/source offsets and is the next
   extension, not part of this cut.
7. **Observability for proof.** The directory file name is
   `a-hot-v2-<id>-<epoch>` and index files are `a-index-v2-<id>-<epoch>`, so an
   external test can assert that an eligible sequence selects a directory and
   that bytes, lengths, aliases, capture views and clean-close refunds are
   unchanged. Count `index_page_writes`/`index_fetches`/`pack_page_writes`
   through `StoreStatus` for the before/after numbers the spec asks for.
8. **Known gap to state honestly.** Slots are never reused until 4.5.3
   normalization exists, so a long workload can exhaust the 64-slot table;
   admission then stops being available and the route falls back to the
   generic path. Do not enlarge the bound to hide that; implement
   normalization (§6.2) or report the limit.

## 5. Guardrails

- Each commit must build and be verified; record production LOC before → after
  with `python3 tools/production_loc.py --json` on the exact first parent and
  the committed tree, and put it in the commit message.
- Production files ≤ 999 physical lines, `lib.rs`/`mod.rs` ≤ 200; no test-only
  product paths, no inline tests in `src/`; tests stay in `tests/`.
- Fail closed: refusal is a bound, not a reason to widen one; unknown
  allocation/unlink/outcome keeps custody and charge.
- Never relabel checkpoint-4 receipts, never promote a functional route row to
  a speed row, never reuse a warm cache claim, and keep `performance_claim`
  false.
- Do not touch #264 files, do not add dependencies, do not run
  `tools/preflight.sh`.
- When the hot path lands, the evidence commit must reissue the registered
  route receipts at the new source and add the focused hot-path proofs the
  spec lists (carries, retirement inspections, cursor eviction, epoch reuse,
  post-Commit continuation, quota refusal, Budget exhaustion).

## 6. Definition of done for phase 4.5

Steps 4.5.2–4.5.5 of the spec's table, the registered matched set and the
independent proofs, the architecture/status update and a frozen candidate with
per-commit LOC. Only then may checkpoint 5 run; it stays **NOT_RUN** until the
owner reviews the frozen candidate. Report failures and unrun work as plainly
as passes.

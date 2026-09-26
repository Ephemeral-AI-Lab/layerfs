# #245 handoff: finish D, E and F — streaming transport, generations, frozen proof

> **Status:** superseded by
> [HANDOFF_E_F_CONTINUATION.md](HANDOFF_E_F_CONTINUATION.md), which carries the
> remaining work. Phase 1A, Phase 1B packages A–C, the mounted write-path
> repair, evidence blocker 1 and the F **control** arm are done, committed and
> gated on this branch. Package D, the route harness (blocker 2) and the F
> target freeze were completed under this handoff; package E is in flight and
> F's candidate arm is not started. The four design decisions that gate the
> remaining work are **made and recorded** in §8 — proceed on them, do not
> re-open them.

Copy the assignment below into a new task. This is a **delta** handoff. Read it
with [ARCHITECTURE.md](ARCHITECTURE.md), [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md),
[VERIFICATION_PHASE1.md](VERIFICATION_PHASE1.md), [LOAD_BEARING_CASES.md](LOAD_BEARING_CASES.md),
[evidence/phase1b-mounted-write-repair/REPORT.md](evidence/phase1b-mounted-write-repair/REPORT.md)
(this round's record — the eight defects, both control campaigns, the
qualifying control), and
[the extent-sequence pin](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md),
which was updated by this round and now describes the algorithm as actually
implemented. The prior handoff
([HANDOFF_PHASE1B_CONTINUATION.md](HANDOFF_PHASE1B_CONTINUATION.md)) remains
accurate for everything it marks done.

## Where the branch stands

| Item | Value |
| --- | --- |
| Branch | `codex/issue245-range-cow-plan`, draft [PR #247](https://github.com/Ephemeral-AI-Lab/layerfs/pull/247), worktree `~/.codex/worktrees/issue245-range-cow-plan/layerfs` |
| Head | `9b891dc6f` (docs) on `fdfc41032` (+4, reclaim) on `76c832f0c` (+121, splice lane) on `4ef71e596` |
| Production LOC | 56,035 → 56,160 across the two repair commits; legacy 68,728 unchanged |
| Done this round | the mounted write path repaired (8 defects); blocker 1 (fresh prepared state + sealed masters); the F pre-optimization **control collected and qualifying**; the ~5.5 s cleanup diagnosis explained and fixed |
| Not done | D (streaming transport), E (generations + reconcile), blocker 2 (route harness), F candidate arm + target freeze; adjacent: #232's 56 shapes verification, load-bearing cases, namespace 128/128/32 KiB ceilings, the 5 s `Exec Unknown` verification |

## 1. Read before editing

1. `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
   `docs/general/documentation-policy.md`, `core/benchmark/fs-bench-pro/AGENTS.md`,
   `benchmark/fs-bench-pro/QUICKSTART.md`; `docs/general/release-policy.md`
   before any release statement.
2. `core/docs/issues/245/`: this file, then the documents named in the header,
   then `evidence/phase1-ordinary-shell-repair/REPORT.md` (Phase 1A, closed),
   then `../243/PHASE1_CONTRACT.md` and `../243/TEST_WORKSPACE_AND_COMMANDS.md`.
3. Source, for D: `core/crates/layerfs-bridge/src/contract/request.rs` and
   `src/adapters/native/protocol/{frame.rs,metadata.rs,payload.rs}` and
   `src/adapters/native/{client.rs,server.rs}`; then
   `core/crates/layerfs-server/src/service/{handler.rs,input.rs}` and
   `src/service/save/content.rs`; then
   `core/crates/layerfs-content/src/file/edit/{input.rs,apply.rs,tree.rs}`;
   then `core/crates/layerfs-workspace/src/commit/{lower.rs,source.rs,save.rs}`.
   For E: `core/crates/layerfs-workspace/src/commit/{reconcile.rs,completion.rs,operation.rs}`,
   `src/overlay/snapshot.rs`, `src/backing/metadata.rs`,
   `src/runtime/coherence.rs`, `src/runtime/lifecycle.rs`.

## 2. What is established — do not redo it

**Phase 1A and Phase 1B A–C** are closed exactly as the prior handoff records;
their commits and receipts stand.

**The mounted write-path repair (this round).** Phase 1B's focused gate had
only ever exercised single-leaf sequences in an in-memory page store; the first
mounted run failed on *every* first write to an existing file. Eight defects of
that one class were pinned by labelled diagnostics and fixed in `76c832f0c`
and `fdfc41032`; the details, sites and receipts are in
[evidence/phase1b-mounted-write-repair/REPORT.md](evidence/phase1b-mounted-write-repair/REPORT.md).
The essentials the next work builds on:

- A NULL pieces root folds into the sequence the version already is: one
  implicit base read of its selected content (first edit of a never-edited or
  capture-converted file), or an empty sequence (fresh/truncated-to-nothing).
- The write path always passes exactly the replacement stream as parts.
- The recorded figures are `edits` = counted runs or `u16::MAX` when a subtree
  was shared, and `replacement` = the **exact** total carried by arithmetic
  (`old − dropped + inserted`); lowering cross-checks both, `save` never
  reuses a base for a `u16::MAX` version.
- Branch levels are relative (root declares the tree's height ≤ 7); the cursor
  (`backing/metadata_cursor.rs`) holds one page path + one leaf; sibling
  advance re-enters through the parent.
- Ownership edges are taken from the encoded page **before** the ledger write
  reuses the window (`ownership.rs` `write_raw_page`), and reclaim cleans up
  both page kinds through `edges_raw` + `load_raw` (`metadata_reclaim.rs`).
- The pieces cursor lives in `backing/metadata_cursor.rs`; both files sit well
  under the 999-line ceiling.

**The frozen control (the F pre-optimization arm) is COLLECTED.** At `fdfc41032`,
`benchmark-results/fs-bench-pro/issue245-shell-package-v3-control-01/`: all four
registered cases **functional PASS, cleanup PASS, sealed verifier PASS**
(0.05–0.16 s), complete commands 0.82–5.88 s inside the 15/25 s budgets,
`repeated-one-byte` back to `write=16 open=16`, the failure case still
publishing nothing. Every latency cell is `INELIGIBLE` under the frozen cache
contract. **Do not re-run the control arm.** The intermediate campaign
(`issue245-shell-package-v2-control-01`, two functional FAIL rows) is retained
as the evidence that located the last defect. The prepared state
(`issue245-shell-package-v3-prepared-01`, image
`sha256:9f76aefe535a9a22daf9b2e6a0df30779dc3c08798864485c98b78c59accf48b`)
is reusable through `shell_package.py run` for **candidate** attempts only
after the source changes; `prepare` refuses a dirty tree and `run` requires the
current source to match the prepared identity, so a post-D/E candidate needs a
fresh `prepare` at that frozen source.

**The ~5.5 s cleanup is explained and gone** (the stuck reclaim was defect 8;
passing rows now clean up in ~0.5 s). The 5 s `Exec Unknown` was *probably* the
same family (structural shifts stalling the all-piece rebuild that no longer
exists) but is **unverified** — one v2-selection run settles it.

## 3. The architecture you are implementing

### D — the transport coordinator (four boundaries, one coherent stream)

Today the replacement **bytes already stream** end-to-end: the workspace's
`ReplacementSource` (`commit/source.rs`) walks the cursor behind a `Source`
trait; the bridge client reads it into 16 KiB `Kind::Body` frames
(`protocol/frame.rs`, `client.rs`); the server's `Input` reader
(`adapters/native/server.rs`, `payload.rs`) enforces per-frame identity,
cumulative bytes and `frame_budget(bytes) = div_ceil(1024)+257`. What does not
stream:

| Boundary | Site | What is bounded today |
| --- | --- | --- |
| Bridge request | `contract/request.rs` — `MAX_REPLAY = 8 MiB`, the `Operation::EditFile` arm (256-edit + `MAX_FILE` + replay checks), `input_length()` | edit **descriptors** ride the single 32 KiB `Kind::Begin` frame (24 B/edit, decode caps `count(256, 24)` in `protocol/metadata.rs`) |
| Workspace lowering | `commit/lower.rs` — `FilePlan { edits: Vec<Edit> }`, `push_edit`'s 256 cap, `vector(256)` | one `Vec<Edit>` materialized per file per Commit; `commit/save.rs` sends it as one `EditFile` |
| Server save | `service/save/content.rs` — `Replacements` | re-materializes every replacement part into `Vec<Vec<u8>>` before C1 runs |
| C1 construction | `file/edit/input.rs` — `MAXIMUM_EDITS_PER_OPERATION = 4_096`, `Replacements`, `Segment`/`Plan` | takes `&dyn EditSource` (the seam exists) but only the materialized impl feeds it |

The decided design (§8, D-1) makes the descriptor list a **prefix of the
existing body stream**: `Begin` declares counts instead of the `Vec`; the first
body frames carry the packed descriptor block; the remaining frames carry the
replacement bytes in edit order; `EndInput` cross-checks byte and edit totals.
The server parses descriptors from the stream head and hands C1 an `EditSource`
backed by the reader. Per-frame validation must cover ordering, lengths, base
identity, acknowledgements and total resource charges. Remove the 256-edit,
8 MiB replay and 1,024-piece ceilings **only** in the commits that make their
layer streaming — never by moving a ceiling elsewhere — and keep the 4 GiB
`MAX_FILE` logical ceiling and real resource budgets explicit. One Branch-head
publication per Commit and canonical `Base` reuse are unchanged.

### E — generations and reconciliation

The Commit lifecycle: `capture_submission` (`overlay/snapshot.rs`; the
host-wide freeze flag plus `Arc<RootOwner>` capture, generation advance) →
save/stage (`commit/save.rs`) → Store publication (`commit/completion.rs` /
`commit/operation.rs`; **no gate held**, G2 writes run) → outcome validation →
`reconcile_commit` (`commit/reconcile.rs:272`) → completion. The reachable
post-publication `Busy` is the writer-gate collision at `reconcile.rs:284`
(`MetadataHost::writer` is an unqueued CAS); the `:419-433` root/revision
compare is defensive *until* writes are allowed to publish during the build,
which is exactly what the decided fix (§8, E-1) enables. A post-publication
failure currently **wedges** the workspace permanently (the retained
submission; `tests/commit_staged.rs:92-104` asserts it). Frozen-root custody is
the `Arc` graph plus `metadata_reclaim.rs`'s `strong_count == 1` +
`routine_eligible` selection — reclaim only after all roots, handles and
uncertain outcomes release.

### The algorithm and its complexity (as implemented, verified this round)

- One accepted edit folds only the leaves the replaced interval touches,
  copies their ancestors and shares every other page by reference:
  `O(H + K)` extents plus the bytes the command sent. `H = log_F P` with
  branch fanout 248 and leaf capacity 124 records; `MAX_EXTENT = 2^24−1`
  (a larger extent is adjacent parts); levels relative, height ≤ 7.
- The first edit of a base-backed version folds into one implicit base extent;
  no page exists until then. A truncation is one trailing deletion edit; an
  extension owns zero bytes.
- Sharing is accounted, not guessed: shared subtree lengths are charged, their
  extents and replacement bytes stay unread, `edits` becomes `u16::MAX` and
  `replacement` carries by arithmetic; when nothing was shared the fold's own
  totals must agree exactly or the splice is refused.
- Reads and the Commit walk use the bounded cursor: one page path plus one
  leaf, `O(H + touched leaves)`, deadline-checked between page reads.
- What is still bounded by the *file* rather than by a frame: the per-call
  replacement extents are materialized before the splice (`Parts::declare`),
  so a complete-file construction is bounded by the file — and the Commit
  transport caps at 256 edits / 8 MiB replay / 1,024 pieces. **That is D's
  work, by decision D-1.**

## 4. Work packages (do them in this order unless a dependency says otherwise)

**0. Freeze the F comparative target FIRST** (§8, F-1): write the frozen
target — functional parity on all four registered cases plus a
complete-command wall envelope of **≤ 2× the control's walls** (0.82 s / 2.20 s
/ 1.09 s / 5.88 s; verifier ≤ 0.16 s observed) — into the evidence docs from
the v3 receipts, before any D/E source edit lands. Latency cells stay
`INELIGIBLE`; no numeric latency admission.

**1. Blocker 2 — bring up the route harness** (§8, B-1): build the linux
binaries (`zigbuild` musl: `layerfs-server`, `layerfs-daemon`, the
`layerfs-bridge` `public_key` example into one binaries dir), produce a store
master (the one unverified link — most likely the `layerfs-server`
`prepare_store` example; if it is awkward, the handoff explicitly allows
writing the fixture deliberately), run `tests/prepare_large_edit.py` to build
the closed 64 MiB fixture, then drive one `*_route.py` case (e.g.
`stage_route.py --case semantics`) in its privileged `rust:1.85.1-bookworm`
container. This unlocks the Linux-gated suites — `stage.rs`, `commit_staged.rs`
(including `commit_successor`, the existing G1/G2 overlap test), `composite.rs`
— which are E's regression net.

**2. Package D** per §3 and decision D-1. Order: write the wire-format pin and
update the affected architecture docs **in the same commit** as the wire
change; prove the frame path is bounded *before* deleting a ceiling; delete a
ceiling only in the commit that makes its layer streaming; extend
`pieces_sequence.rs`-style focused tests plus the pinned tests that currently
lock the ceilings (`layerfs-content/tests/edit_bounds.rs`,
`layerfs-workspace/tests/write.rs` `write_frontier`/`write_envelope` (#[ignore],
route), `layerfs-bridge/tests/protocol.rs`). Gate: large replacement and fresh
streams complete with exact final bytes, no unbounded `Vec<Vec<u8>>`, correct
custody and cleanup, the 4 GiB ceiling explicit.

**3. Package E** per §3 and decision E-1: narrow the writer gate to the two
ordering points (first state read; install), let G2 writes publish during the
successor build, converge with a bounded rebuild from the newer root at
install, make the two short holds wait (deadline-bounded) instead of `EBUSY`,
and make a post-publication reconcile failure retriable rather than wedging.
Preserve frozen-root custody. Gate: the mounted three-generation observation
(pre-capture bytes in `B1`, post-capture bytes in live `G2`, exact `B2` and
live `G3`, both older heads readable, no lost/duplicated/reordered writes, no
unexplained `Busy`) — through the blocker-2 harness.

**4. Package F candidate arm**: fresh `prepare` at the frozen post-D/E source,
one candidate attempt per case under the same enforced cache contract, every
cell reported `PASS`/`FAIL`/`INELIGIBLE`/`NOT_RUN`.

**5. Close the named leftovers if the lane allows**: one #232 v2-selection run
(likely largely unblocked by the splice repair; shapes whose Commit exceeds
the pre-D ceilings need D), the 5 s `Exec Unknown` verification, the
load-bearing cases, the namespace 128/128/32 KiB ceilings (a separate
namespace-lane package — do not fold it into D).

## 5. Operational playbook

- **Fast component loop** (seconds, warm): `cargo +1.85.1 zigbuild
  --manifest-path core/Cargo.toml --locked --offline --target
  aarch64-unknown-linux-musl -p layerfs-workspace --test pieces_sequence`,
  then `docker run --rm -v
  "$PWD/core/target/aarch64-unknown-linux-musl/debug/deps:/d:ro" alpine:3.22
  /d/pieces_sequence-<hash> --test-threads=1`. The musl target dir is
  `core/target/aarch64-unknown-linux-musl`, not `target/`.
- **Mounted diagnostic**: build the release daemon (same `zigbuild --release
  -p layerfs-daemon`), build a diagnostic image from the prepared
  `image-context` with `ENV LAYERFS_FUSE_TRACE=1` and `ENV
  LAYERFS_FUSE_ERROR_DIAGNOSTIC=1`, clone one closed master (byte copy, seals
  checked) and run the exact command through `benchmark_shell run|seed`; the
  sealed verifier follows. For internal sites build the **dev** profile for
  musl and add temporary step markers — **remove every marker before
  committing** (this round's method, see the evidence report).
- The `LFS_FUSE_ERROR` diagnostic prints the mapped error only; `EBUSY` is
  exactly `WorkspaceError::Busy`; plain `Io` from the workspace is a distinct
  class — bisect it with markers, never by re-running a campaign arm.
- `timeout` does not exist on this host's zsh: `perl -e 'alarm shift; exec
  @ARGV' <secs> <cmd>`. `cargo test --all-targets` on macOS compiles the
  Linux-gated suites out; `zigbuild --workspace --all-targets` for musl fails
  linking `-lsqlite3` for `layerfs-history`/`layerfs-storage` targets (no
  `bundled` feature; nothing container-side needs them) — build the target you
  need.
- `cargo fmt` can push a file past the 999-line ceiling: always run
  `python3 core/tools/check_product_boundary.py` after formatting, and split
  by responsibility before editing a near-ceiling file heavily.
- Per-commit LOC: `git archive <parent> | tar -x -C <dir>` for before,
  `git checkout-index -a --prefix=<dir>/` for after, then
  `python3 core/tools/production_loc.py --root <dir> --json`; re-verify
  against the committed tree. Docs-only commits report delta 0.
- Remove stray `docker run` containers when a job dies; they hold ports and
  disk. Diagnostic images from this round (`issue245-repair1/2/2dbg/3`) may be
  pruned; the v3 prepared image is load-bearing.

## 6. Discipline that is not negotiable

- **Route:** `/bin/sh -c` in the mounted Workspace; the kernel's syscalls
  decide the callbacks. No edit tool, no range ioctl, no command-text
  classifier, no direct Store mutation.
- **Sampling:** one attempt per case per arm; the control arm is spent; no
  best-of, no unchanged-arm rerun, no timeout/worker increase, no shortened
  case, no dropped cell, no rewritten receipt. One construction worker;
  `LAYERFS_CONSTRUCTION_WORKERS=1`.
- **Cache:** a timed phase pays for its own work from a declared state; the
  frozen selection's latency cells are `INELIGIBLE` by contract — the
  comparative target (§8 F-1) is functional parity plus the wall envelope,
  nothing else.
- **Budgets:** complete command ≤ 15 s (mixed refresh 25 s), verification
  < 10 s. Evidence is append-only with identities and raw output; `UNAVAILABLE`
  for anything not measured.
- Do not run the retired root preflight; no CI claims; builds `--locked`; no
  new dependencies; no patched/vendored crates; aarch64 AEAD flags come from
  the repository-root `.cargo/config.toml`.
- `inode.edits == u16::MAX` is the unknown-count marker: a version whose splice
  shared a subtree is lowered, never base-reused — preserve this in D.
- Changed wire contract, algorithm or bound ⇒ update the affected
  `core/docs/architecture/` pin **in the same commit**.

## 7. Final reporting

At the frozen final source run once: `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 fmt
--manifest-path core/Cargo.toml --all --check`, `python3
core/tools/check_product_boundary.py`, `python3 -m unittest discover -s
core/tools -p 'test_*.py'`; plus the musl run of every workspace suite the
route harness now unlocks. Report: per-commit production LOC before/after/
delta with method; every registered cell as `PASS`/`FAIL`/`INELIGIBLE`/
`NOT_RUN` with receipt paths; exact reproduction commands; the updated
architecture pins; residual risk and open gates (#232's shapes, load-bearing
cases, namespace ceilings, `Exec Unknown` verification if still open).

## 8. The decisions (owner-approved 2026-09-26 — implement, do not re-litigate)

| # | Decision | Choice | Rejected alternatives |
| --- | --- | --- | --- |
| D-1 | D frame protocol | **Descriptors as a prefix of the existing body stream**: `Begin` carries counts; first body frames carry the packed descriptor block; remaining frames the bytes; `EndInput` cross-checks totals; server streams descriptors then an `EditSource` over the reader | a new `Kind::Descriptors` frame (bigger protocol blast radius); raising `METADATA_BYTES`/edit caps (forbidden: moves buffers, doesn't stream) |
| E-1 | E reconcile shape | **Retry-free reconcile**: narrow the writer gate to the two ordering points; G2 publishes during the successor build; a bounded rebuild from the newer root converges at install; the two short holds wait (deadline-bounded) instead of `EBUSY`; post-publication failure becomes retriable, never wedges | wide gate + successor install only (writes still `EBUSY` through each pass — the exact `mixed-refresh` failure mode); auto-retry alone (leaves the wedge) |
| B-1 | Blocker 2 | **Full route harness** (binaries → store master → `prepare_large_edit.py` → `*_route.py` in the privileged container), fallback to a deliberate minimal fixture only if the store-master link proves awkward | minimal fixture first (leaves the historical suites `NOT_RUN`) |
| F-1 | F comparative target | **Functional parity + complete-command wall envelope ≤ 2× the control walls**, frozen from the v3 receipts before any D/E edit; latency cells stay `INELIGIBLE` | functional parity only (no defensible before/after claim); making latency admissible (changes the frozen selection) |

Proceed exactly on these decisions and finish the remaining work: freeze the
F target, bring up the harness, land D, land E, collect the candidate arm, and
report every cell.

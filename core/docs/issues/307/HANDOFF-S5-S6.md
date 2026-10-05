# Next-agent prompt: finish S5 and S6

> **Status:** Dated planning checkpoint (2026-10-05); not release evidence or a
> product contract. It continues [HANDOFF-S4-S6.md](HANDOFF-S4-S6.md), whose
> rules still apply unless this document says otherwise.

## Assignment and stopping boundary

Finish LayerFS cluster-two **S5** and **S6** in the primary checkout
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, on local `main`, product code under
`core/`. S4 is complete and committed. S5 is **in progress and uncommitted**.
S6 has not started. The owner paused this thread mid-S5 because a test command
ran for more than 12 minutes; the thread stopped it, recorded the new test
ceiling in the guides and wrote this handoff. Local commits and tracker comments
on [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307) are authorized
by that tracker; pushes, releases and deployments are not.

## New binding rule: no test command over 2 minutes

Added to [AGENTS.md §4](../../../../AGENTS.md#4-code-build-and-docs) and
[core/AGENTS.md](../../../AGENTS.md#checks-and-completion) at the owner's
instruction. Every test invocation gets an explicit timeout of at most 120 s.
Build first with `--no-run`. Select by package or test when the whole suite
cannot fit. Never background a test to wait it out and never wrap one in a
repeat loop. A run that reaches the ceiling is a FAILED hang: diagnose from
source and bounded output before any rerun.

The command that broke this was a two-iteration shell loop around
`cargo test -p layerfs-workspace --test namespace_daemon`, piped through `grep`,
so nothing was printed while it hung. Do not repeat either pattern.

## Identities

| Identity | Value |
| --- | --- |
| HEAD (S4 completion) | `f5558fc22cb656a3f7528b64008ec8568021829e`, tree `3d39937230e07c95251580f620740a80e32ac0a7`, local-only |
| S4 tracker receipt | [issuecomment-5995836298](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-5995836298); S4 checkbox ticked; S0 and S5–S13 unchecked |
| S4 audit and evidence | [S4-EXIT-AUDIT.md](S4-EXIT-AUDIT.md), [checks/s4-namespace](checks/s4-namespace/identity.json) |
| Production LOC at HEAD | 146,209 combined; core 80,792; reference 65,417. Receipt `core/target/cluster2-307/loc/s4-complete-staged.json` |
| Toolchain / Linux image | Rust 1.85.1; `rust:1.85.1-bookworm`, `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`; targets `core/target` and `core/target/cluster2-linux`, cache `core/target/cluster2-linux-cargo` |
| Earlier milestones | S1 `7019801f9`, S2 `8e2976e4e`, S3 `c4b49a121` |

Nothing is staged. No Cargo, test or owned Docker process is running. The four
unrelated containers (`layerfs-experiment-305-dev` and three `layerfs-<hash>`)
are still running and were never touched.

## Working tree: uncommitted S5 work

Preserve unrelated state exactly as before: modified `docs/README.md`, untracked
`core/docs/issues/301/`, three `docs/general/*.md` guides,
`docs/research/extent-normalization-analysis-2026-10-02.md` and `output/`.

Owned and uncommitted, all part of S5 unless noted:

| Path | Content |
| --- | --- |
| `AGENTS.md`, `core/AGENTS.md` | The 2-minute test ceiling (owner instruction, not S5) |
| `core/docs/issues/307/HANDOFF-S5-S6.md`, `PROGRESS.md` | This handoff and its pointer |
| `core/docs/issues/307/checkpoints/f5558fc22.md` | Exact text of the posted S4 receipt; adopt it in the next commit |
| `core/docs/issues/307/S5-PAYLOAD-CONTRACT.md` | The selected algorithm, written before code |
| `layerfs-overlay`: `sql/schema.sql`, `src/{cells,layers,stream}.rs` (new), `src/{payload,inode,compound,types,sql,reclaim,db,lib}.rs`, `tests/payload.rs` (new), `tests/{compound,engine}.rs` | Schema v8 and the payload engine |
| `layerfs-workspace`: `src/{write,read}.rs` (new), `src/{operation,attributes,job,port,lib,create,remove,rename}.rs`, `tests/payload.rs` (new), `tests/{harness,common}/mod.rs`, `tests/{base,namespace,namespace_profile,namespace_daemon}.rs` | Write, truncate and composed read |
| `layerfs-daemon`: `src/{commands,read_port}.rs`, `tests/owner.rs` | `SourceRead` job and the read port |

## What S5 has so far

Read [S5-PAYLOAD-CONTRACT.md](S5-PAYLOAD-CONTRACT.md) first; the code follows it.

- **Layers.** One payload layer per live generation in which the inode has a
  row. `inherited_cutoff` is now engine-maintained and relative to the layer
  below, not to the base, so known install needs no rewrite. A row first written
  in a generation gets cutoff = the lower view's length, epoch 0, height 0.
  `put_inode` returns the layer; a smaller `size` on a regular file is a shrink.
- **Cells.** `payload` rows are trimmed to the last valid byte, with `validity`
  NULL when every stored byte is valid, and stamped with the layer epoch.
  `write_cells` handles each intersecting cell alone: a fully covered cell is one
  upsert with no read, a partial one is one point read and one upsert.
- **Shrink.** `shrink` trims the boundary cell, pushes one step onto the
  `shrink(ns,serial,gen,depth)` staircase by binary search and height change,
  and lowers the cutoff. `stale` is the binary-search staleness test.
- **Read.** `source_read` / `captured_read` compose local layers for a window of
  at most 128 KiB into `LocalRead { data, inherited bitmap, span }`.
  `SourceView::read` then fetches the one covering base range.
- **Workspace.** `Operation::Write { serial, position: At|End, data }`,
  `SetAttributes { size }`, `Refusal::TooLarge`, `WriteData` (shared bytes,
  printed by length). `Changes.write` carries the window into `Overlay::apply`.
- **Daemon.** `Command::SourceRead`, `Response::Read`, `OverlayRead::read` for
  `OwnerClient`.
- Terminal reclaim deletes `shrink` rows as phase 5.

## Verification status: read this before trusting anything

All runs below were macOS only, on the dirty tree, and their output was printed
to the console and **not retained** under `checks/`. None is S5 exit evidence.

| Scope | Last observed result | When, relative to the edits |
| --- | --- | --- |
| `layerfs-overlay --test payload` (5 tests) | 5 passed in about 3 s | After the last engine edit |
| `layerfs-workspace` whole package | 22 passed | After adding write/read, before the payload tests |
| `layerfs-workspace --test payload` (3 tests) | 3 passed | After the last product edit |
| overlay + workspace + daemon + SDK | 58 passed | After schema v8, **before** the Workspace write/read layer and `SourceRead` |
| `layerfs-workspace --test namespace_daemon` | **HUNG, stopped after 12+ minutes, no output** | After appending one new test |
| `cargo check --all-targets` on the four crates | compiles, no warnings | After the final edits (compile only) |

**Not run since the S5 edits began:** `cargo fmt --check` (new files are probably
unformatted), Clippy, the boundary guard, tool tests, the full core suite, the
daemon package tests after `SourceRead`, and every Linux check.

### The hang

The appended test
`concurrent_appenders_and_a_tail_reader_see_whole_records_through_real_owner_jobs`
in `tests/namespace_daemon.rs` never finished. Cause **undiagnosed**. The other
four tests in that file passed three times before S5 and were not seen passing
after it, because the pipeline printed nothing.

What is known from reading the test: it set its stop flag only after joining the
writers, so any writer panic would leave the tail thread spinning inside
`thread::scope` forever. That explains a hang but not the panic behind it.
Unverified candidates for the underlying failure: an admission refusal surfacing
through `OwnerClient::namespace` or `::read` as an error that `applied` unwraps
(the test helper retries `AdmissionFull` only for jobs it submits itself, not
for jobs the Workspace port submits); or a real defect in `Position::End`,
`SourceRead` or the daemon charge for large replies.

The test is now `#[ignore]`d with a drop guard for the stop flag and a 60 s
deadline. **Those edits compile but have never executed.** Next steps: build
with `--no-run`, run each of the four older tests individually under a 120 s
timeout with `--nocapture` and unfiltered output, then the ignored test alone
with `-- --ignored --nocapture`. Fix the cause, not the symptom. If admission is
the cause, decide in product code how a Workspace port call waits for owner
credits; do not paper over it in the test.

### Test-side expectation changes made during S5 (not product defects)

Schema assertions 7 → 8. Three S1–S3 assertions that compared a rewritten
inode's `inherited_cutoff` with the caller's value now expect the layered value
(`engine.rs` twice, `base.rs` once). Three first-draft expectations in the new
`tests/payload.rs` were arithmetic mistakes and were corrected. One draft
assertion assumed a 128 KiB base read fetches at least 128 KiB upstream; the
fixture deduplicates, so it now compares a write's demand with the inode's
attribute facts. Record these in the S5 failure log when it is created.

### Numbers seen on the console (regenerate; do not cite as evidence)

128 KiB overwrite after 65,536 alternating one-byte writes: 45 statements,
2,752 VM steps, 36 rows, identical to an unfragmented file. Shrink discarding
2,048 cells: 16 statements, identical to discarding 2. A 1,500-step staircase:
shrink 18 → 38 statements, a read across 32 stale cells 356. A 1 TiB hole: zero
pages, hole read 4 statements. 2,000 files of 100 bytes: 184 bytes each on disk.
Dense data: 1.133 pages per cell. First write into an inherited 400,000-byte
file: 2,094 upstream bytes, equal to its attribute facts; later writes none.
A window with 2,048 inherited gaps: 5 upstream objects, same as unfragmented.

## Remaining S5 work, in order

1. Diagnose and fix the hang as above. Then run fmt, Clippy, the guard and the
   packages one at a time under the 2-minute ceiling.
2. **Split the long engine test.** `dense_fragmentation_...` performs 65,536
   single-byte compound jobs and the file took about 3 s on macOS; confirm it
   stays far below the ceiling on Linux or reduce its count with a stated reason.
3. Complete-operation EXPLAIN and runtime profiles for write, truncate and read
   through Workspace and the real owner (the pattern of
   `tests/namespace_profile.rs`), at several unrelated scales.
4. A daemon-level proof of concurrent appends and a tail reader, once the hang
   is understood.
5. **P4, required for S5 acceptance:** hole-aware canonical construction in
   `layerfs-content`. Extent-tree branch summaries are cumulative within a node,
   so subtrees over a uniform zero run are identical objects. Construct a zero
   run by reuse in work logarithmic in its length plus one chunk of CDC
   alignment at each end, producing the same root as streaming the zeros. Start
   in `file/mapping/build.rs`, `file/content.rs` and `file/cdc/gear.rs`; confirm
   the all-zero chunk period before designing. Canonical compatibility must
   hold: prove root equality against streamed zeros.
6. **P3:** assess `EDIT_DEFERRED_LIMIT` (`file/edit/tree.rs:31`, refusal at
   `:358`). It is listed as an S5/content prerequisite and is a hard dependency
   of S10. Either remove the refusal through backed bounded inputs or record
   precisely why it is carried to S10; do not mark S5 complete while silent.
7. Architecture: a new payload document, updates to 19/21/29/30, the core guide
   status line. Then `S5-EXIT-AUDIT.md`, retained logs under `checks/s5-payload/`
   including the failure log, exact LOC, the completion commit and the tracker
   comment and checkbox.

Known gaps to state in the audit rather than hide: stale cells and abandoned
staircase steps are unreclaimed space until S6; read depth equals live
generations and is unbounded under repeated failed captures until S6; the
symlink target still rides on one creation cell; reply-buffer and kernel
custody are S8; `Overlay::cell` / `captured_cell` return raw stored cells
without staleness, and only `source_read` / `captured_read` are the view.

## S6, not started

Deliverable and exits are unchanged from
[HANDOFF-S4-S6.md](HANDOFF-S4-S6.md#first-ready-slice-and-s4s6-exits). S4 and
S5 leave these concrete inputs:

- Rows of removed inodes (`nlink = 0`) are kept so serial access cannot fall
  through to the base. S6 owns their independent orphan custody and reclaim.
- Stale cells, abandoned `shrink` steps and whiteouts over nothing are garbage
  only bounded live reclamation removes. Only terminal reclaim exists today.
- There is no normal failed-capture resolution. `source_name_window` merges
  exactly two layers; the payload read walks every live layer. S6 must bound
  both and keep `born > installed` and the layered cutoff correct.
- Physical reservations, headroom and device-full behavior are untouched.

## Unchanged rules to carry

From [HANDOFF-S4-S6.md](HANDOFF-S4-S6.md): reconcile tree and tracker before
relying on this snapshot; stage named owned paths only; per-commit production
LOC with `tools/production_loc.py` (SHA256 `c0fe7f36…624adb`) on exact parent
and staged-tree archives; the tracker evidence template; append-only receipts;
no retry or replay; no third-party patch; the measurement rules before any
measurement; and the external fuser 0.18.0 timestamp blocker, which still gates
S0/S8/S12 and not S5–S6.

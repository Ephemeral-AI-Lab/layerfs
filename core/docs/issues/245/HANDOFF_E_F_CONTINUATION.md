# #245 handoff: finish E, collect the F candidate arm, close the leftovers

> **Status:** current handoff. Package D, the F comparative target and the route
> harness (blocker 2) are done, committed and verified on this branch. Package E
> is **in flight and unverified** — two modified files sit in the working tree.
> The F candidate arm and every leftover are not started. The two design
> decisions that gate the remaining work (E-1, F-1) are **made and recorded** in
> [HANDOFF_D_E_F.md §8](HANDOFF_D_E_F.md) — proceed on them, do not re-open them.

Copy the assignment below into a new task. This is a **delta** handoff. Read it
with [ARCHITECTURE.md](ARCHITECTURE.md), [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md),
[VERIFICATION_PHASE1.md](VERIFICATION_PHASE1.md), [LOAD_BEARING_CASES.md](LOAD_BEARING_CASES.md),
[evidence/phase1-f-target/TARGET.md](evidence/phase1-f-target/TARGET.md) (the frozen
comparative contract), [evidence/phase1b-mounted-write-repair/REPORT.md](evidence/phase1b-mounted-write-repair/REPORT.md),
and [the extent-sequence pin](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md).
[HANDOFF_D_E_F.md](HANDOFF_D_E_F.md) remains accurate for everything it marks
done, and its §8 decisions are binding.

## Where the branch stands

| Item | Value |
| --- | --- |
| Branch | `codex/issue245-range-cow-plan`, draft [PR #247](https://github.com/Ephemeral-AI-Lab/layerfs/pull/247), worktree `~/.codex/worktrees/issue245-range-cow-plan/layerfs` |
| Head | `036847824` (D) on `0d4834f81` (+4) on `bf9c3f5e0` (docs) on `57cd456a9` |
| Production LOC | 56,506 core / 68,728 legacy / 125,234 combined at HEAD; this round `bf9c3f5e0` +0, `0d4834f81` +4, `036847824` +342 |
| Uncommitted | `core/crates/layerfs-workspace/src/backing/metadata.rs` (`writer_until`), `core/crates/layerfs-workspace/src/commit/reconcile.rs` (E-1 restructure; a brace fix was applied but the compile check was interrupted) — **treat as unfinished work, verify before trusting it** |
| Done this round | F comparative target frozen; blocker 2 route harness up with 8/8 route cases PASS; package D landed with three latent extent-sequence defects fixed |
| Not done | E (finish + verify + commit), F candidate arm, the 5 s `Exec Unknown` verification, #232's 56 shapes, the load-bearing cases, the namespace 128/128/32 KiB ceilings |

## 1. Read before editing

1. `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
   `docs/general/documentation-policy.md`, `core/benchmark/fs-bench-pro/AGENTS.md`,
   `benchmark/fs-bench-pro/QUICKSTART.md`; `docs/general/release-policy.md`
   before any release statement.
2. `core/docs/issues/245/`: this file, then `HANDOFF_D_E_F.md` (its §8 decisions
   bind), `ARCHITECTURE.md`, `IMPLEMENTATION_PLAN.md`, `VERIFICATION_PHASE1.md`,
   `LOAD_BEARING_CASES.md`, `evidence/phase1-f-target/TARGET.md`, then
   `../243/PHASE1_CONTRACT.md` and `../243/TEST_WORKSPACE_AND_COMMANDS.md`.
3. Source, for E: `core/crates/layerfs-workspace/src/commit/{reconcile.rs,completion.rs,operation.rs}`,
   `src/overlay/snapshot.rs`, `src/backing/metadata.rs` (`writer`/`writer_until`),
   `src/runtime/{coherence.rs,lifecycle.rs,state.rs}`; then the gate test sites
   `tests/commit_staged.rs` (`commit_reconcile_failure`, `retained_failure`) and
   `tests/stage.rs` (`stage_semantics`). For D context (already landed):
   `core/crates/layerfs-bridge/src/contract/request.rs`,
   `core/crates/layerfs-server/src/service/save/edit_stream.rs`,
   `core/crates/layerfs-workspace/src/commit/upload.rs`.

## 2. What is established — do not redo it

**Package D is complete and verified** (`036847824`, +342 LOC, 30 files). The
Commit's `EditFile` crosses all four boundaries as one stream: `Begin` declares
the edit count and the replacement total; the packed 24-byte descriptors are the
body stream's prefix and the replacement bytes follow; the server validates the
stream arithmetic, spools the bytes into a bounded resident window (64 KiB) or a
service-side replay file, and hands C1 a replayable `EditSource`. The 256-edit
and 8 MiB replay ceilings are gone (one explicit 4,096 budget shared with C1's
`EditStream`; the 4 GiB `MAX_FILE` kept). Do not re-open the wire format; the
architecture pins were updated in the same commit.

**Three latent extent-sequence defects were found by the harness and fixed**
(two inside `036847824`, one in `0d4834f81`). Do not chase them again:

- the implicit-base fold emitted one over-`MAX_EXTENT` extent, so any
  never-edited file larger than ~16.7 MiB failed its first edit;
- `descend()` read every child page of the branch it visited, so each splice
  paid O(pages) instead of the documented O(H+K) — a 4,096-run frontier cost
  ~53 ms per write before the fix;
- `leaf()` required every base extent's origin to equal its logical position,
  which any insertion violates (the retained tail keeps its own origin), so an
  insert-then-edit sequence returned `Io`.

**Blocker 2 is solved and the harness is up.** Everything lives under
`benchmark-results/fs-bench-pro/issue245-route-harness/` (gitignored scratch;
receipts are referenced by path):

- `binaries/{layerfs-server,layerfs-daemon,examples/public_key}` — **host
  (macOS, release)** builds; the drivers run these on the host.
- `store-master.sqlite` (`prepare_store-host` example), `fixture-02/result.json`
  — the closed 64 MiB fixture, prepared with `prepare_large_edit.py`
  (deterministic content root `22923acce…`).
- Eight route cases, all PASS on the final source, receipts in `*-f/`:
  `write-envelope-f` (9 MiB replacement past the retired ceiling, 1.0 s),
  `write-frontier-f` (512-edit streaming Commit, 20.1 s), `resize-envelope-f`,
  `fresh-stream-replay-f`, `stage-lowering-f`, `stage-semantics-f`,
  `commit-staged-successor-f`, `composite-successor-f`.
- `run_all_final.py` / `run_final_three.py` are the runner patterns (a Python
  runner, never a zsh loop — see §5).

**The F comparative target is frozen** in
[`evidence/phase1-f-target/TARGET.md`](evidence/phase1-f-target/TARGET.md):
functional parity on all four registered cases plus a complete-command wall
≤ 2× the control's walls (mixed-refresh ≤ 4.392 s, overwrite-4k ≤ 1.633 s,
repeated-one-byte ≤ 2.177 s, failed-command-no-commit ≤ 11.764 s), verifier
limit unchanged (9 s), latency cells `INELIGIBLE` in every case. The v3 control
arm is spent; do not re-run it. A candidate needs a **fresh `prepare`** at the
frozen post-E source (the v3 prepared state is invalid once the source changes).

## 3. The remaining work, in order

### E. Finish the generation/reconcile work (decision E-1, binding)

The decided shape (prior handoff §8): narrow the writer gate to the two ordering
points; G2 writes publish during the successor build; a bounded rebuild from the
newer root converges at install; the two short holds wait (deadline-bounded)
instead of `EBUSY`; a post-publication reconcile failure becomes retriable,
never wedging. Preserve frozen-root custody.

State of the working tree: `MetadataHost::writer_until(deadline)` is added, and
`reconcile_commit` is restructured into the two-hold converge loop (ungated
build; only the converged tree sealed and installed; intermediate iterations
stay in the attempt root's temporary chain so the one seal cleans them). **The
last compile check was interrupted**; the first step is `cargo +1.85.1 check
--manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace`, then
the rest:

1. **Retry-unwedge path** in `commit/completion.rs`. A retained submission whose
   attempt has a **known outcome** (`attempt.known` set) and a reconcile-phase
   `KnownCommitLocalFailure` must be resumable by `commit_staged(selector)` on
   the same selector: skip the C5 `CommitStaged` RPC (the token is consumed),
   skip `submission.fund.finish()` (already called), re-validate the known
   outcome, and re-enter `reconcile_commit`. Note the guards that currently
   refuse: `submission.status().failure.is_some()` and the `commit_claimed`
   CAS. Unknown outcomes (`lost_result`, `consumed_stage`, `denied`,
   `head_moved`) keep the retained wedge — custody, not a bug.
2. **Convert the mounted mutation/read acquisitions** to `writer_until(deadline)`:
   `filesystem/write.rs` (`publish_file_mutation`, the `host.writer()?` at the
   publication) and `filesystem/read.rs` (the read-path acquisition). The
   reconcile's two short holds must never surface as `EBUSY` to a shell command
   — that is the rejected alternative's exact failure mode. Leave the internal
   short read holds (`_view = host.writer()?`) as they are.
3. **Update `tests/commit_staged.rs::commit_reconcile_failure`**: after the
   injected failure, the retained state assertions stay, then clear the fault
   and prove the retry succeeds (installed revision, canonical root, exact
   bytes, old reply intact), and `close_clean` works afterwards. Keep
   `retained_failure` for the unknown-outcome cases unchanged.
4. **Add the E gate test** (route-gated, `#[ignore]`, run through the harness):
   the mounted three-generation observation — write version A, start Commit A
   and cross its capture point, write B into G2 during the build, verify B1 is
   exactly A while the live view is A+B, then start sequential Commit B while a
   racing writer thread keeps appending (no unexplained `Busy`; every accepted
   write appears exactly once, in order, in B2), and verify B2, live G3 and both
   older heads through the native reader. Add its case to the appropriate
   `*_route.py` (`commit_staged_route.py` is the natural home) and run it.
5. Commit with the per-commit production LOC comparison; update any architecture
   pin whose concurrency description changes, in the same commit.

### F. The candidate arm

1. Commit E first: `prepare` refuses a dirty tree, and the prepared identity
   must match the frozen candidate source.
2. `python3 core/benchmark/fs-bench-pro/shell_package.py prepare --output
   benchmark-results/fs-bench-pro/<fresh-prepared-dir>` — one preparation,
   outside any measured window; record its seal.
3. `python3 core/benchmark/fs-bench-pro/shell_package.py run --prepared
   <prepared.json> --output benchmark-results/fs-bench-pro/<fresh-campaign-dir>`
   — **one candidate attempt per case**, same enforced cache contract, one
   construction worker, `shutil.copyfile` clone as before.
4. Report every registered cell against the frozen target: functional/cleanup/
   verifier status, complete-command wall per case versus its 2× envelope
   (4.392 / 1.633 / 2.177 / 11.764 s), latency cells `INELIGIBLE`, an envelope
   miss `FAIL` with the measured wall, an unattempted case `NOT_RUN` with its
   reason. No best-of, no re-run, no dropped cell.

### Leftovers, if the lane allows

- **The 5 s `Exec Unknown` verification.** Now unblocked by D: run one mounted
  historical shift shape (e.g. a 10 MiB prepend or middle shift through
  `/bin/sh`) and record whether the silent `Unknown` still exists, with the
  receipt. Do not raise the progress deadline to make it pass.
- **#232's 56 shapes** — the parent gate remains open; a separate committed
  amendment must select their exact commands and oracles before any run.
- **The load-bearing cases** ([LOAD_BEARING_CASES.md](LOAD_BEARING_CASES.md)) —
  research only; promote a small named selection only after the above.
- **The namespace 128-dirty / 128-name / 32 KiB ceilings** — a separate
  namespace-lane package; do not fold it into E or F.

## 4. Budgets and gates (unchanged, restated)

- Complete command ≤ 15 s (mixed refresh's declared 25 s exception); verifier
  < 10 s; route-driver hard budget 60 s per case.
- One sample/attempt per case per arm; the control arm is spent.
- One construction worker; `LAYERFS_CONSTRUCTION_WORKERS=1`.
- The exact 4,096-write end-to-end frontier shape does **not** fit the 60 s
  route budget (~2 min extrapolated; the per-publication arena cost — one
  custody page per payload plus ownership edges — is the C-layer's, not the
  transport's). That is a recorded decision, not a gap: the route case pins a
  512-edit streaming Commit past the retired 256 ceiling, and the exact budget
  boundary is pinned by the focused in-memory suite. Do not "fix" it by raising
  the driver budget or shrinking the product work.

## 5. Operational playbook (this round's mechanics, verified)

- **Release binaries only** (owner direction, 2026-09-26): build every host and
  musl binary and test binary with `--release`. Never hand a debug binary to a
  route driver or a measurement.
- **Host binaries** (the drivers run these on macOS):
  `cargo +1.85.1 build --release --manifest-path core/Cargo.toml --locked
  --offline -p layerfs-server -p layerfs-daemon --bins` and
  `-p layerfs-bridge --example public_key`, then copy into
  `…/issue245-route-harness/binaries/{,examples/}`. A `-p X --bins` invocation
  is needed for the server/daemon binaries; `--example` alone narrows the
  selection and silently skips them.
- **Musl test binaries**: `cargo +1.85.1 zigbuild --release --manifest-path
  core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p
  layerfs-workspace --test <suite>`; run them in
  `docker run --rm -v "$PWD/core/target/aarch64-unknown-linux-musl/release/deps:/d:ro"
  alpine:3.22 /d/<suite>-<hash> --test-threads=1`. Filter `.d` files when
  picking the newest binary by glob.
- **Route drivers**: `stage_route.py` (and the wrappers `write_route.py`,
  `resize_route.py`, `commit_staged_route.py`, `composite_route.py`) take
  `--fixture --binaries --test-binary --output --case`; the `mkdir`-family
  wrappers (`fresh_stream_route.py`) additionally require `--image
  rust:1.85.1-bookworm`. Drive them from a Python runner file, never a zsh
  `for … set -- $spec` loop (zsh does not word-split there; an empty positional
  once turned a cleanup into `rm -rf <harness dir>` and destroyed scratch
  receipts — never interpolate possibly-empty variables into `rm`).
- **Fixture**: re-prepare with `prepare_large_edit.py --store-master
  <store-master.sqlite> --binaries <binaries> --producer-source "$(git rev-parse
  HEAD)" --producer-seal <product_inputs sha> --output <fresh dir>`; its 60 s
  hard budget is ample (≈2 s). `product_inputs()` comes from
  `tests/payload_route.py`. The content root is deterministic
  (`22923acceef8a0f5…`); the store master is incidental.
  This driver was written against the retired pathless initialization and is
  kept at
  [`a72d07ef81794f1a7224eb721c8673e503cfcb33`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/a72d07ef81794f1a7224eb721c8673e503cfcb33/core/crates/layerfs-workspace/tests/prepare_large_edit.py);
  the current tree initializes its namespace over `ImportNativeDirectory`
  instead.
- **Docker**: `rust:1.85.1-bookworm` vanished once mid-session (re-pull it);
  remove stray containers and volumes when a job dies; `alpine:3.22` is enough
  for musl test binaries.
- **Shell tooling**: this host's zsh has no `timeout` — use
  `perl -e 'alarm shift; exec @ARGV' <secs> <cmd>`; the exec tool rejects
  commands containing `&`-bearing heredocs, so write patch scripts to files and
  run them.
- **Formatting and size**: `cargo fmt` can push a file past the 999-line
  ceiling — always run `python3 core/tools/check_product_boundary.py` after
  formatting, and split by responsibility before editing a near-ceiling file.
- **Per-commit LOC**: `git archive <parent> | tar -x -C <dir>` for before,
  `git checkout-index -a --prefix=<dir>/` for after, then `python3
  core/tools/production_loc.py --root <dir> --json`; re-verify against the
  committed tree. Docs-only commits report delta 0.

## 6. Discipline that is not negotiable

- **Route:** `/bin/sh -c` in the mounted Workspace; the kernel's syscalls decide
  the callbacks. No edit tool, no range ioctl, no command-text classifier, no
  direct Store mutation.
- **Sampling:** one attempt per case per arm; the control arm is spent; no
  best-of, no unchanged-arm rerun, no timeout/worker increase, no shortened
  case, no dropped cell, no rewritten receipt.
- **Cache:** a timed phase pays for its own work from a declared state; the
  frozen selection's latency cells are `INELIGIBLE` by contract.
- **Evidence:** append-only paths with identities and raw output; `UNAVAILABLE`
  for anything not measured.
- Do not run the retired root preflight; no CI claims; builds `--locked`; no new
  dependencies; no patched/vendored crates; aarch64 AEAD flags come from the
  repository-root `.cargo/config.toml`.
- `inode.edits == u16::MAX` is the unknown-count marker: a version whose splice
  shared a subtree is lowered, never base-reused — preserve this in E.
- Changed wire contract, algorithm or bound ⇒ update the affected
  `core/docs/architecture/` pin **in the same commit**.

## 7. Final reporting

At the frozen final source run once: `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked --all-targets`, `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 fmt
--manifest-path core/Cargo.toml --all --check`, `python3
core/tools/check_product_boundary.py`, `python3 -m unittest discover -s
core/tools -p 'test_*.py'`; plus the musl run of every workspace suite and the
route cases. Report: per-commit production LOC before/after/delta with method;
every registered cell as `PASS`/`FAIL`/`INELIGIBLE`/`NOT_RUN` with receipt
paths; the exact reproduction commands; the updated architecture pins; residual
risk and every open gate (#232's shapes, load-bearing cases, namespace
ceilings, the `Exec Unknown` verification if still open).

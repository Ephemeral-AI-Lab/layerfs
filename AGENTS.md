# AGENTS.md

Repo-wide rules for coding agents working in `layerfs`. This file routes; it does
not replace the normative documents below, and where they disagree with this page,
they win.

For the replacement product under `core/`, also read
[`core/AGENTS.md`](core/AGENTS.md). It defines product-only source, external tests,
the 999-physical-line production-file ceiling, and the stricter 200-line
declaration/delegation limit for `lib.rs` and `mod.rs`. These
rules apply to the replacement tree; existing root `crates/` remains reference
code during migration. Follow the core-specific checks before claiming core work
is verified; the root checks alone do not exercise that workspace.

Read before touching measurement, benchmark or release work:

- [`docs/general/benchmark_rules.md`](docs/general/benchmark_rules.md) — the measurement contract
- [`benchmark/AGENTS.md`](benchmark/AGENTS.md) — benchmark-tree specifics and the v0.1.6 exception
- [`benchmark/fs-bench-pro/QUICKSTART.md`](benchmark/fs-bench-pro/QUICKSTART.md) — build, reuse and run mechanics
- [`docs/general/release-policy.md`](docs/general/release-policy.md), [`docs/general/documentation-policy.md`](docs/general/documentation-policy.md)

## 1. A warm cache must never credit a measured phase

The harness runs timers; the OS, the Store and the page cache are not part of the
product's work. Two rules follow, and they are absolute:

**A measured phase pays for its own work from a declared cache state.** If the
machine had just booted and no page cache held this process's data, would the
work still have to happen inside the timed phase? If yes, the phase must pay for
it — every run, in every arm.

Forbidden, without exception:

- letting pages left resident by setup, preparation, an earlier sample, another
  arm or another phase serve a timed phase (a transfer that reads its own recent
  writes out of cache is the canonical case: it measured 19 GB/s instead of the
  2.1 GiB/s the same bytes cost from storage);
- pre-touching, priming, warming or "just checking" the paths a timed phase will
  read, or priming selected ranges with expected-result data;
- measuring arm A warm and arm B cold, or invalidating caches for one arm only —
  cache state MUST be declared and enforced equally, and cold and warm rows MUST
  NOT be pooled;
- re-running a case until a warm variant produces a passing number and reporting
  only that run;
- quoting a lifetime counter as a phase number (cgroup `memory.peak` is a
  lifetime total unless it was verifiably reset after setup);
- excusing file-size-proportional spool or cgroup page-cache growth because the
  process heap is bounded.
- inflating a timeout, changing worker counts, or relaxing a cache/buffer policy
  to turn a miss into a pass;
- dropping a failing cell, or omitting a registered selection, from a report;
- rewriting, re-labelling or promoting a historical receipt after the fact.

Allowed, and expected: untimed, deterministic, recorded preconditioning;
preparing pristine fixtures once outside the measured child; using the harness's
own cold contract — for the families it covers, `shared/cold.py` invalidates
source data pages before the timed sample and checks whole-input residency, so a
row with resident pages is reported `INELIGIBLE` rather than quietly fast.

If a phase's cache state is undeclared, unknown, or different between arms, its
number is `INCOMPLETE` or `INELIGIBLE` — never `PASS`. Say so in the receipt and
in the report.

## 2. Reuse setup (`--setup clone`); never reuse measurement

Agents MUST avoid repeating setup, and MUST NOT let that reuse reach inside a
timed phase. The test is where the saved work lives: reuse that removes work
**outside** the timers is required; reuse that removes work **inside** a timed
phase is cheating.

- **Fixtures: use `--setup clone`, not a fresh regeneration, for every
  post-initialization case.** Clone takes the closed, validated prepared master
  and gives the run an independent writable byte copy, so no sample pays the
  preparation again. `--setup fresh` is for initialization and fresh-output cases
  only — the harness rejects `clone` there. Do not run a family's `setup.sh`
  before every sample, do not clear protected caches routinely, and never reuse a
  mutated sample.
- **A clone is setup reuse, never a cold claim.** Clone means a closed,
  validated, independent writable byte copy — not an APFS clone and not a
  cold-OS-cache claim. Declare the clone/copy method with the row, treat ordinary
  OS-cache effects consistently, never pool clone and fresh rows, and never let
  the master's or the clone's warmed pages credit a timed phase. A family that
  needs a cold claim needs the cold contract's invalidation-plus-residency check,
  not a clone.
- **Verification: `--reuse-pass <verification.json>`** accepts one
  identity-matched `status=PASS`, cleanup-`PASS` receipt instead of re-running
  verification. It fails closed on any schema, identity, hard-limit or wall
  mismatch and records `reused_proof_identities` plus an explicit omission.
- **Builds and images: reuse through seals.** Incremental host builds, the shared
  Cargo target, image layers keyed by the compilation seal, and immutable
  `binary-archive/<sha256>/` executables (a host-only Python/shell change may
  reuse an image whose compilation seal still matches; `--prune-builds` retains
  owned targets).

Never: warm starts, replaying a previous receipt as a new sample, moving cold
product work into setup, priming the paths a timed phase will read, or treating a
cached acquisition or a clone as evidence about the measured operation. Anything
reused MUST be visible in the receipt (`clone_method`, `build_mode`,
`dependency_reuse`, `reused_proof_identities`, `cache_contract`) and stated in
the report.

## 3. Running a measurement

**Current Core SDK Init profile:** `core/benchmark/fs-bench-pro` measures
`init_namespace` with Cargo's default **debug** build only (`--locked`, no
`--release` or optimization override). Use its `target/debug/examples/`
driver and verifier. A release-build diagnostic remains historical context,
not a replacement sample or a speed arm for this SDK selection. Never infer a
regression by comparing this debug route with an older release-build
daemon-host row. The SDK selection's mandatory separate verifier is a scoped
exception to the exploratory performance-only default below; see
[`core/benchmark/fs-bench-pro/AGENTS.md`](core/benchmark/fs-bench-pro/AGENTS.md).

1. **One sample per case per arm, and do not sample.** No n3, no best-of
   selection, and no second run of an arm to confirm stability, to characterise
   spread, or to replace a number that came out inconveniently. **Sampling is not
   the method here and it is not free.** A row's spread is a property of the
   machine and the window rather than of the code — the campaign's own record has
   the same executable reading 16.7 s, 17.6 s and 26.0 s in three windows, and
   this lane's own row carries a teardown term that swings 6.7-469.9 ms on
   identical source — so repeating an arm buys a wider distribution, not a truer
   number, while costing the wall time the work itself needed.

   An anomaly is therefore diagnosed **from the receipts already taken**, or with
   a **labelled diagnostic that measures the cause**: a count-driven instrument
   (statements issued, calls made, bytes written, microseconds per call) that is
   reproducible across rows, never another sample of the same arm. Diagnostics
   are allowed and MUST be labelled as diagnostics and reported alongside the gate
   sample. The one carve-out is the #118 material-regression rule for ordinary
   regression screens (`docs/general/benchmark_rules.md`), which is a different
   activity — screening a tree for a slowdown by median of prospectively declared
   pairs — and is not a licence to repeat a treatment arm.
2. Fresh `--output` path per run; receipts are append-only evidence and are never
   overwritten. Failures, `INELIGIBLE` rows and discarded attempts stay on disk.
3. Pin identities: source commit/seal/tree, product, compilation and dependency
   seals, image ID, harness identity, workload-source hash. A rebuilt artifact
   needs a rebuilt matched arm; a harness change invalidates the pair.
4. **Default exploratory benchmarks to performance only.** Run the selected
   case through the fast lane without full verification; record `SKIPPED` and
   keep the row diagnostic. Verification wall is separate from the performance
   timer and never enters a speed comparison. During an experiment, record a
   verifier defect or timeout and keep working on the measured mechanism; fix
   it then only if it prevents the performance run, corrupts its evidence, or
   blocks a proof the current decision actually requires. A verifier that misses
   its bound cannot turn the row into an admission PASS.

   At a frozen final source identity, verify separately with the exact
   identities from the performance receipt. A performance PASS alone is not
   release admission. **Verify once, with the commands that cover the change;
   do not verify or test iteratively.** A red test is diagnosed from its output
   and the source, the fix is applied once, and the covering commands then run
   once. Do not rerun an unchanged performance arm to select a better number.
5. Respect the measurement lock — it is **per worktree** (owner direction,
   2026-09-21): builds and measurements in different worktrees do not exclude each
   other, two runs in one worktree still never overlap, and no build may take a
   Cargo target directory outside its own worktree. A build that overlaps a timed
   phase is recorded as declared interference on the row rather than prevented;
   see [`measurement-isolation.md`](docs/roadmap/0.1/0.1.7/measurement-isolation.md).
   Never interrupt another owner's run.
6. Record it: append an entry to the active ledger
   (`docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md` and its
   successors) with exact numbers, limits, the arithmetic, the identities, the
   reproduction command, and every non-passing line. Report FAIL, INCOMPLETE and
   unrun work as plainly as PASS.
7. **Fit the budgets.** Preparation is fast and reusable — prepared inputs are
   acquired once and reused with identity checks, and repeated setup before a
   sample is forbidden. A performance selection's **complete command** (product
   timer + container lifecycle + cleanup) is **≤ 15 s**, with a small, declared
   exception list allowed up to **25 s** (declare it in the group report with the
   measured wall time; no sign-off blocks the run). **Verification is small:
   under 10 s, and typically a fraction of a second** — a row that spends more
   than that on verification is spending it on something other than the question,
   and there is no 60 s allowance to grow into. For scale: the
   `pipeline-namespace-10000` row's complete command is **3.1-4.8 s** (a 300 MB
   namespace written into a 302 MB Store, one sample) with verification at
   **0.33-0.36 s**, so a case of that shape has no reason to approach the limit.
   A selection that cannot fit is
   reused from a qualifying receipt with its evidence cited, or recorded as `NOT_RUN`
   with the measured wall time and the reason — never made to fit by moving work
   outside the timer, enlarging a timeout, or shrinking the workload.
8. **One construction worker — for every case except namespace init.** Commit,
   capture and snapshot run with a single worker, in the default wiring and not only
   by environment variable: `construction_worker_limit()`
   (`crates/layerfs-workspace/src/changes.rs`, today
   `available_parallelism().min(8)`) and the canonical construction it feeds
   (`objects::construct_files`, today capped at `SMALL_CONTENT_WORKERS = 4`) must both
   be single-producer. Every run also exports `LAYERFS_CONSTRUCTION_WORKERS=1`, no run
   raises it, and no second lane or helper worker is added to pass a gate.
   **`init_namespace` is the only exception:** its initialization path
   (`LayerStackStore::initialize_layerstack` →
   `direct_initialize_root_directories_inner` / `prepare_parallel_root_directories`)
   legitimately uses multiple workers/threads and keeps its 2.7 s cold Init target — do
   not collapse it to one. **A performance drop against v0.1.5 is expected** for the
   single-worker cases and is absorbed by the bounded acceptance rule, never by adding
   workers back.

## 4. Code, build and docs

- New replacement-product and future application-adapter production files follow
  the 999-physical-line ceiling and the stricter 200-line declaration/delegation
  limit for lib.rs/mod.rs. Keep product-only source and external tests. Existing
  root crates/ remain reference; extend guard coverage when new product formats
  or adapter paths are introduced rather than using them to evade the rules.
- **This repository runs no CI and no aggregate pre-push gate** (owner decisions:
  GitHub Actions is disabled and `.github/workflows/ci.yml` removed — ledger L21 —
  and `tools/preflight.sh` is **permanently retired** — ledger L32). Do not run
  `tools/preflight.sh`, do not restore it, and do not reintroduce an equivalent
  aggregate gate, workflow or wrapper. During the architecture shift it costs minutes
  and verifies a tree that is no longer the deliverable.
  CI being off is not permission to skip verification. Verify the tree you actually
  changed, per workspace, with the commands that cover it — for the replacement
  product that is `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml
  --locked` plus `core/tools/check_product_boundary.py` — and report exactly which
  checks ran, which did not, and why. No push may claim "CI green" or "the preflight
  passed".
- Keep the tree clean for sealed builds — a dirty source seal is recorded and cannot
  be compared against a sealed arm.
- No new dependencies when an existing crate already provides the capability;
  keep platform-specific code `cfg`-gated. In the replacement product, unsupported
  required capabilities fail explicitly; no silent no-op or error-driven fallback.
- **Never patch, vendor, fork or locally modify a third-party crate or package.**
  No `[patch]`/`[replace]` sections, no vendored copies, no edits in the Cargo
  registry or under `~/.cargo`, no forked dependency substituted for a published
  one. Builds stay `--locked`. If a dependency appears to need a change, stop and
  report the blocker with evidence instead of satisfying it locally.
- **aarch64 has exactly one AEAD profile, and it is a build input.** The native
  transport negotiates AES-GCM on the ARMv8 crypto extension; that requires the
  `aes_armv8`/`polyval_armv8` cfgs and the `+aes,+sha2` target features, which only
  a global flag can set. The repository-root `.cargo/config.toml` supplies them for
  every build made from inside this repository — it is at the root, not under
  `core/`, because cargo discovers config by walking up from the *current working
  directory* and this repository's prescribed commands run from the root with
  `--manifest-path core/Cargo.toml`. A build made from outside the repository must
  pass the flags explicitly (an explicit `RUSTFLAGS` overrides the config table, so
  repeat all four); `core/crates/layerfs-bridge` refuses to compile for aarch64
  without them instead of silently negotiating the 2–4x slower ChaCha20-Poly1305
  fallback (measured 214 vs 802 MiB/s on one stream, 425 vs 1556 MiB/s on two).
  Any identity set that pins build flags must record the repository-root
  `.cargo/config.toml` (the transport-probe manifests used to name
  `core/.cargo/config.toml`, which no longer exists). Do not "fix" a slowdown here
  by disabling that refusal or by patching the crates.
- Never claim durability the contract does not provide: no `fsync`/`fdatasync`/
  `sync_data`/`sync_all` on Workspace backing, and memory hints are hints.
- Documentation states measured facts, limits and open rulings; roadmap READMEs
  link to the ledger rather than paraphrasing numbers.

### Production LOC comparison for every commit

Every Git commit must record **production source lines of code before, after,
and the signed delta**. Test, documentation and tooling changes do not contribute
to this number. This is a source-size comparison, not a performance claim.

- **Count production code only.** Count nonblank, non-comment source lines in
  first-party LayerFS product implementation, including required runtime SQL or
  other shipped implementation outside Rust src/ directories. Imports, declarations
  and forwarding code count. Exclude tests (including legacy inline test modules
  and test-only branches), fixtures, mocks, examples, benchmark harnesses, development
  tools, docs, manifests/lockfiles, third-party code and generated build artifacts.
  A line with both code and a comment counts once. Do not substitute raw file-line
  totals or Git insertion/deletion statistics for production LOC.
- **Compare the exact commit snapshots.** Before is the commit's first parent;
  after is the committed tree. Prepare the comparison from the parent and final
  staged tree before committing, then confirm the resulting commit matches it.
  Exclude unstaged/untracked work. Use an empty tree for an initial commit. For
  merges declare the first-parent comparison; recompute after amendments/rebases
  or any change to the staged source.
- **Keep counting reproducible.** Use the same counter/version, source scope,
  exclusions and handling of inline test code for both snapshots. Record the
  command/method with the comparison. Review source classification when files
  move or new product paths appear; do not silently drop code from the count.
  A counter that includes legacy inline tests does not satisfy this rule.
- **Report migration honestly.** While old and replacement implementations
  coexist, report their production totals separately as well as the combined
  total. Include application-adapter production code when introduced. Label
  relocation, duplication and legacy retirement; do not call a scope change or
  deletion of the reference an algorithmic simplification.
- **Put the result in the commit message and handoff.** Use
  `Production LOC: <before> -> <after> (delta <signed difference>)`, with scope
  and counting method, plus migration subtotals when applicable. For multiple
  commits, give a comparison for each. A test/docs-only commit still reports
  the unchanged production total and delta 0; it does not report a fictitious
  zero-sized product or add test/documentation LOC to the headline.

LOC growth is allowed when justified by the product change; this rule does not
require every commit to shrink. Never remove required validation, compress code
into dense lines, or move implementation outside the declared scope to improve
the number. Complete the comparison before committing; do not invent estimates.

## 5. Why these rules exist (worked example)

`#151`'s B2 case wrote a 500 MiB payload through FUSE into the sandbox, which
then read it back during the measured Commit. Because the workload's own writes
had left that payload resident, the transfer looked like 0.026 s (19 GB/s) and the
sandbox's memory looked like the payload itself (563 MB container peak, of which
524 MB was `file` cache and 4.6 MB anonymous). Both readings violated this page:
one was cache-credited, the other was page-cache growth proportional to file size.
After the spool was given a bounded resident window, the sandbox's own residency
fell to ≤ 2.6 MiB and the same transfer cost a storage read — the honest price,
which then showed up as a Commit-phase FAIL against a limit derived from a
cache-served control. Six identical runs also produced container lifetime peaks
from 24.6 MB to 189.8 MB, which is why a lifetime cgroup number cannot decide a
memory gate. See
[`issue151-experiment-ledger.md`](docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md)
L18.

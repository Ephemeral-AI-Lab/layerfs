# Handoff prompt: #235 substrate, then #231 Init first pass

Copy this prompt into the implementation agent's task. The specification is a
working-tree proposal until committed; this prompt is not a performance receipt.

## Task

Implement [#235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235)'s
minimal `core/` fs-bench-pro substrate, then make
[#231](https://github.com/Ephemeral-AI-Lab/layerfs/issues/231)'s public native
Init first pass runnable. Measure the 100, 1,000 and 10,000-file cases once each
at their exact final identities, with full independent proof. Keep the
100,000-file row `NOT_RUN`. This is a fast development loop and a discovery
cohort, not a release or latency PASS claim.

Other agents may work in other worktrees. Preserve their edits and evidence.
Use a dedicated worktree and private Cargo target, fixtures, Stores, scratch and
results. The current checkout may contain uncommitted proposal docs and
unrelated changes; inspect it before copying files or committing. Commit the
frozen spec and relevant docs before implementing the driver or collecting a
sample, with the required before/after production LOC count in every commit.

## Read before changing code

1. Repository [`AGENTS.md`](../../../../../AGENTS.md),
   [`core/AGENTS.md`](../../../../AGENTS.md), and the new harness
   [`AGENTS.md`](../../../../benchmark/fs-bench-pro/AGENTS.md).
2. [`SPEC.md`](SPEC.md), the
   [pipeline/route map](../pipeline-and-modes.md),
   [parameters and stats](../parameters-and-telemetry.md),
   [telemetry ingestion](../telemetry-ingestion-and-retention.md), and
   [cache discipline](../preparation-and-cache.md).
3. The [general benchmark rules](../../../../../docs/general/benchmark_rules.md),
   [worktree isolation decision](../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md),
   legacy [`benchmark/AGENTS.md`](../../../../../benchmark/AGENTS.md),
   legacy [QUICKSTART](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
   and the release/documentation policies linked by repository `AGENTS.md`.
4. GitHub [#230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230),
   [#235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235),
   [#231](https://github.com/Ephemeral-AI-Lab/layerfs/issues/231), and closed
   [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179). Check their
   current state rather than relying on this prompt's date.
5. The real daemon route in
   [`history_route.py`](../../../../../core/crates/layerfs-daemon/tests/history_route.py),
   the small bootstrap tests in
   [`history.rs`](../../../../../core/crates/layerfs-service/tests/history.rs),
   and the `LFT1 ` encoder in
   [`encode.rs`](../../../../../core/crates/layerfs-telemetry/src/output/encode.rs).
   For workload semantics, inspect the legacy
   [Init registry](../../../../../benchmark/fs-bench-pro/families/init_namespace/mod.rs),
   [Init performance and verifier](../../../../../benchmark/fs-bench-pro/src/main.rs),
   and [cold helper](../../../../../benchmark/fs-bench-pro/shared/cold.py).

## Do, in order

1. Confirm the public operation and timer boundary. The former 128-entry
   `InitLayerStack` with pre-saved roots is insufficient. Define and implement
   a bounded public native-directory import through the real daemon, Service,
   C1, C2 and C5 path if it is still missing. Source scan and file reads must
   stay inside the measured Init operation. Add product tests and update
   affected architecture docs for any product API/format change.
2. Build the smallest #235 harness around existing production binaries and
   `core/Cargo.lock`: `list`, `run`, `verify`, `report`. `run --case` prepares only
   the selected case; `run --family init_namespace` shares one build across the
   three selected cases. Use a single registered `daemon-host` route.
3. Put the family declarations, public call and full verifier child in
   `core/benchmark/fs-bench-pro/families/init_namespace.py`. Add focused
   case/fixture/corruption-refusal checks in
   `core/benchmark/fs-bench-pro/tests/test_init_namespace.py` and minimal
   isolation, seal, telemetry and receipt checks in `tests/test_substrate.py`.
   The full benchmark proof is the separate verifier process and its
   `verification.json`, not a unit-test result. Reopen the persisted output and
   check every path, metadata item, byte count and file SHA-256 against a
   once-sealed source manifest through public product read APIs.
4. Give each worktree its own mutable target and result namespace. Use no
   machine-global benchmark lock; keep only a nonblocking local run `flock` for
   same-worktree mutation. Builds take no benchmark lock. Use fresh output,
   Store, run IDs, ports and container names. Reject foreign target/output
   paths and retain observed competing-work evidence.
5. Parse exact `LFT1 ` JSON lines from separate daemon/Service stderr streams.
   Bind run, role, namespace and PID; validate expected events, timing trees,
   loss and truncation. Keep one `telemetry.lft1` and parsed receipt per case;
   remove only validated, owned temporary captures. Keep original stderr on
   ingestion failure. Preserve process clocks and M1–M4 headings separately.
6. Run the real #179 public-operation canary for #235, then the three Init
   cases once each at their final source/fixture identities. Retain failures,
   unknown-cache status and 100,000 `NOT_RUN`. A source-cache-uncontrolled row
   is diagnostic and admission-ineligible. Keep raw timing, verification,
   resource, telemetry, cleanup and complete-command evidence.
7. Record cold, changed-product and no-op Cargo build walls. Every invoked
   `cargo build --locked`, including first-use, must be <=30 s; each complete
   full verifier <=5 s; the three-case preparation/run/proof/cleanup family
   cycle is recommended <=30 s. Preserve the 15 s complete-performance-command
   budget and only prospectively declared small exceptions <=25 s. Fix a miss
   where possible; otherwise retain the precise `BUILD_SLOW`, timeout or
   `NOT_RUN` blocker. Run focused checks after edits and the required core
   workspace checks once at the final identity.
8. Report exact commands, identities, raw one-sample numbers, each budget and
   status, evidence paths, cache/interference limits, checks run and skipped,
   and production LOC before/after/delta per commit. Do not claim the #231 final
   four-tier gate or broader #230 migration is complete.

## Do not

- Do not add control/candidate arms, a paired scheduler, repeated performance
  samples, averages, a second Cargo workspace/lockfile or unused mode adapters.
- Do not use the bootstrap, pre-save file roots, issue thousands of mounted
  creates, move source reads outside Init, or accept a sampled verification
  oracle as native Init evidence.
- Do not run or silently omit the 100,000-file tier in this first pass, shrink
  the selected workloads, relax timeouts/worker counts, warm measured reads or
  rerun a failed cell to select a nicer number.
- Do not restore a machine-global lock, share writable targets/outputs across
  worktrees, reuse mutable Docker tags as identity, or copy the legacy global
  BuildKit cache/pruner behavior into the new harness.
- Do not overwrite receipts, delete failed evidence, invent missing telemetry
  spans, add durations across independent clocks, or call sampled RSS an exact
  phase peak.
- Do not migrate #232/#233 or later families as part of this handoff. Do not
  run the retired aggregate preflight, add CI, or patch/vendor dependencies.

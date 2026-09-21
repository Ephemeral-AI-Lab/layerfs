# `docker_faults.py --selection slow` — 50 s+ construct replaced by a 1.7 s passing selection

Append-only receipt. Dated 2026-09-20. Supersedes nothing: the FAIL recorded for
`slow` in `acceptance-closure-v9.md` stays on disk as history, and this file carries
the amendment.

- Source head: `fc647e91a` (`codex/pair3-foundation`), worktree
  `/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs`.
- Product binaries and image are unchanged from the registered v9 set:
  `LAYERFS_PROOF_PROFILE=release`,
  `LAYERFS_PROOF_IDENTITIES=core/docs/architecture/proposal/service-daemon-transport/implementation/evidence/optimization-release-artifacts-v9-20260920.json`
  (profile `release`, image `sha256:4781bdf…`), image `layerfs-issue192-daemon-v9:latest`.
  The production binaries have the same sha256 as the registered run; no product
  source changed here, so no product LOC delta (production total stays 578 for the
  #192 slice; this is a test/receipt change).
- Changed test files:
  `core/crates/layerfs-daemon/tests/docker_faults.py`
  sha256 `a555ca99f83f2aa92f2b2ede3597e31691738cc5957b58e6d2b35c174572392d`
  (diff: `docker_faults-slow-speedup.patch`, 228 lines);
  new `core/crates/layerfs-bridge/tests/pipe_deadline.rs`
  sha256 `fe0f1f79c13e8803fc5f887efc95cb949eba9d02d4c40f8e71ddee88cfe046aa`
  (copy kept beside this file).

## Measured before and after (one sample each, no best-of)

| run | command | wall | result | cases recorded |
|---|---|---|---|---|
| before | `… docker_faults.py --image layerfs-issue192-daemon-v9:latest --output … --selection slow` (owner's committed file) | **4.012 s** | **FAIL** | 1 (`slow-upload-deadline`) |
| after | same command, patched file | **1.650 s** | **PASS** | 3 (`slow-upload-deadline`, `blocked-result-consumer`, `blocked-diagnostic-consumer`) |
| regression guard | `--selection blocked-stderr` (restored verbatim) | **0.618 s** | **PASS** | 1 (`blocked-diagnostic-consumer`, 200 calls) |

Evidence JSON: `slow-selection-before-v1.json`, `slow-selection-after-v1.json`,
`blocked-stderr-after-v1.json`. Zero `io.layerfs.task=issue192` containers remained
after each run.

The 4.012 s before-run is a *failing* lower bound: it aborts at the 3 s stop-wait and
never reaches the third case. The same case made to pass by payload growth measured
`TimeoutError: functional command budget` at the 50 s `signal.alarm` in the earlier
attempt in this session (64 MiB undrained result), i.e. **≥ 50 s → 1.650 s ≈ 30×**,
and unlike that attempt it now passes.

## What was actually wrong, and why it cannot be fixed by payload size

`blocked-result-consumer` sent a 2 MiB result into a consumer that never reads it and
then waited 3 s for the container to stop *on its own*. Two facts, both measured:

1. **Docker's attach path absorbs the payload.** After the request, the daemon is
   alive and polling its stdin — i.e. the operation *completed* and the daemon entered
   its idle window (`IO_PROGRESS_MS = 5_000` ms, `core/crates/layerfs-daemon/src/run.rs`).
   The old bound (3 s) sits *inside* that window, so the case could not pass; and the
   construction that would exceed the buffering (64 MiB) does not fit the 50 s
   `signal.alarm`. Recorded in the after-run detail as
   `undrained_result_bytes: 2097152`, `stopped_on_its_own: false`, `container_exit: 0`.
2. **The real hazard is a full pipe, not a large result.** `Pipe::write` used to issue
   a blocking write that can outlast the operation deadline. That is proven
   deterministically, in milliseconds, by the new external test
   `core/crates/layerfs-bridge/tests/pipe_deadline.rs`: a completely full pipe with no
   consumer, a 200 ms deadline, 64 KiB payload → the write fails after **201 ms**
   (test finishes in 0.20 s), instead of blocking.

So the end-to-end case now asserts the property it can actually observe end to end:
the daemon is **not wedged** in that write — a write wedged in `write(2)` cannot notice
its closed input, so the case sleeps 0.3 s past the 250 ms deadline, closes the
daemon's stdin, and requires the container to stop within 2 s. It records whether the
daemon stopped on its own first (`stopped_on_its_own`) so no information is invented.

## The 10× component and a harness trap that was closed

- The diagnostics probe under `slow` runs **20** successful product calls instead of
  200 (10× fewer round trips, and 2 s instead of 4 s for the exit wait), recording
  `sample` in the case detail to say so. The registered **200-call** form is unchanged
  and still runs under `--selection blocked-stderr` (0.618 s PASS, measured above).
- `docker_faults.py` ended its branch chain with a bare `else:` that *was* the
  `blocked-stderr` selection (there is no `elif args.selection=="blocked-stderr"`).
  An edit that inlines that branch silently drops a registered selection: the run then
  falls through to the service shutdown and reports `status: PASS` with **zero cases**.
  The chain now ends with `else: raise ValueError("unhandled selection: " + …)`, so an
  unhandled selection fails loudly instead of passing empty. `blocked-stderr` was
  restored verbatim and re-measured above.

## Production LOC

Unchanged. `Production LOC: 578 -> 578 (delta 0)`, scope
`core/crates/{layerfs-bridge,layerfs-service,layerfs-daemon,layerfs-content,layerfs-storage,layerfs-telemetry}`
via `core/tools/production_loc.py`; this receipt changes one external test file and adds
one external test file, both excluded from the count by that tool.

## Verification run with this change

- `env -u RUSTFLAGS cargo +1.85.1 test --locked -p layerfs-bridge` — all targets ok,
  including the new `pipe_deadline` (0.20 s) and the existing relay/batching/AEAD tests.
- `cargo fmt --all -- --check` — clean.
- Both Docker selections above PASS against the registered v9 identities.
- Not run: the full `core` workspace suite and the other nine selections in this
  session; their registered v9 results stand from `acceptance-closure-v9.md`.

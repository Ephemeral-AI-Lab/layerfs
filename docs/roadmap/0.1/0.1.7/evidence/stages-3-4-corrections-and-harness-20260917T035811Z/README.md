# Corrections and harness prerequisites — receipt round

> **Round `stages-3-4-corrections-and-harness-20260917T035811Z`** (host UTC
> `2026-09-17T03:58:11Z`). Collected for the completion prompt's Packages C and D:
> the evidence corrections that follow from the code fixes, and the harness
> prerequisites that must land before any matched campaign. Receipts are
> append-only; nothing here rewrites an earlier round.

## 1. Identity

| Item | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| Base commit | `841d9d2b1` (the A1 round's commit); this round's changes are committed immediately after it |
| Toolchain | `cargo 1.85.1 (d73d2caf9 2024-12-31)` |
| Tool identity | `tool-identities.txt`; every executable archived by sha256 under the git-ignored `benchmark-results/host-store/binary-archive/<sha256>/` with an identity and an archive-observation file |
| Raw logs | `d1-clipped-run.log`, `d2-normal-pooled.log`, `d4-c2-wiring.log`, `d4-c2-wiring-second-case.log`, `d4-all-cases.log`, `c6-frontier-vector.log`, `tests.log`, `clippy.log`, `fmt.log`, `checks.log` |
| Commands, exits and wall times | `commands.txt` (the D1 row's inner exit is `1` and is recorded inside its log; the wrapper's `0` is the `echo` that follows it) |

## 2. D1 — a clipped telemetry tree is a hard failure for a measured row

`measure_pooled`, `measure_edits`, `memory_ledger` and `measure_components` now
return an error from `save_report` when `TimingReport::is_incomplete()`, so a run
whose detail the 1 024-node budget dropped exits non-zero instead of printing
`[incomplete]` and succeeding. The receipt file stays on disk: receipts are never
withdrawn.

Reproduced on the arm the review found clipped. `d1-clipped-run.log`:

```text
timings: 1024 nodes at 3 levels -> /tmp/d1-clip/pooled-save.json
wall_seconds: 2.888903
Error: "timings: INCOMPLETE - the node budget clipped this tree; the run is not a measured row and /tmp/d1-clip/pooled-save.json must not be quoted"
exit: 1
```

The node budget is still 1 024; what changed is that hitting it now fails the run
rather than qualifying it. Deterministic headroom is visible in the same log line:
the unclipped 24-leaf arm prints `timings: 98 nodes at 3 levels`.

## 3. D2 — the budget metric is printed by the tool

`wall_seconds` is printed by all four examples as the last line of a run, from a
timer started at `main`. `d2-normal-pooled.log` records `wall_seconds: 0.140855`
for the 24-leaf arm and `d4-c2-wiring.log` records `wall_seconds: 0.048487` for the
C2 lane, so the ≤15 s per-command rule no longer rests on an unrecorded wrapper.
This is the *tool's* wall time; the wrapper's whole-command wall time is in
`commands.txt` beside it, and the two are not the same metric.

## 4. D3 — the worker identity is recorded, and what it can be cited for

The D4 command line exports `LAYERFS_CONSTRUCTION_WORKERS=1`, so the frozen
identity is now present in a receipt rather than absent. It remains **not an
enforced condition**: no core source reads the variable and neither crate spawns a
thread, so a run that omitted it would behave identically. `stages-3-4-verification.md`
§8.8 states this, and the single-worker property for these rounds is by
construction.

## 5. D4 — the C2 lane can no longer be read as a per-case comparison

`--case` selects the fixture the base is taken from; the stored workload is a fixed
truncation to the whole-file limit plus one fixed patch. Every run of that lane now
prints both facts, and prints the workload it actually stored. `d4-all-cases.log`
reproduces the review's finding exactly:

| `--case` | stored base | patch | `store.sqlite` sha256 (first 16) | bytes |
| --- | ---: | --- | --- | ---: |
| `small` | 65 559 | 256 B at 16 384 | `a9e6591458c23bbc` | 163 840 |
| `chunked` | 131 094 | 256 B at 32 767 | `b59c7b24d91ad7d4` | 294 912 |
| `small-to-large` | 131 094 | 256 B at 32 767 | `b59c7b24d91ad7d4` | 294 912 |
| `large-to-small` | 131 094 | 256 B at 32 767 | `b59c7b24d91ad7d4` | 294 912 |
| `batch` | 131 094 | 256 B at 32 767 | `b59c7b24d91ad7d4` | 294 912 |

Four of five cases write a byte-identical store (`b59c7b24d91ad7d4…`, 294 912 B) —
the review's own recorded hash. The lane was not renamed because existing receipts'
`command.txt` files use `--mode c2`; instead the mode's *output* states what it is,
which is the other remedy the requirement allows.

## 6. D5 — the matched-pair executables are sealed

Both binaries this round's receipts came from, and the memory ledger the completion
prompt's campaign step names, are archived by the sha256 of the produced binary
under `benchmark-results/host-store/binary-archive/<sha256>/`, each with a
`<name>.identity.json` and `<name>.archive-observation.json`, following the pattern
the benchmark tree already uses. `tool-identities.txt` names the hashes, the archive
paths and the exact rebuild command.

**This does not repair the timing round.** Its recorded hashes
(`d7e5c785…`, `f7cdc5ff…`) match nothing on disk and can never match again: those
examples were rebuilt at 09:46 local on 2026-09-17 and no copy was kept. The
archive seals the state from this round forward.

## 7. D6 — the packets without a control log are labelled

W7, W8, W9 and W10 each gained a dated "Control-log status" section stating that
they retain no `*-fails-without-fix` control, that every margin their oracles assert
is **source-derived**, and why a control written after the fact would be a receipt
manufactured for the occasion. The raw census is the review's
`packet-control-audit.txt`. Gate G7's claim therefore holds for W1–W6 only.

## 8. C6 — the with-fix frontier vector now has a receipt

`edit_bounds::the_retained_frontier_does_not_grow_with_the_edit_count` prints its
passing vector instead of only asserting it. Re-run at this tree,
`c6-frontier-vector.log`:

```text
MEASURED frontier peaks (bytes), 1/2/4/8/16 edits: [2208, 2288, 2448, 2768, 3408]
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 1.34s
```

That reproduces the W3 packet's vector byte for byte, so the vector is receipted for
**this** tree. The W3 packet's own copy of it was written against the W3 snapshot and
its README now says which of the two a reader is looking at.

## 9. Checks run for this round

| Check | Exit | Wall | Log |
| --- | ---: | ---: | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` | 0 | see `commands.txt` | `tests.log` — 290 passed, 0 failed |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | | `clippy.log` |
| `cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check` | 0 | | `fmt.log` |
| boundary guard, both tool suites, LOC, `git diff --check` | 0 | | `checks.log` |

## 10. What this round does not do

* It does not collect a campaign. E1 is the owner's decision and is recorded as
  **unanswered** in the closeout report §6; latency, storage and memory stay
  **unqualified** and G13/G15 stay unmeasured.
* It does not repair the timing round's binary identities, and it does not
  re-collect the clipped `e1c-pooled-512` arm.
* It does not add a control log to W7–W10; it labels them.

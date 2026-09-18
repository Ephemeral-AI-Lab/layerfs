# verify-p1-14 — the whole-file edit's buffer is the object

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `d6bc1404a`, arm [`after/`](after/);
> before arm [`../p1-7/after/`](../p1-7/after/) (tree `666691c97`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive d6bc1404a \| tar -x -C /tmp/verify-p114` | 0 | clean P1-14 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | `edit_memory_probe` sha256 in `after/artifacts.txt` |
| R3 | `…/edit_memory_probe` | 0 | `peak_delta_bytes: 131826`, `baseline_live_bytes: 132175`, root `8ddfe36c…` |
| R4 | the same in `/tmp/p1-7-loc` (tree `666691c97`) | 0 | `peak_delta_bytes: 262328`, same baseline and root |
| R5 | `cargo +1.85.1 test … -p layerfs-content --test edit_transitions whole_file_edit` | 0 | `1 passed` (the byte oracle) |
| R6 | `cargo +1.85.1 test --workspace --locked --no-fail-fast` | 0 | 456 passed / 0 failed |
| R7 | counter-only diff of `after/` against `../p1-7/after/` | 0 | only the probe's `peak_live_bytes` and `peak_delta_bytes` differ |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** **No, and it is not meant to.**
`whole_file_edit_emits_the_reference_bytes` is a byte-identity oracle: it asserts
the emitted object equals the reference encoder's output, which is true before and
after (the item changes how the bytes are allocated, never which bytes they are).
The item's gate is the counting-allocator probe (R3/R4), and the new test's job is
to keep a future change from trading the allocation win for different bytes. The
distinguishing evidence is R4 vs R3.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted `peak_delta_bytes` 262,328 → ≈131,118; measured
**131,826** (`2n + 754`), with `baseline_live_bytes`, `edited_root`,
`objects_written`, `objects_written_bytes` and `nodes_read` all unchanged, and
nothing else in the frozen set or the other vehicles moving (R7).

**2c — parity green and unchanged?** Green: the 34-test sealed-oracle set, D10's
65,559 canonical bytes, D20's byte-for-byte readback, and every emitted root. The
test file touched is `edit_transitions.rs`, which **adds** one test and one
one-line field to a helper; no existing assertion was edited.

**2d — single-variable?** One mechanism (the pre-sized object buffer and its one
call site), its test, the two constants it needs exported, and the architecture
note.

**2e — error paths.** Probe each one:
* *An empty payload* and *a payload past `whole_file_raw_limit`* are refused with
  the same `what` and limit the complete encoder uses, **before any byte is
  written**.
* *A canonical object past `whole_file_canonical_limit`* is refused before the
  reserve.
* *A mis-sized assembly* fails closed: the final length check compares
  `canonical.len()` against the header-plus-payload identity, and
  `FinalizedObject::new` re-validates framing, so a wrong reserve or a stray append
  cannot be emitted as a valid object.
* The full edit suite (transitions, single, batch, model, noop, localized, bounds,
  reference) and the read suite are green (R6).

**2f — elapsed as a gate?** No. The gate is `peak_delta_bytes`.

## 3. UNVERIFIED

* **The probe measures one fixture** (n = 65,536, a 512-byte overwrite). No frozen
  row exercises a whole-file edit at another size, so the `2n + O(1)` shape is
  measured at one point and argued from the code for the rest.
* **The `content.encode` child is gone from this route's timing tree** (receipt
  §5.1). No test pins it; nothing re-adds it.
* **`construct_bytes` still double-allocates.** Out of scope by the plan's own
  words; not measured here.
* **No independent reviewer exists.** Every claim above is author-verified; the
  reproducibility of R1–R7 on the named trees is the evidence.

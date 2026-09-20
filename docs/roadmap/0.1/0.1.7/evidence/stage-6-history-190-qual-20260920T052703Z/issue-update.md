## #190 admission qualification: the disposition of `INELIGIBLE` and `INCOMPLETE`

Qualification round on top of #202 (`4049e28b6`), branch
`codex/190-admission-qualification` at `437683aa0`. **No measurement command, build
or test-suite run**: every figure is a constant read from source or a re-derivation
over the retained campaign's own receipts and traces. Full artifact:
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/`
([README](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-admission-qualification/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/README.md),
[DISPOSITION](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-admission-qualification/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/DISPOSITION.md),
[CACHE-STANCE](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/190-admission-qualification/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-qual-20260920T052703Z/CACHE-STANCE.md)).

**Both labels are owner-blocked, and they are one knot.** A history row cannot
reach `PASS`, so `shared/pin_expected.py` refuses its receipt, so the pin set that
would let it pass cannot be generated. No product change moves either label.

**O3.** `runner.re_derive_pins` returns
`FAIL: … has no pinned O3 constant; the oracle cannot have been gated` for all four
retained traces — they publish 421 / 1,249 numeric counters, so `INCOMPLETE` is the
lenient face of a check that reports FAIL. `pin_expected.py` pins every integer
counter of every `status == "PASS"` receipt and has no selection mechanism: the
surface is 42 distinct names (19 row-level, 23 per-state), 2 of 410 / 1,238 values
moved under the retained optimization (both `*_ns` durations), 8 names have 217-row
precedent, and pinning everything adds ~5.3 k constants across the three rows.
Replayed against the status the runner's own rules compose, the generator pins 0.

**INELIGIBLE.** `--verify full` exists, so "a history invocation is always sample
mode" is wrong as written; what is missing is the owner's ruling that these lanes
are admission rows. The budget classifies a formula, not the wall: stride10 is
**20.348 s** (candidate2) — inside a declared 25 s exception, outside the 15 s
limit — and stride3 is **50.356 s**, outside both. A new blocker surfaced: 18.10 s /
23.02 s of the complete command lies outside every declared phase and the runner
fails that reconciliation closed to `INCOMPLETE`; the span is published as
`history.corpus_read_ns` ("inside the root, between the children, untimed"),
measured 16.16 s / 22.94 s, so the honest stride10 command is **36.3 s**, not
20.3 s, once it is declared.

**Cache stance, answered in writing.** `CreatedInSample` is correct for the Store
axis and must not be verified by residency or device attestation —
`resident_pages == 0` is false by construction for a Store written in the timed
region, and `gates::device_attestation` is defined only for cold/de-warmed claims
(and has no call site anywhere in the tree). The undeclared input is the **corpus**
axis: immutable, identity-pinned, read wholly inside the timed region but outside
every measured child, with its residency uncontrolled and unmeasured. The
state-1-silent-read fingerprint is measured (all eight per-state read counters are
zero at state 1 and only there, in all four retained traces).

**Five decisions are needed**, in `DISPOSITION.md` §3: (D1) do these rows carry a
frozen O3 oracle at all; (D3) which bootstrap route creates the first pin set for a
row that cannot pass before it is pinned; (D2) which counters may be frozen;
(D4) admission claim plus frozen budget class, with the phase-accounting choice
behind an honest figure; (D5) whether the corpus-axis stance is acceptable for an
admission row. Standing rulings unchanged: no shrunk selection, no enlarged
timeout, no hand-edited pin, no invented cold claim, no history rows added to
`registry::cases()` / `FROZEN_CARDINALITY` / the 217-row self-check, no promotion of
the 120/240 s diagnostic caps. **#190 stays open.**

Checks: harness shared Python suite 136 tests PASS, `pin_expected.py --self-check`
PASS, corpus manifest SHA256 re-verified. Core cargo checks and the boundary guard
are **not** rerun (no production or core source file changed) and are cited from
L42's recorded PASS, not claimed here. Production LOC: **85,725 → 85,725 (delta 0)**.

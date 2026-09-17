## 8. Next actions

Ordered, smallest first. None of these was performed by this review.

**Blocking full acceptance**
1. **Decide and record the qualification.** Either collect the missing campaign under a
   committed, versioned addendum — pooling families, localized-edit scaling at multiple
   physically realized tree sizes, aligned byte accounting, exact per-case cache/index
   state, n ≥ 5 per arm, release profile — or record both issues' qualification rows as
   **unmeasured** in the issue text and in the gate table, not as PASS. The waiver is the
   owner's to give; the *evidence* gap must not be described as a measurement.
2. **Close the read-amplification gap** (`verification.md:68`) with one real case: count
   bytes read per byte produced for small→large and large→small at one cutoff.
3. **Fix the document contradictions** so one status is readable: the closeout banner and
   `4 against `2 (F-1); the two `8 sections and the stale LOC and directory tables in
   `stages-3-4-report.md` (F-21a,c); the `verification.md:67` against `:110` contradiction.
4. **Re-tie the timing receipts**: archive the producing executables by content hash, or
   re-state the round as source-identical but binary-unbound (F-8); declare
   `e3b-pipeline-chunked-on` (F-7); record the worker-count environment for every arm or
   drop the frozen claim (F-9).
5. **Correct the 158.824 ms smoke row and the `shrink`/`shrink2` mislabel** (F-6) —
   reported by the previous review and still unresolved.

**Correctness and resource hardening (all small)**
6. Bound or justify `pending_values` (`cas/owner.rs:121`) — the only per-save owner with
   no constant and no ledger row (F-10).
7. Page and charge the cleanup pack `DELETE` (`sqlite/cleanup.rs:93–96`) (F-12).
8. Charge, or declare as peaks, the assemble-versus-open-tail overlap, SQLite's BLOB copy
   and MEMORY journal, and the post-hoc transaction check (F-11, F-13).
9. Fix the `next_ordinal` u32 truncation so the ordinal ceiling reports itself (F-14).
10. Cross-check `FileState.logical_len`/`extent_count` against actual tree coverage, and
    validate `ExtentSlice` against the chunk payload at decode (F-15).
11. Add a codec-level decode test (one valid frame, one declared-size lie, one truncated
    frame, one bad checksum) — the FFI boundary currently has zero direct coverage (F-22);
    consider tightening the union oracles to exact variants.
12. Remove the five unreferenced `pub const`s, `SaveHandoff::failure`,
    `mapping_node_limit` and the dead `records_per_group` (F-18); make
    `ConstructionPolicy::capacities()` checked (F-17).

**Evidence hygiene**
13. Add `Scope:`/`Method:` to every commit message and restate the reference subtotal, or
    reduce the line to the delta (F-20).
14. Re-run `examples/memory_ledger.rs` at the final tip and state that its counters exclude
    libzstd's and libsqlite3's heaps (F-19a, `6.4C).
15. Label each number in `w7/README.md` with its receipt, fix the MiB/MB unit error and the
    "E1b shape" mislabel (F-19b,c,d); add `--nocapture` to the `edit_bounds` run so the
    with-fix frontier vector has a receipt.

**Later-stage ownership (not Stage 3–4 defects)**
16. Workspace membership and file count, directory name/path/depth/entry limits, and
    workspace size and quota belong to Stages 5 and 7; they must not be answered from a
    batch-size or object-size limit.
17. Broad qualification (release profile, repeated samples, cold/warm contrast, the full
    limits table, sanitizer and fuzz coverage) belongs to Stage 6 (#171).

### 8.1 What may move, what may not, and what closes the window

The issue tracker maps the stages: #170 is Stage 5 (filesystem trees), **#171 is Stage 6 —
"qualify the complete C1/C2 core for correctness, performance and memory"** — and #172 is
Stage 7 (runtime integration). So the home for *broadened* qualification is Stage 6, not
Stage 7, and Stage 6 itself depends on Stage 5.

**May move (owner-waived, and churn-sensitive anyway):** the repeated-sample latency
sampling, and the full storage and memory campaign. Building a matched pair today would be
invalidated by Stage 5 reshaping `file/edit/apply.rs` and the public surface — AGENTS §3.3:
"A rebuilt artifact needs a rebuilt matched arm."

**May not move — these are invalidated by the same churn, so they are cheapest now:**

1. **Read amplification on the transition.** This is #169 acceptance item 2, not a
   qualification nicety. It is also the cheapest of the missing items: `edit_transitions`
   already passes 5/5 and `StoreReadCounters` already exists and is already printed
   (`w10/w10-verify.log:1052` records `objects: 1, packs_read: 1, pages: 1` for a 6 MiB
   root read). What is missing is an assertion family plus one case on existing harness.
   Reconstructing the transition semantics after Stage 5 changes them is strictly more
   expensive than writing the case against the code it describes.
2. **The versioned campaign addendum** — cases, seeds, identities, cache state including
   cold/warm, sample count, numerical limits and the acknowledgement boundary. Required
   before any collection, it is text, and `verification.md:155–161` already demands it.
3. **The two defects that would make any future campaign inconclusive**: the storage
   byte-accounting boundary (neither arm opens a Store, and the two sides compute different
   quantities — F-4) and **reference-side memory instrumentation**, which today is zero.
4. **Seal the matched-pair binaries** (sha256 plus an archived copy). This is not
   hypothetical: the timing round's binaries were rebuilt and its recorded identities now
   match nothing on disk (F-8). The C1 pair will suffer the same fate.
5. **The harness defects a future campaign would inherit**: telemetry clipping at 1 024
   nodes (57.46 % of `e1c` unattributed), `wall_seconds` printed by no product tool, the
   unexported worker-count variable (F-9), and the C2 lane that ignores `--case` so that
   four of its five arms produced byte-identical stores.

**The trigger is not the end of a stage.** The window for a matched v0.1.6 arm closes when
the reference tree is retired or the C1/C2 public surface is reshaped — today it is still
open (`stages-3-4-file-plan.md:281` "Reference retirement remains later";
`stages-3-4-continue-acceptance.md:101` "Do not release, tag, retire reference code"). If
that lands before Stage 6, the campaign must run then, not later.

**What may *not* be done is relabelling.** Four of the batch's own documents forbid the
blanket move — `stages-3-4-completion-handoff.md:220` "Do not postpone a Stage 3–4
acceptance requirement merely by relabelling it Stage 6";
`stages-3-4-continuation-prompt.md:74` "Stage 6 does not absorb #168/#169's required
evidence"; `stages-3-4-verification.md:153` "The earlier reference to later qualification
does not defer those issue requirements to Stage 6"; and `stages-3-4-continue-acceptance.md:98–100`
"do not close by description change or silently defer it to Stage 6." The only instrument
that legitimately carries a deferral is the owner's written waiver, and it exists for G13
and G15 and for nothing else. When the qualification is eventually collected, it must be
reported under #168/#169 or as an explicit Stage 6 requirement — not as their acceptance.

(*The split above and the "cheapest now" ordering follow an independent audit's
recommendation; the code, counter and harness facts it rests on are cited inline and were
verified in this review. Effort estimates are that audit's judgement, not measurements.*)

---


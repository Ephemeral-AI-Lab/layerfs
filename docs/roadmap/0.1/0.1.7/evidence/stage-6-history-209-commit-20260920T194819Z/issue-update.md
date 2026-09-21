## #209 — `commit_ns` round: the page cache is refuted, and the per-step policy re-assertion is removed

Diagnostic evidence, not release admission. Report:
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/README.md`;
pre-registration, raw runs, diagnostics and analysis are beside it. Ledger L57.

**1. The round's first hypothesis is refuted, by the engine's own page accounting.**
A replay of the step's statement shape on a copy of the run's own Store, interleaved
round-robin in one process (6 rounds × 2,000 commits per arm), writes **exactly 15.30
pages per commit in every arm** — declared profile, `cache_size = 64 MiB`,
`cache_spill = 0`, both, `mmap_size = 256 MiB`, and a contract-breaking
`journal_mode = OFF` diagnostic — at **1.86–1.92 µs per page** (1.8 % spread), zero
spills. A bare 4 KiB `pwrite` is 1.723 µs, so the per-page cost is the write syscall.
`sqlite3BtreeInsert` overwrites in place only when the new payload is the same size as
the old, and a pack grows on every append, so every append reallocates the overflow
chain: **`pages written == ceil(pack body / 4096)`**, i.e. 4.18 GiB = 1,095,640 pages,
and at ~2.5 µs a page that is the 2.809 s `COMMIT`. The payload's pages are the stored
format's price; any fix moves the Store hash and is a different operation.

**2. The one treatment, pre-registered.** A step commits the policy state it changed,
not the policy state it re-asserted. `advance_pack` (0.345 s over 48,446 calls,
instrumented last round) writes back the value already in the row on **48,191** of
them; the `saves.pack_ceiling` UPDATE does the same on **45,794 of 46,049**. The
watermark is advanced only when it moved and the ceiling written only by the append
that creates a pack — **not** deferred to publication, which is the change that would
let a second writer collide. Two new cases in
`core/crates/layerfs-storage/tests/pack_watermark.rs` pin that invariant from outside
the crate, and both fail on a structurally deferred watermark (red run retained).

**3. Measured, both arms from one binary** (sha256 `106f181b…`, arm selected by a
measurement-only lever declared in `extra_environment` and removed before the commit).
Gate pair: operation **17.105 → 16.499 s**, `commit_ns` **1.933 → 1.768 s** (39.9 →
36.5 µs per append). A declared **A B B A** diagnostic reads 16.634 / 16.451 / 16.381 /
16.524 s and 1.847 / 1.746 / 1.749 / 1.872 s: **drift-cancelled −0.163 s operation and
−0.112 s `commit_ns`**. Pooled: operation −0.237 s (−1.41 %), `commit_ns` −0.111 s
(−5.90 %), with `resolve_ns`, `sql_ns` and `filesystem` unmoved. **`commit_ns`
separates without overlap in all seven rows** (control 1.847–1.933 s, treatment
1.746–1.829 s); the operation does not once the shipped row is included.

**4. The held absolute bar is not reproducible today, and that is measured.** The
archived previous-round binary, unchanged, read **26.467 s in its own session and
19.908 s in this one** (identical counters, identical Store); two binaries with the
same product behaviour read 17.105 s and 19.908 s fifteen minutes apart. So the bar
(< 26.467 s, `commit_ns` < 3.154 s) is met by the treatment arm *and by the control*,
does not discriminate, and is not claimed as this round's evidence — the matched
contrast is. Corpus residency is 0 pages in every row of this round against 5,141
previously, so these rows are the colder ones.

**5. Equivalence and capability.** Store byte-identical in all seven rows
(`7ea2fe6ccf13bc5a…`, 51,867,648 bytes) and all **582** workload counters identical.
The second writer still streams on the shipped step in both arms — both writers
finished in every round, **zero `OwnershipUnavailable`**, pair p99 6.7–7.1 ms
(treatment) against 6.4–7.1 ms (control), worst observed 9.8 ms against 36.5 ms;
`multi_writer.rs` 5/5.

**6. Checks.** Core `fmt --check`, `test --locked --workspace` **537/0**,
`clippy -D warnings`, boundary guard PASS over 175 files and self-tests 6/6; harness
**117/0** and a release build. No CI, no `tools/preflight.sh`. Every row is a
diagnostic: admission `INELIGIBLE`, every budget class `NOT_RUN`.

**7. Named for the next round, with its measurement.** The hot per-step statements are
prepared **fresh on every call**: `UPDATE object_packs SET data …` **4,723 ns against
102 ns** cached over 46,049 calls a run — a 0.2–0.4 s term inside `sql_ns` and the
uncharged work, and a separate treatment from this one. `resolve_ns`'s cadence share
and `filesystem` remain untouched.

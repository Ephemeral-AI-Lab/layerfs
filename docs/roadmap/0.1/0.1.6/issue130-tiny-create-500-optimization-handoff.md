# Handoff: optimize `tiny-create-500-mixed-v4` on the wired route (#130)

Scope decision (owner, 2026-09-14): **work on `tiny-create-500-mixed-v4` only.**
The working belief is that fixing this case fixes the other tiny cases; that
belief is a **hypothesis to test at the end**, not an assumption to build on.
**Do not run multi-sample comparisons during iteration** — the route's absolute
time is already large, so one sample per change is the iteration loop. The n3
paired screen remains the terminal evidence rule (see "Evidence discipline").

## Measured starting point (ledger L44)

Same-route control = `codex/issue130-m2-control` (`0b15e1783`): current main with
the three #130 commits reverted. Candidate = main `918ad73de`.

| run | begin | exec | commit | visibility | end | total |
|---|---|---|---|---|---|---|
| candidate median (3 samples) | 10.5 ms | **11,362.6 ms** | **4,406.8 ms** | 0.074 ms | **4,249.2 ms** | **20,029.7 ms** |
| control median (3 samples) | 11.1 ms | 10,980.2 ms | 4,128.9 ms | 0.067 ms | 3,983.9 ms | 19,395.8 ms |
| v0.1.5 (historical, legacy route) | 10.733 ms | 166.715 ms | 59.709 ms | 0.107 ms | 5.395 ms | 242.660 ms |

Screen verdict: PASS (no material regression, +3.27 %, threshold 2,909.4 ms) and
**no visible speedup**. Per affected file the route costs ≈23 ms exec, ≈8.8 ms
commit, ≈8.5 ms end; begin and visibility match v0.1.5, so this is per-operation
work, not setup or timer overhead.

## Iteration loop (fast, one sample)

```bash
# candidate arm (after a source change): rebuild both identities, then ONE sample
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
python3 benchmark/fs-bench-pro/shared/runner.py --build-image
bash benchmark/fs-bench-pro/families/tiny_file_churn/perf.sh \
  --case tiny-create-500-mixed-v4 --seed 1 --setup clone \
  --image "$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image 2>/dev/null | tail -1)" \
  --host-binary "$PWD/target/release/fs-benchmark-pro" --source-arm candidate \
  --perf-fast --collection-mode --product-timeout 300 --timeout 310 --setup-timeout 900 \
  --output "$PWD/benchmark-results/issue130-m2/runs/iter-$(date +%s)"
```

- Prepared input (5,000 files / 500 MiB background) is cached and reused
  (≈2.4 s preparation); do not regenerate it.
- Compare against the recorded control median (19,395.8 ms) for triage; a single
  candidate sample is a **diagnostic**, not acceptance evidence.
- Phase extraction (receipts nest `kind: phase` records inside the sample):

```python
import json, pathlib
def phases(path):
    out = {}
    def walk(o):
        if isinstance(o, dict):
            if o.get('kind') == 'phase' and 'elapsed_ns' in o:
                out.setdefault(o['phase'], o['elapsed_ns'])
            for v in o.values(): walk(v)
        elif isinstance(o, list):
            for i in o: walk(i)
    for line in pathlib.Path(path).read_text().splitlines(): walk(json.loads(line))
    return out
```

## Where the time is — investigate in this order

1. **exec ≈23 ms/file.** The workload writes through the container daemon FUSE →
   host authority. Candidate causes: per-operation request/round-trip overhead on
   the mounted path (daemon → host wire), per-operation lifecycle/admission work,
   and metadata page I/O. In-process counters measured for the same create/write
   path (before #130): 48.2 metadata page writes and 280 page reads per tiny file
   (`host_overlay::tests::tiny_create_metadata_page_cost_is_measured`). Reproduce
   that counter on the *benchmark* path before optimizing, and add a
   host-authority dispatch/operation counter to the run receipt (the M2 runs had
   route-level proof only).
2. **commit ≈8.8 ms/file.** Candidate construction and correspondence per changed
   file. Measure the per-file construction split (scan/correspondence/encode/CAS)
   before changing anything.
3. **end ≈8.5 ms/file (≈788× v0.1.5).** Suspects: verified unmount, authority
   settle/`after_detach`, deleted-directory cleanup, and removal of the
   per-Workspace state directory (arena + catalog files). Measure the split
   (unmount vs settle vs state removal) before optimizing.

Do not resume storage micro-optimization (denser Index pages, compact ranges,
packed tiny payload, singleton correspondence) until the dominant phase above is
attributed and reduced: P130.2's measured −18 % page writes / −23 % page reads per
create is ~1–2 % of this workflow.

## Guardrails (unchanged)

- First-party only: no third-party patch/fork/vendor/substitute, no dependency or
  lockfile change; standard library is fine.
- Preserve semantics: owned capture, C1/C2 locality, live mount/inode/descriptor
  identity, ordinary progress during Commit, CAS/FULL-DELTA/CDC/extents/packing,
  exact stage/retry and Created/UpToDate receipts. No pause/quiesce/drain/
  checkpoint-reset fallback in ordinary Commit. V1 dirty-mmap visibility stays
  open and is not claimed by a faster case.
- Keep gates green: `RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh`, `cargo fmt --all
  --check`, clippy correctness/suspicious. Run affected focused tests, not the
  world, per change.
- No #122 case or `shared/v016_matrix.py`; no release/tag/deployment;
  25k/two-second and million-file stay DEFERRED/OPEN.

## Evidence discipline

- Iteration: one sample per arm per change; label it diagnostic. Retain every
  attempt, including failures, in the append-only ledger; do not rerun unchanged
  cases for a nicer number.
- Terminal evidence for this case: the prospective #118 n3 fresh alternating
  pairs against the same-route control, median paired slowdown threshold
  `max(15 % of control median, 3 ms)` with ≥2/3 pairs slower; whole-workflow
  metric only (`operations=1`), phases reported separately. A passing single
  sample is never terminal evidence.
- Raw receipts stay in `benchmark-results/issue130-m2/runs/` (git-ignored) with
  arm identities; record identities and numbers in
  `overlay-snapshot-verification-ledger.md`.
- The strict **<1 s tier-100 bulk create/delete** criterion is at high risk at
  the current per-file cost and was not measured — measure it before claiming the
  tiny family is healthy.

## Definition of done for this handoff

1. A measured, attributable reduction of the dominant phase on
   `tiny-create-500-mixed-v4` (single-sample iterations), with the change
   explained by counters rather than a favourable run.
2. The same-route n3 paired screen re-run for `tiny-create-500-mixed-v4`.
3. A bounded check that the other tiny cases (at minimum `tiny-stat-500-mixed-v4`,
   `tiny-unlink-500-mixed-v4`; ideally the two bulk-500 cases) follow the
   hypothesis — reported as measured or not measured, never assumed.
4. Correctness/gates green, evidence ledgered, source published through the
   normal PR workflow with the issue updated. #130 stays open until its terminal
   conditions are met.

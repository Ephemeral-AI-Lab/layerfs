# S5 — independent re-derivation (#219 S0/S1/S2)

> **Status:** read-only review. `rederive.py` reads only raw receipts — no report,
> no summary — recomputes every headline number in the S0/S1/S2 reports and compares
> it to the value those reports claim. It exits non-zero on any mismatch.

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue219-s5-review-20260921T031259Z/rederive.py
```

Result on `49ac6f883`: **ALL CHECKS PASS** (44 checks).

What it re-derives: both S1 medians and their 400 MB rates; both verification
statuses, cleanup statuses, `verification_ns` and scanned counts; the full identity
match between each performance row and its verification receipt (source, input,
product, image, harness identities plus the source commit, product, compilation and
dependency seals and the workload hash); the S2 mechanism (identical 73-transaction
geometry, 36.2× store-growth ratio, +370.5 ms system CPU / −113.6 ms user CPU,
24.1× anchor-to-transaction ratio, 0.786 ms/MB marginal, 0.73 MB disk read,
12.0 ms container CPU, 1.89× CPU/wall); and the cache declaration on both rows.

It does **not** re-derive, and the reports do not claim: the anchor's share of the
timer, the per-file/per-byte split, or any cold-cache figure. Those are marked
`NOT_MEASURED` in the S2 report.

## Reviewer's independent checks (not in the script)

```sh
git diff --stat v0.1.6..HEAD -- 'crates/*/src/**'          # empty
git show v0.1.6:benchmark/fs-bench-pro/families/init_namespace/mod.rs \
  | diff - benchmark/fs-bench-pro/families/init_namespace/mod.rs   # exit 0
grep -c '"source_arm": *"baseline"' <each namespace-10000 receipt>  # 0
```

The S0 claim that v0.1.6 and `HEAD` share 216 production files with zero content
differences was checked twice: once by the product-seal replica in S0 §3, once by
the `git diff --stat` above.

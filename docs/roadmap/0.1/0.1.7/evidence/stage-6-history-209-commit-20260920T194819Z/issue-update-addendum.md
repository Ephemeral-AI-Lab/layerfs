## #209 addendum — the per-append multiple is 7.7×, and the regression is window-dependent

Follow-up to the previous comment; diagnostic evidence, not release admission. Report:
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-commit-20260920T194819Z/previous-model-window.md`;
ledger L58. No product line changed.

**Why.** The previous comment priced this model's commit at 36.6–38.7 µs per append
against the previous model's **9.5 µs** and called the multiple ~4×. The 9.5 µs is
right for its own row (0.4367 s over 45,791 appends; 1,149 commits at 380.0 µs) but it
was measured in the **08:10Z** window, so the 4× divided one window's numerator by
another window's denominator. The previous-model binary was never archived; it has now
been rebuilt from `f039bcaf2` (`3d818895f70b…`) and validated as the same operation by
saving the **byte-identical retained previous-model Store `4af37932aa3391b1…`** in both
of its rows. The harness source seal is `04bcfab5…` on both sides — the harness is
identical, only the product crates differ.

**Measured back to back in one window**, one sample per arm per order, balanced
`prev → shipped → shipped → prev` so the corpus-residency asymmetry is shared:
`prev-model` 11.033 s / 0.242 s, `shipped-b` 16.481 s / 1.794 s, `shipped-c` 16.714 s
/ 1.817 s, `prev-model-b` 10.921 s / 0.228 s.

| | previous | shipped | multiple |
| --- | ---: | ---: | ---: |
| operation | 10.977 s | 16.598 s | **1.51×** |
| `commit_ns` | 0.235 s | 1.806 s | **7.67×** |
| per append | 5.14 µs | 39.43 µs | **7.67×** |

- **The per-append commit multiple is ~7.7×, not ~4×** (7.40× and 7.96× by order).
  The 08:10 same-session pair gives **7.17×**, so the **ratio is window-stable while
  the absolute prices are not**: the previous model reads 9.54 µs there and 5.14 µs
  here, this model 68.4 µs and 39.43 µs.
- **`gap-attribution.md` §3's 6.8× is a units mismatch** — `commit_ns` per *commit*
  (65.1 µs) against a per-*append* figure. Per append this model reads 68.9 µs there,
  so the same-session multiple is **7.2×**.
- **The operation-level regression is 1.51× in this window against the 2.02× the #205
  pair measured in its own** (16.360 → 33.123 s, same session, and that row stands).
  Both models are faster here, by different factors — previous 16.36 → 10.98 s, this
  model 33.12 → 16.60 s — so the commit multiple held and the operation multiple did
  not.

**The gap in this window:** operation +5.621 s = `commit_ns` +1.570 s (27.9 %),
`resolve_ns` +1.528 s (27.2 %), `filesystem` +0.980 s (17.4 %), uncharged inside the
accept span +0.780 s (13.9 %), `sql_ns` +0.501 s (8.9 %), the rest +0.262 s. The two
largest terms are within 3 % of each other, which is why the next rounds are
`resolve_ns` and `filesystem` rather than `commit_ns` again.

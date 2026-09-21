## #209 — confirmation window: the kept change reproduces on `commit_ns`

Diagnostic; not release admission. **No product line changed** — the tree stays
byte-identical to `704580673` over `core/crates` and `crates`. Report:
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/README.md`;
ledger **L60**.

**Why re-run it.** The kept change (a step commits the policy state it changed, not the
policy state it re-asserted) was measured in one window, and the machine's level then moved
by more than the effect — `commit_ns` 1.81 → 2.49 s and stride10 16.5 → 22.5 s for identical
work. One window's contrast cannot carry it, so it was re-run with **no source edited**: the
kept round's archived lever binary (`106f181b…`) as the matched same-binary instrument, and
the shipped binary (`3a6c1c20…`), which the current tree rebuilds **byte for byte**.
`collect.py` / `with_locks.py` are byte-identical to the retained ones and the harness seal
is `04bcfab5…`, so no harness change can invalidate the pair.

| row | arm | operation | `commit_ns` | per append |
| --- | --- | ---: | ---: | ---: |
| `confirm-1a` / `confirm-4a` | control (lever unset) | 17.778 / 17.706 s | **2.058 / 2.037 s** | 44.94 / 44.48 µs |
| `confirm-2b` / `confirm-3b` | treatment (`=0`) | 17.794 / 17.772 s | **1.952 / 1.951 s** | 42.62 / 42.61 µs |
| `shipped-confirm` | treatment (shipped binary) | 17.550 s | **1.932 s** | 42.18 µs |
| | Δ (treatment − control) | −0.036 s (−0.20 %) | **−0.103 s (−5.01 %)** | −2.19 µs |

Arms do not overlap in `commit_ns` (2.037–2.058 against 1.951–1.952); the shipped row is
below both treatment rows; the saved Store is byte-identical in all five rows
(`7ea2fe6ccf13bc5a…`) with `commits` 48,446 everywhere.

**What it confirms.** The bucket effect reproduces in sign and size: **−5.90 %** in the
keeping window against **−5.01 %** here, in a window whose level is 8 % higher on the
operation and 5 % higher on `commit_ns` — ~2.2 µs off a 44.7 µs per-append commit price,
which is what the treatment claimed.

**What it does not.** The operation-level effect does not resolve: −0.237 s there against
−0.036 s here, both smaller than this window's own −0.228 s first-row-to-last drift. **The
change is kept for what it provably removes — 48,191 of 48,446 watermark statements and
45,794 ceiling statements, with a byte-identical Store — not for a wall-clock claim**, and
that is now the recorded basis.

One custody note worth carrying forward: `LAYERFS_STORAGE_POLICY_REASSERT=0` **is the
treatment** and unset is the control — the opposite of the statement-cache round's lever,
where `=0` restored the old behaviour. This window's first pass read it backwards and was
caught because the shipped row landed inside the control band; every row's arm is now
checked against its receipt's `extra_environment`.

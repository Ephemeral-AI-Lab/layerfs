# Stage 1.1T TrustedLocalDev final result

Status: `PASS_PRIMARY_TRUSTED_CLASS`. This is explicitly not Verified.

Attempt-001 remains immutable for `5e58fa6`, but handoff review found that a
materialize-only Trusted open did not mark `trusted_history`. Commit `dfa6020`
fixes the boundary by durably setting the existing bit at every explicit
Trusted open. Focused proof shows the next Verified open performs one full
retained-union scrub and clears it. Attempt-002 is the final source-bound
population for that corrected product.

| Class | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary 450 gate |
|---|---:|---:|---:|---:|---|
| Verified 0 MiB | `24.071333 ms` | `24.648250 ms` | N/A | N/A | report |
| Trusted 0 MiB | `22.961833 ms` | `33.035708 ms` | N/A | N/A | report |
| Verified 24 MiB | `62.191459 ms` | `65.981500 ms` | `385.905` | `363.738` | FAIL |
| Trusted 24 MiB | `43.504833 ms` | `45.408083 ms` | `551.663` | `528.540` | PASS |
| Verified 96 MiB | `179.337500 ms` | `183.878333 ms` | `535.304` | `522.084` | PASS |
| Trusted 96 MiB | `89.210208 ms` | `102.889500 ms` | `1076.110` | `933.040` | PASS |

Trusted saved `18.686626/20.573417 ms` at 24 MiB p50/p95 and
`90.127292/80.988833 ms` at 96 MiB. The separate populations are not pooled,
and the earlier identity owner remains an upper bound rather than a promise.

The diagnostic slope is `634,796.875 ns/MiB` (`1575.307 MiB/s`), but measured
zero differs from the `28.269708 ms` intercept by `5.307875 ms`; the model is
invalid. Fixed cost misses by `8.269708 ms`. No unchanged-source rerun was
performed.

```text
source / release SHA-256                         dfa6020 / dc500fc862c76ec5...
campaign wall                                    2.100807792 s
fetched / role / identity passes / identity ns   26,016 / 26,016 / 0 / 0
RSS / Q high / Q terminal                        15,826,944 / 8,388,607 / 0 B
scratch / total connections peak / terminal      1 / 2 / 0
FD baseline/terminal per block                    4/4, 4/4, 4/4
owned residue / network                           0 / 0
```

This material gain admits the Stage 1.2 developer-loop handoff only. Each
workflow may explicitly use TrustedLocalDev for local repeated operations, but
must drop its Trusted workspace/Engine, reopen Verified, complete the mandatory
scrub, and only then capture, publish, export or share.

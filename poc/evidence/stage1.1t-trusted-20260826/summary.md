# Stage 1.1T TrustedLocalDev result

Status: `PASS_PRIMARY_TRUSTED_CLASS`. This is explicitly not Verified.

One clean release, one readiness and one source-bound 12-row campaign were
used. The exact population was one conditioning warmup plus three measured
complete materializations at each of 0, 24 and 96 MiB. Campaign wall was
`2.043962958 s`, below both the 15-second preferred and 30-second hard walls.

| Class | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary 450 gate |
|---|---:|---:|---:|---:|---|
| Verified 0 MiB | `24.071333 ms` | `24.648250 ms` | N/A | N/A | report |
| Trusted 0 MiB | `26.626833 ms` | `36.185625 ms` | N/A | N/A | report |
| Verified 24 MiB | `62.191459 ms` | `65.981500 ms` | `385.905` | `363.738` | FAIL |
| Trusted 24 MiB | `37.595417 ms` | `40.417625 ms` | `638.376` | `593.800` | PASS |
| Verified 96 MiB | `179.337500 ms` | `183.878333 ms` | `535.304` | `522.084` | PASS |
| Trusted 96 MiB | `81.700958 ms` | `89.090125 ms` | `1175.017` | `1077.561` | PASS |

Trusted saved `24.596042/25.563875 ms` at 24 MiB p50/p95 and
`97.636542/94.788208 ms` at 96 MiB. These are separate source-bound
populations and are not pooled. The earlier measured identity owner was only
an upper bound; the slightly larger observed end-to-end deltas include normal
whole-route/build/process variance and are not attributed entirely to hashing.

The 24-to-96 slope implies `1632.448 MiB/s`, but the zero-byte residual is
`3.733263 ms`; therefore the three-point model is invalid. The diagnostic
fixed intercept is `22.893570 ms`, a `2.893570 ms` numerical miss. The campaign
was not rerun for a friendlier intercept.

Trust/resource closure:

```text
fetched rows / role decodes / identity auth passes  26,016 / 26,016 / 0
identity-authentication wall                         0 ns
new/incumbent write authentication                   unchanged and focused-test PASS
Trusted publication history marker                  unchanged
Verified promotion                                  close + reopen + full retained-union scrub
RSS / Q high / Q terminal                           17,694,720 / 8,388,607 / 0 B
scratch / total connections peak / terminal         1 / 2 / 0
FD baseline/terminal per block                       4/4, 4/4, 4/4
owned residue / network                              0 / 0
```

The gain is material enough for the narrow Stage 1.2 developer-loop handoff:
local repeated operations may explicitly open `TrustedLocalDev`; every
capture/publish/export/share promotion must close it and successfully reopen
Verified through the full scrub first.

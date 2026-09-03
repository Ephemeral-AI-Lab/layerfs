# edit_canonical_chunk_count SDK-only edit benchmark

Status: **FAIL**

Raw evidence: [performance JSONL](performance/raw.jsonl), [verification aggregates](verification/raw.jsonl), [source subproofs](verification/subproofs.jsonl).

## Latency

| Operation | Size | Source | Samples | Edit median (min–max) ms | Commit median (min–max) ms | Edit+Commit median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| `overwrite-fixed-64k-chunk-count-preserve` | 1 MiB | baseline | 5 | 19.342 (14.085–25.075) | 6.790 (2.655–9.487) | 25.611 (18.791–34.562) |
| `overwrite-fixed-64k-chunk-count-preserve` | 1 MiB | candidate | 5 | 2.782 (1.234–7.541) | 3.932 (2.302–8.028) | 6.715 (3.537–15.569) |
| `overwrite-fixed-64k-chunk-count-preserve` | 10 MiB | baseline | 5 | 20.896 (17.393–27.611) | 5.256 (4.087–6.528) | 27.424 (22.026–32.867) |
| `overwrite-fixed-64k-chunk-count-preserve` | 10 MiB | candidate | 5 | 2.832 (1.564–3.197) | 4.144 (3.032–5.043) | 6.198 (5.239–7.875) |
| `overwrite-fixed-64k-chunk-count-preserve` | 100 MiB | baseline | 5 | 17.809 (14.293–22.567) | 5.714 (3.959–5.925) | 23.261 (20.218–28.485) |
| `overwrite-fixed-64k-chunk-count-preserve` | 100 MiB | candidate | 5 | 3.183 (1.419–7.251) | 5.116 (4.091–5.273) | 8.060 (5.675–12.367) |
| `overwrite-fixed-64k-chunk-count-preserve` | 500 MiB | baseline | 5 | 14.229 (12.745–20.011) | 12.742 (8.275–17.585) | 26.592 (22.505–37.596) |
| `overwrite-fixed-64k-chunk-count-preserve` | 500 MiB | candidate | 5 | 2.550 (1.931–5.408) | 12.023 (8.338–15.876) | 14.573 (12.863–18.022) |
| `overwrite-fixed-64k-chunk-count-increase` | 1 MiB | baseline | 5 | 18.102 (17.036–22.758) | 3.850 (2.858–4.956) | 21.877 (20.742–27.714) |
| `overwrite-fixed-64k-chunk-count-increase` | 1 MiB | candidate | 5 | 1.855 (1.343–2.983) | 3.056 (2.626–3.591) | 5.069 (3.968–6.469) |
| `overwrite-fixed-64k-chunk-count-increase` | 10 MiB | baseline | 5 | 24.376 (16.859–27.818) | 5.202 (3.606–5.835) | 30.137 (20.465–33.653) |
| `overwrite-fixed-64k-chunk-count-increase` | 10 MiB | candidate | 5 | 2.718 (1.242–4.115) | 3.263 (3.142–6.043) | 6.272 (4.391–9.884) |
| `overwrite-fixed-64k-chunk-count-increase` | 100 MiB | baseline | 5 | 25.079 (14.188–31.967) | 5.543 (5.035–6.251) | 30.249 (19.731–37.002) |
| `overwrite-fixed-64k-chunk-count-increase` | 100 MiB | candidate | 5 | 1.677 (1.458–2.361) | 4.850 (4.571–6.982) | 6.932 (6.053–8.660) |
| `overwrite-fixed-64k-chunk-count-increase` | 500 MiB | baseline | 5 | 16.093 (12.227–27.852) | 13.209 (9.415–21.814) | 30.062 (25.436–49.666) |
| `overwrite-fixed-64k-chunk-count-increase` | 500 MiB | candidate | 5 | 2.471 (1.538–2.783) | 11.471 (7.930–15.396) | 13.008 (10.713–17.127) |
| `overwrite-fixed-64k-chunk-count-decrease` | 1 MiB | baseline | 5 | 19.558 (13.960–23.243) | 2.602 (2.356–4.657) | 22.160 (16.315–25.677) |
| `overwrite-fixed-64k-chunk-count-decrease` | 1 MiB | candidate | 5 | 1.997 (1.367–3.190) | 3.084 (2.419–3.437) | 5.040 (4.436–5.609) |
| `overwrite-fixed-64k-chunk-count-decrease` | 10 MiB | baseline | 5 | 19.099 (13.678–23.248) | 3.694 (3.478–5.859) | 22.609 (17.157–27.722) |
| `overwrite-fixed-64k-chunk-count-decrease` | 10 MiB | candidate | 5 | 1.623 (1.214–3.193) | 3.518 (2.760–3.826) | 5.141 (4.201–7.018) |
| `overwrite-fixed-64k-chunk-count-decrease` | 100 MiB | baseline | 5 | 16.584 (15.182–20.402) | 5.069 (4.556–6.124) | 21.948 (20.505–25.471) |
| `overwrite-fixed-64k-chunk-count-decrease` | 100 MiB | candidate | 5 | 3.119 (2.002–5.162) | 7.430 (5.215–8.120) | 10.433 (7.216–12.592) |
| `overwrite-fixed-64k-chunk-count-decrease` | 500 MiB | baseline | 5 | 16.394 (14.647–24.146) | 11.545 (9.654–13.221) | 27.143 (26.048–37.367) |
| `overwrite-fixed-64k-chunk-count-decrease` | 500 MiB | candidate | 5 | 2.319 (2.165–4.706) | 10.454 (8.398–12.847) | 13.438 (10.717–15.160) |

Nominal targets are 10/10/20 ms; user-approved accepted ceilings are 20/20/30 ms for Edit/Commit/combined. Combined is independently capped at 30 ms. Parity and resource gates are unchanged.

Memory profile: ack-window-v1. Cgroup observations cover an acknowledged broader window, not exact T0–T3. Native peaks are whole-worker/container lifetime bounds. Category maxima, dirty/writeback, and transient swap checks are sampled observations; continuous category ceilings cannot be strictly proven. Gaps are reported diagnostically. Native peak/incremental/size-spread limits and zero OOM remain binding.

| Candidate scenario | Latency classification |
| --- | --- |
| `overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1` | accepted-with-tolerance |
| `overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1` | accepted-with-tolerance |
| `overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1` | nominal-pass |
| `overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1` | accepted-with-tolerance |

## Memory

| Operation | Size | Source | Process phase MiB median (min–max) | Process incremental MiB median (min–max) | Cgroup sampled window MiB median (min–max) | Cgroup sampled window incremental MiB median (min–max) | Dirty/writeback incremental MiB median (min–max) |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| `overwrite-fixed-64k-chunk-count-preserve` | 1 MiB | baseline | 7.797 (7.625–7.812) | 1.953 (1.922–2.141) | 2.477 (2.328–2.664) | 0.281 (0.043–0.629) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 1 MiB | candidate | 7.844 (7.562–8.016) | 1.922 (1.906–1.938) | 2.449 (2.207–2.594) | 0.246 (0.074–0.340) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 10 MiB | baseline | 8.203 (8.094–8.312) | 2.234 (2.188–2.359) | 2.633 (2.449–2.785) | 0.605 (0.305–0.688) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 10 MiB | candidate | 8.094 (7.938–8.156) | 2.312 (2.234–2.469) | 2.473 (2.266–2.578) | 0.328 (0.023–0.551) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 100 MiB | baseline | 8.562 (8.453–8.672) | 2.344 (2.328–2.469) | 2.566 (2.410–2.754) | 0.379 (0.000–0.527) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 100 MiB | candidate | 8.562 (8.391–8.734) | 2.422 (2.344–2.484) | 2.473 (2.324–2.504) | 0.375 (0.121–0.543) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 500 MiB | baseline | 10.547 (10.391–10.609) | 3.938 (3.906–4.000) | 2.617 (2.426–2.664) | 0.402 (0.105–0.832) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-preserve` | 500 MiB | candidate | 10.578 (10.375–10.688) | 3.984 (3.922–4.125) | 2.297 (2.270–2.676) | 0.156 (0.027–0.629) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 1 MiB | baseline | 7.875 (7.719–8.125) | 2.062 (1.906–2.188) | 2.500 (2.324–2.848) | 0.371 (0.285–0.641) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 1 MiB | candidate | 7.875 (7.641–8.000) | 1.984 (1.891–2.125) | 2.402 (2.230–2.527) | 0.250 (0.152–0.602) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 10 MiB | baseline | 8.234 (8.062–8.328) | 2.328 (2.281–2.375) | 2.602 (2.500–2.621) | 0.680 (0.352–0.855) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 10 MiB | candidate | 8.125 (8.078–8.203) | 2.438 (2.297–2.516) | 2.359 (2.234–2.543) | 0.250 (0.000–0.523) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 100 MiB | baseline | 8.641 (8.438–8.688) | 2.453 (2.359–2.578) | 2.590 (2.344–2.668) | 0.727 (0.000–0.816) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 100 MiB | candidate | 8.656 (8.547–8.812) | 2.578 (2.422–2.656) | 2.320 (2.215–2.539) | 0.160 (0.000–0.445) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 500 MiB | baseline | 10.656 (10.516–10.828) | 4.109 (4.016–4.219) | 2.602 (2.430–2.758) | 0.516 (0.129–0.746) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-increase` | 500 MiB | candidate | 10.703 (10.625–10.766) | 3.922 (3.891–4.125) | 2.496 (2.242–2.617) | 0.320 (0.215–0.387) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 1 MiB | baseline | 7.484 (7.375–7.703) | 1.719 (1.594–1.766) | 2.441 (2.348–2.586) | 0.352 (0.246–0.590) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 1 MiB | candidate | 7.344 (7.297–7.438) | 1.641 (1.484–1.781) | 2.383 (2.359–2.461) | 0.246 (0.133–0.367) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 10 MiB | baseline | 7.828 (7.781–8.016) | 2.016 (1.906–2.078) | 2.703 (2.379–2.742) | 0.496 (0.332–0.816) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 10 MiB | candidate | 7.703 (7.703–7.953) | 2.016 (1.891–2.062) | 2.227 (2.152–2.512) | 0.324 (0.098–0.496) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 100 MiB | baseline | 8.438 (8.328–8.609) | 2.375 (2.312–2.438) | 2.562 (2.438–2.645) | 0.461 (0.082–0.793) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 100 MiB | candidate | 8.469 (8.312–8.625) | 2.312 (2.234–2.484) | 2.230 (2.191–2.559) | 0.270 (0.070–0.344) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 500 MiB | baseline | 10.391 (10.328–10.516) | 3.953 (3.906–4.031) | 2.680 (2.609–2.809) | 0.508 (0.348–0.719) | 0.000 (0.000–0.000) |
| `overwrite-fixed-64k-chunk-count-decrease` | 500 MiB | candidate | 10.453 (10.219–10.656) | 3.953 (3.766–4.078) | 2.293 (1.988–2.535) | 0.152 (0.000–0.277) | 0.000 (0.000–0.000) |

Aggregate verifier receipts: 12.

Candidate size parity, matched-operation parity, route, CDC, spool, transaction, memory, cleanup, and custody gates are admission-binding. Baseline latency parity is diagnostic; baseline correctness, route, resource, cleanup, and custody remain binding.

## Per-sample resource and mechanism guards

All maxima below cover every retained sample, not only medians. Swap/OOM, FUSE mutation bytes, and spool must be zero; coverage and cleanup must pass. The 112 MiB target is diagnostic; 128 MiB is the unchanged hard ceiling.

| Operation | MiB | Arm | Lifetime RSS / cgroup max MiB | RSS / cgroup max gap ms | Minimum RSS / cgroup samples | CDC bytes min–max | Candidate bytes max | Spool bytes max | 112 MiB target |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| overwrite-fixed-64k-chunk-count-preserve | 1 | baseline | 7.953 / 4.500 | 0.084 / 1.096 | 30013 / 3001 | 65536–65536 | 68300 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 1 | candidate | 8.172 / 4.230 | 0.436 / 3.589 | 5333 / 882 | 65536–65536 | 68300 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 10 | baseline | 8.484 / 4.297 | 0.135 / 1.062 | 36838 / 3704 | 65536–65536 | 76800 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 10 | candidate | 8.297 / 4.191 | 0.055 / 2.157 | 6919 / 990 | 65536–65536 | 76800 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 100 | baseline | 8.828 / 4.406 | 0.091 / 0.838 | 31866 / 3300 | 65536–65536 | 78624 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 100 | candidate | 8.875 / 4.238 | 0.087 / 2.777 | 7891 / 1511 | 65536–65536 | 78624 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 500 | baseline | 10.734 / 4.797 | 0.114 / 2.906 | 32232 / 2953 | 65536–65536 | 86872 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-preserve | 500 | candidate | 10.844 / 5.996 | 0.205 / 0.503 | 20493 / 2250 | 65536–65536 | 86872 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 1 | baseline | 8.297 / 4.250 | 0.168 / 2.780 | 30737 / 2502 | 65536–65536 | 68361 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 1 | candidate | 8.156 / 4.160 | 0.044 / 2.644 | 5894 / 583 | 65536–65536 | 68361 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 10 | baseline | 8.516 / 4.633 | 0.092 / 3.443 | 29518 / 2745 | 65536–65536 | 76861 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 10 | candidate | 8.391 / 4.504 | 0.090 / 1.263 | 5393 / 1236 | 65536–65536 | 76861 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 100 | baseline | 8.875 / 4.277 | 0.087 / 2.049 | 29951 / 2924 | 65536–65536 | 78685 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 100 | candidate | 9.047 / 4.504 | 0.051 / 0.994 | 9101 / 1242 | 65536–65536 | 78685 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 500 | baseline | 10.969 / 6.152 | 0.238 / 1.471 | 41065 / 3332 | 65536–65536 | 86933 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-increase | 500 | candidate | 10.922 / 4.066 | 0.073 / 0.971 | 14447 / 1872 | 65536–65536 | 86933 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 1 | baseline | 7.859 / 4.469 | 0.063 / 1.920 | 22827 / 2217 | 65536–65536 | 35450 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 1 | candidate | 7.656 / 4.043 | 0.039 / 0.889 | 6306 / 1306 | 65536–65536 | 35450 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 10 | baseline | 8.156 / 4.230 | 0.138 / 1.319 | 27942 / 2522 | 65536–65536 | 43950 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 10 | candidate | 8.156 / 4.844 | 0.071 / 1.987 | 6515 / 856 | 65536–65536 | 43950 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 100 | baseline | 8.781 / 4.449 | 0.174 / 1.428 | 32305 / 3041 | 65536–65536 | 45774 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 100 | candidate | 8.797 / 4.293 | 0.670 / 4.805 | 11110 / 1697 | 65536–65536 | 45774 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 500 | baseline | 10.641 / 4.656 | 0.156 / 1.048 | 35518 / 4614 | 65536–65536 | 54022 | 0 | target-pass |
| overwrite-fixed-64k-chunk-count-decrease | 500 | candidate | 10.812 / 4.840 | 0.289 / 1.924 | 16314 / 1251 | 65536–65536 | 54022 | 0 | target-pass |

## Size parity

Ratios use the 1 MiB median as denominator; spread and allowance are independently evaluated for each metric.

| Operation | Arm | Metric | 10/1 | 100/1 | 500/1 | Spread / allowance ms | Status |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| overwrite-fixed-64k-chunk-count-preserve | baseline | edit_call_ns | 1.080 | 0.921 | 0.736 | 6.667 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-preserve | baseline | commit_call_ns | 0.774 | 0.842 | 1.877 | 7.486 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-preserve | baseline | edit_commit_ns | 1.071 | 0.908 | 1.038 | 4.163 / 2.326 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-preserve | candidate | edit_call_ns | 1.018 | 1.144 | 0.916 | 0.633 / 2.000 | pass |
| overwrite-fixed-64k-chunk-count-preserve | candidate | commit_call_ns | 1.054 | 1.301 | 3.058 | 8.091 / 2.000 | fail |
| overwrite-fixed-64k-chunk-count-preserve | candidate | edit_commit_ns | 0.923 | 1.200 | 2.170 | 8.375 / 2.000 | fail |
| overwrite-fixed-64k-chunk-count-increase | baseline | edit_call_ns | 1.347 | 1.385 | 0.889 | 8.986 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-increase | baseline | commit_call_ns | 1.351 | 1.440 | 3.431 | 9.359 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-increase | baseline | edit_commit_ns | 1.378 | 1.383 | 1.374 | 8.372 / 2.188 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-increase | candidate | edit_call_ns | 1.466 | 0.904 | 1.332 | 1.041 / 2.000 | pass |
| overwrite-fixed-64k-chunk-count-increase | candidate | commit_call_ns | 1.067 | 1.587 | 3.753 | 8.414 / 2.000 | fail |
| overwrite-fixed-64k-chunk-count-increase | candidate | edit_commit_ns | 1.237 | 1.368 | 2.566 | 7.939 / 2.000 | fail |
| overwrite-fixed-64k-chunk-count-decrease | baseline | edit_call_ns | 0.977 | 0.848 | 0.838 | 3.164 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-decrease | baseline | commit_call_ns | 1.420 | 1.948 | 4.438 | 8.943 / 2.000 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-decrease | baseline | edit_commit_ns | 1.020 | 0.990 | 1.225 | 5.194 / 2.195 | fail-diagnostic |
| overwrite-fixed-64k-chunk-count-decrease | candidate | edit_call_ns | 0.813 | 1.562 | 1.161 | 1.496 / 2.000 | pass |
| overwrite-fixed-64k-chunk-count-decrease | candidate | commit_call_ns | 1.141 | 2.409 | 3.390 | 7.370 / 2.000 | fail |
| overwrite-fixed-64k-chunk-count-decrease | candidate | edit_commit_ns | 1.020 | 2.070 | 2.666 | 8.398 / 2.000 | fail |

## Matched-operation parity

| Cohort | MiB | Metric | Medians ms | Status |
| --- | ---: | --- | --- | --- |

## Canonical controls

All five repetitions and both arms are checked for fixture/root/range/replacement-length/topology/timing identity. Unique payload objects are not extent count.

| Scenario | C0 | C1 | Delta | Unique payload objects | Mapping nodes / level | Status |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1 | 54 | 54 | +0 | 54 | 2 / 0 | fail |
| overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1 | 544 | 544 | +0 | 544 | 8 / 1 | fail |
| overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1 | 5394 | 5394 | +0 | 5394 | 46 / 1 | fail |
| overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1 | 26995 | 26995 | +0 | 26995 | 216 / 2 | fail |
| overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1 | 54 | 55 | +1 | 55 | 2 / 0 | fail |
| overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1 | 544 | 545 | +1 | 545 | 8 / 1 | fail |
| overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1 | 5394 | 5395 | +1 | 5395 | 46 / 1 | fail |
| overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1 | 26995 | 26996 | +1 | 26996 | 216 / 2 | fail |
| overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1 | 54 | 53 | -1 | 52 | 2 / 0 | fail |
| overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1 | 544 | 543 | -1 | 542 | 8 / 1 | fail |
| overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1 | 5394 | 5393 | -1 | 5392 | 46 / 1 | fail |
| overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1 | 26995 | 26994 | -1 | 26993 | 216 / 2 | fail |

## Untimed preparation

| MiB | Cache disposition | Build ms | Validation ms | Acquisition ms | Cache key |
| ---: | --- | ---: | ---: | ---: | --- |
| 1 | hit | 0.000 | 9.397 | 39.623 | 61a6d6fbd6c36f4bf99c3c7241e7a5d890d0cc1dfbe9458de57d8b7c81e478c0 |
| 10 | hit | 0.000 | 17.458 | 41.859 | 3d6d2fc2e32570958c9f55e27668df2d3ac9f000b9fbbcb6a5d0fd13a6cb1b6d |
| 100 | hit | 0.000 | 100.598 | 125.145 | 1cdd2d79fdf5ea406a09d56ab7a377856eb8406e7ffc5ccf6867e4e828507807 |
| 500 | hit | 0.000 | 639.523 | 666.916 | 57b81a56f638ef88f2205408d98b9a0a3ff5e9f6727e4eb5031c3665f7872ff1 |

Qualification and clone setup are retained in [qualification timing](environment/qualification-timing.tsv); each raw row records its clone method/digest/wall, container-start wall, and clock_sampler_start_ns for authenticated connection and sampler warmup. These are never part of edit or Commit latency. Cgroup observation uses an acknowledged broader window with no clock probes. Exact phase attribution and continuous category maxima are unavailable; actual gaps are reported diagnostically.

Pre-run manifest SHA-256: 5a4b8fd2c8dbe4ec2838e35f6956630a1f46b3c745585d71b7ac542f09e5a9f7. The enclosing evidence manifest identity is shown by the cross-family report.

## Failures

- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r1:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r1:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r2:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r2:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r3:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r3:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r4:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r4:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1:r5:candidate observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r5:baseline observation scope
- edit_canonical_chunk_count:overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1:r5:candidate observation scope
- candidate overwrite-fixed-64k-chunk-count-preserve commit_call_ns size parity
- candidate overwrite-fixed-64k-chunk-count-preserve edit_commit_ns size parity
- candidate overwrite-fixed-64k-chunk-count-increase commit_call_ns size parity
- candidate overwrite-fixed-64k-chunk-count-increase edit_commit_ns size parity
- candidate overwrite-fixed-64k-chunk-count-decrease commit_call_ns size parity
- candidate overwrite-fixed-64k-chunk-count-decrease edit_commit_ns size parity
- verification overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1 candidate observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1 baseline observation scope
- verification overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1 candidate observation scope

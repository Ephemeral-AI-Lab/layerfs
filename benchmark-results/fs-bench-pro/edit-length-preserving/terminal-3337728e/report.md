# edit_length_preserving SDK-only edit benchmark

Status: **FAIL**

Raw evidence: [performance JSONL](performance/raw.jsonl), [verification aggregates](verification/raw.jsonl), [source subproofs](verification/subproofs.jsonl).

## Latency

| Operation | Size | Source | Samples | Edit median (min–max) ms | Commit median (min–max) ms | Edit+Commit median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| `overwrite-head-4k` | 1 MiB | baseline | 5 | 18.590 (15.186–23.099) | 2.978 (1.975–3.346) | 21.937 (17.973–26.077) |
| `overwrite-head-4k` | 1 MiB | candidate | 5 | 2.643 (1.376–4.508) | 2.170 (2.113–4.158) | 5.402 (4.346–7.905) |
| `overwrite-head-4k` | 10 MiB | baseline | 5 | 16.663 (13.058–32.213) | 2.704 (2.325–3.847) | 19.174 (15.382–35.289) |
| `overwrite-head-4k` | 10 MiB | candidate | 5 | 3.416 (1.585–5.413) | 2.602 (2.569–5.988) | 5.985 (4.492–9.936) |
| `overwrite-head-4k` | 100 MiB | baseline | 5 | 20.512 (16.652–23.592) | 7.650 (5.428–17.899) | 27.750 (22.516–38.411) |
| `overwrite-head-4k` | 100 MiB | candidate | 5 | 2.727 (1.892–5.255) | 4.405 (3.725–7.485) | 6.928 (6.602–9.966) |
| `overwrite-head-4k` | 500 MiB | baseline | 5 | 16.151 (13.236–19.496) | 10.470 (7.359–81.480) | 26.755 (21.965–97.631) |
| `overwrite-head-4k` | 500 MiB | candidate | 5 | 2.602 (1.537–2.947) | 9.586 (7.312–11.187) | 12.188 (9.129–13.165) |
| `overwrite-middle-4k` | 1 MiB | baseline | 5 | 15.299 (12.221–53.441) | 3.447 (2.728–33.722) | 18.511 (15.668–87.163) |
| `overwrite-middle-4k` | 1 MiB | candidate | 5 | 1.990 (1.296–5.913) | 2.779 (2.246–3.795) | 5.409 (4.075–9.307) |
| `overwrite-middle-4k` | 10 MiB | baseline | 5 | 16.661 (15.729–22.909) | 3.451 (2.832–4.014) | 20.035 (18.560–26.503) |
| `overwrite-middle-4k` | 10 MiB | candidate | 5 | 1.939 (1.250–8.074) | 3.176 (2.663–5.210) | 5.115 (4.206–13.284) |
| `overwrite-middle-4k` | 100 MiB | baseline | 5 | 22.082 (16.944–31.522) | 6.678 (3.489–12.365) | 27.641 (20.433–43.887) |
| `overwrite-middle-4k` | 100 MiB | candidate | 5 | 2.888 (1.741–4.774) | 5.503 (4.722–9.054) | 8.238 (6.463–11.971) |
| `overwrite-middle-4k` | 500 MiB | baseline | 5 | 15.960 (14.703–18.062) | 11.097 (7.545–13.610) | 26.769 (23.504–31.672) |
| `overwrite-middle-4k` | 500 MiB | candidate | 5 | 2.650 (1.282–4.089) | 11.146 (7.567–11.940) | 12.685 (10.217–15.235) |
| `overwrite-tail-4k` | 1 MiB | baseline | 5 | 23.396 (17.152–35.623) | 2.767 (2.226–3.452) | 25.622 (19.919–38.817) |
| `overwrite-tail-4k` | 1 MiB | candidate | 5 | 1.527 (1.120–3.012) | 2.295 (2.093–3.285) | 3.770 (3.501–5.390) |
| `overwrite-tail-4k` | 10 MiB | baseline | 5 | 15.929 (13.673–19.579) | 2.900 (2.442–3.141) | 19.041 (16.813–22.021) |
| `overwrite-tail-4k` | 10 MiB | candidate | 5 | 1.815 (1.306–2.788) | 2.490 (2.160–2.977) | 4.341 (3.745–5.278) |
| `overwrite-tail-4k` | 100 MiB | baseline | 5 | 17.000 (14.582–19.642) | 4.848 (3.555–4.983) | 20.727 (19.430–24.508) |
| `overwrite-tail-4k` | 100 MiB | candidate | 5 | 1.972 (1.258–4.043) | 4.101 (3.409–8.520) | 6.899 (5.359–10.617) |
| `overwrite-tail-4k` | 500 MiB | baseline | 5 | 14.266 (11.147–20.241) | 10.519 (6.759–19.236) | 23.120 (17.907–33.501) |
| `overwrite-tail-4k` | 500 MiB | candidate | 5 | 2.508 (1.579–4.412) | 9.751 (7.279–11.877) | 11.331 (9.788–16.288) |

Nominal targets are 10/10/20 ms; user-approved accepted ceilings are 20/20/30 ms for Edit/Commit/combined. Combined is independently capped at 30 ms. Parity and resource gates are unchanged.

Memory profile: ack-window-v1. Cgroup observations cover an acknowledged broader window, not exact T0–T3. Native peaks are whole-worker/container lifetime bounds. Category maxima, dirty/writeback, and transient swap checks are sampled observations; continuous category ceilings cannot be strictly proven. Gaps are reported diagnostically. Native peak/incremental/size-spread limits and zero OOM remain binding.

| Candidate scenario | Latency classification |
| --- | --- |
| `overwrite-head-4k-on-1mib-ops-1` | nominal-pass |
| `overwrite-head-4k-on-10mib-ops-1` | nominal-pass |
| `overwrite-head-4k-on-100mib-ops-1` | nominal-pass |
| `overwrite-head-4k-on-500mib-ops-1` | nominal-pass |
| `overwrite-middle-4k-on-1mib-ops-1` | nominal-pass |
| `overwrite-middle-4k-on-10mib-ops-1` | nominal-pass |
| `overwrite-middle-4k-on-100mib-ops-1` | nominal-pass |
| `overwrite-middle-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `overwrite-tail-4k-on-1mib-ops-1` | nominal-pass |
| `overwrite-tail-4k-on-10mib-ops-1` | nominal-pass |
| `overwrite-tail-4k-on-100mib-ops-1` | nominal-pass |
| `overwrite-tail-4k-on-500mib-ops-1` | nominal-pass |

## Memory

| Operation | Size | Source | Process phase MiB median (min–max) | Process incremental MiB median (min–max) | Cgroup sampled window MiB median (min–max) | Cgroup sampled window incremental MiB median (min–max) | Dirty/writeback incremental MiB median (min–max) |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| `overwrite-head-4k` | 1 MiB | baseline | 6.938 (6.859–7.062) | 1.188 (1.172–1.250) | 2.504 (2.402–2.797) | 0.590 (0.223–0.715) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 1 MiB | candidate | 6.828 (6.797–7.125) | 1.172 (1.125–1.203) | 2.184 (2.027–2.480) | 0.125 (0.000–0.254) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 10 MiB | baseline | 7.188 (7.078–7.391) | 1.500 (1.453–1.516) | 2.566 (2.410–2.945) | 0.535 (0.203–0.879) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 10 MiB | candidate | 7.250 (7.109–7.406) | 1.484 (1.438–1.500) | 2.344 (2.203–2.496) | 0.219 (0.055–0.574) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 100 MiB | baseline | 7.922 (7.891–7.938) | 1.734 (1.703–1.906) | 2.539 (2.516–2.930) | 0.559 (0.395–1.008) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 100 MiB | candidate | 7.812 (7.719–8.000) | 1.844 (1.703–1.859) | 2.438 (2.008–2.496) | 0.289 (0.086–0.461) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 500 MiB | baseline | 9.547 (9.391–9.578) | 2.953 (2.875–3.000) | 2.578 (2.414–3.766) | 0.387 (0.066–1.832) | 0.000 (0.000–0.000) |
| `overwrite-head-4k` | 500 MiB | candidate | 9.562 (9.547–9.641) | 2.906 (2.859–2.969) | 2.320 (2.254–2.402) | 0.043 (0.000–0.355) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 1 MiB | baseline | 6.953 (6.875–7.188) | 1.250 (1.234–1.297) | 2.621 (2.480–2.641) | 0.355 (0.203–0.414) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 1 MiB | candidate | 7.031 (6.812–7.031) | 1.156 (1.156–1.172) | 2.414 (2.242–2.520) | 0.340 (0.246–0.520) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 10 MiB | baseline | 7.266 (7.156–7.438) | 1.562 (1.516–1.609) | 2.637 (2.320–2.750) | 0.488 (0.242–0.586) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 10 MiB | candidate | 7.375 (7.141–7.500) | 1.500 (1.500–1.578) | 2.258 (2.219–2.508) | 0.254 (0.039–0.383) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 100 MiB | baseline | 7.984 (7.953–8.188) | 2.000 (1.922–2.016) | 2.594 (2.402–2.652) | 0.375 (0.047–0.469) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 100 MiB | candidate | 8.078 (7.859–8.234) | 1.891 (1.859–1.969) | 2.266 (2.102–2.395) | 0.000 (0.000–0.340) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 500 MiB | baseline | 9.812 (9.734–10.047) | 3.312 (3.219–3.359) | 2.680 (2.461–2.898) | 0.473 (0.449–0.930) | 0.000 (0.000–0.000) |
| `overwrite-middle-4k` | 500 MiB | candidate | 9.766 (9.656–9.953) | 3.234 (3.125–3.266) | 2.367 (2.160–2.527) | 0.277 (0.000–0.332) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 1 MiB | baseline | 7.016 (6.828–7.031) | 1.141 (1.078–1.219) | 2.641 (2.551–2.656) | 0.383 (0.133–0.598) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 1 MiB | candidate | 7.016 (6.781–7.031) | 1.125 (1.062–1.188) | 2.332 (2.211–2.477) | 0.262 (0.250–0.574) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 10 MiB | baseline | 6.969 (6.922–7.125) | 1.250 (1.109–1.297) | 2.504 (2.195–2.801) | 0.270 (0.172–0.758) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 10 MiB | candidate | 6.922 (6.781–7.078) | 1.188 (1.094–1.297) | 2.441 (2.238–2.637) | 0.164 (0.000–0.301) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 100 MiB | baseline | 7.625 (7.438–7.734) | 1.469 (1.453–1.531) | 2.527 (2.305–2.758) | 0.484 (0.000–0.648) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 100 MiB | candidate | 7.516 (7.453–7.781) | 1.500 (1.391–1.609) | 2.477 (2.184–2.555) | 0.309 (0.141–0.383) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 500 MiB | baseline | 9.453 (9.109–9.500) | 2.750 (2.688–2.766) | 2.641 (2.508–2.660) | 0.500 (0.363–0.746) | 0.000 (0.000–0.000) |
| `overwrite-tail-4k` | 500 MiB | candidate | 9.266 (9.156–9.484) | 2.719 (2.625–2.812) | 2.484 (2.328–2.562) | 0.180 (0.000–0.391) | 0.000 (0.000–0.000) |

Aggregate verifier receipts: 12.

Candidate size parity, matched-operation parity, route, CDC, spool, transaction, memory, cleanup, and custody gates are admission-binding. Baseline latency parity is diagnostic; baseline correctness, route, resource, cleanup, and custody remain binding.

## Per-sample resource and mechanism guards

All maxima below cover every retained sample, not only medians. Swap/OOM, FUSE mutation bytes, and spool must be zero; coverage and cleanup must pass. The 112 MiB target is diagnostic; 128 MiB is the unchanged hard ceiling.

| Operation | MiB | Arm | Lifetime RSS / cgroup max MiB | RSS / cgroup max gap ms | Minimum RSS / cgroup samples | CDC bytes min–max | Candidate bytes max | Spool bytes max | 112 MiB target |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| overwrite-head-4k | 1 | baseline | 7.219 / 7.070 | 0.108 / 0.798 | 27584 / 3219 | 4096–4096 | 6858 | 0 | target-pass |
| overwrite-head-4k | 1 | candidate | 7.297 / 4.977 | 0.027 / 0.471 | 5705 / 1195 | 4096–4096 | 6858 | 0 | target-pass |
| overwrite-head-4k | 10 | baseline | 7.562 / 6.816 | 0.082 / 1.956 | 24526 / 3065 | 4096–4096 | 15358 | 0 | target-pass |
| overwrite-head-4k | 10 | candidate | 7.656 / 6.652 | 0.160 / 1.323 | 6810 / 1170 | 4096–4096 | 15358 | 0 | target-pass |
| overwrite-head-4k | 100 | baseline | 8.125 / 6.590 | 0.255 / 2.115 | 32176 / 2544 | 4096–4096 | 17182 | 0 | target-pass |
| overwrite-head-4k | 100 | candidate | 8.141 / 6.430 | 0.051 / 2.466 | 9110 / 1667 | 4096–4096 | 17182 | 0 | target-pass |
| overwrite-head-4k | 500 | baseline | 9.750 / 6.137 | 12.465 / 3.111 | 34262 / 3184 | 4096–4096 | 25430 | 0 | target-pass |
| overwrite-head-4k | 500 | candidate | 9.812 / 6.191 | 0.165 / 0.623 | 10995 / 2093 | 4096–4096 | 25430 | 0 | target-pass |
| overwrite-middle-4k | 1 | baseline | 7.344 / 4.957 | 3.915 / 11.554 | 25664 / 1990 | 4096–4096 | 6898 | 0 | target-pass |
| overwrite-middle-4k | 1 | candidate | 7.219 / 6.125 | 0.034 / 0.837 | 5928 / 1687 | 4096–4096 | 6898 | 0 | target-pass |
| overwrite-middle-4k | 10 | baseline | 7.578 / 6.473 | 0.066 / 1.973 | 32257 / 3096 | 4096–4096 | 18642 | 0 | target-pass |
| overwrite-middle-4k | 10 | candidate | 7.656 / 4.695 | 0.069 / 2.044 | 6343 / 981 | 4096–4096 | 18642 | 0 | target-pass |
| overwrite-middle-4k | 100 | baseline | 8.312 / 6.457 | 0.400 / 2.140 | 31566 / 3351 | 4096–4096 | 22386 | 0 | target-pass |
| overwrite-middle-4k | 100 | candidate | 8.391 / 6.391 | 0.071 / 0.691 | 9105 / 1751 | 4096–4096 | 22386 | 0 | target-pass |
| overwrite-middle-4k | 500 | baseline | 10.172 / 6.227 | 0.073 / 0.604 | 33157 / 3831 | 4096–4096 | 30634 | 0 | target-pass |
| overwrite-middle-4k | 500 | candidate | 10.141 / 6.336 | 0.051 / 1.290 | 14261 / 1969 | 4096–4096 | 30634 | 0 | target-pass |
| overwrite-tail-4k | 1 | baseline | 7.203 / 4.992 | 0.090 / 4.045 | 31291 / 2902 | 4096–4096 | 6858 | 0 | target-pass |
| overwrite-tail-4k | 1 | candidate | 7.250 / 4.570 | 0.032 / 1.059 | 4358 / 968 | 4096–4096 | 6858 | 0 | target-pass |
| overwrite-tail-4k | 10 | baseline | 7.281 / 6.824 | 0.087 / 0.685 | 23831 / 2692 | 4096–4096 | 11426 | 0 | target-pass |
| overwrite-tail-4k | 10 | candidate | 7.266 / 4.590 | 0.026 / 1.538 | 6030 / 1513 | 4096–4096 | 11426 | 0 | target-pass |
| overwrite-tail-4k | 100 | baseline | 7.906 / 6.387 | 0.150 / 1.164 | 31363 / 2680 | 4096–4096 | 12690 | 0 | target-pass |
| overwrite-tail-4k | 100 | candidate | 7.984 / 4.633 | 0.125 / 3.160 | 7659 / 1294 | 4096–4096 | 12690 | 0 | target-pass |
| overwrite-tail-4k | 500 | baseline | 9.672 / 6.816 | 0.055 / 1.260 | 28189 / 3171 | 4096–4096 | 24778 | 0 | target-pass |
| overwrite-tail-4k | 500 | candidate | 9.703 / 6.410 | 0.111 / 2.688 | 15131 / 1694 | 4096–4096 | 24778 | 0 | target-pass |

## Size parity

Ratios use the 1 MiB median as denominator; spread and allowance are independently evaluated for each metric.

| Operation | Arm | Metric | 10/1 | 100/1 | 500/1 | Spread / allowance ms | Status |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| overwrite-head-4k | baseline | edit_call_ns | 0.896 | 1.103 | 0.869 | 4.361 / 2.000 | fail-diagnostic |
| overwrite-head-4k | baseline | commit_call_ns | 0.908 | 2.569 | 3.516 | 7.766 / 2.000 | fail-diagnostic |
| overwrite-head-4k | baseline | edit_commit_ns | 0.874 | 1.265 | 1.220 | 8.575 / 2.000 | fail-diagnostic |
| overwrite-head-4k | candidate | edit_call_ns | 1.292 | 1.032 | 0.984 | 0.814 / 2.000 | pass |
| overwrite-head-4k | candidate | commit_call_ns | 1.199 | 2.030 | 4.418 | 7.416 / 2.000 | fail |
| overwrite-head-4k | candidate | edit_commit_ns | 1.108 | 1.283 | 2.256 | 6.786 / 2.000 | fail |
| overwrite-middle-4k | baseline | edit_call_ns | 1.089 | 1.443 | 1.043 | 6.783 / 2.000 | fail-diagnostic |
| overwrite-middle-4k | baseline | commit_call_ns | 1.001 | 1.937 | 3.219 | 7.649 / 2.000 | fail-diagnostic |
| overwrite-middle-4k | baseline | edit_commit_ns | 1.082 | 1.493 | 1.446 | 9.130 / 2.000 | fail-diagnostic |
| overwrite-middle-4k | candidate | edit_call_ns | 0.975 | 1.451 | 1.332 | 0.949 / 2.000 | pass |
| overwrite-middle-4k | candidate | commit_call_ns | 1.143 | 1.980 | 4.011 | 8.367 / 2.000 | fail |
| overwrite-middle-4k | candidate | edit_commit_ns | 0.946 | 1.523 | 2.345 | 7.571 / 2.000 | fail |
| overwrite-tail-4k | baseline | edit_call_ns | 0.681 | 0.727 | 0.610 | 9.130 / 2.000 | fail-diagnostic |
| overwrite-tail-4k | baseline | commit_call_ns | 1.048 | 1.752 | 3.802 | 7.752 / 2.000 | fail-diagnostic |
| overwrite-tail-4k | baseline | edit_commit_ns | 0.743 | 0.809 | 0.902 | 6.581 / 2.000 | fail-diagnostic |
| overwrite-tail-4k | candidate | edit_call_ns | 1.188 | 1.291 | 1.642 | 0.981 / 2.000 | pass |
| overwrite-tail-4k | candidate | commit_call_ns | 1.085 | 1.787 | 4.249 | 7.456 / 2.000 | fail |
| overwrite-tail-4k | candidate | edit_commit_ns | 1.151 | 1.830 | 3.005 | 7.561 / 2.000 | fail |

## Matched-operation parity

| Cohort | MiB | Metric | Medians ms | Status |
| --- | ---: | --- | --- | --- |
| overwrite-position | 1 | edit_call_ns | 2.643, 1.990, 1.527 | pass |
| overwrite-position | 1 | commit_call_ns | 2.170, 2.779, 2.295 | pass |
| overwrite-position | 1 | edit_commit_ns | 5.402, 5.409, 3.770 | pass |
| overwrite-position | 10 | edit_call_ns | 3.416, 1.939, 1.815 | pass |
| overwrite-position | 10 | commit_call_ns | 2.602, 3.176, 2.490 | pass |
| overwrite-position | 10 | edit_commit_ns | 5.985, 5.115, 4.341 | pass |
| overwrite-position | 100 | edit_call_ns | 2.727, 2.888, 1.972 | pass |
| overwrite-position | 100 | commit_call_ns | 4.405, 5.503, 4.101 | pass |
| overwrite-position | 100 | edit_commit_ns | 6.928, 8.238, 6.899 | pass |
| overwrite-position | 500 | edit_call_ns | 2.602, 2.650, 2.508 | pass |
| overwrite-position | 500 | commit_call_ns | 9.586, 11.146, 9.751 | pass |
| overwrite-position | 500 | edit_commit_ns | 12.188, 12.685, 11.331 | pass |

## Untimed preparation

| MiB | Cache disposition | Build ms | Validation ms | Acquisition ms | Cache key |
| ---: | --- | ---: | ---: | ---: | --- |
| 1 | hit | 0.000 | 6.890 | 30.930 | 61a6d6fbd6c36f4bf99c3c7241e7a5d890d0cc1dfbe9458de57d8b7c81e478c0 |
| 10 | hit | 0.000 | 14.015 | 34.194 | 3d6d2fc2e32570958c9f55e27668df2d3ac9f000b9fbbcb6a5d0fd13a6cb1b6d |
| 100 | hit | 0.000 | 88.618 | 106.518 | 1cdd2d79fdf5ea406a09d56ab7a377856eb8406e7ffc5ccf6867e4e828507807 |
| 500 | hit | 0.000 | 427.407 | 445.157 | 57b81a56f638ef88f2205408d98b9a0a3ff5e9f6727e4eb5031c3665f7872ff1 |

Qualification and clone setup are retained in [qualification timing](environment/qualification-timing.tsv); each raw row records its clone method/digest/wall, container-start wall, and clock_sampler_start_ns for authenticated connection and sampler warmup. These are never part of edit or Commit latency. Cgroup observation uses an acknowledged broader window with no clock probes. Exact phase attribution and continuous category maxima are unavailable; actual gaps are reported diagnostically.

Pre-run manifest SHA-256: b0beeafd19948a51578b57bc91ca7434e7a907091f5d97abe0abe377204132b4. The enclosing evidence manifest identity is shown by the cross-family report.

## Failures

- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-tail-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-head-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_preserving:overwrite-middle-4k-on-500mib-ops-1:r5:candidate observation scope
- candidate overwrite-head-4k commit_call_ns size parity
- candidate overwrite-head-4k edit_commit_ns size parity
- candidate overwrite-middle-4k commit_call_ns size parity
- candidate overwrite-middle-4k edit_commit_ns size parity
- candidate overwrite-tail-4k commit_call_ns size parity
- candidate overwrite-tail-4k edit_commit_ns size parity
- verification overwrite-head-4k-on-1mib-ops-1 baseline observation scope
- verification overwrite-head-4k-on-1mib-ops-1 candidate observation scope
- verification overwrite-head-4k-on-10mib-ops-1 baseline observation scope
- verification overwrite-head-4k-on-10mib-ops-1 candidate observation scope
- verification overwrite-head-4k-on-100mib-ops-1 baseline observation scope
- verification overwrite-head-4k-on-100mib-ops-1 candidate observation scope
- verification overwrite-head-4k-on-500mib-ops-1 baseline observation scope
- verification overwrite-head-4k-on-500mib-ops-1 candidate observation scope
- verification overwrite-middle-4k-on-1mib-ops-1 baseline observation scope
- verification overwrite-middle-4k-on-1mib-ops-1 candidate observation scope
- verification overwrite-middle-4k-on-10mib-ops-1 baseline observation scope
- verification overwrite-middle-4k-on-10mib-ops-1 candidate observation scope
- verification overwrite-middle-4k-on-100mib-ops-1 baseline observation scope
- verification overwrite-middle-4k-on-100mib-ops-1 candidate observation scope
- verification overwrite-middle-4k-on-500mib-ops-1 baseline observation scope
- verification overwrite-middle-4k-on-500mib-ops-1 candidate observation scope
- verification overwrite-tail-4k-on-1mib-ops-1 baseline observation scope
- verification overwrite-tail-4k-on-1mib-ops-1 candidate observation scope
- verification overwrite-tail-4k-on-10mib-ops-1 baseline observation scope
- verification overwrite-tail-4k-on-10mib-ops-1 candidate observation scope
- verification overwrite-tail-4k-on-100mib-ops-1 baseline observation scope
- verification overwrite-tail-4k-on-100mib-ops-1 candidate observation scope
- verification overwrite-tail-4k-on-500mib-ops-1 baseline observation scope
- verification overwrite-tail-4k-on-500mib-ops-1 candidate observation scope

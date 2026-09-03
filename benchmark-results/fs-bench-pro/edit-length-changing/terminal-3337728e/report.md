# edit_length_changing SDK-only edit benchmark

Status: **FAIL**

Raw evidence: [performance JSONL](performance/raw.jsonl), [verification aggregates](verification/raw.jsonl), [source subproofs](verification/subproofs.jsonl).

## Latency

| Operation | Size | Source | Samples | Edit median (min–max) ms | Commit median (min–max) ms | Edit+Commit median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| `insert-middle-4k` | 1 MiB | baseline | 5 | 24.814 (18.951–25.282) | 2.681 (2.402–4.055) | 27.366 (21.572–28.094) |
| `insert-middle-4k` | 1 MiB | candidate | 5 | 2.502 (1.545–4.522) | 2.294 (2.030–3.878) | 4.842 (4.436–7.284) |
| `insert-middle-4k` | 10 MiB | baseline | 5 | 18.915 (14.267–27.090) | 3.954 (2.457–5.051) | 22.272 (17.181–31.044) |
| `insert-middle-4k` | 10 MiB | candidate | 5 | 2.001 (1.455–8.639) | 3.273 (2.674–4.168) | 5.292 (4.674–12.806) |
| `insert-middle-4k` | 100 MiB | baseline | 5 | 16.771 (10.165–61.033) | 6.087 (3.553–15.576) | 21.734 (14.811–76.609) |
| `insert-middle-4k` | 100 MiB | candidate | 5 | 2.331 (1.873–2.586) | 5.495 (4.073–8.041) | 7.752 (6.404–9.913) |
| `insert-middle-4k` | 500 MiB | baseline | 5 | 22.361 (16.200–23.202) | 10.907 (8.051–16.538) | 31.406 (24.251–39.002) |
| `insert-middle-4k` | 500 MiB | candidate | 5 | 2.245 (1.761–5.089) | 11.293 (8.036–15.587) | 13.538 (9.797–20.677) |
| `delete-middle-4k` | 1 MiB | baseline | 5 | 24.424 (17.563–44.046) | 4.782 (2.842–7.762) | 29.640 (21.148–46.888) |
| `delete-middle-4k` | 1 MiB | candidate | 5 | 5.221 (1.234–7.767) | 2.701 (1.939–6.425) | 10.283 (3.757–11.646) |
| `delete-middle-4k` | 10 MiB | baseline | 5 | 19.302 (16.188–25.393) | 3.244 (2.814–4.558) | 22.402 (19.002–29.951) |
| `delete-middle-4k` | 10 MiB | candidate | 5 | 2.715 (1.985–7.124) | 2.803 (2.401–6.319) | 5.643 (5.041–11.268) |
| `delete-middle-4k` | 100 MiB | baseline | 5 | 16.537 (14.308–27.491) | 4.909 (4.229–8.993) | 21.064 (18.537–36.484) |
| `delete-middle-4k` | 100 MiB | candidate | 5 | 2.698 (1.362–3.504) | 5.913 (3.484–11.370) | 9.191 (4.952–14.069) |
| `delete-middle-4k` | 500 MiB | baseline | 5 | 26.682 (15.597–33.613) | 12.000 (7.716–16.570) | 39.815 (23.314–49.027) |
| `delete-middle-4k` | 500 MiB | candidate | 5 | 2.649 (2.350–3.335) | 11.412 (7.259–16.404) | 14.006 (9.908–18.832) |
| `append-tail-4k` | 1 MiB | baseline | 5 | 18.924 (17.269–20.710) | 2.872 (2.058–3.677) | 22.388 (20.036–23.582) |
| `append-tail-4k` | 1 MiB | candidate | 5 | 2.017 (1.272–2.503) | 3.603 (1.958–8.680) | 6.003 (3.975–9.952) |
| `append-tail-4k` | 10 MiB | baseline | 5 | 20.717 (16.156–22.825) | 4.047 (3.304–7.168) | 25.020 (19.460–29.993) |
| `append-tail-4k` | 10 MiB | candidate | 5 | 3.172 (2.084–4.082) | 3.101 (2.761–4.665) | 6.515 (5.032–7.184) |
| `append-tail-4k` | 100 MiB | baseline | 5 | 23.297 (18.321–36.841) | 4.548 (3.951–12.856) | 27.271 (22.272–49.697) |
| `append-tail-4k` | 100 MiB | candidate | 5 | 3.016 (2.486–4.854) | 4.891 (2.664–6.043) | 7.569 (5.150–10.896) |
| `append-tail-4k` | 500 MiB | baseline | 5 | 18.657 (15.149–25.877) | 12.159 (8.598–19.012) | 28.754 (25.595–41.330) |
| `append-tail-4k` | 500 MiB | candidate | 5 | 2.010 (1.484–3.408) | 12.354 (6.709–16.029) | 14.364 (8.193–17.655) |
| `prepend-head-4k` | 1 MiB | baseline | 5 | 20.612 (15.781–23.596) | 4.619 (2.653–7.266) | 23.854 (21.210–28.215) |
| `prepend-head-4k` | 1 MiB | candidate | 5 | 1.674 (1.415–3.975) | 2.919 (2.199–8.842) | 4.680 (4.058–10.286) |
| `prepend-head-4k` | 10 MiB | baseline | 5 | 19.372 (14.125–20.623) | 3.404 (2.782–5.024) | 23.405 (19.149–24.015) |
| `prepend-head-4k` | 10 MiB | candidate | 5 | 2.164 (1.308–3.062) | 2.779 (1.921–4.107) | 4.883 (3.230–6.271) |
| `prepend-head-4k` | 100 MiB | baseline | 5 | 19.278 (13.904–23.483) | 5.149 (4.788–5.922) | 24.727 (19.543–28.271) |
| `prepend-head-4k` | 100 MiB | candidate | 5 | 2.967 (1.933–7.272) | 4.289 (3.029–6.163) | 7.257 (4.963–11.718) |
| `prepend-head-4k` | 500 MiB | baseline | 5 | 20.187 (10.816–41.251) | 12.628 (10.603–16.659) | 30.790 (21.826–57.909) |
| `prepend-head-4k` | 500 MiB | candidate | 5 | 3.344 (1.970–4.394) | 11.122 (7.977–12.061) | 14.300 (9.947–16.455) |
| `replace-grow-middle-2k-to-4k` | 1 MiB | baseline | 5 | 17.243 (13.103–26.602) | 3.711 (2.037–5.069) | 19.577 (15.140–31.671) |
| `replace-grow-middle-2k-to-4k` | 1 MiB | candidate | 5 | 1.778 (1.441–2.332) | 2.366 (1.994–4.919) | 4.200 (3.834–6.465) |
| `replace-grow-middle-2k-to-4k` | 10 MiB | baseline | 5 | 21.532 (15.612–46.471) | 3.547 (3.109–11.296) | 24.640 (19.195–57.767) |
| `replace-grow-middle-2k-to-4k` | 10 MiB | candidate | 5 | 1.695 (1.214–4.444) | 5.355 (2.610–6.149) | 7.610 (3.825–9.799) |
| `replace-grow-middle-2k-to-4k` | 100 MiB | baseline | 5 | 18.513 (14.622–22.307) | 5.247 (4.744–8.156) | 23.257 (19.869–29.227) |
| `replace-grow-middle-2k-to-4k` | 100 MiB | candidate | 5 | 2.794 (1.522–4.955) | 6.368 (4.095–11.847) | 9.015 (7.400–16.654) |
| `replace-grow-middle-2k-to-4k` | 500 MiB | baseline | 5 | 18.146 (12.796–24.340) | 12.932 (11.782–13.736) | 31.078 (24.578–37.539) |
| `replace-grow-middle-2k-to-4k` | 500 MiB | candidate | 5 | 2.331 (1.783–6.046) | 10.855 (7.557–17.222) | 15.049 (9.878–20.960) |
| `replace-shrink-middle-4k-to-2k` | 1 MiB | baseline | 5 | 19.062 (17.017–27.435) | 3.394 (2.858–4.553) | 22.320 (20.836–30.829) |
| `replace-shrink-middle-4k-to-2k` | 1 MiB | candidate | 5 | 1.589 (1.297–1.865) | 2.333 (2.004–3.532) | 3.930 (3.347–4.830) |
| `replace-shrink-middle-4k-to-2k` | 10 MiB | baseline | 5 | 18.597 (14.375–20.552) | 3.891 (3.351–4.062) | 22.659 (17.726–24.182) |
| `replace-shrink-middle-4k-to-2k` | 10 MiB | candidate | 5 | 1.649 (1.170–2.656) | 3.836 (2.721–5.130) | 5.485 (4.300–7.786) |
| `replace-shrink-middle-4k-to-2k` | 100 MiB | baseline | 5 | 20.398 (15.683–24.791) | 5.470 (4.311–7.479) | 24.709 (20.295–32.270) |
| `replace-shrink-middle-4k-to-2k` | 100 MiB | candidate | 5 | 2.605 (1.272–3.349) | 5.246 (4.027–8.293) | 7.893 (6.518–10.354) |
| `replace-shrink-middle-4k-to-2k` | 500 MiB | baseline | 5 | 16.610 (11.799–21.233) | 11.204 (7.999–13.317) | 29.232 (23.003–31.654) |
| `replace-shrink-middle-4k-to-2k` | 500 MiB | candidate | 5 | 3.700 (2.197–4.853) | 11.994 (8.859–18.451) | 16.847 (11.358–22.370) |
| `truncate-tail-4k` | 1 MiB | baseline | 5 | 17.760 (14.549–21.605) | 2.439 (2.152–3.324) | 20.942 (17.493–23.756) |
| `truncate-tail-4k` | 1 MiB | candidate | 5 | 2.737 (1.625–3.997) | 2.517 (1.952–2.772) | 4.994 (4.023–6.769) |
| `truncate-tail-4k` | 10 MiB | baseline | 5 | 21.612 (15.818–29.061) | 3.260 (2.261–7.498) | 24.957 (18.080–36.559) |
| `truncate-tail-4k` | 10 MiB | candidate | 5 | 1.599 (1.414–2.506) | 2.444 (2.262–4.604) | 4.246 (3.676–6.202) |
| `truncate-tail-4k` | 100 MiB | baseline | 5 | 18.795 (16.123–23.784) | 5.586 (3.647–12.644) | 27.431 (23.573–30.647) |
| `truncate-tail-4k` | 100 MiB | candidate | 5 | 2.136 (1.371–4.338) | 3.866 (3.701–6.064) | 5.837 (5.544–8.520) |
| `truncate-tail-4k` | 500 MiB | baseline | 5 | 18.341 (17.383–27.923) | 10.080 (7.225–14.077) | 32.418 (28.110–35.819) |
| `truncate-tail-4k` | 500 MiB | candidate | 5 | 2.295 (1.456–2.622) | 9.157 (6.920–12.483) | 11.304 (9.543–14.778) |
| `zero-extend-tail-4k` | 1 MiB | baseline | 5 | 20.874 (18.291–30.216) | 3.342 (2.489–5.562) | 23.363 (21.633–35.779) |
| `zero-extend-tail-4k` | 1 MiB | candidate | 5 | 3.090 (1.363–3.803) | 2.687 (2.052–4.393) | 5.767 (4.035–7.483) |
| `zero-extend-tail-4k` | 10 MiB | baseline | 5 | 14.781 (13.954–23.126) | 3.050 (2.421–7.885) | 17.328 (16.481–31.011) |
| `zero-extend-tail-4k` | 10 MiB | candidate | 5 | 1.892 (1.457–2.920) | 4.630 (2.736–7.737) | 7.100 (4.193–10.657) |
| `zero-extend-tail-4k` | 100 MiB | baseline | 5 | 17.318 (12.889–20.024) | 5.067 (3.172–6.330) | 21.944 (18.724–24.641) |
| `zero-extend-tail-4k` | 100 MiB | candidate | 5 | 2.325 (1.517–6.586) | 4.051 (3.772–5.728) | 7.115 (5.515–10.637) |
| `zero-extend-tail-4k` | 500 MiB | baseline | 5 | 23.089 (12.082–29.941) | 10.705 (6.849–16.625) | 35.847 (19.121–46.566) |
| `zero-extend-tail-4k` | 500 MiB | candidate | 5 | 1.925 (1.485–4.129) | 9.985 (7.767–10.477) | 11.896 (9.931–13.007) |

Nominal targets are 10/10/20 ms; user-approved accepted ceilings are 20/20/30 ms for Edit/Commit/combined. Combined is independently capped at 30 ms. Parity and resource gates are unchanged.

Memory profile: ack-window-v1. Cgroup observations cover an acknowledged broader window, not exact T0–T3. Native peaks are whole-worker/container lifetime bounds. Category maxima, dirty/writeback, and transient swap checks are sampled observations; continuous category ceilings cannot be strictly proven. Gaps are reported diagnostically. Native peak/incremental/size-spread limits and zero OOM remain binding.

| Candidate scenario | Latency classification |
| --- | --- |
| `insert-middle-4k-on-1mib-ops-1` | nominal-pass |
| `insert-middle-4k-on-10mib-ops-1` | nominal-pass |
| `insert-middle-4k-on-100mib-ops-1` | nominal-pass |
| `insert-middle-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `delete-middle-4k-on-1mib-ops-1` | nominal-pass |
| `delete-middle-4k-on-10mib-ops-1` | nominal-pass |
| `delete-middle-4k-on-100mib-ops-1` | nominal-pass |
| `delete-middle-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `append-tail-4k-on-1mib-ops-1` | nominal-pass |
| `append-tail-4k-on-10mib-ops-1` | nominal-pass |
| `append-tail-4k-on-100mib-ops-1` | nominal-pass |
| `append-tail-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `prepend-head-4k-on-1mib-ops-1` | nominal-pass |
| `prepend-head-4k-on-10mib-ops-1` | nominal-pass |
| `prepend-head-4k-on-100mib-ops-1` | nominal-pass |
| `prepend-head-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `replace-grow-middle-2k-to-4k-on-1mib-ops-1` | nominal-pass |
| `replace-grow-middle-2k-to-4k-on-10mib-ops-1` | nominal-pass |
| `replace-grow-middle-2k-to-4k-on-100mib-ops-1` | nominal-pass |
| `replace-grow-middle-2k-to-4k-on-500mib-ops-1` | accepted-with-tolerance |
| `replace-shrink-middle-4k-to-2k-on-1mib-ops-1` | nominal-pass |
| `replace-shrink-middle-4k-to-2k-on-10mib-ops-1` | nominal-pass |
| `replace-shrink-middle-4k-to-2k-on-100mib-ops-1` | nominal-pass |
| `replace-shrink-middle-4k-to-2k-on-500mib-ops-1` | accepted-with-tolerance |
| `truncate-tail-4k-on-1mib-ops-1` | nominal-pass |
| `truncate-tail-4k-on-10mib-ops-1` | nominal-pass |
| `truncate-tail-4k-on-100mib-ops-1` | nominal-pass |
| `truncate-tail-4k-on-500mib-ops-1` | nominal-pass |
| `zero-extend-tail-4k-on-1mib-ops-1` | nominal-pass |
| `zero-extend-tail-4k-on-10mib-ops-1` | nominal-pass |
| `zero-extend-tail-4k-on-100mib-ops-1` | nominal-pass |
| `zero-extend-tail-4k-on-500mib-ops-1` | nominal-pass |

## Memory

| Operation | Size | Source | Process phase MiB median (min–max) | Process incremental MiB median (min–max) | Cgroup sampled window MiB median (min–max) | Cgroup sampled window incremental MiB median (min–max) | Dirty/writeback incremental MiB median (min–max) |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| `insert-middle-4k` | 1 MiB | baseline | 6.953 (6.828–7.016) | 1.062 (1.016–1.125) | 2.488 (2.352–2.602) | 0.305 (0.074–0.473) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 1 MiB | candidate | 6.844 (6.703–6.922) | 1.062 (0.984–1.078) | 2.320 (2.184–2.477) | 0.277 (0.082–0.426) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 10 MiB | baseline | 7.281 (7.031–7.359) | 1.375 (1.359–1.406) | 2.613 (2.328–2.719) | 0.594 (0.160–0.766) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 10 MiB | candidate | 7.094 (7.047–7.281) | 1.359 (1.312–1.422) | 2.285 (1.957–2.398) | 0.133 (0.000–0.500) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 100 MiB | baseline | 7.781 (7.750–7.969) | 1.766 (1.719–1.812) | 2.609 (2.457–2.699) | 0.500 (0.414–0.766) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 100 MiB | candidate | 7.938 (7.797–8.062) | 1.828 (1.734–1.859) | 2.281 (2.242–2.566) | 0.211 (0.000–0.406) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 500 MiB | baseline | 9.797 (9.594–9.969) | 3.156 (3.125–3.266) | 2.688 (2.535–2.828) | 0.387 (0.273–0.883) | 0.000 (0.000–0.000) |
| `insert-middle-4k` | 500 MiB | candidate | 9.609 (9.562–9.734) | 3.109 (3.016–3.203) | 2.496 (2.258–2.562) | 0.473 (0.371–0.617) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 1 MiB | baseline | 6.719 (6.672–7.047) | 1.016 (1.000–1.047) | 2.477 (2.383–2.680) | 0.332 (0.121–0.570) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 1 MiB | candidate | 6.797 (6.703–6.984) | 1.062 (0.969–1.141) | 2.336 (2.141–2.648) | 0.254 (0.043–0.402) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 10 MiB | baseline | 7.094 (7.047–7.250) | 1.375 (1.344–1.453) | 2.492 (2.367–2.613) | 0.250 (0.109–0.551) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 10 MiB | candidate | 7.281 (7.047–7.391) | 1.375 (1.328–1.422) | 2.496 (2.164–2.516) | 0.574 (0.000–0.695) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 100 MiB | baseline | 7.875 (7.812–8.109) | 1.797 (1.750–1.891) | 2.539 (2.391–2.691) | 0.488 (0.348–0.660) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 100 MiB | candidate | 7.969 (7.797–8.047) | 1.797 (1.781–1.844) | 2.328 (2.238–2.652) | 0.066 (0.000–0.746) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 500 MiB | baseline | 9.531 (9.453–9.766) | 3.000 (2.938–3.047) | 2.609 (2.371–2.852) | 0.496 (0.367–0.805) | 0.000 (0.000–0.000) |
| `delete-middle-4k` | 500 MiB | candidate | 9.562 (9.438–9.797) | 3.031 (2.875–3.078) | 2.480 (2.277–2.617) | 0.211 (0.000–0.504) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 1 MiB | baseline | 6.953 (6.734–6.969) | 1.062 (1.031–1.094) | 2.453 (2.301–2.637) | 0.523 (0.281–0.871) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 1 MiB | candidate | 6.812 (6.641–7.062) | 0.969 (0.938–1.047) | 2.211 (2.062–2.426) | 0.293 (0.055–0.496) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 10 MiB | baseline | 6.969 (6.781–7.016) | 1.078 (1.062–1.125) | 2.457 (2.363–2.562) | 0.305 (0.148–0.504) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 10 MiB | candidate | 6.984 (6.781–7.047) | 1.078 (1.031–1.141) | 2.496 (2.301–2.566) | 0.199 (0.000–0.355) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 100 MiB | baseline | 7.266 (7.234–7.484) | 1.281 (1.203–1.328) | 2.660 (2.332–2.730) | 0.578 (0.367–0.680) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 100 MiB | candidate | 7.359 (7.281–7.484) | 1.328 (1.141–1.344) | 2.238 (2.023–2.527) | 0.238 (0.000–0.316) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 500 MiB | baseline | 8.781 (8.750–9.000) | 2.281 (2.234–2.328) | 2.586 (2.406–2.719) | 0.566 (0.414–0.719) | 0.000 (0.000–0.000) |
| `append-tail-4k` | 500 MiB | candidate | 8.859 (8.719–9.062) | 2.312 (2.234–2.344) | 2.438 (2.324–2.633) | 0.320 (0.074–0.559) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 1 MiB | baseline | 6.719 (6.719–7.094) | 1.062 (1.016–1.078) | 2.652 (2.469–2.867) | 0.676 (0.180–0.723) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 1 MiB | candidate | 6.922 (6.656–7.047) | 1.000 (0.953–1.062) | 2.266 (2.133–2.680) | 0.137 (0.062–0.492) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 10 MiB | baseline | 7.062 (6.906–7.188) | 1.188 (1.172–1.219) | 2.605 (2.387–2.691) | 0.367 (0.320–0.617) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 10 MiB | candidate | 6.938 (6.828–7.109) | 1.219 (1.141–1.328) | 2.426 (2.184–2.547) | 0.070 (0.000–0.395) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 100 MiB | baseline | 7.578 (7.500–7.766) | 1.516 (1.484–1.594) | 2.508 (2.492–2.746) | 0.383 (0.219–0.707) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 100 MiB | candidate | 7.547 (7.469–7.797) | 1.500 (1.406–1.578) | 2.375 (2.066–2.559) | 0.074 (0.000–0.414) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 500 MiB | baseline | 9.312 (9.125–9.375) | 2.688 (2.656–2.859) | 2.738 (2.375–2.902) | 0.703 (0.309–1.082) | 0.000 (0.000–0.000) |
| `prepend-head-4k` | 500 MiB | candidate | 9.219 (9.094–9.344) | 2.625 (2.578–2.797) | 2.168 (2.125–2.750) | 0.297 (0.137–0.508) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 1 MiB | baseline | 7.031 (6.859–7.078) | 1.078 (1.047–1.172) | 2.457 (2.363–2.820) | 0.438 (0.309–0.816) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 1 MiB | candidate | 6.828 (6.719–6.984) | 1.078 (1.031–1.172) | 2.480 (2.125–2.637) | 0.453 (0.039–0.555) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 10 MiB | baseline | 7.125 (7.109–7.344) | 1.422 (1.422–1.453) | 2.602 (2.418–2.688) | 0.484 (0.199–0.660) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 10 MiB | candidate | 7.266 (7.078–7.328) | 1.359 (1.281–1.406) | 2.383 (2.191–2.539) | 0.207 (0.109–0.348) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 100 MiB | baseline | 7.906 (7.844–8.094) | 1.828 (1.750–1.875) | 2.734 (2.355–3.004) | 0.461 (0.191–0.719) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 100 MiB | candidate | 7.969 (7.812–7.984) | 1.797 (1.750–1.844) | 2.426 (2.270–2.547) | 0.172 (0.000–0.512) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 500 MiB | baseline | 9.625 (9.547–9.875) | 3.109 (3.078–3.141) | 2.602 (2.449–2.770) | 0.496 (0.180–0.805) | 0.000 (0.000–0.000) |
| `replace-grow-middle-2k-to-4k` | 500 MiB | candidate | 9.594 (9.547–9.828) | 3.062 (2.906–3.141) | 2.324 (2.223–2.477) | 0.359 (0.035–0.570) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 1 MiB | baseline | 6.922 (6.719–7.031) | 1.047 (1.047–1.078) | 2.516 (2.355–2.656) | 0.434 (0.336–0.504) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 1 MiB | candidate | 6.938 (6.766–6.969) | 1.062 (1.031–1.094) | 2.402 (2.160–2.512) | 0.211 (0.055–0.348) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 10 MiB | baseline | 7.094 (7.000–7.328) | 1.406 (1.297–1.469) | 2.570 (2.402–2.750) | 0.496 (0.121–0.832) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 10 MiB | candidate | 7.047 (7.016–7.141) | 1.328 (1.281–1.375) | 2.273 (2.070–2.477) | 0.129 (0.000–0.309) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 100 MiB | baseline | 7.812 (7.734–8.094) | 1.781 (1.734–1.781) | 2.605 (2.531–2.684) | 0.633 (0.422–0.668) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 100 MiB | candidate | 7.828 (7.734–7.969) | 1.719 (1.641–1.766) | 2.422 (2.254–2.504) | 0.176 (0.000–0.250) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 500 MiB | baseline | 9.781 (9.656–10.000) | 3.234 (3.188–3.344) | 2.504 (2.457–2.836) | 0.480 (0.211–0.641) | 0.000 (0.000–0.000) |
| `replace-shrink-middle-4k-to-2k` | 500 MiB | candidate | 9.969 (9.594–9.984) | 3.203 (3.109–3.281) | 2.457 (2.242–2.598) | 0.496 (0.000–0.504) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 1 MiB | baseline | 6.719 (6.672–6.859) | 1.000 (0.969–1.031) | 2.555 (2.383–2.664) | 0.395 (0.078–0.629) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 1 MiB | candidate | 6.672 (6.672–6.984) | 1.047 (0.953–1.047) | 2.336 (2.246–2.633) | 0.359 (0.145–0.492) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 10 MiB | baseline | 6.859 (6.766–7.156) | 1.125 (1.062–1.156) | 2.664 (2.441–2.719) | 0.438 (0.367–0.668) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 10 MiB | candidate | 6.766 (6.719–7.078) | 1.109 (1.031–1.141) | 2.246 (2.199–2.465) | 0.164 (0.047–0.285) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 100 MiB | baseline | 7.375 (7.297–7.547) | 1.312 (1.281–1.375) | 2.480 (2.398–2.863) | 0.309 (0.273–0.586) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 100 MiB | candidate | 7.344 (7.250–7.375) | 1.328 (1.219–1.422) | 2.340 (2.133–2.438) | 0.121 (0.070–0.336) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 500 MiB | baseline | 9.156 (9.047–9.312) | 2.641 (2.578–2.688) | 2.633 (2.422–2.660) | 0.500 (0.352–0.723) | 0.000 (0.000–0.000) |
| `truncate-tail-4k` | 500 MiB | candidate | 9.125 (9.109–9.328) | 2.625 (2.594–2.719) | 2.320 (2.238–2.469) | 0.297 (0.098–0.605) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 1 MiB | baseline | 6.844 (6.641–6.953) | 1.047 (0.984–1.078) | 2.539 (2.309–2.723) | 0.496 (0.148–0.656) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 1 MiB | candidate | 6.719 (6.672–6.938) | 1.031 (0.969–1.047) | 2.504 (2.250–2.613) | 0.250 (0.000–0.539) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 10 MiB | baseline | 6.781 (6.719–7.016) | 1.062 (1.031–1.141) | 2.445 (2.363–2.676) | 0.281 (0.223–0.430) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 10 MiB | candidate | 6.969 (6.766–7.062) | 1.094 (1.000–1.156) | 2.438 (2.367–2.633) | 0.324 (0.148–0.492) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 100 MiB | baseline | 7.469 (7.312–7.516) | 1.297 (1.266–1.328) | 2.582 (2.473–2.801) | 0.504 (0.359–0.660) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 100 MiB | candidate | 7.281 (7.266–7.594) | 1.281 (1.234–1.391) | 2.430 (1.922–2.598) | 0.258 (0.000–0.406) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 500 MiB | baseline | 8.969 (8.922–9.219) | 2.484 (2.469–2.500) | 2.754 (2.367–2.840) | 0.562 (0.238–0.844) | 0.000 (0.000–0.000) |
| `zero-extend-tail-4k` | 500 MiB | candidate | 9.016 (8.969–9.250) | 2.500 (2.438–2.562) | 2.430 (2.191–2.555) | 0.238 (0.141–0.547) | 0.000 (0.000–0.000) |

Aggregate verifier receipts: 32.

Candidate size parity, matched-operation parity, route, CDC, spool, transaction, memory, cleanup, and custody gates are admission-binding. Baseline latency parity is diagnostic; baseline correctness, route, resource, cleanup, and custody remain binding.

## Per-sample resource and mechanism guards

All maxima below cover every retained sample, not only medians. Swap/OOM, FUSE mutation bytes, and spool must be zero; coverage and cleanup must pass. The 112 MiB target is diagnostic; 128 MiB is the unchanged hard ceiling.

| Operation | MiB | Arm | Lifetime RSS / cgroup max MiB | RSS / cgroup max gap ms | Minimum RSS / cgroup samples | CDC bytes min–max | Candidate bytes max | Spool bytes max | 112 MiB target |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| insert-middle-4k | 1 | baseline | 7.172 / 4.570 | 0.146 / 1.732 | 35975 / 3225 | 4096–4096 | 6898 | 0 | target-pass |
| insert-middle-4k | 1 | candidate | 7.172 / 4.270 | 0.092 / 2.542 | 7059 / 793 | 4096–4096 | 6898 | 0 | target-pass |
| insert-middle-4k | 10 | baseline | 7.531 / 4.164 | 0.074 / 1.981 | 22975 / 2392 | 4096–4096 | 18642 | 0 | target-pass |
| insert-middle-4k | 10 | candidate | 7.438 / 4.820 | 0.067 / 4.425 | 7301 / 697 | 4096–4096 | 18642 | 0 | target-pass |
| insert-middle-4k | 100 | baseline | 8.125 / 4.312 | 0.479 / 3.153 | 23085 / 2766 | 4096–4096 | 22386 | 0 | target-pass |
| insert-middle-4k | 100 | candidate | 8.203 / 4.773 | 0.120 / 2.785 | 8308 / 809 | 4096–4096 | 22386 | 0 | target-pass |
| insert-middle-4k | 500 | baseline | 10.094 / 4.418 | 0.091 / 1.675 | 43872 / 3215 | 4096–4096 | 30634 | 0 | target-pass |
| insert-middle-4k | 500 | candidate | 9.938 / 4.500 | 0.354 / 2.083 | 15339 / 1155 | 4096–4096 | 30634 | 0 | target-pass |
| delete-middle-4k | 1 | baseline | 7.188 / 4.664 | 3.354 / 3.647 | 34551 / 2935 | 0–0 | 2741 | 0 | target-pass |
| delete-middle-4k | 1 | candidate | 7.203 / 4.844 | 0.070 / 2.619 | 6198 / 753 | 0–0 | 2741 | 0 | target-pass |
| delete-middle-4k | 10 | baseline | 7.406 / 4.531 | 0.084 / 1.062 | 30975 / 3033 | 0–0 | 14485 | 0 | target-pass |
| delete-middle-4k | 10 | candidate | 7.578 / 4.168 | 0.039 / 3.002 | 6411 / 735 | 0–0 | 14485 | 0 | target-pass |
| delete-middle-4k | 100 | baseline | 8.250 / 4.809 | 0.076 / 2.591 | 28635 / 1392 | 0–0 | 18229 | 0 | target-pass |
| delete-middle-4k | 100 | candidate | 8.188 / 5.973 | 0.088 / 0.686 | 6807 / 1688 | 0–0 | 18229 | 0 | target-pass |
| delete-middle-4k | 500 | baseline | 9.953 / 4.410 | 0.102 / 2.170 | 36124 / 3794 | 0–0 | 26477 | 0 | target-pass |
| delete-middle-4k | 500 | candidate | 9.984 / 4.250 | 0.076 / 3.201 | 15116 / 1002 | 0–0 | 26477 | 0 | target-pass |
| append-tail-4k | 1 | baseline | 7.156 / 4.164 | 0.069 / 3.901 | 28978 / 1711 | 4096–4096 | 6858 | 0 | target-pass |
| append-tail-4k | 1 | candidate | 7.266 / 4.750 | 0.067 / 3.046 | 4874 / 864 | 4096–4096 | 6858 | 0 | target-pass |
| append-tail-4k | 10 | baseline | 7.234 / 4.406 | 2.963 / 2.100 | 23769 / 2696 | 4096–4096 | 8182 | 0 | target-pass |
| append-tail-4k | 10 | candidate | 7.266 / 4.309 | 0.152 / 2.499 | 6149 / 583 | 4096–4096 | 8182 | 0 | target-pass |
| append-tail-4k | 100 | baseline | 7.609 / 4.297 | 0.087 / 2.767 | 32639 / 2087 | 4096–4096 | 9726 | 0 | target-pass |
| append-tail-4k | 100 | candidate | 7.688 / 4.527 | 0.082 / 2.034 | 8070 / 1321 | 4096–4096 | 9726 | 0 | target-pass |
| append-tail-4k | 500 | baseline | 9.109 / 6.789 | 0.084 / 2.136 | 39244 / 2323 | 4096–4096 | 19654 | 0 | target-pass |
| append-tail-4k | 500 | candidate | 9.234 / 6.188 | 0.105 / 2.407 | 12732 / 753 | 4096–4096 | 19654 | 0 | target-pass |
| prepend-head-4k | 1 | baseline | 7.250 / 4.973 | 0.102 / 1.428 | 33837 / 3437 | 4096–4096 | 6858 | 0 | target-pass |
| prepend-head-4k | 1 | candidate | 7.234 / 4.551 | 0.049 / 2.269 | 4132 / 561 | 4096–4096 | 6858 | 0 | target-pass |
| prepend-head-4k | 10 | baseline | 7.344 / 4.949 | 0.087 / 2.877 | 28070 / 2503 | 4096–4096 | 10194 | 0 | target-pass |
| prepend-head-4k | 10 | candidate | 7.281 / 4.191 | 0.035 / 1.103 | 4532 / 1368 | 4096–4096 | 10194 | 0 | target-pass |
| prepend-head-4k | 100 | baseline | 7.969 / 4.805 | 0.100 / 2.105 | 30557 / 2112 | 4096–4096 | 12018 | 0 | target-pass |
| prepend-head-4k | 100 | candidate | 7.984 / 4.914 | 0.061 / 1.505 | 8230 / 1605 | 4096–4096 | 12018 | 0 | target-pass |
| prepend-head-4k | 500 | baseline | 9.516 / 4.309 | 1.526 / 5.801 | 35170 / 2677 | 4096–4096 | 16330 | 0 | target-pass |
| prepend-head-4k | 500 | candidate | 9.531 / 4.504 | 0.113 / 2.132 | 14246 / 2183 | 4096–4096 | 16330 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 1 | baseline | 7.219 / 4.180 | 0.079 / 1.379 | 22907 / 1805 | 4096–4096 | 6898 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 1 | candidate | 7.172 / 4.426 | 0.036 / 1.857 | 5471 / 1138 | 4096–4096 | 6898 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 10 | baseline | 7.484 / 4.488 | 0.963 / 1.562 | 28386 / 3096 | 4096–4096 | 18642 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 10 | candidate | 7.516 / 4.578 | 0.042 / 1.383 | 5610 / 1172 | 4096–4096 | 18642 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 100 | baseline | 8.250 / 4.242 | 0.079 / 3.705 | 33499 / 3194 | 4096–4096 | 22386 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 100 | candidate | 8.172 / 4.492 | 0.065 / 2.543 | 11244 / 775 | 4096–4096 | 22386 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 500 | baseline | 9.984 / 4.512 | 0.164 / 1.743 | 38561 / 3513 | 4096–4096 | 30634 | 0 | target-pass |
| replace-grow-middle-2k-to-4k | 500 | candidate | 10.000 / 4.824 | 0.140 / 2.713 | 16495 / 1854 | 4096–4096 | 30634 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 1 | baseline | 7.172 / 4.133 | 0.107 / 2.767 | 27251 / 2082 | 2048–2048 | 4850 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 1 | candidate | 7.141 / 4.387 | 0.089 / 1.603 | 4927 / 685 | 2048–2048 | 4850 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 10 | baseline | 7.453 / 4.492 | 0.074 / 0.870 | 28230 / 3286 | 2048–2048 | 16594 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 10 | candidate | 7.312 / 4.273 | 0.084 / 2.111 | 5852 / 681 | 2048–2048 | 16594 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 100 | baseline | 8.250 / 4.699 | 0.065 / 2.075 | 32488 / 2388 | 2048–2048 | 20338 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 100 | candidate | 8.172 / 5.758 | 0.079 / 2.085 | 9781 / 1230 | 2048–2048 | 20338 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 500 | baseline | 10.109 / 4.414 | 0.059 / 1.058 | 33520 / 3169 | 2048–2048 | 28586 | 0 | target-pass |
| replace-shrink-middle-4k-to-2k | 500 | candidate | 10.141 / 4.359 | 0.082 / 2.223 | 17717 / 1732 | 2048–2048 | 28586 | 0 | target-pass |
| truncate-tail-4k | 1 | baseline | 7.016 / 4.281 | 0.086 / 1.183 | 23233 / 2361 | 0–0 | 2701 | 0 | target-pass |
| truncate-tail-4k | 1 | candidate | 7.219 / 4.461 | 0.037 / 1.577 | 5126 / 1423 | 0–0 | 2701 | 0 | target-pass |
| truncate-tail-4k | 10 | baseline | 7.281 / 4.625 | 0.104 / 2.997 | 30055 / 2351 | 0–0 | 7269 | 0 | target-pass |
| truncate-tail-4k | 10 | candidate | 7.312 / 4.320 | 0.088 / 0.507 | 4915 / 1062 | 0–0 | 7269 | 0 | target-pass |
| truncate-tail-4k | 100 | baseline | 7.719 / 6.246 | 0.102 / 1.171 | 36035 / 2912 | 0–0 | 8533 | 0 | target-pass |
| truncate-tail-4k | 100 | candidate | 7.578 / 4.574 | 0.056 / 3.420 | 5963 / 818 | 0–0 | 8533 | 0 | target-pass |
| truncate-tail-4k | 500 | baseline | 9.453 / 4.703 | 0.059 / 1.153 | 45078 / 3858 | 0–0 | 20621 | 0 | target-pass |
| truncate-tail-4k | 500 | candidate | 9.500 / 4.324 | 0.083 / 0.876 | 13327 / 2009 | 0–0 | 20621 | 0 | target-pass |
| zero-extend-tail-4k | 1 | baseline | 7.125 / 4.387 | 0.117 / 2.270 | 31906 / 2475 | 4096–4096 | 6858 | 0 | target-pass |
| zero-extend-tail-4k | 1 | candidate | 7.125 / 4.586 | 0.434 / 0.810 | 5719 / 1057 | 4096–4096 | 6858 | 0 | target-pass |
| zero-extend-tail-4k | 10 | baseline | 7.188 / 4.043 | 0.331 / 5.262 | 23925 / 2746 | 4096–4096 | 8182 | 0 | target-pass |
| zero-extend-tail-4k | 10 | candidate | 7.219 / 4.156 | 0.298 / 3.991 | 5011 / 1201 | 4096–4096 | 8182 | 0 | target-pass |
| zero-extend-tail-4k | 100 | baseline | 7.641 / 6.156 | 0.256 / 2.111 | 28235 / 2147 | 4096–4096 | 9726 | 0 | target-pass |
| zero-extend-tail-4k | 100 | candidate | 7.844 / 4.418 | 0.095 / 2.079 | 7632 / 1433 | 4096–4096 | 9726 | 0 | target-pass |
| zero-extend-tail-4k | 500 | baseline | 9.422 / 4.820 | 0.443 / 2.597 | 28552 / 2824 | 4096–4096 | 19654 | 0 | target-pass |
| zero-extend-tail-4k | 500 | candidate | 9.391 / 4.297 | 0.304 / 0.804 | 13892 / 2084 | 4096–4096 | 19654 | 0 | target-pass |

## Size parity

Ratios use the 1 MiB median as denominator; spread and allowance are independently evaluated for each metric.

| Operation | Arm | Metric | 10/1 | 100/1 | 500/1 | Spread / allowance ms | Status |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| insert-middle-4k | baseline | edit_call_ns | 0.762 | 0.676 | 0.901 | 8.044 / 2.000 | fail-diagnostic |
| insert-middle-4k | baseline | commit_call_ns | 1.475 | 2.270 | 4.068 | 8.226 / 2.000 | fail-diagnostic |
| insert-middle-4k | baseline | edit_commit_ns | 0.814 | 0.794 | 1.148 | 9.672 / 2.173 | fail-diagnostic |
| insert-middle-4k | candidate | edit_call_ns | 0.799 | 0.931 | 0.897 | 0.502 / 2.000 | pass |
| insert-middle-4k | candidate | commit_call_ns | 1.427 | 2.396 | 4.923 | 8.999 / 2.000 | fail |
| insert-middle-4k | candidate | edit_commit_ns | 1.093 | 1.601 | 2.796 | 8.696 / 2.000 | fail |
| delete-middle-4k | baseline | edit_call_ns | 0.790 | 0.677 | 1.092 | 10.145 / 2.000 | fail-diagnostic |
| delete-middle-4k | baseline | commit_call_ns | 0.678 | 1.027 | 2.510 | 8.756 / 2.000 | fail-diagnostic |
| delete-middle-4k | baseline | edit_commit_ns | 0.756 | 0.711 | 1.343 | 18.751 / 2.106 | fail-diagnostic |
| delete-middle-4k | candidate | edit_call_ns | 0.520 | 0.517 | 0.507 | 2.572 / 2.000 | fail |
| delete-middle-4k | candidate | commit_call_ns | 1.038 | 2.189 | 4.225 | 8.711 / 2.000 | fail |
| delete-middle-4k | candidate | edit_commit_ns | 0.549 | 0.894 | 1.362 | 8.362 / 2.000 | fail |
| append-tail-4k | baseline | edit_call_ns | 1.095 | 1.231 | 0.986 | 4.639 / 2.000 | fail-diagnostic |
| append-tail-4k | baseline | commit_call_ns | 1.409 | 1.583 | 4.234 | 9.287 / 2.000 | fail-diagnostic |
| append-tail-4k | baseline | edit_commit_ns | 1.118 | 1.218 | 1.284 | 6.366 / 2.239 | fail-diagnostic |
| append-tail-4k | candidate | edit_call_ns | 1.572 | 1.495 | 0.996 | 1.162 / 2.000 | pass |
| append-tail-4k | candidate | commit_call_ns | 0.861 | 1.358 | 3.429 | 9.252 / 2.000 | fail |
| append-tail-4k | candidate | edit_commit_ns | 1.085 | 1.261 | 2.393 | 8.361 / 2.000 | fail |
| prepend-head-4k | baseline | edit_call_ns | 0.940 | 0.935 | 0.979 | 1.334 / 2.000 | pass |
| prepend-head-4k | baseline | commit_call_ns | 0.737 | 1.115 | 2.734 | 9.225 / 2.000 | fail-diagnostic |
| prepend-head-4k | baseline | edit_commit_ns | 0.981 | 1.037 | 1.291 | 7.385 / 2.341 | fail-diagnostic |
| prepend-head-4k | candidate | edit_call_ns | 1.293 | 1.773 | 1.997 | 1.670 / 2.000 | pass |
| prepend-head-4k | candidate | commit_call_ns | 0.952 | 1.470 | 3.811 | 8.343 / 2.000 | fail |
| prepend-head-4k | candidate | edit_commit_ns | 1.043 | 1.551 | 3.056 | 9.620 / 2.000 | fail |
| replace-grow-middle-2k-to-4k | baseline | edit_call_ns | 1.249 | 1.074 | 1.052 | 4.289 / 2.000 | fail-diagnostic |
| replace-grow-middle-2k-to-4k | baseline | commit_call_ns | 0.956 | 1.414 | 3.484 | 9.385 / 2.000 | fail-diagnostic |
| replace-grow-middle-2k-to-4k | baseline | edit_commit_ns | 1.259 | 1.188 | 1.587 | 11.501 / 2.000 | fail-diagnostic |
| replace-grow-middle-2k-to-4k | candidate | edit_call_ns | 0.953 | 1.571 | 1.311 | 1.099 / 2.000 | pass |
| replace-grow-middle-2k-to-4k | candidate | commit_call_ns | 2.263 | 2.691 | 4.587 | 8.488 / 2.000 | fail |
| replace-grow-middle-2k-to-4k | candidate | edit_commit_ns | 1.812 | 2.146 | 3.583 | 10.848 / 2.000 | fail |
| replace-shrink-middle-4k-to-2k | baseline | edit_call_ns | 0.976 | 1.070 | 0.871 | 3.789 / 2.000 | fail-diagnostic |
| replace-shrink-middle-4k-to-2k | baseline | commit_call_ns | 1.146 | 1.612 | 3.301 | 7.810 / 2.000 | fail-diagnostic |
| replace-shrink-middle-4k-to-2k | baseline | edit_commit_ns | 1.015 | 1.107 | 1.310 | 6.911 / 2.232 | fail-diagnostic |
| replace-shrink-middle-4k-to-2k | candidate | edit_call_ns | 1.038 | 1.639 | 2.328 | 2.111 / 2.000 | fail |
| replace-shrink-middle-4k-to-2k | candidate | commit_call_ns | 1.644 | 2.249 | 5.141 | 9.661 / 2.000 | fail |
| replace-shrink-middle-4k-to-2k | candidate | edit_commit_ns | 1.396 | 2.008 | 4.287 | 12.917 / 2.000 | fail |
| truncate-tail-4k | baseline | edit_call_ns | 1.217 | 1.058 | 1.033 | 3.852 / 2.000 | fail-diagnostic |
| truncate-tail-4k | baseline | commit_call_ns | 1.337 | 2.290 | 4.133 | 7.641 / 2.000 | fail-diagnostic |
| truncate-tail-4k | baseline | edit_commit_ns | 1.192 | 1.310 | 1.548 | 11.476 / 2.094 | fail-diagnostic |
| truncate-tail-4k | candidate | edit_call_ns | 0.584 | 0.780 | 0.839 | 1.138 / 2.000 | pass |
| truncate-tail-4k | candidate | commit_call_ns | 0.971 | 1.536 | 3.638 | 6.713 / 2.000 | fail |
| truncate-tail-4k | candidate | edit_commit_ns | 0.850 | 1.169 | 2.263 | 7.058 / 2.000 | fail |
| zero-extend-tail-4k | baseline | edit_call_ns | 0.708 | 0.830 | 1.106 | 8.308 / 2.000 | fail-diagnostic |
| zero-extend-tail-4k | baseline | commit_call_ns | 0.913 | 1.516 | 3.203 | 7.655 / 2.000 | fail-diagnostic |
| zero-extend-tail-4k | baseline | edit_commit_ns | 0.742 | 0.939 | 1.534 | 18.519 / 2.000 | fail-diagnostic |
| zero-extend-tail-4k | candidate | edit_call_ns | 0.612 | 0.753 | 0.623 | 1.198 / 2.000 | pass |
| zero-extend-tail-4k | candidate | commit_call_ns | 1.723 | 1.507 | 3.716 | 7.298 / 2.000 | fail |
| zero-extend-tail-4k | candidate | edit_commit_ns | 1.231 | 1.234 | 2.063 | 6.129 / 2.000 | fail |

## Matched-operation parity

| Cohort | MiB | Metric | Medians ms | Status |
| --- | ---: | --- | --- | --- |
| inline-insert | 1 | edit_call_ns | 2.017, 2.502, 1.674 | pass |
| inline-insert | 1 | commit_call_ns | 3.603, 2.294, 2.919 | pass |
| inline-insert | 1 | edit_commit_ns | 6.003, 4.842, 4.680 | pass |
| inline-insert | 10 | edit_call_ns | 3.172, 2.001, 2.164 | pass |
| inline-insert | 10 | commit_call_ns | 3.101, 3.273, 2.779 | pass |
| inline-insert | 10 | edit_commit_ns | 6.515, 5.292, 4.883 | pass |
| inline-insert | 100 | edit_call_ns | 3.016, 2.331, 2.967 | pass |
| inline-insert | 100 | commit_call_ns | 4.891, 5.495, 4.289 | pass |
| inline-insert | 100 | edit_commit_ns | 7.569, 7.752, 7.257 | pass |
| inline-insert | 500 | edit_call_ns | 2.010, 2.245, 3.344 | pass |
| inline-insert | 500 | commit_call_ns | 12.354, 11.293, 11.122 | pass |
| inline-insert | 500 | edit_commit_ns | 14.364, 13.538, 14.300 | pass |
| delete | 1 | edit_call_ns | 5.221, 2.737 | fail |
| delete | 1 | commit_call_ns | 2.701, 2.517 | pass |
| delete | 1 | edit_commit_ns | 10.283, 4.994 | fail |
| delete | 10 | edit_call_ns | 2.715, 1.599 | pass |
| delete | 10 | commit_call_ns | 2.803, 2.444 | pass |
| delete | 10 | edit_commit_ns | 5.643, 4.246 | pass |
| delete | 100 | edit_call_ns | 2.698, 2.136 | pass |
| delete | 100 | commit_call_ns | 5.913, 3.866 | fail |
| delete | 100 | edit_commit_ns | 9.191, 5.837 | fail |
| delete | 500 | edit_call_ns | 2.649, 2.295 | pass |
| delete | 500 | commit_call_ns | 11.412, 9.157 | fail |
| delete | 500 | edit_commit_ns | 14.006, 11.304 | fail |

## Untimed preparation

| MiB | Cache disposition | Build ms | Validation ms | Acquisition ms | Cache key |
| ---: | --- | ---: | ---: | ---: | --- |
| 1 | hit | 0.000 | 7.302 | 28.655 | 61a6d6fbd6c36f4bf99c3c7241e7a5d890d0cc1dfbe9458de57d8b7c81e478c0 |
| 10 | hit | 0.000 | 14.232 | 33.440 | 3d6d2fc2e32570958c9f55e27668df2d3ac9f000b9fbbcb6a5d0fd13a6cb1b6d |
| 100 | hit | 0.000 | 84.115 | 104.570 | 1cdd2d79fdf5ea406a09d56ab7a377856eb8406e7ffc5ccf6867e4e828507807 |
| 500 | hit | 0.000 | 447.203 | 467.493 | 57b81a56f638ef88f2205408d98b9a0a3ff5e9f6727e4eb5031c3665f7872ff1 |

Qualification and clone setup are retained in [qualification timing](environment/qualification-timing.tsv); each raw row records its clone method/digest/wall, container-start wall, and clock_sampler_start_ns for authenticated connection and sampler warmup. These are never part of edit or Commit latency. Cgroup observation uses an acknowledged broader window with no clock probes. Exact phase attribution and continuous category maxima are unavailable; actual gaps are reported diagnostically.

Pre-run manifest SHA-256: df5bccdfaac84cc88910e70edee370be6047726d45b6cb074577859350cd65a2. The enclosing evidence manifest identity is shown by the cross-family report.

## Failures

- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r1:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r1:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r1:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r1:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r1:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r1:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r1:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r1:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r2:candidate observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r2:baseline observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r2:candidate observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r2:baseline observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r2:candidate observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r2:baseline observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r2:candidate observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r2:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r3:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r3:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r3:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r3:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r3:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r3:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r3:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r4:candidate observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r4:baseline observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r4:candidate observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r4:baseline observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r4:candidate observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r4:baseline observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r4:candidate observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r4:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-shrink-middle-4k-to-2k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:truncate-tail-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:zero-extend-tail-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:insert-middle-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:insert-middle-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:insert-middle-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:insert-middle-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:delete-middle-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:delete-middle-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:delete-middle-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:delete-middle-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:append-tail-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:append-tail-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:append-tail-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:append-tail-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:prepend-head-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:prepend-head-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:prepend-head-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:prepend-head-4k-on-500mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-1mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-10mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-100mib-ops-1:r5:candidate observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r5:baseline observation scope
- edit_length_changing:replace-grow-middle-2k-to-4k-on-500mib-ops-1:r5:candidate observation scope
- candidate insert-middle-4k commit_call_ns size parity
- candidate insert-middle-4k edit_commit_ns size parity
- candidate delete-middle-4k edit_call_ns size parity
- candidate delete-middle-4k commit_call_ns size parity
- candidate delete-middle-4k edit_commit_ns size parity
- candidate append-tail-4k commit_call_ns size parity
- candidate append-tail-4k edit_commit_ns size parity
- candidate prepend-head-4k commit_call_ns size parity
- candidate prepend-head-4k edit_commit_ns size parity
- candidate replace-grow-middle-2k-to-4k commit_call_ns size parity
- candidate replace-grow-middle-2k-to-4k edit_commit_ns size parity
- candidate replace-shrink-middle-4k-to-2k edit_call_ns size parity
- candidate replace-shrink-middle-4k-to-2k commit_call_ns size parity
- candidate replace-shrink-middle-4k-to-2k edit_commit_ns size parity
- candidate truncate-tail-4k commit_call_ns size parity
- candidate truncate-tail-4k edit_commit_ns size parity
- candidate zero-extend-tail-4k commit_call_ns size parity
- candidate zero-extend-tail-4k edit_commit_ns size parity
- candidate delete 1048576 edit_call_ns
- candidate delete 1048576 edit_commit_ns
- candidate delete 104857600 commit_call_ns
- candidate delete 104857600 edit_commit_ns
- candidate delete 524288000 commit_call_ns
- candidate delete 524288000 edit_commit_ns
- verification insert-middle-4k-on-1mib-ops-1 baseline observation scope
- verification insert-middle-4k-on-1mib-ops-1 candidate observation scope
- verification insert-middle-4k-on-10mib-ops-1 baseline observation scope
- verification insert-middle-4k-on-10mib-ops-1 candidate observation scope
- verification insert-middle-4k-on-100mib-ops-1 baseline observation scope
- verification insert-middle-4k-on-100mib-ops-1 candidate observation scope
- verification insert-middle-4k-on-500mib-ops-1 baseline observation scope
- verification insert-middle-4k-on-500mib-ops-1 candidate observation scope
- verification delete-middle-4k-on-1mib-ops-1 baseline observation scope
- verification delete-middle-4k-on-1mib-ops-1 candidate observation scope
- verification delete-middle-4k-on-10mib-ops-1 baseline observation scope
- verification delete-middle-4k-on-10mib-ops-1 candidate observation scope
- verification delete-middle-4k-on-100mib-ops-1 baseline observation scope
- verification delete-middle-4k-on-100mib-ops-1 candidate observation scope
- verification delete-middle-4k-on-500mib-ops-1 baseline observation scope
- verification delete-middle-4k-on-500mib-ops-1 candidate observation scope
- verification append-tail-4k-on-1mib-ops-1 baseline observation scope
- verification append-tail-4k-on-1mib-ops-1 candidate observation scope
- verification append-tail-4k-on-10mib-ops-1 baseline observation scope
- verification append-tail-4k-on-10mib-ops-1 candidate observation scope
- verification append-tail-4k-on-100mib-ops-1 baseline observation scope
- verification append-tail-4k-on-100mib-ops-1 candidate observation scope
- verification append-tail-4k-on-500mib-ops-1 baseline observation scope
- verification append-tail-4k-on-500mib-ops-1 candidate observation scope
- verification prepend-head-4k-on-1mib-ops-1 baseline observation scope
- verification prepend-head-4k-on-1mib-ops-1 candidate observation scope
- verification prepend-head-4k-on-10mib-ops-1 baseline observation scope
- verification prepend-head-4k-on-10mib-ops-1 candidate observation scope
- verification prepend-head-4k-on-100mib-ops-1 baseline observation scope
- verification prepend-head-4k-on-100mib-ops-1 candidate observation scope
- verification prepend-head-4k-on-500mib-ops-1 baseline observation scope
- verification prepend-head-4k-on-500mib-ops-1 candidate observation scope
- verification replace-grow-middle-2k-to-4k-on-1mib-ops-1 baseline observation scope
- verification replace-grow-middle-2k-to-4k-on-1mib-ops-1 candidate observation scope
- verification replace-grow-middle-2k-to-4k-on-10mib-ops-1 baseline observation scope
- verification replace-grow-middle-2k-to-4k-on-10mib-ops-1 candidate observation scope
- verification replace-grow-middle-2k-to-4k-on-100mib-ops-1 baseline observation scope
- verification replace-grow-middle-2k-to-4k-on-100mib-ops-1 candidate observation scope
- verification replace-grow-middle-2k-to-4k-on-500mib-ops-1 baseline observation scope
- verification replace-grow-middle-2k-to-4k-on-500mib-ops-1 candidate observation scope
- verification replace-shrink-middle-4k-to-2k-on-1mib-ops-1 baseline observation scope
- verification replace-shrink-middle-4k-to-2k-on-1mib-ops-1 candidate observation scope
- verification replace-shrink-middle-4k-to-2k-on-10mib-ops-1 baseline observation scope
- verification replace-shrink-middle-4k-to-2k-on-10mib-ops-1 candidate observation scope
- verification replace-shrink-middle-4k-to-2k-on-100mib-ops-1 baseline observation scope
- verification replace-shrink-middle-4k-to-2k-on-100mib-ops-1 candidate observation scope
- verification replace-shrink-middle-4k-to-2k-on-500mib-ops-1 baseline observation scope
- verification replace-shrink-middle-4k-to-2k-on-500mib-ops-1 candidate observation scope
- verification truncate-tail-4k-on-1mib-ops-1 baseline observation scope
- verification truncate-tail-4k-on-1mib-ops-1 candidate observation scope
- verification truncate-tail-4k-on-10mib-ops-1 baseline observation scope
- verification truncate-tail-4k-on-10mib-ops-1 candidate observation scope
- verification truncate-tail-4k-on-100mib-ops-1 baseline observation scope
- verification truncate-tail-4k-on-100mib-ops-1 candidate observation scope
- verification truncate-tail-4k-on-500mib-ops-1 baseline observation scope
- verification truncate-tail-4k-on-500mib-ops-1 candidate observation scope
- verification zero-extend-tail-4k-on-1mib-ops-1 baseline observation scope
- verification zero-extend-tail-4k-on-1mib-ops-1 candidate observation scope
- verification zero-extend-tail-4k-on-10mib-ops-1 baseline observation scope
- verification zero-extend-tail-4k-on-10mib-ops-1 candidate observation scope
- verification zero-extend-tail-4k-on-100mib-ops-1 baseline observation scope
- verification zero-extend-tail-4k-on-100mib-ops-1 candidate observation scope
- verification zero-extend-tail-4k-on-500mib-ops-1 baseline observation scope
- verification zero-extend-tail-4k-on-500mib-ops-1 candidate observation scope

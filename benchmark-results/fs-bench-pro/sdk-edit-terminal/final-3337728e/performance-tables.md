# LayerFS SDK-edit performance tables — verification pending

Status: **performance collected; verification pending**; verification complete: **False**.

This is a separately identified consumer of unchanged raw collections. It recognizes the producer's unavailable-attribution field alias in memory and applies the explicitly approved policy. Original producer classifications remain retained, not rewritten.

Nominal Edit/Commit/combined targets: 10/10/20 ms. Accepted ceilings: 20/20/30 ms. Edit-only size and matched-operation parity remain binding except for the three explicitly reviewed LC discrepancies below. Commit/combined size and matched-operation spreads are diagnostic. No size-independent Commit claim is made.

Reviewed exceptions (strict results retained): delete-middle Edit cross-size spread 2.571958 ms; replace-shrink Edit cross-size spread 2.111083 ms; delete versus truncate at 1 MiB Edit spread 2.484458 ms. These are accepted by explicit user review, not represented as passing the original 2 ms rule. No arbitrary future exception is authorized.

Memory uses ack-window-v1: native whole-worker/container peaks are conservative lifetime bounds; category maxima and transient swap observations are sampled, not continuous proofs. Exact cgroup edit-phase attribution is unavailable. No old failed/incomplete campaign is pooled here.

## edit_length_preserving

Performance rows: 120; final classification: verification pending.

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| overwrite-head-4k | 1 | baseline | 5 | 18.590 (15.186–23.099) | 2.978 (1.975–3.346) | 21.937 (17.973–26.077) |
| overwrite-head-4k | 1 | candidate | 5 | 2.643 (1.376–4.508) | 2.170 (2.113–4.158) | 5.402 (4.346–7.905) |
| overwrite-head-4k | 10 | baseline | 5 | 16.663 (13.058–32.213) | 2.704 (2.325–3.847) | 19.174 (15.382–35.289) |
| overwrite-head-4k | 10 | candidate | 5 | 3.416 (1.585–5.413) | 2.602 (2.569–5.988) | 5.985 (4.492–9.936) |
| overwrite-head-4k | 100 | baseline | 5 | 20.512 (16.652–23.592) | 7.650 (5.428–17.899) | 27.750 (22.516–38.411) |
| overwrite-head-4k | 100 | candidate | 5 | 2.727 (1.892–5.255) | 4.405 (3.725–7.485) | 6.928 (6.602–9.966) |
| overwrite-head-4k | 500 | baseline | 5 | 16.151 (13.236–19.496) | 10.470 (7.359–81.480) | 26.755 (21.965–97.631) |
| overwrite-head-4k | 500 | candidate | 5 | 2.602 (1.537–2.947) | 9.586 (7.312–11.187) | 12.188 (9.129–13.165) |
| overwrite-middle-4k | 1 | baseline | 5 | 15.299 (12.221–53.441) | 3.447 (2.728–33.722) | 18.511 (15.668–87.163) |
| overwrite-middle-4k | 1 | candidate | 5 | 1.990 (1.296–5.913) | 2.779 (2.246–3.795) | 5.409 (4.075–9.307) |
| overwrite-middle-4k | 10 | baseline | 5 | 16.661 (15.729–22.909) | 3.451 (2.832–4.014) | 20.035 (18.560–26.503) |
| overwrite-middle-4k | 10 | candidate | 5 | 1.939 (1.250–8.074) | 3.176 (2.663–5.210) | 5.115 (4.206–13.284) |
| overwrite-middle-4k | 100 | baseline | 5 | 22.082 (16.944–31.522) | 6.678 (3.489–12.365) | 27.641 (20.433–43.887) |
| overwrite-middle-4k | 100 | candidate | 5 | 2.888 (1.741–4.774) | 5.503 (4.722–9.054) | 8.238 (6.463–11.971) |
| overwrite-middle-4k | 500 | baseline | 5 | 15.960 (14.703–18.062) | 11.097 (7.545–13.610) | 26.769 (23.504–31.672) |
| overwrite-middle-4k | 500 | candidate | 5 | 2.650 (1.282–4.089) | 11.146 (7.567–11.940) | 12.685 (10.217–15.235) |
| overwrite-tail-4k | 1 | baseline | 5 | 23.396 (17.152–35.623) | 2.767 (2.226–3.452) | 25.622 (19.919–38.817) |
| overwrite-tail-4k | 1 | candidate | 5 | 1.527 (1.120–3.012) | 2.295 (2.093–3.285) | 3.770 (3.501–5.390) |
| overwrite-tail-4k | 10 | baseline | 5 | 15.929 (13.673–19.579) | 2.900 (2.442–3.141) | 19.041 (16.813–22.021) |
| overwrite-tail-4k | 10 | candidate | 5 | 1.815 (1.306–2.788) | 2.490 (2.160–2.977) | 4.341 (3.745–5.278) |
| overwrite-tail-4k | 100 | baseline | 5 | 17.000 (14.582–19.642) | 4.848 (3.555–4.983) | 20.727 (19.430–24.508) |
| overwrite-tail-4k | 100 | candidate | 5 | 1.972 (1.258–4.043) | 4.101 (3.409–8.520) | 6.899 (5.359–10.617) |
| overwrite-tail-4k | 500 | baseline | 5 | 14.266 (11.147–20.241) | 10.519 (6.759–19.236) | 23.120 (17.907–33.501) |
| overwrite-tail-4k | 500 | candidate | 5 | 2.508 (1.579–4.412) | 9.751 (7.279–11.877) | 11.331 (9.788–16.288) |

| Operation | MiB | Candidate native RSS max MiB | Native cgroup max MiB | Sampled dirty/writeback increment max MiB |
| --- | ---: | ---: | ---: | ---: |
| overwrite-head-4k | 1 | 7.297 | 4.977 | 0.000 |
| overwrite-head-4k | 10 | 7.656 | 6.652 | 0.000 |
| overwrite-head-4k | 100 | 8.141 | 6.430 | 0.000 |
| overwrite-head-4k | 500 | 9.812 | 6.191 | 0.000 |
| overwrite-middle-4k | 1 | 7.219 | 6.125 | 0.000 |
| overwrite-middle-4k | 10 | 7.656 | 4.695 | 0.000 |
| overwrite-middle-4k | 100 | 8.391 | 6.391 | 0.000 |
| overwrite-middle-4k | 500 | 10.141 | 6.336 | 0.000 |
| overwrite-tail-4k | 1 | 7.250 | 4.570 | 0.000 |
| overwrite-tail-4k | 10 | 7.266 | 4.590 | 0.000 |
| overwrite-tail-4k | 100 | 7.984 | 4.633 | 0.000 |
| overwrite-tail-4k | 500 | 9.703 | 6.410 | 0.000 |

Remaining findings:

- Final independent verification and repository gates are pending.

## edit_length_changing

Performance rows: 320; final classification: verification pending.

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| insert-middle-4k | 1 | baseline | 5 | 24.814 (18.951–25.282) | 2.681 (2.402–4.055) | 27.366 (21.572–28.094) |
| insert-middle-4k | 1 | candidate | 5 | 2.502 (1.545–4.522) | 2.294 (2.030–3.878) | 4.842 (4.436–7.284) |
| insert-middle-4k | 10 | baseline | 5 | 18.915 (14.267–27.090) | 3.954 (2.457–5.051) | 22.272 (17.181–31.044) |
| insert-middle-4k | 10 | candidate | 5 | 2.001 (1.455–8.639) | 3.273 (2.674–4.168) | 5.292 (4.674–12.806) |
| insert-middle-4k | 100 | baseline | 5 | 16.771 (10.165–61.033) | 6.087 (3.553–15.576) | 21.734 (14.811–76.609) |
| insert-middle-4k | 100 | candidate | 5 | 2.331 (1.873–2.586) | 5.495 (4.073–8.041) | 7.752 (6.404–9.913) |
| insert-middle-4k | 500 | baseline | 5 | 22.361 (16.200–23.202) | 10.907 (8.051–16.538) | 31.406 (24.251–39.002) |
| insert-middle-4k | 500 | candidate | 5 | 2.245 (1.761–5.089) | 11.293 (8.036–15.587) | 13.538 (9.797–20.677) |
| delete-middle-4k | 1 | baseline | 5 | 24.424 (17.563–44.046) | 4.782 (2.842–7.762) | 29.640 (21.148–46.888) |
| delete-middle-4k | 1 | candidate | 5 | 5.221 (1.234–7.767) | 2.701 (1.939–6.425) | 10.283 (3.757–11.646) |
| delete-middle-4k | 10 | baseline | 5 | 19.302 (16.188–25.393) | 3.244 (2.814–4.558) | 22.402 (19.002–29.951) |
| delete-middle-4k | 10 | candidate | 5 | 2.715 (1.985–7.124) | 2.803 (2.401–6.319) | 5.643 (5.041–11.268) |
| delete-middle-4k | 100 | baseline | 5 | 16.537 (14.308–27.491) | 4.909 (4.229–8.993) | 21.064 (18.537–36.484) |
| delete-middle-4k | 100 | candidate | 5 | 2.698 (1.362–3.504) | 5.913 (3.484–11.370) | 9.191 (4.952–14.069) |
| delete-middle-4k | 500 | baseline | 5 | 26.682 (15.597–33.613) | 12.000 (7.716–16.570) | 39.815 (23.314–49.027) |
| delete-middle-4k | 500 | candidate | 5 | 2.649 (2.350–3.335) | 11.412 (7.259–16.404) | 14.006 (9.908–18.832) |
| append-tail-4k | 1 | baseline | 5 | 18.924 (17.269–20.710) | 2.872 (2.058–3.677) | 22.388 (20.036–23.582) |
| append-tail-4k | 1 | candidate | 5 | 2.017 (1.272–2.503) | 3.603 (1.958–8.680) | 6.003 (3.975–9.952) |
| append-tail-4k | 10 | baseline | 5 | 20.717 (16.156–22.825) | 4.047 (3.304–7.168) | 25.020 (19.460–29.993) |
| append-tail-4k | 10 | candidate | 5 | 3.172 (2.084–4.082) | 3.101 (2.761–4.665) | 6.515 (5.032–7.184) |
| append-tail-4k | 100 | baseline | 5 | 23.297 (18.321–36.841) | 4.548 (3.951–12.856) | 27.271 (22.272–49.697) |
| append-tail-4k | 100 | candidate | 5 | 3.016 (2.486–4.854) | 4.891 (2.664–6.043) | 7.569 (5.150–10.896) |
| append-tail-4k | 500 | baseline | 5 | 18.657 (15.149–25.877) | 12.159 (8.598–19.012) | 28.754 (25.595–41.330) |
| append-tail-4k | 500 | candidate | 5 | 2.010 (1.484–3.408) | 12.354 (6.709–16.029) | 14.364 (8.193–17.655) |
| prepend-head-4k | 1 | baseline | 5 | 20.612 (15.781–23.596) | 4.619 (2.653–7.266) | 23.854 (21.210–28.215) |
| prepend-head-4k | 1 | candidate | 5 | 1.674 (1.415–3.975) | 2.919 (2.199–8.842) | 4.680 (4.058–10.286) |
| prepend-head-4k | 10 | baseline | 5 | 19.372 (14.125–20.623) | 3.404 (2.782–5.024) | 23.405 (19.149–24.015) |
| prepend-head-4k | 10 | candidate | 5 | 2.164 (1.308–3.062) | 2.779 (1.921–4.107) | 4.883 (3.230–6.271) |
| prepend-head-4k | 100 | baseline | 5 | 19.278 (13.904–23.483) | 5.149 (4.788–5.922) | 24.727 (19.543–28.271) |
| prepend-head-4k | 100 | candidate | 5 | 2.967 (1.933–7.272) | 4.289 (3.029–6.163) | 7.257 (4.963–11.718) |
| prepend-head-4k | 500 | baseline | 5 | 20.187 (10.816–41.251) | 12.628 (10.603–16.659) | 30.790 (21.826–57.909) |
| prepend-head-4k | 500 | candidate | 5 | 3.344 (1.970–4.394) | 11.122 (7.977–12.061) | 14.300 (9.947–16.455) |
| replace-grow-middle-2k-to-4k | 1 | baseline | 5 | 17.243 (13.103–26.602) | 3.711 (2.037–5.069) | 19.577 (15.140–31.671) |
| replace-grow-middle-2k-to-4k | 1 | candidate | 5 | 1.778 (1.441–2.332) | 2.366 (1.994–4.919) | 4.200 (3.834–6.465) |
| replace-grow-middle-2k-to-4k | 10 | baseline | 5 | 21.532 (15.612–46.471) | 3.547 (3.109–11.296) | 24.640 (19.195–57.767) |
| replace-grow-middle-2k-to-4k | 10 | candidate | 5 | 1.695 (1.214–4.444) | 5.355 (2.610–6.149) | 7.610 (3.825–9.799) |
| replace-grow-middle-2k-to-4k | 100 | baseline | 5 | 18.513 (14.622–22.307) | 5.247 (4.744–8.156) | 23.257 (19.869–29.227) |
| replace-grow-middle-2k-to-4k | 100 | candidate | 5 | 2.794 (1.522–4.955) | 6.368 (4.095–11.847) | 9.015 (7.400–16.654) |
| replace-grow-middle-2k-to-4k | 500 | baseline | 5 | 18.146 (12.796–24.340) | 12.932 (11.782–13.736) | 31.078 (24.578–37.539) |
| replace-grow-middle-2k-to-4k | 500 | candidate | 5 | 2.331 (1.783–6.046) | 10.855 (7.557–17.222) | 15.049 (9.878–20.960) |
| replace-shrink-middle-4k-to-2k | 1 | baseline | 5 | 19.062 (17.017–27.435) | 3.394 (2.858–4.553) | 22.320 (20.836–30.829) |
| replace-shrink-middle-4k-to-2k | 1 | candidate | 5 | 1.589 (1.297–1.865) | 2.333 (2.004–3.532) | 3.930 (3.347–4.830) |
| replace-shrink-middle-4k-to-2k | 10 | baseline | 5 | 18.597 (14.375–20.552) | 3.891 (3.351–4.062) | 22.659 (17.726–24.182) |
| replace-shrink-middle-4k-to-2k | 10 | candidate | 5 | 1.649 (1.170–2.656) | 3.836 (2.721–5.130) | 5.485 (4.300–7.786) |
| replace-shrink-middle-4k-to-2k | 100 | baseline | 5 | 20.398 (15.683–24.791) | 5.470 (4.311–7.479) | 24.709 (20.295–32.270) |
| replace-shrink-middle-4k-to-2k | 100 | candidate | 5 | 2.605 (1.272–3.349) | 5.246 (4.027–8.293) | 7.893 (6.518–10.354) |
| replace-shrink-middle-4k-to-2k | 500 | baseline | 5 | 16.610 (11.799–21.233) | 11.204 (7.999–13.317) | 29.232 (23.003–31.654) |
| replace-shrink-middle-4k-to-2k | 500 | candidate | 5 | 3.700 (2.197–4.853) | 11.994 (8.859–18.451) | 16.847 (11.358–22.370) |
| truncate-tail-4k | 1 | baseline | 5 | 17.760 (14.549–21.605) | 2.439 (2.152–3.324) | 20.942 (17.493–23.756) |
| truncate-tail-4k | 1 | candidate | 5 | 2.737 (1.625–3.997) | 2.517 (1.952–2.772) | 4.994 (4.023–6.769) |
| truncate-tail-4k | 10 | baseline | 5 | 21.612 (15.818–29.061) | 3.260 (2.261–7.498) | 24.957 (18.080–36.559) |
| truncate-tail-4k | 10 | candidate | 5 | 1.599 (1.414–2.506) | 2.444 (2.262–4.604) | 4.246 (3.676–6.202) |
| truncate-tail-4k | 100 | baseline | 5 | 18.795 (16.123–23.784) | 5.586 (3.647–12.644) | 27.431 (23.573–30.647) |
| truncate-tail-4k | 100 | candidate | 5 | 2.136 (1.371–4.338) | 3.866 (3.701–6.064) | 5.837 (5.544–8.520) |
| truncate-tail-4k | 500 | baseline | 5 | 18.341 (17.383–27.923) | 10.080 (7.225–14.077) | 32.418 (28.110–35.819) |
| truncate-tail-4k | 500 | candidate | 5 | 2.295 (1.456–2.622) | 9.157 (6.920–12.483) | 11.304 (9.543–14.778) |
| zero-extend-tail-4k | 1 | baseline | 5 | 20.874 (18.291–30.216) | 3.342 (2.489–5.562) | 23.363 (21.633–35.779) |
| zero-extend-tail-4k | 1 | candidate | 5 | 3.090 (1.363–3.803) | 2.687 (2.052–4.393) | 5.767 (4.035–7.483) |
| zero-extend-tail-4k | 10 | baseline | 5 | 14.781 (13.954–23.126) | 3.050 (2.421–7.885) | 17.328 (16.481–31.011) |
| zero-extend-tail-4k | 10 | candidate | 5 | 1.892 (1.457–2.920) | 4.630 (2.736–7.737) | 7.100 (4.193–10.657) |
| zero-extend-tail-4k | 100 | baseline | 5 | 17.318 (12.889–20.024) | 5.067 (3.172–6.330) | 21.944 (18.724–24.641) |
| zero-extend-tail-4k | 100 | candidate | 5 | 2.325 (1.517–6.586) | 4.051 (3.772–5.728) | 7.115 (5.515–10.637) |
| zero-extend-tail-4k | 500 | baseline | 5 | 23.089 (12.082–29.941) | 10.705 (6.849–16.625) | 35.847 (19.121–46.566) |
| zero-extend-tail-4k | 500 | candidate | 5 | 1.925 (1.485–4.129) | 9.985 (7.767–10.477) | 11.896 (9.931–13.007) |

| Operation | MiB | Candidate native RSS max MiB | Native cgroup max MiB | Sampled dirty/writeback increment max MiB |
| --- | ---: | ---: | ---: | ---: |
| insert-middle-4k | 1 | 7.172 | 4.270 | 0.000 |
| insert-middle-4k | 10 | 7.438 | 4.820 | 0.000 |
| insert-middle-4k | 100 | 8.203 | 4.773 | 0.000 |
| insert-middle-4k | 500 | 9.938 | 4.500 | 0.000 |
| delete-middle-4k | 1 | 7.203 | 4.844 | 0.000 |
| delete-middle-4k | 10 | 7.578 | 4.168 | 0.000 |
| delete-middle-4k | 100 | 8.188 | 5.973 | 0.000 |
| delete-middle-4k | 500 | 9.984 | 4.250 | 0.000 |
| append-tail-4k | 1 | 7.266 | 4.750 | 0.000 |
| append-tail-4k | 10 | 7.266 | 4.309 | 0.000 |
| append-tail-4k | 100 | 7.688 | 4.527 | 0.000 |
| append-tail-4k | 500 | 9.234 | 6.188 | 0.000 |
| prepend-head-4k | 1 | 7.234 | 4.551 | 0.000 |
| prepend-head-4k | 10 | 7.281 | 4.191 | 0.000 |
| prepend-head-4k | 100 | 7.984 | 4.914 | 0.000 |
| prepend-head-4k | 500 | 9.531 | 4.504 | 0.000 |
| replace-grow-middle-2k-to-4k | 1 | 7.172 | 4.426 | 0.000 |
| replace-grow-middle-2k-to-4k | 10 | 7.516 | 4.578 | 0.000 |
| replace-grow-middle-2k-to-4k | 100 | 8.172 | 4.492 | 0.000 |
| replace-grow-middle-2k-to-4k | 500 | 10.000 | 4.824 | 0.000 |
| replace-shrink-middle-4k-to-2k | 1 | 7.141 | 4.387 | 0.000 |
| replace-shrink-middle-4k-to-2k | 10 | 7.312 | 4.273 | 0.000 |
| replace-shrink-middle-4k-to-2k | 100 | 8.172 | 5.758 | 0.000 |
| replace-shrink-middle-4k-to-2k | 500 | 10.141 | 4.359 | 0.000 |
| truncate-tail-4k | 1 | 7.219 | 4.461 | 0.000 |
| truncate-tail-4k | 10 | 7.312 | 4.320 | 0.000 |
| truncate-tail-4k | 100 | 7.578 | 4.574 | 0.000 |
| truncate-tail-4k | 500 | 9.500 | 4.324 | 0.000 |
| zero-extend-tail-4k | 1 | 7.125 | 4.586 | 0.000 |
| zero-extend-tail-4k | 10 | 7.219 | 4.156 | 0.000 |
| zero-extend-tail-4k | 100 | 7.844 | 4.418 | 0.000 |
| zero-extend-tail-4k | 500 | 9.391 | 4.297 | 0.000 |

Remaining findings:

- Final independent verification and repository gates are pending.

## edit_canonical_chunk_count

Performance rows: 120; final classification: verification pending.

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| overwrite-fixed-64k-chunk-count-preserve | 1 | baseline | 5 | 19.342 (14.085–25.075) | 6.790 (2.655–9.487) | 25.611 (18.791–34.562) |
| overwrite-fixed-64k-chunk-count-preserve | 1 | candidate | 5 | 2.782 (1.234–7.541) | 3.932 (2.302–8.028) | 6.715 (3.537–15.569) |
| overwrite-fixed-64k-chunk-count-preserve | 10 | baseline | 5 | 20.896 (17.393–27.611) | 5.256 (4.087–6.528) | 27.424 (22.026–32.867) |
| overwrite-fixed-64k-chunk-count-preserve | 10 | candidate | 5 | 2.832 (1.564–3.197) | 4.144 (3.032–5.043) | 6.198 (5.239–7.875) |
| overwrite-fixed-64k-chunk-count-preserve | 100 | baseline | 5 | 17.809 (14.293–22.567) | 5.714 (3.959–5.925) | 23.261 (20.218–28.485) |
| overwrite-fixed-64k-chunk-count-preserve | 100 | candidate | 5 | 3.183 (1.419–7.251) | 5.116 (4.091–5.273) | 8.060 (5.675–12.367) |
| overwrite-fixed-64k-chunk-count-preserve | 500 | baseline | 5 | 14.229 (12.745–20.011) | 12.742 (8.275–17.585) | 26.592 (22.505–37.596) |
| overwrite-fixed-64k-chunk-count-preserve | 500 | candidate | 5 | 2.550 (1.931–5.408) | 12.023 (8.338–15.876) | 14.573 (12.863–18.022) |
| overwrite-fixed-64k-chunk-count-increase | 1 | baseline | 5 | 18.102 (17.036–22.758) | 3.850 (2.858–4.956) | 21.877 (20.742–27.714) |
| overwrite-fixed-64k-chunk-count-increase | 1 | candidate | 5 | 1.855 (1.343–2.983) | 3.056 (2.626–3.591) | 5.069 (3.968–6.469) |
| overwrite-fixed-64k-chunk-count-increase | 10 | baseline | 5 | 24.376 (16.859–27.818) | 5.202 (3.606–5.835) | 30.137 (20.465–33.653) |
| overwrite-fixed-64k-chunk-count-increase | 10 | candidate | 5 | 2.718 (1.242–4.115) | 3.263 (3.142–6.043) | 6.272 (4.391–9.884) |
| overwrite-fixed-64k-chunk-count-increase | 100 | baseline | 5 | 25.079 (14.188–31.967) | 5.543 (5.035–6.251) | 30.249 (19.731–37.002) |
| overwrite-fixed-64k-chunk-count-increase | 100 | candidate | 5 | 1.677 (1.458–2.361) | 4.850 (4.571–6.982) | 6.932 (6.053–8.660) |
| overwrite-fixed-64k-chunk-count-increase | 500 | baseline | 5 | 16.093 (12.227–27.852) | 13.209 (9.415–21.814) | 30.062 (25.436–49.666) |
| overwrite-fixed-64k-chunk-count-increase | 500 | candidate | 5 | 2.471 (1.538–2.783) | 11.471 (7.930–15.396) | 13.008 (10.713–17.127) |
| overwrite-fixed-64k-chunk-count-decrease | 1 | baseline | 5 | 19.558 (13.960–23.243) | 2.602 (2.356–4.657) | 22.160 (16.315–25.677) |
| overwrite-fixed-64k-chunk-count-decrease | 1 | candidate | 5 | 1.997 (1.367–3.190) | 3.084 (2.419–3.437) | 5.040 (4.436–5.609) |
| overwrite-fixed-64k-chunk-count-decrease | 10 | baseline | 5 | 19.099 (13.678–23.248) | 3.694 (3.478–5.859) | 22.609 (17.157–27.722) |
| overwrite-fixed-64k-chunk-count-decrease | 10 | candidate | 5 | 1.623 (1.214–3.193) | 3.518 (2.760–3.826) | 5.141 (4.201–7.018) |
| overwrite-fixed-64k-chunk-count-decrease | 100 | baseline | 5 | 16.584 (15.182–20.402) | 5.069 (4.556–6.124) | 21.948 (20.505–25.471) |
| overwrite-fixed-64k-chunk-count-decrease | 100 | candidate | 5 | 3.119 (2.002–5.162) | 7.430 (5.215–8.120) | 10.433 (7.216–12.592) |
| overwrite-fixed-64k-chunk-count-decrease | 500 | baseline | 5 | 16.394 (14.647–24.146) | 11.545 (9.654–13.221) | 27.143 (26.048–37.367) |
| overwrite-fixed-64k-chunk-count-decrease | 500 | candidate | 5 | 2.319 (2.165–4.706) | 10.454 (8.398–12.847) | 13.438 (10.717–15.160) |

| Operation | MiB | Candidate native RSS max MiB | Native cgroup max MiB | Sampled dirty/writeback increment max MiB |
| --- | ---: | ---: | ---: | ---: |
| overwrite-fixed-64k-chunk-count-preserve | 1 | 8.172 | 4.230 | 0.000 |
| overwrite-fixed-64k-chunk-count-preserve | 10 | 8.297 | 4.191 | 0.000 |
| overwrite-fixed-64k-chunk-count-preserve | 100 | 8.875 | 4.238 | 0.000 |
| overwrite-fixed-64k-chunk-count-preserve | 500 | 10.844 | 5.996 | 0.000 |
| overwrite-fixed-64k-chunk-count-increase | 1 | 8.156 | 4.160 | 0.000 |
| overwrite-fixed-64k-chunk-count-increase | 10 | 8.391 | 4.504 | 0.000 |
| overwrite-fixed-64k-chunk-count-increase | 100 | 9.047 | 4.504 | 0.000 |
| overwrite-fixed-64k-chunk-count-increase | 500 | 10.922 | 4.066 | 0.000 |
| overwrite-fixed-64k-chunk-count-decrease | 1 | 7.656 | 4.043 | 0.000 |
| overwrite-fixed-64k-chunk-count-decrease | 10 | 8.156 | 4.844 | 0.000 |
| overwrite-fixed-64k-chunk-count-decrease | 100 | 8.797 | 4.293 | 0.000 |
| overwrite-fixed-64k-chunk-count-decrease | 500 | 10.812 | 4.840 | 0.000 |

Remaining findings:

- Final independent verification and repository gates are pending.

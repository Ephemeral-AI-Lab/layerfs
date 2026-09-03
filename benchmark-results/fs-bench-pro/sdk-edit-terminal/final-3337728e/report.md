# LayerFS SDK-edit final classification

Status: **pass**; verification complete: **True**.

This is a separately identified consumer of unchanged raw collections. It recognizes the producer's unavailable-attribution field alias in memory and applies the explicitly approved policy. Original producer classifications remain retained, not rewritten.

Nominal Edit/Commit/combined targets: 10/10/20 ms. Accepted ceilings: 20/20/30 ms. Edit-only size and matched-operation parity remain binding except for the three explicitly reviewed LC discrepancies below. Commit/combined size and matched-operation spreads are diagnostic. No size-independent Commit claim is made.

Reviewed exceptions (strict results retained): delete-middle Edit cross-size spread 2.571958 ms; replace-shrink Edit cross-size spread 2.111083 ms; delete versus truncate at 1 MiB Edit spread 2.484458 ms. These are accepted by explicit user review, not represented as passing the original 2 ms rule. No arbitrary future exception is authorized.

Memory uses ack-window-v1: native whole-worker/container peaks are conservative lifetime bounds; category maxima and transient swap observations are sampled, not continuous proofs. Exact cgroup edit-phase attribution is unavailable. No old failed/incomplete campaign is pooled here.

Admission eligibility (including repository gates): **True**.

Raw bundles, source identities, and SHA-256 manifests are pinned in [inputs.json](inputs.json). Full machine-readable statistics and original findings are in [classification.json](classification.json).

Performance collection finished for all three families before verification. One baseline 10 MiB zero-extension verifier returned InvalidRequest; its container exited 0 without OOM. The failed attempt is retained, six missing proofs passed on retry, and the original 58-proof prefix and all performance bytes were preserved. Root cause of that isolated control error remains unproven.

## edit_length_preserving

Performance rows: 120; final classification: pass.

[Raw performance](../../edit-length-preserving/terminal-3337728e/performance/raw.jsonl) · [Verification subproofs](../../edit-length-preserving/terminal-3337728e/verification/subproofs.jsonl) · [Manifest](../../edit-length-preserving/terminal-3337728e/evidence.sha256)

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms | Latency status |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| overwrite-head-4k | 1 | baseline | 5 | 18.590 (15.186–23.099) | 2.978 (1.975–3.346) | 21.937 (17.973–26.077) | directional comparator |
| overwrite-head-4k | 1 | candidate | 5 | 2.643 (1.376–4.508) | 2.170 (2.113–4.158) | 5.402 (4.346–7.905) | nominal-pass |
| overwrite-head-4k | 10 | baseline | 5 | 16.663 (13.058–32.213) | 2.704 (2.325–3.847) | 19.174 (15.382–35.289) | directional comparator |
| overwrite-head-4k | 10 | candidate | 5 | 3.416 (1.585–5.413) | 2.602 (2.569–5.988) | 5.985 (4.492–9.936) | nominal-pass |
| overwrite-head-4k | 100 | baseline | 5 | 20.512 (16.652–23.592) | 7.650 (5.428–17.899) | 27.750 (22.516–38.411) | directional comparator |
| overwrite-head-4k | 100 | candidate | 5 | 2.727 (1.892–5.255) | 4.405 (3.725–7.485) | 6.928 (6.602–9.966) | nominal-pass |
| overwrite-head-4k | 500 | baseline | 5 | 16.151 (13.236–19.496) | 10.470 (7.359–81.480) | 26.755 (21.965–97.631) | directional comparator |
| overwrite-head-4k | 500 | candidate | 5 | 2.602 (1.537–2.947) | 9.586 (7.312–11.187) | 12.188 (9.129–13.165) | nominal-pass |
| overwrite-middle-4k | 1 | baseline | 5 | 15.299 (12.221–53.441) | 3.447 (2.728–33.722) | 18.511 (15.668–87.163) | directional comparator |
| overwrite-middle-4k | 1 | candidate | 5 | 1.990 (1.296–5.913) | 2.779 (2.246–3.795) | 5.409 (4.075–9.307) | nominal-pass |
| overwrite-middle-4k | 10 | baseline | 5 | 16.661 (15.729–22.909) | 3.451 (2.832–4.014) | 20.035 (18.560–26.503) | directional comparator |
| overwrite-middle-4k | 10 | candidate | 5 | 1.939 (1.250–8.074) | 3.176 (2.663–5.210) | 5.115 (4.206–13.284) | nominal-pass |
| overwrite-middle-4k | 100 | baseline | 5 | 22.082 (16.944–31.522) | 6.678 (3.489–12.365) | 27.641 (20.433–43.887) | directional comparator |
| overwrite-middle-4k | 100 | candidate | 5 | 2.888 (1.741–4.774) | 5.503 (4.722–9.054) | 8.238 (6.463–11.971) | nominal-pass |
| overwrite-middle-4k | 500 | baseline | 5 | 15.960 (14.703–18.062) | 11.097 (7.545–13.610) | 26.769 (23.504–31.672) | directional comparator |
| overwrite-middle-4k | 500 | candidate | 5 | 2.650 (1.282–4.089) | 11.146 (7.567–11.940) | 12.685 (10.217–15.235) | accepted-with-tolerance |
| overwrite-tail-4k | 1 | baseline | 5 | 23.396 (17.152–35.623) | 2.767 (2.226–3.452) | 25.622 (19.919–38.817) | directional comparator |
| overwrite-tail-4k | 1 | candidate | 5 | 1.527 (1.120–3.012) | 2.295 (2.093–3.285) | 3.770 (3.501–5.390) | nominal-pass |
| overwrite-tail-4k | 10 | baseline | 5 | 15.929 (13.673–19.579) | 2.900 (2.442–3.141) | 19.041 (16.813–22.021) | directional comparator |
| overwrite-tail-4k | 10 | candidate | 5 | 1.815 (1.306–2.788) | 2.490 (2.160–2.977) | 4.341 (3.745–5.278) | nominal-pass |
| overwrite-tail-4k | 100 | baseline | 5 | 17.000 (14.582–19.642) | 4.848 (3.555–4.983) | 20.727 (19.430–24.508) | directional comparator |
| overwrite-tail-4k | 100 | candidate | 5 | 1.972 (1.258–4.043) | 4.101 (3.409–8.520) | 6.899 (5.359–10.617) | nominal-pass |
| overwrite-tail-4k | 500 | baseline | 5 | 14.266 (11.147–20.241) | 10.519 (6.759–19.236) | 23.120 (17.907–33.501) | directional comparator |
| overwrite-tail-4k | 500 | candidate | 5 | 2.508 (1.579–4.412) | 9.751 (7.279–11.877) | 11.331 (9.788–16.288) | nominal-pass |

Memory cells are median (min–max), MiB, N=5 performance workers per arm. Native peaks bound whole lifetimes; window/category observations are sampled, not exact-phase or continuous maxima.

| Operation | MiB | Arm | Native RSS lifetime peak | Native cgroup lifetime peak | Sampled cgroup window peak | Sampled cgroup window increment | Sampled RSS increment | Sampled dirty/writeback increment |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| overwrite-head-4k | 1 | baseline | 7.078 (7.016–7.219) | 6.422 (3.871–7.070) | 2.504 (2.402–2.797) | 0.590 (0.223–0.715) | 1.188 (1.172–1.250) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 1 | candidate | 7.062 (7.000–7.297) | 4.289 (3.848–4.977) | 2.184 (2.027–2.480) | 0.125 (0.000–0.254) | 1.172 (1.125–1.203) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 10 | baseline | 7.359 (7.250–7.562) | 4.574 (3.656–6.816) | 2.566 (2.410–2.945) | 0.535 (0.203–0.879) | 1.500 (1.453–1.516) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 10 | candidate | 7.422 (7.281–7.656) | 4.160 (4.043–6.652) | 2.344 (2.203–2.496) | 0.219 (0.055–0.574) | 1.484 (1.438–1.500) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 100 | baseline | 8.094 (8.000–8.125) | 4.227 (3.863–6.590) | 2.539 (2.516–2.930) | 0.559 (0.395–1.008) | 1.734 (1.703–1.906) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 100 | candidate | 8.016 (7.938–8.141) | 4.281 (4.246–6.430) | 2.438 (2.008–2.496) | 0.289 (0.086–0.461) | 1.844 (1.703–1.859) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 500 | baseline | 9.703 (9.594–9.750) | 4.824 (3.867–6.137) | 2.578 (2.414–3.766) | 0.387 (0.066–1.832) | 2.953 (2.875–3.000) | 0.000 (0.000–0.000) |
| overwrite-head-4k | 500 | candidate | 9.734 (9.719–9.812) | 4.363 (3.660–6.191) | 2.320 (2.254–2.402) | 0.043 (0.000–0.355) | 2.906 (2.859–2.969) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 1 | baseline | 7.094 (7.031–7.344) | 4.113 (3.754–4.957) | 2.621 (2.480–2.641) | 0.355 (0.203–0.414) | 1.250 (1.234–1.297) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 1 | candidate | 7.188 (7.000–7.219) | 4.566 (4.168–6.125) | 2.414 (2.242–2.520) | 0.340 (0.246–0.520) | 1.156 (1.156–1.172) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 10 | baseline | 7.406 (7.328–7.578) | 4.266 (4.008–6.473) | 2.637 (2.320–2.750) | 0.488 (0.242–0.586) | 1.562 (1.516–1.609) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 10 | candidate | 7.562 (7.328–7.656) | 4.496 (3.887–4.695) | 2.258 (2.219–2.508) | 0.254 (0.039–0.383) | 1.500 (1.500–1.578) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 100 | baseline | 8.156 (8.125–8.312) | 3.871 (3.805–6.457) | 2.594 (2.402–2.652) | 0.375 (0.047–0.469) | 2.000 (1.922–2.016) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 100 | candidate | 8.281 (8.078–8.391) | 4.391 (3.734–6.391) | 2.266 (2.102–2.395) | 0.000 (0.000–0.340) | 1.891 (1.859–1.969) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 500 | baseline | 9.969 (9.906–10.172) | 4.227 (3.848–6.227) | 2.680 (2.461–2.898) | 0.473 (0.449–0.930) | 3.312 (3.219–3.359) | 0.000 (0.000–0.000) |
| overwrite-middle-4k | 500 | candidate | 9.969 (9.797–10.141) | 5.023 (3.680–6.336) | 2.367 (2.160–2.527) | 0.277 (0.000–0.332) | 3.234 (3.125–3.266) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 1 | baseline | 7.156 (7.000–7.203) | 3.730 (3.688–4.992) | 2.641 (2.551–2.656) | 0.383 (0.133–0.598) | 1.141 (1.078–1.219) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 1 | candidate | 7.188 (6.984–7.250) | 4.488 (3.863–4.570) | 2.332 (2.211–2.477) | 0.262 (0.250–0.574) | 1.125 (1.062–1.188) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 10 | baseline | 7.109 (7.078–7.281) | 4.211 (3.961–6.824) | 2.504 (2.195–2.801) | 0.270 (0.172–0.758) | 1.250 (1.109–1.297) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 10 | candidate | 7.141 (6.953–7.266) | 4.402 (4.008–4.590) | 2.441 (2.238–2.637) | 0.164 (0.000–0.301) | 1.188 (1.094–1.297) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 100 | baseline | 7.781 (7.562–7.906) | 4.516 (3.547–6.387) | 2.527 (2.305–2.758) | 0.484 (0.000–0.648) | 1.469 (1.453–1.531) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 100 | candidate | 7.688 (7.609–7.984) | 4.375 (3.883–4.633) | 2.477 (2.184–2.555) | 0.309 (0.141–0.383) | 1.500 (1.391–1.609) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 500 | baseline | 9.562 (9.266–9.672) | 4.812 (3.746–6.816) | 2.641 (2.508–2.660) | 0.500 (0.363–0.746) | 2.750 (2.688–2.766) | 0.000 (0.000–0.000) |
| overwrite-tail-4k | 500 | candidate | 9.469 (9.344–9.703) | 4.188 (3.766–6.410) | 2.484 (2.328–2.562) | 0.180 (0.000–0.391) | 2.719 (2.625–2.812) | 0.000 (0.000–0.000) |

Remaining findings:

- None under the explicitly recorded final policy.

## edit_length_changing

Performance rows: 320; final classification: pass.

[Raw performance](../../edit-length-changing/terminal-3337728e/performance/raw.jsonl) · [Verification subproofs](../../edit-length-changing/terminal-3337728e/verification/subproofs.jsonl) · [Manifest](../../edit-length-changing/terminal-3337728e/evidence.sha256)

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms | Latency status |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| insert-middle-4k | 1 | baseline | 5 | 24.814 (18.951–25.282) | 2.681 (2.402–4.055) | 27.366 (21.572–28.094) | directional comparator |
| insert-middle-4k | 1 | candidate | 5 | 2.502 (1.545–4.522) | 2.294 (2.030–3.878) | 4.842 (4.436–7.284) | nominal-pass |
| insert-middle-4k | 10 | baseline | 5 | 18.915 (14.267–27.090) | 3.954 (2.457–5.051) | 22.272 (17.181–31.044) | directional comparator |
| insert-middle-4k | 10 | candidate | 5 | 2.001 (1.455–8.639) | 3.273 (2.674–4.168) | 5.292 (4.674–12.806) | nominal-pass |
| insert-middle-4k | 100 | baseline | 5 | 16.771 (10.165–61.033) | 6.087 (3.553–15.576) | 21.734 (14.811–76.609) | directional comparator |
| insert-middle-4k | 100 | candidate | 5 | 2.331 (1.873–2.586) | 5.495 (4.073–8.041) | 7.752 (6.404–9.913) | nominal-pass |
| insert-middle-4k | 500 | baseline | 5 | 22.361 (16.200–23.202) | 10.907 (8.051–16.538) | 31.406 (24.251–39.002) | directional comparator |
| insert-middle-4k | 500 | candidate | 5 | 2.245 (1.761–5.089) | 11.293 (8.036–15.587) | 13.538 (9.797–20.677) | accepted-with-tolerance |
| delete-middle-4k | 1 | baseline | 5 | 24.424 (17.563–44.046) | 4.782 (2.842–7.762) | 29.640 (21.148–46.888) | directional comparator |
| delete-middle-4k | 1 | candidate | 5 | 5.221 (1.234–7.767) | 2.701 (1.939–6.425) | 10.283 (3.757–11.646) | nominal-pass |
| delete-middle-4k | 10 | baseline | 5 | 19.302 (16.188–25.393) | 3.244 (2.814–4.558) | 22.402 (19.002–29.951) | directional comparator |
| delete-middle-4k | 10 | candidate | 5 | 2.715 (1.985–7.124) | 2.803 (2.401–6.319) | 5.643 (5.041–11.268) | nominal-pass |
| delete-middle-4k | 100 | baseline | 5 | 16.537 (14.308–27.491) | 4.909 (4.229–8.993) | 21.064 (18.537–36.484) | directional comparator |
| delete-middle-4k | 100 | candidate | 5 | 2.698 (1.362–3.504) | 5.913 (3.484–11.370) | 9.191 (4.952–14.069) | nominal-pass |
| delete-middle-4k | 500 | baseline | 5 | 26.682 (15.597–33.613) | 12.000 (7.716–16.570) | 39.815 (23.314–49.027) | directional comparator |
| delete-middle-4k | 500 | candidate | 5 | 2.649 (2.350–3.335) | 11.412 (7.259–16.404) | 14.006 (9.908–18.832) | accepted-with-tolerance |
| append-tail-4k | 1 | baseline | 5 | 18.924 (17.269–20.710) | 2.872 (2.058–3.677) | 22.388 (20.036–23.582) | directional comparator |
| append-tail-4k | 1 | candidate | 5 | 2.017 (1.272–2.503) | 3.603 (1.958–8.680) | 6.003 (3.975–9.952) | nominal-pass |
| append-tail-4k | 10 | baseline | 5 | 20.717 (16.156–22.825) | 4.047 (3.304–7.168) | 25.020 (19.460–29.993) | directional comparator |
| append-tail-4k | 10 | candidate | 5 | 3.172 (2.084–4.082) | 3.101 (2.761–4.665) | 6.515 (5.032–7.184) | nominal-pass |
| append-tail-4k | 100 | baseline | 5 | 23.297 (18.321–36.841) | 4.548 (3.951–12.856) | 27.271 (22.272–49.697) | directional comparator |
| append-tail-4k | 100 | candidate | 5 | 3.016 (2.486–4.854) | 4.891 (2.664–6.043) | 7.569 (5.150–10.896) | nominal-pass |
| append-tail-4k | 500 | baseline | 5 | 18.657 (15.149–25.877) | 12.159 (8.598–19.012) | 28.754 (25.595–41.330) | directional comparator |
| append-tail-4k | 500 | candidate | 5 | 2.010 (1.484–3.408) | 12.354 (6.709–16.029) | 14.364 (8.193–17.655) | accepted-with-tolerance |
| prepend-head-4k | 1 | baseline | 5 | 20.612 (15.781–23.596) | 4.619 (2.653–7.266) | 23.854 (21.210–28.215) | directional comparator |
| prepend-head-4k | 1 | candidate | 5 | 1.674 (1.415–3.975) | 2.919 (2.199–8.842) | 4.680 (4.058–10.286) | nominal-pass |
| prepend-head-4k | 10 | baseline | 5 | 19.372 (14.125–20.623) | 3.404 (2.782–5.024) | 23.405 (19.149–24.015) | directional comparator |
| prepend-head-4k | 10 | candidate | 5 | 2.164 (1.308–3.062) | 2.779 (1.921–4.107) | 4.883 (3.230–6.271) | nominal-pass |
| prepend-head-4k | 100 | baseline | 5 | 19.278 (13.904–23.483) | 5.149 (4.788–5.922) | 24.727 (19.543–28.271) | directional comparator |
| prepend-head-4k | 100 | candidate | 5 | 2.967 (1.933–7.272) | 4.289 (3.029–6.163) | 7.257 (4.963–11.718) | nominal-pass |
| prepend-head-4k | 500 | baseline | 5 | 20.187 (10.816–41.251) | 12.628 (10.603–16.659) | 30.790 (21.826–57.909) | directional comparator |
| prepend-head-4k | 500 | candidate | 5 | 3.344 (1.970–4.394) | 11.122 (7.977–12.061) | 14.300 (9.947–16.455) | accepted-with-tolerance |
| replace-grow-middle-2k-to-4k | 1 | baseline | 5 | 17.243 (13.103–26.602) | 3.711 (2.037–5.069) | 19.577 (15.140–31.671) | directional comparator |
| replace-grow-middle-2k-to-4k | 1 | candidate | 5 | 1.778 (1.441–2.332) | 2.366 (1.994–4.919) | 4.200 (3.834–6.465) | nominal-pass |
| replace-grow-middle-2k-to-4k | 10 | baseline | 5 | 21.532 (15.612–46.471) | 3.547 (3.109–11.296) | 24.640 (19.195–57.767) | directional comparator |
| replace-grow-middle-2k-to-4k | 10 | candidate | 5 | 1.695 (1.214–4.444) | 5.355 (2.610–6.149) | 7.610 (3.825–9.799) | nominal-pass |
| replace-grow-middle-2k-to-4k | 100 | baseline | 5 | 18.513 (14.622–22.307) | 5.247 (4.744–8.156) | 23.257 (19.869–29.227) | directional comparator |
| replace-grow-middle-2k-to-4k | 100 | candidate | 5 | 2.794 (1.522–4.955) | 6.368 (4.095–11.847) | 9.015 (7.400–16.654) | nominal-pass |
| replace-grow-middle-2k-to-4k | 500 | baseline | 5 | 18.146 (12.796–24.340) | 12.932 (11.782–13.736) | 31.078 (24.578–37.539) | directional comparator |
| replace-grow-middle-2k-to-4k | 500 | candidate | 5 | 2.331 (1.783–6.046) | 10.855 (7.557–17.222) | 15.049 (9.878–20.960) | accepted-with-tolerance |
| replace-shrink-middle-4k-to-2k | 1 | baseline | 5 | 19.062 (17.017–27.435) | 3.394 (2.858–4.553) | 22.320 (20.836–30.829) | directional comparator |
| replace-shrink-middle-4k-to-2k | 1 | candidate | 5 | 1.589 (1.297–1.865) | 2.333 (2.004–3.532) | 3.930 (3.347–4.830) | nominal-pass |
| replace-shrink-middle-4k-to-2k | 10 | baseline | 5 | 18.597 (14.375–20.552) | 3.891 (3.351–4.062) | 22.659 (17.726–24.182) | directional comparator |
| replace-shrink-middle-4k-to-2k | 10 | candidate | 5 | 1.649 (1.170–2.656) | 3.836 (2.721–5.130) | 5.485 (4.300–7.786) | nominal-pass |
| replace-shrink-middle-4k-to-2k | 100 | baseline | 5 | 20.398 (15.683–24.791) | 5.470 (4.311–7.479) | 24.709 (20.295–32.270) | directional comparator |
| replace-shrink-middle-4k-to-2k | 100 | candidate | 5 | 2.605 (1.272–3.349) | 5.246 (4.027–8.293) | 7.893 (6.518–10.354) | nominal-pass |
| replace-shrink-middle-4k-to-2k | 500 | baseline | 5 | 16.610 (11.799–21.233) | 11.204 (7.999–13.317) | 29.232 (23.003–31.654) | directional comparator |
| replace-shrink-middle-4k-to-2k | 500 | candidate | 5 | 3.700 (2.197–4.853) | 11.994 (8.859–18.451) | 16.847 (11.358–22.370) | accepted-with-tolerance |
| truncate-tail-4k | 1 | baseline | 5 | 17.760 (14.549–21.605) | 2.439 (2.152–3.324) | 20.942 (17.493–23.756) | directional comparator |
| truncate-tail-4k | 1 | candidate | 5 | 2.737 (1.625–3.997) | 2.517 (1.952–2.772) | 4.994 (4.023–6.769) | nominal-pass |
| truncate-tail-4k | 10 | baseline | 5 | 21.612 (15.818–29.061) | 3.260 (2.261–7.498) | 24.957 (18.080–36.559) | directional comparator |
| truncate-tail-4k | 10 | candidate | 5 | 1.599 (1.414–2.506) | 2.444 (2.262–4.604) | 4.246 (3.676–6.202) | nominal-pass |
| truncate-tail-4k | 100 | baseline | 5 | 18.795 (16.123–23.784) | 5.586 (3.647–12.644) | 27.431 (23.573–30.647) | directional comparator |
| truncate-tail-4k | 100 | candidate | 5 | 2.136 (1.371–4.338) | 3.866 (3.701–6.064) | 5.837 (5.544–8.520) | nominal-pass |
| truncate-tail-4k | 500 | baseline | 5 | 18.341 (17.383–27.923) | 10.080 (7.225–14.077) | 32.418 (28.110–35.819) | directional comparator |
| truncate-tail-4k | 500 | candidate | 5 | 2.295 (1.456–2.622) | 9.157 (6.920–12.483) | 11.304 (9.543–14.778) | nominal-pass |
| zero-extend-tail-4k | 1 | baseline | 5 | 20.874 (18.291–30.216) | 3.342 (2.489–5.562) | 23.363 (21.633–35.779) | directional comparator |
| zero-extend-tail-4k | 1 | candidate | 5 | 3.090 (1.363–3.803) | 2.687 (2.052–4.393) | 5.767 (4.035–7.483) | nominal-pass |
| zero-extend-tail-4k | 10 | baseline | 5 | 14.781 (13.954–23.126) | 3.050 (2.421–7.885) | 17.328 (16.481–31.011) | directional comparator |
| zero-extend-tail-4k | 10 | candidate | 5 | 1.892 (1.457–2.920) | 4.630 (2.736–7.737) | 7.100 (4.193–10.657) | nominal-pass |
| zero-extend-tail-4k | 100 | baseline | 5 | 17.318 (12.889–20.024) | 5.067 (3.172–6.330) | 21.944 (18.724–24.641) | directional comparator |
| zero-extend-tail-4k | 100 | candidate | 5 | 2.325 (1.517–6.586) | 4.051 (3.772–5.728) | 7.115 (5.515–10.637) | nominal-pass |
| zero-extend-tail-4k | 500 | baseline | 5 | 23.089 (12.082–29.941) | 10.705 (6.849–16.625) | 35.847 (19.121–46.566) | directional comparator |
| zero-extend-tail-4k | 500 | candidate | 5 | 1.925 (1.485–4.129) | 9.985 (7.767–10.477) | 11.896 (9.931–13.007) | nominal-pass |

Memory cells are median (min–max), MiB, N=5 performance workers per arm. Native peaks bound whole lifetimes; window/category observations are sampled, not exact-phase or continuous maxima.

| Operation | MiB | Arm | Native RSS lifetime peak | Native cgroup lifetime peak | Sampled cgroup window peak | Sampled cgroup window increment | Sampled RSS increment | Sampled dirty/writeback increment |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| insert-middle-4k | 1 | baseline | 7.094 (6.984–7.172) | 3.977 (3.883–4.570) | 2.488 (2.352–2.602) | 0.305 (0.074–0.473) | 1.062 (1.016–1.125) | 0.000 (0.000–0.000) |
| insert-middle-4k | 1 | candidate | 7.000 (6.844–7.172) | 3.832 (3.617–4.270) | 2.320 (2.184–2.477) | 0.277 (0.082–0.426) | 1.062 (0.984–1.078) | 0.000 (0.000–0.000) |
| insert-middle-4k | 10 | baseline | 7.406 (7.156–7.531) | 4.148 (3.887–4.164) | 2.613 (2.328–2.719) | 0.594 (0.160–0.766) | 1.375 (1.359–1.406) | 0.000 (0.000–0.000) |
| insert-middle-4k | 10 | candidate | 7.250 (7.219–7.438) | 4.188 (3.746–4.820) | 2.285 (1.957–2.398) | 0.133 (0.000–0.500) | 1.359 (1.312–1.422) | 0.000 (0.000–0.000) |
| insert-middle-4k | 100 | baseline | 7.922 (7.859–8.125) | 3.961 (3.613–4.312) | 2.609 (2.457–2.699) | 0.500 (0.414–0.766) | 1.766 (1.719–1.812) | 0.000 (0.000–0.000) |
| insert-middle-4k | 100 | candidate | 8.078 (7.938–8.203) | 3.934 (3.539–4.773) | 2.281 (2.242–2.566) | 0.211 (0.000–0.406) | 1.828 (1.734–1.859) | 0.000 (0.000–0.000) |
| insert-middle-4k | 500 | baseline | 9.953 (9.766–10.094) | 4.020 (3.680–4.418) | 2.688 (2.535–2.828) | 0.387 (0.273–0.883) | 3.156 (3.125–3.266) | 0.000 (0.000–0.000) |
| insert-middle-4k | 500 | candidate | 9.781 (9.750–9.938) | 4.168 (4.043–4.500) | 2.496 (2.258–2.562) | 0.473 (0.371–0.617) | 3.109 (3.016–3.203) | 0.000 (0.000–0.000) |
| delete-middle-4k | 1 | baseline | 6.891 (6.875–7.188) | 4.180 (3.871–4.664) | 2.477 (2.383–2.680) | 0.332 (0.121–0.570) | 1.016 (1.000–1.047) | 0.000 (0.000–0.000) |
| delete-middle-4k | 1 | candidate | 6.969 (6.922–7.203) | 3.887 (3.695–4.844) | 2.336 (2.141–2.648) | 0.254 (0.043–0.402) | 1.062 (0.969–1.141) | 0.000 (0.000–0.000) |
| delete-middle-4k | 10 | baseline | 7.266 (7.156–7.406) | 3.773 (3.500–4.531) | 2.492 (2.367–2.613) | 0.250 (0.109–0.551) | 1.375 (1.344–1.453) | 0.000 (0.000–0.000) |
| delete-middle-4k | 10 | candidate | 7.438 (7.203–7.578) | 3.832 (3.699–4.168) | 2.496 (2.164–2.516) | 0.574 (0.000–0.695) | 1.375 (1.328–1.422) | 0.000 (0.000–0.000) |
| delete-middle-4k | 100 | baseline | 8.031 (7.953–8.250) | 3.883 (3.594–4.809) | 2.539 (2.391–2.691) | 0.488 (0.348–0.660) | 1.797 (1.750–1.891) | 0.000 (0.000–0.000) |
| delete-middle-4k | 100 | candidate | 8.109 (7.953–8.188) | 3.980 (3.801–5.973) | 2.328 (2.238–2.652) | 0.066 (0.000–0.746) | 1.797 (1.781–1.844) | 0.000 (0.000–0.000) |
| delete-middle-4k | 500 | baseline | 9.672 (9.609–9.953) | 3.977 (3.863–4.410) | 2.609 (2.371–2.852) | 0.496 (0.367–0.805) | 3.000 (2.938–3.047) | 0.000 (0.000–0.000) |
| delete-middle-4k | 500 | candidate | 9.750 (9.578–9.984) | 4.086 (3.867–4.250) | 2.480 (2.277–2.617) | 0.211 (0.000–0.504) | 3.031 (2.875–3.078) | 0.000 (0.000–0.000) |
| append-tail-4k | 1 | baseline | 7.109 (6.969–7.156) | 3.801 (3.617–4.164) | 2.453 (2.301–2.637) | 0.523 (0.281–0.871) | 1.062 (1.031–1.094) | 0.000 (0.000–0.000) |
| append-tail-4k | 1 | candidate | 6.953 (6.844–7.266) | 4.156 (3.859–4.750) | 2.211 (2.062–2.426) | 0.293 (0.055–0.496) | 0.969 (0.938–1.047) | 0.000 (0.000–0.000) |
| append-tail-4k | 10 | baseline | 7.141 (6.938–7.234) | 3.922 (3.754–4.406) | 2.457 (2.363–2.562) | 0.305 (0.148–0.504) | 1.078 (1.062–1.125) | 0.000 (0.000–0.000) |
| append-tail-4k | 10 | candidate | 7.172 (6.938–7.266) | 4.117 (3.930–4.309) | 2.496 (2.301–2.566) | 0.199 (0.000–0.355) | 1.078 (1.031–1.141) | 0.000 (0.000–0.000) |
| append-tail-4k | 100 | baseline | 7.484 (7.391–7.609) | 4.164 (3.645–4.297) | 2.660 (2.332–2.730) | 0.578 (0.367–0.680) | 1.281 (1.203–1.328) | 0.000 (0.000–0.000) |
| append-tail-4k | 100 | candidate | 7.516 (7.516–7.688) | 3.867 (3.617–4.527) | 2.238 (2.023–2.527) | 0.238 (0.000–0.316) | 1.328 (1.141–1.344) | 0.000 (0.000–0.000) |
| append-tail-4k | 500 | baseline | 8.953 (8.938–9.109) | 4.051 (3.902–6.789) | 2.586 (2.406–2.719) | 0.566 (0.414–0.719) | 2.281 (2.234–2.328) | 0.000 (0.000–0.000) |
| append-tail-4k | 500 | candidate | 9.047 (8.891–9.234) | 4.008 (3.859–6.188) | 2.438 (2.324–2.633) | 0.320 (0.074–0.559) | 2.312 (2.234–2.344) | 0.000 (0.000–0.000) |
| prepend-head-4k | 1 | baseline | 6.875 (6.844–7.250) | 4.211 (3.891–4.973) | 2.652 (2.469–2.867) | 0.676 (0.180–0.723) | 1.062 (1.016–1.078) | 0.000 (0.000–0.000) |
| prepend-head-4k | 1 | candidate | 7.078 (6.844–7.234) | 4.176 (3.699–4.551) | 2.266 (2.133–2.680) | 0.137 (0.062–0.492) | 1.000 (0.953–1.062) | 0.000 (0.000–0.000) |
| prepend-head-4k | 10 | baseline | 7.188 (7.047–7.344) | 4.375 (4.176–4.949) | 2.605 (2.387–2.691) | 0.367 (0.320–0.617) | 1.188 (1.172–1.219) | 0.000 (0.000–0.000) |
| prepend-head-4k | 10 | candidate | 7.094 (7.031–7.281) | 3.992 (3.863–4.191) | 2.426 (2.184–2.547) | 0.070 (0.000–0.395) | 1.219 (1.141–1.328) | 0.000 (0.000–0.000) |
| prepend-head-4k | 100 | baseline | 7.703 (7.688–7.969) | 4.406 (3.930–4.805) | 2.508 (2.492–2.746) | 0.383 (0.219–0.707) | 1.516 (1.484–1.594) | 0.000 (0.000–0.000) |
| prepend-head-4k | 100 | candidate | 7.719 (7.609–7.984) | 3.879 (3.691–4.914) | 2.375 (2.066–2.559) | 0.074 (0.000–0.414) | 1.500 (1.406–1.578) | 0.000 (0.000–0.000) |
| prepend-head-4k | 500 | baseline | 9.438 (9.297–9.516) | 4.250 (3.785–4.309) | 2.738 (2.375–2.902) | 0.703 (0.309–1.082) | 2.688 (2.656–2.859) | 0.000 (0.000–0.000) |
| prepend-head-4k | 500 | candidate | 9.406 (9.234–9.531) | 4.355 (3.750–4.504) | 2.168 (2.125–2.750) | 0.297 (0.137–0.508) | 2.625 (2.578–2.797) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 1 | baseline | 7.188 (7.000–7.219) | 3.969 (3.734–4.180) | 2.457 (2.363–2.820) | 0.438 (0.309–0.816) | 1.078 (1.047–1.172) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 1 | candidate | 7.031 (6.859–7.172) | 4.176 (3.863–4.426) | 2.480 (2.125–2.637) | 0.453 (0.039–0.555) | 1.078 (1.031–1.172) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 10 | baseline | 7.297 (7.281–7.484) | 4.215 (3.891–4.488) | 2.602 (2.418–2.688) | 0.484 (0.199–0.660) | 1.422 (1.422–1.453) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 10 | candidate | 7.422 (7.219–7.516) | 4.160 (3.523–4.578) | 2.383 (2.191–2.539) | 0.207 (0.109–0.348) | 1.359 (1.281–1.406) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 100 | baseline | 8.047 (8.000–8.250) | 4.035 (3.809–4.242) | 2.734 (2.355–3.004) | 0.461 (0.191–0.719) | 1.828 (1.750–1.875) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 100 | candidate | 8.125 (7.984–8.172) | 3.887 (3.746–4.492) | 2.426 (2.270–2.547) | 0.172 (0.000–0.512) | 1.797 (1.750–1.844) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 500 | baseline | 9.781 (9.703–9.984) | 4.289 (3.906–4.512) | 2.602 (2.449–2.770) | 0.496 (0.180–0.805) | 3.109 (3.078–3.141) | 0.000 (0.000–0.000) |
| replace-grow-middle-2k-to-4k | 500 | candidate | 9.766 (9.703–10.000) | 4.199 (3.980–4.824) | 2.324 (2.223–2.477) | 0.359 (0.035–0.570) | 3.062 (2.906–3.141) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 1 | baseline | 7.047 (6.844–7.172) | 3.867 (3.617–4.133) | 2.516 (2.355–2.656) | 0.434 (0.336–0.504) | 1.047 (1.047–1.078) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 1 | candidate | 7.109 (6.938–7.141) | 4.031 (3.652–4.387) | 2.402 (2.160–2.512) | 0.211 (0.055–0.348) | 1.062 (1.031–1.094) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 10 | baseline | 7.234 (7.156–7.453) | 4.121 (3.855–4.492) | 2.570 (2.402–2.750) | 0.496 (0.121–0.832) | 1.406 (1.297–1.469) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 10 | candidate | 7.219 (7.172–7.312) | 4.133 (3.871–4.273) | 2.273 (2.070–2.477) | 0.129 (0.000–0.309) | 1.328 (1.281–1.375) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 100 | baseline | 7.984 (7.906–8.250) | 4.191 (3.688–4.699) | 2.605 (2.531–2.684) | 0.633 (0.422–0.668) | 1.781 (1.734–1.781) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 100 | candidate | 8.031 (7.922–8.172) | 4.156 (3.863–5.758) | 2.422 (2.254–2.504) | 0.176 (0.000–0.250) | 1.719 (1.641–1.766) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 500 | baseline | 9.922 (9.797–10.109) | 4.211 (3.695–4.414) | 2.504 (2.457–2.836) | 0.480 (0.211–0.641) | 3.234 (3.188–3.344) | 0.000 (0.000–0.000) |
| replace-shrink-middle-4k-to-2k | 500 | candidate | 10.125 (9.750–10.141) | 3.914 (3.859–4.359) | 2.457 (2.242–2.598) | 0.496 (0.000–0.504) | 3.203 (3.109–3.281) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 1 | baseline | 6.875 (6.828–7.016) | 4.164 (3.879–4.281) | 2.555 (2.383–2.664) | 0.395 (0.078–0.629) | 1.000 (0.969–1.031) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 1 | candidate | 6.922 (6.859–7.219) | 3.867 (3.484–4.461) | 2.336 (2.246–2.633) | 0.359 (0.145–0.492) | 1.047 (0.953–1.047) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 10 | baseline | 6.984 (6.906–7.281) | 4.223 (4.008–4.625) | 2.664 (2.441–2.719) | 0.438 (0.367–0.668) | 1.125 (1.062–1.156) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 10 | candidate | 6.953 (6.875–7.312) | 4.137 (3.727–4.320) | 2.246 (2.199–2.465) | 0.164 (0.047–0.285) | 1.109 (1.031–1.141) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 100 | baseline | 7.531 (7.500–7.719) | 3.996 (3.609–6.246) | 2.480 (2.398–2.863) | 0.309 (0.273–0.586) | 1.312 (1.281–1.375) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 100 | candidate | 7.516 (7.391–7.578) | 3.910 (3.832–4.574) | 2.340 (2.133–2.438) | 0.121 (0.070–0.336) | 1.328 (1.219–1.422) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 500 | baseline | 9.297 (9.219–9.453) | 3.977 (3.871–4.703) | 2.633 (2.422–2.660) | 0.500 (0.352–0.723) | 2.641 (2.578–2.688) | 0.000 (0.000–0.000) |
| truncate-tail-4k | 500 | candidate | 9.312 (9.281–9.500) | 4.008 (3.625–4.324) | 2.320 (2.238–2.469) | 0.297 (0.098–0.605) | 2.625 (2.594–2.719) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 1 | baseline | 7.047 (6.781–7.125) | 4.273 (4.168–4.387) | 2.539 (2.309–2.723) | 0.496 (0.148–0.656) | 1.047 (0.984–1.078) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 1 | candidate | 6.953 (6.859–7.125) | 3.988 (3.730–4.586) | 2.504 (2.250–2.613) | 0.250 (0.000–0.539) | 1.031 (0.969–1.047) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 10 | baseline | 6.938 (6.891–7.188) | 3.832 (3.512–4.043) | 2.445 (2.363–2.676) | 0.281 (0.223–0.430) | 1.062 (1.031–1.141) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 10 | candidate | 7.203 (6.969–7.219) | 3.906 (3.613–4.156) | 2.438 (2.367–2.633) | 0.324 (0.148–0.492) | 1.094 (1.000–1.156) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 100 | baseline | 7.609 (7.453–7.641) | 3.691 (3.516–6.156) | 2.582 (2.473–2.801) | 0.504 (0.359–0.660) | 1.297 (1.266–1.328) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 100 | candidate | 7.453 (7.422–7.844) | 4.211 (3.648–4.418) | 2.430 (1.922–2.598) | 0.258 (0.000–0.406) | 1.281 (1.234–1.391) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 500 | baseline | 9.125 (9.078–9.422) | 4.156 (3.734–4.820) | 2.754 (2.367–2.840) | 0.562 (0.238–0.844) | 2.484 (2.469–2.500) | 0.000 (0.000–0.000) |
| zero-extend-tail-4k | 500 | candidate | 9.172 (9.141–9.391) | 3.969 (3.867–4.297) | 2.430 (2.191–2.555) | 0.238 (0.141–0.547) | 2.500 (2.438–2.562) | 0.000 (0.000–0.000) |

Remaining findings:

- None under the explicitly recorded final policy.

## edit_canonical_chunk_count

Performance rows: 120; final classification: pass.

[Raw performance](../../edit-canonical-chunk-count/terminal-3337728e/performance/raw.jsonl) · [Verification subproofs](../../edit-canonical-chunk-count/terminal-3337728e/verification/subproofs.jsonl) · [Manifest](../../edit-canonical-chunk-count/terminal-3337728e/evidence.sha256)

| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms | Latency status |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| overwrite-fixed-64k-chunk-count-preserve | 1 | baseline | 5 | 19.342 (14.085–25.075) | 6.790 (2.655–9.487) | 25.611 (18.791–34.562) | directional comparator |
| overwrite-fixed-64k-chunk-count-preserve | 1 | candidate | 5 | 2.782 (1.234–7.541) | 3.932 (2.302–8.028) | 6.715 (3.537–15.569) | nominal-pass |
| overwrite-fixed-64k-chunk-count-preserve | 10 | baseline | 5 | 20.896 (17.393–27.611) | 5.256 (4.087–6.528) | 27.424 (22.026–32.867) | directional comparator |
| overwrite-fixed-64k-chunk-count-preserve | 10 | candidate | 5 | 2.832 (1.564–3.197) | 4.144 (3.032–5.043) | 6.198 (5.239–7.875) | nominal-pass |
| overwrite-fixed-64k-chunk-count-preserve | 100 | baseline | 5 | 17.809 (14.293–22.567) | 5.714 (3.959–5.925) | 23.261 (20.218–28.485) | directional comparator |
| overwrite-fixed-64k-chunk-count-preserve | 100 | candidate | 5 | 3.183 (1.419–7.251) | 5.116 (4.091–5.273) | 8.060 (5.675–12.367) | nominal-pass |
| overwrite-fixed-64k-chunk-count-preserve | 500 | baseline | 5 | 14.229 (12.745–20.011) | 12.742 (8.275–17.585) | 26.592 (22.505–37.596) | directional comparator |
| overwrite-fixed-64k-chunk-count-preserve | 500 | candidate | 5 | 2.550 (1.931–5.408) | 12.023 (8.338–15.876) | 14.573 (12.863–18.022) | accepted-with-tolerance |
| overwrite-fixed-64k-chunk-count-increase | 1 | baseline | 5 | 18.102 (17.036–22.758) | 3.850 (2.858–4.956) | 21.877 (20.742–27.714) | directional comparator |
| overwrite-fixed-64k-chunk-count-increase | 1 | candidate | 5 | 1.855 (1.343–2.983) | 3.056 (2.626–3.591) | 5.069 (3.968–6.469) | nominal-pass |
| overwrite-fixed-64k-chunk-count-increase | 10 | baseline | 5 | 24.376 (16.859–27.818) | 5.202 (3.606–5.835) | 30.137 (20.465–33.653) | directional comparator |
| overwrite-fixed-64k-chunk-count-increase | 10 | candidate | 5 | 2.718 (1.242–4.115) | 3.263 (3.142–6.043) | 6.272 (4.391–9.884) | nominal-pass |
| overwrite-fixed-64k-chunk-count-increase | 100 | baseline | 5 | 25.079 (14.188–31.967) | 5.543 (5.035–6.251) | 30.249 (19.731–37.002) | directional comparator |
| overwrite-fixed-64k-chunk-count-increase | 100 | candidate | 5 | 1.677 (1.458–2.361) | 4.850 (4.571–6.982) | 6.932 (6.053–8.660) | nominal-pass |
| overwrite-fixed-64k-chunk-count-increase | 500 | baseline | 5 | 16.093 (12.227–27.852) | 13.209 (9.415–21.814) | 30.062 (25.436–49.666) | directional comparator |
| overwrite-fixed-64k-chunk-count-increase | 500 | candidate | 5 | 2.471 (1.538–2.783) | 11.471 (7.930–15.396) | 13.008 (10.713–17.127) | accepted-with-tolerance |
| overwrite-fixed-64k-chunk-count-decrease | 1 | baseline | 5 | 19.558 (13.960–23.243) | 2.602 (2.356–4.657) | 22.160 (16.315–25.677) | directional comparator |
| overwrite-fixed-64k-chunk-count-decrease | 1 | candidate | 5 | 1.997 (1.367–3.190) | 3.084 (2.419–3.437) | 5.040 (4.436–5.609) | nominal-pass |
| overwrite-fixed-64k-chunk-count-decrease | 10 | baseline | 5 | 19.099 (13.678–23.248) | 3.694 (3.478–5.859) | 22.609 (17.157–27.722) | directional comparator |
| overwrite-fixed-64k-chunk-count-decrease | 10 | candidate | 5 | 1.623 (1.214–3.193) | 3.518 (2.760–3.826) | 5.141 (4.201–7.018) | nominal-pass |
| overwrite-fixed-64k-chunk-count-decrease | 100 | baseline | 5 | 16.584 (15.182–20.402) | 5.069 (4.556–6.124) | 21.948 (20.505–25.471) | directional comparator |
| overwrite-fixed-64k-chunk-count-decrease | 100 | candidate | 5 | 3.119 (2.002–5.162) | 7.430 (5.215–8.120) | 10.433 (7.216–12.592) | nominal-pass |
| overwrite-fixed-64k-chunk-count-decrease | 500 | baseline | 5 | 16.394 (14.647–24.146) | 11.545 (9.654–13.221) | 27.143 (26.048–37.367) | directional comparator |
| overwrite-fixed-64k-chunk-count-decrease | 500 | candidate | 5 | 2.319 (2.165–4.706) | 10.454 (8.398–12.847) | 13.438 (10.717–15.160) | accepted-with-tolerance |

Memory cells are median (min–max), MiB, N=5 performance workers per arm. Native peaks bound whole lifetimes; window/category observations are sampled, not exact-phase or continuous maxima.

| Operation | MiB | Arm | Native RSS lifetime peak | Native cgroup lifetime peak | Sampled cgroup window peak | Sampled cgroup window increment | Sampled RSS increment | Sampled dirty/writeback increment |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| overwrite-fixed-64k-chunk-count-preserve | 1 | baseline | 7.938 (7.797–7.953) | 4.277 (3.809–4.500) | 2.477 (2.328–2.664) | 0.281 (0.043–0.629) | 1.953 (1.922–2.141) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 1 | candidate | 8.047 (7.828–8.172) | 3.875 (3.480–4.230) | 2.449 (2.207–2.594) | 0.246 (0.074–0.340) | 1.922 (1.906–1.938) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 10 | baseline | 8.312 (8.203–8.484) | 4.059 (3.809–4.297) | 2.633 (2.449–2.785) | 0.605 (0.305–0.688) | 2.234 (2.188–2.359) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 10 | candidate | 8.281 (8.141–8.297) | 3.910 (3.480–4.191) | 2.473 (2.266–2.578) | 0.328 (0.023–0.551) | 2.312 (2.234–2.469) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 100 | baseline | 8.734 (8.594–8.828) | 3.871 (3.496–4.406) | 2.566 (2.410–2.754) | 0.379 (0.000–0.527) | 2.344 (2.328–2.469) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 100 | candidate | 8.719 (8.625–8.875) | 4.160 (3.926–4.238) | 2.473 (2.324–2.504) | 0.375 (0.121–0.543) | 2.422 (2.344–2.484) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 500 | baseline | 10.672 (10.547–10.734) | 4.391 (3.918–4.797) | 2.617 (2.426–2.664) | 0.402 (0.105–0.832) | 3.938 (3.906–4.000) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-preserve | 500 | candidate | 10.734 (10.516–10.844) | 4.176 (3.855–5.996) | 2.297 (2.270–2.676) | 0.156 (0.027–0.629) | 3.984 (3.922–4.125) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 1 | baseline | 8.062 (7.844–8.297) | 3.992 (3.609–4.250) | 2.500 (2.324–2.848) | 0.371 (0.285–0.641) | 2.062 (1.906–2.188) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 1 | candidate | 8.094 (7.875–8.156) | 3.922 (3.617–4.160) | 2.402 (2.230–2.527) | 0.250 (0.152–0.602) | 1.984 (1.891–2.125) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 10 | baseline | 8.375 (8.234–8.516) | 4.145 (3.816–4.633) | 2.602 (2.500–2.621) | 0.680 (0.352–0.855) | 2.328 (2.281–2.375) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 10 | candidate | 8.328 (8.266–8.391) | 3.977 (3.855–4.504) | 2.359 (2.234–2.543) | 0.250 (0.000–0.523) | 2.438 (2.297–2.516) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 100 | baseline | 8.812 (8.625–8.875) | 3.906 (3.871–4.277) | 2.590 (2.344–2.668) | 0.727 (0.000–0.816) | 2.453 (2.359–2.578) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 100 | candidate | 8.844 (8.750–9.047) | 4.168 (3.727–4.504) | 2.320 (2.215–2.539) | 0.160 (0.000–0.445) | 2.578 (2.422–2.656) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 500 | baseline | 10.844 (10.656–10.969) | 4.156 (3.695–6.152) | 2.602 (2.430–2.758) | 0.516 (0.129–0.746) | 4.109 (4.016–4.219) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-increase | 500 | candidate | 10.859 (10.781–10.922) | 3.941 (3.691–4.066) | 2.496 (2.242–2.617) | 0.320 (0.215–0.387) | 3.922 (3.891–4.125) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 1 | baseline | 7.656 (7.547–7.859) | 3.922 (3.855–4.469) | 2.441 (2.348–2.586) | 0.352 (0.246–0.590) | 1.719 (1.594–1.766) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 1 | candidate | 7.547 (7.500–7.656) | 3.945 (3.609–4.043) | 2.383 (2.359–2.461) | 0.246 (0.133–0.367) | 1.641 (1.484–1.781) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 10 | baseline | 7.969 (7.953–8.156) | 3.867 (3.730–4.230) | 2.703 (2.379–2.742) | 0.496 (0.332–0.816) | 2.016 (1.906–2.078) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 10 | candidate | 7.891 (7.891–8.156) | 4.082 (3.840–4.844) | 2.227 (2.152–2.512) | 0.324 (0.098–0.496) | 2.016 (1.891–2.062) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 100 | baseline | 8.547 (8.469–8.781) | 3.793 (3.625–4.449) | 2.562 (2.438–2.645) | 0.461 (0.082–0.793) | 2.375 (2.312–2.438) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 100 | candidate | 8.656 (8.484–8.797) | 4.129 (3.633–4.293) | 2.230 (2.191–2.559) | 0.270 (0.070–0.344) | 2.312 (2.234–2.484) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 500 | baseline | 10.594 (10.469–10.641) | 4.168 (3.930–4.656) | 2.680 (2.609–2.809) | 0.508 (0.348–0.719) | 3.953 (3.906–4.031) | 0.000 (0.000–0.000) |
| overwrite-fixed-64k-chunk-count-decrease | 500 | candidate | 10.641 (10.375–10.812) | 4.152 (3.594–4.840) | 2.293 (1.988–2.535) | 0.152 (0.000–0.277) | 3.953 (3.766–4.078) | 0.000 (0.000–0.000) |

Remaining findings:

- None under the explicitly recorded final policy.

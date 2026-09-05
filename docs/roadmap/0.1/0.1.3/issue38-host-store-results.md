# Issue 38 — host-owned SQLite qualification and topology comparison

Status: **host follow-up PASS** for the four prescribed construction families. All 114 unique host samples and all 30 adjacent-tier normalized median ratios pass the user’s strict **<1.25** scaling gate. All samples complete the prescribed work within 15 seconds, with passing cleanup and observed resource checks. The frozen Docker-only campaign remains separately preserved and qualified for its original topology. This report makes no claim of proportional scaling or a 25% tolerance for topology regressions.

## Scope and provenance
- source_identity: `f1eb7c6f3dd1ebd31f639e888953be523c842ad21c8d77d45b1d7c37cef42181`
- product_identity: `20aee20d96b09e0cd1d934b751a1e0370523987adfabc220aa63f283bb7a7d9e`
- image: `sha256:f66ce7083572d8c81d43948f5308cc4b96080f548e5a796b57af8d3b40c2c988`
- image_source_identity: `eb9b79cd87e6d655e26cad37b69327c70379e831b4d61fb4dd49838795d6fd25`
- Host binary SHA-256: `ab9ecba30a3cb792e9e21259b981ad61eed9b5d01f1fb0ba58fa1e311997d45a`; Rust 1.85.1, macOS arm64; source commit `0dc2565f0d3f2ff6271fc743bf4a904b7267b58f` plus the recorded benchmark-only working changes.
- Host system SQLite:3.51.0; `otool -L` confirms the coordinator links `/usr/lib/libsqlite3.dylib`. The product crate bytes match the frozen optimized Linux image. Source identity includes benchmark wiring; producing cache identity and executing binary identity remain separate in every receipt.
- Host SDK/coordinator, Workspace manager/capture/spool, Store and embedded local SQLite; Docker Desktop Linux daemon, workload and real FUSE. Existing `ContainerManager`/`ContainerBinding`, `Client::connect_with_container` and ProxyHost/authenticated transport are reused. No new product transport, HTTP service or product optimization was introduced.
- Actual per-sample inspection confirms no Docker data mounts/binds/volumes/socket, only `/dev/fuse` and SYS_ADMIN, bridge network and one daemon port published only on127.0.0.1. Linux image architecture is arm64; host architecture is arm64. No system caches, swap settings or Docker/Linux memory settings were tuned.
- Linux workload/daemon/FUSE containers remain capped at2 CPUs/2 GiB/no swap. The14-logical-CPU host coordinator is not constrained by that cgroup; native construction uses the existing scheduler with up to8 workers. Host process RSS stays below2 GiB in these samples. There is no aggregate2-CPU or aggregate2-GiB claim. Host page cache, Docker VM/global memory and system-wide CPU are not completely measured.
- Measurements are matched in optimized product bytes, cases, seeds, actual work and product timer, with an explicitly different topology/resource scope. Faster host native import can use more workers. Lower container memory largely moves Store/spool/file-cache ownership to the host and is not a pure memory-efficiency improvement.

## Preparation and safety
Workspace uses compatible, protected, closed and quiescent masters under the ignored project-local `benchmark-results/host-store/prepared/`. Every sample receives an independent writable byte copy before product timing; fsync, SQLite quick_check, matching hashes and distinct inodes are checked. WAL/SHM/journal sidecars cause refusal. No live Store is copied. Post-sample validation confirms masters unchanged. Native cases reuse source fixtures under `fixtures/` with original modes, and initialize absent output Stores. Mutable sample data under `samples/` is removed; compact evidence is retained in `results/`.

Compatibility uses actual initial fixture descriptors/profile, seed, v5 SQL DDL and the versioned canonical/fixture contract, with native family/case qualification. Unrelated source, image, harness and report changes do not force database initialization. Producer provenance remains attached to cached state; it is not rewritten as the current executor. Byte-generation or canonical-format changes outside descriptors require a compatibility-contract bump. The cache is bounded to8 entries/10 GiB with ownership-checked eviction. Prepared data is evictable, samples disposable, and git ignore is not backup; durable backups need separate storage/retention. Ordinary product Store paths remain caller-selected and outside benchmark cleanup.

Actual miss→hit, two independent writable samples, unchanged masters and source reuse are retained in the smoke receipts. Twenty focused shared checks passed, including consistency/isolation, live WAL rejection, fixture-plan invalidation, unrelated executor/image reuse, protected eviction, unowned cleanup refusal, and loopback/mount validation. The existing SDK live proof passed normal Commit, injected post-attach failure and disconnect cleanup. The first host verifier failed `NotReady` because harness Client teardown dropped the daemon owner; the corrected harness retains one binding across Store close/reopen. That initial FAIL remains visible in the failure table below.

## All individual samples and distributions
Timer: `pure_call_sum_ns`; native equals its sole initialize phase. Workspace includes Create, Exec, full-status Commit, visibility and End. Preparation, Store copying, container startup and separate verification are excluded. Repeated phase instances are summed before aggregation; overlapping internal checkpoints are never added into another total. Work is completed Workspace writes or actual native source bytes scanned.

| Case | Work bytes | Seed1 s | Seed2 s | Seed3 s | Median s | Mean s | Min s | Max s | MiB/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| payload-create-1m-compact-v2 | 1048576 | 0.025027875 | 0.024693665 | 0.025669625 | 0.025027875 | 0.025130388 | 0.024693665 | 0.025669625 | 39.955 |
| payload-create-10m-compact-v2 | 10485760 | 0.062607749 | 0.057788541 | 0.062064000 | 0.062064000 | 0.060820097 | 0.057788541 | 0.062607749 | 161.124 |
| payload-create-100m | 104857600 | 0.529123250 | 0.408488916 | 0.396174500 | 0.408488916 | 0.444595555 | 0.396174500 | 0.529123250 | 244.805 |
| payload-create-500m | 524288000 | 2.402669499 | 1.881036958 | 1.899561791 | 1.899561791 | 2.061089416 | 1.881036958 | 2.402669499 | 263.219 |
| dedup-cross-file-anchor-1 | 1048576 | 0.004671792 | 0.003829709 | 0.004263250 | 0.004263250 | 0.004254917 | 0.003829709 | 0.004671792 | 234.563 |
| dedup-cross-file-unique-10 | 10485760 | 0.022828708 | 0.022332958 | 0.022747708 | 0.022747708 | 0.022636458 | 0.022332958 | 0.022828708 | 439.605 |
| dedup-cross-file-unique-100 | 104857600 | 0.098148750 | 0.093801500 | 0.095276917 | 0.095276917 | 0.095742389 | 0.093801500 | 0.098148750 | 1049.572 |
| dedup-cross-file-unique-500 | 524288000 | 0.452314208 | 0.488961083 | 0.446352417 | 0.452314208 | 0.462542569 | 0.446352417 | 0.488961083 | 1105.426 |
| dedup-cross-file-identical-10 | 10485760 | 0.018363958 | 0.018145083 | 0.019082000 | 0.018363958 | 0.018530347 | 0.018145083 | 0.019082000 | 544.545 |
| dedup-cross-file-identical-100 | 104857600 | 0.030205709 | 0.030316708 | 0.029878000 | 0.030205709 | 0.030133472 | 0.029878000 | 0.030316708 | 3310.632 |
| dedup-cross-file-identical-500 | 524288000 | 0.107891500 | 0.107734417 | 0.108330917 | 0.107891500 | 0.107985611 | 0.107734417 | 0.108330917 | 4634.285 |
| dedup-cross-file-mixed-10 | 10485760 | 0.023141333 | 0.024174542 | 0.023856416 | 0.023856416 | 0.023724097 | 0.023141333 | 0.024174542 | 419.174 |
| dedup-cross-file-mixed-100 | 104857600 | 0.116676333 | 0.112735500 | 0.115391125 | 0.115391125 | 0.114934319 | 0.112735500 | 0.116676333 | 866.618 |
| dedup-cross-file-mixed-500 | 524288000 | 0.536459750 | 0.553384000 | 0.544910292 | 0.544910292 | 0.544918014 | 0.536459750 | 0.553384000 | 917.582 |
| dedup-cdc-overwrite-1 | 2097152 | 0.004060917 | 0.004324042 | 0.004098791 | 0.004098791 | 0.004161250 | 0.004060917 | 0.004324042 | 487.949 |
| dedup-cdc-overwrite-10 | 11534336 | 0.018678250 | 0.018764333 | 0.018548208 | 0.018678250 | 0.018663597 | 0.018548208 | 0.018764333 | 588.920 |
| dedup-cdc-overwrite-100 | 105906176 | 0.033183459 | 0.032378208 | 0.032318541 | 0.032378208 | 0.032626736 | 0.032318541 | 0.033183459 | 3119.382 |
| dedup-cdc-overwrite-500 | 525336576 | 0.123970000 | 0.122797792 | 0.123218500 | 0.123218500 | 0.123328764 | 0.122797792 | 0.123970000 | 4065.948 |
| dedup-cdc-insert-1 | 2101248 | 0.004123625 | 0.004260458 | 0.004345875 | 0.004260458 | 0.004243319 | 0.004123625 | 0.004345875 | 470.350 |
| dedup-cdc-insert-10 | 11575296 | 0.018393458 | 0.019338625 | 0.019226250 | 0.019226250 | 0.018986111 | 0.018393458 | 0.019338625 | 574.166 |
| dedup-cdc-insert-100 | 106315776 | 0.035708625 | 0.034826500 | 0.034841416 | 0.034841416 | 0.035125514 | 0.034826500 | 0.035708625 | 2910.060 |
| dedup-cdc-insert-500 | 527384576 | 0.153700750 | 0.134772125 | 0.134277208 | 0.134772125 | 0.140916694 | 0.134277208 | 0.153700750 | 3731.878 |
| dedup-cdc-delete-1 | 2093056 | 0.004046750 | 0.004079375 | 0.004003417 | 0.004046750 | 0.004043181 | 0.004003417 | 0.004079375 | 493.258 |
| dedup-cdc-delete-10 | 11493376 | 0.019286208 | 0.019020583 | 0.018947791 | 0.019020583 | 0.019084861 | 0.018947791 | 0.019286208 | 576.267 |
| dedup-cdc-delete-100 | 105496576 | 0.034486125 | 0.033338416 | 0.032912917 | 0.033338416 | 0.033579153 | 0.032912917 | 0.034486125 | 3017.821 |
| dedup-cdc-delete-500 | 523288576 | 0.132063958 | 0.126776875 | 0.131976542 | 0.131976542 | 0.130272458 | 0.126776875 | 0.132063958 | 3781.330 |
| dedup-cdc-common-body-1 | 2097152 | 0.004420958 | 0.004261917 | 0.004629417 | 0.004420958 | 0.004437431 | 0.004261917 | 0.004629417 | 452.391 |
| dedup-cdc-common-body-10 | 11534336 | 0.021174500 | 0.021269666 | 0.021296333 | 0.021269666 | 0.021246833 | 0.021174500 | 0.021296333 | 517.168 |
| dedup-cdc-common-body-100 | 105906176 | 0.060739041 | 0.058585041 | 0.060719208 | 0.060719208 | 0.060014430 | 0.058585041 | 0.060739041 | 1663.395 |
| dedup-cdc-common-body-500 | 525336576 | 0.247651584 | 0.243209792 | 0.234852958 | 0.243209792 | 0.241904778 | 0.234852958 | 0.247651584 | 2059.950 |
| dedup-cdc-scattered-1 | 2097152 | 0.004809583 | 0.005062542 | 0.004848750 | 0.004848750 | 0.004906958 | 0.004809583 | 0.005062542 | 412.477 |
| dedup-cdc-scattered-10 | 11534336 | 0.024949292 | 0.023127958 | 0.025459167 | 0.024949292 | 0.024512139 | 0.023127958 | 0.025459167 | 440.894 |
| dedup-cdc-scattered-100 | 105906176 | 0.100217250 | 0.099687208 | 0.097014042 | 0.099687208 | 0.098972833 | 0.097014042 | 0.100217250 | 1013.169 |
| dedup-cdc-scattered-500 | 525336576 | 0.437729708 | 0.457377208 | 0.434046083 | 0.437729708 | 0.443051000 | 0.434046083 | 0.457377208 | 1144.542 |
| dedup-workspace-unique-100 | 104857600 | 0.748318958 | 0.741998125 | 0.761773416 | 0.748318958 | 0.750696833 | 0.741998125 | 0.761773416 | 133.633 |
| dedup-workspace-unique-500 | 524288000 | 3.875961457 | 3.776175416 | 3.869114334 | 3.869114334 | 3.840417069 | 3.776175416 | 3.875961457 | 129.229 |
| dedup-workspace-unique-1-base128-v3 | 1048576 | 0.028680375 | 0.028497334 | 0.027837291 | 0.028497334 | 0.028338333 | 0.027837291 | 0.028680375 | 35.091 |
| dedup-workspace-unique-10-base128-v3 | 10485760 | 0.100320542 | 0.098560499 | 0.103581042 | 0.100320542 | 0.100820694 | 0.098560499 | 0.103581042 | 99.680 |

## All scaling comparisons
| Curve | Tiers | Work growth | Median normalized | Mean normalized | Seed1 / Seed2 / Seed3 normalized | <1.25 |
|---|---|---:|---:|---:|---|---|
| payload-create | 1→10 | 10.000000000 | 0.247979503 | 0.242018133 | 0.250152 / 0.234022 / 0.241780 | PASS |
| payload-create | 10→100 | 10.000000000 | 0.658173685 | 0.731001067 | 0.845140 / 0.706868 / 0.638332 | PASS |
| payload-create | 100→500 | 5.000000000 | 0.930043248 | 0.927174998 | 0.908170 / 0.920973 / 0.958952 | PASS |
| dedup-cross-file-identical | 1→10 | 10.000000000 | 0.430750202 | 0.435504312 | 0.393082 / 0.473798 / 0.447593 | PASS |
| dedup-cross-file-identical | 10→100 | 10.000000000 | 0.164483653 | 0.162616881 | 0.164484 / 0.167079 / 0.156577 | PASS |
| dedup-cross-file-identical | 100→500 | 5.000000000 | 0.714378199 | 0.716715353 | 0.714378 / 0.710726 / 0.725155 | PASS |
| dedup-cross-file-mixed | 1→10 | 10.000000000 | 0.559582853 | 0.557568973 | 0.495342 / 0.631237 / 0.559583 | PASS |
| dedup-cross-file-mixed | 10→100 | 10.000000000 | 0.483690111 | 0.484462356 | 0.504190 / 0.466340 / 0.483690 | PASS |
| dedup-cross-file-mixed | 100→500 | 5.000000000 | 0.944457890 | 0.948225068 | 0.919569 / 0.981739 / 0.944458 | PASS |
| dedup-cross-file-unique | 1→10 | 10.000000000 | 0.533576684 | 0.532007040 | 0.488650 / 0.583150 / 0.533577 | PASS |
| dedup-cross-file-unique | 10→100 | 10.000000000 | 0.418841832 | 0.422956582 | 0.429936 / 0.420014 / 0.418842 | PASS |
| dedup-cross-file-unique | 100→500 | 5.000000000 | 0.949472805 | 0.966223162 | 0.921691 / 1.042544 / 0.936958 | PASS |
| dedup-cdc-common-body | 1→10 | 5.500000000 | 0.874745248 | 0.870562457 | 0.870831 / 0.907388 / 0.836403 | PASS |
| dedup-cdc-common-body | 10→100 | 9.181818182 | 0.310911467 | 0.307632909 | 0.312411 / 0.299984 / 0.310522 | PASS |
| dedup-cdc-common-body | 100→500 | 4.960396040 | 0.807492702 | 0.812591750 | 0.821972 / 0.836908 / 0.779747 | PASS |
| dedup-cdc-delete | 1→10 | 5.491193738 | 0.855954506 | 0.859605289 | 0.867908 / 0.849109 / 0.861908 | PASS |
| dedup-cdc-delete | 10→100 | 9.178902352 | 0.190954715 | 0.191685821 | 0.194808 / 0.190955 / 0.189242 | PASS |
| dedup-cdc-delete | 100→500 | 4.960242274 | 0.798084533 | 0.782131877 | 0.772035 / 0.766641 / 0.808402 | PASS |
| dedup-cdc-insert | 1→10 | 5.508771930 | 0.819187854 | 0.812223418 | 0.809710 / 0.823976 / 0.803087 | PASS |
| dedup-cdc-insert | 10→100 | 9.184713376 | 0.197303870 | 0.201428566 | 0.211370 / 0.196073 / 0.197304 | PASS |
| dedup-cdc-insert | 100→500 | 4.960548621 | 0.779784446 | 0.808742101 | 0.867707 / 0.780118 / 0.776921 | PASS |
| dedup-cdc-overwrite | 1→10 | 5.500000000 | 0.828548090 | 0.815471619 | 0.836276 / 0.789006 / 0.822780 | PASS |
| dedup-cdc-overwrite | 10→100 | 9.181818182 | 0.188793890 | 0.190392393 | 0.193489 / 0.187928 / 0.189767 | PASS |
| dedup-cdc-overwrite | 100→500 | 4.960396040 | 0.767196754 | 0.762034086 | 0.753145 / 0.764577 / 0.768613 | PASS |
| dedup-cdc-scattered | 1→10 | 5.500000000 | 0.935547287 | 0.908251557 | 0.943166 / 0.830627 / 0.954667 | PASS |
| dedup-cdc-scattered | 10→100 | 9.181818182 | 0.435163557 | 0.439750259 | 0.437477 / 0.469433 / 0.415013 | PASS |
| dedup-cdc-scattered | 100→500 | 4.960396040 | 0.885218003 | 0.902446294 | 0.880536 / 0.924951 / 0.901955 | PASS |
| dedup-workspace-unique | 1→10 | 10.000000000 | 0.352034832 | 0.355774961 | 0.349788 / 0.345859 / 0.372095 | PASS |
| dedup-workspace-unique | 10→100 | 10.000000000 | 0.745927946 | 0.744586057 | 0.745928 / 0.752835 / 0.735437 | PASS |
| dedup-workspace-unique | 100→500 | 5.000000000 | 1.034081602 | 1.023160589 | 1.035912 / 1.017840 / 1.015818 | PASS |

The gate uses normalized adjacent-tier medians, not every corresponding seed. Individual variability is retained. CDC actual work includes its fixed reference and insertion/deletion length changes; Workspace unique low tiers retain the128-file base. Compact smoke backgrounds do not substitute for those scaling tiers.

## Frozen Docker vs host: same product and work
| Case | Docker median s | Host median s | Host/Docker time | Docker MiB/s | Host MiB/s |
|---|---:|---:|---:|---:|---:|
| payload-create-1m-compact-v2 | 0.015796376 | 0.025027875 | 1.584406 | 63.306 | 39.955 |
| payload-create-10m-compact-v2 | 0.054880708 | 0.062064000 | 1.130889 | 182.213 | 161.124 |
| payload-create-100m | 0.428373834 | 0.408488916 | 0.953580 | 233.441 | 244.805 |
| payload-create-500m | 2.368766125 | 1.899561791 | 0.801920 | 211.080 | 263.219 |
| dedup-cross-file-anchor-1 | 0.004265958 | 0.004263250 | 0.999365 | 234.414 | 234.563 |
| dedup-cross-file-unique-10 | 0.026099250 | 0.022747708 | 0.871585 | 383.153 | 439.605 |
| dedup-cross-file-unique-100 | 0.161730500 | 0.095276917 | 0.589109 | 618.313 | 1049.572 |
| dedup-cross-file-unique-500 | 0.778405167 | 0.452314208 | 0.581078 | 642.339 | 1105.426 |
| dedup-cross-file-identical-10 | 0.020164375 | 0.018363958 | 0.910713 | 495.924 | 544.545 |
| dedup-cross-file-identical-100 | 0.092081209 | 0.030205709 | 0.328033 | 1085.998 | 3310.632 |
| dedup-cross-file-identical-500 | 0.479259916 | 0.107891500 | 0.225121 | 1043.275 | 4634.285 |
| dedup-cross-file-mixed-10 | 0.026917083 | 0.023856416 | 0.886293 | 371.511 | 419.174 |
| dedup-cross-file-mixed-100 | 0.154316417 | 0.115391125 | 0.747757 | 648.019 | 866.618 |
| dedup-cross-file-mixed-500 | 0.892975875 | 0.544910292 | 0.610218 | 559.926 | 917.582 |
| dedup-cdc-overwrite-1 | 0.003939375 | 0.004098791 | 1.040467 | 507.695 | 487.949 |
| dedup-cdc-overwrite-10 | 0.020071250 | 0.018678250 | 0.930597 | 548.048 | 588.920 |
| dedup-cdc-overwrite-100 | 0.096835166 | 0.032378208 | 0.334364 | 1043.010 | 3119.382 |
| dedup-cdc-overwrite-500 | 0.488210250 | 0.123218500 | 0.252388 | 1026.197 | 4065.948 |
| dedup-cdc-insert-1 | 0.004345250 | 0.004260458 | 0.980486 | 461.172 | 470.350 |
| dedup-cdc-insert-10 | 0.021310917 | 0.019226250 | 0.902178 | 518.000 | 574.166 |
| dedup-cdc-insert-100 | 0.099640042 | 0.034841416 | 0.349673 | 1017.569 | 2910.060 |
| dedup-cdc-insert-500 | 0.512471958 | 0.134772125 | 0.262984 | 981.426 | 3731.878 |
| dedup-cdc-delete-1 | 0.004234583 | 0.004046750 | 0.955643 | 471.379 | 493.258 |
| dedup-cdc-delete-10 | 0.020706333 | 0.019020583 | 0.918588 | 529.352 | 576.267 |
| dedup-cdc-delete-100 | 0.095872334 | 0.033338416 | 0.347738 | 1049.410 | 3017.821 |
| dedup-cdc-delete-500 | 0.485700708 | 0.131976542 | 0.271724 | 1027.478 | 3781.330 |
| dedup-cdc-common-body-1 | 0.004362959 | 0.004420958 | 1.013294 | 458.404 | 452.391 |
| dedup-cdc-common-body-10 | 0.023337375 | 0.021269666 | 0.911399 | 471.347 | 517.168 |
| dedup-cdc-common-body-100 | 0.123581792 | 0.060719208 | 0.491328 | 817.272 | 1663.395 |
| dedup-cdc-common-body-500 | 0.595288625 | 0.243209792 | 0.408558 | 841.609 | 2059.950 |
| dedup-cdc-scattered-1 | 0.006169500 | 0.004848750 | 0.785923 | 324.175 | 412.477 |
| dedup-cdc-scattered-10 | 0.026922958 | 0.024949292 | 0.926692 | 408.573 | 440.894 |
| dedup-cdc-scattered-100 | 0.152760500 | 0.099687208 | 0.652572 | 661.166 | 1013.169 |
| dedup-cdc-scattered-500 | 0.829035084 | 0.437729708 | 0.527999 | 604.317 | 1144.542 |
| dedup-workspace-unique-100 | 0.700712791 | 0.748318958 | 1.067940 | 142.712 | 133.633 |
| dedup-workspace-unique-500 | 3.680507918 | 3.869114334 | 1.051245 | 135.851 | 129.229 |
| dedup-workspace-unique-1-base128-v3 | 0.016188501 | 0.028497334 | 1.760344 | 61.772 | 35.091 |
| dedup-workspace-unique-10-base128-v3 | 0.083100749 | 0.100320542 | 1.207216 | 120.336 | 99.680 |

The frozen500 MiB payload reference is2.368766125s/~211.08 MiB/s. Host median is1.899561791s/~263.22 MiB/s. No numerical topology regression allowance was invented. Low-tier absolute overhead and native worker differences are visible in the table. These topology deltas cannot be attributed solely to product efficiency.

## Host phases and separate resources
| Case | Create ms | Exec ms | Commit ms | Visibility ms | End ms | Init ms | Host process CPU median s | Container command CPU median s | Host RSS max MiB | Container peak max MiB | Host read/write median MiB | Native workers |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| payload-create-1m-compact-v2 | 9.034583 | 9.323375 | 4.168375 | 0.072500 | 2.510917 | — | 0.030276 | 0.021212 | 12.828 | 7.598 | 0.000 / 1.012 | — |
| payload-create-10m-compact-v2 | 9.103416 | 26.731333 | 22.912375 | 0.075417 | 2.619583 | — | 0.070434 | 0.029909 | 43.000 | 7.344 | 0.023 / 23.320 | — |
| payload-create-100m | 10.029208 | 192.506625 | 204.085917 | 0.077334 | 4.848667 | — | 0.428976 | 0.092460 | 77.172 | 10.516 | 0.121 / 203.789 | — |
| payload-create-500m | 8.266458 | 881.768833 | 1009.072083 | 0.076084 | 7.589833 | — | 2.052330 | 0.355290 | 88.750 | 10.430 | 0.242 / 1005.891 | — |
| dedup-cross-file-anchor-1 | — | — | — | — | — | 4.263250 | 0.024922 | 0.012769 | 10.781 | 5.926 | 0.000 / 0.004 | 1 |
| dedup-cross-file-unique-10 | — | — | — | — | — | 22.747708 | 0.047351 | 0.012886 | 30.172 | 5.859 | 0.000 / 4.004 | 1 |
| dedup-cross-file-unique-100 | — | — | — | — | — | 95.276917 | 0.265215 | 0.013260 | 59.141 | 5.910 | 0.000 / 4.027 | 7 |
| dedup-cross-file-unique-500 | — | — | — | — | — | 452.314208 | 1.237232 | 0.014515 | 62.109 | 5.664 | 0.004 / 4.031 | 8 |
| dedup-cross-file-identical-10 | — | — | — | — | — | 18.363958 | 0.040650 | 0.011971 | 11.406 | 5.426 | 0.000 / 0.004 | 1 |
| dedup-cross-file-identical-100 | — | — | — | — | — | 30.205709 | 0.190170 | 0.013062 | 14.891 | 5.648 | 0.000 / 0.027 | 7 |
| dedup-cross-file-identical-500 | — | — | — | — | — | 107.891500 | 0.856952 | 0.013133 | 16.359 | 4.883 | 0.000 / 0.031 | 8 |
| dedup-cross-file-mixed-10 | — | — | — | — | — | 23.856416 | 0.047996 | 0.012906 | 26.922 | 5.641 | 0.000 / 3.004 | 1 |
| dedup-cross-file-mixed-100 | — | — | — | — | — | 115.391125 | 0.284273 | 0.013083 | 62.922 | 5.422 | 0.012 / 3.027 | 7 |
| dedup-cross-file-mixed-500 | — | — | — | — | — | 544.910292 | 1.332158 | 0.015498 | 66.297 | 5.906 | 0.008 / 4.031 | 8 |
| dedup-cdc-overwrite-1 | — | — | — | — | — | 4.098791 | 0.026167 | 0.013023 | 11.406 | 5.645 | 0.000 / 0.008 | 2 |
| dedup-cdc-overwrite-10 | — | — | — | — | — | 18.678250 | 0.042926 | 0.012613 | 12.203 | 5.395 | 0.000 / 0.008 | 2 |
| dedup-cdc-overwrite-100 | — | — | — | — | — | 32.378208 | 0.198115 | 0.012827 | 20.453 | 5.410 | 0.000 / 0.031 | 8 |
| dedup-cdc-overwrite-500 | — | — | — | — | — | 123.218500 | 0.878532 | 0.014199 | 40.156 | 5.152 | 0.016 / 3.031 | 8 |
| dedup-cdc-insert-1 | — | — | — | — | — | 4.260458 | 0.026913 | 0.012359 | 11.281 | 5.391 | 0.000 / 0.008 | 2 |
| dedup-cdc-insert-10 | — | — | — | — | — | 19.226250 | 0.041524 | 0.012450 | 12.922 | 5.684 | 0.000 / 0.008 | 2 |
| dedup-cdc-insert-100 | — | — | — | — | — | 34.841416 | 0.201462 | 0.012910 | 26.359 | 5.684 | 0.004 / 0.031 | 8 |
| dedup-cdc-insert-500 | — | — | — | — | — | 134.772125 | 0.889264 | 0.012876 | 46.750 | 7.422 | 0.008 / 2.035 | 8 |
| dedup-cdc-delete-1 | — | — | — | — | — | 4.046750 | 0.026548 | 0.012500 | 11.625 | 5.180 | 0.000 / 0.008 | 2 |
| dedup-cdc-delete-10 | — | — | — | — | — | 19.020583 | 0.043050 | 0.012356 | 13.000 | 5.645 | 0.000 / 0.008 | 2 |
| dedup-cdc-delete-100 | — | — | — | — | — | 33.338416 | 0.201180 | 0.012828 | 25.062 | 7.914 | 0.000 / 0.031 | 8 |
| dedup-cdc-delete-500 | — | — | — | — | — | 131.976542 | 0.879295 | 0.013247 | 43.375 | 5.656 | 0.008 / 2.035 | 8 |
| dedup-cdc-common-body-1 | — | — | — | — | — | 4.420958 | 0.024849 | 0.012614 | 11.625 | 5.148 | 0.000 / 0.008 | 2 |
| dedup-cdc-common-body-10 | — | — | — | — | — | 21.269666 | 0.044730 | 0.012564 | 18.906 | 4.645 | 0.000 / 0.008 | 2 |
| dedup-cdc-common-body-100 | — | — | — | — | — | 60.719208 | 0.231886 | 0.012991 | 59.641 | 5.691 | 0.004 / 3.031 | 8 |
| dedup-cdc-common-body-500 | — | — | — | — | — | 243.209792 | 0.996366 | 0.015128 | 61.891 | 5.078 | 0.008 / 3.031 | 8 |
| dedup-cdc-scattered-1 | — | — | — | — | — | 4.848750 | 0.028306 | 0.012674 | 13.609 | 5.148 | 0.000 / 0.008 | 2 |
| dedup-cdc-scattered-10 | — | — | — | — | — | 24.949292 | 0.049690 | 0.012900 | 31.641 | 5.656 | 0.012 / 4.008 | 2 |
| dedup-cdc-scattered-100 | — | — | — | — | — | 99.687208 | 0.274363 | 0.013564 | 58.141 | 5.906 | 0.000 / 4.031 | 8 |
| dedup-cdc-scattered-500 | — | — | — | — | — | 437.729708 | 1.223891 | 0.014900 | 60.531 | 5.406 | 0.000 / 4.035 | 8 |
| dedup-workspace-unique-100 | 10.015500 | 313.127875 | 424.538833 | 0.075250 | 4.415750 | — | 0.543589 | 0.147791 | 70.391 | 7.914 | 0.043 / 200.590 | — |
| dedup-workspace-unique-500 | 8.882583 | 1575.821875 | 2217.179208 | 0.080625 | 4.690250 | — | 2.586666 | 0.630902 | 81.641 | 7.977 | 0.676 / 1003.168 | — |
| dedup-workspace-unique-1-base128-v3 | 9.445125 | 11.158125 | 4.752375 | 0.067542 | 3.268167 | — | 0.062111 | 0.022064 | 35.188 | 7.582 | 0.000 / 1.008 | — |
| dedup-workspace-unique-10-base128-v3 | 9.852500 | 39.747333 | 47.068583 | 0.067917 | 2.732625 | — | 0.112351 | 0.034184 | 63.672 | 7.660 | 0.012 / 20.078 | — |

Host CPU/I/O is process before→after-product (native before→after), including observation/orchestration within that window. Host RSS is a process peak, supplemented by10ms sampled RSS in receipts. Container CPU covers the command window and container peak covers lifetime including startup; neither is an operation-only memory peak. Darwin I/O counters and Linux cgroup/process counters have different accounting. Host page cache is missing, not zero. Workspace construction worker count is not separately instrumented; the native count is an actual diagnostic, not host CPU count.

## Transport, copies and admission (Workspace)
| Case | Write requests median | Payload bytes median | Client frame bytes median | Socket write median ms | Socket read median ms | Client copy bytes max | Frame/decode copy bytes max | Admission median ms | Admission owned bytes max | Spill readback bytes max | Borrowed admission copy bytes max |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| payload-create-1m-compact-v2 | 2.000000 | 1048576.000000 | 1048601.000000 | 0.174333 | 2.449209 | 1048576 | 0 | 1.922 | 1053277 | 0 | 0 |
| payload-create-10m-compact-v2 | 20.000000 | 10485760.000000 | 10486010.000000 | 1.600207 | 5.639751 | 10485760 | 0 | 20.241 | 0 | 10520811 | 0 |
| payload-create-100m | 200.000000 | 104857600.000000 | 104860100.000000 | 100.543543 | 17.150372 | 104857600 | 0 | 194.490 | 0 | 105191997 | 0 |
| payload-create-500m | 1000.000000 | 524288000.000000 | 524300500.000000 | 558.114994 | 66.960128 | 524288000 | 0 | 977.394 | 0 | 525955698 | 0 |
| dedup-workspace-unique-100 | 200.000000 | 104857600.000000 | 104860100.000000 | 14.994082 | 153.181421 | 104857600 | 0 | 185.638 | 0 | 105236234 | 0 |
| dedup-workspace-unique-500 | 1000.000000 | 524288000.000000 | 524300500.000000 | 73.947166 | 761.298970 | 524288000 | 0 | 982.408 | 0 | 526140615 | 0 |
| dedup-workspace-unique-1-base128-v3 | 2.000000 | 1048576.000000 | 1048601.000000 | 0.204458 | 2.467541 | 1048576 | 0 | 2.276 | 1057668 | 0 | 0 |
| dedup-workspace-unique-10-base128-v3 | 20.000000 | 10485760.000000 | 10486010.000000 | 1.502416 | 16.764495 | 10485760 | 0 | 20.875 | 0 | 10532568 | 0 |

These transport and admission values are parsed from existing explicit debug-text receipts. Socket durations include waiting and overlap dispatch/Exec; they must not be added to Commit or the complete product timer. Zero borrowed admission copies describes that particular boundary, not total copies or total I/O. Full native source reads, worker counts, slab waits/copies, SQL counters and Store footprint remain in every compact native receipt.

## Separate bounded verification, including retained failed attempt
| Case / run | Seed | Status | Wall s | Canonical current bytes/files | FUSE current bytes/files | Cleanup |
|---|---:|---|---:|---|---|---|
| dedup-cdc-insert-500 / dedup-cdc-insert-500-s3-verify | 3 | PASS | 17.583759959 | 527384576 / 501 | 527384576 / 501 | PASS |
| dedup-cross-file-mixed-500 / dedup-cross-file-mixed-500-s3-verify | 3 | PASS | 13.656885417 | 524288000 / 500 | 524288000 / 500 | PASS |
| dedup-workspace-unique-500 / dedup-workspace-unique-500-s3-verify | 3 | PASS | 13.998164958 | 658505728 / 628 | 658505728 / 628 | PASS |
| dedup-cross-file-anchor-1 / dedup_cross_file-smoke-2-verify | 1 | PASS | 1.745926542 | 1048576 / 1 | 1048576 / 1 | PASS |
| payload-create-500m / payload-create-500m-s3-verify | 3 | PASS | 7.224484500 | 524288000 / 1 | 524288000 / 1 | PASS |
| payload-create-1m-compact-v2 / payload-smoke-verify-1 | 1 | FAIL | 4.625872458 | — / — | — / — | PASS |
| payload-create-1m-compact-v2 / payload_create_read-smoke-2-verify | 1 | PASS | 1.638602584 | 1048576 / 1 | 1048576 / 1 | PASS |
| payload-create-1m-compact-v2 / payload_create_read-smoke-final-verify | 1 | PASS | 1.692918209 | 1048576 / 1 | 1048576 / 1 | PASS |

Four final selected verifiers total52.463294833s, each below59s and aggregate below600s;45s work allowance remains unchanged. Payload additionally verifies same-session continuation after two full-status Commits, returned/published heads, bytes/mode, fresh projection and clean End. The representative proofs cover every current byte/path/metadata/alias on their selected cases, with independent applicable CAS/CDC transcripts. They explicitly omit exhaustive typed-object/storage census (`fully_verified=false`, `full_canonical_census_performed=false`). They do not claim every seed/case was independently reverified or qualify #39's separate campaign.

## Final checks and cleanup
- All114 host sample work/environment/setup/resource/cleanup checks passed. Maximum complete product time:3.875961457s. Host and container swap/OOM observations show no reported sample violation.
- Host release coordinator built with Cargo1.85.1/j2. Shared focused tests, formatting and benchmark Clippy results are recorded in the progress log. Frozen product all-feature Linux native checks remain289 PASS/one preexisting ignored; the warm suite96s and strict Clippy remain applicable to the unchanged product crates.
- Benchmark-owned sample containers and sample data are removed; bounded prepared caches and compact receipts remain. Final inventory is recorded in the progress log. Normal product Store files are outside cleanup. No code was pushed, merged or released.

## Evidence
- [Frozen Docker assessment](issue38-candidate3-docker-results.md).
- `payload-create-1m-compact-v2`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-1m-compact-v2-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-1m-compact-v2-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-1m-compact-v2-s3/perf.jsonl)
- `payload-create-10m-compact-v2`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-10m-compact-v2-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-10m-compact-v2-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-10m-compact-v2-s3/perf.jsonl)
- `payload-create-100m`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-100m-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-100m-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-100m-s3/perf.jsonl)
- `payload-create-500m`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-500m-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-500m-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/payload-create-500m-s3/perf.jsonl)
- `dedup-cross-file-anchor-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-anchor-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-anchor-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-anchor-1-s3/perf.jsonl)
- `dedup-cross-file-unique-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-10-s3/perf.jsonl)
- `dedup-cross-file-unique-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-100-s3/perf.jsonl)
- `dedup-cross-file-unique-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-unique-500-s3/perf.jsonl)
- `dedup-cross-file-identical-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-10-s3/perf.jsonl)
- `dedup-cross-file-identical-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-100-s3/perf.jsonl)
- `dedup-cross-file-identical-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-identical-500-s3/perf.jsonl)
- `dedup-cross-file-mixed-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-10-s3/perf.jsonl)
- `dedup-cross-file-mixed-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-100-s3/perf.jsonl)
- `dedup-cross-file-mixed-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cross-file-mixed-500-s3/perf.jsonl)
- `dedup-cdc-overwrite-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-1-s3/perf.jsonl)
- `dedup-cdc-overwrite-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-10-s3/perf.jsonl)
- `dedup-cdc-overwrite-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-100-s3/perf.jsonl)
- `dedup-cdc-overwrite-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-overwrite-500-s3/perf.jsonl)
- `dedup-cdc-insert-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-1-s3/perf.jsonl)
- `dedup-cdc-insert-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-10-s3/perf.jsonl)
- `dedup-cdc-insert-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-100-s3/perf.jsonl)
- `dedup-cdc-insert-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-insert-500-s3/perf.jsonl)
- `dedup-cdc-delete-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-1-s3/perf.jsonl)
- `dedup-cdc-delete-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-10-s3/perf.jsonl)
- `dedup-cdc-delete-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-100-s3/perf.jsonl)
- `dedup-cdc-delete-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-delete-500-s3/perf.jsonl)
- `dedup-cdc-common-body-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-1-s3/perf.jsonl)
- `dedup-cdc-common-body-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-10-s3/perf.jsonl)
- `dedup-cdc-common-body-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-100-s3/perf.jsonl)
- `dedup-cdc-common-body-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-common-body-500-s3/perf.jsonl)
- `dedup-cdc-scattered-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-1-s3/perf.jsonl)
- `dedup-cdc-scattered-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-10-s3/perf.jsonl)
- `dedup-cdc-scattered-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-100-s3/perf.jsonl)
- `dedup-cdc-scattered-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-cdc-scattered-500-s3/perf.jsonl)
- `dedup-workspace-unique-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-100-s3/perf.jsonl)
- `dedup-workspace-unique-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-500-s3/perf.jsonl)
- `dedup-workspace-unique-1-base128-v3`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-1-base128-v3-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-1-base128-v3-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-1-base128-v3-s3/perf.jsonl)
- `dedup-workspace-unique-10-base128-v3`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-10-base128-v3-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-10-base128-v3-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/host-store/results/dedup-workspace-unique-10-base128-v3-s3/perf.jsonl)


## Delivered build and final inventory

The qualified114-sample host executor is retained under `target/issue38-context/host-qualified-executor/`. Final delivered source `69cf6ec212bed950f89b25ec547e6713f33e4dadc1fc19c68e9f70c534b5fa5f` / binary `c232aa143366c9870951326bf62091933505923f110b9ca2973f7905b940d573` follows a lint-only collapse of the verifier exchange condition and final harness identity/output checks. Product code and performance operations are unchanged. A final small payload performance+bounded verifier passed, including continuation; schema provenance is now sealed at build time. An actual cache hit reused the previously produced seed3 fixed-base Store under the new executor, preserving producer provenance and the master hash. These are explicitly recorded domain-compatible changes, not relabeling of old receipts.

Final formatting,20 shared checks and strict benchmark Clippy passed. Host v5 migration/staging/publication failure-injection check also passed. Cleanup inventory confirms zero owned containers, empty host sample directory,8 host cache entries/509,078,386 bytes and8 Docker cache entries. Detailed check logs and the initial Clippy diagnostic remain under `target/issue38-context/`. The final small verifier is additional changed-harness smoke coverage; the four representative500-tier verifiers remain the bounded final family evidence above.

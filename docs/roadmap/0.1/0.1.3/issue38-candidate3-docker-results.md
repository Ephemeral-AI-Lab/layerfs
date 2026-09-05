# Issue 38 — frozen Candidate 3 Docker-owned assessment

Status: complete candidate collection; terminal numerical gates PASS.

The user amended the primary gate to normalized adjacent-tier median growth **strictly <1.25**. This describes bounded scaling overhead. Original <=1.00 outcomes remain separately visible. No samples are discarded and no individual-seed tolerance is invented.

All 114 unique samples use seeds 1–3: payload 12, cross-file 30(shared tier 1 anchor counted once), CDC 60, Workspace unique reuse 12. Product timing is `pure_call_sum_ns`; for native imports it equals the sole `initialize` phase. Fixture preparation, copying, verification and outer cleanup are excluded. All phase repetitions are summed within a sample before aggregation; nested checkpoints are not added into another total.

## Identities and environment
- source_identity: `eb9b79cd87e6d655e26cad37b69327c70379e831b4d61fb4dd49838795d6fd25`
- product_identity: `20aee20d96b09e0cd1d934b751a1e0370523987adfabc220aa63f283bb7a7d9e`
- harness_identity: `d90ce9f76cec63a5d356d6d8f81adaed86be8381853e3dd7a3ab041d7fe0e70a`
- image: `sha256:f66ce7083572d8c81d43948f5308cc4b96080f548e5a796b57af8d3b40c2c988`
- Product base commit: `46de7d42918257cd9e86075833d4f7f45af62a67` plus the frozen working-tree changes on `codex/issue38-four-families`.
- macOS orchestration; Docker Desktop Linux/aarch64 product execution. Each sample:2 Docker CPUs,2 GiB memory/no swap,256 PID limit. Source-bound runtime guards inspect actual mounts/binds/image volumes/device/capabilities/ports before product execution: no Docker data sharing or socket, only `/dev/fuse` device. No system cache or Docker memory configuration was changed.
- Workspace operations use authenticated Exec and real FUSE, independent writable byte copies of closed prepared Stores, hash equality and distinct device/inode identity before timing. Native imports reuse the prepared source inside the container image and create absent output Stores. Native source-copy removal is an explicit setup/cache difference from earlier candidates, not a pure product gain.
- Container command-window CPU includes coordinator/daemon/workload work; container lifetime peak includes setup and charged file cache. The legacy `host-resources` record in this topology is Linux coordinator-process data, not macOS host resource use. Missing whole-system/cache measurements are not zeros.

## Every sample and distribution

| Case | Work bytes | Seed1 s | Seed2 s | Seed3 s | Median s | Mean s | Min s | Max s | Median MiB/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| payload-create-1m-compact-v2 | 1048576 | 0.015796376 | 0.012608416 | 0.016124625 | 0.015796376 | 0.014843139 | 0.012608416 | 0.016124625 | 63.306 |
| payload-create-10m-compact-v2 | 10485760 | 0.061246375 | 0.053728376 | 0.054880708 | 0.054880708 | 0.056618486 | 0.053728376 | 0.061246375 | 182.213 |
| payload-create-100m | 104857600 | 0.428077750 | 0.569551709 | 0.428373834 | 0.428373834 | 0.475334431 | 0.428077750 | 0.569551709 | 233.441 |
| payload-create-500m | 524288000 | 2.368766125 | 2.445822585 | 2.286476000 | 2.368766125 | 2.367021570 | 2.286476000 | 2.445822585 | 211.080 |
| dedup-cross-file-anchor-1 | 1048576 | 0.004060500 | 0.004375459 | 0.004265958 | 0.004265958 | 0.004233972 | 0.004060500 | 0.004375459 | 234.414 |
| dedup-cross-file-unique-10 | 10485760 | 0.025802333 | 0.027600667 | 0.026099250 | 0.026099250 | 0.026500750 | 0.025802333 | 0.027600667 | 383.153 |
| dedup-cross-file-unique-100 | 104857600 | 0.162747625 | 0.153633708 | 0.161730500 | 0.161730500 | 0.159370611 | 0.153633708 | 0.162747625 | 618.313 |
| dedup-cross-file-unique-500 | 524288000 | 0.849682667 | 0.778405167 | 0.767317584 | 0.778405167 | 0.798468473 | 0.767317584 | 0.849682667 | 642.339 |
| dedup-cross-file-identical-10 | 10485760 | 0.019390166 | 0.020197291 | 0.020164375 | 0.020164375 | 0.019917277 | 0.019390166 | 0.020197291 | 495.924 |
| dedup-cross-file-identical-100 | 104857600 | 0.092081209 | 0.091887250 | 0.097288334 | 0.092081209 | 0.093752264 | 0.091887250 | 0.097288334 | 1085.998 |
| dedup-cross-file-identical-500 | 524288000 | 0.446433376 | 0.479259916 | 0.511001751 | 0.479259916 | 0.478898348 | 0.446433376 | 0.511001751 | 1043.275 |
| dedup-cross-file-mixed-10 | 10485760 | 0.028593959 | 0.026653166 | 0.026917083 | 0.026917083 | 0.027388069 | 0.026653166 | 0.028593959 | 371.511 |
| dedup-cross-file-mixed-100 | 104857600 | 0.147327625 | 0.154316417 | 0.156865042 | 0.154316417 | 0.152836361 | 0.147327625 | 0.156865042 | 648.019 |
| dedup-cross-file-mixed-500 | 524288000 | 0.812059375 | 0.892975875 | 0.953535292 | 0.892975875 | 0.886190181 | 0.812059375 | 0.953535292 | 559.926 |
| dedup-cdc-overwrite-1 | 2097152 | 0.003939375 | 0.004240208 | 0.003935875 | 0.003939375 | 0.004038486 | 0.003935875 | 0.004240208 | 507.695 |
| dedup-cdc-overwrite-10 | 11534336 | 0.020028375 | 0.022170125 | 0.020071250 | 0.020071250 | 0.020756583 | 0.020028375 | 0.022170125 | 548.048 |
| dedup-cdc-overwrite-100 | 105906176 | 0.096717667 | 0.100743708 | 0.096835166 | 0.096835166 | 0.098098847 | 0.096717667 | 0.100743708 | 1043.010 |
| dedup-cdc-overwrite-500 | 525336576 | 0.517705750 | 0.457729334 | 0.488210250 | 0.488210250 | 0.487881778 | 0.457729334 | 0.517705750 | 1026.197 |
| dedup-cdc-insert-1 | 2101248 | 0.004580917 | 0.004345250 | 0.004089666 | 0.004345250 | 0.004338611 | 0.004089666 | 0.004580917 | 461.172 |
| dedup-cdc-insert-10 | 11575296 | 0.020578417 | 0.021310917 | 0.021392875 | 0.021310917 | 0.021094070 | 0.020578417 | 0.021392875 | 518.000 |
| dedup-cdc-insert-100 | 106315776 | 0.100947042 | 0.099022041 | 0.099640042 | 0.099640042 | 0.099869708 | 0.099022041 | 0.100947042 | 1017.569 |
| dedup-cdc-insert-500 | 527384576 | 0.512471958 | 0.457216125 | 0.529681500 | 0.512471958 | 0.499789861 | 0.457216125 | 0.529681500 | 981.426 |
| dedup-cdc-delete-1 | 2093056 | 0.004234583 | 0.004237125 | 0.004006542 | 0.004234583 | 0.004159417 | 0.004006542 | 0.004237125 | 471.379 |
| dedup-cdc-delete-10 | 11493376 | 0.021767750 | 0.020460500 | 0.020706333 | 0.020706333 | 0.020978194 | 0.020460500 | 0.021767750 | 529.352 |
| dedup-cdc-delete-100 | 105496576 | 0.109335292 | 0.095872334 | 0.094564042 | 0.095872334 | 0.099923889 | 0.094564042 | 0.109335292 | 1049.410 |
| dedup-cdc-delete-500 | 523288576 | 0.480622208 | 0.485700708 | 0.499648834 | 0.485700708 | 0.488657250 | 0.480622208 | 0.499648834 | 1027.478 |
| dedup-cdc-common-body-1 | 2097152 | 0.004362959 | 0.004338834 | 0.004469291 | 0.004362959 | 0.004390361 | 0.004338834 | 0.004469291 | 458.404 |
| dedup-cdc-common-body-10 | 11534336 | 0.023337375 | 0.022882917 | 0.024249375 | 0.023337375 | 0.023489889 | 0.022882917 | 0.024249375 | 471.347 |
| dedup-cdc-common-body-100 | 105906176 | 0.123581792 | 0.122596625 | 0.126355708 | 0.123581792 | 0.124178042 | 0.122596625 | 0.126355708 | 817.272 |
| dedup-cdc-common-body-500 | 525336576 | 0.614044834 | 0.595288625 | 0.553380417 | 0.595288625 | 0.587571292 | 0.553380417 | 0.614044834 | 841.609 |
| dedup-cdc-scattered-1 | 2097152 | 0.006169500 | 0.006448417 | 0.005192459 | 0.006169500 | 0.005936792 | 0.005192459 | 0.006448417 | 324.175 |
| dedup-cdc-scattered-10 | 11534336 | 0.026922958 | 0.028712792 | 0.026211667 | 0.026922958 | 0.027282472 | 0.026211667 | 0.028712792 | 408.573 |
| dedup-cdc-scattered-100 | 105906176 | 0.153717042 | 0.150665750 | 0.152760500 | 0.152760500 | 0.152381097 | 0.150665750 | 0.153717042 | 661.166 |
| dedup-cdc-scattered-500 | 525336576 | 0.829035084 | 0.761852126 | 0.835653459 | 0.829035084 | 0.808846890 | 0.761852126 | 0.835653459 | 604.317 |
| dedup-workspace-unique-100 | 104857600 | 0.712819751 | 0.700712791 | 0.686322583 | 0.700712791 | 0.699951708 | 0.686322583 | 0.712819751 | 142.712 |
| dedup-workspace-unique-500 | 524288000 | 4.374328835 | 3.680507918 | 3.426634627 | 3.680507918 | 3.827157127 | 3.426634627 | 4.374328835 | 135.851 |
| dedup-workspace-unique-1-base128-v3 | 1048576 | 0.017604084 | 0.015267459 | 0.016188501 | 0.016188501 | 0.016353348 | 0.015267459 | 0.017604084 | 61.772 |
| dedup-workspace-unique-10-base128-v3 | 10485760 | 0.084817417 | 0.074726583 | 0.083100749 | 0.083100749 | 0.080881583 | 0.074726583 | 0.084817417 | 120.336 |

Throughput above is actual completed write bytes (Workspace) or scanned source bytes (native), divided by the named complete product timer. It is not fixture size divided by outer wall or operation-only CPU.

## All adjacent-tier comparisons

| Curve | Tiers | Actual work growth | Median time growth | Normalized median | Normalized mean | Seed1 / Seed2 / Seed3 normalized | Old <=1.00 | Prior <1.10 | Current <1.25 |
|---|---|---:|---:|---:|---:|---|---|---|---|
| payload-create | 1→10 | 10.000000000 | 3.474259412 | 0.347425941 | 0.381445504 | 0.387724 / 0.426131 / 0.340353 | PASS | PASS | PASS |
| payload-create | 10→100 | 10.000000000 | 7.805544965 | 0.780554496 | 0.839539277 | 0.698944 / 1.060058 / 0.780554 | PASS | PASS | PASS |
| payload-create | 100→500 | 5.000000000 | 5.529670435 | 1.105934087 | 0.995939455 | 1.106699 / 0.858859 / 1.067514 | MISS | MISS | PASS |
| dedup-cross-file-identical | 1→10 | 10.000000000 | 4.726810484 | 0.472681048 | 0.470415859 | 0.477531 / 0.461604 / 0.472681 | PASS | PASS | PASS |
| dedup-cross-file-identical | 10→100 | 10.000000000 | 4.566529287 | 0.456652929 | 0.470708234 | 0.474886 / 0.454948 / 0.482476 | PASS | PASS | PASS |
| dedup-cross-file-identical | 100→500 | 5.000000000 | 5.204752644 | 1.040950529 | 1.021625133 | 0.969651 / 1.043148 / 1.050489 | MISS | PASS | PASS |
| dedup-cross-file-mixed | 1→10 | 10.000000000 | 6.309739336 | 0.630973934 | 0.646864627 | 0.704198 / 0.609151 / 0.630974 | PASS | PASS | PASS |
| dedup-cross-file-mixed | 10→100 | 10.000000000 | 5.733028984 | 0.573302898 | 0.558039924 | 0.515240 / 0.578980 / 0.582771 | PASS | PASS | PASS |
| dedup-cross-file-mixed | 100→500 | 5.000000000 | 5.786655058 | 1.157331012 | 1.159658831 | 1.102386 / 1.157331 / 1.215740 | MISS | MISS | PASS |
| dedup-cross-file-unique | 1→10 | 10.000000000 | 6.118027885 | 0.611802788 | 0.625907491 | 0.635447 / 0.630806 / 0.611803 | PASS | PASS | PASS |
| dedup-cross-file-unique | 10→100 | 10.000000000 | 6.196748949 | 0.619674895 | 0.601381512 | 0.630748 / 0.556630 / 0.619675 | PASS | PASS | PASS |
| dedup-cross-file-unique | 100→500 | 5.000000000 | 4.812976940 | 0.962595388 | 1.002027247 | 1.044172 / 1.013326 / 0.948884 | PASS | PASS | PASS |
| dedup-cdc-common-body | 1→10 | 5.500000000 | 5.348978755 | 0.972541592 | 0.972787565 | 0.972542 / 0.958905 / 0.986505 | PASS | PASS | PASS |
| dedup-cdc-common-body | 10→100 | 9.181818182 | 5.295445268 | 0.576731663 | 0.575751600 | 0.576732 / 0.583497 / 0.567500 | PASS | PASS | PASS |
| dedup-cdc-common-body | 100→500 | 4.960396040 | 4.816960617 | 0.971083877 | 0.953892447 | 1.001681 / 0.978887 / 0.882902 | PASS | PASS | PASS |
| dedup-cdc-delete | 1→10 | 5.491193738 | 4.889816305 | 0.890483297 | 0.918478321 | 0.936130 / 0.879383 / 0.941167 | PASS | PASS | PASS |
| dedup-cdc-delete | 10→100 | 9.178902352 | 4.630097178 | 0.504428199 | 0.518932028 | 0.547213 / 0.510489 / 0.497545 | PASS | PASS | PASS |
| dedup-cdc-delete | 100→500 | 4.960242274 | 5.066119575 | 1.021345188 | 0.985898322 | 0.886218 / 1.021345 / 1.065212 | MISS | PASS | PASS |
| dedup-cdc-insert | 1→10 | 5.508771930 | 4.904416777 | 0.890292217 | 0.882581603 | 0.815464 / 0.890292 / 0.949569 | PASS | PASS | PASS |
| dedup-cdc-insert | 10→100 | 9.184713376 | 4.675539865 | 0.509056698 | 0.515475226 | 0.534092 / 0.505899 / 0.507106 | PASS | PASS | PASS |
| dedup-cdc-insert | 100→500 | 4.960548621 | 5.143233059 | 1.036827466 | 1.008843846 | 1.023403 / 0.930808 / 1.071646 | MISS | PASS | PASS |
| dedup-cdc-overwrite | 1→10 | 5.500000000 | 5.095034111 | 0.926369838 | 0.934489866 | 0.924391 / 0.950645 / 0.927194 | PASS | PASS | PASS |
| dedup-cdc-overwrite | 10→100 | 9.181818182 | 4.824570767 | 0.525448301 | 0.514729841 | 0.525934 / 0.494904 / 0.525448 | PASS | PASS | PASS |
| dedup-cdc-overwrite | 100→500 | 4.960396040 | 5.041662757 | 1.016383111 | 1.002615335 | 1.079098 / 0.915956 / 1.016383 | MISS | PASS | PASS |
| dedup-cdc-scattered | 1→10 | 5.500000000 | 4.363880055 | 0.793432737 | 0.835543761 | 0.793433 / 0.809580 / 0.917823 | PASS | PASS | PASS |
| dedup-cdc-scattered | 10→100 | 9.181818182 | 5.673986491 | 0.617958925 | 0.608301216 | 0.621828 / 0.571492 / 0.634728 | PASS | PASS | PASS |
| dedup-cdc-scattered | 100→500 | 4.960396040 | 5.427025206 | 1.094070950 | 1.070086465 | 1.087263 / 1.019389 / 1.102805 | MISS | PASS | PASS |
| dedup-workspace-unique | 1→10 | 10.000000000 | 5.133319570 | 0.513331957 | 0.494587304 | 0.481805 / 0.489450 / 0.513332 | PASS | PASS | PASS |
| dedup-workspace-unique | 10→100 | 10.000000000 | 8.432087550 | 0.843208755 | 0.865403077 | 0.840417 / 0.937702 / 0.825892 | PASS | PASS | PASS |
| dedup-workspace-unique | 100→500 | 5.000000000 | 5.252519956 | 1.050503991 | 1.093548907 | 1.227331 / 1.050504 / 0.998549 | MISS | PASS | PASS |

CDC denominators include its fixed 1 MiB reference and each variant’s actual length(+4096 for insert,-4096 for delete). Fixed-base Workspace low controls use128 preexisting 1 MiB files; compact low smoke backgrounds are excluded from this scaling curve. Payload creation remains empty genesis plus the identical N MiB prefix at every tier despite the low-tier version suffix.

## Phases and resource observations

| Case | Create ms | Exec ms | Commit ms | Visibility ms | End ms | Init ms | Command CPU median s | Container peak max MiB | Admission median ms | Admission copied bytes max |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| payload-create-1m-compact-v2 | 3.583 | 6.770 | 3.167 | 0.040 | 1.254 | — | 0.043471 | 14.434 | 2.152 | 0 |
| payload-create-10m-compact-v2 | 2.738 | 26.723 | 23.899 | 0.040 | 1.780 | — | 0.094428 | 66.254 | 22.130 | 0 |
| payload-create-100m | 3.239 | 201.063 | 228.901 | 0.052 | 1.666 | — | 0.574413 | 382.848 | 219.116 | 0 |
| payload-create-500m | 2.764 | 1011.666 | 1350.194 | 0.057 | 1.610 | — | 2.872974 | 1448.809 | 1311.080 | 0 |
| dedup-cross-file-anchor-1 | — | — | — | — | — | 4.266 | 0.032034 | 8.473 | — | — |
| dedup-cross-file-unique-10 | — | — | — | — | — | 26.099 | 0.059994 | 34.797 | — | — |
| dedup-cross-file-unique-100 | — | — | — | — | — | 161.731 | 0.298349 | 167.109 | — | — |
| dedup-cross-file-unique-500 | — | — | — | — | — | 778.405 | 1.357087 | 994.602 | — | — |
| dedup-cross-file-identical-10 | — | — | — | — | — | 20.164 | 0.048649 | 9.098 | — | — |
| dedup-cross-file-identical-100 | — | — | — | — | — | 92.081 | 0.211174 | 43.996 | — | — |
| dedup-cross-file-identical-500 | — | — | — | — | — | 479.260 | 0.982500 | 412.031 | — | — |
| dedup-cross-file-mixed-10 | — | — | — | — | — | 26.917 | 0.060034 | 28.895 | — | — |
| dedup-cross-file-mixed-100 | — | — | — | — | — | 154.316 | 0.291883 | 159.445 | — | — |
| dedup-cross-file-mixed-500 | — | — | — | — | — | 892.976 | 1.419946 | 854.703 | — | — |
| dedup-cdc-overwrite-1 | — | — | — | — | — | 3.939 | 0.033139 | 8.785 | — | — |
| dedup-cdc-overwrite-10 | — | — | — | — | — | 20.071 | 0.051433 | 9.906 | — | — |
| dedup-cdc-overwrite-100 | — | — | — | — | — | 96.835 | 0.223234 | 18.633 | — | — |
| dedup-cdc-overwrite-500 | — | — | — | — | — | 488.210 | 0.997093 | 510.879 | — | — |
| dedup-cdc-insert-1 | — | — | — | — | — | 4.345 | 0.033750 | 12.359 | — | — |
| dedup-cdc-insert-10 | — | — | — | — | — | 21.311 | 0.051399 | 11.875 | — | — |
| dedup-cdc-insert-100 | — | — | — | — | — | 99.640 | 0.223470 | 25.945 | — | — |
| dedup-cdc-insert-500 | — | — | — | — | — | 512.472 | 1.037647 | 440.277 | — | — |
| dedup-cdc-delete-1 | — | — | — | — | — | 4.235 | 0.034009 | 9.191 | — | — |
| dedup-cdc-delete-10 | — | — | — | — | — | 20.706 | 0.051897 | 10.820 | — | — |
| dedup-cdc-delete-100 | — | — | — | — | — | 95.872 | 0.221941 | 23.039 | — | — |
| dedup-cdc-delete-500 | — | — | — | — | — | 485.701 | 0.985229 | 312.918 | — | — |
| dedup-cdc-common-body-1 | — | — | — | — | — | 4.363 | 0.033795 | 9.840 | — | — |
| dedup-cdc-common-body-10 | — | — | — | — | — | 23.337 | 0.053438 | 18.359 | — | — |
| dedup-cdc-common-body-100 | — | — | — | — | — | 123.582 | 0.250891 | 79.906 | — | — |
| dedup-cdc-common-body-500 | — | — | — | — | — | 595.289 | 1.129885 | 614.703 | — | — |
| dedup-cdc-scattered-1 | — | — | — | — | — | 6.170 | 0.034337 | 12.238 | — | — |
| dedup-cdc-scattered-10 | — | — | — | — | — | 26.923 | 0.061985 | 38.410 | — | — |
| dedup-cdc-scattered-100 | — | — | — | — | — | 152.761 | 0.293044 | 168.363 | — | — |
| dedup-cdc-scattered-500 | — | — | — | — | — | 829.035 | 1.439042 | 1020.965 | — | — |
| dedup-workspace-unique-100 | 3.673 | 188.602 | 513.156 | 0.062 | 1.877 | — | 0.740249 | 508.938 | 274.803 | 0 |
| dedup-workspace-unique-500 | 3.440 | 882.959 | 2791.214 | 0.068 | 2.556 | — | 3.551078 | 1504.293 | 1510.622 | 0 |
| dedup-workspace-unique-1-base128-v3 | 4.121 | 6.893 | 3.743 | 0.062 | 1.396 | — | 0.075406 | 262.738 | 2.616 | 0 |
| dedup-workspace-unique-10-base128-v3 | 3.986 | 26.291 | 50.846 | 0.048 | 1.975 | — | 0.136740 | 231.875 | 25.144 | 0 |

Internal Commit/admission and native producer details remain explicitly debug-text diagnostics in the compact receipts. Owned admission copy count is only the candidate-to-admission borrowed-copy boundary; selected spill bytes exclude framing/read-ahead/physical I/O. It is not a claim of zero copying or zero spill in the whole product.

## Separate bounded verification

| Selection | Seed | Status | Wall s | Canonical current bytes / files | Native/FUSE current bytes / files | Cleanup |
|---|---:|---|---:|---|---|---|
| dedup-cross-file-anchor-1 | 1 | PASS | 2.404482417 | 1048576 / 1 | 1048576 / 1 | PASS |
| dedup-cross-file-mixed-500 | 3 | PASS | 9.729145708 | 524288000 / 500 | 524288000 / 500 | PASS |
| dedup-cdc-insert-500 | 3 | PASS | 10.618646792 | 527384576 / 501 | 527384576 / 501 | PASS |
| dedup-cdc-overwrite-1 | 1 | PASS | 2.310303000 | 2097152 / 2 | 2097152 / 2 | PASS |
| payload-create-500m | 1 | PASS | 8.309350958 | 524288000 / 1 | 524288000 / 1 | PASS |
| dedup-workspace-unique-100 | 1 | PASS | 7.300328208 | 239075328 / 228 | 239075328 / 228 | PASS |
| dedup-workspace-unique-500 | 3 | PASS | 16.128966208 | 658505728 / 628 | 658505728 / 628 | PASS |

These are `fast-verify`/`independent_current_content` selections with no reused covered paths: their receipts read every current regular byte and report zero skipped current bodies/metadata for the selected inputs, with full current namespace/inode/metadata/alias checks and applicable independent CAS/CDC transcripts. They explicitly omit exhaustive typed-object/storage census (`fully_verified=false`, `full_canonical_census_performed=false`). They do not certify unselected larger inputs or a later host-Store topology. Performance PASS is not inferred verification PASS.

## Resource and cleanup assessment
- All sample checks failed: `[]`.
- Maximum product sample: 4.374328835s; maximum sample-container lifetime peak: 1504.293MiB.
- All sample receipts and verification receipts retain actual source/input/image/setup identities and cleanup outcomes. The bounded prepared cache is retained; sample containers and writable sample Stores are removed by the existing owner cleanup. Final container inventory must be checked after the run.

## Evidence index
- `payload-create-1m-compact-v2`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-1m-compact-v2-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-1m-compact-v2-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-1m-compact-v2-s3/perf.jsonl)
- `payload-create-10m-compact-v2`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-10m-compact-v2-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-10m-compact-v2-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-10m-compact-v2-s3/perf.jsonl)
- `payload-create-100m`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-100m-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-100m-s3/perf.jsonl)
- `payload-create-500m`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-500m-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/payload-create-500m-s3/perf.jsonl)
- `dedup-cross-file-anchor-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-anchor-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-anchor-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-anchor-1-s3/perf.jsonl)
- `dedup-cross-file-unique-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-10-s3/perf.jsonl)
- `dedup-cross-file-unique-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-100-s3/perf.jsonl)
- `dedup-cross-file-unique-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-unique-500-s3/perf.jsonl)
- `dedup-cross-file-identical-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-10-s3/perf.jsonl)
- `dedup-cross-file-identical-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-100-s3/perf.jsonl)
- `dedup-cross-file-identical-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-identical-500-s3/perf.jsonl)
- `dedup-cross-file-mixed-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-10-s3/perf.jsonl)
- `dedup-cross-file-mixed-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-100-s3/perf.jsonl)
- `dedup-cross-file-mixed-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cross-file-mixed-500-s3/perf.jsonl)
- `dedup-cdc-overwrite-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-1-s3/perf.jsonl)
- `dedup-cdc-overwrite-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-10-s3/perf.jsonl)
- `dedup-cdc-overwrite-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-100-s3/perf.jsonl)
- `dedup-cdc-overwrite-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-overwrite-500-s3/perf.jsonl)
- `dedup-cdc-insert-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-1-s3/perf.jsonl)
- `dedup-cdc-insert-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-10-s3/perf.jsonl)
- `dedup-cdc-insert-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-100-s3/perf.jsonl)
- `dedup-cdc-insert-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-insert-500-s3/perf.jsonl)
- `dedup-cdc-delete-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-1-s3/perf.jsonl)
- `dedup-cdc-delete-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-10-s3/perf.jsonl)
- `dedup-cdc-delete-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-100-s3/perf.jsonl)
- `dedup-cdc-delete-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-delete-500-s3/perf.jsonl)
- `dedup-cdc-common-body-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-1-s3/perf.jsonl)
- `dedup-cdc-common-body-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-10-s3/perf.jsonl)
- `dedup-cdc-common-body-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-100-s3/perf.jsonl)
- `dedup-cdc-common-body-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-common-body-500-s3/perf.jsonl)
- `dedup-cdc-scattered-1`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-1-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-1-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-1-s3/perf.jsonl)
- `dedup-cdc-scattered-10`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-10-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-10-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-10-s3/perf.jsonl)
- `dedup-cdc-scattered-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-100-s3/perf.jsonl)
- `dedup-cdc-scattered-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-cdc-scattered-500-s3/perf.jsonl)
- `dedup-workspace-unique-100`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-100-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-100-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-100-s3/perf.jsonl)
- `dedup-workspace-unique-500`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-500-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-500-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-500-s3/perf.jsonl)
- `dedup-workspace-unique-1-base128-v3`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-1-base128-v3-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-1-base128-v3-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-1-base128-v3-s3/perf.jsonl)
- `dedup-workspace-unique-10-base128-v3`: [seed1](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-10-base128-v3-s1/perf.jsonl), [seed2](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-10-base128-v3-s2/perf.jsonl), [seed3](/Users/yifanxu/Ephemeral-AI-Lab/layerfs/target/issue38-evidence/candidate3/dedup-workspace-unique-10-base128-v3-s3/perf.jsonl)

## Historical before/after context

Historical values below are published Phase1 medians from the retained [issue38 inventory](https://github.com/Ephemeral-AI-Lab/layerfs/issues/38), primarily product7948df2d, with corrected CDC-delete500 seed3 at e24a3b34. They are **historical-approximate** comparisons: operation and actual work are similar, but host/split-owner/APFS-era preparation, CPU/cache placement and source revisions differ from this Docker-only campaign. Published baseline precision is six decimals. These deltas are observations, not attribution of the whole gain to the new product code. No new baseline campaign or old harness was run.

| Curve | Historical tiers | Historical medians s | Candidate3 medians s | Larger-tier delta | Classification |
|---|---|---|---|---:|---|
| payload-create | 100→500 | 0.475692→3.744340 | 0.428373834→2.368766125 | -36.74% | historical-approximate |
| dedup-cross-file-identical | 100→500 | 0.156959→0.808296 | 0.092081209→0.479259916 | -40.71% | historical-approximate |
| dedup-cross-file-mixed | 100→500 | 0.203634→1.056044 | 0.154316417→0.892975875 | -15.44% | historical-approximate |
| dedup-cross-file-unique | 100→500 | 0.176675→0.941368 | 0.161730500→0.778405167 | -17.31% | historical-approximate |
| dedup-cdc-common-body | 10→100 | 0.021993→0.268495 | 0.023337375→0.123581792 | -53.97% | historical-approximate |
| dedup-cdc-delete | 100→500 | 0.169896→0.910435 | 0.095872334→0.485700708 | -46.65% | historical-approximate |
| dedup-cdc-insert | 100→500 | 0.171250→0.915639 | 0.099640042→0.512471958 | -44.03% | historical-approximate |
| dedup-cdc-overwrite | 100→500 | 0.170187→0.895133 | 0.096835166→0.488210250 | -45.46% | historical-approximate |
| dedup-cdc-scattered | 1→10 | 0.005955→0.056221 | 0.006169500→0.026922958 | -52.11% | historical-approximate |
| dedup-cdc-scattered | 100→500 | 0.289743→1.681869 | 0.152760500→0.829035084 | -50.71% | historical-approximate |
| dedup-workspace-unique | 100→500 | 0.726023→3.847812 | 0.700712791→3.680507918 | -4.35% | historical-approximate |

Some historical small points are slower here(e.g.CDC common-body10 and scattered1); their changed platform/CPU/cache scope prevents a matched regression conclusion. No artificial delays or smaller-tier padding were added. The full candidate distributions and individual ratios above remain the numerical evidence, including earlier strict/<1.10 misses. Compact Workspace smoke backgrounds are not mixed with the fixed-base curve; no direct speedup is claimed for unavailable or incompatible historical low-tier records.

## Scoped child assessment under the final <1.25 gate

| Child | Samples | Comparisons | Maximum normalized median | Numerical outcome |
|---|---:|---:|---:|---|
| #41 | 12 | 3 | 1.105934087 | PASS under user amendment |
| #42 | 30 | 9 | 1.157331012 | PASS under user amendment |
| #43 | 60 | 15 | 1.094070950 | PASS under user amendment |
| #44 | 12 | 3 | 1.050503991 | PASS under user amendment |

Evidence cohort SHA256: `d5ffd1632d2d014a315808bf997299f7cf004e2bcf94ad5c69cd83474b7ef8aa`(SHA256 of sorted relative-path, tab, file-SHA256, newline records over Candidate3 compact receipt files). No owned sample container remains after collection. The bounded prepared cache contains8 entries/1,282,396,850 data bytes; it is reusable preparation, not retained writable samples.

The four final family representative verifiers are payload500/seed1, cross-file mixed500/seed3, CDC insert500/seed3 and Workspace unique500/seed3. Their total wall is44.786109666s, maximum16.128966208s, all below59s each and600s aggregate. Additional small receipts are initial changed-route checks, not extra final family invocations.

## Required regression checks

Independent canonical directory/preorder, capture invalidation/reuse, contiguous/sparse/overlapping/limit edits, selected owned spill/memory delivery, collision and v5 staged-publication tests passed. The full Linux all-feature native suite passed functionally with1 test executable at a time on2 CPUs, but its177s warm phase exceeded the unchanged120s test-fast gate; that failed scheduling attempt is retained in `target/issue38-context/final-checks-j1.log`. The bounded2-job scheduling check and Clippy are recorded separately when complete. Formatting passed before the test run. No old Phase1 benchmark/recovery campaign was restarted.

Final required checks PASS: Cargo1.96 formatting, full Linux all-feature native regression suite with2 bounded test jobs on2 CPUs, and Cargo1.96 Clippy with warnings denied. The unchanged120s warm-suite gate passed on the2-job execution; exact output is retained at `target/issue38-context/final-checks-j2.log`. The prior177s/1-job timing failure remains intact. Source remained identical to the frozen Candidate3 seal throughout.

**FROZEN_DOCKER_QUALIFICATION_PASS under the explicit user <1.25 amendment.** This completes the four-family Docker-owned campaign, its required candidate coverage and bounded correctness/resource/cleanup assessment. It does not qualify host-owned SQLite, #39, the missed #40 namespace target, an exhaustive object census, or a release. The separately authorized host-owned migration follows in this same task/worktree, preserving the optimized product and this baseline.

The final native check totals are289 passed,0 failed and1 preexisting ignored across36executables. The2-job warmphase was96 seconds; total check-image build/test/lint/export wall was135.879197708s. The temporary check image was removed after success.

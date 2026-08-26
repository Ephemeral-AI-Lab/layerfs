# Stage 2 Docker/Linux FUSE terminal summary

Status: `PASS_OPTIMIZED`

- Source: `9d4fc28cc9b6d3f63d1ea601e6f87ce1cef4a8ec` (tree `d79a178fd9224b6579d02e1de9135b645e443490`)
- Image: `sha256:4248624c903931181bb71f29e8b99f4c6c5a5bec470a706eab96d0f0ccac2071` (`linux/arm64`)
- Upstream `fs-bench.sh`: `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`
- Direct path: `layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store`
- `/var/tmp`: SL `3287045668` ns, Rsum `2.103137`, G `3.158427`, Spread `1.017835`
- `/tmp`: SL `3288120376` ns, Rsum `2.156910`, G `3.590222`, Spread `1.035161`
- Functional/restart/resource/cleanup: `PASS`
- Materialization/capture: `0 / 0`
- Stage 1.2: skipped

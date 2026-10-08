# Cluster-two Linux prerequisite proof

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner-authorized lifecycle extension2026-10-08: `session-lifecycle-proof` exercises
the additive public Session runner/monitor/outcome. Cases are ready, panic,
io-error, destroy-panic, invalid, partial-spawn and native. Partial-spawn requires
a dedicated container with pids.max2 (caller plus one receiver); native requires
the owned /dev/fuse and mount powers below. Each complete proof has an8s wall
stop, with2s internal observation bounds. Parser fixtures are not native evidence.
The [exact selection/results](../../docs/issues/307/checks/r2-fuser-lifecycle-20261008/18-proof-summary.json)
retain identities, outcomes and cleanup; this qualifies no LayerFS native route.

Current dependency selection2026-10-06: the owner-authorized
[fuser0.18.0 timestamp patch](../../patches/fuser-0.18.0/README.md) is applied via
this independent harness root's `[patch.crates-io]`. The historical results below
used the unmodified published crate; the new qualification is recorded separately
in [the patch record](../../docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md).
`timestamp-proof` exercises native negative, minimum-whole and minimum-fractional
cases. `request-time-proof` verifies five exact public Session parser/reply inputs,
including fractional signed minimum, with explicit DESTROY/join and bounded socket
waits. It is an external fixture, not a product proxy or native-mount substitution.
Build tests first with `--no-run`; every actual proof/test invocation needs an
explicit wall bound (native/parser cases8s, all tests at most120s). Preserve failed
cases and the owning Linux boundary normalization; do not credit these correctness
proofs as cold speed, RSS or sustained service results.

This external harness proves the required published dependencies/toolchain and
native mount/read/detach/join path. It is neither a replacement Workspace nor a
performance arm. Its own locked graph includes fuser 0.18.0, rusqlite 0.40.2
with bundled SQLite, and the current public core content library.

Observed 2026-10-05: Docker 29.5.2, Linux aarch64, kernel
`6.12.76-linuxkit`; Rust `1.85.1 (4eb161250 2025-03-15)`.
Image: `rust:1.85.1-bookworm`, immutable ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Build from repository root so shared `.cargo/config.toml` ARM64 flags apply:

```sh
docker run --rm --name layerfs-307-platform-build --platform linux/arm64 \
  -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs:/work -w /work \
  -e CARGO_TARGET_DIR=/work/core/target/cluster2-linux \
  -e LAYERFS_CONSTRUCTION_WORKERS=1 rust:1.85.1-bookworm \
  cargo build --manifest-path core/benchmark/cluster2-platform/Cargo.toml --locked
docker run --rm --name layerfs-307-platform-proof --platform linux/arm64 \
  --device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined \
  -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/target/cluster2-linux/debug/platform-proof:/usr/local/bin/platform-proof:ro \
  rust:1.85.1-bookworm /usr/local/bin/platform-proof /tmp/cluster2-platform-mount
```

Both commands exited 0. Actual binary output:

```text
content-id=ObjectId("72b30b91db95369853fba1c82da503c23c1d226ed8e6671a999f93b395861033"); bundled-sqlite=3.53.2
native Linux FUSE mount/read/detach/join PASS; no product qualification claimed
```

No dependency was patched/forked, and no alternate FUSE library was selected.
The Linux proof uses a minimal synthetic file to isolate platform readiness;
complete-root, cached mmap, permissions, fairness and integrated daemon/Exec
acceptance remain S8–S12 work. No latency, throughput or resident-bound claim
follows from this correctness proof. The disposable in-memory SQLite test is a
link/type prerequisite only; it is not the S1 disk-backed overlay profile.

# #232 staged ioctl carrier: prospective live probe

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before live Docker execution. This is a test-only FUSE provider using
> published `fuser` 0.18.0, not the LayerFS product adapter or a speed sample.

Source baseline: `304ffe3a7de2517db77aff678ee37d2bde2176dc`.
Test source: [range_ioctl_staging_probe.rs](../../../../../crates/layerfs-fuse/tests/range_ioctl_staging_probe.rs),
SHA-256 `a6f4f4e25e96ea92e368010bd35d21c7f65798ba27935786ce006985b662da9d`.
Locked release binary `range_ioctl_staging_probe-f90289f2e0dce2b6`, SHA-256
`bfc14ada6dce72693ff5907183f16c1e3d362c046bbdf3e29d67747e44d91456`.
Build: `cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline
--target aarch64-unknown-linux-musl -p layerfs-fuse --test
range_ioctl_staging_probe --release`; Rustc 1.85.1. `core/Cargo.lock` SHA-256
`09b880a18e1c221ba830908467f0985b0419ae0bf3c80c90ede7f7c5178272d6`;
root `.cargo/config.toml` SHA-256
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
Docker image `sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`
(Linux arm64); target Docker kernel observed before the probe:
`6.12.76-linuxkit` on aarch64. The run receipt must capture these again.

Run each named `PROBE_CASE` once, in a fresh mount directory. Retain the exact
Docker argv, exit, stdout/stderr, `events.log`, mountinfo, binary/image/source
hashes, and cleanup result in an append-only case directory. The test logs
actual FUSE callback `cmd`, `fh`, flags, input and output lengths, reply result
and errno. It checks caller errno and observable file bytes separately.

| Case | Prospective acceptance |
| --- | --- |
| `frames` | Total `_IOW` frames of 4096, 8192, 12288 and 16368 bytes each reach exactly one callback with that input length and a successful zero-length reply. |
| `success` | A 128-byte BEGIN yields a token; sixteen ordered 4224-byte DATA frames deliver exactly 64 KiB; before APPLY, reads remain pristine; one APPLY publishes exact bytes and one revision. |
| `wrong_order` | DATA beginning at nonzero stream offset returns `EINVAL`; no publication; ABORT consumes the stage. |
| `bad_digest` | Complete DATA with a wrong declared digest makes APPLY return `EINVAL`; no publication. |
| `stale_stamp` | BEGIN with an old revision returns `ESTALE`; no stage or publication. |
| `close` | Closing the owner descriptor discards its stage; DATA on a reopened descriptor with the old token returns `EBADF`. |
| `abort` | ABORT consumes the stage; subsequent APPLY returns `EBADF`, with no publication. |
| `unmount` | Unmount discards an unfinished stage, with no publication. |
| `lost_reply` | A test-only daemon exits after recording publication but before APPLY reply; caller sees a connection error and treats the result as UNKNOWN without retry. |

The test-only token is deterministic so request ordering is easy to audit; the
product must generate an unpredictable, incarnation-bound token. The test-only
digest check compares the fixed 64 KiB pattern and its pinned SHA-256; the
product must implement and separately test streaming SHA-256 for arbitrary
Bytes and Zero elements. This probe answers carrier GO/NO-GO, not product
atomicity, quota, cache, SDK or latency admission. Any failing or unrun case
is retained and reported; no third-party source is modified.

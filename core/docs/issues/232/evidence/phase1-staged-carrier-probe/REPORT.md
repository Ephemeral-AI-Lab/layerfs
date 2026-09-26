# #232 staged ioctl carrier probe: limited GO

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Functional Linux/FUSE carrier result only. The product staged ABI, public SDK
> route, cache eligibility and Edit→Commit latency remain unproved.

## Verdict and exact custody

**GO for implementing a bounded staged Linux ioctl carrier with published,
unmodified `fuser` 0.18.0 on the tested Docker Linux 6.12.76 aarch64 kernel.**
The test-only provider received total ioctl frames of 4,096, 8,192, 12,288
and 16,368 bytes, each with exactly the declared input length, flags
`IoctlFlags(0x0)`, zero output length and one successful reply. A 128-byte
BEGIN returned a 128-byte token response. Sixteen ordered 4,224-byte DATA
frames delivered 65,536 literal bytes; APPLY published one revision. The
test checked pristine reads before APPLY and exact inserted bytes, length
and EOF afterward. The raw callback log shows 16 DATA callbacks and one
publication. [verify.py](verify.py) checks the retained logs and checksums;
its [derived result](derived.json) is `GO_TEST_ONLY`.

The first eight cases ran once at source
`d75d4a9a569987ff95d131c15166cc4ad5028497`, probe source SHA-256
`a6f4f4e25e96ea92e368010bd35d21c7f65798ba27935786ce006985b662da9d`,
release binary SHA-256
`bfc14ada6dce72693ff5907183f16c1e3d362c046bbdf3e29d67747e44d91456`.
The successful unmount case used source
`09489a48fc4afbc42c50f570ff6b91954799495c`, probe source SHA-256
`7c283f3d5664fb3c418fedc6b70f7b601af4553a437f389bf3b42de27bf5e744`,
release binary SHA-256
`ccebe58336953778b5c730cd938da1e79738f9453ed56548ff762d5cf56c2b88`.
Both builds used `cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml
--locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test
range_ioctl_staging_probe --release`. The immutable Docker image ID was
`sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`.
Each [attempt-01](attempt-01/), [attempt-02](attempt-02/) and
[attempt-03](attempt-03/) directory retains exact Docker argv, case exit,
stdout/stderr, callback/reply/errno events, source/binary/image/kernel
identities, `SHA256SUMS` and container/volume cleanup. No third-party
package was edited. No number here is an Edit→Commit performance sample.

| Case | Actual observation | Classification |
| --- | --- | --- |
| `frames` | Input lengths 4096/8192/12288/16368, output 0, four callbacks/replies | PASS |
| `success` | BEGIN response 128; 16 DATA callbacks of input 4224; pristine before APPLY; one publication | PASS |
| `wrong_order` | Nonzero first DATA offset `EINVAL` 22; ABORT; zero publication | PASS |
| `bad_digest` | Complete DATA, APPLY `EINVAL` 22; zero publication | PASS |
| `stale_stamp` | BEGIN `ESTALE` 116; zero publication | PASS |
| `close` | Owner FD release discards stage; reopened FD DATA `EBADF` 9 | PASS |
| `abort` | ABORT then APPLY `EBADF` 9; zero publication | PASS |
| `unmount`, first source | Normal unmount with open FD returned `EBUSY` 16 | FAIL, retained |
| `unmount`, second source | Forced detach, then caller/daemon wait cycle; 30.005 s timeout | FAIL, retained |
| `unmount`, final source | Forced detach with FD open; stage discarded before caller closes FD; zero publication | PASS at changed source |
| `lost_reply` | Publication recorded, daemon exited before reply, caller `ECONNABORTED` 103; no retry | PASS for UNKNOWN handling |

The two unmount failures are explained in [attempt 01](ATTEMPT-01-FAIL.md)
and [attempt 02](ATTEMPT-02-FAIL.md). They remain failed records; the final
case uses a changed test source and a fresh mount, not a replacement receipt.
The final test's forced detach and explicit daemon cleanup prove a possible
stage-disposal ordering; they do not establish that the current product
performs it. The probe uses a deterministic token and compares its fixed
payload with a pinned SHA-256; product token randomness, general streaming
hash validation, aggregate quota, zero runs, mutation permit and Workspace
rollback/UNKNOWN custody still need implementation and external proof.

The provider supplies a workable **carrier** on this kernel. No macFUSE or
Windows result follows. No release or cold-cache latency PASS follows.

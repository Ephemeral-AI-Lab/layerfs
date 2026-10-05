# Pinned FUSE timestamp capability blocker

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

S0/S8 blocker observed 2026-10-05 while completing independent S2 engine/service
work. No third-party source/package was edited, patched, forked or substituted.
The affected dependency is the target's published `fuser =0.18.0`, Cargo VCS pin
`9c957f74efe715112049298cdf1d601781829c8d`. Its parser converts incoming signed
times before invoking LayerFS's `Filesystem::setattr` callback.

## Exact source and supported-interface finding

[Pinned upstream time.rs](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/time.rs)
subtracts a negative time's nanoseconds rather than counting them forward and
negates `i64::MIN`. `request.rs` calls the private conversion before the public
callback. Public Session/new/from_fd/run/Filesystem interfaces do not expose a
pre-conversion timestamp hook. A callback can infer/reverse some fractional errors,
but cannot prevent a parser panic which happens before that callback. No raw-wire
proxy, patched parser, overflow/profile bypass or smaller timestamp contract is
adopted as a workaround.

Upstream has an unmodified fix at
[e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7).
The crates.io API was read directly on 2026-10-05: latest published version is
0.18.0 (Rust 1.85; published 2026-07-22). GitHub's release page is not a reliable
registry-version list; it still displays 0.17.0 as its latest release. The fix
is not in the currently pinned registry package.

## Owning-platform reproduction and actual outcomes

External harness: [timestamp.rs](../../../benchmark/cluster2-platform/timestamp.rs),
through actual mounted Linux `File::set_times`/`getattr`, not a copy of dependency
code. Rust 1.85.1, Linux ARM64, kernel 6.12.76-linuxkit, `/dev/fuse`, SYS_ADMIN,
two native receiver threads, default_permissions, kernel writeback unselected.
Image ID:
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Debug binary SHA256:
`6553eb4ea82e49a327d7b320cc1d1596ea1b5b658caa4430fdbf00c533bfe972`.
The proof was built before harness-only rustfmt; its inputs/behavior are unchanged.

```sh
docker run --rm --name layerfs-307-time-build --platform linux/arm64 \
  -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs:/work -w /work \
  -e CARGO_HOME=/work/core/target/cluster2-linux-cargo \
  -e CARGO_TARGET_DIR=/work/core/target/cluster2-linux \
  sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
  cargo build --manifest-path core/benchmark/cluster2-platform/Cargo.toml --locked --bin timestamp-proof
```

Both probes used the same image, `/dev/fuse`, SYS_ADMIN, apparmor=unconfined,
the read-only executable bind and `timeout 8s timestamp-proof <mount> <mode>`.
Modes are distinct prescribed inputs, each attempted once:

| Input | Actual result | Native cleanup / complete-command disposition |
| --- | --- | --- |
| negative: `(-2,800000000)` | Callback/returned metadata becomes `(-3,200000000)` despite syscall success; exit 1, FAIL | `umount_and_join` succeeded |
| minimum: `(i64::MIN,200000000)` | Receiver panic at dependency time.rs:46, before callback; FAIL | Original 8 s stop budget did not fence a D-state FUSE waiter. Normal join did not complete. Explicit forced connection abort was required; final command exit 137. This is not cleanup PASS |

Raw build, stdout/stderr and force-cleanup logs:
[time-build.log](checks/s2-owner/time-build.log),
[time-negative.log](checks/s2-owner/time-negative.log),
[time-minimum.log](checks/s2-owner/time-minimum.log),
[time-force-cleanup.log](checks/s2-owner/time-force-cleanup.log).
No latency/cache/throughput or qualification claim follows from these diagnostics.

The minimum container was `30ed12dd292bdb3d197125c2250bbe927682ab457f6b5a1c2ee8d42005676db1`.
An ordinary Docker forced removal failed to receive an exit event. Only the owned
probe's process (`98118`, exact timestamp-proof command) and mount were inspected.
Its mountinfo identified `0:91`, `/tmp/time-minimum`, source `cluster2-time-risk`.
After confirming that exact identity again, an isolated cleanup container mounted
fusectl and aborted connection 91 only; waiting count was 1. The waiter exited,
the original container disappeared, and a final name-filtered inventory was empty.
No other connection/container/owner was interrupted. Regular native cleanup for
the failed minimum case remains unverified, rather than rewritten as success.

## Smallest required decision and independent work

Pending owner decision: allow a locked pin to the exact **unmodified upstream**
fix revision as an explicit exception to the registry/dependency-source rule,
then rebuild/requalify both native cases; or keep the registry requirement and
retain this blocker until a corrected published release exists. The dependency
and pins remain unchanged while that approval is pending. The async approval
request names this exact revision and the governing repository/user restriction.

This blocks S0's required native capability closure and complete S8/S12 admission.
It does not make partial S1/S2/S3 complete and does not stop independent engine,
backed-construction/import or runtime work. The initial indexed engine, genuine
base access, fair SQL owner, installed floors, cursor/profile evidence and real
SQLite-full atomicity have progressed independently. All unfinished criteria
remain in [PROGRESS](PROGRESS.md) and the append-only tracker comments.

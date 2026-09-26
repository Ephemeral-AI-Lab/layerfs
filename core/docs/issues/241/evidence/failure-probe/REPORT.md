# #241 mounted ioctl failure follow-up — 2026-09-24

## Decision

**The ioctl carrier remains promising; fault handling needs an explicit product
rule.** A real read-only FUSE mount still delivered `_IOW` ioctl to the daemon,
so the adapter must refuse it before mutation. A genuinely closed descriptor
was rejected by Linux with `EBADF` after FUSE `RELEASE`, before any ioctl
callback. Deliberately killing the daemon after it wrote a test-only accepted
state but before replying made the caller see `ECONNABORTED` (errno 103), while
the accepted state remained inspectable. That supports treating lost replies as
**uncertain** and forbids automatic retry. The test-only artifact is not a
Workspace, Store, or durability proof.

The important negative result is that the published `fuser` 0.18.0
`Notifier::inval_inode` returned `Ok(())` **after** `umount_and_join` and with
the mount absent. Thus a successful notifier call is not by itself proof that
a live client observed updated inode state. Its source maps `NotFound` send
errors to `Ok`, but this probe did not capture the underlying write errno, so
that mapping is a possible explanation rather than a measured cause. A real
post-publication notifier *error* inside a live ioctl callback was not
induced. No raw FUSE wire reply frames were captured; caller errno and daemon
events are the observed boundaries.

| Gate/case | Status | One-attempt observation |
| --- | --- | --- |
| Actual RO mount | **PASS for explicit adapter refusal** | Mountinfo has `ro`; one exact ioctl callback; configured-RO daemon returned `EROFS` before publication; size/mtime unchanged. Linux did not reject `_IOW` before FUSE. The product needs its own mount-authority check. |
| Actual close/release | **PASS at syscall boundary; callback stale-handle INCOMPLETE** | One OPEN and RELEASE, then ioctl on that closed numeric fd returned kernel `EBADF`; zero ioctl callbacks. Ordinary syscalls cannot supply an already released FUSE handle to test callback authorization. |
| First lost-reply attempt | **INCOMPLETE** | One callback and caller errno 103, but the original probe only logged a `published` string without accepted bytes and had a mistaken errno assertion; test exited 101. Kept raw, never rerun. |
| Revised `lost_reply_state` | **PASS for test-only custody; product uncertain outcome INCOMPLETE** | One exact callback; daemon wrote and parent verified all 12,288 test-only accepted bytes, then daemon exited 23 before reply. Caller got errno 103; no retry; mount cleanup succeeded. No claim about Workspace publication or crash durability. |
| Real notifier after unmount | **FAIL for proposed error/liveness signal** | After actual unmount, `Notifier::inval_inode(2,0,0)` returned `Ok(())`; test exited 101 because it had prospectively required an error. The API return alone cannot certify live invalidation. |

**Compatibility/coherence remains INCOMPLETE overall**, not a demonstrated
reason to abandon the ioctl carrier or optimize it. Product implementation
must (1) reject configured read-only operation before mutation, (2) validate
its own handle/version authority, and (3) retain an uncertain outcome if a
post-publication reply or cache-coherence guarantee cannot be established.
Further mounted proof must inspect actual product mutation/notification order;
the current test cannot turn a notifier `Ok` into a coherence certificate.

## Frozen method and identities

- [CONTRACT.md](CONTRACT.md) froze four initial cases before execution and
  prospectively added `lost_reply_state` after the first attempt exposed a
  marker-only harness defect. None of the first four cases was rerun.
- Independent worktree `/Users/yifanxu/.codex/worktrees/issue241-failure-probe/layerfs`,
  branch `codex/issue241-failure-probe`; base
  `3ef5824f01ceb51a326f9bfb4f77ace35cb657e7`.
  Initial measured test source commit `d7ade911ede2620cabac889dbfbd1030dbd27767`,
  binary SHA-256 `828db65baa90dd2914636128802de1445ed4b5b5cda6cefa3427667e5a8728ac`.
  Revised case source commit `62d2ba3f8fda4ab26a24cf627ebcf49fbdc05308`,
  binary SHA-256 `d74bb477668fd9e462889d99360504734ea4e2569be45622622ba9c6c54da997`.
  The original binary was superseded in the worktree-local target directory;
  its hash and source commit remain in every original attempt receipt.
- Unmodified registry `fuser` 0.18.0; `core/Cargo.lock` SHA-256
  `791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`.
  `rustc 1.85.1`, locked offline release `aarch64-unknown-linux-musl` build.
  Repository `.cargo/config.toml` SHA-256
  `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
- Privileged aarch64 Alpine image
  `sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`,
  Linux `6.12.76-linuxkit`, no container CPU or memory quota. Host is macOS
  26.4.1 arm64. [environment.json](environment.json) has container ID,
  mounts and limits. Each case used a fresh FUSE mount and virtual fixture.
- Cache state was uncontrolled. This is a semantic fault diagnostic, with no
  cold-cache, LFT1, CPU, RSS, or latency claim. Complete `docker exec` walls
  below are command-accounting numbers only, never carrier speed samples.

## Raw attempts and custody

Every [attempt directory](attempts/) retains stdout, stderr, `events.log`,
mountinfo, the exact Docker argv, exit status, timeout status, source/binary
identity and complete-command wall in `receipt.json`. The callback logs the
full 4,128 input bytes in hex. Independent decoding matched the frozen `LFR1`
request byte for byte in every callback, with command `0x5020f541`, inode 2,
handle 40, and no output bytes. There were no repeat attempts.
[verify.py](verify.py) reproduces the table's command walls, callback counts,
exact requests and case outcomes from raw files; [derived.json](derived.json)
is its retained output. [SHA256SUMS](SHA256SUMS) seals retained evidence files.

| Case | Source identity | Docker exit | Ioctl callbacks | Complete command wall ns | Relevant raw result |
| --- | --- | ---: | ---: | ---: | --- |
| `ro_mount` | initial | 0 | 1 | 96,379,459 | `EROFS`; no publication |
| `closed_fd` | initial | 0 | 0 | 89,023,833 | `EBADF` after RELEASE |
| `lost_reply` | initial | 101 | 1 | 86,586,125 | `ECONNABORTED`; marker-only harness defect and assertion failure |
| `notifier_after_unmount` | initial | 101 | 0 | 73,598,125 | real notifier returned `Ok(())` after unmount |
| `lost_reply_state` | revised | 0 | 1 | 77,266,042 | errno 103; accepted artifact 12,288 exact bytes, SHA-256 `361df1df6bf7d82ce7e5cb543b7da175c8d5920d371d1086c4c771d167425d71` |

The first lost-reply test panicked before its own unmount cleanup. A separate
owned-container `umount -l /probe/lost_reply/mnt` returned 0; the subsequent
mountinfo check found neither its mount nor the notifier case's mount. See
[cleanup.txt](cleanup.txt) and [post_cleanup_mountinfo](post_cleanup_mountinfo).
The revised case's own `umount2` returned 0 and its no-mount assertion passed.
The printed `errno=Some(103)` beside that successful cleanup is stale thread
errno after a successful syscall; only its return code is meaningful.

## Exact execution commands

Build at each source identity, from the worktree root:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_failure_probe --release
```

The unique container was created once:

```sh
docker run -d --name layerfs-241-failure-probe-d7ade911 --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-failure-probe/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-failure-probe-d7ade911,dst=/probe 5291449c3df7 -c 'sleep infinity'
```

For each case, the exact executed argv is in its `receipt.json`. Its command
shape was:

```sh
docker exec -e PROBE_CASE=<case> -e PROBE_ROOT=/probe/<case> layerfs-241-failure-probe-d7ade911 /runner/range_ioctl_failure_probe-66c5fa7842234956 --ignored --exact linux::probe --nocapture
```

The reported Docker-exec wall is Python `time.monotonic_ns()` around that
one command. `docker cp` of each `/probe/<case>/.` ran after the timer and
its exit status is in the receipt. No operation boundary uses this wall.

## Verification and limits

The Linux test-only binary was built with the locked release command and
executed in actual Docker/FUSE. The raw expected-failure tests intentionally
exit nonzero and are retained. At final test-source identity:

| Command | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | PASS, 290 production files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS, 9 tests |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` | PASS; mounted failure probe remains ignored on the macOS host |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | PASS on macOS host |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_failure_probe --release -- -D warnings` | BLOCKED before probe by existing `clippy::useless_conversion` in `core/crates/layerfs-workspace/src/backing/segments.rs:192` |
| `python3 core/docs/issues/241/evidence/failure-probe/verify.py` | PASS against all five retained raw attempts |

The Linux release build and mounted runs exercise the test despite the
target-specific Clippy blocker. No source under `core/crates/*/src/`, benchmark
registry or tool, Cargo dependency, or third-party package changed. Raw
kernel FUSE reply frames, post-publication live-notifier failure,
product-level stale-handle rejection, and Workspace/Commit persistence remain
unproved. The owned container and volume were removed after copying all raw
files; [cleanup.txt](cleanup.txt) records the commands.

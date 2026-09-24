# #241 mounted caller acknowledgement probe — 2026-09-24

## Decision

**The caller-confirmation rule passed this test-only Linux FUSE probe.** With a
real `inval_inode` before the reply, ioctl returned zero and the existing
descriptor immediately reported the new 12,288-byte size, newer mtime, exact
boundary bytes and EOF. With notification deliberately omitted, ioctl still
returned zero and direct-I/O reads saw the inserted bytes, but the *same*
descriptor's `fstat` retained the old 8,192-byte size and mtime. The caller
classified that combination **UNCERTAIN**, never success, and did not retry.
When the daemon deliberately exited after writing test-only accepted state but
before replying, ioctl returned `ECONNABORTED` (errno 103); the caller again
classified **UNCERTAIN** and did not retry.

This establishes a usable caller-side confirmation pattern for the candidate
carrier. It is not a Workspace, Store, Commit, durability or cold-cache proof.
The omitted notification is a test-only fault, not a real provider send error.
No real live-callback notifier error was induced, so that gate remains
**INCOMPLETE**. The source review in the preceding failure follow-up also
showed that `fuser` may return `Ok(())` after unmount; this probe does not use
that return alone as a success certificate. There is no demonstrated ioctl
carrier blocker or carrier optimization need.

| Frozen case | Result | Same-FD observation and caller decision |
| --- | --- | --- |
| `ack_success` | **PASS** | ioctl `0`; `fstat=(12288,1700000001)` from initial `(8192,1700000000)`; boundary `4a4b4c000102030405060708090a0b0c`; EOF empty; **SUCCESS**. A post-invalidation GETATTR callback occurred. |
| `ack_suppressed` | **PASS for detecting this induced stale state** | ioctl `0`; direct boundary read had the new bytes, but `fstat=(8192,1700000000)` stayed stale and no post-ioctl GETATTR occurred; **UNCERTAIN**. |
| `ack_lost_reply` | **PASS for test-only uncertain custody** | ioctl error `ECONNABORTED` 103; accepted 12,288-byte artifact verified exactly; **UNCERTAIN**. Daemon exited 23 before reply. |

Every case issued exactly one ioctl with command `0x5020f541`, inode 2, handle
40 and the same exact 4,128 input bytes. Callback counts were one each, retry
counts zero. Full callback inputs, notification and reply-method decisions,
caller errno/readback, mountinfo and exit context are retained in
[attempts/](attempts/). The caller's `ioctl` return/errno is the observable
reply boundary; raw `/dev/fuse` reply frames were not captured. The daemon's
`ReplyIoctl::ioctl` method returns no delivery result, so a logged method call
is not treated as caller acknowledgement.

## Frozen identity and execution

- [CONTRACT.md](CONTRACT.md) froze all three cases before any mounted run.
  One attempt per case, in contract order; no changes to source, binary,
  workload or gates between attempts.
- Independent worktree `/Users/yifanxu/.codex/worktrees/issue241-ack-probe/layerfs`,
  branch `codex/issue241-ack-probe`, base
  `6f56e5d6f47bf43559bb365a28544526cdf1a248`. Contract commit
  `590ea3735`; test-source commit
  `80496bdbc9bc34ff31cf537af0ababc7eb8371e7`.
- Test source SHA-256 `2cb244ce1716e336614bcd73ee5f59962a0be25e1124a7b00c46b2a09ecb8dba`;
  locked release binary SHA-256
  `b4f094b6e8c0bf1885c28dfbecf73145e195cd63c1e984b314b9d8cc4c8de7d5`.
  Unmodified `fuser` 0.18.0; `core/Cargo.lock` SHA-256
  `791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`;
  repository `.cargo/config.toml` SHA-256
  `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
- `rustc 1.85.1`; target `aarch64-unknown-linux-musl`; Alpine image
  `sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`;
  privileged Linux `6.12.76-linuxkit` aarch64, no CPU or memory quota.
  [environment.json](environment.json) records container/mount identity.
- Fresh test-only FUSE mount and logical fixture per case, 60-second attribute
  TTL, writable regular open returning `FOPEN_DIRECT_IO`. Cache state was
  uncontrolled; initial `fstat` intentionally populated attribute cache for
  the semantic test. There is no timing, CPU, RSS or cold-cache performance
  claim. Docker-exec walls below include harness lifecycle and are reported
  only for command accounting.

| Case | Docker exit | Complete command wall ns | GETATTR callbacks | READ callbacks | Retained decision |
| --- | ---: | ---: | ---: | ---: | --- |
| `ack_success` | 0 | 109,900,500 | 2 | 2 | SUCCESS |
| `ack_suppressed` | 0 | 81,009,833 | 1 | 2 | UNCERTAIN |
| `ack_lost_reply` | 0 | 92,446,125 | 1 | 0 | UNCERTAIN |

The accepted artifact's SHA-256 is
`361df1df6bf7d82ce7e5cb543b7da175c8d5920d371d1086c4c771d167425d71`.
It is a test-only byte record outside the mount, written before intentional
daemon exit; it does not establish crash durability or real product
publication. Each mount was absent from `/proc/self/mountinfo` after its case;
the owned container and volume were removed after raw copy. See
[post_cleanup_mountinfo](post_cleanup_mountinfo) and [cleanup.txt](cleanup.txt).

## Commands and reproduction

Locked release build from the worktree root:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_ack_probe --release
```

One container setup outside the case attempts:

```sh
docker run -d --name layerfs-241-ack-probe-80496bdb --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-ack-probe/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-ack-probe-80496bdb,dst=/probe 5291449c3df7 -c 'sleep infinity'
```

The exact executed Docker argv, exit, timeout, SHA identities and
`time.monotonic_ns()` complete-command wall are in each `receipt.json`. The
shape, substituted once per frozen case, was:

```sh
docker exec -e PROBE_CASE=<case> -e PROBE_ROOT=/probe/<case> layerfs-241-ack-probe-80496bdb /runner/range_ioctl_ack_probe-eac4f53eedc9a66a --ignored --exact linux::probe --nocapture
```

After each command, `docker cp layerfs-241-ack-probe-80496bdb:/probe/<case>/.
<attempt-dir>` copied raw files outside the command wall. No case was rerun.
[verify.py](verify.py) independently checks every request byte, callback
count, mount profile, caller decision and accepted artifact from raw files;
[derived.json](derived.json) is its output. [SHA256SUMS](SHA256SUMS) hashes
every retained evidence file except itself.

## Remaining scope

At final test-source identity, `python3 core/tools/check_product_boundary.py`
passed (290 production files), `python3 -m unittest discover -s core/tools -p
'test_*.py'` passed (9 tests), `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all -- --check` passed, `cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked` passed, and `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` passed
on macOS. The additional Linux-target command `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --locked --target
aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_ack_probe
--release -- -D warnings` stopped before this test at the pre-existing
`clippy::useless_conversion` in
`core/crates/layerfs-workspace/src/backing/segments.rs:192`. The locked Linux
release build and mounted cases did execute. `python3
core/docs/issues/241/evidence/ack-probe/verify.py` passed against all three
retained attempts.

The readback rule is a proposed addition to the cooperating caller and will
add FUSE work to a real Edit→Commit route; its full cost is unmeasured here.
Only the existing descriptor was read in these three cases. Alias and later
write coherence have separate positive carrier evidence but are not proved by
this caller probe. The product must still validate mount writability and
handle/version authority before publication, use the actual Workspace splice,
preserve a mutation receipt on uncertain outcomes, and independently verify
Commit and old-Branch behavior. A live provider notification failure and raw
reply-send failure remain uninduced; do not promote this test-only PASS into
full Phase 1 or #241 release admission.

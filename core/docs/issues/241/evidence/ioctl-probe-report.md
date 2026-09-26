# #241 standalone ioctl carrier probe — 2026-09-24

## Decision

**Carrier feasibility: promising. No provider/kernel blocker or carrier
performance miss was demonstrated.** The published `fuser` 0.18.0 and Linux
6.12.76 kernel delivered the 4,128-byte ioctl once;
the tested direct-I/O plus inode-invalidation profile provided immediate
length, mtime, old-FD, alias, read/EOF and later-write coherence. The observed
carrier wall rows were below the frozen 1.0 ms diagnostic screen. **Efficiency
is INCOMPLETE**: LFT1 has one resource sample per sub-millisecond operation and
therefore `cpu_shared_ns:null` in every row. Page-cache state is uncontrolled;
none of these rows is a cold admission PASS. **Compatibility/coherence is
INCOMPLETE** for fault behavior: the post-publication notification failure was
injected in the probe before the real notifier send; actual kernel notifier
failure, reply loss, actual stale-handle lifecycle and read-only *mount*
capability detection were not proved. The callback log holds exact request
bytes; raw kernel reply frames were not captured. Client success/errno came
from the mounted test assertions. These are evidence gaps, not observed ioctl
failures or a reason to optimize the carrier.
No `range_ioctl.rs`, product `src/`, benchmark tool or registry was changed.

**Phase 1 sign-off remains incomplete**, so this probe does not authorize the
product implementation gate in the #241 plan. The next step is a focused
prospective proof of the missing failure/capability behavior and sub-millisecond
CPU observation. It is not a recommendation to abandon ioctl or tune its
latency.

This is a carrier diagnostic only. The virtual file never calls Workspace,
Store, SDK or Commit. The 4 KiB WRITE control is a carrier control, not an
insert baseline or a #232/#241 product latency comparison.

| Gate | Decision | Evidence and limit |
| --- | --- | --- |
| Delivery/ABI | **PASS** | Insert, delete and overwrite each reached one callback. Command `0x5020f541`, inode `2`, handle `40`, `in=4128`, `out=0`; full bytes in each raw `daemon.log`. Seven refusal calls returned the asserted errno with unchanged size/mtime. |
| Mounted compatibility/coherence | **INCOMPLETE** | The positive direct-I/O/invalidation path passed; fault and mount capability boundaries above remain unproved. |
| Carrier efficiency | **INCOMPLETE** | All four ioctl walls <1.0 ms, one callback, no suffix callbacks, but CPU is unavailable from 10 ms LFT1 sampling and cache admission is ineligible. |

## Frozen identity and method

- Worktree `/Users/yifanxu/.codex/worktrees/issue241-ioctl-feasibility/layerfs`, branch `codex/issue241-ioctl-feasibility`, parent `65ffd102c905ff5eda3e5681f044a7d7344ca8b1`, parent tree `9f1227579c87fe4af05dfdb24b6d715fdaeb0e3e`.
- Frozen protocol/cases: [ioctl-probe-contract.md](ioctl-probe-contract.md). Only a 4 KiB inline replacement is tested. Linux's command encoding has a 14-bit size field; no 64 KiB atomic-edit claim follows.
- `fuser` 0.18.0 registry checksum `b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`, unmodified. `core/Cargo.lock` SHA-256 `791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`.
- `rustc 1.85.1 (4eb161250 2025-03-15)`, Zig 0.16.0, target `aarch64-unknown-linux-musl`, locked offline release. Repository `.cargo/config.toml` SHA-256 `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9` supplies `aes_armv8`, `polyval_armv8`, `chacha20_force_neon`, `+aes,+sha2`.
- Docker image `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`, privileged, no CPU quota. Host is Darwin arm64; mounted experiment ran inside Linux aarch64 Docker, kernel `6.12.76-linuxkit`, FUSE protocol `Version(7, 41)`. Actual mountinfo: `rw,nosuid,nodev,noatime - fuse layerfs rw,user_id=0,group_id=0,default_permissions,max_read=1048576`. The probe's OPEN returns `FOPEN_DIRECT_IO`; it calls `Notifier::inval_inode(2, 0, 0)` before the success reply.
- All cases used fresh logical virtual-file state and a fresh FUSE mount. Untouched bytes were computed by offset. The container and executable were reused; instruction/kernel cache was uncontrolled. No timed source data pages were intentionally primed, but this is **not** a cold contract. WRITE and ioctl controls share that declared unknown cache class, not a proved equal cache state. No ratio or cold PASS is inferred.
- The initial insert used test-source SHA-256 `938a62665fabdc1bc7b66007f5b307f9638163a8b0cef8c75de2a6aa1782bdc6`, binary `24ecb2fd5709da739d13326489df5ada35636c8f27358f0c5d66f8280dbd74ab`. Daemon LFT1 callback timing was added before the remaining cases. They used source SHA-256 `cf1ba3373fe4c0be3840b9772764952c3b71517b3ffb5b36ac6611fd85ecc7142bcf8519e`, binary `9c4c21f33cf1b2797b29451c4102e05c05ac46b9718da9acb8d0aecb9ef9da9a`. This was a diagnostic instrumentation change, not a replacement sample. After all attempts, `cargo fmt` changed whitespace: final test-source SHA-256 `185e20dd15a95b051d6e7b2da64edf471cf5b36ac6611fd85ecc7142bcf8519e`, rebuilt binary `d49971ceec8359567091d92dad10c63ab3ad479537cfaac3f591c37a8e0c2203`. The formatted binary was **not** measured or substituted for retained rows.

## Raw attempts and observations

Every declared case was invoked once. [attempts.jsonl](attempts.jsonl) gives
the command, exact tested source/binary hash, exit, cache declaration, LFT1
operation fields and SHA-256 of each raw file. [attempts/](attempts/) retains
each daemon's exact ioctl input hex, callback counts, FUSE mountinfo, and
unmodified caller/daemon LFT1 segments. All test processes exited 0 and all
mounts were absent from `/proc/self/mountinfo` after their cases.

Independent Python decoding of the raw hex confirmed `LFR1`, version 1,
reserved/flag zeros, 4,128 input bytes and the full 4 KiB pattern. The insert
request SHA-256 is `8dddd633474264fffba3257e16ba1e1d14142f64ad212197a96396ede5a626af`.
Delete uses `(offset=4093, deletion=4096, insertion=0)`; overwrite uses
`(4093,4096,4096)`; the four virtual-size insert offsets are exactly
`524288`, `5242880`, `52428800` and `262144000` bytes.

| Case | Result | Ioctl / READ / WRITE callbacks | Resulting size / notes |
| --- | --- | --- | --- |
| insert | PASS | 1 / 0 / 0 | 12,288 bytes; first instrumented build |
| delete | PASS | 1 / 0 / 0 | 4,096 bytes |
| overwrite | PASS | 1 / 0 / 0 | 8,192 bytes |
| refusals | PASS for checked branches | 7 / 0 / 0 | Version, 4097 length, flags, overflow, read-only FD, injected stale-handle marker and unknown command refused; size 8,192 and mtime unchanged |
| coherence | PASS for tested positive path | 1 / 7 / 1 | 12,288 after insert; three existing/opened paths agreed on inode, size, newer mtime, boundary bytes and EOF; later 5-byte write read through alias |
| uncertain | PASS for injected classification | 1 / 0 / 0 | Client got `EIO`; daemon retained 12,288-byte published state with `injected-notification-failure uncertain` custody log |

The efficiency rows below are **one diagnostic attempt each**. Wall is LFT1
operation `elapsed_ns`; RSS is LFT1 `sampled_max_rss` in bytes, an absolute
sampled process value rather than an operation allocation or peak. CPU is
`null` in every caller/daemon record. Each ioctl row has one ioctl callback,
4,128 input bytes and zero READ/WRITE callbacks. Each WRITE row has one
4,096-byte WRITE callback and zero ioctl/READ callbacks.

| Virtual size | Ioctl caller / daemon wall ns | Ioctl caller / daemon RSS B | WRITE caller / daemon wall ns | WRITE caller / daemon RSS B |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB | 301,792 / 225,209 | 831,488 / 2,703,360 | 396,041 / 337,042 | 901,120 / 13,139,968 |
| 10 MiB | 281,291 / 230,625 | 901,120 / 4,755,456 | 376,708 / 268,750 | 888,832 / 8,269,824 |
| 100 MiB | 266,333 / 210,083 | 901,120 / 8,019,968 | 335,750 / 287,292 | 831,488 / 10,055,680 |
| 500 MiB | 229,042 / 182,334 | 974,848 / 8,609,792 | 320,958 / 272,833 | 839,680 / 12,922,880 |

No wall row approached 5.30 ms. Count evidence establishes no suffix
callbacks; the virtual model itself stores one replacement buffer and does
not model Core's Workspace, Commit, or real-file memory cost. LFT1's 10 ms
minimum monitor interval gave `samples=1` for these sub-millisecond windows,
so its covered CPU delta is unavailable. The callback timer includes test-only
hex/event logging; those costs remain in the reported diagnostic wall.
The Docker-exec complete-command wall was not captured as a separate
receipt field; the test harness printed approximately 0.04–0.05 s per case,
which is not an exact full-command wall measurement.

## Commands and gaps

The exact build command was:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_probe --release
```

The mount container was created with:

```sh
docker run -d --name layerfs-241-probe-1 --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-ioctl-feasibility/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-probe-1,dst=/probe alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8 -c 'sleep infinity'
```

For every case, the full `docker exec` command and resulting raw directory
are recorded in `attempts.jsonl`; no case was repeated to improve a number.
The first locked build required updating the test-only lockfile edge; three
compile attempts then found missing `native` telemetry feature and Linux
`ioctl` type / non-exhaustive `Config`, a role type mismatch, and a callback
`Result` destructuring mismatch. The locked release build succeeded after
those source fixes.
No package was patched, vendored or forked. `cargo update --offline -p
layerfs-fuse` added only the test-only telemetry dependency edge to the lockfile.

Final checks: `python3 core/tools/check_product_boundary.py` PASS (290 files),
`python3 -m unittest discover -s core/tools -p 'test_*.py'` PASS (9 tests),
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` PASS,
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`
PASS on the macOS host, and `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked` PASS. The additional target-specific Clippy command
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --target
aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_probe --release
-- -D warnings` was **blocked** by an existing `clippy::useless_conversion`
error in `core/crates/layerfs-workspace/src/backing/segments.rs:192`, before
checking this Linux-only test. That product file was outside this probe's edit
scope and was not modified. The Linux test was compiled and run via the locked
release build above.

Before Phase 1 sign-off, a new prospective probe should demonstrate real
notifier/reply failure custody, read-only mount behavior and actual stale-handle
lifecycle, and use a CPU observation method whose boundary resolves a
sub-millisecond operation. These missing proofs must remain separate from this
one-attempt receipt. No carrier optimization is indicated by the measured wall
rows.

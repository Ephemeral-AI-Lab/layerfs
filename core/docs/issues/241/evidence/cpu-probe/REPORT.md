# #241 mounted ioctl CPU follow-up — 2026-09-24

## Finding

**The sub-millisecond carrier CPU observation gap is partly closed.** Two
prospectively frozen, single-attempt Linux FUSE ioctls produced valid two-sample
LFT1 process CPU windows for caller and daemon. The native 10 ms monitor's
`cpu_shared_ns:null` result was a sampling limitation; it was not a slow-CPU
observation. The new test-only `WindowSource` takes resource snapshots outside
the LFT1 wall timer, so it does not lengthen the measured ioctl call.

No carrier CPU anomaly or size-proportional callback was seen. **This is a
diagnostic, not Phase 1 efficiency admission:** LFT1 process CPU includes any
other thread in the same process and the boundary windows are slightly wider
than the wall timer. The separate thread-clock numbers narrow attribution to
the calling/FUSE callback thread but are not LFT1 `cpu_shared_ns`. RSS is an
absolute two-endpoint observation, not a phase peak or allocation delta. The
virtual file does not exercise Workspace, Store or Commit; cache state is
uncontrolled. No cold Edit→Commit result follows.

| Gate | Status | Evidence |
| --- | --- | --- |
| Mounted carrier delivery | **PASS** for the two frozen cases | Actual Linux 6.12.76 FUSE mount; one `0x5020f541` callback, 4,128 input bytes, zero READ/WRITE callbacks per case; successful `fstat` size after ioctl. |
| CPU observation | **PASS** for measurable process and thread boundaries; **INCOMPLETE** for exclusive whole-operation attribution | Each LFT1 record has two samples, zero gaps, and nonzero CPU delta. Process/shared and thread scopes are explicitly separate; 1 µs `getrusage` quantization remains. |
| Carrier efficiency admission | **INCOMPLETE** | Cache state unknown; virtual file; no eligible cold profile or product route. |

## Frozen identity and method

- Worktree `/Users/yifanxu/.codex/worktrees/issue241-cpu-probe/layerfs`, branch `codex/issue241-cpu-probe`; parent `3ef5824f01ceb51a326f9bfb4f77ace35cb657e7`; frozen source/contract commit `de4cb0db560d7bbd2a8486395555ac86c31176d2`.
- [Prospective contract](CONTRACT.md): exactly `ioctl-1m` at 524,288 and `ioctl-500m` at 262,144,000. One mounted attempt per case, in that order, with a new logical virtual file and FUSE mount. No WRITE control or ratio.
- Test source SHA-256 `26b85fee72405dbc94670209097278463b5eb3b6b7372907a666594fe5a0789e`; locked release test binary SHA-256 `41487f40f865004e39b40087d2dd8b4e8d8d018a0fe3ea9c4b7a68426cb63313`.
- `core/Cargo.lock` SHA-256 `791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`; repository `.cargo/config.toml` SHA-256 `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`; published, unmodified `fuser` 0.18.0. `rustc 1.85.1 (4eb161250 2025-03-15)`, Zig 0.16.0, locked offline release, target `aarch64-unknown-linux-musl`.
- Container `27eca219b3da4ef9b1c0bc7bc150848c1acbc281807a65a52759d0d1448846c2`, image `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`, privileged, Linux `6.12.76-linuxkit` aarch64. Mountinfo records `rw,nosuid,nodev,noatime - fuse layerfs rw,user_id=0,group_id=0,default_permissions`; FUSE protocol `Version(7, 41)`. Open returned `FOPEN_DIRECT_IO`; callback sent `inval_inode(2,0,0)` before success reply. No mount remained after either case.
- `getrusage(RUSAGE_SELF)` is process-shared user/system CPU with 1 µs native quantum. `CLOCK_THREAD_CPUTIME_ID` is the separate caller/callback thread delta. `/proc/self/statm` gives resident page endpoints. Snapshot acquisition occurs outside the LFT1 operation timer; LFT1 still owns wall timing and encodes the two-sample process window. The probe does not run the periodic telemetry monitor. No CPU number is described as exclusive total carrier cost. No lifetime `ru_maxrss` or cgroup `memory.peak` is used.
- Cache contract: virtual source bytes have no backing storage; OS kernel/instruction and metadata caches were uncontrolled. The container/executable were reused, while each case had a fresh file state and mount. The rows are diagnostic and not cold eligible. The parent task's other worktree did not overlap these timed windows.

## Raw results

Both case invocations returned exit 0; the [raw attempt directories](attempts/)
retain unmodified caller/daemon LFT1, thread CPU sidecars, mountinfo and daemon
counts. Test stdout and stderr are retained beside the command receipts
[`ioctl-1m.command.json`](ioctl-1m.command.json) and
[`ioctl-500m.command.json`](ioctl-500m.command.json). Raw file SHA-256 values
are in [raw-sha256.txt](raw-sha256.txt).

| Case | Caller LFT1 wall ns | Caller process CPU user+sys ns | Caller thread CPU ns | Daemon LFT1 wall ns | Daemon process CPU user+sys ns | Daemon thread CPU ns | Complete `docker exec` wall ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 MiB | 71,917 | 7,000 + 14,000 | 18,499 | 9,542 | 10,000 + 2,000 | 10,708 | 83,909,167 |
| 500 MiB | 64,000 | 0 + 16,000 | 13,750 | 9,458 | 9,000 + 4,000 | 10,709 | 80,948,083 |

Each operation LFT1 has `samples=2`, `gaps=0`, `concurrency=1`, `source=supplied`,
and `cpu_shared_ns` equal to the process user/system pair above. Caller
first-to-last resource window widths were 82,667 and 70,458 ns; daemon widths
were 14,583 and 14,917 ns. These exceed the operation wall times by the LFT1
timing machinery and boundary sampling. Process CPU and thread CPU must not be
added together. The caller and daemon process windows are on different local
clocks, so their wall times must not be summed into an end-to-end duration.

Absolute sampled RSS endpoints were caller 2,863,104→2,867,200 B and daemon
17,940,480→17,940,480 B for 1 MiB; caller 778,240→782,336 B and daemon
17,940,576→17,940,576 B for 500 MiB. The 4 KiB caller change can include
the measurement's own procfs read and allocator activity. These endpoints do
not establish a phase memory peak. Daemon `ioctl=1`, `input_bytes=4128`,
`read=0`, `write=0` in both cases; resulting lengths were 1,052,672 and
524,292,096 B, observed through `fstat` before the caller exited.

## Reproduction and checks

Build from this worktree root:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_cpu_probe --release
```

Create the isolated Linux container (the immutable binary is bind mounted):

```sh
docker run -d --name layerfs-241-cpu-probe-de4cb0 --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-cpu-probe/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-cpu-probe-de4cb0,dst=/probe alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8 -c 'sleep infinity'
docker exec layerfs-241-cpu-probe-de4cb0 mkdir -p /probe/ioctl-1m /probe/ioctl-500m
```

Invoke each case once. The host wrapper around these exact argv recorded
`time.perf_counter_ns()` across each whole `docker exec` and retained stdout,
stderr and exit status in the corresponding `.command.json`/`.stdout`/`.stderr`:

```sh
docker exec -e PROBE_CASE=ioctl-1m -e PROBE_ROOT=/probe/ioctl-1m layerfs-241-cpu-probe-de4cb0 /runner/range_ioctl_cpu_probe-1aff9fc4deff40ad --ignored --exact linux::probe --nocapture
docker exec -e PROBE_CASE=ioctl-500m -e PROBE_ROOT=/probe/ioctl-500m layerfs-241-cpu-probe-de4cb0 /runner/range_ioctl_cpu_probe-1aff9fc4deff40ad --ignored --exact linux::probe --nocapture
```

The locked build and both actual mounted ignored tests passed. Final checks
from this worktree root: `python3 core/tools/check_product_boundary.py` PASS
(290 files); `python3 -m unittest discover -s core/tools -p 'test_*.py'`
PASS (9 tests); `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all
-- --check` PASS; `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked` PASS; `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked --examples` PASS; and `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --locked --all-targets -- -D warnings` PASS on the macOS
host. The host compilation excludes this Linux-only test module. The
target-specific `cargo +1.85.1 clippy --manifest-path core/Cargo.toml
--locked --target aarch64-unknown-linux-musl -p layerfs-fuse --test
range_ioctl_cpu_probe --release -- -D warnings` stopped before this test at
the existing `clippy::useless_conversion` error in
`core/crates/layerfs-workspace/src/backing/segments.rs:192`; no product source
was changed. The Linux test did compile in the locked release build and ran
through the actual mounted kernel path.

These results do not replace the fault/capability probe or authorize product
Edit→Commit claims. Production LOC for the source/contract commit and this
test/docs/evidence commit is unchanged: combined 119,981→119,981 (delta +0),
Core 54,564→54,564, reference 65,417→65,417, counted by
`python3 tools/production_loc.py --root . --json` on the parent and staged
snapshots; tests and docs are excluded.

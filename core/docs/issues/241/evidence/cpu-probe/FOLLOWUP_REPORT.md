# #241 CPU probe follow-up — 2026-09-24

## Decision

The two missing ioctl sizes and four same-class 4 KiB WRITE controls all
completed in one mounted Linux FUSE attempt each. Every ioctl had one
4,128-byte callback and no READ/WRITE callback; every WRITE had one
4,096-byte callback and no ioctl/READ callback. LFT1 captured two valid
boundary process CPU samples in caller and daemon for all six cases.
No carrier blocker or >1 ms diagnostic caller wall was seen. The first new
ioctl row (`ioctl-10m`) was higher than the others at 224,750 ns; it is
retained as-is with no repeat or size-scaling inference.

**Efficiency admission remains INCOMPLETE.** These are virtual-file carrier
diagnostics with uncontrolled kernel/instruction/metadata caches. LFT1 CPU
is process-shared, RSS is only two absolute endpoints, and WRITE does not
perform insert semantics. The previous `ioctl-1m`/`ioctl-500m` receipts used
the [first probe's](REPORT.md) different source/binary identity; they were
not rerun or pooled with these rows. Nothing here measures Workspace or
Edit→Commit.

| Gate | Status | Evidence and limit |
| --- | --- | --- |
| Mounted delivery/counts | **PASS** | Six real Linux mounts; exact one selected callback and byte count per case; zero other callbacks; `fstat` checked resulting length. |
| CPU observation | **PASS** for measurable process boundaries; **INCOMPLETE** for exclusive operation attribution | Caller and daemon LFT1 `samples=2`, `gaps=0`, nonzero CPU; independent thread-clock sidecars. `getrusage(RUSAGE_SELF)` has 1 µs quantum and includes other process threads. |
| Carrier efficiency admission | **INCOMPLETE** | Unknown cache state, virtual source, no cold admission or semantic WRITE baseline. |

## Frozen identity and method

- Worktree `/Users/yifanxu/.codex/worktrees/issue241-cpu-probe/layerfs`, branch `codex/issue241-cpu-probe`. Parent evidence commit `fa2f28715806637261bc84f3627738e05ecc49ab`; frozen follow-up source/contract commit `09f30cd63655700a5e23d1499f200086082a6437`. The [six-case prospective contract](FOLLOWUP_CONTRACT.md) was committed before the first Docker attempt.
- Test source SHA-256 `6db0f115f306cc0b89a026e9cd89bdbbeab463a5a7f7d07f34219d8d45168608`; locked release test binary SHA-256 `3351ff933c80c1b778280c638c74518cc8705048e3399818bc3a740c6e7a0cba`. `core/Cargo.lock` SHA-256 `791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`; repository `.cargo/config.toml` SHA-256 `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`. Published, unmodified `fuser` 0.18.0; `rustc 1.85.1 (4eb161250 2025-03-15)`, Zig 0.16.0; locked offline `aarch64-unknown-linux-musl` release.
- Container `837f6819b85ccf5688e5148935e9c7d17fbda349bc5315145a9bf0d22753670e`, image `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`, privileged, Linux `6.12.76-linuxkit` aarch64, FUSE `Version(7, 41)`. The raw mountinfo files show `rw,nosuid,nodev,noatime - fuse layerfs rw,user_id=0,group_id=0,default_permissions`; open returned `FOPEN_DIRECT_IO`. No probe mount remained after the six attempts; the container and volume were removed after copying evidence.
- Source uses test-only `WindowSource` snapshots before and after the LFT1 wall timer. LFT1 `cpu_shared_ns` uses `getrusage(RUSAGE_SELF)` user/system CPU, native 1 µs quantum; the separate `CLOCK_THREAD_CPUTIME_ID` sidecar is the caller/FUSE callback thread. `/proc/self/statm` RSS is sampled at two endpoints. Resource windows include timing machinery and may include concurrent process threads. No lifetime `ru_maxrss`, cgroup `memory.peak`, or Python timer is substituted for operation telemetry. Python `time.perf_counter_ns()` records only complete `docker exec` wall.
- New logical virtual state and fresh mount for each case; executable and Docker image reused. No source data page backing, cache invalidation or priming. Ordinary cache state remains unknown; no cold PASS or ioctl/WRITE ratio is inferred. The earlier 1/500 MiB ioctl rows are a different compiled identity.

## Raw observations

The [follow-up attempt directories](followup-attempts/) contain unmodified
caller/daemon LFT1, thread sidecars, daemon counts and mountinfo. The six
`followup-*.command.json` files retain exact `docker exec` argv, exit status
and complete wall; matching `.stdout`/`.stderr` are retained. All raw file
hashes in [followup-raw-sha256.txt](followup-raw-sha256.txt) verified.

| Case | Caller LFT1 wall ns | Caller CPU user+sys ns | Caller thread CPU ns | Daemon LFT1 wall ns | Daemon CPU user+sys ns | Daemon thread CPU ns | Complete command wall ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ioctl-10m | 224,750 | 44,000 + 22,000 | 58,959 | 22,417 | 26,000 + 3,000 | 26,375 | 95,011,750 |
| ioctl-100m | 88,959 | 5,000 + 15,000 | 16,917 | 15,875 | 13,000 + 5,000 | 17,292 | 96,766,333 |
| write-1m | 88,417 | 3,000 + 18,000 | 18,249 | 6,458 | 13,000 + 1,000 | 7,958 | 82,120,458 |
| write-10m | 77,625 | 5,000 + 17,000 | 19,166 | 7,291 | 9,000 + 1,000 | 8,666 | 87,295,250 |
| write-100m | 79,292 | 5,000 + 18,000 | 19,709 | 6,542 | 8,000 + 1,000 | 7,834 | 79,940,209 |
| write-500m | 69,208 | 16,000 + 8,000 | 20,750 | 5,917 | 8,000 + 1,000 | 7,209 | 80,032,375 |

The first `ioctl-10m` caller window lasted 252,750 ns around a 224,750 ns
wall timer, with 66,000 ns process CPU and 58,959 ns caller-thread CPU.
The receipt does not identify why this single row was higher; off-CPU time,
kernel scheduling and uncontrolled cache state are possible. The next
`ioctl-100m` caller wall was 88,959 ns. No resampling or best-of selection
was done. CPU scopes are not additive: process CPU includes the thread CPU,
and caller/daemon timers have independent local clocks.

The two ioctl daemon logs show lengths 10,489,856 and 104,861,696 B,
`ioctl=1`, `input_bytes=4128`, `read=write=0`. The four WRITE logs show
unchanged lengths 1/10/100/500 MiB, `write=1`, `write_bytes=4096`,
`ioctl=read=0`. The WRITE callback acknowledges a 4 KiB FUSE write; it does
not store, read back or splice a suffix. It is a transport control only.

RSS remained an absolute endpoint observation: each new caller ended one
4 KiB page above its beginning (most 778,240→782,336 B; `ioctl-100m`
770,048→774,144 B). The daemon RSS endpoints were unchanged within every
case: 17,944,576 B (`ioctl-10m`), 17,936,384 B (`ioctl-100m`),
17,940,480 B (`write-1m`), 17,944,576 B (`write-10m`), 17,948,672 B
(`write-100m`) and 17,944,576 B (`write-500m`). The caller's 4 KiB change
can include the probe's own procfs/allocator activity; none is a phase peak.

## Commands and verification

From the worktree root, build the immutable test binary with:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_cpu_probe --release
```

The container setup was:

```sh
docker run -d --name layerfs-241-cpu-followup-09f30 --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-cpu-probe/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-cpu-followup-09f30,dst=/probe alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8 -c 'sleep infinity'
docker exec layerfs-241-cpu-followup-09f30 mkdir -p /probe/ioctl-10m /probe/ioctl-100m /probe/write-1m /probe/write-10m /probe/write-100m /probe/write-500m
```

The host wrapper invoked the following exact `docker exec` command form once
for each case in the frozen order, with `CASE` equal to each row's case ID.
The command receipts retain every expanded argv and `time.perf_counter_ns()`
complete wall:

```sh
docker exec -e PROBE_CASE=CASE -e PROBE_ROOT=/probe/CASE layerfs-241-cpu-followup-09f30 /runner/range_ioctl_cpu_probe-1aff9fc4deff40ad --ignored --exact linux::probe --nocapture
```

The exact host wrapper was one invocation with six arguments; it performed
one `subprocess.run` per argument, kept each exit and raw output, and did not
retry failures:

```sh
python3 -c 'import json,pathlib,subprocess,time,sys; root=pathlib.Path("core/docs/issues/241/evidence/cpu-probe"); failed=False; cases=sys.argv[1:];
for name in cases:
 cmd=["docker","exec","-e","PROBE_CASE="+name,"-e","PROBE_ROOT=/probe/"+name,"layerfs-241-cpu-followup-09f30","/runner/range_ioctl_cpu_probe-1aff9fc4deff40ad","--ignored","--exact","linux::probe","--nocapture"]
 start=time.perf_counter_ns(); p=subprocess.run(cmd,capture_output=True); elapsed=time.perf_counter_ns()-start; prefix=root/("followup-"+name); prefix.with_suffix(".stdout").write_bytes(p.stdout); prefix.with_suffix(".stderr").write_bytes(p.stderr); prefix.with_suffix(".command.json").write_text(json.dumps({"argv":cmd,"complete_command_wall_ns":elapsed,"exit_code":p.returncode},sort_keys=True)+"\n"); print(json.dumps({"case":name,"complete_command_wall_ns":elapsed,"exit_code":p.returncode}),flush=True); failed|=p.returncode!=0
sys.exit(int(failed))' ioctl-10m ioctl-100m write-1m write-10m write-100m write-500m
```

No failure or discarded attempt occurred. The tests print one PASS per case.
At this final test source identity, `python3 core/tools/check_product_boundary.py`
PASS (290 files), `python3 -m unittest discover -s core/tools -p
'test_*.py'` PASS (9 tests), `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all -- --check` PASS, `cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked` PASS, `cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked --examples` PASS, and `cargo
+1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets
-- -D warnings` PASS on the macOS host. The host excludes Linux-only test
code. Target-specific `cargo +1.85.1 clippy --manifest-path core/Cargo.toml
--locked --target aarch64-unknown-linux-musl -p layerfs-fuse --test
range_ioctl_cpu_probe --release -- -D warnings` stopped before this test
at the existing `clippy::useless_conversion` error in
`core/crates/layerfs-workspace/src/backing/segments.rs:192`. No product
source was changed. The locked Linux release build and all six mounted runs
passed.

The frozen source/contract and this evidence-only commit each have combined
production LOC 119,981→119,981 (delta +0), Core 54,564→54,564 and reference
65,417→65,417. `python3 tools/production_loc.py --root . --json` counted
the first-parent and final staged snapshots under the same production-only
scope; tests and docs are excluded.

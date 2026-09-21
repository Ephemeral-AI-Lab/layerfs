# Throughput magnitude, CPU use and threads on the writer and transport paths

> **Status: DIAGNOSTIC. Not a gate, not a qualification, not a performance claim.**
> It answers three questions about the product as built: do we reach multi-GB
> throughput, how much CPU does the work use, and does it use more than one
> core or thread. The short answers are **no**, **about one core per
> operation/stream**, and **only where the caller supplies the threads**.

Companion to the [writer-budget diagnostic](../issue216-writer-budget-20260921T004651Z/README.md),
which reports the admission correctness and the budget's throughput plateau. This
page adds the CPU and thread accounting that plateau needed, plus the transport
path.

## 1. Rates actually measured

One sample per arm, release build, same declared cache state, same declared
interference as the companion page.

| Path | Work | Rate | CPU |
| --- | --- | ---: | ---: |
| C1 construct + C2 save, incompressible | 64 writes × 4 MiB (256 MiB) at budget 1 | **115.16 MiB/s** (0.121 GB/s) | 0.97 cores |
| C1 construct + C2 save, incompressible | same at budget 8 | **126.07 MiB/s** (0.132 GB/s) | 1.30 cores |
| C1 construct + C2 save, one large file | 1 write × 256 MiB at budget 1 | **124.23 MiB/s** (0.130 GB/s) | 0.97 cores |
| C1 construct + C2 save, repetitive content | 64 MiB, one write | **471.73 MiB/s** (0.495 GB/s) | 0.96 cores |
| Encrypted transport (AEAD), loopback | 1 GiB, 1 stream | **0.222 GB/s** | 0.97 cores |
| Encrypted transport (AEAD), loopback | 2 GiB, 2 streams | **0.441 GB/s** | 1.92 cores |
| Encrypted transport, verification pass | 1 GiB, 1 stream | 0.217 GB/s, 1 GiB verified byte-for-byte | 0.95 cores |
| C2 read-back (derived from the companion arms) | one sequential read per write plus comparison | 447–464 MiB/s | not sampled |

**No measured product path reaches multi-GB/s.** The only multi-GB row in this
repository's transport evidence is **unencrypted TCP at 1.882 GB/s** (frozen
`tdx1-tcp-upload-1-perf-v2`), and the encrypted rows there are 0.23–0.61 GB/s
against a frozen target of **2 GB/s** which they never met. Today's encrypted
rows (0.222 / 0.441 GB/s) reproduce that shape on this host. So:

- the encrypted wire path is ~0.2 GB/s per stream and scales with streams
  (0.44 GB/s at two, 1.92 cores — near-linear, one thread per stream);
- the C1+C2 write path is ~0.12 GB/s for incompressible bytes and ~0.5 GB/s for
  content that deduplicates (the stored bytes collapse to 0.3 MiB for a 64 MiB
  payload, so that row is not a like-for-like comparison);
- a single 256 MiB write runs at the same ~0.12 GB/s as the 4 MiB writes, so the
  rate is a steady-state property of the path, not per-operation overhead.

## 2. CPU use

`effective cores = (user + sys) / real`, from `/usr/bin/time -l` on each arm.

| Arm | real | user | sys | effective cores |
| --- | ---: | ---: | ---: | ---: |
| `cpu-w1` (budget 1, 256 MiB) | 2.87 s | 1.71 s | 1.07 s | **0.97** |
| `cpu-w8` (budget 8, same work) | 2.70 s | 2.12 s | 1.38 s | **1.30** |
| `one-w1-256mib` | 2.68 s | 1.70 s | 0.90 s | **0.97** |
| `repeat-64mib` | 0.23 s | 0.20 s | 0.02 s | **0.96** |
| `transport-1gib-perf` | 4.84 s | 4.55 s | 0.14 s | **0.97** |
| `transport-2gib-perf` | 4.87 s | 9.12 s | 0.24 s | **1.92** |
| `transport-1gib-verify` | 4.95 s | 4.56 s | 0.12 s | **0.95** |

Two conclusions, both measured rather than inferred:

1. **The C2 save path is effectively single-core.** Eight concurrent writers,
   each with its own thread and its own save, used **1.30 cores** — barely more
   than one. The threads exist (see §3) but they spend their time waiting on the
   per-Store arbitration and the Store's short write transactions, so the budget
   admits concurrency that the storage half then serializes. This also explains
   the budget's throughput plateau in the companion page better than the work-mix
   split alone: it is not only that the construction half is small, it is that
   even that half barely overlaps.
2. **The transport scales per stream and only per stream.** One stream saturates
   one core (0.97) at 0.222 GB/s; two streams use 1.92 cores and deliver 0.441
   GB/s. The AEAD relay is CPU-bound at roughly 4.6 ns per payload byte per
   stream on this host.

The ~0.22 GB/s per stream is a *host* observation under declared interference
(load ≈ 7 of 14 cores, desktop processes busy), not a hardware ceiling: the same
selection measured 0.26 GB/s per stream in the frozen container run on
2026-09-20.

## 3. Threads

The product creates **no** worker threads for a save or a read: one operation is
one thread with one connection and one construction producer, as the repository
rules require. Every thread counted here belongs to the caller (this harness).

| Arm | threads observed (max) | histogram over 20 ms samples |
| --- | ---: | --- |
| `threads-w1` (budget 1, 64 × 1 MiB) | **2** | 1 thread × 5 samples, 2 threads × 18 |
| `threads-w8` (budget 8, same work) | **9** | 1 × 5, 2 × 1, **9 × 16** |

So the thread count is exactly `1 + concurrent callers` while the work is in
flight, and nothing else: no pool, no second lane, no helper worker. The budget
therefore controls *how many callers may be in flight*, and those callers are the
only source of parallelism the path has.

## 4. Identity, cache state and interference

See `identity.json`: commit and tree, `core/Cargo.lock` sha256, the four release
binaries by sha256, host (macOS 26.4.1, aarch64, 14 logical cores), the CPU
accounting method, the transport topology deviation (host-to-host loopback rather
than the frozen `--cpus=1` container client), the declared cache state, and the
declared interference. Raw outputs are at
`~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-cpu-rate-run3-20260921T010400Z`
and `.../issue216-threads-run3-20260921T011100Z`.

## 5. Gaps, stated plainly

- **One sample per arm.** No n3, no best-of; the transport rows are a single
  loopback run each and the host was busy.
- **The transport topology is not the frozen one.** The client ran on this host,
  not in a `--cpus=1` container, so these rows answer "what does this host do"
  and are not a container qualification. The comparison with the frozen
  `tdx1-*` rows is a shape comparison, not a matched pair.
- **The read-back rate is derived, not measured**: it comes from the verification
  phase of the companion arms (sequential reads plus comparison), not from a
  dedicated read benchmark.
- **No idle-host rerun and no per-arm CPU accounting at budgets 2 and 4**; only 1
  and 8 were accounted.
- **The 256 MiB single-write arm is one write**, so its rate has no spread
  estimate.
- **Three runner defects are retained on disk and not used** (`results.json` →
  `discarded`): a sampler that watched the wrong pid, transport arms that used
  private keys where the probe expects peer public keys, and a command-list
  formatting crash. None of them involved the product.

## 6. Reproduction

```sh
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-service --example measure_admission
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-storage --example measure_ingest
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-daemon --example transport_probe
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-bridge --example public_key

/usr/bin/time -l core/target/release/examples/measure_admission --writers 8 --writes 64 \
  --bytes 4194304 --mode queued --commit "$(git rev-parse HEAD)" --output /tmp/cpu-w8
/usr/bin/time -l core/target/release/examples/measure_ingest --bytes 67108864 --pattern repeat --output /tmp/repeat
# transport: server prints LISTEN <port>, each client connects, the driver writes "go\n" to the server's stdin
SK=$(openssl rand -hex 32); CK=$(openssl rand -hex 32)
SPUB=$(LAYERFS_PRIVATE_KEY=$SK core/target/release/examples/public_key)
CPUB=$(LAYERFS_PRIVATE_KEY=$CK core/target/release/examples/public_key)
LAYERFS_PRIVATE_KEY=$SK LAYERFS_PEER_KEY=$CPUB core/target/release/examples/transport_probe server noise upload 1 perf
LAYERFS_PRIVATE_KEY=$CK LAYERFS_SERVER_KEY=$SPUB LAYERFS_ENDPOINT=127.0.0.1:<port> \
  /usr/bin/time -l core/target/release/examples/transport_probe client noise upload 0 perf
```

Effective cores are `(user + sys) / real` from `/usr/bin/time -l`; thread counts
come from `ps -M -p <pid>` on the arm's own pid, sampled every 20 ms.

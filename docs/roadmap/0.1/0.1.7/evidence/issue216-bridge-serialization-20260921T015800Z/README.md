# Is the bridge serialized per operation? 512 MiB as one file or as 64

> **Status: DIAGNOSTIC. Not a gate, not a qualification, not a performance claim.**
> The question was whether transporting 64 x 8 MiB is much slower than one 512 MiB
> file, i.e. whether the bridge pays a large fixed cost per operation. On this
> route the answer is **no: +6.7% for 64 operations**, and **1.86x faster** when
> those operations run concurrently.

Measured over the real authenticated route on this host: `layerfs-service` on
127.0.0.1 with a fresh Store copy per arm, and one `layerfs-daemon` process per
operation feeding the plaintext `LFB1` frames. One operation per session is the
protocol's own rule, so every operation really does open a connection, complete a
Noise handshake and take a service session.

## 1. The same 512 MiB, three shapes (sequential operations)

| Arm | operations | wall | rate | per operation | effective cores |
| --- | ---: | ---: | ---: | ---: | ---: |
| `single-1x512mib` | 1 x 512 MiB | 7.302 s | **70.12 MiB/s** | 7302 ms | 1.21 |
| `split-8x64mib` | 8 x 64 MiB | 6.944 s | **73.73 MiB/s** | 868 ms | 1.30 |
| `split-64x8mib` | 64 x 8 MiB | 7.795 s | **65.68 MiB/s** | 121.8 ms | 1.12 |
| `connect-64x0` | 64 x 0 B | 0.783 s | - | **12.24 ms** | 0.14 |

- **64 operations cost 0.493 s more than one operation: +6.75%**, i.e. **7.8 ms of
  fixed cost per extra operation**.
- **8 operations were 4.9% *faster*** than the single 512 MiB operation, so the
  "one big file is best" ordering does not even hold at that split.
- 64 empty operations cost **12.24 ms each**. Of that, **5.45 ms is this harness
  spawning a process per operation** (measured: 64 spawns of the same binary with a
  refused endpoint, 0.349 s). The remainder - roughly 2-7 ms depending on which
  estimate is used - is the product's own per-operation work: connect, handshake,
  session thread, Store open, save slot and publication.
- The two estimates of that remainder disagree (7.8 ms marginal versus
  12.24 - 5.45 = 6.8 ms direct) and the difference is **not resolved here**. Both
  are small against the ~115 ms of work one 8 MiB operation performs.

## 2. The same 512 MiB with concurrent operations

Eight operations of 64 MiB, all in flight at once, against a Store whose writer
budget is 8:

| Arm | shape | wall | rate | effective cores |
| --- | --- | ---: | ---: | ---: |
| `seq-8x64mib` | 8 operations, one at a time | 7.057 s | 72.55 MiB/s | 1.29 |
| `conc-8x64mib-budget8` | 8 operations in flight | **3.916 s** | **130.74 MiB/s** | 2.55 |

**Concurrency is worth 1.80x** on the same bytes: the daemons encrypt in their own
processes, the service decrypts and the store serializes only its short
transactions. So the route is not serialized *per operation*; what is serialized
is the per-byte work *inside* one operation, which is why one operation uses only
~1.2 cores: 70.12 MiB/s on the route, against 115.16 MiB/s for construct+save
alone and 212 MiB/s for the transport alone (both measured in the companion
pages). The wire and the store do not fully overlap within one operation.

## 3. Session capacity, and where the budget bites

The transport admits `session_capacity(budget) = budget + MAX_READ_OPERATIONS`
sessions. Measured against a budget of 2 (four session slots):

| Arm | clients | outcome | codes | wall |
| --- | ---: | --- | --- | ---: |
| `conc4-budget2` | 4 | 2 succeeded, 2 refused | `Capacity` (4) | 1.081 s |
| `conc5-budget2` | 5 | 2 succeeded, 3 refused | `Capacity` (4) | 1.292 s |

Exactly two writers were admitted - the budget - and every other client got an
explicit bounded `Capacity` refusal, with no waiting and no queue. This is the
same admission rule the in-process service tests assert, now observed through the
real transport.

**One unreproduced observation.** In an earlier arm of eight concurrent clients
against the same budget of 2, four clients were served (2 admitted, 2 refused),
one more was refused, and **one produced no response within the 180 s deadline**
while the arm's wall time is that deadline, not product time. The 4- and
5-client arms above were clean, so the mechanism is **not established**; it is
retained and reported rather than explained away, and no number is taken from
that arm's wall time.

## 4. What this does and does not say

- It says the fixed cost of an operation is small here (single-digit
  milliseconds) relative to the work of one 8-64 MiB operation, and that running
  operations concurrently pays.
- It does **not** reproduce any "much slower" penalty. If a measurement elsewhere
  shows one, the candidates this experiment can rule in or out are: a *container*
  (not process) per operation - the frozen tdx1/issue192 routes do exactly that
  with `docker run`, which costs hundreds of milliseconds per operation, and this
  harness measured only process spawn; a client-side setup cost paid per
  operation outside the product; or payloads small enough that the fixed cost
  dominates (at 64 empty operations the fixed cost is the whole measurement).
- The absolute rate is far below the transport's own 212 MiB/s on both shapes,
  because the store half is the slow half and one operation does not overlap them.

## 5. Gaps

- One sample per arm; no n3, no best-of. The 8-operation arm came out 4.9% faster
  than the single operation, which is inside the ~10% noise floor measured on this
  host, so only the 64-operation penalty is treated as signal.
- Host-to-host loopback, not the frozen `--cpus=1` container topology; the daemon
  ran as a host process, so process spawn is cheaper here than container start.
- A process per operation is the harness's structure, not the product's; its
  5.45 ms is excluded from the product estimate but included in the wall times.
- The service was started per arm, so its startup is outside every arm's wall.
- One no-response observation in an eight-client arm is unresolved (§3).

## 6. Reproduction

```sh
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml \
  -p layerfs-service --example prepare_store -p layerfs-service --bins -p layerfs-daemon --bins \
  -p layerfs-bridge --example public_key
# then, per arm: a fresh Store copy from the prepared master, layerfs-service on a
# free port with LAYERFS_PEERS="1,<client public>,<expiry>,31", and one
# layerfs-daemon per operation carrying the plaintext frames on stdin:
#   request(2, ...) + 16 KiB frames(3, ...) + EndInput frame(4, ...)
# The drivers and their exact commands are retained at the raw roots below.
```

Raw outputs: `~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-bridge-serialization-20260921T014000Z`,
`.../issue216-bridge-concurrency-run2-20260921T015000Z`,
`.../issue216-bridge-sessions-20260921T015500Z`.

# Stage 1.1T — Explicit TrustedLocalDev Materialization

Status: **implemented and measured; `PASS_PRIMARY_TRUSTED_CLASS`; not Verified**

Predecessor: [20 — Stage 1.1M Verified terminal closure](20-stage1.1-full-materialization-optimization.md)

Canonical, durability and Apple authority:
[10 — handoff freeze](10-handoff-freeze.md)

## 1. Decision

Stage 1.1M Verified remains closed as
`REVISE_NO_AUTHORIZED_OWNER`; `terminal_pass=false`. This document does not
reopen, weaken, replace or relabel that result.

Stage 1.1T is a separate developer-loop product class using the existing
`IntegrityMode::TrustedLocalDev`. Its only prospective read optimization is:

```text
fetched canonical row
  -> exact SQLite kind and length
  -> exact canonical framing and role codec
  -> bounded delivery
  -> no fetched-row ObjectId hash in TrustedLocalDev
```

Verified continues to authenticate every fetched canonical row against its
ObjectId.

## 2. Trust boundary

The following rules are mandatory:

```text
Verified is the default open mode.
TrustedLocalDev is selected explicitly at Store open.
The selected mode is immutable for that Engine lifetime.
No SQLite property, locality, ownership, environment or cache state infers trust.
No live Engine changes from TrustedLocalDev to Verified.
Explicit TrustedLocalDev open durably sets the existing trusted_history bit.
New canonical objects are authenticated before write in both modes.
Existing incumbent objects are authenticated before reuse in both modes.
Trusted publication sets trusted_history=1 in the visibility transaction.
Trusted local materialization is labeled TrustedLocalDev; it is not verified export.
Publish/export/share promotion requires closing TrustedLocalDev, reopening
  Verified, and completing one full retained-union scrub.
Any scrub failure blocks the Verified handle and therefore blocks promotion.
```

There is no new integrity enum, schema bit, registry, cache, dependency,
canonical format or trust-escalation API.

## 3. Exact product change

Owned product file:

```text
crates/layerfs-engine/src/lib.rs
```

The existing public Engine/SDK/VFS routes stay unchanged. Only mode-aware read
validation differs:

| Boundary | Verified | TrustedLocalDev |
|---|---|---|
| Single fetched object | Exact codec + ObjectId authentication | Exact codec; no ObjectId hash |
| Ordered payload batch | Exact Bytes codec + ObjectId authentication per occurrence | Exact Bytes codec; no ObjectId hash |
| New object write | authenticate | authenticate |
| Incumbent reuse | authenticate | authenticate |
| Publication closure | full Verified root verification | marks trusted history |
| Reopen for Verified use | full retained-union scrub after any explicit Trusted open | not applicable |

The existing `ObjectRead` method names are retained because changing the Core
trait would expand canonical authority. Their guarantee is interpreted through
the immutable Engine integrity mode. Trusted counters must make the weaker
route observable:

```text
Verified:        fetched = authentication passes = role-decode passes
TrustedLocalDev: fetched > 0; authentication passes = 0;
                 fetched = role-decode passes; identity-authentication wall = 0
```

## 4. Focused proof

Required tests:

```text
Verified rejects a valid canonical BLOB substituted under another ObjectId.
TrustedLocalDev may read that valid substituted BLOB and is counted/labeled weaker.
TrustedLocalDev still rejects malformed kind/length/framing/role bytes.
TrustedLocalDev new-object and incumbent-reuse writes remain authenticated.
Trusted publication cannot become Verified authority without close/reopen/scrub.
A successful Verified-after-Trusted scrub restores fetched-row authentication.
An invalid Trusted retained union blocks Verified reopen before promotion.
The real SDK/VFS/Apple Trusted materializer emits exact output with zero read hashes.
```

No benchmark result substitutes for these tests.

### 4.1 Implemented focused result

The product implementation is confined to the Engine's shared fetched-row
read helpers. Publication, retained-union verification, compaction,
reconciliation and object-write call sites remain on the always-authenticated
helper. Explicit Trusted open also durably sets the existing trusted-history
bit so a materialize-only session cannot bypass the Verified scrub boundary.

Focused closure:

```text
Engine unit + publication/fault integration     72 passed
SDK materialization attribution                  2 passed
SDK Stage 1 routes                               7 passed
evaluator unit population                       43 passed, 4 ignored
touched Engine/SDK/evaluator clippy              PASS, -D warnings
```

The evaluator's first touched run found one legacy small-route test feeding an
explicit Trusted handle into the Verified-only Stage 1.1 equation checker. The
test now opens Verified; the production Stage 1.1 checker was not weakened.
The preserved failure and focused repaired proof are test evidence, not a
performance row.

## 5. Measurement population

After focused and touched-crate closure, build one clean release evaluator and
run one source-bound campaign:

```text
integrity mode             TrustedLocalDev
sizes                      0, 24, 96 MiB
per size                   1 untimed-conditioning warmup + 3 measured rows
operation                  same-open source, fresh destination, complete durable output
fixture                    existing sealed Stage 1.1M fixtures
path                       public SDK -> VFS -> host driver -> Apple/APFS
destination                fresh and removed after exact oracle
preferred complete wall    <15 s
hard complete wall         <30 s
```

The release, commit, source manifest, SHA-256, BLAKE3, fixture identity, argv,
row population and campaign wall equation are retained. Verified M7 values are
reported beside Trusted values but are never pooled.

## 6. Gates and disposition

Correctness/resource gates are identical to Stage 1.1M:

```text
exact bytes, metadata and destination inventory
buffer <=1 MiB
Q high-water <8 MiB; terminal 0
RSS <32 MiB
primary/scratch/total connections <=1/1/2; terminal 0
FD closure and zero owned residue
no worker, async runtime, retry, WAL, pool, watcher, mount or network
```

Comparative targets:

```text
24/96 MiB p50 throughput >=450 MiB/s     primary comparison
fitted sustained bandwidth >=500 MiB/s  stretch comparison
fitted fixed cost <20 ms                 report against Verified gate
```

Every miss remains a miss. The measured Verified identity-authentication owner
is an upper bound, not a promised Trusted saving.

## 7. Stage 1.2 handoff rule

Only a material source-bound Trusted gain authorizes a Stage 1.2 handoff. The
handoff may use explicit TrustedLocalDev for repeated local developer-loop
operations, but capture/publish/export/share promotion must use:

```text
close TrustedLocalDev
-> reopen Verified
-> full retained-union scrub when trusted_history=1
-> only then publish/export/share
```

Stage 1.2 execution is outside this document unless separately assigned.

## 8. Executed result

The compact durable evidence is
[Stage 1.1T TrustedLocalDev result](evidence/stage1.1t-trusted-20260826/summary.md).

```text
source commit          5e58fa6f0f35451ab49c48de56a144fca8276cc5
release SHA-256        ec015b50d22b7d93b9028155f4ac761bc44b5d239cfaec69aff2c93d179fa4a7
release BLAKE3         5fced67500a0ced8e1d77af9958989ba3b60a71d015367d89f9410dea1558716
rows SHA-256           059b1bf4ac091f9f5ff5444203d8c69ec6827d3d27b5a002169d3e85b0c15082
population             3 warmups + 9 measured rows
campaign wall          2.043962958 s
```

| Size | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary gate |
|---:|---:|---:|---:|---:|---|
| 0 MiB | `26.626833 ms` | `36.185625 ms` | N/A | N/A | report |
| 24 MiB | `37.595417 ms` | `40.417625 ms` | `638.376` | `593.800` | PASS |
| 96 MiB | `81.700958 ms` | `89.090125 ms` | `1175.017` | `1077.561` | PASS |

Every row was explicitly labeled `TrustedLocalDev`. The raw equation is
`26,016 fetched rows = 26,016 role decodes`, with zero fetched-row identity
authentication passes and zero identity-authentication nanoseconds. RSS, Q,
connections, FD and residue gates all pass.

The diagnostic 24-to-96 slope is `612,576.958 ns/MiB` (`1632.448 MiB/s`) and
the fitted intercept is `22.893570 ms`. The model is invalid because measured
zero differs from that intercept by `3.733263 ms`; fixed cost also remains a
`2.893570 ms` miss. Those misses are preserved and were not rerun.

Relative to the frozen Verified population, Trusted saved
`24.596042/25.563875 ms` at 24 MiB p50/p95 and
`97.636542/94.788208 ms` at 96 MiB. The result is a material source-bound gain
and admits only the narrow Stage 1.2 trust handoff in section 7.

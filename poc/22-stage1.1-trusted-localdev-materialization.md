# Stage 1.1T — Explicit TrustedLocalDev Materialization

Status: **implemented, corrected, and independently audited;
`PASS_PRIMARY_TRUSTED_CLASS`; fixed-cost miss retained; not Verified**

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
Verified:                  fetched = authentication passes = role-decode passes
TrustedLocalDev read-only: fetched > 0; authentication passes = 0;
                           fetched = role-decode passes;
                           identity-authentication wall = 0
TrustedLocalDev write:     new and incumbent object authentication remains exact
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

The terminal audit later exercised the real mixed Stage 1.1 route and made its
counter contract mode-aware. Trusted read-only phases require zero fetched-row
identity hashing; Trusted publication phases may authenticate fetched rows
needed by a write, and still require exact new/incumbent write authentication;
fresh Verified phases require fetched/authentication/role-decode equality.
The focused Apple route now covers this split directly.

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
-> full retained-union scrub (explicit Trusted open set trusted_history=1)
-> only then publish/export/share
```

Stage 1.2 execution is outside this document unless separately assigned.

## 8. Historical attempt-002 result — superseded for timer/custody

The numbers in this section are preserved product-operation diagnostics.
Attempt-002 is not the terminal artifact: all 12 rows aliased row wall to
product-operation wall and violated the controlling row timer equation, and
its manifest did not bind an observed clean build log. No raw byte was edited;
attempt-003 in section 9 supersedes only those timer/custody claims.

The compact durable evidence is
[preserved Stage 1.1T attempt-002 result](evidence/stage1.1t-trusted-20260826-attempt-002/summary.md).

Attempt-001 remains preserved for commit `5e58fa6`, but it is superseded as a
trust-boundary result: materialize-only Trusted use did not yet mark history.
Attempt-002 measures the corrected `dfa6020` product.

```text
source commit          dfa60200084962cc3ac16c8518655ed85e62eb7f
release SHA-256        dc500fc862c76ec5de5e6d99c391e047d0da59fb78b5b50d859e3914033f295d
release BLAKE3         73eb52dd54101d18d4113f4da9b7e93b910e3f9203d2f80ebedece9ac5ca3a92
rows SHA-256           487a93dfd279f0cbdcbb04264686296f34a2aabe1de35d81bafa4b3f6a91f0a6
population             3 warmups + 9 measured rows
campaign wall          2.100807792 s
```

| Size | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary gate |
|---:|---:|---:|---:|---:|---|
| 0 MiB | `22.961833 ms` | `33.035708 ms` | N/A | N/A | report |
| 24 MiB | `43.504833 ms` | `45.408083 ms` | `551.663` | `528.540` | PASS |
| 96 MiB | `89.210208 ms` | `102.889500 ms` | `1076.110` | `933.040` | PASS |

Every row was explicitly labeled `TrustedLocalDev`. The raw equation is
`26,016 fetched rows = 26,016 role decodes`, with zero fetched-row identity
authentication passes and zero identity-authentication nanoseconds. RSS, Q,
connections, FD and residue gates all pass.

The diagnostic 24-to-96 slope is `634,796.875 ns/MiB` (`1575.307 MiB/s`) and
the fitted intercept is `28.269708 ms`. The model is invalid because measured
zero differs from that intercept by `5.307875 ms`; fixed cost also remains an
`8.269708 ms` miss. Those misses are preserved and were not rerun.

Relative to the frozen Verified population, Trusted saved
`18.686626/20.573417 ms` at 24 MiB p50/p95 and
`90.127292/80.988833 ms` at 96 MiB. The result is a material source-bound gain
and admits only the narrow Stage 1.2 trust handoff in section 7.

## 9. Corrected and audited attempt-003

The compact portable evidence is
[attempt-003 terminal audit](evidence/stage1.1t-trusted-20260826-attempt-003/summary.md).
It binds clean commit `3635dfc`, the exact successful build command/log, the
running executable, and a byte-identical source manifest. The release SHA-256
is `97e4fb265af9142e63da762ed1721f0df8440cdeab2f447d8de8301e8aeddf26`;
the BLAKE3 is
`8845ad9962f1a6413dbde43909f26f1c943e3b3890edd55bfab4d3cf4850c66c`.

Exactly 3 warmups and 9 measured rows were run. All 12 are v2
`TrustedLocalDev` rows and independently satisfy both timer equations.

| Size | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary gate |
|---:|---:|---:|---:|---:|---|
| 0 MiB | `22.912958 ms` | `26.752542 ms` | N/A | N/A | report |
| 24 MiB | `38.150500 ms` | `41.080458 ms` | `629.087` | `584.219` | PASS |
| 96 MiB | `84.157708 ms` | `96.167333 ms` | `1140.715` | `998.260` | PASS |

The fitted model is valid: intercept `22.814764 ms`, slope `638,989 ns/MiB`,
sustained bandwidth `1564.972 MiB/s`, and zero residual `0.098194 ms`. The
fixed `<20 ms` target still misses by `2.814764 ms`; it is not relabeled as a
PASS.

Against the separately frozen Verified M7 population, attempt-003 saves
`24.040959/24.901042 ms` at 24 MiB p50/p95 and
`95.179792/87.711000 ms` at 96 MiB. The zero-byte p50 improves by
`1.158375 ms`, while zero-byte p95 is `2.104292 ms` slower. The populations
remain separate and are never pooled.

Raw trust/resource closure:

```text
fetched / role decode / fetched auth    26,016 / 26,016 / 0
identity-authentication wall            0 ns
RSS peak                                15,564,800 B
maximum row CPU                         108,275,000 ns
Q high / terminal                       8,388,607 / 0 B
scratch / total connections high        1 / 2
connections terminal                    0
FD maximum / block terminal             13 / 4
residue / network                       0 / 0
campaign wall                           2.106328041 s
rows SHA-256                            4c476039cb8cc7881a3103e9d832c5032387b120cf91775e8625475305b00cea
```

Later commits through `36d05d8` change only the Apple-edge evaluator; they do
not change product crates or `stage1_materialize.rs`, so attempt-003 remains
the source-bound Trusted product-performance population. The final
current-source attempt-020 separately passes 47/51/34 and proves Trusted
read-only omission, authenticated writes, and seven Verified-after-Trusted
scrubs. Stage 1.2 and Docker/FUSE were not started or resequenced by this
audit.

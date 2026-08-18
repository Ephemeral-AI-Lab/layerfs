# WP4-M M4.5-5 — retained 100-MiB C0/C1 causal release comparison

- Verdict: **PASS for the private same-count changed-spine mechanism**.
- Primary metric: same-middle durable edit latency, not 100-MiB throughput.
- Qualification: **false**.
- Promotion: **false**.
- Rejection: **false**.
- Decision: retain the candidate implementation and advance only to the
  terminal independent-audit checkpoint.
- Scope: retained 100-MiB fixture, K64/F64, same-middle, candidate benchmark
  `Store` only.  No 512-MiB row, 198-row profile campaign, `+1`, M5 product
  work, full-create measurement, or production `Engine` claim was run.

## Frozen arms and causal design

The release executable was built exactly once after the M4.5-0 through
M4.5-4 correctness/accounting gates passed:

```text
cargo build --release -p layerfs-engine \
  --bin phase4_create_edit_benchmark
  -> PASS in 6.90s
```

The three arms are:

```text
A0 = frozen historical M3 executable
C0 = corrected M4.5 substrate + ordinary full pre-COMMIT closure
C1 = byte-identical C0 substrate + changed-spine qualification
```

C0 and C1 are the exact same `f0ba1c24…` executable.  The only variable is
`WP4M_M45_QUALIFICATION_MODE=full-closure|changed-spine`.  Authority,
complete-head publication, oracle, reconciliation, errors, Q, SQL/W/D,
post-COMMIT verification, and external observation paths are byte-identical.

The protocol separately records:

1. C0/C0 A/A noise calibration;
2. A0/C0 substrate/correctness tax;
3. C0/C1 causal algorithm effect; and
4. A0/C1 cumulative continuity.

Each comparison received one isolated warmup per arm and five balanced AB/BA
measured pairs.  The official five C0/C1 rows triggered the predeclared RSS
procedure, so exactly 15 additional balanced C0/C1 pairs were appended,
yielding 20 memory-adjudication pairs.  The original five were not replaced,
pooled away, or normalized.

Every base was regenerated in an untimed preparation child before its timed
row.  Current C0/C1 preparations used the same separately constructed
full-rebuild oracle and reproduced expectation SHA-256 `81b2eaf5…` in every
row.  A0 retained its historical expectation format/hash `cf9f99e3…`.
Per-arm database and authority hashes differ because their private authority
tuples are freshly generated, but all logical source/base/edit/result
identities are exact and are recorded before timing.  Preparation never opens
the measured image during the timed child.

## Fingerprints and raw artifacts

| Item | SHA-256 / value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| cumulative tracked implementation diff | `640285dde7f3f5a84c0cb16a589b63020e5cb354acb0df3a7b3c257c018b44e0` |
| benchmark source | `976cba60408bc00e939f063add8bf427fc66857477158d3aea4eae2235eafe18` |
| frozen A0 executable | `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c` |
| frozen C0/C1 executable | `f0ba1c2423161cc2f79a0e7378408141eecfed30d4e65aceab3c8c667e5570af` |
| retained source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| raw JSONL, 78 rows | `f6f1e698b7e50272cb993897c6ecff0c53fa4ba6bbd72c742f99de513f6e6165` |
| raw macOS `/usr/bin/time -l` observations | `673bb4e5da4dc8b955744b15c696136350d4e1345c2254c47e328c09c387ef49` |
| exact commands, 156 lines | `13b30deda2063f04df2a6fc5683afa5f61da5aebafe1fee4f0e76633be4c0a3a` |
| per-row prepared-base preflight, 78 data rows | `6168b8be546b25a504321340bafd6e0b9659aa79bd5c55da9b6ef2ab069634a3` |
| structural summary JSON | `8ff8ad020e348904c3c89a539b1c299dfa6b718e87860ce9ecd8f0d14a84cce3` |

Artifacts are under
`target/wp4m-m45-k64-20260818/`.  The raw JSONL contains eight warmup rows,
40 initial measured rows, and 30 predeclared RSS-extension rows.  All 78 rows
are preserved verbatim with comparison/arm/pair/order labels.  The preflight
file contains executable, DB, authority, expectation, logical-size, and
allocated-size evidence for every timed child.

## Identity, correctness, atomicity, and timer hard gates

All 78 rows are `PASS`; no measured or warmup row failed.  Every arm produced
the same exact result:

```text
source fingerprint  bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
CDC references      5,284
edited CDC digest   58b61bbd4f319ecb6011278ca42caf2b5d696e42b4655c054c48b3906d017b83
root                cc8f31adc20eaa56b621744fe45f90f65fb9ac6177446d33b0052d7ebd404560
transition          2686d6ffc512b38f64922073dcc191a1ff1c7eacedb1c73e0a72045bf7cf4a92
closure             7b7142f5e203ae23efd46662efe576a182f8043c4323f487407bbb031b7cc2bb
transactions        1
COMMITs             1
```

For every current C0/C1 row:

```text
q_high_water = 48,133 bytes
q_current    = 0
new canonical writes = 26,249 bytes
canonical rewrites   = 7,382 bytes
SQL execute calls    = 12
SQL rows changed     = 8
row BLOB writes      = 11
post logical store   = 109,297,696 bytes
```

Both disjoint timer equations passed in every current row:

```text
durable edit
  = mapping/CAS + pre-COMMIT qualification + COMMIT

post-authority lifecycle
  = durable edit + reopen + scrub + reconstruction + ranges
```

Initial same-open authority establishment is linear, separately timed, and
not included in durable edit latency.  Its official-five median was 239.172 ms
for C0 and 237.256 ms for C1 (−0.801%, 4/5 C1 wins), confirming the shared
substrate is not the changed-spine speed attribution.

## C0 A/A noise calibration

| Metric | C0a | C0b | Paired result |
|---|---:|---:|---|
| durable edit median | 438.358 ms | 436.975 ms | −0.316% arm-median delta |
| paired percentage deltas | — | — | −0.281%, −0.883%, +1.585%, −3.886%, −0.011% |
| median absolute paired delta | — | — | 0.883% |
| C0b wins | — | — | 4/5 |

The causal 99% effect is far outside this calibrated noise.  The A/A rows are
not combined with causal rows.

## Official five-pair C0/C1 causal result

Values below are `median / min / max / spread`, all in nanoseconds.

| Phase | C0 | C1 | Median delta | C1 wins |
|---|---:|---:|---:|---:|
| same-open authority | 239171875 / 234531000 / 240620958 / 6089958 | 237255708 / 232127375 / 247877208 / 15749833 | −0.801% | 4/5 |
| mapping/CAS edit | 863417 / 714666 / 998167 / 283501 | 820417 / 669083 / 1124666 / 455583 | −4.980% | 2/5 |
| pre-COMMIT qualification | 428828500 / 424535334 / 440009625 / 15474291 | 150250 / 142334 / 291084 / 148750 | −99.965% | 5/5 |
| SQLite COMMIT | 2332084 / 1351833 / 2416541 / 1064708 | 1445083 / 1106125 / 2389542 / 1283417 | −38.035% | 3/5 |
| **durable edit latency** | **431489750 / 427670000 / 443314666 / 15644666** | **2436875 / 2256833 / 3470917 / 1214084** | **−99.435%** | **5/5** |
| fresh reopen | 938750 / 854083 / 1084209 / 230126 | 926709 / 782666 / 1236250 / 453584 | −1.283% | 4/5 |
| fresh full scrub | 270556500 / 263529209 / 298295083 / 34765874 | 273928541 / 264238542 / 280821917 / 16583375 | +1.246% | 2/5 |
| reconstruction | 423123959 / 422637875 / 437172125 / 14534250 | 435838500 / 421614875 / 440907334 / 19292459 | +3.005% | 2/5 |
| ranges | 682500 / 656583 / 784250 / 127667 | 666750 / 652833 / 775583 / 122750 | −2.308% | 2/5 |
| post-authority lifecycle | 1125994125 / 1115404208 / 1180554958 / 65150750 | 717957583 / 689739708 / 720814458 / 31074750 | −36.238% | 5/5 |

The five paired durable-edit deltas were:

```text
-437,059,000; -428,921,667; -429,137,500; -440,792,791; -425,233,125 ns
-99.212%; -99.477%; -99.455%; -99.431%; -99.430%
```

This passes the predeclared material speed gate (`>=5%`, `>=4/5`, above A/A
noise).  It is an edit-latency result.  Dividing 100 MiB by these values and
calling the result throughput would be invalid and is not done.

## Work and counter causality

The official current rows are deterministic by arm:

| Counter | C0 full closure | C1 changed spine | Classification |
|---|---:|---:|---|
| canonical new-write bytes | 26,249 | 26,249 | exact invariant |
| canonical rewrite bytes | 7,382 | 7,382 | exact invariant |
| historical W | 26,249 | 26,249 | exact; equality to new-write is workload-specific, not a redefinition |
| historical D | 421,349,913 | 316,091,595 | exact cumulative fetched/authenticated work |
| canonical authenticated-nonnew | 421,349,913 | 316,091,595 | exact; equality to D is workload-specific |
| canonical authenticated total | 421,376,162 | 316,117,844 | `new + authenticated-nonnew` |
| cache acquisitions | 16,261 | 10,899 | −5,362 |
| query calls | 16,347 | 10,985 | −5,362 |
| execute calls | 12 | 12 | invariant |
| rows returned | 21,548 | 16,186 | −5,362 |
| rows changed | 8 | 8 | invariant |
| row BLOB reads | 21,574 | 16,212 | −5,362 |
| row BLOB writes | 11 | 11 | invariant |
| covered equal edges | 0 | 127 | causal mechanism |
| new/different edges | 0 | 4 | causal mechanism |
| fully authenticated new objects | 0 | 1 | causal mechanism |
| fully authenticated new bytes | 0 | 18,867 | causal mechanism |

C0 intentionally does not emit incremental-edge counters; it performs the
ordinary full-closure oracle.  C1's four different edges remain namespace,
root branch, branch leaf, and new chunk, with the other 127 authenticated
edges covered.  Native SQLite prepare remains `Unavailable`; cache
acquisition is not relabeled as prepare.  Incremental BLOB opens/reads/writes
are observed zeros because this path made no incremental-BLOB API call.

## Substrate tax and cumulative continuity

| Comparison | A median edit | B median edit | Delta | B wins | Classification |
|---|---:|---:|---:|---:|---|
| A0 historical M3 → C0 corrected substrate | 441.740 ms | 439.255 ms | −0.562% | 3/5 | wall inconclusive / within A/A noise |
| A0 historical M3 → C1 cumulative | 440.879 ms | 2.738 ms | −99.379% | 5/5 | continuity only; not causal attribution |

The C0 substrate did not materially change edit wall time.  It did increase
whole-child median CPU from 1.150 s to 1.400 s (+21.739%, 0/5 C0 CPU wins),
because the corrected transaction-owned initial authority scrub and added
correctness/accounting work are present only in C0.  This is reported as the
substrate/correctness CPU tax; it is not attributed to changed spine and is
not averaged into the C0/C1 causal result.

## Protected resources

### CPU and exact logical Q

Official-five median CPU fell from 1.360 s C0 to 0.940 s C1 (−30.882%), with
5/5 C1 wins.  Exact Q was 48,133 bytes in every current row for both arms,
and every row ended at `q_current=0`.  Both pass.

### RSS and peak-footprint adjudication

The frozen official five arm medians triggered extension:

```text
RSS:  C0 17,661,952 -> C1 18,759,680 bytes (+6.215%)
peak: C0 11,829,632 -> C1 12,943,768 bytes (+9.418%)
```

The pair directions/ranges were mixed, so the prescribed 15-pair extension
ran.  Across all 20 pairs:

| Resource | C0 median/range | C1 median/range | Paired median | Mean diagnostic | >5% regressions |
|---|---:|---:|---:|---:|---:|
| RSS | 17,727,488 / 16,302,080–18,923,520 | 18,055,168 / 16,220,160–19,120,128 | +131,072 bytes / +0.699% | +165,478.4 bytes / +1.204% | 5/20 |
| peak footprint | 11,862,400 / 10,486,144–13,107,584 | 12,239,244 / 10,404,248–13,320,600 | +163,840 bytes / +1.271% | +183,506.8 bytes / +2.159% | 6/20 |

C1 was higher in 13/20 pairs for each measure, but repeatable regression
requires a paired median above 5% and at least 16/20 pairs above 5%.
Neither condition holds.  Protected external memory therefore passes the
predeclared no-repeatable-regression rule; the noisy original five remain
preserved.

### Storage and physical observations

Logical and apparent post-store sizes were exactly 109,297,696 bytes in every
current row.  Median allocated growth was 16,777,216 bytes for both arms;
19/20 paired deltas were identical and one C1 row improved by 16,777,216
bytes.  C1 never increased endpoint storage.  The authority sidecar remained
32 bytes and journal endpoint length was observed as zero.

macOS process block-input and block-output operation counters were directly
observed as zero for these rows.  Instructions, cycles, maximum RSS, and peak
footprint were observed.  Byte-level physical I/O, SQLite/filesystem cache,
sync/fsync call counts, and peak temp/journal high-water are **Unavailable**;
no zero is substituted for them.

## Algorithm and memory interpretation

The measured and declared bounds remain:

```text
same-count mutation          O(Xb + Xc + K + F*H)
C1 qualification             O(K + F*H + A_delta + V_delta + H^2)
resident candidate memory    O(H + K + bounded pages/chunks/SQL/output)
C0 / first authority / scrub linear in complete closure
fresh reconstruction         linear in source plus closure
+1                           suffix-linear and NotRun
```

The H² term is the bounded ancestry-cycle scan.  Initial authority remains a
separate linear 237–239 ms median in this fixture.  Fresh scrub and
reconstruction remain linear and dominate post-COMMIT latency.  The result
does not imply a full-create gain, a logarithmic complete lifecycle, profile
selection, 100-GiB throughput, or production-engine integration.

## Defects and retain/revise/revert decision

The original rejected M4 mixed authority, oracle, reconciliation, Q, SQL, and
changed-spine changes into one A/B attribution.  M4.5's C0/C1 experiment
isolates the changed-spine qualifier and shows that the large latency effect
is causal, while A0/C0 separately exposes the corrected-substrate CPU cost.
The original five-row RSS alarm does not reproduce as a repeatable >5%
paired regression under the predeclared 20-pair procedure.

Decision: **retain** the private M4.5 candidate and classify its same-count
changed-spine mechanism as PASS.  This is not qualification or promotion.
Advance only to M4.5-6 final validation/reporting, then stop for independent
read-only audit.

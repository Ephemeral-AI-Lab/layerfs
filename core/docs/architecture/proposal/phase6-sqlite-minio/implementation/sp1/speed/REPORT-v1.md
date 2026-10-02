# SP1 component speed screen v1 — 2026-10-02

Nine unique operations were observed once; all nine separate persisted-output
proofs passed. Every complete sample command is <=15s and every proof is <10s.
All numeric speed claims remain **INELIGIBLE**, admission_eligible=false.
This is the experimental strict component API on the host, independently of S2;
public SDK/Exec/FUSE, release admission and physical resource proofs are NOT_RUN.

The prospective [SPEC-v1.md](SPEC-v1.md) was published at
1ad6715d10ad070cdf6ceefdf8b97cf2e6483347 before implementation/collection.
The measured clean source is c838d8d6db85158c6ed9576dd7e8fddf65489704, tree
cd3c5ec0dd46ef3f59413dc48a49ebe4bdcc6be3. Schema3 fingerprint
0b7c20d7394314f40d370899dff62fe9afccab2d51351d24d92096a33677fb40;
wire P6SP1V2. Locked Cargo1.85.1 release executable SHA256
97e296deab3007e64a0d85551a43dd3cd6209fc4a975893f17386f9683f87cc0.
Compilation seal6968743777cdcf9c665b53757caff004f2307fdf5c970a3e964d101dc6d39493;
harness seal002362ed2810cfee80ee90a6853c32610f49a4720d04b77649c496243ea250aa;
dependency seal59446b7e3fd3d0646248fb361e85a45759191dd11a5fb0ef219b2b57b0c7924a.
Full identities and operands are in [summary.json](evidence/screen-v1/summary.json)
and the archived exact receipts below.

## Observations and separate proofs

IDs have prefix sp1-speed-v1-. Limits are diagnostic alerts, not release targets:
metadata20ms, small/write250ms, reads/hydration100ms, transfer250ms. Complete
sample command limit15,000,000,000ns; proof limit9,999,999,999ns. Milliseconds
are operation_ns /1,000,000. Commands include independent clone, provider copy,
preconditioning, driver, local close/cleanup and receipt; global provider startup
and final teardown are separate. There is one sample per row, no spread/median.

| Case suffix | Operation ns | Operation ms | Complete command ns | Separate proof ns | Proof | Diagnostic alert | Numeric speed |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `metadata-local` | 2,237,750 | 2.237750 | 34,324,917 | 67,701,209 | PASS | UNDER_ALERT | INELIGIBLE |
| `metadata-native` | 11,577,959 | 11.577959 | 39,363,500 | 61,917,917 | PASS | UNDER_ALERT | INELIGIBLE |
| `small-new-full` | 13,650,083 | 13.650083 | 43,528,958 | 69,583,333 | PASS | UNDER_ALERT | INELIGIBLE |
| `small-exact-reuse` | 12,961,834 | 12.961834 | 45,196,958 | 69,652,333 | PASS | UNDER_ALERT | INELIGIBLE |
| `payload-prefix-write` | 18,307,042 | 18.307042 | 51,939,542 | 75,681,125 | PASS | UNDER_ALERT | INELIGIBLE |
| `payload-full-read` | 9,135,875 | 9.135875 | 42,713,416 | 71,085,167 | PASS | UNDER_ALERT | INELIGIBLE |
| `payload-prefix-read` | 11,744,959 | 11.744959 | 46,571,416 | 75,689,750 | PASS | UNDER_ALERT | INELIGIBLE |
| `candidate-hydration` | 16,690,875 | 16.690875 | 51,276,958 | 65,979,250 | PASS | UNDER_ALERT | INELIGIBLE |
| `metadata-transfer` | 1,885,250 | 1.885250 | 30,628,709 | 61,162,792 | PASS | UNDER_ALERT | INELIGIBLE |

Every operation returned its normal ACK and every sample process exited, closing
owned SQL/native threads/connections. Bucket data remained owned for separate
proof. The provider stop/remove later passed in270,818,625ns; bind data is retained,
so this is lifecycle cleanup, with no DELETE/GC/physical reclamation claim.
Global provider startup wall is UNAVAILABLE (not instrumented); the pinned
ready-HTTP200 identity is retained separately. No complete campaign wall is
invented from sample commands. Build/preparation are recorded outside operation.

## Four investigations

1. **Metadata route.** Both rows reconstruct the same sealed3284B pool0 canonical
and40 values, using two SQL metadata packs totaling7483B and no MinIO calls.
The native route connects once (1,335,584ns) and makes70 RPCs: capture1,
descriptor8, locations8, HasUse8, body-page2, GroupFor41 and BodyDomain2. GroupFor
request attribution is4,886,919ns; it is nested in the11,577,959ns operation,
not an additional phase. Local operation is2,237,750ns. These are non-paired
route diagnostics; no product speedup or eligible latency ratio is claimed.
A bounded batched group lookup or request-scoped reuse merits source investigation,
with all current visibility/domain/identity checks retained.
2. **Small save and exact reuse.** Both canonical arguments are96023B. New FULL
has Writer init3,280,042ns, offer1,583,333ns and finish8,786,083ns; it issues one
96097B PUT and no GET. Exact reuse has init2,816,041ns, offer9,664,625ns and
finish480,792ns, makes31 native calls, performs one96097B GET and no PUT, and
creates no object/pack. Reuse avoids upload but currently pays validation of the
existing payload. Any optimization must preserve that validation, not warm/pre-read
it outside the operation. Nested spans are not added to another headline.
3. **FULL/PREFIX writer and reader.** The FULL write row is shared with view2;
it was never measured again. PREFIX write selected exactly one trial, retained the
base, read96097B and uploaded190B in one GET/one PUT; total18,307,042ns with
init2,664,625ns, offer10,934,791ns and finish4,706,792ns. FULL read makes one
96097B GET in9,135,875ns. PREFIX read makes two GETs totaling96287B, follows one
base edge at depth1 and reconstructs the original96023B canonical in11,744,959ns.
The190B envelope demonstrates stored/uploaded bytes, not reduced base-read work.
The exact original canonical and full logical payload were verified separately.
FULL decoder attribution counters do not count all returned canonical bytes;
proof byte totals are separate and a zero decoder counter is not a zero-byte read.
4. **SQL hydration and metadata transfer.** Hydration walks8192 declared synthetic
eligible rows in65 pages, one snapshot build/8192 slot lookups. It records631327
VM steps,57865 page VM steps, zero page fullscan/sorts, TEMP182x4096B, with
16,690,875ns operation. This checks indexed bounded paging; it does not establish
normal candidate population/quality. Metadata transfer registers a legal Nativev15
5752B envelope containing32 copies of a sealed51B group: only one8192B transfer
page. It takes1,885,250ns, one connection and three actions(begin/append/finish),
no MinIO calls. It ends at ready-body ACK with save status0/body-ready1; local save
allocation was declared setup. It is small-body fixed cost, with no multi-page,
save-finalization, C5 or filesystem semantic claim.

## Cache, resources and acquisition

Five masters were acquired once: empty, metadata, FULL, PREFIX and synthetic ring.
Preparation wall ns respectively16,942,584;3,722,456,250;27,164,750;35,436,000;
334,071,125. Closed masters and exact original fixtures were identity-checked;
every sample used setup=clone/closed-independent-byte-copy, fresh writable SQL
and fresh provider bucket; no APFS clone/hard link/mutated sample was reused.
Original producer and oracle hashes, copied pack bytes, source hashes, dependency
reuse and exact master paths are retained in raw receipts. Canonical API arguments
were prepared in memory before timers as declared; no disk/CDC workload is claimed.

The existing host residency instrument self-check passed and invalidated every
complete SQL clone to zero resident pages before launch. Launch gaps75,666–97,167ns.
Catalog schema/header/custody opening happens outside the inner timer; SQLite
pager state and MinIO process/VM/volume/device caches remain UNVERIFIED. Therefore
zero host residency cannot make any of these numbers eligible. No cold/warm pool,
unchanged-arm retry or expected-range priming occurred. No owned build overlapped
a timed phase. External-owner interference is UNAVAILABLE, not asserted absent.
Construction workers1; the native service thread performs RPC/SQL, not construction.

Host user/system CPU deltas are recorded per row (producer and native service in
one process). RSS, reset cgroup phase peak, device reads and total provider allocation
are UNAVAILABLE; the512MiB/1CPU provider limits are quotas, not measurements.

The first release build timed out at30,012,927,292ns while compiling dependencies:
all9 rows in r001 remain NOT_RUN with sample_count0. The original FAIL receipt and
logs were retained. A second incremental build in the same owned target completed
in7,208,789,708ns with the same30s limit; r002 then collected each operation once.
There was no repeated performance arm and no relaxed timeout/workload/worker/cache
rule. No additional builds or samples are needed for this screen.

## Custody and reproduction

- [Initial build miss / nine NOT_RUN rows](evidence/screen-v1/sp1-speed-r001.tar.gz)
- [Nine exact performance receipts, clones, commands and logs](evidence/screen-v1/sp1-speed-r002.tar.gz)
- [Nine separately run exact-identity proofs](evidence/screen-v1/sp1-speed-proof-r001.tar.gz)
- [Archive SHA256/byte manifest](evidence/screen-v1/manifest.json)
- [Pinned public provider identity](evidence/screen-v1/provider.json) and
  [separate teardown](evidence/screen-v1/provider-teardown.json)
- [Source checks](evidence/screen-v1/source-checks.json), [catalog21pass/1ignored](evidence/screen-v1/catalog-test.log),
  [family11pass](evidence/screen-v1/harness-test.log), [exact source LOC](evidence/screen-v1/source-loc.json).

Exact public commands from the owned worktree, with private configuration only in
the environment (not argv/receipts):

```sh
python3 core/benchmark/fs-bench-pro/runner.py run --family sp1-components --out benchmark-results/fs-bench-pro/sp1-speed-r001
python3 core/benchmark/fs-bench-pro/runner.py run --family sp1-components --out benchmark-results/fs-bench-pro/sp1-speed-r002
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/sp1-speed-r002 --out benchmark-results/fs-bench-pro/sp1-speed-proof-r001
python3 core/benchmark/fs-bench-pro/runner.py verify --run benchmark-results/fs-bench-pro/sp1-speed-r002
python3 core/benchmark/fs-bench-pro/runner.py verify --run benchmark-results/fs-bench-pro/sp1-speed-proof-r001
```

SP1_EVIDENCE_DIR is the existing sp1/evidence directory; SP1_MINIO_IDENTITY_JSON
points to the sanitized pinned ready-provider JSON; SP1_MINIO_IMAGE_DIGEST is
cgr.dev/chainguard/minio@sha256:4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285.
Private SP1_MINIO_AUTHORITY/ACCESS/SECRET and LAYERFS_CONSTRUCTION_WORKERS=1
are supplied in the process environment. New provider instances need matching new
prepared custody identities and fresh outputs; the above receipts must not be
replayed/promoted. Provider buckets now reside in retained owned bind data.

Source checkpoint c838d8d6d: Production LOC135991 ->135991(delta+0), Reference
65417 unchanged/Core70574 unchanged. Same first-parent/staged counter and scope
as prior checkpoints; strict experimental runtime5955 ->6009(+54) separately,
modified support652 unchanged. Spec checkpoint1ad6715d1 is docs-only with the same
product and research totals unchanged. Evidence checkpoint reports its own exact
parent/staged LOC in the commit and implementation ledger.

The next speed work should examine native request counts from these receipts,
then define a count-driven diagnostic for any proposed batching/reuse change.
A new prospective legal multi-page metadata fixture is required to study transfer
scaling. Public end-to-end speed verification waits for a sealed S2 integration;
physical resource and cache proofs remain separate open work. No current row
requires an unchanged-arm rerun or larger timeout.

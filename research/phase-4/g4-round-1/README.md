# Phase 4 G4 Round-1 research decision package

Disposition: **ROUND 1 COMPLETE / G4 PREREGISTRATION READY / G4 UNSTARTED**

This package jointly researches authenticated logical reconstruction and native
materialization from the full CAS + CDC + COW + Canonical-v2 + SQLite + native
filesystem + future VFS/projection perspective. It contains no production
candidate, G4 acceptance result, G5 work, production/VFS/SDK integration, Git
commit, or change to sealed G3 evidence.

## Outcome

The accepted design direction is deliberately small for G4:

1. freeze an exact current logical reconstruction control;
2. measure the unchanged G3 complete fallback first as immutable
   `M0-control`, then add a bounded writer sink to the existing **batched,
   proof-preserving**
   authenticated traversal and publish its private native temp through the
   existing G3 old-or-new durability protocol;
3. qualify G3-v13's same-open protected-seed clone/patch/fallback unchanged,
   while scoring full seed reads separately from clones; and
4. separately prove and A/B whether the closure commitment is derivable from
   the authoritative Canonical-v2 root and per-object authentication.

The top disruptive later architecture is Canonical-v2 durable truth plus a
capacity-bounded content-root native seed cache under a stronger protection
domain. It is not ready for G4: no current primitive preserves exact seed-byte
authority across a true broker restart without full reauthentication/rebuild.

Detailed decision: [final synthesis](decision/final-synthesis.md).
Full ranking and kill rules: [candidate matrix](decision/candidate-matrix.md).

## Package tree

```text
research/phase-4/g4-round-1/
├── README.md
├── reconstruction/report.md
├── materialization/report.md
├── core-architecture/report.md
├── experiments/ledger.md
├── benchmark-contract/proposed-g4-contract.md
├── decision/candidate-matrix.md
├── decision/final-synthesis.md
├── roadmap/post-g4-dependency-map.md
└── custody/inspected-files.sha256
```

The checksum manifest is an additional custody artifact required by the task;
all required output paths are present.

## Repository custody freeze

| Item | Frozen value |
|---|---|
| Work directory | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| Starting HEAD | `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` |
| Starting tracked diff | empty; empty-stream SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| Starting untracked set | only `implementation-detail/phase-4/experiments/g4-materialization-acceptance/` |
| Pre-existing handoff | `round-1-research-handoff.md`, 29,162 bytes, SHA-256 `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`; preserved |
| Sibling repository | never entered, read, or changed |
| Starting benchmark state | no active Phase-4/G3/G4/cargo campaign found; no active target benchmark lock found |
| Production integration | false |

The user attachment itself was read at SHA-256
`fa44b9550988bc9278206852d0ef1705add027b35a5771f4c208d59a63e623fe`.
The pre-existing untracked handoff is not an exact byte copy of that attachment;
both independent hashes are retained rather than conflated.

## Host and toolchain freeze

| Item | Observed value |
|---|---|
| OS/kernel | macOS 26.4.1 build 25E253; Darwin 25.4.0; arm64 |
| CPU | Apple M3 Max, 14 logical/physical CPUs |
| Memory | 38,654,705,664 bytes |
| Filesystem/device | APFS Data volume; 4,096-byte block; internal Apple Fabric SSD |
| Rust | `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2 |
| Cargo | `cargo 1.96.0 (30a34c682 2026-05-25)` |
| Git | 2.47.1 |
| SQLite CLI | 3.51.0 |
| rusqlite | 0.40.2 with `cache`, `blob`, and `hooks` features |
| Retained DB policy | `DELETE`, `FULL`, `temp_store=FILE`, `mmap_size=0`, `cache_spill=2000` |

These observations say nothing about cold cache or stable-media completion.

## G0/G1/G2/G3 evidence custody

| Gate | Source/source-set | Executable | Static closure | Manifest | Terminal | Verification |
|---|---|---|---|---|---|---|
| G0 FastCDC-v2 | exact retained CDC source `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6`; no separately named aggregate source-set field | `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8` | `be39b66aaf844314a53d149a003a4537b76139769e2c2f69c319bab7e473ba18` | baseline manifest `f64a484c7966d17f7e1af2ebc8a91c58248605e28d29c9d0d750ded93f951e38`; durable manifest `d7c16a25c4cc89e71745fd1b472f41d38f2e1cc267ee066a66aa283c74e13a97` | terminal manifest actual `8c749c5281038d8e339f891c2cadbd2f12148442be83cbf19a5d55b40cf26d3b` | `7b3a77f9fda6bdbfb31fe648e1df821c9ad06685bcc859a544189706cbaf03bf` |
| G1 writer memory | candidate source `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2`; no separately named aggregate source set | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` | `8c512b39a04481174fb4e9729d5385284d63e9fd5eb10b8a56f144b400d47566` | baseline `1e93b6ffb06051cdfef6958b799dcaaecb97349e3c04bbc23403041ec2ace473`; payload `f02664ea4d82a73126584ed6197b4cea5bc3a21fc08a1562488a7c253dac2a3c` | `54692f9a8d4445bb7c6e17738b0bbb781c8554aad8111d881aa3826d35fc2f07` | `0c89f9913b09ffe1259419b532e70e8d124244e0a942d6f8db20d4cdaeca2b85` |
| G2-v5 decomposition | sealed schema contains no aggregate source-set or static-closure field; this is explicitly `Unavailable`, not invented | control `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55`; instrumented `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5` | `Unavailable` as a formal G2-v5 field | payload `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` | `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` | `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` |
| G3-v13 | source set `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d` | `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e` | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` | 67-entry payload `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` | `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |

G3-v13's sealed STATIC-CLOSURE/ENVIRONMENT record binds the measurement to
HEAD `d79f0e0e2582d1bc491410224fec2b6cef7482e9`, the then-dirty frozen
four-file source set
`3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`,
and executable
`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
Those exact source bytes were committed later in clean controlling checkpoint
`5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`; the v13 measurement must not be
attributed to that clean checkpoint alone.

Additional controlling G2 hashes: raw
`c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb`,
primary `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803`,
independent
`86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e`.
Additional controlling G3 hashes: raw
`3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c`,
primary `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7`,
independent
`2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace`,
cleanup `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e`,
and row cleanup
`1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506`.

Round 1 does not pretend the absent formal G2 source-set/static fields exist.

## Reusable fixture and base custody

Fixture manifest:
`92efe0a320dfe7926293d255c19da24cf688669a975cc26aab7dd424528dadb6`.

| Fixture | Bytes | SHA-256 |
|---|---:|---|
| S1-1 | 1,048,576 | `4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a` |
| S1-10 | 10,485,760 | `0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e` |
| S1-100 | 104,857,600 | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |

Canonical-v2 candidate-B empty full-create masters are read-only and remain
available for research preparation:

| Size | DB SHA-256 | Authority SHA-256 | Expectations SHA-256 |
|---:|---|---|---|
| 1 MiB | `882b73cbaf7221847cf85d5f47653dfce77c4cd44a4ddcb8e35bc5cea095fc54` | `16d1a62e368f4bc32487de90722dc14e8d1fbb8ef5064861c45b91b1076c6337` | `602df7586aa0eb6a5eca18a66a3f27e32a385b1b6bf6d64f757288ab5ecb53bb` |
| 10 MiB | `6df5af8547253447e88e263887047f5808c88828ced72ffa0cd74b36d771ed36` | `4122a2b372cd6d9bffa6cf13db35e8f4bffa3379e475ce2deadf1b31325b4369` | `572337f2bbe0160dd427ad7f4010fb5bdca17852c3ee7fbac975e85e58e85783` |
| 100 MiB | `7321dbc25d3b6efb3f47285392aadcfba008ee243075a9c200b7e63bdbf16c8f` | `bda67899a4424f2827ddb63b936bffe0cf1d29667d7daf12278650ab8850a3ef` | `a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a` |

The G0/G1/G2 common empty 100-MiB base is separately frozen at database
`8657363e0f90d61bdb911c138a734b66c6adf4cd2dcd50c63c1ca1dae814e30c`,
authority
`7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48`,
and expectations
`a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a`.
G4 must prepare version-bound private operands and may not reuse a database
without its exact authority/expectations custody.

## Current path facts that drive the decision

- Canonical-v2 already is an authenticated ordered extent DAG. The root commits
  total length, count, level, cumulative child ends, and child Object IDs; leaf
  references are `(u32 length, ObjectId)`. Do not re-propose raw-ID removal.
- The accepted full reconstruction authenticates 5,371 objects / 105,122,401
  canonical bytes and uses 170 queries: 87 singleton mapping/object queries
  plus 83 batched leaf queries for 5,284 references.
- G2 medians: canonical auth 94.816564 ms; closure 88.483070 ms; raw output
  fingerprint 87.889943 ms; BLOB acquisition 59.403771 ms; occurrence 0.408711
  ms; mapping validation 0.199333 ms; secondary decode 0.141476 ms.
- The accepted `materialize-warm`/`fresh` rows are logical hashing-sink reads,
  not native materialization.
- G3 is the only native path. Its protected fast path is correct but
  benchmark-private and same-open. Its complete fallback writes a native temp
  in one traversal, but uses about one query per object and omits accepted
  closure/occurrence outputs; it is diagnostic-only for G4 M0.
- The public engine's older range/load path performs redundant BLOB passes, but
  that is not the accepted benchmark path.
- VFS and SDK are five-line component stubs; OS is an environment probe. No
  production projection/materialization API exists.

## Exactly three subagents and cross-review

Exactly three initial research subagents were launched after the lead custody
freeze. No additional subagent was created.

| Agent/lane | Owned report | Final SHA-256 | Cross-review conclusion |
|---|---|---|---|
| reconstruction/authenticated read | [report](reconstruction/report.md) | `a4ee4c03759d968c1069e595e42bfee79101bb1c002a862fcf88553d8c0a8261` | Materialization challenged G3 fallback promotion. Report corrected: G3 is diagnostic-only; promotable M0 must retain accepted batching and all proof/error outputs. |
| native materialization/OS/cold | [report](materialization/report.md) | `5d5a78edf880e8738ab84fa4fcf1212eca135e382130b28b936bd35da78522d4` | Reconstruction challenged persistent seed authority. Report confirms live fd handoff is not restart persistence; named metadata/receipts fail same-UID threats. Total-duration compliance uses only the conservative cleanup-inclusive `<26 s` bound. |
| holistic core architecture | [report](core-architecture/report.md) | `f55a96a2576d93e0c8d78b5ce057a2a4a4caad10608049e3b0e24d88b8035f01` | Materialization challenged where the sink boundary belongs, and the audit corrected G2 timer arithmetic, seed-cache first-edit scope, maintenance accounting, and storage research ordering. Report now recommends a narrow shared engine proof-product deletion/fusion inside the already-one-traversal path, while leaving native publication and public VFS/SDK exposure for later. It confirms restart means full revalidation/rebuild. |

All three reports were corrected/accepted after at least one question based on
another lane. Their independent local-file hash ledgers revalidated with zero
mismatch.

## Disposable experiment

One historical/inadmissible capability scratch probe ran in a disjoint `/tmp`
namespace. It preceded
creation of the shared ledger and exact global lock-path freeze; this is a
recorded methodology deviation. It remains API/capability evidence only and is
inadmissible as performance/cold/G4 candidate evidence. No later build or
timing probe ran. The frozen future lock path is
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/target/BENCHMARK_LOCK`.

- compile/probe timer: `1,928,154,208 ns`; `end_mono` preceded `shasum` and
  `stat`, so hashes/stat/cleanup are excluded and this is not a complete wall;
- exact cleanup-inclusive wall: `Unavailable` because cleanup monotonic end was
  not captured;
- UTC start through final cleanup proves a conservative bound `<26 s`, within
  the 120-second hard ceiling;
- source SHA `ad178b73d4fda7a50fc027a36663d3fbb7a68d78d8a468fb768f0523adf288d9`;
- binary SHA `0210b3fb9f1d82da4b234ea6a931b9e8ec5f0e3f436ccef3e82d58eecc05bb79`;
- APFS unlinked-fd clone and CoW isolation PASS; `F_NOCACHE` and preallocation
  calls returned success;
- one byte written into the 4-MiB sparse sample reported the full 4-MiB
  allocation; sparse savings are not assumed;
- no performance/cold/physical-sharing/stable-media claim; and
- cleanup PASS, zero retained bytes, no tracked source to restore.

Full commands, raw results, hashes, limitations, and cleanup:
[experiment ledger](experiments/ledger.md).

## Ranked next actions

### DO NOW / G4

1. Batched proof-preserving verified stream into a durable private native temp,
   after immutable unchanged-G3 `M0-control`, with all clone misses/fallbacks
   converging on the candidate.
2. Same-open protected-seed full-read row plus unchanged G3-v13 clone/patch/
   fallback/fault qualification.
3. Formal proof and one-variable closure-product A/B in the shared
   verified-stream boundary.

### Later architecture

1. Capacity-bounded content-root native cache under stronger service custody.
2. Bounded SQLite-resident authenticated extent BLOBs first; external immutable
   segments only if the one-file SQLite lower bound is insufficient.
3. Direct VFS range/sequential streaming over the shared engine boundary.

### Rejected/deferred

1. Mutable destination/seed metadata/receipt authority.
2. Restart/reopen/`F_NOCACHE` relabeled controlled cold.
3. New Merkle/prolly/CDC profiles during G4; loose-file reflink and foreground
   compression/pack for the current workload.

## Performance and resource hypotheses

```text
verified-stream impossible closure-ceiling floor
  = 338.775916 - 88.483070
  = 250.292846 ms / 396.37 MiB/s
  (upper bound, not a result; plausible hypothesis is 20–88 ms)

first full native warm
  = 338.775916 ms + native write/sync/publish - measured overlap
  acceptance <=400 ms requires net native overhead <=61.224084 ms

trusted seed full read
  100 MiB / 50 ms = 2,000 MiB/s
  100 MiB / 35 ms = 2,857.14 MiB/s
  (live descriptor hypothesis only; persistent/restart claim unavailable)

segment/value-plane impossible acquisition floor
  = 338.775916 - 59.403771
  = 279.372145 ms
  (gross upper bound; plausible hypothesis 20–55 ms)
```

G4 resource goals are RSS <=20 MiB, bounded <=1-MiB output/read buffers, exact
Q with terminal zero, no full-file application buffer, one private temp for
first native output, no per-revision native duplicate, and <=5% mandatory
metadata/storage overhead. APFS clone sharing is never inferred from apparent
or allocated bytes. CPU, SQLite cache, direct bytes/calls, filesystem
publication, and storage high-water are mandatory counters.

The G3 3.414166-ms one-byte number is operation-local: the whole 100-MiB child
was 4.24 s external real / 3.23 s user / 0.91 s system, with a 100-MiB seed,
100-MiB temp, and 100-MiB post-operation verification read. Fill/
qualification, qualified hit, and maintenance revalidation/eviction/repair/
rebuild remain separate CPU/RSS/Q/storage/wall ledgers. A native seed cache
does not prove the canonical mapping/edit authority behind the approximately
154-ms first edit after reopen.

## Controlled-cold and G4 readiness

Controlled cold is unavailable for Round 1. The proposed contract retains
administrative `Unavailable` cells and defines an optional future exclusive-
host buffer-cache approximation using successful `purge`; device cache remains
unknown. Process restart is `fresh-process/warm-or-unknown`.

The [proposed G4 contract](benchmark-contract/proposed-g4-contract.md) is ready
for a separate preregistration author/reviewer, not direct execution: it
contains separate
reconstruction/native scoreboards; 1/10/100 rows; all required state labels;
direct resource/work/durability counters; exact objectives and protection
gates; exact global fail-fast lock path; separate frozen control/candidate
binary identities; a lock-held <=120-second candidate build plus 1/10-MiB
screen; a fixed no-build 30-row chronology with `M0-control` before
`M0-candidate`; a bucketed <=120-second measured-campaign equation; separately
gated non-measured workspace static validation; independent analysis;
append-only repair; and an explicit one-file, non-integration boundary.

## Local source custody

The lead's complete local-file checksum list is
[inspected-files.sha256](custody/inspected-files.sha256). Specialist-specific
additional files and dependency sources are in each report's appendix. The
manifest records complete hashes rather than line-only fingerprints.

## Final validation record

Final disposition: **PASS / TERMINAL ROUND-1 GO FOR SEPARATE G4
PREREGISTRATION REVIEW**. This is not G4 execution authorization.

| Check | Final proof |
|---|---|
| Branch / HEAD | `codex/empty-worktree` / `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` |
| Tracked modified/deleted | 0 |
| Tracked diff bytes | 0 |
| Untracked set | exactly the pre-existing `implementation-detail/.../g4-materialization-acceptance/` and new `research/phase-4/g4-round-1/` directories |
| Required package | 10 files, 398,310 bytes; all required paths plus one custody manifest |
| Lead input checksum manifest | 137 valid nonblank lines: 134 SHA-256 records plus 3 header comments; `shasum -a 256 -c` emitted zero non-OK records; manifest SHA-256 `9cc5004d694b85250f9242d16214c237982e6358b2c2b0632e6e2380248ae211` |
| Specialist reports | reconstruction `a4ee4c03759d968c1069e595e42bfee79101bb1c002a862fcf88553d8c0a8261`; materialization `5d5a78edf880e8738ab84fa4fcf1212eca135e382130b28b936bd35da78522d4`; core `f55a96a2576d93e0c8d78b5ce057a2a4a4caad10608049e3b0e24d88b8035f01` |
| Lead decision artifacts | ledger `72bea463c6cdaa79c015f43c93a638e1d017652ae7313c23b5391e4e91ab3ef2`; contract `d0465118cb3f6b1b03234b41a3fcbf963ac2ffed418ea23ecb0281be9453db05`; matrix `a40d142e1aecc20cfb9921ed774e10e2ebaf74fb1064ecfec88234546c090839`; synthesis `7b3075f0a3f456b27276091576ea8e07882a56c69fae63f9865aa38590b1eeb8`; roadmap `fbbc31f635f41f7f8c72de1754931b0831964c87b47703de453429e78fd06842` |
| Pre-existing handoff | SHA-256 `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`, 29,162 bytes, mode 0644, original mtime preserved |
| Sealed G3 controlling files | raw/static/manifest/terminal/verification hashes exactly `3d2b40…956c` / `cbefce…531` / `1581f8…a49` / `123018…28e` / `a9d068…dd6`; files mode 0444, result root mode 0555 |
| Disposable namespace | absent; retained experiment bytes 0 |
| Global lock / active campaign | exact future `target/BENCHMARK_LOCK` absent; no matching benchmark/build/runner/analyzer/finalizer process |
| Whitespace | every package file passed no-index diff whitespace check; tracked `git diff --check` passed |
| Source restoration | not required; no tracked experimental source changed |
| Commit / G4 / G5 / production integration | none |

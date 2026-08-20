# WP4-M M4.5-0 — frozen evidence and authority-correct baseline

- Verdict: **PASS** for evidence custody; release performance: **NotRun**.
- Decision: retain the accepted M3 dirty tree and advance only to the private
  same-open-witness milestone.  The rejected M4 source was not restored.
- Scope: the benchmark `Store` shadow only; this is not production `Engine`
  integration, profile selection, qualification, promotion, or rejection.
- Labels remain `qualification=false`, `promotion=false`, and
  `rejection=false`.

## Repository and dirty-tree custody

The pre-edit audit ran from
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` and established:

| Item | Frozen value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| HEAD parent / retained implementation checkpoint | `c96b5396e98db523b9a983df4ec80fdedfa971c1` |
| HEAD subject | `docs: specify authority-correct WP4 M4.5 optimization` |
| retained dirty implementation diff SHA-256 | `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb` |
| Cargo/rustc writer at both preflight checks | none |
| worktree-local `AGENTS.md` | absent |
| `git diff --check` | PASS |

The complete initial dirty scope was recorded before any report edit:

```text
 M crates/layerfs-core/src/lib.rs
 M crates/layerfs-core/src/object/codec.rs
 M crates/layerfs-core/src/object/mod.rs
 M crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs
?? ../m0.md
?? ../m1.md
?? ../m1b.md
?? ../m2.md
?? ../m3.md
?? ../m4.md
?? ../../progress.md
```

The four retained source-file SHA-256 values were:

```text
4330a8d04207069ec8ce740f660a0ae2489517e1cef223178591527da4c7562b  crates/layerfs-core/src/lib.rs
69bc0cc76f4fa7a1587343bf2c52f840e6791e790489a00bcdc03a3cd8c0b8be  crates/layerfs-core/src/object/codec.rs
9fd3fb930deb870eb18ae649aafb7610cf030313d896b4cf3e43ce31d50e7ed7  crates/layerfs-core/src/object/mod.rs
76a702f3a365082e7abf2c6be902f1e44f21c0bf42c47da894eb03b01a599ad9  crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs
```

No file under `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was read for mutation
or modified.  No commit was created.

## Frozen executables, fixture, reports, and raw artifacts

All values below were recomputed with `shasum -a 256`; none was copied from a
new benchmark run.

| Artifact | SHA-256 |
|---|---|
| retained M3 executable | `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c` |
| rejected M4 executable | `310d63e95a0d5dcbeedd537370c7d875cc0a2d57735e87b6254721de5a9043ad` |
| retained 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| M3 raw JSONL | `bd57ab8d8736165555303034863f877fa5090a16110346ad0c66777608da1563` |
| M4 raw JSONL | `d57de54c754831fbde0719d58d23122e98c5dd3727543b85a9d2b31dc9c4cf09` |
| M3 environment | `bb136d0239d815e472a200ee2d92faba7984242e2a20569f8abedb2ae2405431` |
| M4 environment | `6df817b8704c9300ab7ee103bdd22992e2e41574ea683847c6df33d889e0bd1b` |
| M3 report | `18138ca24bcf403fdcfc990d0b268d05d084db0de067118eef4101cdfb2c4c5d` |
| M4 report | `983eb8f34a78eb8d1782acd5b3ceb333d0962da4d8da0557ee9cfb177f227825` |

The rejected M4 candidate diff remains frozen only by its recorded SHA-256
`91f394fdcfccca4c3625e7962db56ac0304f2b2b32bc65875089755316d0a139`.
The exact rejected source is not in the worktree and was not reconstructed from
the binary.  The post-rollback retained diff recomputed exactly to the M3 hash
above.

Raw evidence is preserved under
`target/wp4m-opt2-k64-20260818/`.  The M3 and M4 raw JSONL contain 13 and 12
rows respectively; their corresponding external-resource JSONL contain the
same counts.  `jq -e` parsed every row.  Direct assertions passed for all M4
rows: nonqualifying labels, `status=PASS`, both timer equations, one COMMIT,
and the frozen source/root/transition/closure identities.

## Retained row and corrected M4 rejection rationale

The retained same-middle result is the exact 100-MiB K64/F64 fixture:

```text
source bytes              104,857,600
base source fingerprint   bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
edited source fingerprint a4aaf02c293df75c63072af86264908183c6e213997cf677b63f75d8a9819e3e
result references         5,284
result CDC fingerprint    58b61bbd4f319ecb6011278ca42caf2b5d696e42b4655c054c48b3906d017b83
result root               cc8f31adc20eaa56b621744fe45f90f65fb9ac6177446d33b0052d7ebd404560
result transition         2686d6ffc512b38f64922073dcc191a1ff1c7eacedb1c73e0a72045bf7cf4a92
result closure            7b7142f5e203ae23efd46662efe576a182f8043c4323f487407bbb031b7cc2bb
changed source bytes      18,854
```

The row-level `source_fingerprint` is the prepared base-source identity, while
the `file=` field of the frozen per-row expectation manifest binds the edited
source fingerprint and CDC sequence.  M4.5 must make that distinction explicit
in the prepared oracle rather than infer it from a field name.

The frozen M4 speed evidence remains nonqualifying:

| Metric | M3 median | M4 median | delta | paired wins |
|---|---:|---:|---:|---:|
| pre-COMMIT closure | 430,182,417 ns | 150,417 ns | -99.965% | 5/5 |
| durable edit | 433,029,417 ns | 2,195,375 ns | -99.493% | 5/5 |
| complete lifecycle | 1,134,315,958 ns | 691,662,792 ns | -39.024% | 5/5 |

The corrected rejection is not “the optimization was too slow” and is not a
claim that M4 inherently caused a 7.921% RSS increase.  M4 is rejected because
its row process treated a receipt persisted by a different preparation process
as skip authority.  Deleting or corrupting an unchanged sibling between those
processes could therefore publish an incomplete closure.  Independently, M4
did not bind the exact edited source/ordered CDC/result IDs before COMMIT,
omitted file-root `mode` from summary equivalence, did not exercise real
ambiguous-COMMIT outcomes, lost exact missing IDs, mislabeled statement-cache
acquisitions as native prepares, and used max-local rather than summed-live Q.

The five-pair RSS result is preserved unchanged.  Its arm medians were
16,547,840 and 17,858,560 bytes (+7.921%), but the paired deltas were
`-2,506,752; +1,589,248; -294,912; -65,536; +1,490,944` bytes.  Under the
M4.5 contract this is **INCONCLUSIVE**, not causal failure: ranges overlap and
three of five pairs favor M4.  A 20-pair adjudication is allowed only if the
new M4.5 five-pair result triggers it.

## Work, resource, and storage classification

| Class | Frozen evidence | M4.5-0 classification |
|---|---|---|
| correctness / identities | all raw M3/M4 rows pass exact source, CDC, root, transition, closure and one-COMMIT gates | PASS as frozen evidence; M4 authority still invalid |
| CPU | 1.13 s M3 vs 0.68 s M4 median | Observed, favorable, nonqualifying |
| RSS | mixed pairs; arm median +7.921% | INCONCLUSIVE under corrected rule |
| peak footprint | arm median +11.872%, overlapping/noisy process evidence | INCONCLUSIVE pending trigger procedure |
| logical Q | M4 reported `33,604,696 + 12,288 = 33,616,984` bytes | INVALID as an exact-Q proof because it used max-local rather than summed-live accounting |
| W | 26,249 canonical bytes newly written in both arms | Observed and equal |
| D | M4 reported `210,825,999 - 26,249 = 210,799,750` bytes | arithmetic passes for its old counter definition; must be rechecked under M4.5 counters |
| SQL | 5,622 `prepare_cached` acquisitions/executions in M4 | Observed, but not native prepares; label requires repair |
| BLOB | 10,824 row reads, 11 row writes, zero incremental BLOB opens/reads/writes | Observed |
| storage | 109,297,696 logical/apparent and 126,050,304 allocated bytes after; 16,777,216 allocated delta; 32-byte authority sidecar; zero endpoint journal | arms byte-identical; PASS as frozen evidence |
| physical I/O / fsync / SQLite page-cache memory | absent from host evidence | Unavailable |

Release performance for M4.5-0 is **NotRun**.  This milestone only validates
frozen evidence and does not reproduce or extend the benchmark.

## Before/after path, complexity, and memory bound

No implementation path changed in M4.5-0.

```text
retained M3 before/after:
same-count COW mutation -> full pre-COMMIT closure -> COMMIT

rejected M4 evidence only:
same-count COW mutation -> receipt-backed changed-spine proof -> COMMIT
```

The retained source therefore remains full-closure `Theta(N)` before COMMIT.
The rejected experiment demonstrated a potential
`O(K + F*H + A_delta + V_delta + H^2)` qualification shape, but M4.5-0 does
not claim that bound for current code.  Fresh scrub, reconstruction, and the
complete lifecycle remain linear.  `+1` remains `Theta(suffix)`.

The current retained Q claim is only the prior M3 diagnostic:

```text
Q_old = 33,604,696 fixed windows + 6,836 max-local semantic bytes
      = 33,611,532 bytes
```

M4.5-4 must replace `max-local` with checked charge/decharge accounting for
the sum of every simultaneous LayerFS-owned capacity; until then exact Q is
not established for M4.5.

## Commands and exit decision

Read-only commands used for this milestone included:

```text
pwd
git branch --show-current
git rev-parse HEAD
git status --short --branch
git diff --stat
git diff --name-status
git diff --binary | shasum -a 256
git diff --check
git show -s --format=...
ps -axo pid=,ppid=,command= | rg '(^|/)(cargo|rustc)( |$)'
shasum -a 256 <frozen executables/fixture/manifest/raw/report artifacts>
wc -l <M3/M4 raw and external-resource JSONL>
jq -e . <each frozen JSONL>
jq -e -s <frozen label/timer/identity/COMMIT assertions>
```

Exit decision: **retain M3 baseline and advance to M4.5-1**.  No production
engine, schema, receipt bytes, persisted authority, benchmark result, or source
file was changed in this milestone.

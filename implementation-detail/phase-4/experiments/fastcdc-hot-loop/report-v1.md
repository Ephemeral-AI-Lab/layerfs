# FastCDC exact hot-loop experiment v1 — NO-GO / REVERT

Date: 2026-08-21. Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`.
Starting checkpoint: `daf4cefc1fd7861681de3f94bf042b556cc21ccb`.

## 1. Disposition

**REVERT — FASTCDC EXACT HOT LOOP NO-GO.** Exact boundary/callback/resource
parity passed, but the candidate had no mechanism signal: it was 0.269698 ms
slower position-balanced, improved only one of four measured pairs, and lost
the second execution-position stratum. The conditional durable A/B was
therefore forbidden and was not started. The candidate `cdc/mod.rs` change and
its focused test additions were reverted; the live file is byte-identical to
the committed Canonical-v2 source.

## 2. Exact source diff and mechanism

Frozen candidate diff SHA-256:
`facc341de5ffff6d25d1daab5f8219b5784672326e9211d86aebb14997dc7816`.
It added two scanner-local `u64` fields holding the active normal/shifted mask,
initialized them to the frozen small masks, switched them once when
`next_even == TARGET_CHUNK_BYTES`, and reset them during existing emission.
This replaced two per-pair small/large mask selections. It changed no table,
mask value, chunk size, rolling update, pending-byte path, callback, buffer,
identity, dependency, unsafe code, worker, or persistence behavior.

The frozen candidate also expanded fragmentation patterns and added exact
callback-propagation and fixed-capacity tests. Those test-only edits were part
of the frozen source/diff and were reverted with the failed candidate so the
live tracked source exactly matches the checkpoint.

## 3. Tests and static checks actually run

Before timing, all of these passed:

- 5/5 focused CDC unit tests (frozen boundaries, five fragmented-reader
  patterns, short/min/max edges, exact callback error, fixed capacity);
- CAS scan identity/deduplication/reconstruction callback test;
- streaming full-replace test;
- independent Canonical-v2 1/10/100-MiB fixture oracle, including the retained
  100-MiB 5,284-occurrence corpus;
- `cargo fmt --all -- --check`, `git diff --check`, standalone harness rustfmt,
  Python syntax, frozen schedule, external timer, and 15-entry methodology
  custody checks.

The full workspace and Clippy were not run because the preregistration permits
them only after both the mechanism screen and conditional durable campaign
pass. The screen failed before that gate.

## 4. Source and executable custody

| Item | SHA-256 |
|---|---|
| accepted Canonical-v2 benchmark source | `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120` |
| accepted durable control executable | `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280` |
| control `cdc/mod.rs` | `82d8463101675e8f0e5632b532a3a96893405adaa09d311fddb25ca322620940` |
| candidate `cdc/mod.rs` | `eed17659d0d8f86793ad6b0ffbd4a6b89470555503aa033dda1d3acb5e417923` |
| candidate diff | `facc341de5ffff6d25d1daab5f8219b5784672326e9211d86aebb14997dc7816` |
| CDC screen control executable | `a3a0808fc98148a979dfde9d70030b925a0dbd83acd946c7b089e2e2c8515f0d` |
| CDC screen candidate executable | `6de6085e4eaaf140a59d944876316ad433a450fb78a1435f19f3ff29f920f814` |
| once-built durable candidate executable (not run) | `9160fcad455af20aecd04c28b59665d0f414e52aa91a4cb845746b4e2961774f` |
| fixture | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| preregistration | `81b2354f1f3b4899f0b44ef5694554af0bf3eed563616d8a86d1997e11228c46` |
| prospective methodology | `02d00335327ee4958ea35ca8d0a1579b2ba36c505b48bbec53c11093e3a5246d` |

Each control/candidate scanner was compiled once from independent archived
source. The candidate scanner and durable executable were produced by one
release build. No measured row or executable was rebuilt.

## 5. Mechanism screen

The exact schedule was one uncounted `AB` warmup followed by measured
`AB / BA / AB / BA`. Ten scanner invocations completed, including acquisition,
analysis, disposition, and the small manifest, in 4.597846 seconds under the
19-second ceiling.

| Pair | Order | Control ms | Candidate ms | Candidate saved ms | B faster |
|---:|:---:|---:|---:|---:|:---:|
| 1 | AB | 190.064583 | 191.291541 | -1.226958 | no |
| 2 | BA | 189.676250 | 190.860958 | -1.184708 | no |
| 3 | AB | 189.333791 | 190.371500 | -1.037709 | no |
| 4 | BA | 186.823083 | 184.452500 | +2.370583 | yes |

Position-balanced arithmetic means were 188.974427 ms control and
189.244125 ms candidate: **candidate -0.269698 ms saved**, or **-0.142717%**
(a regression). Position 1 favored B, 187.656729 versus 189.699187 ms;
position 2 rejected B, 190.831521 versus 188.249667 ms. Only 1/4 pairs favored
B. The required 15.000 ms, 10%, 3/4, and both-position gates all failed.

## 6. Exact parity

All ten rows independently reported and retained:

- 104,857,600 input/scanned/read/reconstructed bytes;
- 5,284 occurrences and 5,284 callbacks;
- summed occurrence length 104,857,600;
- minimum/maximum observed length 8,219 / 32,768 bytes;
- 3,200 nonempty source reads plus one EOF read;
- scanner chunk-buffer capacity 32,768 and bounded observer capacity 5,284;
- reconstructed source BLAKE3
  `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7`;
- ordered boundary transcript BLAKE3
  `b932a2a719ce671d58d06a1e8c1aa3c20b6f27d4cbe7cbf0ec7e369c6b97588d`;
- independently parsed exact boundary TSV SHA-256
  `eed726dcab0c2141cee9ad36322ad86f8e9b6d8d681f8b7adb23b38a81656008`.

Every start was the prior end, every `end-start` equaled length, no occurrence
was empty or duplicated, no boundary exceeded 32,768, and the terminal end was
exactly 104,857,600. Fragmentation and callback behavior passed the focused
tests. There was no allocation-capacity growth.

## 7. Conditional durable result

**Not run.** `advance_to_durable=false` is frozen in the screen analysis. No
database was opened and no durable row, preparation, or SQLite observation was
started for this experiment.

## 8–9. Canonical baseline and phase breakdown

The controlling frozen Canonical-v2 durable center remains 512.214000 ms:

| Phase | Frozen baseline | Candidate |
|---|---:|---:|
| Canonical CAS + mapping | 321.749854 ms | Unavailable — durable campaign forbidden |
| Proof | 0.047855 ms | Unavailable — durable campaign forbidden |
| SQLite observation / durable COMMIT | 190.416292 ms | Unavailable — durable campaign forbidden |
| Total | 512.214000 ms | Unavailable — durable campaign forbidden |

No historical subtraction or CDC-screen projection is presented as candidate
full-create performance.

## 10. CPU, RSS, Q, and storage

Measured four-row arm means were identical at the resolution of
`/usr/bin/time -l`: 0.17 s user and 0.01 s system for both arms. Mean maximum
RSS was 1,880,064 bytes control and 1,875,968 bytes candidate. Per-row maximum
RSS ranged from 1,867,776 to 1,884,160 bytes; peak footprint ranged from
1,245,496 to 1,261,880 bytes. CPU, RSS, and fixed-capacity screen gates passed.

Scanner capacity was exactly 32,768 bytes in every row. Product Q, SQLite
cache, logical/apparent/allocated store bytes, journal residue, and durable
resource endpoints are not applicable/unobserved because the durable campaign
was forbidden and no SQLite path ran.

## 11. Unavailable observations

Instructions, cycles, physical I/O, sync-call counts, true cold-cache state,
phase-local CPU, and non-scanner heap allocation are unavailable because the
frozen public/runtime observers do not expose them. Q and storage endpoints are
unavailable because the mechanism screen is intentionally CDC-only. Candidate
durable wall and phases are unavailable because running them after the failed
screen would violate the prospective protocol. None is inferred from wall,
RSS, logical bytes, the historical 128.723-ms attribution, or subtraction.

## 12. Artifacts

Artifact root:
`target/phase4-fastcdc-exact-hot-loop-20260821-v1`.

The CDC screen retained 39 manifested payloads. Its manifest SHA-256 is
`299a7f9945e492e50dd92065b77dae9620a200c5a7da26d340ecdc59e769869f`.
Raw/analysis/report/final-clock SHA-256 values are respectively
`4939ff1fc44d6ca60f35ee8af2a7e4ab4f5aa109fe4c206441c53baa7883d7c8`,
`c80956c7e574b581bd17e4a2dc9f3e8d906d2ff497b174f8e397483655d0e0e9`,
`f6c1a626d457dcdeda6d3a267ef40ad896ccc58e73752fb2e0cd26292c8dd9dc`,
and `c48974bb89f7b5be88c4c009bdce56221b63548918fbc582664f9209d2df265f`.

## 13. Milestones

500/400/333.333/250 ms were **not evaluated and not claimed crossed**. No
candidate durable total exists.

## 14. Final audit

- exact branch/checkpoint and clean starting custody: pass;
- exact fixture, baseline, source, diff, binary, preregistration, runner,
  analyzer, and methodology custody: pass;
- ten-row chronology, four balanced measured pairs, no rerun/deletion: pass;
- exact boundary, callback, byte, read, capacity, CPU/RSS parity: pass;
- 4.597846-second screen ceiling: pass;
- prospective signal decision and durable stop: pass;
- live `crates/layerfs-core/src/cdc/mod.rs` restored to SHA-256
  `82d8463101675e8f0e5632b532a3a96893405adaa09d311fddb25ca322620940`:
  pass;
- tracked candidate source diff after revert: empty;
- active experiment locks: none;
- commit: not performed.

## 15. Next-control eligibility

The candidate has **no next-control eligibility**. The exact frozen
Canonical-v2 executable
`f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280`
remains the accepted control. No baseline-successor report or roadmap-control
change is authorized.

## 16. Scope confirmation

H09, SQLite changes/campaigns, concurrency, materialization, reopen-authority
work, production integration, another FastCDC optimization, and commit were
not started.

# Stage 1.0 — Implementation closure and accepted A02 exception

Status: **CLOSED after adversarial correctness repair, with the explicit A02
performance exception retained**
Date: 2026-08-24
Scope: implemented Stage 1.0 product baseline and preserved A01–A17 evidence;
Stage 1.1 (`poc/16`) and Stage 1.2 (`poc/15`) remain prospective verification
specifications; mounted/write-intercepted macOS work remains Stage Two

## 1. Exact disposition

```text
product correctness and custody       PASS / closed
post-repair source closure/readiness  PASS; portable receipt below
preserved pre-repair A01–A17 artifact REVISE
A02 frozen latency target             measured miss, user-accepted exception
all other measured hard targets       PASS
Stage 1.1 APFS edge benchmark         not started; prospective
Stage 1.2 npm/workspace benchmark     not started; prospective
mounted Stage Two                     not started; separately specified later
```

The A02 latency miss is closed only by explicit user acceptance. Correctness,
authentication, recovery, bounded-state, and custody findings from the later
adversarial audit were repaired separately and are not covered by that waiver.
The evaluator thresholds, raw rows, summary status, and prior receipts remain
unchanged.

## 2. Preserved pre-repair measured campaign

Controlling artifact:
`poc/evidence/stage1-pre-repair-campaign-20260824` (portable copy of the
original ignored `target/layerfs-stage1-run-final-profile-20260824`).

```text
schema                    layerfs-stage1-summary-v2
artifact status           REVISE
valid rows                61/61
resets                    54
complete wall             42.866453417 s
campaign timing equation  closed exactly
aggregate SHA-256         ba53703603f2513d7864bb6e5ffb2948e38371e1f1f06ac108cad9ef3a36445f
campaign git baseline     e6436678567de29e27ff532bfcc2c2800fbf5823
campaign source BLAKE3    7063a3ea6353d57c519371172fef3b7cf032a8676b3334686ee73cc3ecf63c1e
campaign executable BLAKE3 07b44b9567b85d1f23457e69b94e62952e0e73839f380a0c7efc65369c786611
```

This campaign does **not** bind the post-adversarial-repair source. It remains
historical performance and operation evidence for its exact source/population.
The repaired source is bound by focused correctness proofs, a self-sealing full
test transcript, release build, and zero-row readiness; no new measured rows
were authorized.

Measured hard-target results:

| Gate | Result | Disposition |
|---|---:|---|
| A01 100 MiB range read | `261.599827 MiB/s` p50 | PASS (`>=250`) |
| A03a streamed import | `265.519358 MiB/s` p50 | PASS (`>=150`) |
| A03b replace existing | `257.713444 MiB/s` p50 | PASS (`>=150`) |
| A04 logical 4 KiB edit | `4.103541 ms` p50 | PASS (`<=15 ms`) |
| A04 native edit + checkpoint | `16.582167 ms` p50 | PASS (`<=20 ms`) |
| A09 reconstruction | `279.659100 MiB/s` p50 | PASS (`>=200`) |
| A10 cold materialization | `255.575404 MiB/s` p50 | PASS (`>=150`) |
| A11 exact no-op | `0.044042 ms` p50 | PASS (`<=5 ms`) |
| A12 changed-root refresh | `16.429208 ms` p50 | PASS (`<=25 ms`) |
| A13 reopen/head ready | `0.849000 ms` p50 | PASS (`<=4 ms`) |
| Complete campaign | `42.866453417 s` | PASS (`<60 s`; hard `<=120 s`) |

A05–A08, A14, A15, and A17 were report-only populations and retain their raw
measurements and exact counter/oracle equations. A16 passed with peak/current
RSS `20,725,760 / 14,991,360 B`, FD `5/5`, and terminal residue, Store
connections, and operation Q all `0`.

The original `poc/14` sentence capped every intermediate/output regular file at
100 MiB, but Store authority files were intentionally reported separately by
the evaluator. Prepared Store databases reached `109,211,648` bytes, and the
campaign reported `store_database_bytes_max=218,378,240`. They therefore do not
satisfy that old literal all-file wording. `poc/14` now states the actual gate:
input, intermediate user-data, and native output files are capped at
`104,857,600` bytes; SQLite Store amplification is separate evidence.

## 3. A02 accepted exception

The frozen target remains p50 `<=500,000 ns` and p95 `<=1,000,000 ns`.
The preserved campaign measured:

```text
p50  679,021 ns
p95  1,025,875 ns
```

The separately preregistered 300-unique-offset diagnostic measured:

| Layer | p50 | p95 |
|---|---:|---:|
| Raw SQLite payload retrieval | `616,791 ns` | `1,151,459 ns` |
| Authenticated Engine | `687,417 ns` | `1,140,083 ns` |
| Public SDK range route | `777,917 ns` | `1,356,250 ns` |

Diagnostic result:
`poc/evidence/a02-diagnostic-20260824/result.json`, SHA-256
`0d7ecc3cfbceb75239137616d130208f3d7e29c776c1388d24f3a204e54a184f`.

Disposition: **user-accepted performance exception**. It must not be reported
as a measured PASS, an external impossibility, or evidence for weakened
thresholds. No `WITHOUT ROWID` migration, covering-BLOB index, mmap/profile
change, payload-authentication bypass, or additional campaign was performed.

## 4. Post-adversarial-repair correctness and custody

The current product source also repairs the adversarial findings: external
materialization binds its pre-construction RefState; rollback requires caller
freshness; failed managed mutations do not grow the spool; equal-root refresh
revalidates native binding; empty reads authenticate their mapping root; batch
callbacks have exact cardinality; ContentDigest uses its frozen domain; managed
record loads use the borrowed role boundary; compaction authenticates the full
object index, preserves/reports unresolved residue and caller mode, and refuses
nonempty legacy state rather than erasing it; A16 is appended on early failure;
and A17 serializes rematerializations explicitly. Initial descriptor-spool seek
failure is fail-closed; successful appends return their exact final length so
replace/rename accounting cannot fail after native visibility; residual spool
observation failure is fail-closed; and a committed checkpoint records the
contractually empty replacement spool without another fallible seek.

The portable current-source closure is
`poc/evidence/stage1-post-repair-closure/closure.json`. It binds the exact
Rust/Cargo source manifest, release executable, self-sealing test transcript,
focused proof list, final zero-row readiness, and test counts. The v3 transcript
is the full-workspace foundation; the exact-source v5 transcript reruns the
touched VFS/SDK closure, release build, and readiness after the final spool
repairs. Each prints its source/command seal before tests and its
readiness/executable seal after the release build rather than relying on
operator chronology.

```text
post-repair source BLAKE3   413e1a7d454914b66e8d1dbcd9fccaa429f168f381b3d444ff0be1ff0dd67e96
closure manifest SHA-256   fe7c10d008d898d850e4ad702702218d0bc3cc3d5029bb686cd839dfa0beb36f
release executable SHA-256 49f6ee56f623c9986c57f407eab1b058227485efad0fcba124aafdb48e44d101
release executable BLAKE3  94f5a87f10176d79b4ec90ace341bd296396b42fd1c25c59b9b880c31cf12524
full closure SHA-256       6c8e91582d815fd5d38bb9ca930de00802d5e2a7e1c194f834ac3263accceb9a
exact-source closure SHA-256 0fbfffe6c695fa6615fae5ff8e7237c3fc085e5bfa64a6f2fa08ddd3b96abbf4
full workspace foundation  19 suites; 239 passed; 3 ignored; 0 failed
exact-source touched tests VFS 7 + SDK 21 passed; 0 failed
readiness SHA-256          ecb99f0343265570f12f8a2e550d83cb54014a89a4e4de68ca331a5e9146bfff
readiness                  PASS; measured_rows_started=false
```

The preceding v2 closure stopped at Clippy and is preserved as
`closure-v2-clippy-failure.typescript`; v3 is the distinct repaired full run.
The v4 touched run is also preserved as superseded because its adversarial
review found the adjacent post-append observation boundary. V5 is the final
exact-source touched run; no receipt was overwritten or relabelled.

## 5. Preserved evidence custody

The original target campaign directories remain immutable with aggregate
SHA-256, and the controlling pre-repair run is copied under `poc/evidence`:

```text
target/layerfs-stage1-run-final-20260824
  cdd46ea6bf203d3856ac32c7baf326404e8d8686ab0a39b997448606c0c8b75d
target/layerfs-stage1-run-repair-20260824
  d436b7a40f83434cbddd5b3c7c6cee3ca1b8aa11987263e38c754c25bb52ffd4
target/layerfs-stage1-run-spill-readplan-20260824
  8dd1848806dc1218aaa4f0e45d8b3a0127ef5b233ff4c1e6ed6233fc5fb2d328
target/layerfs-stage1-run-final-profile-20260824
  ba53703603f2513d7864bb6e5ffb2948e38371e1f1f06ac108cad9ef3a36445f
```

## 6. Handoff boundary

Stage 1.0 implementation is closed after the adversarial repairs. Reopening it
requires a correctness, durability, authentication, custody, or hard-resource
regression—not the accepted A02 latency values. Stage 1.1 and Stage 1.2 are
separate source-bound verification specifications and cannot retroactively
relabel the pre-repair campaign. Their accepted completion closes Stage One
verification and makes a separately specified mounted Stage Two eligible.

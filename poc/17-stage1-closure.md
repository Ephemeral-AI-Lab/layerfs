# Stage One — Closure and accepted A02 exception

Status: **CLOSED for the PoC by explicit user acceptance**
Date: 2026-08-24
Scope: current Stage One product implementation and the preserved A01–A17
single-file evidence; no mount, Part Two workspace campaign, or supplemental
Apple-edge campaign

## 1. Exact disposition

```text
product correctness and custody       PASS / closed
current-source zero-row readiness     PASS
preserved A01–A17 artifact            REVISE
A02 frozen latency target             measured miss, user-accepted exception
all other measured hard targets       PASS
Part Two workspace campaign           not started; deferred
supplemental Apple-edge campaign      not started; deferred
```

The Stage One project is closed because the user explicitly accepted the A02
latency miss as sufficient for this PoC and directed that no more time be spent
optimizing it. This is a project-acceptance decision, not a change to measured
facts. The evaluator thresholds, raw rows, summary status, and prior receipts
remain unchanged.

## 2. Preserved measured campaign

Controlling artifact:
`target/layerfs-stage1-run-final-profile-20260824`.

```text
schema                    layerfs-stage1-summary-v2
artifact status           REVISE
valid rows                61/61
resets                    54
complete wall             42.866453417 s
campaign timing equation  closed exactly
aggregate SHA-256         ba53703603f2513d7864bb6e5ffb2948e38371e1f1f06ac108cad9ef3a36445f
```

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
`target/a02-unique-attribution/results/run-v1/result.json`, SHA-256
`0d7ecc3cfbceb75239137616d130208f3d7e29c776c1388d24f3a204e54a184f`.

Disposition: **user-accepted performance exception**. It must not be reported
as a measured PASS, an external impossibility, or evidence for weakened
thresholds. No `WITHOUT ROWID` migration, covering-BLOB index, mmap/profile
change, payload-authentication bypass, or additional campaign was performed.

## 4. Current-source correctness and custody

The current product source includes the post-campaign correctness repairs,
including one-snapshot compact return state, counter-only measurement
snapshots, borrowed authenticated payload handling, bounded SQLite cache/spill,
honest Q accounting, and the no-sort ordered payload query.

Current release evidence:

```text
source baseline/parent commit e6436678567de29e27ff532bfcc2c2800fbf5823
final Stage One commit        recorded post-commit in target closure receipt
Rust/Cargo source BLAKE3      938d15d67b3f8934df97b6e75372c2d1c12dbca779a5cf87ee27d0e7c2cb9290
release executable SHA-256   8fb6d9d187644c8f62509152ee5536e672571aa40aed0438267a883bbb83b6e0
release executable BLAKE3    ac0a7ec90901514829dfea04ec6909a5b65afbc521deb6d00dc5d6f5ea0b5e31
release executable bytes     2,672,352
```

Durable closure transcript:
`target/layerfs-stage1-closure-20260824-v1/closure.typescript`, SHA-256
`715d73a23de4e6eaffca3ba0d3a82c053572a0b21c878b9486007d9035f1b53a`.
It records:

```text
cargo fmt --all -- --check                         PASS
cargo check --workspace --all-targets              PASS
cargo clippy --workspace --all-targets -- -D warnings
                                                     PASS
cargo test --workspace --all-targets -- --test-threads=1
                                                     19 suites; 226 passed;
                                                     3 ignored; 0 failed
cargo build --release --workspace                   PASS
git diff --check                                    PASS
```

The final zero-row receipt is
`target/layerfs-stage1-readiness.json`. It must say `status=PASS` and
`measured_rows_started=false`, bind the exact Rust/Cargo source and release
executable above, retain the 4096/1280/1280 Store profile, and pass three APFS
clone resets. Its final digest is recorded in the target-owned closure receipt.

## 5. Preserved evidence custody

The prior campaign directories remain immutable with aggregate SHA-256:

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

Stage One is closed. Reopening it requires a correctness, durability,
authentication, custody, or hard-resource regression—not the accepted A02
latency values. The deferred workspace and Apple-edge documents remain future
supplements and cannot retroactively relabel this campaign.

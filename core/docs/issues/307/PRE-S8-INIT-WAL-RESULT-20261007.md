# Pre-S8 Init WAL decision and private-import implementation

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> One registered sample on2026-10-07 at `c5fae7e3aa783dcdb1c09bf977c0395a5b066dd5`.

The WAL-throughout candidate fails the existing1.10× speed comparison. The
owner's precommitted rule selects private memory-journal import followed by
WAL conversion at seal. No second timing sample is authorized or collected.

| Selected case | Complete product ns | Complete performance command ns / bound | Separate verifier ns / bound | Store allocated bytes | Correctness / cache / cleanup | Speed gate |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Disposable1000 serverless-wal-v1 | 198720291 | 1478753125 /30000000000 | 673972125 /19000000000 | 20574208 | PASS / PASS / PASS | FAIL |
| Retained memory incumbent-restored-v1 | 155291459 | Retained original receipt | Retained original receipt | 20590592 | Retained original PASS | Original verdict preserved |

Arithmetic: `1987202910 > 1708206049` for10×candidate versus11×control.
Raw delta is+43428832ns (+27.9660145379%) and−16384B (−0.0795703203%).
This is one selected development decision against a retained control, not a
new matched-pair speedup or release result. Admission is false; phase peaks and
resource qualification remain unavailable. The eight historical speed and eight
strict-allocation failures are unchanged. New strict-allocation selections are
NOT_RUN — mechanism removed; all Durable execution is owner-deferred NOT_RUN.

The fixture is1000 files/11 directories/20000000 bytes, seed1, reused from the
closed prepared source. Source-content residency is zero after the native
cold helper; metadata residency remains unobserved. The independent oracle
checks1011 paths and70 selected files/6430827 bytes. It is not a20MB full-byte
oracle. The driver observes one public Init call, uses four supported Init
constructors and exports `LAYERFS_CONSTRUCTION_WORKERS=1`. No daemon/FUSE/SDK
transport or container participates in this host component measurement.

## Identity and custody

- Source/tree: `c5fae7e3aa783dcdb1c09bf977c0395a5b066dd5` / `ba306ece0fe5ae4929a0ff9ad65e50b8badc9132`.
- Product seal: `0b8a71227c14eb5d9946c8d9152585f3f6248536b20b680fc7d64d89e93a5f79`.
- Compilation seal: `a32a3ed0a9e7d5e694d321f0667ea4e48a3147c5ed40f16dad38aa29b00820d2`.
- Dependency seal: `06392f24ff430d8e772025ea1e9fb133f20398f1ed66d14b90ed9f5592e29874`.
- Release driver: `4a6e7919a7a90ca2f33ec26eba835b51a549824f66901b7dc495b3ff7503d535`.
- Independent verifier: `dfd6a2eadc71d91f84261302af8761ab70458e1428c7b4b65588ddcff3d8762d`.
- Fixture/oracle manifest: `e4c484767163117b3c846b6d846cc8157fed052e0b15c70a908ad977f59d881c`.
- Host macOS/system SQLite3.51.0; image N/A. Existing Docker background services
  are recorded as observed competing processes; no competing Cargo/test was run
  by this owner during the sample.
- Git is globally dirty only because the three required non-input handoffs stay
  untracked. Their exact hashes are retained; committed measured inputs are
  separately validated. The whole working tree is never labelled clean.

[Exact command](checks/pre-s8-wal-init-20261007/09-invocation.stdout),
[raw original receipt](checks/pre-s8-wal-init-20261007/10-wal-sample/receipt.json)
and [compact identity/result](checks/pre-s8-wal-init-20261007/11-result.json)
retain all operands, build/cold-helper reuse, commands, scope and outputs. The
original closed Store remains at the owned ignored output
`benchmark-results/fs-bench-pro/pre-s8-wal-init-20261007-v1/store.sqlite`.
Only text/JSON evidence is copied into Git, without rewriting raw bytes.
Scratch is empty, the Store has no sidecars and the original child exited0.
No unknown publication or cleanup outcome occurred.

## Deepest-file plan for the selected import route

| File under `core/crates/` | Change and requirement |
| --- | --- |
| `layerfs-persistence/src/store/open.rs` | Add an explicit `create_for_init` constructor. Disposable builds privately under memory journal; normal create stays WAL; Durable stays WAL/FULL. |
| `layerfs-persistence/src/backend/sqlite/{connection,profile}.rs` | Record private-init state and actual profile identity. Opening an existing file still only verifies WAL and never converts it. |
| `layerfs-persistence/src/backend/sqlite/seal.rs` | Sole-owner private seal explicitly changes to WAL, verifies the answer, then uses the existing one checkpoint/close/sidecar check. |
| `layerfs-persistence/tests/private_init.rs` (new) | Disposable private state is refused by shared open, canonical/history state survives seal, retained providers refuse before conversion, reopening uses the shared WAL profile. |
| `layerfs-project/tests/init_sqlite.rs` | Verify real host Init and its oracle through the private creation API. |
| API handbook / shared-Store architecture | Distinguish private acquisition's weaker crash contract from the ready shared Store. |

No daemon uses the private creation API. The unsealed private import is not
available to shared open and has no crash-survival claim; only successful seal
produces the normal WAL provisioning artifact. The eventual thin SDK Init uses
this explicit constructor. No default open migration, automatic retry, second
Store, dependency or file-sized copy is introduced. Functional proof and the
required scoped checks follow, with no new timing run.

## Private-route verification

Receipt12 builds the new public API and real Init oracle. Receipt13 passes all
three host private-init proofs;14 passes the real100/1000-file namespace oracle
through `create_for_init`. Clippy15 passes for Persistence, Project, SDK and
Daemon all targets. Receipt16 hashes the changed inputs,17 builds the Linux
private-init binary and18 passes its three bodies on container-local storage.
Formatting19, boundary20 and42 guard self-tests21 pass. Every test invocation
has a100s wall stop; no test reaches it. Linux remains the pinned image from
F1–F4, Rust1.85.1 with repository ARM64 flags. Both profiles build; all execution
is Disposable. There is no second performance sample and no private-import
speed claim. The thin SDK's use of this constructor remains F5.

Production LOC:170469 ->170525 (delta+56). Core105052 ->105108; reference
65417 ->65417; active core61887 ->61943; excluded predecessor36325 ->36325;
excluded integration6840 ->6840. Receipt22 pins exact parent/staged snapshots
using the unchanged production counter. No transport or reference retirement
occurs. The measured sample remains pinned to `c5fae7e3a`; this implementation
is not relabelled as measured evidence.

# #232 frozen-v2 POSIX shift count diagnostic

**Outcome:** both newly labelled count diagnostics completed through public
SDK Exec → ordinary FUSE WRITE/SETATTR → Commit, independent full-file
verification and cleanup. Both numeric rows remain **INELIGIBLE** for cold
latency under uncontrolled Linux FUSE backing cache. This is cause-finding
evidence, not another performance sample of the historical v2 arms and not a
new 56-case qualification.

## Frozen identity and custody

The [prospective contract](CONTRACT.md) preceded diagnostic instrumentation.
The [pre-run seal](PRE_RUN.json) preceded both live attempts. Execution source
was `5a1b8e9f2208cccb939aa035d869bab20a859239`, product seal
`d125b8b35848b84288b30b954153419e33c2c9cf2fadf62876305279ee051e0f`,
harness seal `3c51e92850f6a66033bec48838e130d29eaf8db6cb1d18a856c95328882f840a`,
locked release image
`sha256:a925ad7893bf64b0d854e28ad2dd827dc9febbb6710c3e5cbf22d6e044c5e88c`.
The release SDK driver SHA-256 is
`1278794213c25ba78abaac8d553b4587ed9744657c00fe451d2a29c57797ecb0`;
the exact archived Init and verifier binary hashes and both validated closed
master hashes are in `PRE_RUN.json`. Each run received an independent writable
byte copy outside the timer. The v4 complexity image builder supplied the
unchanged v2 insert-middle payload with a matching SHA-256; the executed
command remained the frozen v2 **POSIX shift**, with no ioctl. One
construction worker and one attempt per selected case were used.

The complete retained [1 MiB](attempt-01/1mib/receipt.json) and
[10 MiB](attempt-01/10mib/receipt.json) receipts, raw caller and daemon LFT1,
piece-count logs, SDK output, independent verifier output, exact case specs
and per-file SHA-256 manifests are under `attempt-01/`. Store/history bytes
remain at the ignored local paths in [POST_RUN.json](POST_RUN.json), which
records their sizes and hashes. [derive.py](derive.py) checks all copied raw
hashes, the frozen source/product/harness seals, callback route, LFT1 root
and dropped-record count, full-file oracle, historical root and cleanup, and
reproduces [derived.json](derived.json). The selected output directories were
fresh; there was no replay or resample.

## POSIX callbacks and piece work

| Pristine size; suffix moved | FUSE READ / WRITE / SETATTR | Piece events; final pieces | Sum of old pieces loaded / summary visits | Piece-index pages written | Raw LFT1 Exec→Commit | Complete command |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 MiB; 0.5 MiB | 8 / 9 / 1 | 10; 10 | 51 / 60 | 10 | 127.127 ms | 2.762 s |
| 10 MiB; 5 MiB | 80 / 81 / 1 | 82; 82 | 3,363 / 3,444 | 144 | 2,884.365 ms | 3.752 s |

Status reported `range_state=0`, `range_edit=0`, `rename=0` in both rows.
The 128 KiB editor blocks became two FUSE reads and two writes each at this
mounted kernel/driver route; the final 4 KiB replacement added one WRITE.
The size extension added one SETATTR. The daemon emitted one
`LFS_PIECE_COUNT` and one `LFS_PIECE_PAGES` line for every successful
SETATTR/WRITE. Moving ten times as many suffix bytes took nine times as many
WRITE callbacks, but **66 times as many old-piece loads** and **14.4 times as
many piece-index pages written**. These are per-callback work counts from
the live ordinary POSIX route. They directly support the source finding that
each callback reloads, splices and rebuilds the growing piece list. The raw
LFT1 walls are shown for diagnosis only; cache qualification does not permit
a latency PASS or a speedup/regression ratio.

## Commit substep scope

The new `LFS_FINISH_SUBSTEP v=1` line exposes existing
`SaveOutcome.profile.diag` totals for the EditFile save. It labels them
**save-wide**. They overlap, so they cannot be added. `finish_drain_ns` is the
final drain alone; the other totals can include earlier flush waves.

| EditFile save | Final drain | Save-wide `flush_batch` | Final-batch objects | Whole-save inserted + reused | Safe attribution |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 MiB | 3.352 ms | 3.342 ms | 30 | 5 + 25 = 30 | Every accepted object was in the final batch. The save-wide flush total is the final flush here. |
| 10 MiB | 3.428 ms | 15.677 ms | 65 | 10 + 268 = 278 | Earlier waves contributed; the 15.677 ms total is **not** the final drain. Its nested wave/offer/seal totals cannot be assigned to the final drain. |

The 1 MiB save-wide wave, offer, seal, pack-write and validation totals were
0.555, 0.776, 1.308, 0.261 and 0.045 ms, respectively. They nest or overlap
and are not an additive breakdown. The 10 MiB row does not isolate these
parts of its final 3.428 ms drain. Neither row answers the separate
1-versus-500 MiB [Phase 1B batch-drain growth](../../../241/evidence/phase1b-finish-diagnostic/REPORT.md),
and no narrow Commit optimization follows from these two counts. A later
diagnostic must capture a before-finish profile snapshot or instrument only
the final flush under a new prospective identity; these attempts are not
rerun to fill the missing attribution.

## Verification and remaining gates

Both independent verifiers passed a full-file SHA-256 and retained pristine
root check; both Status/Unmount/public Sandbox Delete results passed. Caller
LFT1 roots were complete with zero dropped or overflowed records. Both
complete commands met 15 s. Both verifiers returned before their separate
10 s subprocess timeout, but this diagnostic runner omitted their **exact
wall time**; `POST_RUN.json` records it as `UNAVAILABLE` rather than inventing
a number. Cache state remained uncontrolled, so both rows are performance
`INELIGIBLE`. No Docker container with the diagnostic name remained.

The count result supports work on a path-local piece index for ordinary
WRITE/SETATTR. It does **not** remove the frozen v2 shifts' 8 MiB replay
limit or the 500 MiB fresh-file 1,024-piece limit. Those Phase 3 design gates
remain open in the [five-checkpoint route report](../../SHELL_ROUTE_CORRECTION.md).
The owning Core checks at the diagnostic source passed: `cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked --offline`, `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings`,
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`, and
`python3 core/tools/check_product_boundary.py`. There was no CI or aggregate
preflight run.

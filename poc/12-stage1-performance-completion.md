# Stage One — Performance Completion Authority

Status: **implemented and closed for this PoC with one accepted performance exception**
Source baseline: `e643667` (`perf: instrument Apple PoC operations`)
Scope: non-mounted Apple/APFS PoC; correctness first; implementation before measurement

Final disposition and current-source custody are recorded in
[17-stage1-closure.md](17-stage1-closure.md). The A01–A17 campaign remains
`REVISE` because A02 missed its frozen p50/p95 targets; the user explicitly
accepted that miss for this PoC. This acceptance closes Stage One without
changing the thresholds or fabricating a measured PASS.

This document controls Stage One when older PoC pages describe a route as
complete but the current product source does not expose that route. It does
not change canonical object bytes, the fresh profile, publication durability,
or the ordinary-APFS support boundary.

## 1. Outcome

```text
Stage One input
  current correct AppleWorkspaceV1
  + persistent extent/inode/namespace B+ structures
  + immutable roots/history/publication

Stage One output
  direct canonical read/write/edit
  + explicit Store-lifetime integrity policy
  + reusable managed workspace
  + exact managed no-op
  + changed-root A -> B refresh
  + reduced redundant SQLite/APFS work
  + operation and realistic-workspace evidence
```

Stage One does **not** add a mount, FUSE, FSKit, File Provider, watcher,
asynchronous projector, background worker, online GC, pack format, remote
store, or new dependency.

## 2. Current disposition

| Surface | Current state at `e643667` | Stage One gate |
|---|---|---|
| Canonical extent/namespace/inode codecs | Implemented and tested | Keep bytes unchanged |
| Persistent local rope edit | Implemented; `O(B + log E)` shape | Expose through SDK and prove counters |
| Direct canonical range/stream read | Core exists; VFS/SDK route missing | Implement |
| Direct logical write/edit | Product route missing | Implement streamed write and splice |
| Public current head | Cached at open and not advanced after capture/rollback | Query/rotate exact accepted ref state |
| `Verified` | Engine and publication implemented | Remains default |
| `TrustedLocalDev` | Engine policy exists; Apple/SDK hard-code `Verified` | Expose explicitly for Store lifetime |
| Managed workspace | Materializes and edits, then capture destroys it | Keep live after durable checkpoint |
| Managed exact-root no-op | Missing; current row scans and CDCs all bytes | Zero payload read/CDC/write |
| Changed-root refresh | Missing | Merkle-diff and apply changed paths |
| External Bash/editor capture | Correct full namespace/file scan | Retain honest linear route |
| Cold materialization | Correct, but duplicate walks/syncs | Remove redundant work only |
| Reopen | Duplicate engine admission/open and unconditional parent sync | One clean admission/open |
| One fetch/auth/decode | Not true end-to-end | Repair shared borrowed-read boundary |
| Operation counters | Selected SQL and canonical counters only | Cover actual canonical/native/scratch work |
| External unchanged-file reuse | Capture CDC-builds every file; history-shaped file roots can churn | Digest-compare and reuse prior file/inode roots |
| Canonical parent/child delta | V3 product publication has no meaningful durable V3 transition authority | Explicitly defer; root diff is Stage One authority |

## 3. Non-negotiable invariants

```text
CAS + frozen FastCDC 8/16/32 KiB + immutable COW
fresh FileStateV3 writer; legacy reads only
canonical extent/namespace/inode/metadata identities unchanged
fetched + new + incumbent identity authentication unconditional
Verified default
TrustedLocalDev explicit and fixed for Store lifetime
Verified reopen after Trusted history scrubs
expected head + one writer transaction + one visibility COMMIT
fresh reconciliation after ambiguous publication outcome
SQLite DELETE/FULL/FILE/mmap=0/busy=0; no WAL/retry/pool
bounded streaming buffers; no source-sized Vec
old roots remain readable; fork/rollback copy zero payload bytes
ordinary APFS external capture remains exact and linear
```

## 4. Stage sequence

| Stage | Implementation | Smallest proof | Measurement allowed? |
|---|---|---|---:|
| S1.0 | Correct current claim vocabulary; bind source baseline | Existing focused tests | No |
| S1.1 | Shared authenticated borrowed/batched object-read path and complete counters | Core/engine focused tests | No |
| S1.2 | Direct canonical range/stream read, streamed write, logical splice, integrity-mode API | SDK route tests on small fixtures | No |
| S1.3 | Repeatable managed checkpoint and live authority rotation | 2, 10, 100 edits without rematerialization | No |
| S1.4 | Remove duplicate materialization authority pass, proven redundant syncs, duplicate reopen | Fault/reopen and metadata tests | No |
| S1.5 | Managed exact no-op and inode/directory/rope root diff refresh | Same-root zero-work and A→B exact tests | No |
| S1.6 | Freeze evaluator, deterministic masters, counters and equations | Zero-row readiness | Yes |
| S1.7 | 100 MiB single-file campaign | Preferred `<60 s`; hard `<=120 s` | Once |
| S1.8 | `<=300 MiB` workspace campaign | Preferred `<60 s`; hard `<=120 s` | Once |

No full campaign runs before S1.6. No unchanged rerun for favorable noise.

## 5. Required public behavior

```rust
LayerFs::open(path)                         // Verified default
LayerFs::open_with_integrity(path, mode)   // explicit Store-lifetime mode
LayerFs::current_head(ref_name)             // fresh RefState, not cached root
LayerFs::read_range(root, path, range, out)
LayerFs::read_to(root, path, out)
LayerFs::replace_range(expected_ref_state, path, start, delete_len, input)
LayerFs::replace_file(expected_ref_state, path, input)
// mutators return the newly accepted RefState

ManagedWorkspace::checkpoint()             // publish, retain native workspace
ManagedWorkspace::ensure_exact(target)     // true no-op when authority matches
ManagedWorkspace::refresh(target_ref_state)// A -> B; align named ref first
ManagedWorkspace::discard()                // explicit terminal cleanup
```

Stage One publicly supports the named `main` ref first. Every mutator carries
and compares its exact `RefState { name, generation, root }`; a root ID alone
is not expected-head authority.

Compatibility rule:

```text
existing terminal capture
  either remains terminal under its old name
  or is migrated once with all callers/tests updated

new repeated publication
  uses checkpoint()
```

Do not overload one method with both lifecycle meanings.

## 6. Complexity contract

Symbols:

```text
F      complete file bytes
B      supplied replacement bytes
E      extents in a file
C_R    extents intersecting a range
R      returned bytes
I      inode-table entries
D_i    entries in path component directory i
d      path depth / number of inode-record lookups
P      workspace paths
U      unique regular-file bytes in an external workspace
H      relevant persistent-tree height
S      native contiguous suffix bytes moved by a count change
```

| Operation | Required canonical work | Allowed native work |
|---|---|---|
| Point/range read by path | `O(sum_i(log D_i + log I) + log E + C_R + R)` | `0` |
| Full logical reconstruction by path | path lookup + `Theta(F)` | `0` |
| Streamed write/import | `Theta(F)` | External input read `Theta(F)` |
| Same-size logical edit | `O(B + log E + sum_i(log D_i + log I))` | `0` |
| Insert/delete logical edit | `O(B + log E + sum_i(log D_i + log I))` | `0` |
| Managed same-size native edit | Canonical local work above | `O(B)` patch after retained authority |
| Managed count-changing native edit | Canonical local work above | Explicit `Theta(S+B)` shift or `Theta(F)` full fallback |
| Managed exact no-op | `O(1)` authority/root/generation checks | zero payload/native bytes |
| Locally derived A→B refresh | shared-identity changed spines/paths + changed bytes | clone/patch or per-file fallback |
| Arbitrary/unrelated A→B refresh | worst case `Theta(nodes(A)+nodes(B))` | only classified changed paths applied |
| Cold materialization | `Theta(F) + O(P log I + namespace/metadata visits + name preflight)` | linear output + path metadata |
| External capture | current digest + changed CDC reread + uncached prior stream + represented metadata + indexed grouping | complete exact scan |
| Clean reopen | metadata/ref admission | no full native scan |
| Root switch/fork/rollback | indexed ref work | zero payload copy |
| Offline compaction | retained-union graph and surviving bytes | maintenance-only linear work |

Hard resource bounds:

```text
largest product stream buffer <= 1 MiB
operation Q high-water         <= 8 MiB
operation Q terminal           = 0
no all-extents Vec
no complete namespace in memory
one writer + at most two query readers
small-PoC RSS diagnostic       <= 64 MiB
```

The prior 32 MiB small-PoC target remains an optimization goal, not a reason
to hide actual RSS. The 64 MiB Stage One diagnostic is a prospective bound for
the larger evaluator and does not weaken the 8 MiB owned-operation bound.

## 7. Performance targets

Targets are prospective gates for the frozen Stage One source, not current
results.

Current Apple PoC diagnostic at 4 MiB (`poc/11`, source `e643667`):

| Operation | Current median | Decisive work |
|---|---:|---|
| Import capture | `38.54 ms` | read + CDC `4,194,592 B` |
| Cold materialize | `99.85 ms` | native write `4,194,335 B` |
| Labeled warm exact | `35.83 ms` | still read + CDC the complete file |
| 4 KiB overwrite + publish | `132.58 ms` | fresh workspace lifecycle dominates |
| 8 KiB insert + publish | `125.79 ms` | `1,040,384 B` suffix shift |
| 4 KiB delete + publish | `127.91 ms` | `1,036,288 B` suffix shift |
| External capture | `41.81 ms` | complete external scan |
| Reopen | `7.68 ms` | duplicate clean admission/open remains |

These rows diagnose current owners; they are not extrapolated acceptance
measurements for 100 MiB.

| Operation | Current useful anchor | Stage One target | Expected change |
|---|---:|---:|---:|
| 1 MiB canonical range read | G5 `315 MiB/s` | `>=250 MiB/s` | restore direct route |
| 100 MiB logical reconstruction | G5 `231–235 MiB/s` | `>=200 MiB/s` | about `0.50 s` max |
| 100 MiB streamed durable import | G5 `156 MiB/s` | `>=150 MiB/s` | `<=0.67 s` payload path |
| Trusted 4 KiB logical edit | G5 `7.9–9.4 ms` | p50 `<=15 ms` | G5-class locality |
| Verified 4 KiB edit | PoC composite `~133 ms` | no material regression; exact work reported | integrity route, not hidden |
| Reopen/head ready | PoC `7.68 ms` | p50 `<=4 ms` | `1.9–2.6×` |
| Managed exact no-op | PoC `35.83 ms/4 MiB scanned` | `<=5 ms`, zero scan/write | `>=7×` at old size |
| 100 MiB cold native materialization | PoC normalized `~40 MiB/s` at 4 MiB | `>=150 MiB/s` large-file fixture | `~3.7×` throughput class |
| Locally derived 100 MiB A→B, one 4 KiB overwrite | missing | `5–25 ms` | `~20–100×` vs full rebuild |
| External capture | PoC `96–104 MiB/s` at 4 MiB | planning `100–160 MiB/s`; no hard throughput gate | exact multi-pass work; still linear |

Many-small-file workspace materialization is reported in both MiB/s and
files/s. No aggregate `150 MiB/s` promise applies to thousands of fsync-heavy
small files.

## 8. Acceptance matrix

| Gate | PASS condition |
|---|---|
| Bytes | Independent oracle matches every output |
| Identity | Old/new roots and every fetched/new/incumbent object authenticate |
| History | Retained roots read directly; no replay |
| Publication | State change has one writer transaction/COMMIT; no-op has zero |
| Read locality | Direct range creates no workspace and writes no native bytes |
| Edit locality | Replacement CDC `<=B`; unaffected canonical suffix reads/writes `0` |
| Fetched decode | fetched rows = fetched-row auth passes = fetched-row role decodes |
| Admission auth | new-object and incumbent auth passes reported separately |
| Batching | ordered payload batch maximum `<=64` |
| Managed reuse | 100 checkpoints after one materialization; rematerializations `0` |
| Managed no-op | payload/native read, CDC, native write, COMMIT all `0` |
| Refresh | Exact B; unchanged paths untouched; fallback files named |
| External capture | Full scan is labeled and completely byte-accounted |
| Resources | buffer/Q/RSS/FD/connection/temp bounds pass; terminal zero/baseline |
| Time | Each campaign preferred `<60 s`, hard `<=120 s` |

Mode-separated edit work:

```text
Trusted live edit
  = path resolution + supplied B + changed rope/inode work

Verified edit without an authenticated transition receipt
  = Trusted work + Theta(visible root closure)

Verified-after-Trusted reopen
  = unique retained marking + required per-root/context validation
```

Only the Trusted live path has the local-edit latency target. Verified remains
exactly measured and must not be described as `O(B)` until its authority work
actually changes.

## 9. Explicit limitations after Stage One

```text
No transparent POSIX mount.
No local arbitrary-editor capture without a full scan.
No claim that APFS shifts a contiguous middle suffix for free.
Different-length native refresh may rebuild the changed file.
Cold materialization remains linear in requested output.
Verified-after-Trusted scrub remains linear in reachable authority.
Offline compaction remains linear maintenance work.
No new canonical parent/child delta format; Stage One derives root diffs.
```

These limitations do not invalidate the extent tree. Direct logical
read/edit, immutable history, fork/rollback, same-size refresh, and bounded
changed-path reconstruction remain useful without a mount.

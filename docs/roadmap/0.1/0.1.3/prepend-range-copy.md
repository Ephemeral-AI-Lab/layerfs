# Prepend and range copy

## Status

Draft v0.1.3 family contract: 6 timed scenarios and 7 proof-only scenarios.
`prepend-temp-copy-rename` retains its registered meaning; every other ID is
unregistered and remains compatibility-gated.

## Problem statement

The registered prepend row proves exact bytes and missing-only canonical
storage for one ordinary prepend, but the ordinary helper transfers the source
payload through FUSE. There is no frozen load curve comparing repeated ordinary
prepend with a standard range-copy expression, and edge semantics are not yet
captured as stable proofs.

## Goal

Keep the existing one-operation prepend anchor unchanged. Add nested 10- and
100-operation prepend rows, parallel 1/10/100 range-copy rows, and seven
proof-only semantic cases. A range-copy optimization is acceptable only when it
produces the ordinary-copy canonical result and safely falls back when it
cannot do so.

## Files to read

- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 parent plan](README.md)
- [v0.1.2 range-copy proposal](../0.1.2/copy-file-range-prepend.md)
- [`fs-bench-pro` campaign](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing prepend helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [FUSE protocol](../../../../crates/layerfs-fuse/src/protocol.rs)
- [FUSE proxy host](../../../../crates/layerfs-fuse/src/proxy_host.rs)
- [Workspace file I/O](../../../../crates/layerfs-workspace/src/file_io.rs)

## Fixed topology and lifecycle boundary

- Every timed row owns a fresh Store, Client, the existing 32 MiB
  `payload.bin`, genesis Layer, Branch, real-FUSE Workspace, fresh process, and
  evidence directory.
- Fixture import, Store/Client/container preparation, source sealing, and
  report writing are excluded and recorded separately.
- One prepend operation writes the existing ten-byte `PREPEND010` marker to a
  temporary file, transfers the complete current payload after it, syncs, and
  atomically renames the temporary over `payload.bin`.
- Ordinary prepend rows retain the current userspace stream-copy expression.
  Range-copy rows express the payload transfer with Linux `copy_file_range`,
  loop over partial completions, then use the same sync and rename boundary.
- One fresh process performs all operations declared by a row. The timer covers
  Workspace Create, the process, one final Commit and visibility, clean End,
  fresh reconnect/verification, and cleanup.
- A timed range-copy row must report the optimized operation; it cannot silently
  become the unsupported-fallback proof.

## Timed scenarios

| Scenario ID | Status | Load | Exact timed operation | Required oracle |
| --- | --- | --- | --- | --- |
| `prepend-temp-copy-rename` | Registered; frozen | 1 ordinary prepend | Existing complete 32 MiB lifecycle, unchanged | Existing 32 MiB + 10 byte digest/root/reopen proof |
| `prepend-10` | Draft | First 10 ordinary prepend operations | One process, 10 temp-copy-rename operations, one Commit | 100 marker bytes followed by original payload |
| `prepend-100` | Draft | First 100 ordinary operations | Same lifecycle with 100 operations | 1,000 marker bytes followed by original payload |
| `range-copy-1` | Draft | 1 range-copy prepend | One process, one supported range-copy transfer, one Commit | Canonical root equals ordinary one-operation prepend |
| `range-copy-10` | Draft | First 10 range-copy prepend operations | One process, 10 supported transfers, one Commit | Canonical root equals `prepend-10` oracle |
| `range-copy-100` | Draft | First 100 range-copy prepend operations | One process, 100 supported transfers, one Commit | Canonical root equals `prepend-100` oracle |

## Proof-only scenarios

Proof cases run once per candidate campaign. They have no latency distribution
or row gate, but their complete walls count toward the family budget.

| Proof ID | Exact operation | Required proof |
| --- | --- | --- |
| `range-copy-proof-partial` | Copy a fixed proper subrange between different files with nonzero source and destination offsets | Completed length, offsets, bytes, root, and reopen match ordinary copy |
| `range-copy-proof-same-file-nonoverlap` | Copy between two non-overlapping ranges of one file | Exact ordinary same-file result; all bytes outside the destination remain unchanged |
| `range-copy-proof-overlap-fallback` | Request an overlapping same-file copy | Documented safe error or ordinary-copy fallback; never silent corruption |
| `range-copy-proof-copy-overwrite` | Range copy, then overwrite a strict destination subrange | Later write wins exactly after Commit and reopen |
| `range-copy-proof-copy-truncate` | Range copy, then truncate inside the copied destination | Exact final length, bytes, extent closure, root, and reopen |
| `range-copy-proof-interrupted` | Interrupt after partial transfer before publication | No visible partial Commit and no leaked process, mount, spool, Workspace, or lease |
| `range-copy-proof-unsupported-fallback` | Force helper/kernel capability rejection | Ordinary byte-copy fallback produces the exact canonical result and records fallback |

## Tier/load rule and deterministic schedule

The shared multiplier is `a = 10`; this family's primary load unit is one
prepend/range-copy operation. For each expression, the 1- and 10-operation
schedules are exact prefixes of the 100-operation schedule. Every operation
uses the frozen `PREPEND010` marker, so the final oracle is:

```text
PREPEND010 repeated N times || original 32 MiB payload
```

The existing one-operation ordinary row remains the tier-1 anchor with no
semantic or timing-boundary change. The range-copy tiers use the same source,
marker, temporary-file, sync, rename, Commit, and proof boundaries.

There is no random workload in this family. Candidate evidence nevertheless
uses three fresh samples, labeled exactly:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

The labels identify fresh evidence custody; they do not alter the frozen
payload or operation sequence.

## Required metrics and oracles

Record per timed row and proof where applicable:

- complete wall and Workspace Create, process, Commit, visibility, End,
  reconnect/verification, and cleanup wall;
- prepend/range-copy calls, requested/completed bytes, partial completions,
  fallbacks, and errno/result distribution;
- FUSE read/write/range operation counts and payload bytes transferred across
  kernel, proxy, and Workspace boundaries;
- dirty and borrowed range counts/metadata bytes, spool bytes, process
  user/system CPU, peak RSS, cgroup peak, and swap;
- capture, candidate, content, namespace, admission, publication, and rebase
  phase walls;
- candidate, inserted, and reused canonical objects/bytes, transaction maxima,
  and Store semantic/allocation growth; and
- exact final length, SHA-256, canonical root equality with ordinary copying,
  fresh reopen, and cleanup state.

An unavailable transfer/fallback field is an evidence error. Timed range-copy
rows fail if they silently use the proof-only fallback path.

## Expected-rate assumptions and family budget

Use the shared planning model:

```text
0.5 s
+ sequential_payload_MiB / 100
+ paths / 10,000
+ same_count_edits / 100
+ count_changing_edits / 50
```

The fixed 0.5 s component covers Create, Commit, End, fresh reopen,
verification, and cleanup. Ordinary payload transfer is expected to sustain at
least 100 MiB/s; prepend is a count-changing edit expected to sustain at least
50 operations/s once unchanged payload transfer is avoided. The namespace and
same-count terms are not governing loads here.

Candidate evidence is three fresh samples per timed row plus each of the seven
proof cases once. The family wall sums all 18 timed-row walls and all 7 proof
walls. Environment/fixture preparation is excluded and recorded separately.
The budgets are candidate gates, not claims about the unoptimized baseline.

- Target family wall: **20 seconds**.
- Hard family wall: **40 seconds**.

## Acceptance criteria

- [ ] Run exactly 6 timed IDs and 7 proof-only IDs.
- [ ] Preserve the existing prepend row's operation, fixture, timing, result
  schema, and registered meaning.
- [ ] Prove each 1/10/100 schedule is a nested operation prefix.
- [ ] Prove ordinary and supported range-copy rows produce identical bytes and
  canonical roots at every tier.
- [ ] Prevent silent fallback in timed range-copy rows and prove explicit safe
  fallback separately.
- [ ] Pass all partial, same-file, follow-up mutation, interruption, and
  unsupported-helper proofs with exact reopen and cleanup.
- [ ] Demonstrate materially reduced payload transfer for the supported
  range-copy expression without changing the 0.1.x canonical format.
- [ ] Retain three fresh timed samples, each proof once, and every required
  transfer, resource, Store, and Commit receipt.
- [ ] Meet the 20 s target and never exceed the 40 s hard family wall.
- [ ] Move incompatible protocol or canonical changes out of v0.1.x.

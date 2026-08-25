# LayerFS Apple/APFS PoC v1

Status: **AppleWorkspaceV1 Stage 1.0 and the Stage 1.1 correctness/performance
baseline are closed; Stage 1.1M materialization optimization and Stage 1.2
remain prospective**.
The controlling closure and custody record is
[17 — Stage 1.0 closure and accepted A02 exception](17-stage1-closure.md).
The preserved A01–A17 campaign remains measured `REVISE`; its A02 latency miss
is explicitly user-accepted and is not relabeled as a threshold PASS. Stage
1.1 passed its source-bound 47-row/51-edit/34-transition campaign in
13.517581334 seconds; its controlling result is recorded in `poc/16` and
`poc/evidence/stage1.1-apple-edge-20260825`. Later audit preserved those results
but found incomplete full-materialization SQL/native attribution. The separate
portable repair and target authority is `poc/20`. Stage 1.2 is the
npm/developer-workspace specification in `poc/15`. The prospective post-Stage-1
LayerFS + Linux FUSE direct workspace is specified in `poc/19`, with Docker
only as its Linux execution envelope. macFUSE and FSKit are not selected
dependencies.

```text
Stage 1.0  implemented product baseline and A01–A17 closure
Stage 1.1  poc/16 single-file APFS edge benchmark — PASS / closed
Stage 1.1M poc/20 portable full-materialization attribution and optimization
Stage 1.2  poc/15 npm/developer-workspace benchmark
Stage 2    poc/19 LayerFS + Linux FUSE direct workspace after Stage 1.2
```

The PoC collapses the remaining G6 and project Phase 5–8 work into one vertical
delivery:

```text
canonical payload CAS
  -> persistent measured file structure
  -> persistent measured namespace structure
  -> SQLite root publication
  -> ordinary APFS workspace
  -> Bash/editor or managed edit capture
  -> minimal SDK
  -> exact rematerialization
```

It retains internal dependency order, but rejects separate versioned evidence
programs for each small implementation repair.

## 1. Document map

| Document | Purpose | Primary implementation owner |
|---|---|---|
| [00 — Scope and decisions](00-scope-and-decisions.md) | Frozen PoC boundary, selected mechanisms, evidence discipline, success definition | Whole PoC |
| [01 — Architecture and file structure](01-architecture-and-file-structure.md) | Target repository tree, crate/module ownership, dependency rules, extraction/deletion map | All crates |
| [02 — Data structures and algorithms](02-data-structures-and-algorithms.md) | Exact extent/node models, canonical codecs, operations, validation, complexity | `layerfs-core` |
| [03 — Operation workflows](03-operation-workflows.md) | Create/read/edit/capture/history/fork/rollback state machines | Core + engine + VFS |
| [04 — Apple/APFS materialization and recovery](04-apple-apfs-materialization-and-recovery.md) | Native correctness path, clone/patch option, atomic publication, restart | `layerfs-os`, `layerfs-vfs` |
| [05 — Minimal implementation plan](05-minimal-implementation-plan.md) | One vertical work sequence, extraction order, budgets, stop rules | Whole PoC |
| [06 — Correctness and fast verification](06-correctness-and-fast-verification.md) | Model tests, goldens, fault/restart checks, structural counters, small benchmark | Tests + evaluator |
| [11 — Apple PoC operation profile](11-operation-profile.md) | Per-operation wall, native routes, byte amplification, measured bottlenecks | Release evaluator |
| [07 — Implementation checklist](07-implementation-checklist.md) | Cross-document executable checklist and completion ledger | Whole PoC |
| [08 — Native workspace and shell verification](08-native-workspace-and-shell-verification.md) | Ordinary-file workspace contract, Bash lifecycle, supported filesystem subset, and realistic command corpus | SDK + VFS + evaluator |
| [09 — Portability and Apple completeness](09-portability-and-apple-completeness.md) | Universal projection-driver boundary, canonical inode/metadata model, Apple/APFS completeness profile, and cross-platform rule | Core + VFS + OS |
| [10 — Handoff freeze](10-handoff-freeze.md) | Final independently audited authority for codecs, inode allocation, drivers, metadata, storage reads, capture, durability and compaction | Implementation handoff |
| [12 — Stage One performance completion](12-stage1-performance-completion.md) | Current disposition, narrowed scope, sequence, complexity, targets and acceptance gates | Stage One authority |
| [13 — Stage One implementation and complexity](13-stage1-implementation-and-complexity.md) | Exact resulting tree, files to edit, route algorithms, counter proofs and tests | Product implementation |
| [14 — 100 MiB operation campaign](14-stage1-single-file-benchmark.md) | Fast deterministic read/write/edit/reconstruct/materialize/refresh/reopen campaign | Single-file evaluator |
| [17 — Stage 1.0 closure and accepted A02 exception](17-stage1-closure.md) | Implemented baseline disposition, current-source custody, measured PASS gates, and explicit A02 waiver | Stage 1.0 closure |
| [16 — Stage 1.1 single-file APFS edge specification](16-stage1-part1-apple-edge-benchmark.md) | One 24 MiB file; 47 rows; bidirectional edits, save bursts, checkpoints, history and refresh under 60 s | Stage 1.1 evaluator |
| [18 — Stage 1.1 result and handoff template](18-stage1.1-result-template.md) | Exact summary tables, JSONL row schema, availability rules, timer receipt and final response | Stage 1.1 evaluator |
| [20 — Stage 1.1M portable full-materialization optimization](20-stage1.1-full-materialization-optimization.md) | Guarded Verified reads, derived scratch, portable native facts, fixed/stream attribution, CPU/memory gates and 0/24/96 MiB targets | Engine + VFS + OS + evaluator |
| [15 — Stage 1.2 npm/developer-workspace specification](15-stage1-workspace-benchmark.md) | Reusable <=300 MiB offline npm/build/search/capture workflow | Stage 1.2 evaluator |
| [19 — Stage 2 LayerFS + Linux FUSE workspace](19-stage2-docker-linux-fuse.md) | Post-Stage-1 container admission, direct mounted read/write/locality proof, real workspace gates, and strict exclusions | LayerFS VFS + thin Linux FUSE adapter |

## 2. Reading order

```text
00 scope/decisions
  -> 01 module/file ownership
  -> 02 data structure and algorithms
  -> 03 logical workflows
  -> 04 native APFS boundary
  -> 05 implementation order
  -> 06 verification
  -> 07 execution checklist
  -> 08 ordinary workspace/Bash contract
  -> 09 portability/Apple completeness gate
  -> 10 final handoff freeze
  -> 11 measured current operation profile
  -> 12 Stage One controlling scope/targets
  -> 13 exact implementation/complexity audit
  -> 14 100 MiB operation campaign
  -> 17 Stage 1.0 implementation closure and custody
  -> 16 Stage 1.1 single-file APFS edge specification
  -> 18 Stage 1.1 result and handoff template
  -> 20 Stage 1.1M portable full-materialization optimization
  -> 15 Stage 1.2 npm/developer-workspace specification
  -> 19 Stage 2 LayerFS + Linux FUSE direct workspace
```

Do not begin with the benchmark. Implement and model-check the selected
algorithm first.

## 3. Sixty-second architecture

```text
                                 immutable authority
                                         |
                                         v
Managed edit bytes -> FastCDC -> payload CAS objects
                         |               |
                         |               v
                         +-------> measured extent slices
                                         |
                                         v
                              persistent B+ file state
                                         |
                   persistent namespace --+
                              |          delta
                              v           |
                         new root <--------+
                              |
                              v
                SQLite expected-head publication
                              |
                one writer transaction / one COMMIT
                              |
                              v
                      immutable visible root
                              |
              +---------------+----------------+
              |                                |
              v                                v
       virtual/range read               APFS materialization
                                         exact/no-op
                                         clone + patch
                                         full stream fallback
```

## 4. Selected PoC mechanisms

| Concern | PoC decision | Not selected in this PoC |
|---|---|---|
| Payload identity | Existing canonical immutable CAS object identity | Host inode, SQLite row ID, path-derived identity |
| Chunking | Existing fixed FastCDC 8/16/32 KiB profile | Another CDC tournament |
| File state | Fresh versioned profile using a persistent byte-measured B+ extent rope | K64/F64 for new file state; CD32–64 as the product representation |
| Extent | Slice of an immutable payload object: object ID, source offset, length | Copying a payload merely to split a logical extent |
| File authority | Operational `FileStateRoot`; optional/lazy semantic `ContentDigest` | Mandatory complete-file digest on every bounded edit |
| Directory state | Persistent byte-bounded canonical B+ tree per directory | Full `BTreeMap` clone/hash on every namespace mutation |
| Durable engine | SQLite rollback journal, expected head, one writer transaction, one publication COMMIT | WAL, hidden retry, new carrier, PostgreSQL |
| Integrity | `Verified` default; explicit Store-lifetime `TrustedLocalDev` | Benchmark-only policy switch or authority laundering |
| Projection boundary | OS-neutral VFS `ProjectionDriver`; Apple/APFS implementation only in `layerfs-os` | Apple syscalls or platform `cfg` in core/engine/VFS |
| Native target | Ordinary private APFS directory usable by Bash, editors, compilers, and normal file APIs | Claiming the canonical store itself is an ordinary directory |
| Same-size projection | Optional verified parent clone plus bounded patch | Clone as correctness authority |
| Length-changing projection | Exact streaming fallback for ordinary native file; logical edit remains local | Pretending APFS shifts a contiguous suffix for free |
| Capture | Managed exact-range fast path; arbitrary external-editor capture walks and scans the complete supported workspace | Inferring complete changed paths/ranges from mtime/FSEvents |
| History | Immutable roots and named refs; Merkle root diff derives changes; a new canonical V3 delta is deferred | Deep replay stack for ordinary reads |
| Rollback | Expected-head ref/root movement to an already verified immutable root | Destructive mutation of old roots |
| Compaction/GC | Explicit offline exclusive mark-copy-verify-swap; no background deletion | Online/in-place/background GC |
| Public API | `open`, `materialize_managed`, `materialize_external`, managed/external `capture`, `discard`; managed edit surface may be PoC-only | Writable-path leakage from managed authority; backend registry/plugin framework |

## 5. Evidence discipline

Every nontrivial statement in this package must use one of these classes:

| Class | Meaning | May authorize implementation? |
|---|---|---:|
| **Current source** | Directly present in the checked-out Rust source and tests | Yes, for current behavior only |
| **Accepted evidence** | Recomputable controlling raw evidence with intact custody | Yes, within its exact population and labels |
| **Derived** | Arithmetic or complexity derived from stated inputs | Only as an implementation hypothesis |
| **Proposal** | Selected PoC design not yet implemented | Yes, but never reported as a result |
| **Unavailable** | Required fact not observed or exposed | No; choose a fallback or add a test |

Existing architecture pages, research reports, preregistrations, benchmark
conclusions, and this package are not evidence merely because they are
detailed. Source and accepted raw artifacts control factual claims.

## 6. Correctness hierarchy

```text
canonical bytes and identities
  > exact logical byte result
  > retained historical-root readability
  > atomic durable publication
  > bounded memory/storage ownership
  > structural complexity counters
  > wall-clock performance
```

A fast wrong root is a failure. A correct operation with an unbounded resource
path is also a failure. A correct bounded implementation may use an honest full
fallback until a supported local route is proven.

## 7. Required end-to-end proof

The PoC is useful only when this works through the actual public/internal
product modules:

```text
open a fresh Store
  -> create or import an immutable root
  -> materialize it into an APFS directory
  -> run a real `/bin/bash` command sequence in that directory
  -> perform managed and ordinary native edits, mode changes, create/remove/rename and symlink operations
  -> wait for every LayerFS-owned/registered child and attest cooperative quiescence
  -> capture to a new immutable root
  -> close and reopen the Store
  -> materialize old and new roots independently
  -> compare both native trees byte-for-byte and metadata-for-metadata
  -> fork a root reference
  -> advance one fork and retain the other
  -> roll a reference back to an old verified root
  -> prove every retained root still reads exactly
```

## 8. Real-operation coverage

| Operation | PoC route required | Honest lower bound / fallback |
|---|---|---|
| Empty/tiny/full create | CDC + CAS + rope build + persistent namespace path-copy + publication | `Theta(F)` input bytes |
| Point read | measured descent + verified payload slice | `O(log E + R)` |
| Range read | measured descent + overlapping extents | `O(log E + C_R + R)` |
| Full read/reconstruction | in-order extent stream | `Theta(F)` |
| Same-size overwrite | split range + CDC new bytes + concat/path-copy | `O(B + K + log E)` target |
| Insert/delete | split/concat/path-copy | `O(B + K + log E)` logical target |
| Append/truncate | right-spine split/concat | `O(B + K + log E)` target |
| Native warm no-op | uninterrupted exact live projection authority | bounded authority/path checks; reopen/external state must verify or rebuild |
| Native same-size refresh | verified clone/patch or stream fallback | APFS clone cost is platform-observed, not assumed constant |
| Native length-changing refresh | full stream in ordinary-file PoC | `Theta(F)` contiguous native output |
| Managed capture | exact changed ranges | local target above |
| External-editor capture | no authoritative path/range journal | complete namespace walk; digest every unique inode, reread changed files for CDC/CAS, stream uncached prior digests and metadata; `Theta(workspace bytes)` with explicit passes |
| Bash/tool session | ExternalWorkspace + registered-child wait + cooperative quiescence attestation | unregistered/hostile writers excluded; direct writes select full scan |
| Reopen | mode-dependent Store/head validation | metadata-only normally; Verified-after-Trusted may require `Theta(reachable)` scrub |
| Fork | retain an immutable root/ref | zero object-byte copies; `O(log refs)` indexed DB operation |
| Rollback | expected-head update to retained root | zero object-byte copies; `O(log refs)` indexed DB operation plus authority checks |
| Long history | direct root reads; no replay for ordinary state | storage grows with new reachable objects |
| Compaction | authenticated retained-union mark plus offline copy/verify/swap | `Theta(indexed objects + surviving bytes)` maintenance operation |

Symbols:

```text
F   complete file bytes
B   supplied changed bytes
K   replacement chunks/extents and replacement-tree nodes created within B
E   extent count
C_R extents overlapping a requested range
R   returned bytes
```

## 9. Fast-lane policy

The implementation loop is deliberately small:

```text
edit selected product code
  -> touched unit/model tests
  -> rustfmt
  -> touched-crate check
  -> continue
```

At a completed module boundary:

```text
workspace tests
  -> deterministic randomized model sequence
  -> corruption/fault cases for the changed boundary
  -> structural-counter assertions
```

At final PoC closure only:

```text
one small native directory
  -> create
  -> read/range
  -> overwrite
  -> insert/delete
  -> execute a shell script
  -> direct `mkdir`, redirect, `dd`, `mv`, `rm`, `chmod`, and symlink operations
  -> materialize
  -> capture
  -> reopen
  -> fork/rollback
  -> exact oracle
  -> one compact benchmark report
```

No 500 MiB campaign, repeated unchanged-source rerun, per-defect method version,
benchmark-only semantic copy, or multi-backend matrix is required.

## 10. Explicit non-goals

- no transparent arbitrary-application write interception in PoC v1;
- no kernel extension, FUSE layer, FSKit integration, or custom ioctl;
- no remote store, PostgreSQL, replication, consensus, or distributed lease;
- no branch/merge/rebase framework;
- no automatic, concurrent, in-place, or background compactor; the PoC uses one explicit offline copy/swap path;
- no deep overlay chain or log replay for ordinary reads;
- no plugin/factory/registry for one selected backend and one selected file
  structure;
- no production migration from every historical K64/F64 store in the first
  fresh-profile PoC;
- no APFS syscall, host inode, clone/projection state or physical allocation in
  canonical identity; typed platform-extension metadata uses the universal
  metadata envelope and may be interpreted only by a driver;
- no Apple/Linux/Windows syscall or platform branch in core, engine, or VFS;
- no performance claim from a structural model alone.

## 11. Completion statement

PoC v1 is complete only when the implementation checklist is green and the
small end-to-end ordinary-APFS workflow, including a real Bash child process,
passes after a process reopen. Completion
means the selected architecture is viable on the qualified Apple/APFS host. It
does not mean production portability, transparent VFS integration, online GC,
hostile same-UID filesystem security, or remote operation is qualified.

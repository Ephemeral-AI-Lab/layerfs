# S4 exit audit — namespace semantics

> **Status:** COMPLETE; S4 implementation and required exit evidence pass at the
> local milestone-completion commit after `8d691ab8a`. Not release, performance
> or native-mount qualification.

The [S4 row](../303/07-implementation-validation.md#3-slices) requires ordinary
lookup/getattr/create/mkdir/symlink/link/unlink/rmdir/rename/readdir/chmod/
utimens with serial ranges, byte-name ordering, parent/reference ownership and
bounded cursors, and exit evidence of atomic effects, exact errors, aliases,
rename/resume behavior, permissions/time behavior, no resident name/handle cap,
paired EXPLAIN/runtime profiles, real canonical roots and actual daemon jobs on
the owning hosts. Implemented contract: [namespace operations](../../architecture/30-namespace-operations.md).

| Criterion | Code and evidence | Explicitly not covered here |
| --- | --- | --- |
| All ordinary operations | [Operation](../../../crates/layerfs-workspace/src/operation.rs) and one evaluator per family; `SourceView` stat/lookup/list/readlink. Every operation is exercised over a content-built root with aliases, symlinks and nested directories in [namespace.rs](../../../crates/layerfs-workspace/tests/namespace.rs) | Byte writes, truncate and opens are S5/S6; kernel request mapping is S8 |
| Atomic effects, exact errors | One `apply` transaction per operation with one ticket. A value refused after an earlier write in the same transaction leaves no row, ticket, revision or count ([compound.rs](../../../crates/layerfs-overlay/tests/compound.rs)). Each refusal is asserted to leave Workspace state byte-identical | Real device ENOSPC during a compound job is S6 pressure work; the existing SQLite FULL proof covers the shared transaction path only |
| Hard-link aliases, references | Link/unlink/replace keep one absolute reference count visible through every alias; 80 concurrent links from four threads end at exactly 82 references and return to 2 | Open-unlinked content custody and removed-row reclamation are S6 |
| Rename replacement, parents, cycles | Replacement drops one reference; kinds and emptiness are checked; a moved base directory gets no descendant row. Cross-parent directory moves verify caller ancestry in the publishing round; opposing moves from real threads publish exactly one of two in 40 of 40 rounds | Exchange-rename is not representable. Supplying the destination path is the S8 adapter's job. Commit-time topology evidence is P14/S10 |
| Resume under declared mutation semantics | Name-keyset cursor: every name present for the whole enumeration is returned once across removal, creation and rename between windows; a partly consumed reply resumes after its last name | 64-bit native cookies and handle custody are S8 |
| Permissions and time | Portable mode grammar per kind, exact refusals for unrepresentable bits and symlink mode; caller-supplied time, one mtime reported as ctime, negative fractional and extreme seconds preserved | Identity-based access is the mount's default-permissions profile and command identity, S8. Directory link count mapping is S8 |
| Serial ranges | Workspace consumes locally reserved ranges; the SDK port reserves from the owning history catalog under per-call authority on macOS; out-of-space replies are refused | Logical transport, refill fairness under load and disconnect fences are S9 |
| No resident name/handle cap | 3,000 files in one directory: one owner round each, zero base demand, every window visits at most 64 keys, three allocator calls. No per-name, per-inode or per-handle resident state exists | A larger population is S7/S12 qualification, not claimed here |
| EXPLAIN plus runtime profiles | Six exact statement plans and complete-job counters at 128/1,024/4,096 siblings: [engine](checks/s4-namespace/compound-profile.log), [operations](checks/s4-namespace/operation-profile.log), [Linux](checks/s4-namespace/linux-test.log) | Page/journal/pager/kernel residency and sustained throughput are S7/S8 |
| Real roots, actual daemon jobs, owning hosts | Direct engine and real `Owner` thread with concurrent callers, a blocked provider and an interleaved capture ([namespace_daemon.rs](../../../crates/layerfs-workspace/tests/namespace_daemon.rs)); macOS and Linux ARM64 | Native FUSE remains blocked by the published fuser timestamp defect |

## Work derivation

A compound job reads a fixed set of point rows and writes at most four inodes,
two names, one ticket and one frontier row. Every statement is a primary-key
seek, upsert or delete, so its cost is O(log N) per touched key and independent
of directory and namespace size. The cycle proof adds two point reads per
destination path component, at most 256 by the canonical limit. Emptiness is
one maintained count. Amortized and cumulative work are therefore linear in the
number of operations; no step revisits earlier names. Row growth is one inode
row per changed inode and one name row per changed name per live generation; a
name created and removed in one generation leaves none.

Measured counters, identical at all three sibling scales, zero full-scan steps,
sorts, automatic indexes and re-prepares:

| Complete operation (source window, rounds, publication, reply release) | Owner rounds | Statements | VM steps macOS / Linux | Rows changed | Base demand |
| --- | ---: | ---: | ---: | ---: | ---: |
| create under inherited parent | 2 | 39 | 979 / 941 | 10 | 3 |
| create under local parent | 1 | 34 | 873 / 840 | 10 | 0 |
| link | 1 | 35 | 963 / 930 | 10 | 0 |
| rename with replacement | 2 | 49 | 1310 / 1264 | 11 | 3 |
| unlink | 2 | 41 | 1091 / 1053 | 10 | 3 |
| directory rename across parents | 2 | 61 | 1579 / 1520 | 11 | 3 |
| rmdir | 1 | 35 | 951 / 919 | 10 | 0 |
| chmod + utimens | 1 | 28 | 653 / 628 | 8 | 0 |

The engine-only compound job (reads plus one publication of three inodes and
two names) is 26 statements and 997 VM steps on macOS, 971 on Linux. Base demand
is counted with the object cache disabled. These are deterministic work
diagnostics, not timing samples or cache-cold claims.

## Checks at the final product identity

Rust 1.85.1, locked core manifest, root ARM64 flags and Linux image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`
are unchanged; no third-party record changed. Source hashes are in
[identity.json](checks/s4-namespace/identity.json).

| Check | Outcome |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets` | PASS 595; [core-test.log](checks/s4-namespace/core-test.log) |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | PASS; [clippy.log](checks/s4-namespace/clippy.log) |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS |
| `python3 -B core/tools/check_product_boundary.py` | PASS, 514 production Rust/SQL files |
| `python3 -B -m unittest discover -s core/tools -p 'test_*.py'` | 26 OK |
| Linux ARM64 overlay/daemon/Workspace/SDK all-target selection | PASS 52; [linux-test.log](checks/s4-namespace/linux-test.log). SDK global-provider cases execute on macOS and compile only on Linux |

The earlier [Linux run](checks/s4-namespace/linux-initial.log) passed 51 at the
same product source, before one external test was added. Test-side failures met
on the way are retained in [initial-failures.log](checks/s4-namespace/initial-failures.log).

## Remaining obligations carried forward

- **S5:** the cell/tail/validity representation, including the symlink target's
  use of one creation cell, and all byte, truncate and hole semantics.
- **S6:** reclaiming rows of removed inodes after their last owner, independent
  orphan custody, normal failed-capture resolution and its effect on the
  two-layer name window, physical pressure.
- **S7:** whether a request's source acquire/release and reply release should be
  separate owner jobs; page, journal and residency evidence.
- **S8:** kernel identity and link-count mapping, native cookies, lookup/open
  references, destination-path evidence from adapter state, deferred replies.
- **S9/S10:** logical serial transport and fairness; P14 topology evidence for
  Commit validation of directory moves.

No S4 exit remains open. S0 and S5–S13 keep their own checkboxes.

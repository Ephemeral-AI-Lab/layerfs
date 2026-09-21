# Shared prepared directory construction and metadata

> **Status: implemented and verified in the declared functional scope; target v0.1.7, not released.**
> Implementation parent: `8e01d28a1c8b7708f5d319990c1440ae682f8b44`.
> Implementation commit: `85582e1ac2fb75761897115ec9679c59439b1efe`; [exact staged-tree confirmation](evidence/prepared-directories/commit-confirmed.json).
> Frozen product input seal: `e635d6cc772adddf07d4908d1cde5ef0229b0ad4f78ee98ad74ca3c12d9acc75`.

Native mkdir needs the shared Service to construct canonical directory pages and
portable metadata under its existing save owner. A directory created in G may be
modified again in G+1; that second generation also needs to patch its parent's
portable metadata without constructing canonical objects inside Workspace or
losing generic attributes. This prerequisite extends the existing direct
`UpdatePreparedFilesystem` and history `PreparedChanges` body. It adds no endpoint,
separate save, object cache, dependency or Workspace namespace operation.

## Shared contract and encoding

`DirectoryMetadata` contains a positive compact serial, mode, signed mtime seconds
and nanoseconds. `new_directories` declares fresh directories;
`directory_metadata` patches existing directories, including the root. Both lists
are strictly sorted and unique, mutually disjoint and disjoint from ordinary inode
values. The serial maximum remains i64::MAX, mode uses the existing directory mask
0o1777, and nanoseconds must be below one billion.

A new directory must be absent from the base and cannot be the root. Every new
declaration requires a matching directory-change record, including an empty one.
An existing-directory patch must resolve to a directory in the base; it may occur
without a changed-name record. The original scope/root checks remain in force.
Service checks these base facts, not the provenance of a C5 reservation. A future
Workspace caller must reserve in the exact captured scope and must not recycle an
exposed serial or replay an unknown reservation.

The old body remains byte-for-byte unchanged when both lists are empty. Otherwise
one trailing extension is encoded as version byte 1, u16 new count, new records,
u16 patch count, patch records. A record is 24 bytes: u64 serial, u32 mode, i64
seconds, u32 nanoseconds, all big-endian. A present empty extension, unknown
version, truncation or trailing bytes is invalid. Each decoded count is checked
against the remaining combined inode budget before allocation.

Ordinary inode values, new declarations and existing-directory patches share the
unchanged 128-record bound. Directory-change records and aggregate changed names
retain their respective 128 bounds; the entire metadata envelope remains 32768
bytes. Extension overhead is `5 + 24 × (new + patches)`, at most 3077 bytes within
that envelope. This is encoded-size arithmetic, not a heap or RSS qualification.

## Construction and failure ownership

Service reuses bootstrap's portable-metadata construction through the current
`SaveHandoff`. New directory inode values initially carry a private metadata-root
placeholder for content. C1 receives both the new serials and their mandatory
directory-change rows, constructs the actual empty/nested directory pages and
replaces the placeholder before publishing an inode. C1 retains its existing
behavior of omitting an unreferenced new directory; this is not a new reachability
rule. Generated objects remain in the same C2 save, whose dependency resolver can
resolve accepted objects from that save. `StoreProvider` never needs to read a
newly emitted private object.

Existing-directory patches resolve the original inode and reuse the same portable
attribute patcher as `UpdatePortableMetadata`. It preserves the original directory
content, generic attribute values and reference counts. The existing standalone
metadata operation retains its deadline-aware reader/output wrappers.

All callers retain one save owner and explicit abort failure precedence. Review
also corrected the existing history-stage and both bootstrap save paths: a
post-build deadline expiry now enters the checked abort branch instead of relying
on best-effort Drop cleanup. Existing construction/storage failures take precedence
over the deadline observation; unknown storage outcomes are not guessed away.

## Declared verification

Bridge tests cover legacy bytes, both extension lists, malformed/overlapping input
and the exact 32768/32769-byte envelope. Service public direct and authenticated
native tests cover fresh nested declarations, omission of unbound declarations,
the mixed 128/129 inode boundary, validation refusals, unchanged prior history/stage
and portable patches preserving 300 generic attribute entries.

Four external native selections are registered: `nested`, `metadata_followup`,
`refusals`, and `unreferenced`. Each independently byte-copies the closed canonical
Store fixture and creates fresh live C5 authority. The nested case reserves four
serials and uses disjoint pairs for direct construction and one composite Commit;
identity-dependent roots are not claimed equal. The metadata follow-up performs
two explicitly declared Commits, creating directories in G1 and patching those
existing directories in G2. All selections use a 60-second complete-command hard
budget and 5000 ms request budget, one construction worker, with no cold or
performance claim. They run the host Service/client and make no Docker quota claim.

The first all-target compile check failed on three new test compilation errors
(Source input type and a Timing Result return). The corrected source and original
failure log remain separate evidence. The final whole-core checks and native
selections below use the frozen corrected product source.

Workspace's maintained namespace index, FUSE mkdir/create, new-file/symlink upload,
complete DSH upload and one full-upload Commit, broader capacity prerequisites,
matched R6/#207, and hard resource qualification remain unrun. The prepared DSH
master from [34](34-preinstalled-dsh-workload.md) is reused unchanged; live npm
installation and network latency remain outside the workload.


## Completed evidence

Locked Rust 1.85.1 whole-core tests passed: host **680 passed / 0 failed / 3 ignored**;
Linux **678 passed / 0 failed / 126 ignored**. Host/Linux warning-denying all-target
Clippy, host bins/examples, fmt, the 247-file product boundary check and six boundary
self-tests passed. The initial failed all-target check remains archived; subsequent
all-target Clippy covers its corrected targets. Linux standalone bins/examples were
not separately linked because these external selections use host binaries; Linux
whole-core tests and all-target Clippy cover their changed source. No retired
aggregate preflight or CI was run.

The host immutable binary archive is
`core/target/pair1-evidence/binary-archive/46535d91a32ee44ba9fa80fac73d0b2f4e58d7b1b964de7fed2c27fe0298f4fc/host`.
These are dev-profile functional binaries, not a performance arm. Each caller and
its imported repository Python dependencies was snapshotted before execution; all
four receipts match the frozen source, binary hashes and caller dependencies.

| Native selection | Result | Driver wall, s | Complete command, s | Native cleanup |
| --- | --- | ---: | ---: | --- |
| nested | PASS | 3.174913625 | 3.302862042 | PASS |
| metadata_followup | PASS | 0.504646083 | 0.615442292 | PASS |
| refusals | PASS | 3.754269417 | 3.876827209 | PASS |
| unreferenced | PASS | 0.365929542 | 0.495878209 | PASS |

All 13 registered assertions passed; no selection was repeated and no selected row
is omitted. The refusal selection records 63 attempts across 21 variants and three
operation routes. Some requests are correctly refused by the native client's
shared validator before submission; others reach authenticated Service and receive
a definite failure terminal. Their receipts distinguish those paths. The wider
Rust direct/native tests cover the 300-generic-attribute preservation case; the
external metadata follow-up does not claim generic-attribute transport coverage.
Every Service exited normally with status 0, no forced client/service/group cleanup,
and the closed master remained unchanged.

[Functional index](evidence/prepared-directories/functional-index.json.gz),
[check index](evidence/prepared-directories/checks-index.json.gz),
[frozen source and binary inputs](evidence/prepared-directories/inputs-01.json.gz),
[independent review disposition](evidence/prepared-directories/source-review.json.gz)
and [raw archive manifest](evidence/prepared-directories/archive-manifest.json)
retain the exact commands, hashes, caller bytes, original failed compile check and
all native receipts. Original uncompressed evidence remains under
`core/target/pair1-evidence/prepared-directories/`. Reproduce one selection with
`python3 core/crates/layerfs-service/tests/prepared_directories_route.py`, the
recorded `--case`, immutable `--binaries` archive, closed
`--fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json`, and
a fresh `--output` path. This performs an independent writable Store byte copy and
creates fresh live C5 authority; it never reopens copied write authority.

Production source is **110285 → 110540 (delta +255)**: replacement core
**44868 → 45123**, reference **65417 → 65417**. There is no legacy retirement or
scope change. The unchanged counter blob
`b5b9617d08204977176302311e0b2c72a811b420` excludes tests, tooling, docs, examples and
legacy inline tests; [31](31-source-map-and-loc.md) carries every file/folder total.
The exact parent/final-staged comparison is required before committing this source.

# Shared portable metadata construction

> **Status: implemented and functionally verified for the declared constructor scope.**
> Implementation parent: `3b34e3002b4a7abfd205b68bc94de9a4af22b572`.
> Frozen product input seal: `5708eecb436c8b1a386cbd0bf20b80e03dcf3589cef5e4032445aa98b13ad2e2`.

This round adds one shared Service operation, ConstructPortableMetadata, for a
fresh attribute tree with no prior metadata root. It reuses the same C1 portable
metadata builder already used by history initialization and new directories,
under the existing C2 save owner. It allocates no inode identity, changes no
namespace and publishes no Branch or Commit.

Workspace already records both saved content and metadata roots in its R record
before Stage/Commit. That record is also required to substitute captured G under
later D1 on known completion. Fresh file creation therefore needs an independently
saved metadata root; constructing metadata only during the final filesystem save
would leave that existing bookkeeping boundary incomplete. No Workspace mutation
is added by this prerequisite.

## Contract and ownership

Constructor opcode15 uses operation profile1 and explicitly shares the existing
portable-metadata grant128 with update opcode9. The u8 grant mask is unchanged;
legacy31 and127 do not grant this operation. No new daemon control authority is
introduced.

The request carries kind(u8), mode(u32), signed mtime seconds(i64) and nanoseconds
(u32). Kind1 accepts permission bits0777, kind2 additionally accepts sticky01000,
and kind3 requires0777. Nanoseconds must be less than1,000,000,000. Signed seconds
retain their full existing i64 domain. The operation has zero input and output
body; the response byte allowance must be zero.

The request metadata occupies44 bytes: existing27-byte envelope plus17 portable
bytes. Distinct MetadataConstructed result tag19 occupies66 bytes: tag1, fields17,
root32, inserted8 and reused8. The result echoes the exact portable fields, without
a fictional base root. Existing update request76/result98 and its tag11 remain
unchanged. Root bytes remain opaque32-byte content identities and save counts
remain u64; the constructor invents no new hash-value or count restriction.

Service validates empty input before acquiring a save. The common metadata helper
uses the existing deadline-aware consumer and chooses either a fresh builder or
an existing-root portable patch. It emits through the normal SaveHandoff; it does
not read unpublished new objects through StoreProvider. The existing write owner
gives retained C2 failures precedence, checks expiry before finish, explicitly
aborts definite failures and preserves unknown/cleanup outcomes. A successful
finish is never relabeled as an abort because of later response delivery trouble.
The native client matches the distinct result and all request echoes under the
existing admitted-mutation uncertainty rules.

## Verification and remaining work

The independent product review found no actionable defect. Six external Rust tests
cover exact framing and portable values, native result correlation and uncertainty,
actual public C1 readback of the two portable keys, repeat-root reuse, authority,
empty input, expiry and real C2 writer admission. The C2 case explicitly aborts a
separately held save owner before successful construction; it does not exercise an
entered constructor save or late SQL failure.

The first caller review found two narrow gaps, corrected before native samples.
Malformed-terminal scenarios originally closed the fake peer immediately; they now
keep its receiving half alive and reject a subsequent frame before the client is
dropped. Deliberate lost-terminal behavior remains separate. Python now labels its
log as attempted framed client requests because local validation can return
NOT_SUBMITTED. Zero attempted Reserve/Commit/filesystem-save operations do not
claim internal Service instrumentation; the public constructor test has no
HistoryCatalog as an independent boundary check. Original caller snapshots and
initial passing checks retain their exact identities.

Both first native selections pass. The [functional index](evidence/construct-metadata/functional-index.json.gz)
and [archive manifest](evidence/construct-metadata/archive-manifest.json) preserve
exact receipts, commands, source/caller/binary identities, ownership journals and
all client/Service stderr. Native execution uses the host Service/client, not a
Docker runtime; the Docker two-CPU limit applies to Linux verification builds.

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| construct-01 | PASS | 2.956167583 | 3.063408750 |
| refusals-01 | PASS | 0.411268667 | 0.555276209 |

These are functional command observations with no cache/performance claim. Each
uses a separate byte copy of the closed canonical Store and fresh live C5 authority,
one construction worker and a60-second complete budget. All six selected check
rows pass, including normal cleanup and unchanged prepared master. No forced
client or Service cleanup occurred. Repeated construction and existing metadata
no-op updates return the same root with inserted0/reused7 for each selected kind.
The refusal case distinguishes two actual Service grant denials from six locally
rejected invalid-field requests. It preserves unchanged Store bytes and old
namespace/history observations. No passing selector was repeated.

The [check index](evidence/construct-metadata/checks-index.json.gz) records exact
commands and logs: locked/offline Rust1.85.1 whole-core host **686 passed/3 ignored**,
Linux **684 passed/136 ignored**, host and Linux all-target Clippy with warnings
denied, host binaries/examples, fmt, the251-file boundary guard and six guard tests.
After caller review, the revised Bridge test passed all3 host checks and the whole
Linux test/Clippy commands covered the final caller. Unchanged host product tests
were not repeated. CI and the retired aggregate preflight did not run.

[Linux aarch64 DWARF evidence](evidence/construct-metadata/layout/layout-01.json.gz)
selects the exact Bridge namespace and confirms unchanged Operation304,
Response112 and Request344-byte layouts. It pins both immutable executables and
compiler output. This is object layout, not heap/RSS/cgroup qualification. No new
retained collection, worker, queue, persisted format or resource policy was added.

Production LOC: **111756 -> 111922 (delta +166)**; core **46339 -> 46505 (+166)**,
reference **65417 -> 65417 (+0)**. The unchanged counter
`tools/production_loc.py`, blob `b5b9617d08204977176302311e0b2c72a811b420`, counts
nonblank/noncomment product Rust/runtime SQL in exact parent/final staged snapshots,
excluding tests, docs, tooling, manifests and generated output. No legacy retirement,
relocation or counting-scope change. The full file/folder/crate inventory is
[recorded separately](evidence/construct-metadata/source-loc.json). The next separate prerequisite is fresh regular-file declarations in the
existing prepared filesystem operation, before native file creation. The full
preinstalled DSH tree still must be uploaded in full and followed by one explicit
Commit; no hidden smaller Commits, network installation or regenerated fixture is
authorized by this operation. File/symlink creation, larger input admission, hard
memory qualification and matched R6/#207 remain open.

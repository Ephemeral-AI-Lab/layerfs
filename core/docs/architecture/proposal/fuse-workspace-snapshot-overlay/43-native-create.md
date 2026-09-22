# Native file creation and admitted handle rights

> **Status: implemented; native capacity gate failed/open. Other functional proof is recorded below.**
> Implementation parent: `ab473145a606a71327d55d10f12205edb1803946`.
> Exact implementation commit: `7eb46ef0766eae0d6ccfb894d07919a39c69ef38`; [confirmation](evidence/native-create/commit-confirmed.json).
> Product input seal: `2ba9298beca16b990d1fa9f4884a630461623f9ade02a82923236daace74aa65`.

This round adds one native Workspace operation, `create_file(parent, name,
FileCreateOptions, deadline)`, returning exact accepted attributes plus a ready
file handle. Options contain portable mode, umask, exclusive behavior and the
existing FileOpenOptions. Success owns one Local lookup reference and one Local
handle. The operation is LocalEdit-only; mounted creation remains outside this
round and refuses before preparation or inode reservation.

## Creation and opening

A new binding requires parent write/search permission and one exact-scope C5
reservation. Child empty-file state, the binding, parent mtime, generation
accounting, lookup reference and initial ready handle publish together. Node,
handle, handle-ID and complete-request capacities are checked before reservation
and at publication. Later failure consumes its reservation without replaying it
or exposing a partial name.

An existing name with exclusive requested returns Exists without reservation.
Nonexclusive existing regular files reuse existing open/append/truncate behavior,
requiring parent search and existing-file open rights, but not parent write.
Creation mode and umask do not alter an existing inode; input option shapes still
must be valid. Existing directories and symlinks retain selected wrong-kind
refusals with no implicit symlink following. Internal open returns the exact
published attributes with its handle, including truncation, so no fallible
post-publication getattr can lose a ready handle. A failed return releases only
its newly acquired lookup reference.

One admitted local operation spans the complete path, preventing a mount from
entering between helpers. Original-resolution and open helpers reuse that guard;
no nested scratch charge or held remote permit is needed.

## Handle permission boundary

New-file creation admits the initial requested handle independently of the final
portable permission bits. A mode0400 file created with a writable handle therefore
accepts writes through that handle, while a later writable open is denied.
Handle-based mutation validates ready state, stored access, kind, scope and inode
identity. Handleless edits/size changes and later open/OpenReservation retain
mode checks. The same rule already applies to reads through admitted handles.
No special permanent created-handle bypass is introduced.

There is no new public native handle-resize API. The existing native set_len is
handleless; fd-based resize belongs to the projection path. Native tests must not
claim that latter path is exercised by this round.

## Backing, capture and save

An explicit fresh-identity flag uses reserved bytes of the existing inode record.
Directory bindings can reference regular-file I records. Forget/relookup and
original resolution consult fresh/captured local state before querying a path
absent from B. Fresh and captured may coexist for D1 edits of a file created in G.

An uncaptured fresh file has an empty base and Local/Zero pieces only. It saves
through ConstructFile, including length zero, then ConstructPortableMetadata,
then the ordinary R row before Stage/Commit. Its serial enters the prepared F
list. ReplacementSource reuse requires no Base pieces and replacement==length.
Known own G completion replaces both saved roots and clears fresh/captured state;
files first created in D1 remain fresh. No hidden Commit or inspection repair is
introduced.

The fresh counter counts only creations in the current generation and resets at
capture. D1 edits of G-created files add dirty file rows without another F count.
The captured F list must match that count. Exact request admission retains H195/228:

```text
H + 73*files + 34*directories + encoded_name_bytes
  + (fresh_files > 0 ? 7 + 8*fresh_files : directories > 0 ? 5 : 0)
```

The existing128-record/name,8-MiB replacement,640-KiB writer and128-KiB I/O limits
remain selected. ConstructFile's larger transport length bound does not itself
qualify larger fresh Workspace files. New publication uses four global rows and
one E update. Fresh identity uses reserved byte25 of the160-byte I record;
E remains16 bytes. The137 candidate-page and26 reconciliation-slot bounds remain.

## Functional evidence and unresolved capacity failure

The registered set contains seven native-create selectors plus seven affected
mkdir/write/resize/coherence regressions. Samples use independent byte copies of
the closed64-MiB master and fresh live C5 authority, one construction worker,
Docker CPU quota2, the original10-second operation deadline and60-second complete
verification budget. They use development binaries and claim neither cold-cache
performance nor hard RSS/cgroup limits. The configured permission owner is
UID/GID1000; the test process is root, so this is not a nonroot-kernel proof.

The original capacity gate remains **FAIL**. It admitted93 fresh files with
255-byte names and one existing-file edit, then failed in FileSave with definite
Service Io before Stage/Commit. The complete command was13.486086792 seconds;
its test did not reach either final verification or native clean close. Owned
container/volume removal and normal Service exit succeeded. The retained databases
show131 published saves, no active save owner, no Stage and only the fixture
Commit. Interpreting114 workload saves as57 content/metadata pairs is consistent
with the fixed save order: the8-MiB construction had already finished. The precise
original failing RPC and cause cannot be recovered from that receipt.

An explicitly labelled **DIAGNOSTIC_NOT_GATE** repeats the same workload and
budget with external phase/Source observation. It completed Commit in6.675095
seconds with3.324904 seconds remaining and passed its assertions and native clean
close. Its189 calls comprise93 ConstructFile,93 ConstructPortableMetadata, one
existing-file EditFile and metadata update, and one composite Commit. Connect/
authentication plus HELLO used0.939562 seconds; call intervals total5.034857
seconds. The8-MiB constructor took1.885796 seconds in call and received8388608
bytes in514 Source reads, with EOF and no Source error. This proves that diagnostic
attempt only. It does not supersede the failed gate or establish its root cause,
repeatability, host interference or cache state. No timeout, worker, buffer,
workload or product change was used to turn the failure into a pass.

The exact admission frontier is32520=342+346*93 bytes; one more such name/file
would require32866>32768. The large fresh file is8MiB with a Zero gap and one final
local byte; one further byte is refused atomically. It receives one full content
save and one explicit full Commit. Its readback is the declared three windows,
not a whole-file digest. Empty/small results use complete content comparison.

Two external-caller issues are separately retained in evidence. The first
coherence truncate-failure attempt stopped at an immediate procfs thread-absence
assertion after join; kernel thread teardown does not promise that observation.
Both notification fault callers now retain the actual joined result, per-thread
seccomp counts, unaffected-worker counts, no-extra-RPC and functional/custody
assertions without that racy procfs check. Optional diagnostic instrumentation
also briefly shortened Client lifetime before response gates; the final helper
restores the original lifetime and early connection-failure return. Historical
caller snapshots and their original attempts are retained, not relabelled.

## Completed verification selections

| Selection | Current status | Complete command seconds |
| --- | --- | ---: |
| create-semantics | PASS | 3.791364083 |
| create-flags_permissions | PASS | 1.331004208 |
| create-successor | PASS | 1.323311000 |
| create-capacity | FAIL | 13.486086792 |
| create-refusals | PASS | 12.955643458 |
| create-reserve_denied | PASS | 1.104993209 |
| create-reserve_unknown | PASS | 1.170449208 |
| mkdir-visibility | PASS | 1.260608875 |
| mkdir-notification-failure | PASS | 1.135376125 |
| coherence-truncate-failure | PASS | 1.179371166 |
| kernel-resize-semantics | PASS | 1.335640042 |
| write-zero-rights | PASS | 1.081911959 |
| write-successor | PASS | 1.334022208 |
| mkdir-capacity | PASS | 5.386293917 |

All14 registered selections ran:13 have a passing functional proof and the native
create-capacity gate remains FAIL. The retained earlier coherence failure is
followed by its corrected-caller pass; mounted notification also ran once after
the same two-file correction. The diagnostic stays separate. Complete per-attempt
commands, checks, failures, source/caller/binary identities and cleanup are in the
[functional index](evidence/native-create/functional-index.json.gz) and
[archive manifest](evidence/native-create/archive-manifest.json).

Locked/offline Rust1.85.1 whole-core checks passed: host691 tests/3 ignored,
Linux689 tests/143 ignored, host binaries/examples, both all-target Clippy runs
with warnings denied, fmt, the254-file product boundary guard and six guard tests.
Final external-caller corrections then passed Linux workspace test compilation,
both all-target Clippy checks and fmt; unchanged full suites were not repeated.
[Check index](evidence/native-create/checks-index.json.gz) preserves the exact
commands and logs. CI and the retired aggregate preflight did not run.

## Resource and source accounting

Compiler layouts show Workspace runtime State264->272 bytes, captured overlay
state64->72 and Submission976->984, reflecting one fresh-file count. Node4320,
Handle72, Cell288 and in-memory Inode136 remain unchanged. FileCreateOptions is
12 bytes. The layout dump includes names also used by enum variants; those
ambiguous matches are not alternative sizes for the runtime structures.

The prepared-request conservative requested-capacity bound is59392 bytes:
128*80 inode rows +128*32 directory rows +2*128*24 directory metadata rows
+128*40 name-binding tuples +32768 name bytes +128*8 fresh IDs. Independent
maxima make this conservative. Five outer Vec headers remain inline, and inner
name headers already appear in directory rows. This is not allocator rounding,
RSS or a cgroup peak. The640-KiB writer and128-KiB I/O charges are unchanged.

Production LOC: **112031 ->112364 (delta +333)**; core46614->46947 (+333),
reference65417->65417 (+0). The exact first-parent/final-staged comparison uses
unchanged tools/production_loc.py, blob b5b9617d08204977176302311e0b2c72a811b420,
with product-only nonblank/noncomment Rust/runtime SQL and identical exclusions.
The [complete inventory](evidence/native-create/source-loc.json) has254 core
production files. Moving original resolution out of write.rs and sharing child
creation with mkdir are relocation/refactoring, not legacy retirement.

Mounted CREATE, symlinks, larger full-input admission, the unchanged preinstalled
DSH tree uploaded in full before one explicit Commit, repeated incremental Commits,
hard memory qualification and matched R6/#207 remain open. The capacity failure
also remains an explicit qualification gap.

Additional retained client/fixture output is indexed in the
[archive supplement](evidence/native-create/archive-supplement-01.json).
Original receipts and result classifications are unchanged.

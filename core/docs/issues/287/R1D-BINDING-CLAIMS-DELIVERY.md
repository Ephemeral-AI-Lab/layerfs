# R1d binding claims: delivery and proof scope

> **Status: Scoped exits/checks and exact staged LOC PASS; publication pending.**
> First parent: `d3a10aadbb7506772552474478b14ef25947c899`.
> Full R1 remains PARTIAL/open. No release candidate or #288 admission exists.

The actual Server scalar construction route now rejects duplicate directory or
symlink bindings through a typed paged claim authority, verifies its complete
ordered result and acknowledges known retirement before directory-root work.
It no longer constructs the validation additions map on that route. The
[prospective contract and transparent refinements](R1D-BINDING-CLAIMS-FREEZE.md)
record exact records, phases, failure custody and resource arithmetic.

## Resulting behavior and authority

Claims have key25, implied exclusive class1, frame32, separate seal130/domain
`layerfs/binding-claims/v1\0` and page header163. At most128 pending keys are
resident. Exact acknowledged presence plus local-window membership preserves
the first duplicate verdict across batch boundaries. A regular-file alias needs
no claim. Stored canonical kind wins over a caller's value when classifying an
existing inode. All scalar name/row/completion/EOF, allocator, alias, root and
cycle semantics remain in the common validator.

The new `CheckedTopologyInput` returns input/topology without an additions map.
Explicit legacy `check`/`check_bindings` preserve non-file1/file0 entries; legacy
canonical construction uses explicit resident claims without that unused result
map. Canonical merge/construction remains one implementation. The source split
into focused compatibility/construction files is relocation, not an independent
algorithmic reduction. V1 canonical reference roots/pages remain unchanged.

`ConstructionScopes` binds claims1/table2 then roots2/table1 to the same issued
selection/native owner. Claimed pages use independent C1 count/bytes/digest/EOF
verification. The pending allocation ends before seal, and each consumed page
ends before the next request. Exact logical abandonment terminalizes failed
attempts before or within validation and after successful retirement, including
root-constructor refusal. It performs no SQL/native cleanup/refund or guessed
retirement. There is no hidden Drop action, retry or second eligible attempt.

C2 explicit `begin_phased(selector,D,B)` selects private LFCS schema/header2;
legacy `begin` keeps private v1. One immutable native association, connection and
16MiB reservation owns both phases. Bounded batch SQL validates every selected
key/class before inserting; a later provider/corrupt-class failure cannot be
masked by an earlier existing key. Known Duplicate rolls back the entire current
batch and terminalizes the phase, retaining earlier acknowledgements.

Provider seal uses one transaction, indexed MAX and one ordered bounded scan.
Retirement deletes at most128 exact keys per transaction and confirms actual
empty claims. The final COMMIT and native allocation observation both succeed
before the live owner enables roots. Known partial retirement retains exact
remaining state; Unknown retains original typed error, attempted phase/counts/
serials, proposed seal/MAX when applicable, native handles and full credit.
No reread adopts an Unknown, and taking its error cannot enable cleanup/refund.

Server admission checks declaration/spool/native shape before Save/body effects.
The complete path is scalar receive/seal/subject checking, C1 claims validation,
seal verification, known retirement, common canonical build, native cleanup,
Save finish and Stage. A known unfinished Save still owns explicit abort when
scratch is Unknown. Legacy whole-request C2 allowance/result custody remains
until R3; no Bridge opcode, advertised capability, quota or worker changed.

## Independent and real-provider proofs

Current host is Darwin25.4 arm64, uid501. Actual selected SQLite is
`/usr/lib/libsqlite3.dylib`, version3.51.0, source
`2025-06-12 13:14:41 f0ca7bba1c5e232e5d279fad6338121ab55af0c8c68c84cdfb18ba5114dcaapl`.
These are direct library/provider proofs. Native main still refuses this
provider's unsupported required32MiB hard-limit readback0; no native guard,
global/physical memory or protected healthy-progress claim is inferred.

- C1 independently transcribed scope/frame/digest grammar and sealed v1 fixtures
  prove private bytes and canonical compatibility separately. Codec8 plus final
  semantic/caller17 are25 new primary cases. The257-claim dispatch proof observes
  three batches and three verification pages in an explicit resident provider;
  it makes no physical or speed claim. Same-owner refusal, duplicate priority,
  stored-kind precedence, root admission/emission fence and exact cleanup pass.
- Real C2 metadata SQL/native proofs pass16 new primary cases plus8 affected v1
  compatibility cases. Five empty helper invocations are excluded from the
  combined reported29 passes; actual helper bodies run through owning process
  cases. Literal independent transcripts and native owner/header identities are
  checked independently of the product encoders.
- The maximum new provider case claims65536/2097152 framed bytes, retires them,
  then writes65536 roots/4128768 framed bytes in the same file. It observes
  SQLite570 pages for claims,565 free pages after retirement and1836 final pages
  for roots, with16777216 allocated bytes and unchanged16777216 retained credit.
  These are actual file/SQL observations, not heap/cache/RSS/lifetime peaks.
- Three fresh process owners establish actual BEGIN+SELECT SHARED barriers before
  Batch, Seal and Retire COMMIT. Each selected operation is attempted once.
  Original Busy Unknown, exact attempted custody, non-destructive abandonment,
  no root/refund/native release and explicit helper reaping pass.
- Real direct Service Stage passes257 exclusive new directories and an external
  FK first-root assertion. That test explicitly alters only its owned scratch
  schema, keeps trigger depth0 and foreign_keys1, observes owner/root column
  counts16/4 and validates actual negative FK result787 in open phase0. It proves
  phase3 ordering; C2 separately proves phase3 requires exact empty claims.
- Duplicate257 after two128-key windows preserves the prior Stage, old root and
  bytes. The first body read observes a real active Save; afterwards active Save0,
  unchanged total saves, removed scratch/spool and exact known cleanup pass.
  Affected real two-Save/catalog progress and scratch Unknown/known-Save cleanup
  cases also pass, including explicit blocked cleanup with retained credit.

The logical phase peak is `max(63D,32B)`, not a sum of coexisting populations:
at most4128768 framed bytes and65536 rows. The encoded batch is4184 bytes and
page4259 bytes at128. ClaimKey/ClaimRecord native element width is25, giving3200
bytes per128-element window. Fixed attempted serials add1024 bytes; local batch
sorting or retirement adds another bounded1024-byte array. These figures omit
actual Vec/header/Arc/hasher/native engine ownership; encoded arithmetic alone
does not establish aggregate native memory. Seal, C1 verification and retirement
are three O(U) walks. No prefix COUNT/OFFSET/rank scan or growing key vector exists
in those provider operations.

## Exact checks, failures and reuse

All command argv, actual source inventory/seal, exit and raw output are preserved
in [append-only evidence](evidence/r1d-binding-claims/). Cargo uses Rust1.85.1,
`--manifest-path core/Cargo.toml --locked`, the owned `core/target` and root ARMv8
AEAD configuration. No performance selection, campaign, new family or runner ran.

| Evidence | Owning command/result | Scope and disposition |
| --- | --- | --- |
| 00 | Scoped Rustfmt failed1 on numeric-leading external test identifier; exact rename then formatter0 | Before functional freeze; output/correction retained |
| 01 | Content selected tests PASS97 |21 new +7 indexed +69 affected scalar/semantic/bounds/reference tests; immutable tested closure |
| 02 | Unused external test import warning removed | No behavior change; functional proof reused |
| 03 | `test -p layerfs-content --test binding_claim_validation -- --nocapture` PASS16 | Three new early-entry refusal cases after demonstrated source correction |
| 04 | Storage selected test command FAIL101 at compile | Four unsupported u64 FromSql reads; no provider body ran |
| 05 | Same focused validation target PASS17 | New post-retirement root-constructor refusal; C1 total101 distinct primary cases |
| 06 | `test -p layerfs-storage --test construction_claims --test construction_state -- --nocapture` PASS24 primary | Corrected external i64/checked-u64 reads;5 empty helper invocations excluded |
| 07 | Server selected command FAIL101 | First observer NOFOLLOW open14 on `/var` alias; second test blocked by mutex poison; other targets unrun |
| 08 | External owned fixture path correction | Regular final file, canonical pathname before one NOFOLLOW open; protection/library unchanged |
| 09 | `test -p layerfs-server --test binding_claims --test prepared_bindings --test catalog_admission --test history -- --nocapture` PASS43 primary | One empty helper invocation excluded; complete Stage/custody/provider route |
| 10 | Owning all-target Clippy FAIL101 | One unnecessary private lifetime; equivalent elision |
| 11 | Product boundary PASS415 files | Whole current Core; policy behavior is not semantic proof |
| 12 | Guard self-tests PASS9 | Selected unchanged guard module; unrelated SDK/counter checks reused |
| 13 | Covering all-target Clippy FAIL101 | Twelve non-owning FilesystemObjects explicit drops |
| 14 | Covering all-target Clippy FAIL101 | Child-handle lint association after already completed wait |
| 15 | Exact lint source corrections | Non-destructor borrow-only drops removed, actual owner/page/connection drops retained |
| 16 | `clippy -p layerfs-content -p layerfs-storage -p layerfs-server --all-targets -- -D warnings` PASS | Locked owning/dependent compile/lint; no blanket lint suppression |
| 17 | Focused exact real-provider Unknown test PASS1 | Covers changed helper ownership/reaping at three fresh Batch/Seal/Retire owners; not counted twice |
| 18 | `build -p layerfs-content -p layerfs-storage -p layerfs-server --examples` PASS | Locked compilation; no example workload/measurement executed |
| 19 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` PASS | Whole final Core formatting |

Counts are168 distinct primary functional cases: C1101, C224, Server43. Covering
C1 reruns follow actual new entry branches and are not counted twice. The helper
ownership correction receives its own narrow lifecycle cover; unchanged maximum
provider and canonical suites are reused. Owning examples and whole-Core formatting
pass. Boundary415/guard9 source is reused after equivalent lifetime-only source
edits; unaffected SDK2 and revised counter24 tests remain reused, full final Core
checks remain R7. Counted commit/publication evidence follows before handoff.


## Exact production source comparison

Same revised root counter blob `a1cb6c064dd150dbc3aa19648a3748b5aeb741b5`,
SHA-256 `0d798f53263c0f18636d96ccdfdc3728f501f53b066e90577d251859f1f117b2`,
on exact first-parent and final staged `git archive ... crates core/crates`
snapshots reports:

```text
Production LOC: 141630 -> 143585 (delta +1955)
Reference: 65417 -> 65417 (delta +0)
Core: 76213 -> 78168 (delta +1955)
```

Rust and shipped runtime SQL are included with imports/declarations/delegation;
inline/test-only code, external tests/examples/fixtures/tools/docs/manifests,
third-party/generated files are excluded identically. Per-file classification is
in [PRODUCTION-LOC.json](evidence/r1d-binding-claims/PRODUCTION-LOC.json).
Core staged subtree is `a46bb725fc01f2a14e10e97fb4a8ccdc624dd5aa`; unchanged
reference subtree is `498dd1917812ae90efb8841f57e22bfc284e96fb`. Changed staged
source/test seal44 files is
`80d5dc3c2307d75257d231be079e45bdb0bc100491a077ce8197adc6d265f0e0`
(sorted relative path/NUL/exact bytes/NUL). Documentation additions cannot change
these product subtrees. Reference retirement is0; old canonical construction file
extraction is relocation, explicit public v1 compatibility remains, and new typed
claims/provider/native phase custody justify the growth. Committed recount and
normal push confirmation follow; no self-referential future commit SHA is used.

## Retained limits and next responsibility

Demand/missing/base memo/alias sites and bindings/graph seen/frontier/reachability/
unreachable/reference/count/touched/final-row/release populations remain later
R1d gates. Native exact aggregate engine/cache/memory/protected dispatch and Linux
provider observations remain incomplete; R1e's original4 eligible-body failures
retain their verdicts. V2 canonical policies, schema11/import, live Workspace v3,
full Commit, runtime/FUSE/UntilOwnedExit, concurrent enablement and cutover are
R2–R7 work. No deferred10240 workload or other ticket is activated or closed.

#288 qualification remains delegated/unrun. Its later source-matched coverage
must account for new claim SQL/seal/retirement and removal of the additions map,
unchanged native file admission and the explicit unsupported Darwin executable.
No historical receipt is promoted to this source and no speed/release PASS is
claimed. Next is the separately frozen R1d validation-prefetch producer that
removes whole demand/missing/answer unions with bounded waves; it does not solve
the remaining memo or alias authority. Combined compact alias-site authority is
a subsequent selected contract, with independent failure-order/source gates.

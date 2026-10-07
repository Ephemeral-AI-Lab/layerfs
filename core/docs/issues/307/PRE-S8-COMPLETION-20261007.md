# Serverless Store: pre-S8 implementation handoff

> **Status:** Local implementation/evidence handoff2026-10-07. Not release evidence,
> a product guarantee or completion of S8/S10/S12. Starts at `21ff451fa` on local
> `main`; final product change is `01f72eee0`, final diagnostic driver `02922afa1`.

The global Store, separate daemon overlay, host seal/install, direct daemon ports,
Store half of Commit and authenticated controls have Disposable proofs. F0–F13
and F15 are functionally closed at the scopes below. F14 has actual registered
direct operations, independent whole-operation count validation and supported
phase diagnostics. Exact continuous peaks/physical attribution remain incomplete;
new numerical acceptance is owner-deferred. No claim says future failures are
impossible or that these proofs cover FUSE, Bash or live Commit normalization.

The subsequent owner-authorized [resource-growth investigation](PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md)
now passes14 actual host/Linux selections: namespace growth,4/32/256MiB streamed
Saves and64 distinct Commits in one Workspace. Source-bounded Rust ownership and
terminal release are supported; OS file-cache growth is explicitly accounted.
No production correction was indicated. This closes that focused investigation
and supports S8 implementation, while the broader measurement precision limits
below remain unqualified.

Later owner direction makes Disposable/WAL/OFF the only permitted execution
profile; Durable is disabled indefinitely until explicit reauthorization (the
earlier deferrals below retain their historical scope). The subsequent
[full Init/history matrix](DISPOSABLE-WAL-MATRIX-20261007.md) completes all seven
product workloads and their functional/cold proofs. Three Init historical speed
comparisons and the stride1 original allocated-storage ceiling fail. Those
numerical findings are retained and are not changed by the functional closure
table below.

## Controlling owner decisions

- The approved narrow macOS seal file-control exception is in
  [the seal decision](SEAL-PERSIST-WAL-DECISION-20261007.md). It changes no other
  unsafe boundary and does not authorize manual sidecar deletion.
- [Counted Save block refills](SAVE-RESERVATION-DECISION-20261007.md) supersede
  exactly one reservation for an arbitrarily large streaming Save. Each initial
  reservation/refill is counted and attempted once; no total Save/file cap.
- [WAL throughout](PRE-S8-WAL-BASELINE-DECISION-20261007.md) supersedes the temporary
  private MEMORY import route. Its measured speed FAIL stays FAIL and is accepted
  as the exact new host baseline; no second Init measurement was run.
- [Overwrite-only Branch publication](BRANCH-OVERWRITE-DECISION-20261007.md)
  supersedes F8/F9/F13's same-Branch HeadMoved requirement. Last successful database
  effect wins; captured parent/provenance is retained. No merge/rebase/retry.
- [Diagnostic acceptance](PRE-S8-ACCEPTANCE-DECISION-20261007.md) defers new latency
  and absolute phase-memory thresholds. It retains correctness, bounded-state,
  deterministic counts, original custody and honest unsupported observations.

## Feature proof map

All links resolve to retained original receipts or their owning reports. Reports
record iterations, exact hashes, failures and reuse scope; later rows never rewrite
an earlier verdict. The table's closure is functional/count scope, not numerical
performance qualification. The [retained evidence index](checks/pre-s8-accounting-20261007/73-retained-evidence-index.json)
hashes489 existing artifacts, including42 receipts with nonzero exit footers.
That inventory is not a verdict generator: reports also preserve ineligibility,
incomplete attempts and the known erroneous exit0 footer.

| Feature | Status and actual proof | Exact retained evidence |
| --- | --- | --- |
| F0 | CLOSED: R2 whole-root qualifier;10 original Content bodies and1 daemon body on Linux. Subsequent counter-custody regression adds the11th Content body on both systems. | [Linux Content](checks/s9-root-qualification-20261007/15-linux-filesystem-qualify.stdout), [Linux daemon](checks/s9-root-qualification-20261007/16-linux-root-qualification.stdout); corrected [Content](checks/pre-s8-r2-counter-custody-20261007/09-linux-content-proof.stdout) and [daemon](checks/pre-s8-r2-counter-custody-20261007/10-linux-daemon-proof.stdout); [report](PRE-S8-F0-20261007.md) |
| F1 | CLOSED: Linux WAL create/publish/read/reopen; three pack layouts; public checkpoint/allocation owner removed; both profiles compile. | [Linux provider proof](checks/pre-s8-store-foundation-20261007/09-linux-serverless-store.stdout), [final Linux build](checks/pre-s8-store-foundation-20261007/25-linux-build-cfg-corrected.stdout); [report](PRE-S8-F1-F4-20261007.md) |
| F2 | CLOSED: real second process holds writer; typed Busy before write effects, session healthy, later explicit call succeeds. | Same [provider proof](checks/pre-s8-store-foundation-20261007/09-linux-serverless-store.stdout); composed [Commit proof](checks/pre-s8-branch-overwrite-20261007/11-linux-store-commit.txt) |
| F3 | CLOSED: reads complete during a process-held writer; separate fixed daemon readers. | [Provider proof](checks/pre-s8-store-foundation-20261007/09-linux-serverless-store.stdout), [adapter report/proofs](PRE-S8-F6-F7-20261007.md) |
| F4 | CLOSED: consuming seal refuses shared ownership, checkpoints/closes, verifies one file and reopens; no sidecar deletion. | [Host final](checks/pre-s8-store-foundation-20261007/11-host-serverless-final.stdout), [Linux](checks/pre-s8-store-foundation-20261007/09-linux-serverless-store.stdout) |
| F5 | CLOSED: macOS Init/seal → authenticated stream → named Linux volume → daemon full small-root oracle after source/sealed-host removal; both SQLite versions in returned manifest. | [Cross-platform handoff](checks/pre-s8-native-install-20261007/17-macos-linux-handoff.txt), [report](PRE-S8-F5-20261007.md) |
| F6 | CLOSED: provider-independent store adapter, bounded root bind, shared immutable cache, batched objects/lengths and serial ranges; application wiring supplied by F5/F12/F13. | [Adapter report/proofs](PRE-S8-F6-F7-20261007.md), [native control](checks/pre-s8-branch-overwrite-20261007/12-linux-native-control.txt) |
| F7 | CLOSED:4 bound Workspaces, concurrent callers read32 complete files independently over one overlay and shared Store/cache. | [Adapter report with09 Linux proof](PRE-S8-F6-F7-20261007.md) |
| F8 | CLOSED under overwrite policy: Committed, current-root UpToDate, stale overwrite, missing dependencies, process Busy/no-stage/no-local-change, new Workspace root, known publication/failed install, nested unknown custody. | [Current Linux Commit](checks/pre-s8-branch-overwrite-20261007/11-linux-store-commit.txt), [atomic History](checks/pre-s8-branch-overwrite-20261007/09-linux-atomic.txt), [composition report](PRE-S8-F8-COMPOSITION-20261007.md), [unknown correction](PRE-S8-NESTED-UNKNOWN-20261007.md) |
| F9 | CLOSED under overwrite policy: real daemon subprocesses on one named volume, overlapping Saves, A-then-B same-head overwrite with both parents retained, SIGKILL after5 acknowledged publication batches, fresh daemon full-root reopen. | [Linux process proof](checks/pre-s8-shared-processes-20261007/08-linux-sharing.txt), [report/custody](PRE-S8-F9-20261007.md) |
| F10 | CLOSED:32 finite arrivals for each of6 classes in4 Workspaces,1060 original jobs, every class/Workspace progresses, credit/outstanding0 and4 acknowledged physical namespace reclaims. | [Host](checks/pre-s8-finite-engine-20261007/11-host-terminal-proof.txt), [Linux](checks/pre-s8-finite-engine-20261007/16-linux-terminal-proof.txt), [independent validation](checks/pre-s8-accounting-20261007/71-final-independent-validation.txt) |
| F11 | CLOSED within owner size waiver: all28 mixed paths/metadata/raw symlinks/hard links,100000 names, full500000000-byte dense oracle, full1000000019-byte sparse oracle. | [Linux mixed](checks/pre-s8-complete-roots-20261007/15-linux-mixed.txt), [dense](checks/pre-s8-complete-roots-20261007/17-linux-dense.txt), [sparse](checks/pre-s8-complete-roots-20261007/18-linux-sparse.txt); final explicit-byte-copy [host wide](checks/pre-s8-accounting-20261007/16-byte-copy-host-proof.txt), [Linux wide](checks/pre-s8-accounting-20261007/21-byte-copy-linux-proof.txt); [report](PRE-S8-F11-20261007.md) |
| F12 | CLOSED: SDK client/runtime, Bridge codec/contract/data framing, API-core and daemon upstream retired; active workspace test/example compilation and Clippy pass at retirement, later affected checks retained. | [Retirement/path/check ledger](PRE-S8-F12-20261007.md), [exact LOC](checks/pre-s8-transport-retirement-20261007/20-loc.json) |
| F13 | CLOSED before Exec: authenticated mount binding, Store-half Commit, status, terminal unmount, fork and anchored history; typed Busy, stale-token and exact lost-reply custody. Status uses zero Store SQL. | [Current Linux controls](checks/pre-s8-branch-overwrite-20261007/12-linux-native-control.txt), [native record/custody report](PRE-S8-F13-20261007.md) |
| F14 | Selected pre-S8 work COMPLETE: registered direct engine/Commit routes actually run on both systems; independent family/count validation, supported process/cgroup/file phase observations. E3 exact peaks/unsupported attribution INCOMPLETE; numerical acceptance OWNER_DEFERRED. | [Host engine](checks/pre-s8-accounting-20261007/31-host-engine-run.txt), [Linux engine](checks/pre-s8-accounting-20261007/39-linux-engine-run.txt), [host Store](checks/pre-s8-accounting-20261007/66-host-store-v3-run.txt), [Linux Store](checks/pre-s8-accounting-20261007/69-linux-store-v3-run.txt), [independent report](checks/pre-s8-accounting-20261007/72-final-accounting-report.json), [full scope/limits](PRE-S8-F14-20261007.md) |
| F15 | CLOSED: explicit paid R2 pass through installed Store ports and operation-owned overlay records;1/1024-file cases and authenticated installed mixed root; mount itself never walks the tree. | [Host](checks/pre-s8-installed-qualification-20261007/03-host-proof.txt), [Linux](checks/pre-s8-installed-qualification-20261007/06-linux-proof.txt), [report](PRE-S8-F15-20261007.md) |

## Deterministic gates and limits

Fresh object-cache binds use10 batches for1 file,11 for1024 and12 for100000,
following bounded canonical tree paths. A warm repeat uses zero object Store
demands; the coherent history snapshot is still paid. Three object IDs batch into
one call and two lengths into one call. Actual reads finish under a process-held
writer. One write attempt has no retry/busy wait. Large data or namespace oracles
use fixed windows and enforced cache bounds; no total file/Save cap was introduced.

Small Commit proofs use3 writes:1 initial reservation +1 publication +1 atomic
History. Large/refill proofs retain explicit additional reservations. F14's
100000-name changed-root operation writes8292 pack bytes with820 Store statements;
UpToDate writes0 pack bytes with31 statements. Both have20 engine statements in
the original capture/install pair. Their8191 scan steps are the proven fixed
8192-slot signature ring, with zero unexplained scans. Count limits are structural,
not a new owner latency threshold. No transaction spans the whole Save/Commit.

The only new Init measurement is198720291ns against155291459ns retained MEMORY
evidence:10×198720291 >11×155291459, a27.9660145379% regression. Store allocation
20574208B versus20590592B saves16384B. The original comparison is FAIL, accepted
as the new exact WAL baseline; see [all identities/arithmetic](PRE-S8-INIT-WAL-RESULT-20261007.md).
All eight prior speed failures and eight prior strict-allocation failures remain
unchanged. New strict-allocation selections are NOT_RUN — mechanism removed.

## Failures, ineligible runs and unrun scope

“Preserved” means original evidence is unchanged, including a failure fixed later.
It does not mean every historical failure is an unresolved product defect.

- Each feature report above owns its complete iteration ledger: compile failures,
  stale bind-mount refusals, fixture/oracle corrections, actual product regressions
  and their corrected proofs. The failed R2 counter-custody and nested-unknown
  regressions have product corrections and host/Linux proofs, not erased failures.
- F11's original [wide run08](checks/pre-s8-complete-roots-20261007/08-host-wide.txt)
  passed its oracle but is budget-INELIGIBLE at53.88s. Explicit52.93s preparation
  and separate clone/oracle proofs replace its qualification role, not its verdict.
  Earlier log clocks are Rust-body diagnostics unless an exact external clock was
  recorded; never promote them to complete-command measurements.
- [Terminology iteration](PRE-S8-TERMINOLOGY-20261007.md) retains two overlapped
  host invocations as INELIGIBLE, followed by serialized corrected proofs. F0
  receipt11 has a compiler failure despite an exit0 footer; that is not a PASS.
- F14 retains failed observer fixture04, first Store assertion34/35, read-only
  CLI36/37, second Store assertion53/54 and validator55. The Store assertions
  followed known successful publication; final66/69 and independent71 pass.
  The two superseded Linux Store selections29/51 were NOT_RUN. No failed raw
  output is edited to match the current diagnostic.
- All Durable execution is **NOT_RUN — deferred by owner for Disposable-only
  development**. Both provider branches compile. No new Durable test, diagnostic
  or measurement was executed.
- Actual native files above4GiB are **NOT_RUN — waived by owner**. The1000000-name
  legacy proposal is NOT_RUN/unselected;100000 names are fully proven here.
- Removed host-mediated transport selections are prospectively WITHDRAWN, not
  ported or retrospectively relabeled. E04 stays functionally closed on its
  original topology and was never rerun. The27-row disposition is retained in
  [the diagnostic registry](../../../benchmark/fs-bench-pro/registry/pre-s8-accounting-v1.json).
- Continuous phase peaks, short-phase interior coverage, exact internal
  SQLite/driver copies and visited pager pages, exclusive physical I/O and
  runnable-only wait remain INCOMPLETE/UNAVAILABLE at their stated scopes.
  Supported raw observations are in F14; numerical latency/RSS gates are deferred.
  No unsupported dimension is represented as zero or a numerical PASS.

## Identities, custody and preserved work

All builds are locked Rust/Cargo1.85.1 from repository root, preserving
`aes_armv8`, `polyval_armv8`, `chacha20_force_neon` and `+aes,+sha2`.
Construction workers are1. Functional binaries are test/dev builds as pinned in
their build receipts; the sole Init measurement uses its pinned release driver.
Each feature report retains source/build/binary/workload hashes. F14 registration
manifests26/28/63/64 additionally bind all selected source/build inputs, actual
binaries and observer hashes. Natural functional cache is not OS-cold evidence;
named warm-cache count checks and the one Init cold-content selection keep their
own narrower meanings.

Linux image is
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
ARM64 Linux6.12.76-linuxkit. Host is ARM64 Darwin25.4.0. SQLite versions are
macOS system3.51.0 and Linux bundled3.53.2, both recorded in F5's install manifest.
Global Store uses `sqlite-wal-off-v2` (Disposable); implemented Durable uses
`sqlite-wal-full-v2`. Overlay is independent schema16 MEMORY/OFF/EXCLUSIVE,
8MiB credit,64KiB lifecycle reserve,16 Workspaces and16+2 jobs per Workspace.
Linux SQLite opens only on native `/tmp` or named `/store`, never repository `/work`.

Custody remains exact: F8/nested-unknown/native-control tests retain original
client/provider unknowns even when a separate observer sees a committed root.
F9's killed daemon leaves its original missing-reply unknown after5 acknowledged
batches; fresh reopen proves Store health without resolving that original caller.
Known publication followed by failed local install retains its original capture.
Successful fixture cleanup is explicitly recorded after owners stop. No production
GC, guessed rollback/deletion or replay resolves those outcomes.

Failed fixtures remain at their report paths. The sealed100000-name preparation
remains at `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-q1-24248-Wide`,
Store SHA256 `39d0bd5c614dc3b7d4ed2ec7477b2ff909e5030c817bb9ade1afe91c0e86c08e`.
Final F14 host artifacts and failed copies are listed in its custody section.
Successful F14 Linux containers exit and remove their local files; text/JSON
evidence remains. F5/F9 named-volume cleanup is acknowledged in their receipts.
The sole Init closed Store and prior ignored build/benchmark artifacts remain.

The root `crates/` reference and excluded predecessor source are unchanged.
Protected containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59`, `d2550144998b`
remain running and untouched. These three original untracked files remain
unstaged and unchanged: `HANDOFF-PRE-S8-SERVERLESS-20261007.md`,
`HANDOFF-S7-S9-RESUME-20261006.md`, `S7-S9-SPEED-TEST-PLAN.md`. The latter two were
not authority. No push, release, deployment, new worktree or checkout reset.
Final Git state is checked after the documentation/evidence commit; ignored
build outputs and retained fixtures are not a claim of an empty filesystem.

## What the next stages own

S8 consumes the direct Store adapter, ready overlay owner, explicit bound tokens,
authenticated controls and exact Commit/failure custody. It still implements
FUSE attachment/readiness, ordinary Bash Exec/output/lifecycle, hiding the Store
from Bash, real concurrent Bash, permissions/kernel caches and request/open/lookup
ownership. This work does not claim kernel mount readiness or Exec capability.

S10 still captures and normalizes a live Workspace namespace into faithful Content
input. Current Store-half tests construct the changed root directly through
Content, paired with explicit local mutations; they are not a live normalizer.
S11–S13 and root-reference retirement remain later work. Remote Save, host data
application assembly and restart custody were not built.

## Exact production source size per local commit

The following table is derived from the exact per-commit comparisons and commit
messages, using `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
It counts production Rust and shipped SQL, excluding comments, inline/test-only
code, tests, examples, tooling and docs. Each comparison is first parent against
the staged/committed product tree, with its original JSON snapshot receipt.
Reference65417 and excluded predecessor36325 are unchanged in every row.
The excluded integration subtotal drops6840→6549 only at transport retirement.
No reduction is claimed as an algorithmic simplification.

| Commit | Combined before → after (delta) | Core before → after | Active before → after | Excluded integration before → after | Purpose |
| --- | --- | --- | --- | --- | --- |
| `727476a4d` | 170293 → 170765 (+472) | 104876 → 105348 | 61711 → 62183 | 6840 → 6840 | feat(content): qualify whole-root topology with indexed scratch |
| `8c66eb8b4` | 170765 → 170469 (-296) | 105348 → 105052 | 62183 → 61887 | 6840 → 6840 | feat(persistence): open shared WAL Stores on Linux and seal host handoffs |
| `c5fae7e3a` | 170469 → 170469 (+0) | 105052 → 105052 | 61887 → 61887 | 6840 → 6840 | bench(init): register one pre-S8 WAL decision against retained memory evidence |
| `d5ef7ba4e` | 170469 → 170525 (+56) | 105052 → 105108 | 61887 → 61943 | 6840 → 6840 | feat(persistence): build private Disposable Init under memory journal until seal |
| `1e5f5033f` | 170525 → 170526 (+1) | 105108 → 105109 | 61943 → 61944 | 6840 → 6840 | fix(content): keep qualifier proof counts local to one attempted pass |
| `4e8247af5` | 170526 → 170470 (-56) | 105109 → 105053 | 61944 → 61888 | 6840 → 6840 | refactor(persistence): retain WAL throughout Init and Commit |
| `e2f2e62c7` | 170470 → 170951 (+481) | 105053 → 105534 | 61888 → 62369 | 6840 → 6840 | feat(daemon): bind independent Workspaces directly to an opened Store |
| `656575d7a` | 170951 → 170965 (+14) | 105534 → 105548 | 62369 → 62383 | 6840 → 6840 | feat(history): stage and publish a saved candidate in one transaction |
| `a40acd673` | 170965 → 170969 (+4) | 105548 → 105552 | 62383 → 62387 | 6840 → 6840 | refactor(storage): reserve combined Save blocks and count explicit refills |
| `01e60021a` | 170969 → 171109 (+140) | 105552 → 105692 | 62387 → 62527 | 6840 → 6840 | refactor(overlay): name directory entries and operation records consistently |
| `52e1f2e18` | 171109 → 171493 (+384) | 105692 → 106076 | 62527 → 62911 | 6840 → 6840 | feat(daemon): compose capture Save atomic publication and local install |
| `2f4668b73` | 171493 → 163223 (-8270) | 106076 → 97806 | 62911 → 54932 | 6840 → 6549 | refactor(runtime): retire host-mediated Store transport |
| `a5f8b7f27` | 163223 → 164082 (+859) | 97806 → 98665 | 54932 → 55791 | 6549 → 6549 | feat(install): stream a sealed host Store into the authenticated daemon |
| `9041f338d` | 164082 → 164088 (+6) | 98665 → 98671 | 55791 → 55797 | 6549 → 6549 | fix(storage): preserve uncertainty nested in cleanup failures |
| `bd5ab61d6` | 164088 → 165678 (+1590) | 98671 → 100261 | 55797 → 57387 | 6549 → 6549 | feat(control): serve authenticated Workspace and history commands in daemon |
| `8e5f8066f` | 165678 → 165669 (-9) | 100261 → 100252 | 57387 → 57378 | 6549 → 6549 | feat(history): publish Branch candidates with overwrite-only semantics |
| `b2a9e87b5` | 165669 → 165669 (+0) | 100252 → 100252 | 57378 → 57378 | 6549 → 6549 | test(daemon): qualify installed roots through owned overlay records |
| `916aa7d81` | 165669 → 165669 (+0) | 100252 → 100252 | 57378 → 57378 | 6549 → 6549 | test(daemon): prove complete large installed roots through direct ports |
| `01f72eee0` | 165669 → 165673 (+4) | 100252 → 100256 | 57378 → 57382 | 6549 → 6549 | feat(daemon): observe terminal cleanup and prove finite class service |
| `a2189c250` | 165673 → 165673 (+0) | 100256 → 100256 | 57382 → 57382 | 6549 → 6549 | test(accounting): register direct engine and Store diagnostic routes |
| `772bda27a` | 165673 → 165673 (+0) | 100256 → 100256 | 57382 → 57382 | 6549 → 6549 | test(accounting): distinguish bounded signature scans from namespace work |
| `02922afa1` | 165673 → 165673 (+0) | 100256 → 100256 | 57382 → 57382 | 6549 → 6549 | test(accounting): charge candidate-ring loading for every Save |
| `1fd566b74` |165673 →165673 (+0)|100256 →100256|57382 →57382|6549 →6549|Preserve diagnostics, scope/limitations handoff and repaired retired-source links|
| `0c736c4bf` |165673 →165673 (+0)|100256 →100256|57382 →57382|6549 →6549|Register focused resource-growth and repeated-Commit checks; external instrumentation only|
| This resource-growth results commit, first parent `0c736c4bf` |165673 →165673 (+0)|100256 →100256|57382 →57382|6549 →6549|Retain14 passing selections, resource interpretations and successful owned cleanup|

Net from the dispatched starting point: combined170293→165673 (−4620),
core104876→100256 (−4620), active61711→57382 (−4329). The transport retirement
commit alone is−8270 combined/core:−7979 active and−291 excluded API-core.
Later implementation additions are reported separately, not hidden in retirement.
The final documentation/evidence-only commit has unchanged totals and its own
[exact staged LOC receipt](checks/pre-s8-accounting-20261007/75-final-handoff-loc.json);
no fictitious zero-sized product is reported. The final
[document/custody check](checks/pre-s8-accounting-20261007/74-final-document-custody-check.txt)
resolves188 local targets, verifies all489 indexed artifacts unchanged, confirms
the three protected note hashes against the original Init receipt, and observes
the four protected containers still running.

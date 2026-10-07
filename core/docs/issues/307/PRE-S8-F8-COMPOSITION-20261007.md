# F8 in-process Store Commit composition

> **Status:** Store-half composition and Disposable host/Linux functional
> proofs after `01e60021a`; F8 Store-half implementation closed at this checkpoint.

- Reuse Content construction and Save, atomic History stage_and_commit, bounded
  checked_base, overlay Capture/ResolveFailed/InstallPrepared and Workspace's
  prepared paired base install. No native live namespace normalizer is built;
  S10 supplies that producer later. Before S8 callers build changes directly
  through Content over the capture's frozen base/context.
- Correct Storage save/provider.rs failure custody: the AuthenticatedObjects
  implementation currently consumes StorageError in its Content mapping without
  retaining it. Add a real-Store missing-read regression first; factor the map
  by reference and retain the original for take_failure/finish, just as SaveSink
  does. This prevents an unknown same-Save read becoming a guessed definite
  Commit failure. No provider retry or error-driven alternate read is added.
- Daemon store/operation.rs and bind.rs: share the Workspace through Arc, keep
  the Branch snapshot behind brief metadata ownership, and expose one active
  Commit admission per Workspace. No mutex spans a Save or history operation.
- Add store/commit.rs: one synchronous capture → independent Storage/Save →
  caller Content producer → finish → bounded candidate checks/prepared install →
  one atomic history transition → original local install. Validate scope/profile
  and stable root serial; derive the next local snapshot from the known history
  result without rereading or refreshing Branch state.
- Add store/commit_types.rs and settlement helper: retain original errors,
  attempted owner Completions, request/candidate/capture, Save counts, known
  publication and install disposition. Resolve local capture once only after
  definite nonpublication. Unknown and known-publication/local-install failure
  retain custody and block another Commit; never discard or guess success.
- Add external daemon Store-half tests over a real installed Store: direct Content
  change, Committed, UpToDate, exact stale-head conflict, missing dependency,
  same-Save read failure, real process-held writer Busy with no stage/local view
  change, and a newly bound Workspace reading the published root. Count initial
  reservations + refills + publication batches + one history transaction.
  Cover explicit local install failure without claiming history did not happen.
- Update API/architecture descriptions. S8 FUSE/Exec and S10 normalization are
  explicitly excluded. Host first then affected pinned Linux binaries with
 100s stops, serial execution, appended failures, final checks and exact LOC.

The proof scope was extended before Linux validation with an external forwarding
HistoryCatalog adapter. It delegates to the real provider and either holds a
real second-process writer at the publication boundary or deliberately loses
that one returned acknowledgement. It records actual shared-session write counts
without adding a product hook. The latter tests original caller custody, not
native SQLite I/O/quarantine. A further body writes after capture and proves
both current-live preservation and a subsequent Commit of that later write.

## Functional outcomes and exact counts

Receipts are [checks/pre-s8-commit-composition-20261007](checks/pre-s8-commit-composition-20261007/).
Receipt01 built the real-Store read-custody regression. Receipt02 accidentally
selected an older hash-suffixed binary and ran zero matching tests; it is
**INELIGIBLE — stale artifact selection**, not PASS. The actual build-listed
binary in03 fails because take_failure loses the original ObjectMissing. After
the by-reference mapping/first-original retention correction,04/05 build and
pass both take_failure and finish settlement paths. Failed receipt03 is retained.

Build06 found private Generation field access; the existing number() accessor
corrected that source error. Build07 passed with an unused fixture-count warning;
08 records the fixture count and builds without it. The first Commit run09 passes
three bodies but its refusal body fails an assertion: the hand-built negative
root omitted the direct-reference metadata that real Content constructors emit.
The assertion did not print the original deciding variant. Source inspection
identified that fixture defect;10 adds the reference and original-error diagnostic,
and11 passes the corrected body. This does not change product validation or
relabel09. Later12–15 add and prove real publication-boundary contention,
acknowledgement-loss custody and mutations after capture.16 covers installed
Store reads under the changed shared binding.17 pins15 exact source/build inputs;
18 verifies them and builds in Linux.19–21 pass all seven Commit bodies, the read
custody regression and all three installed-Store bodies on final Linux source.

A changed small Commit and UpToDate each count3 Store write transactions:
1 initial reservation +0 refills +1 publication +1 atomic history transaction.
The external history-boundary proof correlates that equation against actual
shared-session write counters. History Busy after Save records2 acknowledged
Save writes, one failed BEGIN statement, zero new stage rows and preserved local
bytes. A later explicit Commit counts3 writes and succeeds. Busy before Save
never calls the constructor. Neither case retries automatically.

Committed installs the exact published root. UpToDate keeps the exact head/root.
A stale Workspace gets exact expected/actual HeadMoved fields, no new stage and
one local resolution preserving its changed bytes. Missing dependency and
same-Save read failures retain their original typed Storage errors. A new
Workspace reads the complete published small root. A held producer permits reads
and refuses a second same-Workspace Commit before effect.

A write after capture remains live through the first install; that published root
contains the capture's G bytes while the live Workspace contains later H bytes.
The next explicit Commit publishes H with the correct parent; an existing other
Workspace still reads G and a newly bound Workspace reads H. All8 directory
entries survive. Inputs are built directly through Content to match known real
overlay writes; this does not implement or qualify S10's namespace normalizer.

Stopping the local owner after construction produces a known Committed history
result followed by an unattempted Install. Original request/capture/prepared input
custody survives, local metadata is not advanced, another Commit is refused and
a fresh Workspace can read the published bytes. Explicit fixture teardown occurs
after the old owner is stopped; it is not an automatic product rollback/unmount.

The acknowledgement-loss seam deliberately leaves an **original terminal
UnknownOutcome**. It retains binding, route, capture, candidate intent, prepared
root and original capture completion; no resolution or install is attempted.
A separate observer sees the real published root but does not settle the
original unknown. The test explicitly stops the owner before disposing of its
fixture. This is a public-port acknowledgement-loss proof over a real Store,
not an execution of native SQLite I/O failure/quarantine or restart recovery.

All proof binaries run individually under100s wall stops and finish below1s of
test-body time. Profile: Disposable WAL/OFF for the Store, schema16 MEMORY/OFF for
overlay. Linux databases are container-local, never `/work`. Cargo1.85.1 is locked
with repository ARM64 inputs and LAYERFS_CONSTRUCTION_WORKERS=1; image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Cache state supports named count checks only; there is no OS-cold timing or
performance admission claim. Durable compiles but execution stays NOT_RUN —
owner-deferred. No E04 or >4GiB native-file test was run. Failed09's fixture
remains; successful fixture cleanup and bounded child release/join complete.

## Final identity and retained gaps

Clippy22 rejected only `Option::map_or(true, predicate)`; the equivalent
`is_none_or(predicate)` correction passes all-target Clippy26 for Storage,
Persistence and Daemon. Formatting27 and boundary28 pass at that final source;
44 unchanged guard self-tests in25 pass.29 pins every changed Rust input,
Cargo.lock and ARM64 configuration;30 verifies those hashes and builds all
three affected Linux test binaries. Linux functional receipts19–21 remain the
covering evidence: the sole subsequent change is that equivalent Option
expression, not altered behavior or an additional timing sample. Both profile
implementations build; all Durable execution remains owner-deferred NOT_RUN.

The failed09 fixture remains at
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-7752-commit-refusals`.
No unrelated process/container was stopped. The original terminal unknown in
receipt19 remains unknown despite the observer's root; explicit test-owner stop
and fixture disposal are recorded separately from product resolution. The next
stage still requires transport retirement and native install/control. S8 needs
FUSE/Exec; S10 needs the live namespace constructor. Neither is claimed here.

Production LOC: 171109 -> 171493 (delta +384). Exact first-parent/staged
comparison in31: core105692→106076, active62527→62911; reference65417,
excluded predecessor36325 and excluded integration6840 unchanged. Counted with
the pinned tools/production_loc.py over archived snapshots, including shipped SQL.

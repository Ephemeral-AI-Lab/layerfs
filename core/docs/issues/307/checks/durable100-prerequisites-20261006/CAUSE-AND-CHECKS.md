# Prerequisite and final-wave causes

Parent9b74ac035 already removes the spurious reservation. Its qualifying
Durable100 checkpoint remains FAIL at83,626,792 ns,5,296,128 allocated bytes,
18 total write transactions and five Storage publications. Original reference
41,992,917 ns remains unchanged. The complete tail cohort and all prior failures
are retained separately; no unchanged candidate was measured again.

Project previously finished scan attributes/targets in a separate Save, then
opened the file Save. The revised flow retains one bounded prerequisite Save,
finishing before inode reservation and the separate tree Save. Public Init success
and publication order remain, while an earlier file failure may discard scan
objects that were not yet published. Existing acknowledged objects remain valid.
Acquisition cleanup still precedes finishing the tree and publishing history.

Storage's final drain previously registered ready packs before sealing its open
groups, creating a second publication even when both fit within the independent
canonical/physical/row limits. Only final preparation now defers that registration
until closing; ordinary accepting waves remain bounded and publish normally.
The existing reference-ordered publisher partitions the combined final output
under unchanged limits. Same-Save authenticated reads keep their old behavior.
No import-sized transaction/output, new engine path, worker or capacity applies.

Locked count `15` observes three Storage publications/47 statements/27,287 VM,
versus five/54/27,347 in the earlier queued-tail diagnostic. Reservations remain
two/8/108 and acquisition writes remain eight. Init write commits fall17→15;
including the schema commit, total writes18→16. Before both corrections in this
turn the original count was19. Canonical root remains
ca1c20f806277e3fb0447ef526a5ca8d298753d01667f501e8b58b6350cd86f1.
Actual provider-linked read-only EXPLAIN `16` covers the unchanged reservation
statements plus representative1/2/4/8/16/32-row pack/location INSERT programs,
with foreign keys enabled and matching SQLite3.51.0 source/options. EXPLAIN never
executes an INSERT. Runtime publication fullscan steps remain0; plan opcodes are
not substituted for actual VM work or device/sync counters. All diagnostic
clocks/RSS/rates remain INELIGIBLE. The changed binary has matching pre/post hashes
under the nonblocking normal primary phase7 lock. One later comment-only correction
is recorded in count-summary.json, without relabeling its original source hashes.

`02` exercises one baseline regression body and fails with four publications
against an overly strict expected two. Source diagnosis identified the separate
final-wave drain/closure; the prerequisite-only regression was corrected to at
most three and passes in `07`'s77-body Project coverage. `10` then reproduces the
independent-charge final-wave regression: three write commits instead of two.
`12` passes261 bodies across all four affected packages after `11`'s locked
`--no-run` build. Full namespace oracle, native aliases/symlinks, typed failures,
unknown custody and independent bounds remain checked. No test hangs/timeouts.

The first instrumented invocation `08` supplied a nonexistent `/input` path;
it created a Store but refused root inspection before acquisition begin. Its
failed output is retained. The later explicitly selected changed-source diagnostic
uses the actual sealed `/payload` source and a new Store; no owned in-flight
operation, mutation or failed acknowledgement was replayed.

Host Clippy `17` passes warnings denied. Linux `18` builds the exact two-package
portable selection with `--no-run`, then `19` passes129 bodies in3.679s. Linux
`20` checks all four affected packages/targets with warnings denied. Native global
Persistence remains macOS-only. Each test has an explicit110s inner deadline
where applicable,1s kill grace and120s outer ceiling. Boundary `21` scans652
production Rust/SQL files, and formatting `22` passes. The unchanged tooling
counter/guard40-test evidence is reused from the preceding correction. Existing
unused-fuser-patch Cargo warnings remain; no vendor/dependency/harness change.

The final operation is separately registered at committed source. These count and
functional checks do not establish speed, Family2 history, integrated runtime or
S7/S9 completion. Larger/v1 cases and all other profiles remain unrun at that
new checkpoint, with earlier receipts preserved at their own identities.

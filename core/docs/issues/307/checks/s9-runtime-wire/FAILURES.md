# Retained failures and repairs

Every command has a fresh log/receipt; originals are not overwritten. No wall timeout occurred.
All cases are functional diagnostics/covering tests with uncontrolled caches, not performance samples.

- bridge-tests-initial: 5 logical/5 existing native passed; two new native framing
  cases failed with WouldBlock/peer EOF. The accepted macOS test socket inherited
  O_NONBLOCK from its nonblocking readiness listener. The fixture now explicitly
  selects blocking sockets with2-second read/write fences. Product I/O never retries.
  native-fixture-diagnosis.json retains the cause. Repaired build/tests pass.
- sdk-reply-build: Rust E0603, parent reexport of a private child reply module.
  Changed only that module's crate visibility; rebuilt exact source.
- sdk-wire-tests-build: test closure could not relate borrowed failure-node lifetimes.
  Replaced it with an ordinary lifetime-preserving external helper; rebuilt.
- sdk-wire-functional-initial: four cases passed; output fairness case assumed the
  control cursor reset after prior control sends. Source preserves round-robin state.
  The fixture now checks Demand/Save/Control turns and drains its bounded receipt
  queue while receiving. Product scheduling was unchanged. The affected test passed.
- host-scope-clippy-initial / host-scope-clippy-owner-check: large returned socket
  ownership, single-element loop, large new Binding enum and a needless test Vec.
  Binding is boxed; ordinary loop/fixture are corrected. Three allocation/refusal
  constructors deliberately return original owners inline with scoped lint reasons,
  avoiding another allocation while returning TryReserveError. Both-platform
  warning-denying checks cover this source; no crate-wide lint suppression.
- Two apply_patch attempts had malformed/outdated hunks; neither changed files.
  Corrected the patch syntax/context before applying it.
- Source review found native-failed framing copies counted only after send success.
  Counts now occur at encoding, with separate completed send/encoded-fragment/
  header/initialization observations. The added actual quarantined-send regression
  retains7 copied bytes/40 header bytes and zero socket/crypto work.
- Service source review found byte reserves alone did not protect first demand/
  control job slots. Actual live class ownership now preserves those slots through
  caller-held results, validates a3-slot minimum and reserves128KiB control bytes.
  A public real-provider test proves both slots remain after ordinary Save admission.

Earlier build identities were diagnostic during module/fixture construction.
The early SDK header build overlapped formatting completion; it is not used as a
final execution identity. Final custody source uses fresh no-run builds and before/
after executable hashes. Prior passing/source-equivalent receipts retain their
original identities rather than being relabeled as the later37/19-body checks.

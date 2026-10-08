# R2 component checkpoint: event-driven original pending completion

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Source after4236225ee. Native R2 and full R2–R5 remain incomplete.

`Pending` implements Future using one fixed registered Waker slot. Registration,
replacement and publication/loss ordering preserve one original result and its
credit. Waker clone/drop/wake behavior runs outside the notification lock. The
existing synchronous caller API is retained. No executor, admission-credit wait
queue, FUSE dispatcher or native mount is introduced. The asynchronous engine
port can consume this future without occupying a service worker while pending.

The [selection](01-selection.json) and [source hashes](02-source-identity.json)
define the component scope. A later callback can still violate its executor's
own resource/behavior contract: this fixed slot does not bound arbitrary caller
Waker/task allocations or implement native task disposal. Publisher loss shares
the notification settlement path by source inspection; the new external tests
exercise success and stopped unattempted outcomes, not injected publisher panic.

| Check | Outcome and original evidence |
| --- | --- |
| Host locked all-target build, tests/examples compiled first | PASS [build](03-host-build.txt), [result](04-host-build-result.json) |
| Host public tests | PASS18: new Future5, completion ownership3, completion storage1, Owner9; [commands/hashes/results](09-host-results.json) |
| Linux locked all-target build and warning-denying Clippy | PASS [transcript](11-linux-build-checks.txt), [original container command](10-linux-create.json) |
| Linux public tests | PASS same18; [commands/hashes/results](12-linux-test-results.json) |
| Host warning-denying all-target Clippy, fmt, boundary, tooling | PASS [check results](19-check-results.json); boundary759 source files, tooling47 tests |
| Owned Linux container cleanup | PASS [exact identity/exit/removal](14-linux-cleanup.json) |
| Native FUSE, mounted coherence/Commit, performance | NOT_RUN; no acceptance inferred |

Every test invocation had an explicit100s wall stop; none timed out. New event
waits have5s inner failure bounds. Store execution is absent from this selection:
the actual Overlay uses MEMORY/OFF/EXCLUSIVE. Durable remains NOT_RUN by owner.
Natural functional caches and debug/test profiles establish no speed, storage or
residency qualification. Builds preserve repository ARM64 flags and export one
construction worker. No R1 functional treatment was repeated unchanged.

The fixed slot adds32 admission bytes per job on both observed architectures.
Host lifecycle charge2000B ×32 slots =64000B≤65536B reserve; Linux1992B ×32
=63744B≤65536B. The ordinary saturation/lifecycle-capacity tests retain all
configured slots and zero receipt overruns. Existing external allocation checks
pass for held, parked and stopped outcomes; they do not count a native executor's
future task storage or claim whole-process bounds.

No fuser byte, dependency, Cargo lock, SQL algorithm, root reference, predecessor,
protected owner note or historical receipt is changed by this component. The
separate scoped fuser extension is owner-authorized but still to be implemented.
The sole new Linux container is removed after observed exit0; no mount/volume or
global Store was created. Exact per-commit LOC is recorded in the accounting
receipt and commit message, with post-commit confirmation appended separately.

Production LOC:170673 ->170710 (delta +37). Core105256 ->105293;
active62382 ->62419; reference65417, excluded predecessors37431 and excluded
integration5443 unchanged. The [exact snapshot comparison](21-exact-production-loc.json)
and [per-file classifications](22-per-file-production-loc.json) use the unchanged
pinned production counter. No relocation or retirement is credited.

Full staged whitespace is FAIL exit2 solely for the final blank line in eight
byte-preserved raw Rust test transcripts; [original check](23-document-verification.json)
retains that failure and its failed validation assertion. Source/Markdown/JSON
whitespace is checked separately in the subsequent scoped receipt. Raw evidence
is not rewritten to turn the full check into PASS. All115 inspected local
Markdown targets resolve and the four protected untracked files retain their hashes.

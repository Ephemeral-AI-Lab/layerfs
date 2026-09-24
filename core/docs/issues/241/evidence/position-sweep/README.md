# #241 functional position probe status

The [frozen manifest](../../position-manifest-v1.tsv) has **264** exact
functional positions: 192 SHA-selected bands and 72 named edges across the
four pristine sizes and three edit operations. It is frozen before the full
sweep. The five exploratory mounted attempts below used different frozen IDs
and successive test-only harness identities; none is a performance sample.

| Attempt | Frozen offset | Result | What the receipt establishes |
| --- | ---: | --- | --- |
| [`1mib-delete-band00`](FIRST_CASE.md) | 45,395 | **FAIL** | Product bytes and Commit verified from retained Store; mounted primary result masked by unmount I/O error. |
| [`1mib-delete-band01`](SECOND_CASE.md) | 96,547 | **FAIL** | Product bytes and Commit verified; one-byte `dd` mounted verifier returned Exec `Unknown`, then unmount I/O error. |
| [`1mib-delete-band02`](THIRD_CASE.md) | 135,511 | **FAIL** | Bounded mounted verifier and cleanup completed; fresh-rebuild root equality was an incorrect oracle for localized edits. |
| [`1mib-delete-band03`](FOURTH_CASE.md) | 206,873 | **FAIL** | Bounded mounted reads made 5 FUSE callbacks and product bytes matched; fresh remount on the same Sandbox returned `Busy`. |
| [`1mib-delete-band04`](FIFTH_CASE.md) | 283,084 | **PASS** | Corrected byte oracle, fresh Sandbox per mounted view, old/new/pristine Branch evidence, 5 read callbacks and confirmed deletion of all four Sandboxes. |

The good finding is concrete: on the retained failed attempts, the projected
splice and explicit Commit published the expected full bytes, and the fifth
attempt proved one complete public SDK mounted functional route. The observed
bad outcomes were in the exploratory **test harness**: its first mounted
verifier used excessive one-byte reads; a later oracle compared a localized
chunked root to a fresh reconstruction; and one Sandbox was reused after a
Workspace unmount. Their original FAIL receipts remain append-only. None of
these findings establishes a Linux ioctl carrier blocker.

The **full 264-case Phase 3 gate remains unrun**, including 10/100/capped
500 MiB sizes, insert/overwrite, exact edges and every remaining band. The
fifth PASS does not establish position-independent latency, a cold-cache PASS,
the four registered Edit→Commit targets or #232's separate admission gate.
The parent integration will run the full functional sweep at one sealed final
source identity, preserving these historical diagnostics alongside its result.

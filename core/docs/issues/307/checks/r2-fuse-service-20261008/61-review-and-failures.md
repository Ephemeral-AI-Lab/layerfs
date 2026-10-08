# Native request service review and retained failures

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This selection starts on8d800de670ab138807ce8e0eb014a0a00ee7463b. It activates the
replacement Fuse and implements its real dispatcher/Owner/Store read composition.
It is a component checkpoint within the active full R2 Goal. No application mount,
native Ready, kernel permissions, normal EBUSY, detach or drain proof was selected
or passed here. Those remain required work, alongside directory callback completion.

Original failures are retained:

| Receipt | Original outcome | Diagnosis and disposition |
| --- | --- | --- |
| [07](07-dispatch-test-build.txt) | Rust E0521 | FailureView's borrowed error trait object inherited the borrow lifetime and prevented downcasting the actual retained static error. The public view now explicitly preserves the original object's static lifetime while borrowing it. |
| [13](13-port-build.txt) | Locked build refused | `metadata --no-deps` had not updated the new daemon→Fuse edge. Full offline resolution changed only that existing local package's dependency list; [14](14-port-lock-delta.json) records the exact delta. No required locked build bypass was used. |
| [17](17-port-test-build.txt) | Rust E0599 | New test called `work` instead of the actual public `OwnerClient::diagnostics`. Only the test call was corrected. |
| [30](30-wakeup-custody-test.txt) | Test FAIL, retained-drop count0 instead of1 | Task→pending event→strong Task waker formed a cycle after all explicit owners were disposed. One fixed weak Notification per request removes that cycle; [33](33-weak-notification-tests.txt) passes all eight dispatcher cases, including exact retention until the last queue owner releases. |
| [Linux build](linux-service-fuse-build.txt) | Rust E0308 | The new native callback used the string constructor for borrowed byte input. It now uses existing `PathName::from_bytes`, preserving the canonical validation rules. [Linux callback build](linux-callback-fuse-build.txt) passes. |

No runtime selection hit its100second ceiling. Build commands preceded runtime
tests; no failure was relabeled PASS or rerun unchanged. These are functional
component checks with natural caches, not time/storage treatments. The elapsed
command values in receipts are diagnostics and establish no performance gate.

Review also identified two issues before final source selection. The initial
daemon adapter cloned a LocalRead at the port and then again for mutable
composition. A borrowed ServiceReply now keeps that payload in its original
Completion; only the mutable composition window and inherited/output buffer are
additional bounded copies. The mixed128KiB data test verifies original inherited
bytes plus locally published bytes through this actual path. This source-copy
accounting is not a measured RSS or kernel-page bound.

The original read operation is now stored before source acquisition, including
name, protected serial and encoded handle. A failure before NativeReadPlan exists
therefore retains those original inputs as well as the exact unattempted engine
command. Factory failure at kernel callback entry retains the same read input.
No failed acquisition, read, publication or release is replayed or resolved by
inspection. Directory GETATTR association and the remaining callback input/phase
coverage must be completed in the next native integration work.

Global Store fixtures explicitly select Disposable/WAL/OFF. Overlay remains
MEMORY/OFF/EXCLUSIVE. Durable execution is NOT_RUN — disabled by owner until
explicit reauthorization. The native callback library has not been mounted in
this selection; no candidate profile, fuser delivery result or readiness fact is
inferred from compilation, component tests or an empty guard finding.

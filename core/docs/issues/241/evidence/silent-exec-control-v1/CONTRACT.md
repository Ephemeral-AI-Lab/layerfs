# #241 silent Exec control v1

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Frozen before the first attempt on 2026-09-25.

This is one distinct, synthetic public SDK diagnostic, case
`issue241-silent-exec-control-v1`. A fresh four-byte source file initializes
one Project; a fresh Branch and Sandbox mount it. The sole Exec command is
`sleep 6`, chosen prospectively to keep the child silent longer than the
native transport's five-second no-progress clock while remaining inside the
30-second Exec deadline. The test records the typed SDK result and elapsed
context wall, then attempts one SDK unmount, Sandbox deletion, and absence
check. Unmount may return either success or a typed failure; both are retained.
There is no range EDIT, Commit, or retry.

The expected mechanism result is `Failure(Unknown, unknown=true)` after
roughly five seconds. The broad 4–10-second test window is diagnostic only,
not a performance limit. Cache state is uncontrolled and
`admission_eligible=false`. Run the first attempt once with a fresh output
directory; preserve its failure or success and the exact source commit/tree,
test binary hash, immutable image ID, command, and raw console. Do not use a
passing synthetic control to relabel or replace the retained
`10mib-delete-band13` failure. The control can show that the transport
mechanism is possible; it cannot establish whether that mechanism caused the
historical small baseline write to fail.

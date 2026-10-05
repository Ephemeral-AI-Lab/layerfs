# S5 failure ledger

> **Status:** Append-only diagnostic evidence; not release qualification.

Inherited from HANDOFF-S5-S6.md: a repeated namespace_daemon command, piped
through grep, hung for more than 12 minutes and was stopped without retained
output. This is FAILED and violates the newly binding 120-second ceiling. No
later passing receipt relabels that run. The handoff also records schema 7→8
assertion updates; three inherited-cutoff expectations changed to the layered
value; three draft payload arithmetic errors and one upstream-demand expectation
corrected before this resume. Their original console output was not retained.

2026-10-05 resume: four older namespace_daemon tests each pass individually
under an explicit 120-second timeout. `append-diagnostic-initial.log` fails
immediately: generated records are 23 bytes, expected WIDTH=24. All writers
panic before submitting a Write. The original tail stops only after writer
joins, causing the earlier infinite wait. WIDTH=23 repairs the source cause;
the panic-safe Stop guard, 60-second tail deadline, 10-second owner/admission
waits and bounded provider gate prevent the test from depending on successful
writer exit. `append-width-repaired.log` passes. The enabled package proof
also passes. No product admission workaround or replay was added.

`zero-period-build.log` fails because the new diagnostic omitted `std::io::Read`.
A test command was mistakenly dispatched before noticing that build failure;
`zero-period.log` retains the same compile failure. The import is corrected;
the separate repaired --no-run build and `zero-period-repaired.log` pass.

`runs-initial.log`: three run-constructor proofs pass, but the large-run case
panics when an empty tail is passed to the existing scanner while it retains
a pending byte. The new zero-run caller now skips the zero-length tail call.
`runs-tail-repaired.log` passes the affected case at 1 MiB, 1 GiB and 1 TiB.

`owner-profile-initial.log`: the second unrelated-population case fails exact
counter equality because it reuses the preceding case's mutated file, adding
a staircase step. Each case now starts with an independent file with the same
history. `owner-profile-repaired.log` passes identical complete work at all
three scales. The original confounded counts remain in the failed receipt.

`clippy-initial.log`: nonminimal boolean in inherited S5 compound validation.
It is simplified without changing refusal conditions. `clippy-repaired.log`
then reports a large Response variant after adding operator DatabaseWork.
The response is boxed and its full payload remains credited.
`clippy-boxed.log` passes warning-denying all-target Clippy.

During test splitting, a text transformation briefly inserted `pub` into test
struct literals and rustfmt failed parsing. The visibility rewrite is corrected
to apply only to declarations. No test ran that malformed source. The split
retains all 65,536 mutations in a separately selectable integration target.

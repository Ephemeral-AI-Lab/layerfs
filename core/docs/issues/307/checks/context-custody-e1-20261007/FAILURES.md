# Retained original failures and source diagnoses

> Status: append-only command receipts; functional/tooling checks, no performance samples.

- `01-e1-tests`:39/40passed; the positive oracle executable path used macOS `/var` while the sealed artifact resolved `/private/var`. String equality rejected identical filesystem identity. The E1 owner is correcting path resolution and independently reviewed cohort/result-scope defects; no sample or gate was relaxed.
- `02-content-no-run`: compile failure; `filled_page` is the existing directory codec helper, not a directory module reexport. The context implementation now imports that same helper directly.
- `03-content-no-run`: all targets compiled, but the new context test had an unused `policy` import. Removed that test import before the final scoped check.
- `04-content-scoped-tests`:38existing bodies passed;11/13new context bodies passed; two negative short-child fixtures could not be encoded because their `a` maximum sorts after the numeric right-child maximum. Use `0000` and its exact encoded byte count to make a locally canonical branch that reaches the intended non-root-fill rejection. No product validation was weakened. `object_identity` was not reached.
- `05/06`: affected build and13context+11previously unreached identity bodies pass. The38unchanged earlier passed bodies retain their exact evidence.
- `07-sdk-no-run`: the history child module could not read private Sessions.authority. Runtime-internal `pub(super)` visibility lets the existing authorized root adapter serve that handler; no public authority API was added.
- `08-sdk-no-run`: the new custody test called a nonexistent no-argument Timing::disabled and scope method. It now uses the actual public disabled(name, closure) API and child scope.
- `09-sdk-no-run`: all targets compiled; shared test socket helper poll is unused in custody. That external fixture module has the same dead-code annotation used by other tests; product warnings remain denied.

Every original stdout/stderr and exit remains intact. No timeout has occurred in these commands.

- `10-sdk-tests`:2attachment+5custody+20/21runtime bodies passed. The nested-denial fixture omitted the required mtime key; Content correctly rejected `mtime missing` before demanding mode's denied value. Both portable keys now exist and reference the denied absent object, so the test reaches the required authority-before-Storage boundary. Root-binding/supervisor/wire targets were not reached.

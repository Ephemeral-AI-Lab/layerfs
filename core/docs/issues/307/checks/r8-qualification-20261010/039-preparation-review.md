# R8 preparation review and custody

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Inherited checkpoint `f8a0a5ff1` already contained the plan. Task-owned dirty
harness repairs and append-only preparation receipts were audited and carried
forward. The next-agent prompt and unrelated untracked material remain untouched.
No product implementation changed; no timing sample or R8 functional invocation
has run before the forthcoming registration.

The functional inventory has 90 unique rows. Exact current product/lock/fuser
objects match the broad execution at `7999a6356`; only executed assertions are
reused. The measured inventory preserves all 222 canonical IDs and adds separate
SC01-SC13 dispositions, 60 IDs, for broader scenario scope. [Validation038](038-inventory-validation/stdout.txt)
checks IDs, counts, workload hashes, source/receipt presence and zero attempts.

Harness repairs preserve historical operations and outcomes. Removed
host-mediated families fail closed before build/acquisition and emit explicit
mechanism-removed records; [020](020-retired-family-tests/result.json) passes
14 owning tests. E04 aligned borrowed writes derive (4096,1,0,0) from the product,
while old receipt defaults stay (4096,1,0,4096). [031](031-e04-contract-tests/result.json)
FAILED because the scoped Python invocation omitted the test-module import path;
its executed assertions and 12 import errors are retained. [032](032-e04-import-precondition-tests/result.json)
supplies that path and passes all 37 affected imported tests. No product or
assertion was changed to fix that invocation.

The content harness lock resolves only already pinned core registry versions;
[017](017-content-lock-parity.json) records exact parity. Historical 4096 batches
remain a workload parameter, not a current total namespace refusal. Its locked
[build021](021-content-harness-build/result.json) FAILED on five removed API
imports; no test ran and no private/root/removed-package substitute was supplied.
These historical families need an explicitly designed API port before execution.

The full comparator is independently authored over the closed native input seal.
Portable symlink mode0777 follows Project import; configured ownership and
ctime=mtime follow Fuse. The dated R7 update in architecture77 supersedes constant
2 directory links with 2+child directories. Comparator expectations use that
written update and input topology, never observed output. [034](034-full-oracle-tests/result.json)
FAILED on 13 external fixture-model assertions because APFS directory nlink was
not projected by the fake mount. [035](035-full-oracle-tests/result.json) passes
27 after correcting only the external model, keeping the wrong-nlink test.
[036](036-full-oracle-time-range-tests/result.json) passes29 after storing oracle
nanoseconds as decimal TEXT and adding large/negative timestamp assertions.
No test hook, total timestamp cap, weakened comparison or product edit was added.

The lead reviewed the comparator and tests and reran the inventory/count checks.
Worker review found two controller omissions: external raw oracle retention and
actual copied comparator hash comparison. Both are fixed before source freeze;
no full proof was executed with either omission. The controller exports known
oracle JSON/artifacts before terminal teardown and preserves original failure
plus any independent export/close failure. Every operation is attempted once.

[033](033-full-proof-copy-preparation/result.json) validates the retained host
sealed file and streams its 1417285632 bytes into a new independent named volume.
It takes17985751250ns as setup only, with natural cache warmth explicitly declared.
The original failed R7 preparation015 stays FAILED. No protected master/sample
container or volume was read, changed or deleted. New volume ownership is in
`/tmp/layerfs-r8-full-proof-inputs-20261010/owned-volume.json`.

The explicit wide preparation [014](014-wide-preparation/result.json) is closed.
Build-listed host/Linux proof binaries are retained in [011](011-host-proof-build/result.json)
and [013](013-linux-proof-build/result.json); the product source stayed identical.
Fresh release runtime and daemon builds [028](028-runtime-release-build/result.json)
and [030](030-daemon-release-build/result.json) pass. Linux checked all1171
host source hashes first. The focused fuser check [010](010-fuser-provenance/result.json)
still FAILED solely for the known passthrough manifest/lock override. Its rules
are unchanged and P execution remains PENDING OWNER.

Rust product fmt/Clippy/guard and broad suites are reused only from identical
product/config/dependency scopes, with their original missing-precondition
failures visible. Harness-only changes do not trigger unrelated product sweeps.
The content-harness build failure is an independent unverified scope.

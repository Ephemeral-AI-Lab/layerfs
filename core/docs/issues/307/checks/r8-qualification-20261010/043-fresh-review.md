# Fresh review of frozen R8 harness

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Fresh-context reviewer read only commit `63fe77e3d` using Git objects. No test,
Docker operation, proof, edit, count or Git mutation was performed by the reviewer.
It found no expected data derived from product output, metadata reconciliation,
hidden fallback or product hook in the comparator. That review is not execution.

Three controller findings were accepted before any prospective proof:

1. An EventProcess startup carrier can have no child. Finally dereferenced the
   missing process, hiding the first original error and omitting proof.json.
   The close path now accepts absent process streams and retains the original.
2. An expired comparator can leave partial rows/scratch without final JSON.
   Export previously failed first on the absent JSON and skipped those artifacts.
   Every known member now gets one independent original export attempt, partial
   outcomes are recorded, and the container is retained on failure.
3. Copied forensic rows were not checked against their original comparator hashes.
   Expected/observed JSONL SHA-256 now must match before successful terminal Stop.

[Controller tests042](042-controller-custody-tests/result.json) passed the three
export/integrity assertions, but FAILED the startup assertion: its independent
fixture omitted the required manifest field and reached a KeyError before spawn.
The fixture now supplies that declared field; [044](044-startup-custody-test/result.json)
executes the missing no-child custody assertion. No original copy/control operation is
replayed. Comparator and product binaries are unchanged; this is a new Python
controller identity before registration, not a resample. The earlier source and
all review/test failures remain in Git/append-only receipts. No finding was
rejected. The worker's constant-directory-link brief was corrected using the
explicit dated R7 architecture supersession before comparator implementation.

The original raw wide-preparation stdout014 contains a final blank line flagged
by git diff --check. It is retained byte-for-byte under the evidence policy.
Changed source and maintained documentation whitespace checks pass independently.
No raw receipt is rewritten merely to make that check pass.

# CDC deletion fixture offset collision

The required `dedup-cdc-delete-500`, seed 3 fast-verification attempt
`bf6d01cc7939` failed with `duplicate CDC reference/variant fixture` on source
`e0922904e2bb607a138157755dab9613b441d5b9`. The preceding canonical check
accepted all 501 regular files and 523,288,576 logical bytes. The failure arose
in independent fixture qualification before native verification. Preserve this
attempt as failed evidence.

The frozen framed SHA-256 offset formula maps deletion ordinals **74 and 471**
to the same offset, **359004**. Both variants delete 4096 bytes from the same
reference, so their complete byte sequences are identical. Distinct framed
ordinals do not guarantee distinct offsets after reduction modulo 917505.
The [CDC contract](../dedup-cdc-locality.md) explicitly requires distinct
variant digests; the verifier correctly rejected this fixture.

The necessary correction preserves that uniqueness gate. For the
`dedup_cdc_locality` deletion profile only, assign offsets in ordinal order.
Start each ordinal at its existing framed-hash candidate. The first ordinal
owns that offset. If it is already assigned, advance by one byte, wrapping
from 983040 to 65536, until an unused offset is found. Lower tiers retain the
corresponding prefix of the 500-variant schedule. No candidate CDC output or
observed performance influences this choice.

For the current three seeded schedules, this changes only seed 3, ordinal
471: **359004 → 359005**. Ordinal 74 retains 359004. Seeds 1 and 2 have no
collisions, and the first 100 ordinals of seed 3 are unchanged. All reference
bytes, other profile bytes, file names, file counts, logical lengths, seeds,
metadata, public initialization routes, resource bounds and deadlines remain
unchanged. The largest affected fixture still has 501 regular files and
523,288,576 logical bytes. Complete independent content and CDC-transcript
verification, including variant uniqueness, remains mandatory.

Only `dedup-cdc-delete-500`, seed 3 changes its input identity. Its previous
performance observation remains historical and cannot qualify the corrected
input; collect one replacement performance sample and retry its failed fast
verification. Other case/seed inputs and previously passing verification are
retained at their authentic source identities. Do not treat this correction
as permission to resample seeds or alter any other offsets after an outcome.

This finding records a specific Phase 1 fixture repair. It does not weaken
or replace the existing frozen CDC specification's uniqueness requirement.
Seal this correction before changing the generator and qualify the corrected
deterministic schedule before collecting its replacement evidence.

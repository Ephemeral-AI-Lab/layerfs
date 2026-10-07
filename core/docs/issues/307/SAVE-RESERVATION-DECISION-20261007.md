# Save reservation count and unbounded input

> **Status:** Owner approved “Allow counted block refills (Recommended)” in
> the active chat2026-10-07. Allocator implementation follows separately.
> Inspected at `e2f2e62c74d92b782bc4f87fba929cad2d98f589`.

The plan asks both (a) one fixed larger pack/ordinal block per Save and (b) no
file/Save/Commit limit derived from that block. Existing Save consumes an open-ended
stream and does not know its total pack/value count before constructing it.
`save/wave.rs::reserve_packs` refills as needed. `save/reservation.rs` and
`save/pooled.rs` separately reserve value ordinals. The physical identifiers are
finite i64 pack ids and u32 pooled ordinals. A fixed per-Save block can exhaust
before those format-wide limits; refusing then would add the prohibited Save cap.

## Recommended concrete revision

Use one combined initial reservation for a configured pack/ordinal block. Consume
it locally. If an unbounded operation exhausts a range, make a new explicit
reservation for the next block. It is a new allocation operation, never a retry
of a refused/unknown write. Each reservation still attempts BEGIN IMMEDIATE once;
Busy terminates that Save. Report exact actual reservation and publication counts.
Unused ordinal tails follow the existing exact finish rule; no GC, recycling of
consumed ids, or global producer ownership is added.

The revised count equation would be:

`write transactions = initial reservation + required refill reservations + publication batches + one stage-and-publish`

A Save fitting its initial blocks has the originally requested count. Large Saves
pay linear/amortized refill counts without a hidden total-size cap. This choice
preserves streaming Save and existing physical formats. It does not claim a
speed improvement or alter the accepted Init baseline; no timing is requested.

Alternative: require the caller to declare a proved upper bound on total Save
pack/value needs before beginning. Reserve that workload-derived range once.
This changes the Save input contract and requires a paid count/planning pass or
sound upper-bound arithmetic for every constructor, including unknown-length
streams; it is more scope than the current bounded-stream API.

The owner explicitly approved the revised equation above. It supersedes the
original exact-one-reservation wording. Implementation must still report each
refill, keep one-attempt Busy/unknown semantics, and prove boundary crossing
without an artificial Save cap. Atomic History proceeds independently.

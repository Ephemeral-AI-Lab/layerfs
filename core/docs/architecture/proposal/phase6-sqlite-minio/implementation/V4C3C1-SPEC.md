# V4c3c1 owner-bound paged inode reservations

> Status: Research; informative and not a product contract.

Parent50af45b896f4cdbf52c1519fa7dee722204174c5. Full integration goal remains open.
Freeze this slice before implementation: replace one bootstrap512serial grant with
C5-backed64serial pages, requested only after the current grant is consumed. The
current512population/256handle/changed/edit/verifier/provider limits remain. Larger
population admission and inherited import/mount are separate next slices; no silent
constant increase or claim of unbounded physical resources.

## Exact experimental wire and authority

Increment private metadata profile P6META5 -> P6META6; public Bridge tags unchanged.
Actions0..6 retain their roles, action7 reserves the next inode page. Maximum frame
16384bytes; action counters become fixed8slots. Initial action0 payload is owner:
u16-length workspace name (1..255ASCII alnum/_/-), incarnation32 (actual WorkspaceId
valid authority), project17, Branch17. Parse exact EOF before effects. Authority
validates project/Branch against actual current C5snapshot before claiming the sole
owner. Snapshot scope/profile become the immutable reservation context. Single
registered authenticated daemon peer and one live Workspace remain this profile.
No owner registry, anonymous allocations or cross-Workspace sharing.

Bootstrap response keeps MinIO fields + actual Snapshot, then reservation record:
owner selector above, scope32, profile32, sequenceu64=1, startu64, countu64=64,
end-exclusiveu64. Action7 request: owner, scope32, profile32, next sequenceu64,
previous exclusive-endu64. Response is the same complete reservation record. Integers
big endian; count exactly64; checked start/count/end within C5signed serial profile.
A next grant may have a gap if another legitimate C5allocator consumed serials;
it must not overlap its predecessor. No assumed contiguous ranges or range recycling.

Server retains only one owner, last sequence/end and pending/quarantine bit under
one mutex, established before actual reserve_inodes. Context/sequence/endpoint
mismatch refuses before allocation. C5range consumption is unconditional; uncertain
allocation or response never refunded or resent. Client retains one sequence/end
cursor and quarantines after any submitted/invalid response failure, including
malformed identity/cardinality/end. Known idle session rotation remains permissible,
with a new request ID and no replay of previous allocation.

Daemon Engine creates from the explicit first range and actual native provider.
When cursor==end, it makes one next-page request before the inode/name mutation
transaction; new cursor changes only after a complete authenticated range reply.
Guard accepted-source/installation state first. Invalid/refused range performs no
new inode/name mutation and leaves earlier writes readable. Production C5allocator
provides non-overlap/highwater; no namespace/history scan. Existing external Engine
constructor can retain its declared512local fixture grant for unaffected checks;
live daemon must use explicit page/provider wiring.

## Ownership and exit

Own dedicated reservation grammar/book/native provider, wire/session fixed grammar
and counters, metadata bootstrap/action7/publish owner checks, daemon creation,
Engine refill and external actualSQLite/C5/native framing checks. No canonical
object/MinIO/C2/provider/deadline/worker/durability change or new dependency.

Prove actual C5 successive ranges, allowed interleaved gap, wrong owner/context/
sequence/end refusal without highwater consumption, consumed-range no replay,
wire EOF/range identity, exhaustion without inode effects and real Engine refill
across64boundary. Owning locked host tests/Clippy/fmt/native and Linux build.
Then one frozen affected-source full128+sparse live cohort crossing actual native
reservation boundary, independent byte/history/cleanup and exact8slot calls. Do
not rerun unrelated67/270arms merely because tooling source seal changed. Runtime
checkpoint PARTIAL until live gate; physical resource/canonical/concurrency/import/
fullfamilies/DeepSeek remain open.

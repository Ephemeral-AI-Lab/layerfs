# #237: prospective C3 peak-memory phase diagnostic

> **Status: Research; informative and not a product contract.** Frozen
> before a separate count-driven diagnostic. Neither v0.1.6 nor the
> earlier C3 performance and memory rows are resampled as speed arms.

The retained exact-source comparison establishes a 62,357,504-B
promoted-C3 excess in `getrusage` process high-water RSS at public-call
return. Core's scan holds 101,001 entries and 100,000 jobs together,
but its namespace builder later holds the entries with serial, metadata,
content-root and inode vectors. The existing receipt does not identify
which interval first sets the process high-water.

Run one more **distinct mechanism diagnostic** at the same seed-1 SHAKE
manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`.
Use locked Cargo release, a fresh independent source copy and Store,
0/126,206 resident source payload pages before launch, the real SDK
`Client::init_project` call and a full separate reopened oracle. It
must retain all raw output and remain performance `INELIGIBLE` because
source metadata residency is unqualified. Do not repeat the old route.

Temporary reporting-only probes will record Darwin current RSS,
physical footprint and process-lifetime RSS high-water at these ordered
boundaries: import start, scan end, file loop end, file Save end,
namespace entry, prerequisites end, tree-input construction,
tree-build end, namespace Save end and History publication. They will
also record capacities and element sizes of the retained entry/job,
serial, metadata-root, content-root, inode and directory vectors,
plus Save-cache/index point readings where available. Markers are
point readings; the high-water is cumulative. A high-water increase
between two markers places the peak in that interval, but cannot
identify an individual allocation without a finer allocator ledger.
All source changes are temporary, hashed before the run and restored
after it. A diagnostic failure remains retained; no unchanged arm is
rerun for a better memory number.

The decision question is whether the peak is created in the file
import while both scan vectors are live, in namespace construction
while several vectors coexist, or in another Store/allocator window.
Recommend a product change only where the measured owner and the
source lifetime agree; distinguish an avoidable representation from
work needed by the current public Init contract. Keep #229 sparse
history admission separate.

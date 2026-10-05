# Canonical zero-run construction

> **Status:** Selected first-party algorithm for S5/P4; not release evidence.

The external frozen-scanner diagnostic establishes a 32,768-byte all-zero chunk
period. Preserve the scanner/profile and every canonical codec. Add a public
run source supplying bounded data windows, explicit zero lengths, and EOF. Its
whole-file probe still pays at most the selected cutoff. Chunked construction
uses the existing scanner and ExtentBuilder. A zero run scans at most one full
chunk to clear preceding data and one tail; interior complete periods reuse one
canonical zero chunk. Skipping an even full period preserves the scanner's
zero-only buffer, pending byte, hash and boundary-search position exactly.

ExtentBuilder retains its unfinished lookahead at every level. First flush one
possibly mixed boundary page using the ordinary path. For remaining uniform
entries, compute the number of full pages that ordinary streaming would emit,
emit their identical canonical node once, recursively append that summary as a
repeated run to the next level, and retain the same unfinished suffix. Finish
continues to use the existing canonical partition and root rules. Cumulative
branch summaries restart within each node, making repeated subtrees identical.
Child objects are accepted before referring parents; consumer failure stops the
single operation without replay. Repeated references count logical extents but
do not multiply object acceptance. No new hole object or canonical format exists.

For page capacity C, unfinished capacity U and height H, one zero run takes
O(chunk size + U H) byte/entry work and O(H) object emissions. H is logarithmic
in the number of zero chunks and bounded by the existing canonical grammar.
Ordinary data costs its actual bytes. Across R runs, cost is actual data plus
O(R (chunk size + U H)); the producer uses bounded run windows, with no total
run/file-size limit. Resident state is the threshold probe, scanner/input buffer,
unfinished boundary pages, and O(H) recursion. Downstream consumer custody and
physical storage are separate. Root equality against streamed zeros, mixed
prefix/suffix, odd data-window splits and canonical partition boundaries is the
acceptance proof, plus large-run logical reads and object-count diagnostics.

P3 is explicitly carried to S10: EDIT_DEFERRED_LIMIT guards the current resident
draft/parent-reference/detached/committed maps in localized editing, rather than
S5 overlay writes or this fresh run constructor. Removing its check alone would
replace a refusal with unbounded resident state. S10 must supply an owning backed
draft/reference/release interface and bounded cursors, including synthetic draft
identity exhaustion. Sparse edit replacement must then use the run constructor
through the ordinary edit path. This does not resolve P3 or qualify integrated
Commit; the S5 handoff permits this explicit dependency disposition.

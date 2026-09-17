# W8 - the campaign packet, work done while E1 is open

W8 is the one packet that needs an owner yes/no (E1: may the matched v0.1.6-versus-
candidate C1 campaign run more than one sample per case per arm). The escalation is
recorded in the closeout report's §1; nothing in this directory pretends to answer
it. The packet's parts that do **not** depend on the answer were completed here.

## W8.4 - the registry is now RUN or NOT_RUN

`stages-3-4-verification.md` §2.1 maps every case the frozen contract declares to
the product case that executes it, or to `NOT_RUN` with its reason. Eleven rows run;
two do not:

* the matched campaign itself - blocked by E1 (no addendum was committed, so no
  matched arm may exist);
* read amplification on the representation transition - no case exists; it is the
  residual gap the acceptance report names.

The `store-bytes` row the contract was missing is **new and real**. One sample of a
deterministic 1 048 583-byte chunked fixture, saved through a real `Store`, measured
on the closed database:

```text
MEASURED store-bytes: raw=1048583 canonical=1052271 objects=60 inserted=60 packs=6 \
  pack_bodies=1052818 largest_pack=259777 database=1204224 files=["store_bytes.sqlite"]
```

Read as: 60 canonical objects (1 052 271 B) framed into six pack bodies of
1 052 818 B in total, the largest 259 777 B (inside the ordinary 262 144-byte pack
limit), all of it inside rows of the single 1 204 224-byte database file, with no
pack, payload or spool file on disk. Pack bodies are reported separately from the
database that contains them, and no database size delta is claimed as write I/O. The
case asserts the containment, the largest-pack bound, the inserted-object count, the
file list and a byte-exact logical readback before it reports the row.

Command (`w8-verify.log`):
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test memory_bounds the_retained_footprint_reports_pack_bodies_and_database_bytes -- --nocapture`

## W8.7 - the clipping is disclosed wherever it was quoted

The retained `e1c-pooled-512` receipt is telemetry-clipped (`pooled.save 3.105s
[incomplete]`, `"incomplete": true` on the root and on one `storage.accept`, 57.5 %
of that scope unattributed). The receipt, its timing tree and its `stdout.log` are
**untouched**: no receipt is re-labelled. Disclosure was added to every quotation:

* `evidence/stages-3-4-timing-20260917T031000Z/ledger.md` - an appended section
  dated 2026-09-17 (the ledger is append-only, so the row above it is not rewritten);
* `stages-3-4-verification.md` §4 - a dated disclosure paragraph beside the round
  table, stating that only the wall time is used, as budget accounting;
* `stages-3-4-final-review.md` - a dated correction above the table that quotes the
  arm, naming the clipped scope and which parts of the row are complete.

Re-collection was not attempted. The arm is a wiring and correctness demonstration
whose receipt is cited for budget accounting and for structural counts that are
complete in its summary lines; a single-sample re-run would produce another n=1
number and could not change what the arm may be claimed for. That is stated in the
ledger annotation as well.

## What is still blocked

G13 (matched campaign under a pre-committed addendum), G15 ("existing-or-better"
resolved or waived) and, through them, the Stage 3 "qualified" verdict. On a yes
from the owner the order is: commit the versioned addendum (cases, sizes, seeds,
identities, cache state, sample count, allowed claim), align the byte-accounting
boundary and de-duplicate the replacement fixture (W8.3), collect the arms, verify
separately with the same identities (W8.8), then resolve the gates.

## Evidence

* `w8-verify.log` - six recorded commands, all exit 0: the content registry targets
  (`edit_noop`, `edit_transitions`, `edit_batch`, `edit_single`), the storage ones
  (`delta_payload`, `edit_pipeline`, `policy_capacity`, `memory_bounds`) with
  `--nocapture` so the raw `MEASURED store-bytes` line is on disk, then the product
  rustfmt check, clippy with `-D warnings`, the workspace suite (43 targets / 273
  tests, 97.5 s, a declared verification run) and the product-boundary check. The
  runs happened on the working tree that the W8 commit contains; the header records
  the parent commit and the commit's own tree is what was verified.
* Production LOC is unchanged by this packet: `11058 -> 11058` (delta 0) - one new
  external test case and documentation only.
* `stages-3-4-verification.md` §2.1, the ledger annotation, the §4 disclosure and
  the review correction named above.

## Note added 2026-09-17 after the owner's waiver

The section above describes the state while E1 was open. The owner then passed G13
and G15 by written waiver, unmeasured, no flaw being open in the recorded audits, and
deferred the qualification to Stage 6 (#171). Nothing in this directory changes: the
registry, the store-bytes row and the clipping disclosure stand as collected, and the
waiver claims no campaign, no comparison and no speed.

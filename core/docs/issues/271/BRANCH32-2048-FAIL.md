# Issue 271: 2,048-write count diagnostic and handoff

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective selection](../../../../docs/roadmap/0.1/0.1.7/issue271-2048-diagnostic-spec.md)
was committed before adding `diagnostic2048` to the existing public runner.
At source `4ceb5d7ee79109c38fe9bf0134fb0c93cffabe6a`, one attempt reused
the closed master, locked release driver/verifier/writer and product daemon
from the same 32-child source as the retained 4,097 gate FAIL. The 2,048
selection kept the ordinary **15 s** complete-command limit and 512-WRITE
diagnostic snapshots. Cache status remained `INELIGIBLE`.

The host command timed out at **15.005228 s**, so its row is **FAIL**: no
driver receipt, public callback count, Commit, independent verifier or clean
product teardown was returned. The owned daemon's [raw log](evidence/branch32-2048-fail-v1/orphan-final.log)
independently recorded all 2,048 extent splices and its `WorkspaceExec`
success at **14.784311 s**, but that late result cannot convert the host row
to a PASS. The four declared checkpoints are intact:

| Daemon WRITE checkpoint | Ledger 4 KiB reads / writes | Child edges added / removed | Local custody edges added / removed | Branch writes | Root height |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 512 | 20,953 / 12,150 | 3,896 / 3,864 | 23,139 / 22,589 | 450 | 1 |
| 1,024 | 50,947 / 32,937 | 16,144 / 16,080 | 47,303 / 46,233 | 962 | 1 |
| 1,536 | 88,978 / 58,783 | 29,259 / 29,176 | 71,531 / 69,941 | 1,973 | 2 |
| 2,048 | 128,311 / 85,837 | 42,902 / 42,800 | 95,792 / 93,713 | 2,998 | 2 |

The last splice at 100/512/1,024 writes named **3/16/32 children** in its
new root branch. The 1,536 and 2,048 splices each wrote two branches naming
33 and 34 children in total. Earlier #271 prose described ideal minimum leaf
counts of two/nine for the 100/512 trees. Actual balanced touched-leaf packing
leaves partial pages, so those are lower bounds, not the observed tree shape.
The 32-child treatment still leaves 100/512 branch packing unaffected, but
its split occurs earlier than the idealized threshold in the prospective
2,048 specification. The within-run ledger counts continue to grow steeply
after the split; this is a count observation, not a measured time exponent.

After the host timeout, the owned daemon stayed alive and logged `sandbox
shutdown retained: Busy` on deliberate stop. The container exited 137. Its
container and named volume were then removed, with no new owned resource left;
that forced cleanup is **not** a product cleanup PASS. The [append-only receipt,
raw host/daemon logs, redacted identity and cleanup record](evidence/branch32-2048-fail-v1/)
preserve this failure. Neither the 2,048 selection nor the 4,097 gate was
retried at unchanged identity.

Further #271 work belongs in a separate session and source identity. A
read-only audit pointed to the copied 124-record leaf's Local custody edges as
the dominant all-count term; `change_refs_run` already batches adjacent refs
per ledger page. A moderate leaf target with the existing branch bound is an
**unproven candidate**. The rejected 32-record/eight-child treatment proves
that lowering edge counts alone can increase ledger I/O by creating more
pages. Any next change needs prospective 100/512 count receipts, old-root/G1/G2
custody proof and a meaningful one-shot 4,097 gate at its own final identity.

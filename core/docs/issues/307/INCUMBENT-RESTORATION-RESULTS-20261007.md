# Restored incumbent: Init and history stride results

> **Status:** Dated measurement checkpoint; not release evidence or a product contract.

Owner direction 2026-10-07: restore the best 9.75 s version, then run namespace Init and history stride 10, 3 and 1 for both Durable and Disposable. [Plan and selections](INCUMBENT-RESTORATION-PLAN-20261007.md), [ledger](checks/incumbent-restoration-20261007/ledger.json), [compact receipts](checks/incumbent-restoration-20261007/). Every row is one sample; nothing here is a median or repeatability claim.

## Identity and custody

| Field | Receipt value |
| --- | --- |
| Product | Restored at `2fced797d`; `core/crates` byte-identical to `4a207cea1`; product seal `830593d5d4cd4f79…` in every candidate receipt |
| Init source / tree | `2fced797d14f9d4f6b72c9ad00574976a4dfd5f0` / `9bb5da6f863e…`; driver binary SHA-256 `a608d02a44af854d…`, the same executable as the retained work-reduction sample |
| History source / tree | `6af972dc0a9e97111bafe97aa0627c6811f62686` / `840feca35d6e…` (documents and campaign scripts only after `2fced797d`); reference `7edddbdb8`; `benchmark_history` `6f709717c5d197d0…`, `history_reference` `13f0727988730046…` |
| Harness seal | `244b3a17e03634b6…` at both commits |
| Profiles and layout | Init: Monolithic with acquisition tables, four constructors. History: GroupRowsIndexed, one construction worker. Durable WAL/FULL/fullfsync; Disposable MEMORY/OFF |
| Fixtures | Init: identity-checked prepared namespace masters, fresh Store. History: original pinned corpus `deepseek-history-data`, manifest `03f21acfb415907f…`, no prepared Store |
| Host and cache | macOS arm64 host; in-run native source attestation `resident_after=0` in every sampled arm; history also attests the database at every state boundary. Docker Desktop and the desktop Codex app were running throughout (11 listed processes per receipt) |
| Commands | `checks/incumbent-restoration-20261007/measure.py <worktree> <case> <arm> <limit>`; history arms preceded by `precondition.py`; fresh output per arm; release/locked sealed binaries reused by seal |
| Verifier | Init: `verify_namespace`, every path and kind plus the declared file sample, 19 s bound. History: `phase7_history_proof.py`, all-state structure with bounded representative content, family bounds below |

## Namespace Init, `-incumbent-restored-v1` at `2fced797d`

Speed gate `10 × candidate ≤ 11 × control`; allocation gate final database+WAL+SHM ≤ control. Control is the reused cluster-one-end row at `197d2fb7d`. All eight complete, proof, cold-source and cleanup checks PASS; **all eight speed gates and all eight allocation gates FAIL.**

| Profile / files | Control ns | Restored ns | Difference | Speed | Control B | Restored B | Allocation | Complete command ns / 30 s | Proof ns / 19 s | Earlier same-binary sample ns |
| --- | ---: | ---: | ---: | --- | ---: | ---: | --- | ---: | ---: | ---: |
| durable / 100 | 79759708 | 93168667 | +16.812% | FAIL | 5255168 | 5292032 | FAIL | 127826042 | 21656667 | 107616541 |
| durable / 1000 | 201566000 | 232671917 | +15.432% | FAIL | 20545536 | 20611072 | FAIL | 323488291 | 28566875 | 222706583 |
| durable / 10000 | 2492429625 | 2885705750 | +15.779% | FAIL | 305070080 | 305516544 | FAIL | 3688801625 | 327888417 | 2832280500 |
| durable / 100000 | 7724523333 | 10692601500 | +38.424% | FAIL | 514965504 | 515624960 | FAIL | 18835135541 | 738876750 | 9749380917 |
| disposable / 100 | 38747750 | 45688500 | +17.913% | FAIL | 5222400 | 5255168 | FAIL | 72769625 | 15084416 | 43854417 |
| disposable / 1000 | 129258375 | 155291459 | +20.140% | FAIL | 20537344 | 20590592 | FAIL | 251292875 | 31467708 | 157947084 |
| disposable / 10000 | 1645276292 | 1943650833 | +18.135% | FAIL | 305074176 | 305471488 | FAIL | 2746595083 | 346910875 | 1977715500 |
| disposable / 100000 | 5558569958 | 7423399375 | +33.549% | FAIL | 514940928 | 515579904 | FAIL | 15509010958 | 796405458 | 7301489875 |

The last column is the retained work-reduction sample of the byte-identical executable; it is context, not a second arm, and no lower value is selected. Durable100000 reads 9,749,380,917 ns there and 10,692,601,500 ns here, a 9.67% difference between two windows of one binary. The streaming V3 sample (10,132,066,834 ns) lies between them, so these single samples do not establish that V3 was slower than the incumbent; V1 (12,798,146,625 ns) lies outside. No sample of any source reaches the 8,496,975,666 ns speed limit.

## History stride 10 / 3 / 1, second selection at `6af972dc0`

Speed gate `10 × candidate ≤ 11 × reference`, reference arm sampled first at the same harness identity. Allocation gate is the approved ceiling. **All six pairs PASS speed and allocation**; all twelve arms pass proof, source and database cold attestation and cleanup, and reference and candidate roots are equal in every pair.

| Profile | Stride / states | Reference ns | Candidate ns | Difference | Speed | Candidate allocated B / ceiling | Allocation | Complete command ns (reference; candidate) / bound | Proof ns (reference; candidate) / bound |
| --- | --- | ---: | ---: | ---: | --- | --- | --- | --- | --- |
| durable | 10 / 17 | 33519000375 | 33463455583 | -0.166% | PASS | 50724864 / 54278964 | PASS | 47592692000; 47694741292 / 120 s | 3026737708; 3926516416 / 24 s |
| durable | 3 / 53 | 72226545709 | 69832602917 | -3.314% | PASS | 64278528 / 70427034 | PASS | 87700822292; 84694631708 / 340 s | 6942079583; 7320803209 / 24 s |
| durable | 1 / 157 | 190526544250 | 193992185709 | +1.819% | PASS | 85204992 / 92342273 | PASS | 205312168958; 207216476542 / 600 s | 17581886167; 17488460958 / 60 s |
| disposable | 10 / 17 | 34544042250 | 33708739000 | -2.418% | PASS | 50692096 / 54278964 | PASS | 47866607500; 46874867875 / 60 s | 3165924833; 3261549375 / 12 s |
| disposable | 3 / 53 | 72230709750 | 66838917708 | -7.465% | PASS | 64245760 / 70427034 | PASS | 87422624084; 79753570625 / 170 s | 6881713166; 6622459666 / 12 s |
| disposable | 1 / 157 | 192712692209 | 185189368250 | -3.904% | PASS | 85172224 / 92342273 | PASS | 207408965750; 198557270125 / 300 s | 18009315375; 17628540791 / 30 s |

The internal operation value is each driver's complete `operation_ns`; SDK and daemon time are N/A for this component family. Storage is the candidate's single combined Store; the reference's separate Store and history total 52,473,856 / 65,142,784 / 86,179,840 B. The command and proof bounds are the existing owner-approved family limits, not general defaults or test timeouts.

These single samples show no history regression at the restored source. The history driver calls content construction, Save and the history catalog directly on a Store created without acquisition tables, so Init acquisition and its bounded reclamation are not on this path; the shared Storage/Persistence changes made since product freeze `ba6499a61` are on it and produced no slowdown here. The candidate ranges from 7.465% faster to 1.819% slower than its matched reference. The handbook's earlier history rows keep their own identity and are not relabeled.

## Non-passing, ineligible and reused rows

| Item | Value |
| --- | --- |
| Init, all eight cases | Speed FAIL and allocation FAIL, as tabulated; cause is the acquisition row lifecycle and incremental-vacuum pointer-map overhead recorded in [space and scaling](SPACE-AND-SCALING-RESULTS-20261006.md) |
| Durable stride 10 and stride 3 candidates, first selection at `2fced797d` | **INELIGIBLE, zero samples**: in-run source attestation left 1,466 and 1,345 pages resident. Their reference arms completed at 36,579,340,833 ns and 74,818,878,083 ns and are retained. Not retried at that identity; the second selection is a new identity |
| Cause of the ineligible arms | The cold helper opens resident corpus files writable; Spotlight then re-read them during the residency pass. The owner excluded the corpus from Spotlight before the second selection |
| Preconditioning | 21 recorded helper passes before the twelve second-selection arms, 8 of which found resident pages and repeated; each arm then passed its own in-run attestation with 4 resident pages before invalidation and 0 after. [Log](checks/incumbent-restoration-20261007/precondition.jsonl) |
| Order | Second selection ran Durable 10, then Disposable 10 / 3 / 1, then Durable 3 / 1, at the owner's request to report Disposable first; the Durable stride 10 pair was already in flight and was allowed to finish |
| Harness test | A wide `test_phase7_*` selection is retained as FAIL at the restoration commit: two unrelated tests need a release verifier and scratch directory absent from the primary checkout. The owning `test_phase7_sqlite.py` passes |
| Unavailable | Phase-only RSS, device-byte and VFS attribution; spread or repeatability for any row |
| Production LOC | `2fced797d`: 160755 → 160473 (−282), Core 95338 → 95056, reference 65417 unchanged. `6af972dc0` and this record: 160473 → 160473 (0). Same counter `tools/production_loc.py` and scope |

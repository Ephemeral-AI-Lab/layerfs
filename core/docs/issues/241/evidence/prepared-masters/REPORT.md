# #241 closed v3 prepared masters

Four diagnostic 1/10/100/capped-500 MiB pristine fixtures were acquired
once, outside any edit timer, using locked release benchmark_init through
edit_route.master at source 576dd181f. The exact input sizes, fixture digests,
canonical file roots and extent counts are in the individual
[master receipts](master-1048576.json). The [seal record](SEALS.json) lists
all four Store and history SHA-256 values, their local source paths, the
benchmark_init binary hash and the first-use preparation walls. [SHA256SUMS](SHA256SUMS)
seals the compact receipts.

**Qualification limit found after sealing:** the original preparation code
created each final cache directory in place and the master manifest did not
contain Store/history hashes. The SHA-256 values here prove the bytes observed
when this report was sealed, but they cannot establish atomic publication or
the bytes at the instant Init finished. These four directories remain
diagnostic input and must not be promoted into a registered performance or
Phase 3 PASS. The v3 preparation path is being changed to publish complete,
content-sealed entries atomically under a new key. These receipts and
directories remain unchanged. A later functional sweep and each performance
selection must use an independent writable byte copy of a qualified closed
master, then fresh Branch/Workspace state. Clone reuse is setup reuse and
does not establish a cold-cache claim.

| Pristine bytes | Store SHA-256 | History SHA-256 |
| ---: | --- | --- |
| 1,048,576 | 9465deba5661e03d34824b3e5812bb2ade6b3299bff69a93c69ad4da7193534e | 9b68554f457c202b8c100d4c40f63c4b64cc2349fa47677b3e408afd72298b5e |
| 10,485,760 | 11f963a475884626634c0e1f542cf8b8712384ee91971f2396313ae3b277bd72 | a81c55c19276a5af35353e99a612127496ccfcb5d28582b44d8a850f6846111d |
| 104,857,600 | d52fd630c4ca64142b488a863254c39e683a97e7bdd570e494d790a9ec5b125e | 66195665839c3b6b3a34e8b2f42abcfebbea057f6a2ff24e0d5238cb3063458e |
| 524,283,904 | 63204bba085ce60ab14299b5ddb4da23dc12b7d0cd43004a9bbfa184e44deaa7 | 92dbaeb014d5049e83ada470888e609db352f45e725e71522d830189a3c6827c |

The preparation command was the public SDK benchmark_init release example
via edit_route.master, once per size. The Store file sizes were 2.69, 13,
112 and 562 MB respectively. No edit, Commit benchmark, position check or
verifier ran during this preparation.

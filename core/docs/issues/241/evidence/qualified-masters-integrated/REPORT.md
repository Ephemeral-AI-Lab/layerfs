# #241 integrated-source qualified masters

These are the four closed pristine Stores for the integrated position-sweep
source. The SDK test-only dependency/lockfile change in fdb9c632f changed the
locked release benchmark_init executable from SHA-256
f3fa5771038fe40495d9c3de7b777ea652c8fd8b50efba5172489a9dcb7d7aae
to 70b69321fc8d95ae53f281d39b956eb30ece52da24684b09e1bc83f1dcb65eb2.
The preparation compatibility key pins that executable, so the earlier
[qualified set](../qualified-masters/REPORT.md) remains intact and cannot be
relabelled as this producer. The fixture source bytes were reused and
validated; one new release SDK Init ran per size outside any edit timer.

The v3 path published each complete three-file master by same-filesystem
atomic rename after per-key locking and creation-time Store/history SHA-256
validation. [SEALS.json](SEALS.json) records the exact local paths, producer
commit/tree/binary, compatibility keys and hashes; the four master.json files
are copied here and [SHA256SUMS](SHA256SUMS) checks the compact receipts.
Private Store/history bytes remain in ignored benchmark-results/prepared
directories. Each later functional or registered case must validate the
closed master once on acquisition and verify its independent writable byte
copy against these hashes. This is setup reuse, with no cold-cache claim.

| Pristine bytes | Master receipt SHA-256 | Store SHA-256 |
| ---: | --- | --- |
| 1,048,576 | e2fc6f92a031a06455ffa9752c6e6533f9bc5343046c76f35ff5cecb9be1d589 | 59e1ff00e0449fea9d15fdc4ef5b446c4be062a6cf3b1d7c3d4f206062698198 |
| 10,485,760 | 2b314a9bcb83c50dd3cf0416d2a4305cb589bdcb1b7773f83c9c0e4469d0e76f | c639d3286c67c75d1f6df4615941ee44b882d404244876693cf19341a928481c |
| 104,857,600 | aa94f2a94c956719c290fce603234f758e737282e0131058a5307ac197a38d30 | 4e0bede0612910b006e34daaec054d1980972492f35c28cefd72156bb01223a9 |
| 524,283,904 | a51d7d36308da3129103ff085d3c2062866a99582acf21528af0576c2e61772f | 5315c04f4fd9fcf12b73423409dbed8d62675ce784c7addd81f386cf97a0dd3a |

No mounted sweep case or Edit→Commit performance sample ran during this
preparation. The first four qualified receipts remain historical evidence
for their own executable identity and are not pooled with this set.

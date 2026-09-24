# #241 qualified closed v3 masters

These four pristine masters were prepared once under the v3 qualification
path at source eb17431fc, after the original in-place masters were marked
diagnostic. Preparation reused the already validated fixture source bytes,
but performed a new locked release SDK Init for each size. The new path holds
a per-key preparation lock, builds in a same-filesystem temporary directory,
requires exactly three regular files, hashes the closed Store/history and
manifest, and atomically renames the complete directory into its final
master-v3 namespace. No old master directory or receipt was rewritten.

The [seal record](SEALS.json) pins each local directory, source commit/tree,
release benchmark_init binary, compatibility key, Store/history SHA-256 and
first-use wall. The four [master manifests](master-v3-1048576.json) carry
their public SDK Init receipts, fixture SHA/root/count and creation-time
Store/history hashes. [SHA256SUMS](SHA256SUMS) seals the compact tracked
records. The actual closed database files remain in ignored
benchmark-results/prepared paths. A later acquisition rehashes them and
fails closed on a mismatch, unexpected sidecar or incomplete prior attempt.

| Pristine bytes | Store SHA-256 | History SHA-256 |
| ---: | --- | --- |
| 1,048,576 | 34b21cc3b51d066451e97e271db2aea9c4cf1f5d9c1b8468a5c8b256ffdd1d65 | 9c796ad09bb6a79e4f78b82372927941c76ad0839c7366dfea6b89436050104b |
| 10,485,760 | 9881bdd7a9cdf773a33ae883ee01ab5b6cf550688d6dd5786441143f0a1ddbeb | 735f2647c842d2cbb5caac0bfa081bd2381671e77ad1ec305d434ce9b5a3c739 |
| 104,857,600 | cd08e7adcfed8f6268e4fe53f53745f2aea8ef7b06dac57f600191c5aecd2576 | 81a3fe92e7f1e87cb5d4ae97b616dde36b4866e7dcc42c3307a7e8d1be9277b0 |
| 524,283,904 | 5ab17146b73f347e5397f6b4f4b301f19d2bfb50d375f2408d66374a4cb6bb5c | e6e21a23238e60d3f4a1b8ae2706cfbc2adfd522409161fe172f40f85dd7f8b1 |

Each functional or performance case must take an independent writable byte
copy and verify its hashes against this qualified source before mounting.
The qualified master is setup reuse only. No edit, Commit performance sample,
position sweep or cold-cache admission was run during preparation.

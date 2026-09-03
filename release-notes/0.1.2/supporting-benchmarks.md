# v0.1.2 supporting benchmark refresh

> **Status:** Completed release-source supporting-family measurements and separate verification.

Measured source: `e978edd19f189d56ca8678bae4dcdc7b6cd4f409`. These are fresh candidate observations, not a paired product-speedup claim. Namespace initialization and Store construction use their frozen historical lifecycle controls; they do not replace or claim the semantics of the three SDK-only edit families.

Environment: native macOS SDK/Store, Docker Desktop managed Linux container, real FUSE, no host bind mount. Every sample initializes its own Store. Input files may be reused, but initialized output Stores and measured results are not. Performance was collected before final full verification. OS caches are not flushed.

All elapsed-time cells below are **median (minimum–maximum), in milliseconds**. Namespace first-sample and subsequent-sample cache cohorts are separated. MB is decimal; MiB is binary.

## Namespace initialization

| Files | Logical MB | Cache cohort | N | Initialization ms | MB/s at median init | Create ms | Commit/visibility ms | Lifecycle ms | Process lifetime RSS MiB |
| ---: | ---: | --- | ---: | --- | ---: | --- | --- | --- | --- |
| 100 | 125 | first | 1 | 222.313 (222.313–222.313) | 562.3 | 19.601 (19.601–19.601) | 3.619 (3.619–3.619) | 33.003 (33.003–33.003) | 57.344 (57.344–57.344) |
| 100 | 125 | subsequent | 2 | 212.229 (211.197–213.261) | 589.0 | 9.841 (9.231–10.451) | 2.661 (2.603–2.719) | 20.576 (19.787–21.365) | 57.398 (57.062–57.734) |
| 1,000 | 200 | first | 1 | 258.801 (258.801–258.801) | 772.8 | 9.443 (9.443–9.443) | 3.299 (3.299–3.299) | 21.512 (21.512–21.512) | 62.953 (62.953–62.953) |
| 1,000 | 200 | subsequent | 2 | 258.267 (256.892–259.643) | 774.4 | 8.839 (8.087–9.591) | 2.990 (2.731–3.248) | 19.712 (19.015–20.409) | 63.203 (62.828–63.578) |
| 10,000 | 300 | first | 1 | 409.638 (409.638–409.638) | 732.4 | 9.977 (9.977–9.977) | 3.391 (3.391–3.391) | 21.115 (21.115–21.115) | 69.734 (69.734–69.734) |
| 10,000 | 300 | subsequent | 2 | 407.023 (404.119–409.928) | 737.1 | 14.545 (13.894–15.195) | 3.200 (3.100–3.300) | 27.386 (27.336–27.437) | 69.359 (69.188–69.531) |
| 100,000 | 500 | first | 1 | 3248.966 (3248.966–3248.966) | 153.9 | 15.361 (15.361–15.361) | 3.969 (3.969–3.969) | 28.874 (28.874–28.874) | 102.406 (102.406–102.406) |
| 100,000 | 500 | subsequent | 2 | 3011.238 (2990.033–3032.444) | 166.0 | 15.791 (14.022–17.561) | 3.756 (3.589–3.922) | 30.310 (28.299–32.321) | 105.758 (101.984–109.531) |

Initialization throughput is logical bytes divided by median initialization wall. Lifecycle excludes initialization and includes Create, historical execution, Commit/visibility acknowledgement and End. This Commit boundary is not identical to the SDK-only edit report's Commit-return boundary. Process lifetime RSS includes initialization; it is not edit-only incremental memory.

## Durable Store footprint

| Control | N | Logical MB | Durable MiB | Canonical MiB | Initialization ms | Commit ms | Reopen ms | Complete ms | Process lifetime RSS MiB |
| --- | ---: | ---: | --- | --- | --- | --- | --- | --- | --- |
| `store-footprint-large-object-500m` | 3 | 500 | 568.750 (568.750–568.750) | 478.411 (478.411–478.411) | 994.042 (980.503–1290.542) | 3.583 (3.425–3.735) | 140.299 (130.704–146.290) | 1642.539 (1321.891–1859.585) | 59.031 (58.578–59.297) |
| `store-footprint-metadata-cardinality-100000` | 3 | 500 | 699.312 (697.688–699.938) | 552.755 (552.755–552.755) | 5257.655 (5123.122–5528.791) | 4.643 (4.315–4.667) | 435.266 (415.678–460.331) | 6196.410 (5940.794–6489.065) | 119.516 (118.594–121.000) |
| `store-footprint-unique-100000` | 3 | 500 | 631.250 (631.188–632.688) | 517.759 (517.759–517.759) | 3876.445 (3754.052–3952.762) | 4.142 (4.008–5.718) | 358.525 (303.868–453.495) | 4479.297 (4391.616–4652.061) | 103.750 (102.609–104.453) |

All persistent Store files are included in durable bytes. SQLite dbstat, census and digest work are outside product timers, but add external harness wall time. The original 600 MB primary footprint target remains a documented limitation, not an achieved target. Far-future storage alternatives in #18 are unscheduled and are not part of this release.

## Separate verification

| Family | Proofs | Verification ms, median (min–max) | Result |
| --- | ---: | --- | --- |
| Namespace | 4 | 3244.681 (960.862–54829.207) | pass |
| Store footprint | 3 | 45101.734 (4051.590–61407.188) | pass |

Verification times aggregate different controls only to show verifier cost, not product performance. Per-control raw records remain authoritative.

## Raw evidence

- `benchmark-results/fs-bench-pro/init_namespace/release-v012-e978edd1-performance`; manifest SHA-256 `1ca7f8d6ff50662c0e1441e4596d96a6e69bf685a24fa7b8b4f66a955a75b75e`.
- `benchmark-results/fs-bench-pro/init_namespace/release-v012-e978edd1-verification`; manifest SHA-256 `94e584f5216cd5ce77b790428c846006a24907c65290dbb70b06bf4b05022a45`.
- `benchmark-results/fs-bench-pro/store-footprint/release-v012-e978edd1-performance`; manifest SHA-256 `9884fc04468321c9ed321901a1a25a8e30b6e90de01efca37e59204dc5018eaa`.
- `benchmark-results/fs-bench-pro/store-footprint/release-v012-e978edd1-verification`; manifest SHA-256 `d105a86c0df24241170d25f4cec07f251326693a721883685f3d7a4176139a1e`.

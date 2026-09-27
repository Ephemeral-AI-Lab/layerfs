# Issue 271: extended 4,097-write count diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective diagnostic specification](../../../../docs/roadmap/0.1/0.1.7/issue271-4097-extended-count-diagnostic-spec.md)
was committed before the runner change and this one attempt. The four-hop
product binary and sealed master were reused under product seal
`7e1e36eed0920a717284c277e61ed983d12d04a3669450300e3402bdb2f144fa`;
the diagnostic's source commit is `61eed04a979341353054781a551dd973fcb04d63`.
The [raw receipt, logs, full verifier, SQLite clones and hashes](evidence/fourhop-4097-extended-v1/)
retain the fresh `diagnostic4097` run. The original [25 s 4,097 gate
FAIL](FOURHOP-GATE-FAIL.md) remains unchanged. The user-requested 60 s limit
applies **only to this count diagnostic**. Ordinary host/container cache was
uncontrolled, so every raw latency is **INELIGIBLE** for a speed PASS.

| Public selection | WRITEs observed | SDK Exec | Commit | Complete command | Full oracle | Cleanup | Status |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| four-hop separated 100 | 100 | 0.412395 s | 0.031369 s | 1.333013 s | PASS | PASS | INELIGIBLE |
| four-hop separated 512 | 512 | 2.366790 s | 0.074215 s | 3.722882 s | PASS | PASS | INELIGIBLE |
| four-hop 4,097 extended diagnostic | 4,097 | 27.918267 s | 0.434905 s | 32.421651 s | PASS | PASS | INELIGIBLE |
| original 4,097 gate, retained | unavailable | unavailable | unavailable | **25.006973 s timeout** | NOT_RUN | FAIL | **FAIL** |

The extended diagnostic used the same generic one-process/one-fd writer,
locked release SDK/verifier, one construction worker, closed verified master,
independent writable byte-copy clone, Mount → Exec → explicit Commit, and full
old/new-head oracle. It observed exactly 4,097 WRITE callbacks, four upstream
calls, a complete driver receipt, clean unmount and sandbox deletion. The
independent verifier passed in 0.019799 s: 4,097 changed runs, both 8,194-byte
file contents and the exact old/new heads
`12ef44464e764a34399e5be721a9d14ab714813471949c06bf20413d2ca6f9a098`
→ `12ad7df0fcfbd7ea584ba47724cdfd04203d04402270b5d95b2be4776502cd5f6a`.
The diagnostic image was `sha256:d8f5b6561d02bece818f449bd91db0ea0380a3cc83544d52bf7e6d07812436cf`.
The product daemon SHA-256 remained
`2903029af36875bc050bd99da497c07d34f385958d68bfd3cbc46f16b54590e3`.

The existing FUSE instrument took cumulative 4 KiB ledger and clock snapshots
every 512 accepted WRITEs. These are **within-run increments**, including the
work to acquire and publish each block; the 4,097th WRITE has no separate
ledger snapshot and is not included in the final counter below.

| WRITEs in block | Ledger reads | Ledger writes | FUSE elapsed for block |
| --- | ---: | ---: | ---: |
| 1–512 | 19,426 | 9,807 | 2.674264 s |
| 513–1,024 | 22,356 | 12,071 | 2.830907 s |
| 1,025–1,536 | 28,173 | 15,190 | 3.271153 s |
| 1,537–2,048 | 29,575 | 16,653 | 3.674774 s |
| 2,049–2,560 | 29,518 | 15,870 | 3.703541 s |
| 2,561–3,072 | 30,285 | 16,500 | 3.909940 s |
| 3,073–3,584 | 30,513 | 16,740 | 3.828887 s |
| 3,585–4,096 | 30,531 | 16,661 | 4.026112 s |

At 4,096, cumulative ledger counts were **220,377 reads / 119,492 writes**.
The first 512-WRITE checkpoint exactly matches the prior 512 row's
19,426/9,807 counts. The last block costs **1.57×** the first block's ledger
reads, **1.70×** its writes and **1.51×** its sampled elapsed time. The extent
root first reached height 2 at WRITE **1,039**; child-edge additions rose
from 1,225 in the first block to 3,991 in the last, while custody additions
stayed near 5,200 per later block. At 4,096, backing allocated bytes were
18,751,488, including 1,974,272 metadata bytes and 409 allocated metadata
pages. These are point-in-run counters, not physical disk bytes or a
phase-local memory peak.

The raw SDK Exec ratios are **5.74×** from 100→512 for **5.12×** the WRITEs,
then **11.80×** from 512→4,097 for **8.00×** the WRITEs. Raw Exec cost per
WRITE rises from 4.124 to 4.623 to 6.814 ms. Different diagnostic sampling
and uncontrolled cache make those cross-run times descriptive only. The
within-run ledger blocks show real count-dependent amplification through the
height transition, followed by a near plateau around 30,000 reads per 512
WRITEs from block four onward. They do **not** support a sustained quadratic
growth claim across 4,096 WRITEs, nor prove a global asymptotic bound. The
four-hop source still fails the original 25 s gate; finishing in the extended
diagnostic does not make it eligible for release or satisfy issue #271.

Reproduction identity: `python3 core/benchmark/fs-bench-pro/separated_writes.py
prepare --reuse-prepared benchmark-results/fs-bench-pro/issue271/fourhop-gate-prepared-v1/prepared.json
--reuse-selection diagnostic4097 --output benchmark-results/fs-bench-pro/issue271/fourhop-4097-count-prepared-v1`,
then `python3 core/benchmark/fs-bench-pro/separated_writes.py run --prepared
benchmark-results/fs-bench-pro/issue271/fourhop-4097-count-prepared-v1/prepared.json
--selection diagnostic4097 --output benchmark-results/fs-bench-pro/issue271/fourhop-4097-count-v1`.
These record the executed commands; the one-attempt contract forbids an
unchanged-arm rerun to select a different number.

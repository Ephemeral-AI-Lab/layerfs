### Milestone 2 exit

- Commit: `ba64ed9`
- Date: 2026-08-10
- Checklist complete: yes
- Commands and environment: Windows, Node 24.11.1, pnpm 10.32.1, workerd 2026-08-10; `pnpm validate:m2`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: storage microbenchmarks are represented by the bounded 100,001-member staging and 100,000-row collection cases; end-to-end B01/B05 evidence is gated at M3/M9
- Validation duration and operation counts: 17.7 seconds total; 44 Node tests plus 5 workerd checks
- Resource high-water: 128 MiB managed-resident default, 64 MiB byte-weighted cache, 16 MiB final-transaction byte ceiling, and a fixed 524,288-byte FastCDC buffer
- Known deviations: none for M2; the public range-mutation and streaming-read integration remains the M3 gate
- Approved to begin next milestone: yes

The storage suite covers deterministic schema creation, writable v1-to-v2
migration, rollback after every migration statement, read-only reopen, scoped
transactions, exact CAS and overlay usage, immutable COW page replacement at
4/8/16 KiB, segmented patches, admission rollback, and staging lease cleanup.
A 100,001-object staged membership seals incrementally and its final certificate
is validated with one bounded query rather than a membership rescan.

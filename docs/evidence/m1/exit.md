### Milestone 1 exit

- Commit: `0eaf3ab`
- Date: 2026-08-10
- Checklist complete: yes
- Commands and environment: Windows, Node 24.11.1, pnpm 10.32.1, workerd 2026-08-10; `pnpm validate:m1`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the pure-algorithm milestone; end-to-end B01/B05 evidence is gated at M3/M9
- Smoke duration and operation counts: 12.1 seconds total; 37 Node tests plus 5 workerd golden/local checks
- Resource high-water: streaming FastCDC retains one 524,288-byte scan buffer; the workerd local overwrite read 524,287 source bytes and hashed 155,909 bytes
- Known deviations: none
- Approved to begin next milestone: yes

The local-rebuild suite covers overwrite, insertion, deletion, truncation,
start/end/EOF boundaries, and 24 replayable seeded edits. Each local result is
compared with the canonical full FastCDC and complete manifest rebuild. The
instrumented source rejects partial reads, caps every request at one configured
FastCDC window, and demonstrates affected-path-only node creation.

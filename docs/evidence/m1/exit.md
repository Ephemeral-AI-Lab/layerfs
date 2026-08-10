### Milestone 1 historical run — sequential acceptance suspended

- Commit: `a1be487d04852b3088e698597746b58eab630db8`
- Date: 2026-08-10
- Status: not currently accepted; this historical run predates the repaired M0
  baseline `f1c4ebd` and must be rerun unchanged or superseded before M2 work
- Checklist complete: not yet reflected in the implementation plan
- Commands and environment: Windows, Node 24.11.1, pnpm 10.32.1, workerd 2026-08-10; `pnpm validate:m1`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the pure-algorithm milestone; end-to-end B01/B05 evidence is gated at M3/M9
- Smoke duration and operation counts: 26.8 seconds total; 46 Node tests plus 5 workerd golden/local checks
- Resource high-water: streaming FastCDC retains one 524,288-byte scan buffer; 100,001-entry construction retained at most 273 records (one 256-entry group plus a 17-row keyset page); the forced streamed fallback read at most 257 source bytes per request
- Known deviations: none
- Approved to begin next milestone: no; pending exact-state M1 revalidation

Canonical construction now consumes an entry iterator and emits encoded nodes
and level records to a caller-supplied durable workspace. Higher levels are
read back through bounded keyset pages; the builder retains neither the entry
stream nor a node map. A file-backed Node SQLite workspace constructs and
validates a genuine 100,001-entry manifest while enforcing a 17-row read page.

The local-rebuild suite covers overwrite, insertion, deletion, truncation,
start/end/EOF boundaries, and 24 replayable seeded edits. Each local result is
compared with the canonical full FastCDC and complete manifest rebuild. The
instrumented source rejects partial reads, caps every request at one configured
FastCDC window, and demonstrates affected-path-only node creation. Its legacy
fixture graph, offset map, boundary map, and object set are now subject to fixed
entry/node/affected-window limits. Crossing any cap selects the streamed
FastCDC plus durable-workspace fallback, which is tested against the canonical
root without retaining a complete entry or node graph.

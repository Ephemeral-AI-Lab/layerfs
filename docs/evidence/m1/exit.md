### Milestone 1 repaired exit

- Commit: `5c1570411352065d8fa9207a9d679ae956830129`
- Date: 2026-08-10
- Checklist complete: yes
- Commands and environment: Windows 11, Node 24.11.1, pnpm 10.32.1,
  workerd 2026-08-10; `pnpm validate:m1`; `pnpm test:m1`;
  `pnpm test:workerd`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the pure-algorithm milestone;
  end-to-end B01/B05 evidence remains gated at M3/M9
- Validation duration and operation counts: 44.01 seconds; 11 Node tests plus
  5 workerd golden/local checks, 0 failures
- Resource high-water: streaming FastCDC retains one 524,288-byte scan buffer;
  100,001-entry construction retained at most 273 records (one 256-entry
  group plus a 17-row keyset page); the forced streamed fallback read at most
  257 source bytes per request
- Known deviations: none for M1; storage and all later runtime paths remain
  provisional and are excluded from this gate
- Approved to begin next milestone: yes, M2 only

Canonical construction consumes an entry iterator and emits encoded nodes and
level records to a caller-supplied durable workspace. Higher levels are read
back through bounded keyset pages; the builder retains neither the entry stream
nor a complete node map. A file-backed Node SQLite workspace constructs and
validates a genuine 100,001-entry manifest while enforcing a 17-row read page.

The local-rebuild suite covers overwrite, insertion, deletion, truncation,
start/end/EOF boundaries, and 24 replayable seeded edits. Each local result is
compared with the canonical full FastCDC and complete manifest rebuild. The
instrumented source rejects partial reads, caps every request at one configured
FastCDC window, and demonstrates affected-path-only node creation. Its bounded
legacy fast path falls back to streamed FastCDC plus a durable workspace before
entry, node, offset, boundary, or object collections can scale with file size.

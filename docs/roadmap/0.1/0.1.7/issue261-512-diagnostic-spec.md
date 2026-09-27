# Issue 261 second count diagnostic: 512 separated mounted writes

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This selection is declared after the one 100-write public diagnostic at source
`b094a4a1f178e01922e084bdea511aa907919801` and before adding or running the
512 selection. The 100-write within-run ledger read/write increments rose when
the file extent index acquired a branch. Source inspection shows that every
copied branch owns each referenced child and every copied leaf owns each Local
payload custody. The 512 selection asks whether this count growth continues
as the unchanged product adds leaf pages. It is a **distinct diagnostic**, not
an unchanged-arm resample or a latency admission row.

Scenario `issue261-separated-512-v1` belongs to
`workspace_mounted_separated_writes`. It uses the same existing 8,194-byte
`data.bin`, source byte `A`, destination byte `X`, mode, old-head oracle,
independently writable prepared-master Store/history copy, public SDK mount,
**one** Exec, **one** explicit Commit, independent verifier, one writer process,
one fd and one-byte positional write syscall at each even offset `2*i` for
`i=0..511`. The exact shell command is
`/fixtures/bin/write-separated data.bin 512`; the writer binary and source
must match the 100-write diagnostic. Expected result: unchanged length/mode,
exactly 512 separated changed runs, 512 replacement bytes and 1,024 file
extents. The old head and all old bytes stay unchanged. The measured driver
does not call internal mutation methods.

The writer emits only cumulative progress at 128, 256, 384 and 512 accepted
syscalls. The diagnostic image emits backing/metadata counter snapshots at
those four successful FUSE WRITE counts, using the same bounded operator
instrumentation as the 100-write image with interval 128. Capture actual
callback classes, ledger reads/writes, metadata reads, payload and root
ownership, allocated bytes, Commit lower/Service/Store counters, Exec/Commit/
complete/cleanup wall, resource scopes, identities and failures. Four
snapshots may scan retained payload records; report the overhead as diagnostic
and never use the elapsed row for a speed claim. No per-write RPC or output
write is introduced.

Use the identical locked release driver, verifier and daemon product binaries
where their compilation seals match; build a fresh interval-128 diagnostic
image with the same writer and daemon bytes. Reuse the previously closed
prepared master **only** after checking its fixture, old-head proof,
Store/history hashes, cursor-key custody, product seal, writer hash and source
compatibility; clone an independent writable byte copy. Do not regenerate the
fixture or seed branch. The source cache is uncontrolled, with no warming;
`cache_status=INELIGIBLE`, `admission_eligible=false`. The complete command
limit is 15 s, independent oracle target under 10 s, and every focused test
command under 30 s. One attempt only; a timeout or failure remains append-only
evidence. Do not change the count, deadline, workers or cache policy to pass.

# #818 M3 latency diagnosis

> **Status:** Diagnostic receipt, one run per command; no product change. Binary `fs_call_round_trips-9eab8e660a198019` (code `5af53d4667`), PG `lfs818-bench-pg`, netem inside the container only (removed after: `qdisc noqueue 0: root`). Host load 7 → 16.

Question: why 7 rows at +10 ms exceeded the 1.10× latency band at M3 (1.27–1.43×).

**Verdict:** an environment/calibration artefact, not extra round trips.

- The trip counter is exact: wire exchanges equal counted trips in every case
  (`summary-per-case.txt`; write.new_file 43 = 42 + its counted connect).
- No TCP slow start: MTU 65535, largest reply 19.4 KB in 1–3 segments.
- No pipelined flight waits twice: W4 reservation and publish flights each
  leave in one packet and wait once.
- The excess per trip is Docker Desktop netem releasing replies late (3–4.6 ms
  past 10 ms, vs 0.1–0.3 ms locally) and longer client gaps under load
  (1.6–3.3 ms vs 0.4–0.8 ms). The `SELECT 1` calibration median (1.6–2.3 ms
  late) understates the skewed mean real trips pay (2.5–2.9 ms;
  `calibration-decomposition.txt`).
- The ratio moves run to run with the same binary: write.overwrite 1.36× at M3
  → 1.07× here, apply.10_writes 0.95× → 1.97×, revision.h100 1.25×.
- Not explained: revision.h100 cost 12.05 ms/trip at M2 vs 16–17 at M3 (no M2
  capture).

Suggested (not done, harness change): judge latency against the mean of a
pause-matched calibration or the in-container measured delay; treat Docker
Desktop netem as indicative, not a gate.

Packet captures (26 MB) are kept outside the repository at
`~/.cache/layerfs-818-receipts/M3-latency-diag/`; their hashes are in
`PCAPS.sha256`. `pg-history.run1-capture-dropped-105-packets.log` is a first
run whose capture dropped packets; it was repeated losslessly.

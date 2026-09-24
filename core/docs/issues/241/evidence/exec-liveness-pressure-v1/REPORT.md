# #241 synthetic host slot pressure result

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The sole [frozen plan](plan.json) ran at source `d3bae7c75213d1cb151a84146e3a0a8b0c210c5d` with image `sha256:37b3caabf807af540a8853287706a380afbf2de254e872669227e8e8dd0b89ef`. It held three incomplete TCP handshakes after public SDK Mount, then issued one baseline Exec from a fresh 1 MiB master copy. The [attempt](attempt.json), [raw receipt/logs](raw/) and [SHA inventory](RAW-SHA256.txt) retain the failure; no retry was made.

The host acceptor recorded **5 accepted, 4 admitted, 4 reaped, peak live 4, one capacity drop, capacity 4, terminal error none**. The second missing-path Inspect's new TCP connection took 1.076 ms; Noise failed after 0.238 ms. Exec returned a known shell exit 1 after 8 ms with `can't create .position-baseline: I/O error`. Docker showed the daemon running without OOM. Unmount, Sandbox deletion and final absence passed. This deliberately pressured diagnostic is **FAIL** for successful create, with complete cause attribution.

Capacity refusal can break the baseline route, but this observed refusal has an immediate signature. It does **not** reproduce the frozen v3 five-second `Unknown,true`, and it does not establish that v3 had a full host session cap.

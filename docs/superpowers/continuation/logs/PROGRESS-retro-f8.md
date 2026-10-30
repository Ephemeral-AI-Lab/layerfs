Retro finding 8 — blame single conversion (implemented directly by the
commander; kimi quota exhausted, no kimi session used).
Base: private area-p/p1-p15 tip 7d18a1551.
Change: blame.rs only, 1 file +13/-10. Prefix collection 3 text()
conversions -> 1; track selection 2 -> 1; short-circuit behavior
preserved exactly; no new test (no honest counter exists, and adding
test instrumentation is forbidden).
Verification (clone branch area-p/retro-f8): blame coarse 14/0,
area_p8_reads 8/0, area_p1_history 8/0, sdk_method_golden 1/0
(umask 022), bridge control 4/0, bridge observed 7/0, fmt PASS,
workspace clippy (excl. node, -D warnings) PASS, product boundary
967 PASS, tool self-tests 155/0. JS not re-run (no JS touched).
Mounted/real-E2E NOT RUN (no /dev/fuse, no Docker). Durable NOT RUN.
Production LOC: 104896 -> 104899 (+3).
Private PR #15 (c9950a6c), merged to 9a4fb5b5f.

# LayerFS Stage One Part 1

Disposition: REVISE.

- Complete wall: 42866453417 ns
- Store resets: 54 / 54
- Maximum user file: 104857600 bytes
- Store database maximum (separate authority): 218378240 bytes

| Metric | n | min ns | p50 ns | p95 ns | max ns | throughput MiB/s | target |
|---|---:|---:|---:|---:|---:|---:|---|
| A01 | 3 | 382224917 | 382263250 | 382769166 | 382769166 | 261.600 | PASS (>=250 MiB/s) |
| A02 | 300 | 333750 | 679021 | 1025875 | 1522875 | 92.044 | REVISE (p50<=0.5ms and p95<=1.0ms) |
| A03a | 3 | 375563459 | 376620375 | 377674000 | 377674000 | 265.519 | PASS (>=150 MiB/s) |
| A03b | 3 | 383309958 | 388027875 | 388633000 | 388633000 | 257.713 | PASS (>=150 MiB/s) |
| A04/logical | 3 | 4073500 | 4103541 | 4450875 | 4450875 | N/A | PASS (p50<=15ms) |
| A04/native-edit-plus-checkpoint | 3 | 14761708 | 16582167 | 16944542 | 16944542 | N/A | PASS (p50<=20ms) |
| A05/logical | 3 | 3940125 | 3999166 | 4328292 | 4328292 | N/A | REPORT_ONLY |
| A05/native-edit-plus-checkpoint | 3 | 33347833 | 34602917 | 37285250 | 37285250 | N/A | REPORT_ONLY |
| A06/logical | 3 | 3795375 | 4031041 | 4336916 | 4336916 | N/A | REPORT_ONLY |
| A06/native-edit-plus-checkpoint | 3 | 20692750 | 20971166 | 23153083 | 23153083 | N/A | REPORT_ONLY |
| A07/logical | 3 | 3931625 | 4351875 | 4403291 | 4403291 | N/A | REPORT_ONLY |
| A07/native-edit-plus-checkpoint | 3 | 7323792 | 7418042 | 7832917 | 7832917 | N/A | REPORT_ONLY |
| A08/logical | 3 | 2616625 | 2755083 | 2935166 | 2935166 | N/A | REPORT_ONLY |
| A08/native-edit-plus-checkpoint | 3 | 6679875 | 7013750 | 7207083 | 7207083 | N/A | REPORT_ONLY |
| A09 | 3 | 356262166 | 357578208 | 357712834 | 357712834 | 279.659 | PASS (>=200 MiB/s) |
| A10 | 3 | 389914958 | 391273959 | 396090708 | 396090708 | 255.575 | PASS (>=150 MiB/s) |
| A11 | 3 | 37375 | 44042 | 47959 | 47959 | N/A | PASS (p50<=5ms) |
| A12 | 3 | 15443792 | 16429208 | 16442625 | 16442625 | N/A | PASS (p50<=25ms) |
| A13 | 11 | 768209 | 849000 | 1469333 | 1469333 | N/A | PASS (p50<=4ms) |
| A14/edit | 4 | 1845791 | 2596916 | 3249333 | 3249333 | N/A | REPORT_ONLY |
| A15 | 3 | 2695667 | 3435625 | 3855792 | 3855792 | N/A | REPORT_ONLY |
| A17/checkpoint | 100 | 1770417 | 2465333 | 3187708 | 3717458 | N/A | REPORT_ONLY |
| A17/edit-plus-checkpoint | 100 | 10692501 | 12867646 | 14287583 | 15187166 | N/A | REPORT_ONLY |

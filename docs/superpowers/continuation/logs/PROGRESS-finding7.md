area_p8_reads old-code build: FAIL (new test code compile errors; correcting before the discriminating run)
area_p8_reads old-code two-size test: FAIL 0 passed / 1 failed (intended: first reply missing nextOffset; 7 filtered)
sdk_text_scan old-code build: FAIL (new test typo refused/refusal; correcting before the discriminating run)
area-p old-code: FAIL 2 of 245 (intended: offset rejected as unknown; exact offset-type refusal missing)
calls old-code: FAIL 1 of 66 (intended: offset request rejected before binding)
sdk_text_scan old-code cursor test: FAIL 0 passed / 1 failed (intended: nextOffset absent; 1 filtered)
area_p8_reads old-code full suite: FAIL 4 passed / 4 failed (all discriminating cursor tests missing nextOffset; numeric compatibility guard passed as designed)
area_p8_reads implementation pass 1: FAIL 7 passed / 1 failed (beyond-size fixture used size exactly; two-size 2 MiB=1,663,637 bytes/0 reads/43 pages, 4 MiB=3,221,324 bytes/0 reads/86 pages, late=15,327 bytes)
sdk_text_scan implementation pass 1: FAIL 0 passed / 1 failed (valid-text fixture produced Binary; diagnosing fixture vs scanner)
area_p8_reads implementation pass 2: PASS 8 / 8 (two-size 2 MiB=2,286,557 bytes/21 reads/43 pages, 4 MiB=3,918,010 bytes/46 reads/86 pages, late=15,711 bytes; ratios and late bound passed)
sdk_text_scan cursor classification: PASS 1 / 1 (1 pre-existing test filtered)
area-p implementation: PASS 241 cases + 2 P7 groups + 1 P8 group + 1 budget traversal
calls implementation: PASS 66 / 66
cli.test implementation pass 1: FAIL 105 / 106 (only generated CLI docs table lacks --offset)
sdk_method_golden implementation (umask 022): PASS 1 / 1
FINAL area_p8_reads: PASS 8 / 8 (2 MiB=1,663,637 bytes/0 reads/43 pages; 4 MiB=3,221,324 bytes/0 reads/86 pages; late=15,327 bytes)
FINAL sdk_text_scan: PASS 2 / 2
FINAL sdk_method_golden (umask 022): PASS 1 / 1
FINAL area_p1_history: PASS 8 / 8
FINAL area_p14_paging: PASS 8 / 8
FINAL area_p7_attribution: PASS 11 / 11
FINAL area-p: PASS 241 cases + 2 P7 groups + 1 P8 group + 1 budget traversal
FINAL calls: PASS 66 / 66
FINAL cli.test: PASS 106 / 106
FINAL product boundary: PASS (967 production files scanned)
FINAL tool self-tests: PASS 155 / 155
fmt check pass 1: FAIL (rustfmt layout differences in new Rust code)
fmt final: PASS
clippy pass 1: FAIL (lines_until has 8/7 arguments; replacing line+offset pair with typed Start)
clippy pass 2: FAIL (test hex format_collect and one single-element loop; correcting test code)
FINAL fmt check: PASS
FINAL workspace clippy (exclude layerfs-node, all-targets, -D warnings): PASS
FINAL post-clippy area_p8_reads: PASS 8 / 8 (2 MiB=1,663,637 bytes/0 reads/43 pages; 4 MiB=3,221,324 bytes/0 reads/86 pages; late=15,327 bytes)
FINAL post-clippy sdk_text_scan: PASS 2 / 2
FINAL post-clippy product boundary: PASS (967 production files scanned)
NOT RUN mounted/real-E2E: /dev/fuse and Docker unavailable

# Original lifecycle test failure

15-engine_lifecycle: three cases PASS, success case FAILED after exact stdout marker because macOS shutdown on the already-closed finite external peer returned ENOTCONN. Original marker/progress and fence failure were correctly retained. The follow fixture closed immediately after sending all bytes, unlike an actual follow subscription. Fixture now keeps only log-follow connections open through one bounded read, waiting for the client fence. No product timeout/workload/fence relaxation. First failure and subsequent bounded peer admission failure are preserved. Rerun only affected success case; unchanged three results retain their prior scope.

08/10 source builds FAILED because the textual Duration repair duplicated two dereferences; the exact source is repaired with direct copied Duration arguments. 12/14 locked combined --no-run PASS. No Cargo operation was still running when the next began; prior result handles were collected later.

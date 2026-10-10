| Cell | Command | Command ms, in-container clock | Command ms, host clock | Mount ms | Unmount ms | Requests | Owner jobs | Statements | Overlay logical bytes | Store logical bytes | Regime | Verifier / custody / cleanup |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| C01 | create 1000 files | 279.7 | 315.8 | 8.8 | 5.6 | 5002 | 5002 | 44004 | 430080 | 221312 | MIXED | PASS / KNOWN_STOP / Gone |
| C02 | create 1000, stat 1000 | 842.5 | 881.6 | 7.6 | 6.4 | 6002 | 6002 | 46004 | 430080 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C03 | create 1000, remove them | 549.5 | 594.0 | 9.6 | 3.8 | 9022 | 9038 | 102540 | 430080 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C04 | 100 directories, 1000 files | 716.7 | 750.5 | 8.9 | 5.4 | 5342 | 5342 | 42644 | 376832 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C05 | that tree, then `find` | 744.0 | 785.1 | 8.8 | 6.3 | 5890 | 6000 | 46637 | 376832 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C06 | write 64 MiB | 99.5 | 136.3 | 8.2 | 5.1 | 518 | 518 | 9766 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C07 | write 64 MiB, copy it | 234.5 | 268.8 | 8.0 | 7.1 | 1551 | 1550 | 23133 | 136708096 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C08 | write 64 MiB, read it | 92.7 | 129.6 | 10.9 | 5.6 | 521 | 521 | 9783 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C09 | read a 64 MiB base file | 60.5 | 93.0 | 8.8 | 6.2 | 517 | 517 | 1055 | 229376 | 204800 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C10 | copy a 64 MiB base file | 147.4 | 187.5 | 7.5 | 8.4 | 1548 | 1547 | 14408 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C11 | overwrite a 64 MiB base file | 95.2 | 132.4 | 8.1 | 7.0 | 517 | 517 | 9760 | 68468736 | 204800 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C12 | `git init`, 100 files, add, commit | 207.7 | 243.2 | 7.7 | 5.7 | 3436 | 3439 | 24146 | 335872 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |

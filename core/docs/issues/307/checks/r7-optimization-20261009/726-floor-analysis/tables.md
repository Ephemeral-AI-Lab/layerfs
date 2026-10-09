
#### TRUE  (710-floor-TRUE)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 53.7 | 0.0 | 53.7 | — | — | — | — | — | 0.0 | — |
| P1 | 51.3 | 0.0 | 51.3 | 1 | 25.0 | 0.0 | 45.5 | 1.00 | 0.0 | -20.5 |
| P1E | 34.8 | 0.0 | 34.8 | 1 | 22.9 | 0.0 | 46.6 | 1.00 | 0.0 | -23.7 |
| P2 | 47.2 | 0.0 | 47.2 | 1 | 26.0 | 0.1 | 58.1 | 1.00 | 0.0 | -32.1 |

#### C01  (711-floor-C01)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 62.0 | 15.5 | 46.6 | — | — | — | — | — | 16.0 | — |
| P1 | 356.3 | 307.1 | 49.1 | 7000 | 43.9 | 97.3 | 13.9 | 0.86 | 77.0 | 19.0 |
| P1E | 285.5 | 243.2 | 42.3 | 5001 | 48.6 | 71.0 | 14.2 | 0.80 | 54.0 | 23.6 |
| P2 | 378.6 | 326.3 | 52.3 | 7000 | 46.6 | 109.3 | 15.6 | 1.00 | 84.0 | 19.0 |
| L (659) | 379.7 | not recorded (streams 372.8) | not recorded | 5001 | 74.5 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 183.4 (in-container Exec) | not in its span | 7001 (predicted) | 26.2 | — | — | — | — | — |

#### C02  (712-floor-C02)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 551.0 | 507.7 | 43.2 | — | — | — | — | — | 532.0 | — |
| P1 | 897.0 | 850.8 | 46.3 | 8002 | 106.3 | 108.2 | 13.5 | 0.88 | 604.0 | 17.3 |
| P1E | 844.1 | 797.2 | 47.0 | 6003 | 132.8 | 86.3 | 14.4 | 0.83 | 591.0 | 20.0 |
| P2 | 935.0 | 888.1 | 46.9 | 8002 | 111.0 | 118.8 | 14.8 | 1.00 | 630.0 | 17.4 |
| L (663) | 914.8 | not recorded (streams 907.8) | not recorded | 6002 | 151.2 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 965.0 (in-container Exec) | not in its span | 8001 (predicted) | 120.6 | — | — | — | — | — |

#### C03  (713-floor-C03)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 72.1 | 20.9 | 51.2 | — | — | — | — | — | 22.0 | — |
| P1 | 494.0 | 448.7 | 45.3 | 11012 | 40.8 | 141.5 | 12.8 | 0.82 | 107.0 | 18.2 |
| P1E | 406.6 | 366.5 | 40.1 | 9013 | 40.7 | 117.9 | 13.1 | 0.78 | 86.0 | 18.0 |
| P2 | 501.1 | 454.1 | 47.0 | 11012 | 41.2 | 160.7 | 14.6 | 1.02 | 112.0 | 16.5 |
| L (667) | 715.9 | not recorded (streams 710.0) | not recorded | 9021 | 78.7 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 410.8 (in-container Exec) | not in its span | 10011 (predicted) | 41.0 | — | — | — | — | — |

#### C04  (714-floor-C04)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 463.0 | 423.4 | 39.6 | — | — | — | — | — | 454.0 | — |
| P1 | 899.5 | 854.2 | 45.3 | 7341 | 116.4 | 123.4 | 16.8 | 1.00 | 570.0 | 21.9 |
| P1E | 819.0 | 773.0 | 46.0 | 5342 | 144.7 | 97.7 | 18.3 | 0.98 | 540.0 | 25.3 |
| P2 | 891.7 | 846.7 | 45.0 | 7341 | 115.3 | 124.9 | 17.0 | 1.00 | 552.0 | 23.1 |
| L (671) | 823.4 | not recorded (streams 815.2) | not recorded | 5342 | 152.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 952.6 (in-container Exec) | not in its span | 7321 (predicted) | 130.1 | — | — | — | — | — |

#### C05  (715-floor-C05)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 474.4 | 435.1 | 39.3 | — | — | — | — | — | 470.0 | — |
| P1 | 974.8 | 928.7 | 46.1 | 7888 | 117.7 | 134.2 | 17.0 | 0.98 | 615.0 | 22.8 |
| P1E | 854.2 | 813.7 | 40.4 | 5889 | 138.2 | 107.8 | 18.3 | 0.98 | 549.0 | 26.6 |
| P2 | 931.6 | 888.1 | 43.5 | 7888 | 112.6 | 133.9 | 17.0 | 1.00 | 563.0 | 24.2 |
| L (675) | 880.5 | not recorded (streams 873.9) | not recorded | 5889 | 148.4 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 949.6 (in-container Exec) | not in its span | 7644 (predicted) | 124.2 | — | — | — | — | — |

#### C06  (716-floor-C06)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 65.9 | 17.3 | 48.6 | — | — | — | — | — | 18.0 | — |
| P1 | 100.3 | 55.2 | 45.1 | 518 | 106.5 | 24.0 | 46.4 | 1.00 | 22.0 | 17.6 |
| P1E | 101.8 | 55.0 | 46.8 | 517 | 106.4 | 24.5 | 47.4 | 1.00 | 22.0 | 16.4 |
| P2 | 100.5 | 59.1 | 41.4 | 518 | 114.1 | 26.6 | 51.4 | 1.00 | 23.0 | 18.3 |
| L (679) | 226.8 | not recorded (streams 219.9) | not recorded | 517 | 425.4 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 78.6 (in-container Exec) | not in its span | 519 (predicted) | 151.4 | — | — | — | — | — |

#### C07  (717-floor-C07)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 81.0 | 33.9 | 47.1 | — | — | — | — | — | 35.0 | — |
| P1 | 173.2 | 128.9 | 44.3 | 1553 | 83.0 | 55.5 | 35.7 | 1.00 | 47.0 | 17.0 |
| P1E | 174.7 | 129.0 | 45.7 | 1550 | 83.2 | 53.4 | 34.4 | 1.00 | 46.0 | 19.1 |
| P2 | 184.8 | 131.6 | 53.2 | 1553 | 84.8 | 54.7 | 35.2 | 1.00 | 47.0 | 19.3 |
| L (683) | 499.4 | not recorded (streams 492.5) | not recorded | 1550 | 317.7 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 171.6 (in-container Exec) | not in its span | 1553 (predicted) | 110.5 | — | — | — | — | — |

#### C08  (718-floor-C08)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 82.6 | 27.6 | 55.0 | — | — | — | — | — | 29.0 | — |
| P1 | 111.4 | 67.0 | 44.4 | 523 | 128.1 | 26.7 | 51.0 | 0.98 | 31.0 | 17.9 |
| P1E | 114.4 | 65.2 | 49.2 | 521 | 125.2 | 25.2 | 48.3 | 0.99 | 32.0 | 15.4 |
| P2 | 95.1 | 61.6 | 33.6 | 523 | 117.8 | 24.0 | 46.0 | 1.00 | 26.0 | 22.1 |
| L (687) | 261.8 | not recorded (streams 254.5) | not recorded | 521 | 488.5 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 106.1 (in-container Exec) | not in its span | 523 (predicted) | 202.9 | — | — | — | — | — |

#### C09  (719-floor-C09)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 46.0 | 6.6 | 39.4 | — | — | — | — | — | 6.0 | — |
| Nc | 67.8 | 23.0 | 44.8 | — | — | — | — | — | 14.0 | — |
| P1 | 90.4 | 46.6 | 43.8 | 517 | 90.1 | 26.1 | 50.4 | 1.00 | 19.0 | 2.9 |
| P1E | 82.6 | 41.2 | 41.4 | 517 | 79.8 | 22.1 | 42.8 | 0.99 | 17.0 | 4.1 |
| P1Ec | 101.6 | 58.7 | 42.8 | 517 | 113.6 | 32.9 | 63.7 | 1.00 | 25.0 | 1.5 |
| P1c | 100.2 | 56.3 | 43.9 | 517 | 108.8 | 30.7 | 59.4 | 1.00 | 25.0 | 1.1 |
| P2 | 86.2 | 43.2 | 43.0 | 517 | 83.7 | 22.9 | 44.4 | 1.00 | 18.0 | 4.5 |
| P2c | 93.0 | 58.5 | 34.5 | 517 | 113.1 | 32.9 | 63.7 | 1.00 | 24.0 | 3.1 |
| L (691) | 167.1 | not recorded (streams 160.6) | not recorded | 517 | 310.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 106.2 (in-container Exec) | not in its span | 517 (predicted) | 205.4 | — | — | — | — | — |

#### C10  (720-floor-C10)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 66.9 | 16.8 | 50.1 | — | — | — | — | — | 18.0 | — |
| Nc | 80.3 | 27.2 | 53.1 | — | — | — | — | — | 22.0 | — |
| P1 | 134.9 | 89.7 | 45.1 | 1547 | 58.0 | 46.4 | 30.0 | 0.67 | 32.0 | 7.3 |
| P1E | 137.1 | 93.7 | 43.4 | 1546 | 60.6 | 49.5 | 32.0 | 0.67 | 34.0 | 6.6 |
| P1Ec | 154.1 | 107.6 | 46.5 | 1546 | 69.6 | 57.9 | 37.5 | 0.67 | 39.0 | 6.9 |
| P1c | 140.2 | 98.5 | 41.7 | 1547 | 63.7 | 53.3 | 34.5 | 0.67 | 35.0 | 6.6 |
| P2 | 127.9 | 85.5 | 42.4 | 1547 | 55.3 | 54.1 | 35.0 | 1.00 | 33.0 | -1.0 |
| P2c | 135.7 | 92.7 | 43.0 | 1547 | 59.9 | 58.6 | 37.9 | 1.00 | 39.0 | -3.2 |
| L (695) | 400.2 | not recorded (streams 393.7) | not recorded | 1546 | 254.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 178.2 (in-container Exec) | not in its span | 1547 (predicted) | 115.2 | — | — | — | — | — |

#### C11  (721-floor-C11)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 62.5 | 12.4 | 50.2 | — | — | — | — | — | 13.0 | — |
| P1 | 97.3 | 47.6 | 49.7 | 518 | 91.8 | 16.5 | 31.9 | 1.00 | 23.0 | 15.5 |
| P1E | 87.2 | 47.4 | 39.8 | 517 | 91.7 | 17.4 | 33.6 | 1.00 | 21.0 | 17.5 |
| P2 | 87.1 | 48.5 | 38.6 | 518 | 93.6 | 17.8 | 34.3 | 1.00 | 21.0 | 18.8 |
| L (699) | 242.4 | not recorded (streams 235.9) | not recorded | 517 | 456.2 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 85.8 (in-container Exec) | not in its span | 518 (predicted) | 165.6 | — | — | — | — | — |

#### C12  (722-floor-C12)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 57.3 | 14.6 | 42.7 | — | — | — | — | — | 16.0 | — |
| P1 | 265.2 | 218.6 | 46.7 | 3994 | 54.7 | 59.0 | 14.8 | 0.89 | 55.0 | 26.2 |
| P1E | 244.7 | 197.7 | 47.0 | 3435 | 57.6 | 50.6 | 14.7 | 0.86 | 46.0 | 29.4 |
| P2 | 249.2 | 205.0 | 44.2 | 3692 | 55.5 | 56.2 | 15.2 | 1.00 | 51.0 | 26.5 |
| L (703) | 317.7 | not recorded (streams 311.8) | not recorded | 3436 | 90.8 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 189.3 (in-container Exec) | not in its span | 3976 (predicted) | 47.6 | — | — | — | — | — |

#### S01  (723-floor-S01)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 65.1 | 27.3 | 37.8 | — | — | — | — | — | 28.0 | — |
| P1 | 4028.8 | 3977.7 | 51.1 | 101001 | 39.4 | 1201.5 | 11.9 | 1.00 | 1020.0 | 17.4 |
| P2 | 4119.8 | 4064.7 | 55.1 | 101001 | 40.2 | 1231.3 | 12.2 | 1.00 | 1036.0 | 17.8 |

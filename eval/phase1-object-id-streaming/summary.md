# Phase 1 canonical-object baseline

This is a correctness-preserving microbenchmark for the Phase 1 core.

It measures bounded path validation, canonical encode/decode, and
BLAKE3 identity work for representative byte and directory objects.

It does not measure CDC, CAS, SQLite, materialization, or large-file
small-edit behavior; those remain Phase 2 and later gates.

Each case has one warm-up and five measured iterations. `peak_memory_bytes`
remains explicitly unavailable because this process does not sample RSS.
Capture peak memory externally with `/usr/bin/time -l` or Instruments.

| Case | Median ns | Input bytes | Output bytes | Correct |
|---|---:|---:|---:|:---:|
| `bytes-1024/encode_vec` | 166 | 1037 | 1037 | true |
| `bytes-1024/encode_writer` | 167 | 1037 | 1037 | true |
| `bytes-1024/decode_slice` | 166 | 1037 | 1024 | true |
| `bytes-1024/decode_reader` | 167 | 1037 | 1024 | true |
| `bytes-1024/hash_slice` | 1500 | 1037 | 32 | true |
| `bytes-1024/hash_reader` | 1875 | 1037 | 32 | true |
| `bytes-1024/object_id` | 1583 | 1024 | 32 | true |
| `bytes-1048576/encode_vec` | 19875 | 1048589 | 1048589 | true |
| `bytes-1048576/encode_writer` | 51375 | 1048589 | 1048589 | true |
| `bytes-1048576/decode_slice` | 58166 | 1048589 | 1048576 | true |
| `bytes-1048576/decode_reader` | 59417 | 1048589 | 1048576 | true |
| `bytes-1048576/hash_slice` | 748917 | 1048589 | 32 | true |
| `bytes-1048576/hash_reader` | 1144875 | 1048589 | 32 | true |
| `bytes-1048576/object_id` | 704959 | 1048576 | 32 | true |
| `bytes-8388608/encode_vec` | 234291 | 8388621 | 8388621 | true |
| `bytes-8388608/encode_writer` | 487125 | 8388621 | 8388621 | true |
| `bytes-8388608/decode_slice` | 715292 | 8388621 | 8388608 | true |
| `bytes-8388608/decode_reader` | 681125 | 8388621 | 8388608 | true |
| `bytes-8388608/hash_slice` | 6258291 | 8388621 | 32 | true |
| `bytes-8388608/hash_reader` | 9540334 | 8388621 | 32 | true |
| `bytes-8388608/object_id` | 5998500 | 8388608 | 32 | true |
| `directory-16/encode_vec` | 334 | 781 | 781 | true |
| `directory-16/decode_reader` | 2708 | 781 | 781 | true |
| `directory-16/hash_reader` | 1167 | 781 | 32 | true |
| `directory-256/encode_vec` | 4208 | 12301 | 12301 | true |
| `directory-256/decode_reader` | 35042 | 12301 | 12301 | true |
| `directory-256/hash_reader` | 14250 | 12301 | 32 | true |
| `directory-4096/encode_vec` | 65917 | 196621 | 196621 | true |
| `directory-4096/decode_reader` | 584833 | 196621 | 196621 | true |
| `directory-4096/hash_reader` | 210292 | 196621 | 32 | true |
| `path-short/validate` | 83 | 5 | 5 | true |
| `path-max/validate` | 10375 | 4095 | 4095 | true |

Phase 1 is eligible to close only when all cases are correct, the
results and environment artifacts are retained, and the external peak
memory observation is recorded as a value or explicitly unavailable.

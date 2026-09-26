# #232 Phase 1C count diagnostic contract

**Status:** prospective, frozen before tool changes or live attempts. This is
`issue232-phase1c-complexity-v1`, separate from the #232 v3 56-case registry
and #241 v4 insert rows. Contract parent source is
`e0ff52bb6aa76d48047090c77e4bd6771d25a666`. This contract
sets no latency PASS target; every timed value is diagnostic and subject to
the existing cache eligibility rules.

All routes use a locked release public `layerfs-sdk` driver to create Project,
Branch, Sandbox and mounted Workspace, then one public `WorkspaceApi::exec`
command followed by one public `WorkspaceApi::commit`. A cooperating mounted
editor issues only checked Linux range ioctls for mutations and POSIX reads
for boundary observation. Status, unmount and Sandbox Delete are public SDK.
An independent verifier checks full resulting bytes, canonical root,
mode/mtime, old Commit and reopened Branch. No benchmark-only product
visibility or direct SDK edit path is allowed.

## Declared selections

The diagnostic registry will have exactly nine rows, one attempt each. All
source files contain the deterministic prepared-master bytes from the existing
fs-bench-pro generator; new cutoff masters are prepared once and sealed before
the first attempt. Each attempt uses `--setup clone` semantics: an independent
writable byte copy of a closed, validated master outside the operation timer.
The resulting [registry](../../../../../benchmark/fs-bench-pro/registry/workspace-exec-complexity-v1.json)
has SHA-256 `670973c95d747828fe2db02c82323a4df4f585bc3d9ed011e2c378159a731cad`.
Its command, fixture and result hashes, callback counts and payload identity
are exact per row; [the generator](../../../../../benchmark/fs-bench-pro/shared/edit_complexity_v1.py)
rechecks its bytes. The mounted batch command uses `splice-batch --count R
--output-version 5`; single-edit commands use `splice --output-version 4`.

| Group | Pristine bytes | Mounted mutation | Expected result bytes |
| --- | ---: | --- | ---: |
| repeated-1 | 1,048,576 | 1 insert, 4,096 literal bytes | 1,052,672 |
| repeated-32 | 1,048,576 | 32 inserts, 128 literal bytes each | 1,052,672 |
| repeated-128 | 1,048,576 | 128 inserts, 32 literal bytes each | 1,052,672 |
| locality-1m | 1,048,576 | middle 4,096-byte overwrite | 1,048,576 |
| locality-100m | 104,857,600 | middle 4,096-byte overwrite | 104,857,600 |
| locality-capped500m | 524,283,904 | middle 4,096-byte overwrite | 524,283,904 |
| cutoff-below | 131,071 | middle 4,096-byte overwrite | 131,071 |
| cutoff-at | 131,072 | middle 4,096-byte overwrite | 131,072 |
| cutoff-above | 131,073 | middle 4,096-byte overwrite | 131,073 |

For repeated rows, the 4,096 replacement bytes are one sealed payload file.
Request `i` (zero based) inserts slice
`payload[i * (4096/R) .. (i+1) * (4096/R)]` at current-file offset
`524288 + i * (256 + 4096/R)`. This places edits at increasing distinct base
locations with 256 untouched base bytes between them. No request deletes
bytes. Each request checks STATE, issues EDIT and checks STATE/revision,
file length, mtime and bounded adjacent bytes before the next request. There
are exactly `2R` STATE and `R` EDIT callbacks, 4,096 accepted literal bytes,
one SDK Exec, one SDK Commit and zero shifted suffix bytes per row. A failed
or uncertain edit stops the command; it is never retried as another sample.

For locality and cutoff rows, the offset is `(pristine_bytes - 4096) / 2`
rounded down to an integer. Delete and replace exactly 4,096 bytes through
one inline ioctl using the first 4,096 bytes of the same sealed payload file.
Expected callbacks are two STATE and one EDIT, with 4,096 accepted bytes and
zero shifted suffix bytes. The construction policy is the default exclusive
128 KiB cutoff: 131,071 is WholeFile and 131,072/131,073 are Chunked.
No policy override or worker change is allowed.

## Count and resource questions

The repeated screen records the piece count after each request, cumulative
piece visits during splice, metadata pages constructed/written, final Commit
lowering visits, ioctl callback counts and accepted bytes. Count work at
1/32/128 with total replacement bytes fixed. A piece-index change requires
both superlinear visits and material LFT1 wall share in the supported route;
do not infer materiality from an `O(P)` source loop alone.

The locality screen records actual CDC input bytes, affected extent-tree
pages/paths, newly stored CAS objects, canonical extent counts and suffix
read/write callbacks at 1/100/capped-500 MiB. A file-size-proportional count
needs a traced cause before C1 changes. The cutoff screen records constructed
bytes, final representation/root and LFT1 sampled RSS coverage; its bounded
WholeFile reconstruction is accepted if correctness and budgets hold. Source
code, existing counters and a minimal diagnostic instrument may supply counts;
timing claims use only LFT1. No counter substitutes for wall/CPU/RSS and no
post-run Store page count is attributed to a timed substep.

Before the first live attempt, append a committed `PRE_RUN.json` with exact
source/tree, product/compilation/harness seals, binary hashes, image ID,
kernel/fuser, registry hash, payload and prepared-master hashes, fresh output
paths, cutoff policy and cache label. Changing tool or product source requires
a new identity and, for a product algorithm change, a prospective proof
selection. Run each registered count once with a complete command ≤15 s,
independent verifier <10 s, one construction worker and append-only receipts.
Keep all FAIL, INELIGIBLE and NOT_RUN rows. The Linux FUSE backing domain is
`INELIGIBLE` for cold latency until invalidation plus full residency checks
exist; the count and functional conclusions can still be reported.

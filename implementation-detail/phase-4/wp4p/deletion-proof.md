# WP4-P selected-only deletion proof

- Starting checkpoint: `9def7af5ab2b408121b9dcbe40b6affa007626e5`
- Selected file layout: `K64/F64`
- Selected directory ceiling: `DIR256K` (`262,144` bytes)
- Production profile ID:
  `b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1`

## Removed live profile and campaign surface

```text
K59-F101
K256-F256
DIR64K
DIR1M
FILE_CANDIDATES
DIR_CANDIDATES
CAMPAIGN_ORDER
candidate_by_name
SOURCE_512 and retained 512-MiB fixture custody
multi-profile template/copy/scheduler/summary functions
LAYERFS_ALLOW_RETIRED_PROFILE_CAMPAIGN
--prepare-fixtures / --campaign
--retired-prepare-fixtures / --retired-profile-campaign
--prepare-row / --row
implementation-detail/phase-4/test/run-phase4-fast.sh
private `layerfs/mapping-profile/wp4m/v1` admission ID
```

The public file validator no longer accepts leaf/fanout parameters. The public
directory validator no longer accepts a page ceiling. Runtime encoding and
validation use only the selected constants. Synthetic compact malformed tests
do not expose a durable format selector.

## Active-source search

The terminal audit searches `crates/` and the live scripts under
`implementation-detail/phase-4/test/`, excluding immutable checkpoint JSONL,
for the removed names, IDs, CLI arguments, campaign symbols, and archival
override. Success is zero matches.

Historical Markdown and CP-0001 through CP-0006 evidence intentionally retain
the old names and private profile ID as decision history. They are not live
admission, selection, or compatibility authority.

## Preserved selected paths

```text
one SELECTED_PROFILE in the private regression benchmark
selected core constants 64 / 64 / 262144 / 8388608
selected production profile-ID function
selected K64/F64 and DIR256K frozen identities
--fast-* selected regression CLI
--fixed-radix-acceptance-* selected regression CLI
proof/COW/range/reopen/receipt/publication/Q/W/D tests
```

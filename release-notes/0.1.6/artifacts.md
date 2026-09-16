# v0.1.6 artifacts and checksums

> **Status:** Source-only v0.1.6 artifact record.
> [Published assets](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6).

The release follows the source-only v0.1.3–v0.1.5 distribution model. The
`v0.1.6` tag resolves to the final release-documentation commit, and the tagged
Git tree is the only source of the published assets.

| Asset | Contents |
|---|---|
| `layerfs-0.1.6.tar.gz` | Tagged source under `layerfs-0.1.6/` (every tracked file) |
| `layerfs-0.1.6.zip` | The same tagged source |
| `layerfs-0.1.6-benchmark-data.tar.gz` | The tracked evidence bundle: `docs/roadmap/0.1/0.1.6/**`, `release-notes/0.1.5/**`, the benchmark family registry (`benchmark/fs-bench-pro/families/**`), the benchmark rules, `docs/versioned/0.1.6/**`, `docs/releases/v0.1.6/**` and `release-notes/0.1.6/**` |
| `Cargo.lock` | Tagged root lockfile |
| `LICENSE` | Tagged license |
| `SHA256SUMS` | Actual SHA-256 checksums of the preceding five assets |

GitHub's automatically generated source archives are additional
platform-generated downloads and are not part of this list.

**What the evidence bundle does and does not contain.** The v0.1.6 campaign
receipts (raw `perf.jsonl` and `verification.json` files) live under the
untracked local evidence roots `benchmark-results/v016/final3-seed1/` and
`benchmark-results/v016/final3-ext/`; they are cited by path from the tracked
matrices and reports and are deliberately **not** part of the archive. The bundle
therefore ships the complete tracked report set — including every per-case value,
disposition, declared allowance, receipt path and identity — but not the
untracked raw bytes, and not the 196-cell #152 receipt tree either. That
limitation is stated here rather than implied.

## Published checksums

<!-- CHECKSUMS:BEGIN -->
Resolved from the annotated tag object `dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79`
at commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` and built from that tree with
`prepare_artifacts.py` (4921 tracked source members, 180 evidence members, set
validation PASS):

```text
7ab7538946130d1dcd19d6d258ba1db966e9d0b51fccc3307ad8e0cd32805aab  layerfs-0.1.6.tar.gz
c77ce44066e8786cc93ed09927a487ac7b73eb08f3ffa5561714d118270b83bb  layerfs-0.1.6.zip
4932d3ee85c9c835bedfcd97786f9b0e47fc1760efa7369164b5d2db8c473c1e  layerfs-0.1.6-benchmark-data.tar.gz
210a6c8f96899af557fed53a917f5a3446acf3a73ac2bd93257dd40a3d2d63a1  Cargo.lock
e20a92efe4b92c0460bd0c475395166e37b6450ed410af155fa0ed99d479f676  LICENSE
```

The published `SHA256SUMS` asset was downloaded again from the release and
compares byte-identical to this set. `layerfs-0.1.6.tar.gz` is 64,308,867 B,
`layerfs-0.1.6.zip` 70,411,650 B and `layerfs-0.1.6-benchmark-data.tar.gz`
563,579 B.
<!-- CHECKSUMS:END -->

## Publication sequence

1. Create the annotated `v0.1.6` tag on the final release-documentation commit
   and record the resolved tag and commit IDs here and in
   [release-evidence.json](release-evidence.json). Never move an existing tag.
2. Prepare the six assets from the tagged Git tree with
   `release-notes/0.1.6/prepare_artifacts.py`, which archives tracked content only
   (untracked files are excluded), verifies that every required evidence scope is
   tracked at that commit, writes `SHA256SUMS` and re-validates the complete set.
3. Attach the six assets to the GitHub release with the
   [announcement](github-release.md), keeping the Developer Preview status, the
   unchanged schema-10 Store format, the one-worker cost, the waived cold-Init
   target, the declared allowances and every measured regression visible.
4. Copy the resulting checksums into this file. That update is a documentation
   commit after the tag; the tag tree itself differs from the measured product
   revision only by documentation and the version bump.

No crates.io publication, prebuilt executable or public runtime image is part of
this release. Local benchmark binaries and images remain under their original
seals and are not relabeled as release binaries.

## Reproducible preparation

Run `python3 release-notes/0.1.6/prepare_artifacts.py FRESH_OUTPUT` to prepare the
six assets from the actual `refs/tags/v0.1.6`; an absent tag or a commit whose
workspace version is not `0.1.6` is rejected. Repeat with `--verify` to validate
an existing directory without writing. `--candidate --ref COMMIT` allows a clearly
labeled untagged run for review; candidate assets are never published.

`release-notes/0.1.6/check_artifact_helper.py` is a self-contained fixture check
for that helper: missing-tag rejection, six-asset construction, overwrite
refusal, untracked-file exclusion, and checksum corruption of `SHA256SUMS` and of
a standalone asset. It was executed in this closure and printed
`artifact helper fixture check: PASS`; it writes only under a temporary
directory and does not touch the checkout.

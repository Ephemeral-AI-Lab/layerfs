# Stage 5 remediation evidence - 2026-09-17 (UTC)

Started at c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6 (branch main).

This directory is scratch-free: every log below was produced by the binary it
names, on the commit recorded next to it. The reviewer's retained evidence
under ../stages-1-5-review-20260917T160000Z/ was copied, never edited.

## diagnostics/

A copy of the reviewer's four-binary diagnostic client with relative crate paths,
so the before/after pair is the same client against two trees.

  diagnostics/stage5-diagnostics/        the copied client (public API only)
  diagnostics/*-before.log               before any change (HEAD c99a8d9f9)
  diagnostics/*-after-wp1.log            after WP1 (HEAD a1601b92f)

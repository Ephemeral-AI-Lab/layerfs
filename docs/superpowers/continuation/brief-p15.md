# P15 — npm preparation ONLY (no publication)

Issue: layerfs-connect#66. Parent: layerfs-connect#14.

## Scope boundary (read first)

This brief covers PREPARATION ONLY: making the @layerfs/*
packages publishable (metadata, versioning, build, README,
license, files list, pre-publish checks). PUBLICATION to npm
is NOT authorized by this brief and needs a separate,
explicit user approval. Do not publish, do not run any
publish command, do not create any npm token or account.

## Preparation checklist

- Package metadata for each @layerfs/* package (name,
  version, license, repository, files) reviewed against the
  repo's actual contents — no invented install path where
  upstream wrote none.
- Versioning scheme decided and documented (how the next
  version is chosen; what counts as breaking for each
  package).
- Build + pre-publish check script that runs green locally
  (pack + install-from-tarball smoke test, no registry
  writes).
- README accuracy pass: every command in every README must
  have been run, or be removed.

## Report

A preparation report: what is ready, what is still missing,
and the exact command the user would approve to publish.
No registry interaction of any kind.

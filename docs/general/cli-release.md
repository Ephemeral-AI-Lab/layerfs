# Publishing the `layerfs` CLI package to npm

> **Status:** Current general guide. Nothing here was published while it was
> written; publishing, logging in and rollback are the owner's steps.

The package is `api/cli` (`name: "@layerfs/cli"`). It ships the native `layerfs` binary and
is restricted to **linux / x64 / glibc >= 2.39** by `os`, `cpu` and `libc` in
its `package.json`. The first publish is a prerelease, `2.0.0-rc.1`, under the `rc` tag. npm also creates
`latest` on a package's first publish even when `--tag` is given, so both
tags point at the prerelease; `2.0.0` itself stays free until a stable
version is published.

## What the tarball contains

Four files, and nothing else:

```text
LICENSE
README.md
bin/layerfs
package.json
```

`bin/layerfs` is a release build for linux/x64, linked only against standard system libraries.
The package is MIT licensed, requires Node 22 or later, and provides the `layerfs` executable.

## 1. Build the binary and pack

In the release tree, in WSL:

```bash
node api/scripts/build-cli-dist.mjs /root/lfs-clidist/out
cp target/release/layerfs api/cli/bin/layerfs
chmod 755 api/cli/bin/layerfs
cp LICENSE api/cli/LICENSE
(cd api/cli && npm pack --pack-destination /root/lfs-clidist/out)
```

`prepack` refuses to pack when `bin/layerfs` is missing. The file produced is
`layerfs-cli-2.0.0-rc.1.tgz`.

## 2. Dry run and publish

```bash
cd api/cli
npm publish --dry-run --tag rc --access public
```

When ready to publish:

```bash
npm publish --tag rc --access public
```

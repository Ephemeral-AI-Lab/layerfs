# Publishing the `layerfs` Node package to npm

> **Status:** Current general guide. Nothing here was published while it was
> written; publishing, logging in and rollback are the owner's steps.

The package is `api/node` (`name: "@layerfs/node"`). It ships one native addon and
is restricted to **linux / x64 / glibc >= 2.39** by `os`, `cpu` and `libc` in
its `package.json`. There is no per-platform prebuild scheme. The first
publish is a prerelease, `2.0.0-rc.1`, under the `rc` tag. npm also creates
`latest` on a package's first publish even when `--tag` is given, so both
tags point at the prerelease; `2.0.0` itself stays free until a stable
version is published.

The name and tag follow the earlier LayerFS packages: `@layerfs/agent`, `@layerfs/cli`,
`@layerfs/backend` and `@layerfs/runtime` were published as `1.1.0-rc.1` on 2026-11-01 under
the `@layerfs` scope and the `rc` tag. The unscoped name `layerfs` is not available: npm refuses
it (403, too similar to the existing package `layer-fs`).

No token, password or `.npmrc` with a token belong in this repository, a
command line or a log. `npm login` writes the credential to the user's npm
configuration; keep it there.

## What the tarball contains

Nine files, and nothing else:

```text
LICENSE
README.md
layerfs.node
package.json
src/errors.js
src/index.d.ts
src/index.js
src/native.js
src/values.js
```

`layerfs.node` is a release build for linux/x64, linked only against
`libgcc_s`, `libm` and `libc`, with `GLIBC_2.39` as its highest symbol
version. `libc` tells npm the family (glibc or musl), not the version: a
linux x64 host with glibc older than 2.39 installs the package and then fails
to load the addon, because npm has no field for that floor. The package is MIT
licensed, requires Node 22 or later, and is an ES module.

## Before publishing

From the release tree, on linux x64 (the WSL Ubuntu host is suitable):

```bash
git status --short                 # empty
git rev-parse HEAD                 # the reviewed commit
git log -1 --format='%H %s'
node --version                     # >= 22
npm --version                      # the npm that will publish
```

From the terminal that will publish (the owner's Windows terminal):

```bash
npm whoami                         # the expected account
npm config get registry            # https://registry.npmjs.org/
npm view @layerfs/node version           # E404 before the first publish
```

`E404` for `layerfs` means the name is free. `npm view @layerfs/node-mcp` answers
`0.1.0`, a different package; do not confuse the two. If `npm whoami` fails,
log in in that terminal; do not paste a token into a command or a file here.

The npm page's repository link follows `repository.directory` on the default
branch `main`; `api/node` is there only after 2.0 merges, so that one
generated link 404s until then. The README's own docs links are pinned to
commit `716197c4` and resolve now.

## 1. Build the addon in release mode

In the release tree, in WSL:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.npm-global/bin:$PATH"
node api/scripts/build-native.mjs
```

This runs `cargo +1.85.1 build --release --locked --offline -p layerfs-node`
and copies the artifact to `api/node/layerfs.node`. Expect:

```text
Finished `release` profile [optimized] target(s)
Built linux/x64: <tree>/api/node/layerfs.node
```

Check it before packing:

```bash
ls -l api/node/layerfs.node
objdump -T api/node/layerfs.node | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1
ldd api/node/layerfs.node
```

Expect about 16.6 MB, `GLIBC_2.39`, and no library beyond `libgcc_s.so.1`,
`libm.so.6`, `libc.so.6` and the loader. If the highest symbol is newer than
2.39, the artifact needs that newer glibc: build on an older host or change
the documented floor before publishing.

## 2. Pack and check the bytes

Run this from the repository root:

```bash
OUT=${OUT:-$HOME/layerfs-npm-out}
mkdir -p "$OUT"
(cd api/node && npm pack --pack-destination "$OUT")
```

`npm pack` reads the package in the current directory. `--prefix` does not
apply to `pack`, so `npm --prefix api/node pack` fails with
`ENOENT .../package.json`. `prepack` refuses to pack when `api/node/layerfs.node` is missing,
so a tarball without its addon cannot be produced this way. The file is
`layerfs-node-2.0.0-rc.1.tgz`. Check the file list, the size and the hash:

```bash
cd "$OUT"
tar -tzf layerfs-node-2.0.0-rc.1.tgz
sha256sum layerfs-node-2.0.0-rc.1.tgz
```

The list must be the nine files above, each under `package/`. Any
`package/test/`, `package/examples/`, `package/.npmrc`, source map or
`node_modules/` entry means stop. Record the size, the sha256 and the commit.
On Windows the same hash is
`certutil -hashfile layerfs-node-2.0.0-rc.1.tgz SHA256`.

`npm pack --dry-run --json` in `api/node` prints the same list without writing
a file; `npm publish --dry-run --tag rc --access public` in `api/node`
prints the tarball contents, name, version and tag and changes nothing.

## 3. Publish

From `api/node`, in the terminal that is logged in:

```bash
cd api/node
npm publish --tag rc --access public
```

If npm asks for a one-time password, enter it at the prompt. Expect
`+ @layerfs/node@2.0.0-rc.1`. The command runs `prepack` again; a missing addon
aborts before upload.

When the logged-in npm is the Windows one and the tree lives only in WSL,
publish the tarball that was checked instead of packing again on Windows:

```powershell
npm publish E:\path\to\layerfs-node-2.0.0-rc.1.tgz --tag rc --access public
```

That uploads exactly the bytes whose sha256 was recorded. Either way the tag
is `rc` and the access is public, but npm also sets `latest` on the first
publish (see above). Publishing a name and version that already exists fails;
never reuse a published version.

`--provenance` is not used. npm provenance requires a supported CI provider
with an OIDC identity (for example GitHub Actions); a publish from a local
terminal cannot produce a provenance statement. The npm page will show none,
and that is expected.

## 4. Verify after publishing

```bash
npm view @layerfs/node@rc version dist.tarball dist.integrity
npm view @layerfs/node dist-tags
npm view @layerfs/node@rc os cpu libc
```

Expect `2.0.0-rc.1`, a `dist-tags` map with `rc` and `latest` both at
`2.0.0-rc.1` (npm sets `latest` on the first publish), and the three platform
fields. Then install from the registry on a clean linux x64 host, outside the
repository:

```bash
mkdir -p "$HOME/layerfs-npm-check" && cd "$HOME/layerfs-npm-check"
npm init -y
npm install @layerfs/node@rc
node -e "import('@layerfs/node').then((m) => console.log(m.table.length, m.families.join(',')))"
```

Expect `61 project,branch,commit,file,sandbox,workspace,machine,store,backup`.
Run the README example (`demo.mjs`) there too; it must exit 0 and print
`# notes`. The temporary directory must be ext4, xfs or btrfs, not tmpfs.

`npm install @layerfs/node` also installs the rc while `latest` points at it; use
`@layerfs/node@rc` to ask for the prerelease explicitly. On macOS, Windows, Linux
arm64 or musl the install is refused with `EBADPLATFORM`. On linux x64 with
glibc older than 2.39 it installs and the addon fails to load; npm has no
field for the glibc version.

## Rollback

A published version is effectively immutable. In order of preference:

1. **Deprecate it.**
   `npm deprecate @layerfs/node@2.0.0-rc.1 "broken: <reason>; use 2.0.0-rc.2"`
   The install still succeeds but prints the message. Undo with
   `npm deprecate @layerfs/node@2.0.0-rc.1 ""`.
2. **Move the tag.**
   `npm dist-tag rm @layerfs/node rc` removes `rc`; `npm dist-tag add
   @layerfs/node@2.0.0-rc.2 rc` points it at a later version. `latest` cannot be
   removed (npm refuses with 400); move it to a later stable version with
   `npm dist-tag add @layerfs/node@2.0.0 latest`. Exact-version installs still
   reach the old tarball.
3. **Unpublish under npm's policy.**
   Within 72 hours of the first publish, a new package with no dependents can
   be unpublished. After 72 hours, self-service still works when no package
   depends on it, it had fewer than 300 downloads in the last week, and it has
   one owner/maintainer. `npm unpublish @layerfs/node@2.0.0-rc.1` deletes that
   version and its tarball; npm may ask for `--force` because this removes
   the only version. That also removes the package entry, and the name cannot
   be published again for 24 hours; the `name@version` can never be reused.
   `npm unpublish @layerfs/node --force` removes the root entry and every version:
   do not use it as a rollback.
4. **Nothing recalls a downloaded tarball.** Deprecation is advisory; users
   who already installed keep it.

Re-check npm's current [unpublish policy](https://docs.npmjs.com/policies/unpublish)
before relying on the window.

## What this package does not cover

- macOS, Windows, Linux arm64 and musl: no prebuilds; npm refuses them.
- Linux x64 with glibc older than 2.39: installs, then the addon fails to
  load; npm can check the libc family but not its version.
- Durable Stores: disabled by the owner; only Disposable/WAL runs.
- PostgreSQL and TLS: not in the default build; such a call is refused
  `Unsupported`. They need `--features postgresql,tls` at build time.
- The `layerfs` command and the Python wheel: separate artifacts, not in this
  tarball; the Python package is not on PyPI.
- An MCP server: not built.
- VM machine kinds: the package does not ship the sealed Linux bundle or a
  daemon; those are expected in the host's cache.
- Linux `native` Sandboxes: namespaces need root; the package installs no
  service.
- A provenance statement: impossible from a local publish, as above.

# layerfs

The LayerFS SDK for Node: nine families — `project`, `branch`, `commit`,
`file`, `sandbox`, `workspace`, `machine`, `store`, `backup` — over one call
table of 61 calls into one native library. Each function makes one call; it
does not retry, replay or commit on its own. Results are plain objects with
snake_case keys, whole numbers are exact (`bigint` beyond
`Number.MAX_SAFE_INTEGER`), bytes are `Buffer`s, and the TypeScript typings
are generated from the call table.

## Install

```bash
npm install @layerfs/node@rc
```

`2.0.0-rc.1` is a prerelease under the `rc` tag. npm also creates `latest`
on a package's first publish, even with `--tag rc`, so both tags point at
this version until a stable version is published; `npm install @layerfs/node` then
also installs the prerelease.

Node 22 or later. The package is an ES module. TypeScript users need
`@types/node` 22 or later: `src/index.d.ts` references Node's types.

## Platform

The tarball contains one native addon, built on Linux x64; its highest
required glibc symbol is `GLIBC_2.39`.

- **linux / x64 / glibc >= 2.39 only.** `os`, `cpu` and `libc` make npm
  refuse another OS, CPU or C library family with `EBADPLATFORM` instead of
  installing a package that cannot load. npm cannot check the glibc
  *version*: a glibc older than 2.39 still installs, then fails to load the
  addon.
- macOS, Windows, Linux arm64 and musl (Alpine) need a per-platform prebuild
  scheme, which does not exist yet. This version does not support them.
- The addon is `layerfs.node` beside `src/`, or the file `LAYERFS_NATIVE`
  names; nothing else is tried. A missing addon is an error that says how to
  build it from the repository.

## Example

```js
// demo.mjs
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { client, close } from "@layerfs/node";

const root = mkdtempSync(join(tmpdir(), "layerfs-"));
const folder = join(root, "app");
const home = join(root, "home");
mkdirSync(folder);
writeFileSync(join(folder, "README.md"), "# app\n");

const lfs = client({ home });
try {
  await lfs.store.create("shop");
  await lfs.project.init("shop/app", folder);
  await lfs.branch.fork("shop/app", "agent", "main");
  await lfs.file.write("shop/app", "agent", "/notes.md", "# notes\n", { message: "add notes" });
  console.log((await lfs.file.read("shop/app", "agent", "/notes.md")).text.trim());
} finally {
  close();
  rmSync(root, { recursive: true, force: true });
}
```

Save it as `demo.mjs` and run `node demo.mjs`; it prints `# notes`. It uses a
throwaway home and no machine or Sandbox. The temporary directory must be on
ext4, xfs or btrfs (a Store needs SQLite locks); on Linux set `TMPDIR` to such
a directory if `/tmp` is not one.

## Errors and uncertain outcomes

- A refused call throws `LayerFSError` with a stable `code`, a `message` and
  the library's structured `detail`; check `code`, not the text.
- Every call is one attempt. There is no automatic retry, busy handler or
  replay, and a lost reply does not undo a mutation that was already
  published: read the state again.
- Call `close()` before the process exits to release the Stores this process
  holds open; a later call opens them again.

## Docs

- [LayerFS 2.0 manual](https://github.com/Ephemeral-AI-Lab/layerfs/blob/716197c4999fb087aa0fb17bac5ceb48b0b556ae/docs/versioned/2.0.0/README.md)
- [The 61 calls](https://github.com/Ephemeral-AI-Lab/layerfs/blob/716197c4999fb087aa0fb17bac5ceb48b0b556ae/docs/versioned/2.0.0/calls.md)
- [Node and Python SDK](https://github.com/Ephemeral-AI-Lab/layerfs/blob/716197c4999fb087aa0fb17bac5ceb48b0b556ae/docs/versioned/2.0.0/sdk.md)
- [Limitations](https://github.com/Ephemeral-AI-Lab/layerfs/blob/716197c4999fb087aa0fb17bac5ceb48b0b556ae/docs/versioned/2.0.0/limitations.md)
- [The JavaScript package guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/716197c4999fb087aa0fb17bac5ceb48b0b556ae/api/README.md)
- [Source and issues](https://github.com/Ephemeral-AI-Lab/layerfs)

MIT licensed. The tarball ships `LICENSE`, `README.md`, `package.json`,
`src/*.js`, `src/index.d.ts` and the addon.

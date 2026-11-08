# @layerfs/cli

The LayerFS command-line interface: every call of the table as a shell command, with JSON and human-readable output.

## Install

```bash
npm install -g @layerfs/cli@rc
```

`2.0.0-rc.1` is a prerelease under the `rc` tag. npm also creates `latest` on a package's first publish, even with `--tag rc`, so both tags point at this version until a stable version is published.

Node 22 or later.

## Platform

The package ships a native binary for Linux x64 with glibc >= 2.39.

- **linux / x64 / glibc >= 2.39 only.** `os`, `cpu` and `libc` ensure npm refuses unsupported platforms with `EBADPLATFORM`.
- macOS, Windows, Linux arm64 and musl are not supported by this build.

## Usage

```bash
npx layerfs help
npx layerfs --version
```

# LayerFS 1.0.0: official release

> **Status:** Current planning checklist; no release candidate exists.

## Goal

1.0.0 is LayerFS's first official release, after the 0.1.x Developer Previews.
It starts from the [0.1.7 release](../../versioned/0.1.7/README.md): the
rewritten core, the shared Store opened directly by each daemon, per-Workspace
overlays, the JavaScript SDK and the terminal UI.

The owner discarded the earlier v0.2.0 and v0.3.0 plans on 2026-10-15. Their
documents are kept as archives in [`../0.2/`](../0.2/README.md) and are not
inputs to 1.0.0 unless a 1.0.0 issue picks them up explicitly.

## Workstreams

| Issue | Workstream | Done when |
| --- | --- | --- |
| [#330](https://github.com/Ephemeral-AI-Lab/layerfs/issues/330) | Reorganize the documentation into one product manual and an archive | Inventory and layout agreed; files moved without loss; 1.0.0 manual written; links and examples checked |
| [#331](https://github.com/Ephemeral-AI-Lab/layerfs/issues/331) | Demos a newcomer can run, on the live terminal UI | One-command start; selected stories pass live at the release candidate; walkthrough and manual section published |
| [#332](https://github.com/Ephemeral-AI-Lab/layerfs/issues/332) | Benchmark campaign preparation | Specification frozen; harness dry run per family on `main`; environment and report template ready |
| [#333](https://github.com/Ephemeral-AI-Lab/layerfs/issues/333) | Troubleshooting telemetry: SDK call trace, phase timing, observed daemon diagnostics | Trace, Commit/mount/merge phase timing and joined daemon records on and off tested; docs and overhead recorded |
| [#334](https://github.com/Ephemeral-AI-Lab/layerfs/issues/334) | SDK skill verified live; an e-commerce project built on LayerFS by a bug-fixing agent, then by a fresh agent | Every skill example captured live; both builds finished or blockers recorded; storage and per-operation speed results written ([plan](../../../core/docs/issues/334/README.md)) |

## Integrations under review

- [Rust rutis plugin: in-process and remote architectures](rutis-rust-plugin.md):
  proposal for a Rust-first LayerFS plugin; not a released contract.

## Decisions still needed from the owner

- [ ] **Compatibility promise.** 1.0.0 follows stable semantic versioning (see
      the [release policy](../../general/release-policy.md)). Decide which
      surfaces it promises: the 29-method SDK, the Store format, the daemon
      protocol, the operator surface (currently internal).
- [ ] **Store profile.** Decide whether the Durable profile stays disabled for
      1.0.0 or is reauthorized and qualified.
- [ ] **Release gates.** Decide which release-policy gates must pass with no
      waiver for an official release, for example the whole workspace test
      suite and the FUSE and container gates on a capable host.
- [ ] **Distribution.** Decide whether the release stays source-only or
      publishes an npm package, prebuilt daemon or runtime image.
- [ ] **Carried limits.** Decide which 0.1.7 limits must be fixed for 1.0.0:
      restart recovery, zombie reaping, replaced-inode FUSE behaviour, B7 and
      A9.

## Not yet planned

Optimization and benchmark issues left open after 0.1.7 (#313, #316–#325, #241
and the FUSE and storage follow-ups) are triaged through #332. Each one either
joins 1.0.0 or is explicitly left out.

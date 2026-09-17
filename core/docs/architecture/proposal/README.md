# Replacement-core proposals

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Design proposals for the `core/` product. These are **not** descriptions of
shipped behaviour and must not be read as such — the descriptive set is one level
up in [`core/docs/architecture/`](../README.md).

## Why a separate folder

The parent set is source-backed description: every claim is read from code at a
recorded pin. A proposal is a different kind of document — it states what *should*
be built, and it is wrong to mix the two because a proposal read as a description
becomes an unverified claim.

So proposals live here, and each one carries an explicit table separating:

| Kind | Meaning |
| --- | --- |
| **holds today** | read from source; true of the current tree |
| **proposed** | does not exist; a design to be argued with |
| **open — required** | a prerequisite for something else in the proposal |
| **deferred** | explicitly out of scope, with the issue that owns it |

## Contents

| Document | Covers |
| --- | --- |
| [`init-commit-and-concurrency.md`](init-commit-and-concurrency.md) | The init-namespace and workspace-commit pipelines against the Store: every DB operation by phase, what is shared versus private, three concurrency cases with diagrams, the three prerequisite races, why a stale merge is rejected rather than queued, and the races-versus-conflicts boundary |

## Conventions

- Every proposal records the source pin it was written against and marks which
  parts are read from source.
- **No proposal in this folder is measured.** Claims about behaviour under
  concurrency are structural arguments from the current code, not test results.
- A proposal that is implemented moves into the descriptive set, and the entry
  here becomes a pointer — it is never silently re-dated.

## Keeping this current

A change under `crates/*/src` or `crates/*/sql` that invalidates a proposal's
reading of the code updates the proposal in the same commit. A proposal whose
premise no longer holds is **withdrawn explicitly**, with the reason recorded,
rather than left to rot.

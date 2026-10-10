# R8b repair and requalification completion

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**R8b CLOSED — NOT QUALIFIED.** Every selection registered in v3 has an
outcome. Eleven of twelve registered proofs passed, the full-byte mounted
fixture proof among them; one failed; eight of nine prerequisite gates are
still unmet, so no timing was attempted. R9b is closed separately as
[`NOT_EXECUTED — conditions unmet`](R9B-COMPLETION-20261010.md). The earlier
[R8](R8-COMPLETION-20261010.md) and [R9](R9-COMPLETION-20261010.md) records
keep their outcomes. Nothing is pushed or published; this is a closed
examination, not product acceptance.

## Frozen identity

| Item | Value |
| --- | --- |
| Input | `496bb5643` on local `main` |
| Registered source | `77a9b52dbe1e221954f8a2afbf7e0495a82b615e`, tree `3f90565f496b` |
| Product tree `core/crates` | `2e0d94c8835e` (input: `9a76077239b4`) |
| Registration | [v3](../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v3.json), SHA-256 `1d9ea498…6190`, committed in `b22fa088f` before the first registered invocation |
| Runtime and daemon | release builds from the registered source, receipts 027 and 028; Linux source hashes checked in the container (024) |
| Profiles | Global Store Disposable/WAL/OFF selected explicitly; Durable `NOT_RUN — disabled by owner until explicit reauthorization`; overlay MEMORY/OFF/EXCLUSIVE; `LAYERFS_CONSTRUCTION_WORKERS=1` |

Host Cargo `+1.85.1`, locked, repository ARM64 flags; pinned image
`sha256:378b799e…2cd6`. Locks and fuser provenance are identical to v2.

## What was repaired

| Item | Commit | Result |
| --- | --- | --- |
| Full-oracle timeout (047/053) | `4a920a3ee` | Diagnosed by count: a serial through-mount traversal projects to about 130 s against the 85 s stop. The comparator now observes with 8 bounded walkers; every assertion, input and stop is unchanged (lead decision L-1) |
| P-A, gate G01 | `c127b6d55`, `f0e1e2d84` | A create that leaves fewer unconsumed serials than the daemon's configured low-water makes one early reservation. Deployed value is 0, which makes none |
| P-B, gate G02 | `ea812d3f8` | A new Mount is refused `Capacity` at `mount:debt` while maintenance is stopped |
| Test tracks T-A to T-J | `24d3d3a9d`, `bdef1cfbb` | 30 external tests in 11 new files; no existing assertion removed or loosened |
| Runtime harnesses | `136a6ec72`, `ab7e25b89`, `ebfd560f2` | Confinement, held-mount Unmount, stream delivery and lifecycle controllers over the real SDK, Sandbox and release daemon |
| Content/storage harness | `77a9b52db` | Builds against the current Store API; no registered row executed |

Both product edits are demonstrated contract defect fixes. No dependency,
canonical format, negotiated profile, retry, thread, limit or lifetime changed.
An independent review of the two fixes found three misstatements, corrected in
`f0e1e2d84`; see [019](checks/r8b-requalification-20261010/019-product-review.md).

## Outcomes

Complete inventory: [046-outcomes.md](checks/r8b-requalification-20261010/046-outcomes.md)
and its [machine form](checks/r8b-requalification-20261010/045-outcomes.json).

Registered invocations, one attempt each:

| Selection | Outcome |
| --- | --- |
| `R8b-full-fixture-mounted` | **PASS**: 130,046 names, 103,108 regular files, 3,475,776,149 bytes, 0 differences; comparator 57.0 s under the 85 s stop; terminal Unmounted, Gone observed |
| `Q1-wide-host`, `Q1-wide-linux`, `Q1-host-handoff`, `Q1-shared-processes-linux` | **PASS** each |
| `R8b-confinement` | **PASS**: 84 protected routes refused, no inherited descriptor |
| `R8b-holder-cwd`, `-descriptor`, `-mapping` | **PASS** each: reversible `Busy`, sibling Unmounted and Gone |
| `R8b-holder-private` | **FAIL**: with a holder in a private user and mount namespace, the sibling's normal Unmount did not reply inside its 5 s stop ([diagnosis](checks/r8b-requalification-20261010/042-holder-private-diagnosis.md)) |
| `R8b-streams` | **PASS**: both streams exact at every size up to 64 MiB each |
| `R8b-lifecycle` | **PASS**: mount, external mutation, Commit, unmount, fresh mount, equal oracle |

Functional rows, 90: **54 PASS, 27 PARTIAL, 1 FAIL, 8 WITHDRAWN** (closed R8:
45, 35, 1, 8, and 1 NOT_RUN). Nine rows moved to PASS: R2-STEP-3, R2-STEP-7,
R4-2, R4-3, R4-9, R5-5, R5-9, R5-10 and the full fixture. FP-22-FS moved from
PARTIAL to FAIL. FP-30-Runtime moved from NOT_RUN to PARTIAL.

Timing rows, 282: all **NOT_RUN**, zero attempts, zero samples. E08 and E09
stay owner-excluded, the `P` arm stays unexecuted at the unchanged provenance
refusal, and A2 supplies no arm.

| Gate | Status | Why |
| --- | --- | --- |
| G01 low-water refill | PENDING OWNER | Mechanism built and proved; no deployed value |
| G02 debt admission | UNMET | Proved for a quarantined engine only; headroom and Status need rulings |
| G03 request maxima | UNMET | Flag half proved; READ and WRITE maxima have no observation source |
| G04 mapping headers | UNMET | Dirty-page Commit proved; WRITE header identity and held GETATTR are not observable |
| G05 runtime matrix | UNMET | One registered FAIL; backpressure and lost-result scopes not constructible or not run |
| G06 Commit failure matrix | UNMET | Two stages have no producer; reproduction R5-6a reported |
| G07 full-byte oracle | **MET** | Proof 033 |
| G08 counts | UNMET | No count selection; several quantities have no product observation |
| G09 resources | UNMET | Phase-boundary samples only; no bound exists |

## Failed attempts, all retained

| Attempt | What failed | Disposition |
| --- | --- | --- |
| `R8b-holder-private` (042), registered | Sibling Unmount reached its stop | FAIL kept; not rerun; container `71e27756ad4f` retained |
| Namespace holder (010), exploratory | Normal Unmount ended in retained teardown custody | The finding behind FP-22-FS; not repaired |
| Confinement probe (008), exploratory | The probe reported its own listing descriptor | Tool corrected; 011 then passed |
| Streams (014, 015), exploratory | Controller parsing and case design | Tool corrected; 016 then passed |
| `pb-attempt1` | The test's own staging was fenced behind a parked capture | Test restaged |
| `tc-attempt1`, `tc-attempt7` | Reproduction R5-6a fails against the product | Kept as an ignored test, reported |
| `te-attempt1` (host), `th-attempt1`, `ti-attempt1`, `ti-attempt2`, `tj-attempt1` | The tests' own bookkeeping or expectation | Tests corrected; final attempts pass. Commit `24d3d3a9d` says five earlier attempts failed in its tracks; the retained receipts show these four there |
| `r9a-attempt1` | Guard token matched a domain term | Tokens spelled as calls |
| Final suites (021, 025) | Two environment-gated tests on each side lack their inputs in a plain run | Registered with inputs and passed (034 to 037) |

Two shell slips ran no operation and left no receipt: an unsplit argument
array, and a format helper called without its package arguments.

## Reused evidence

- The sealed full fixture Store, manifest and inventory prepared on
  2026-10-09, by digest; each run used a fresh independent byte copy.
- The closed wide preparation of the earlier run, unchanged by digest.
- The functional and timing inventories 025 and 026 of the closed R8.
- The final host and Linux suites at the registered source, run once before
  the registration was written and cited rather than re-invoked.

No exploratory receipt was promoted to a registered outcome, and no earlier
sample or measurement was reused.

## Decisions that need owner review

Decisions taken without the owner are L-1 to L-7 in the
[plan](checks/r8b-requalification-20261010/01-deepest-file-plan.md), with the
corrections in its section 10. `PENDING OWNER`, 21 items: the seven inherited
(OWNER-1 to OWNER-7, unchanged) and OWNER-8 to OWNER-21, each with a
recommendation, in [046-outcomes.md](checks/r8b-requalification-20261010/046-outcomes.md#owner-questions).
The four that block the most rows:

1. **OWNER-8, namespaces.** May a Sandbox command create a mount namespace?
   One such command defeats normal Unmount for every Workspace of its daemon.
2. **OWNER-12, system-call fixture.** Six rows need an observation of the
   test's own system calls; the alternative is product telemetry.
3. **OWNER-10, low-water value.** P-1 stays PARTIAL until a number is named.
4. **OWNER-21, counts and resources.** G08 and G09 need observations and bounds
   that do not exist.

## Source accounting

Pinned `tools/production_loc.py` (SHA-256 `c0fe7f36…624adb`) over exact
first-parent and staged archives.

| Commit | Production LOC |
| --- | --- |
| `4a920a3ee`, `388503a77`, `136a6ec72`, `a8aa0b2b6`, `ab7e25b89`, `ebfd560f2`, `24d3d3a9d` | 144,675 -> 144,675 (delta +0) each |
| `c127b6d55` (P-A) | 144,675 -> 144,743 (delta +68) |
| `ea812d3f8` (P-B) | 144,743 -> 144,753 (delta +10) |
| `f0e1e2d84`, `02dd96b0b`, `bdef1cfbb`, `77a9b52db`, `b22fa088f` | 144,753 -> 144,753 (delta +0) each |

Active core 79,258 -> 79,336; root reference 65,417 unchanged; no relocation
or retirement. The commit carrying this record reports its own comparison.

## Resources

[Custody record](checks/r8b-requalification-20261010/048-resource-custody.txt).
Removed, all created in this run: 15 exited containers of passing or completed
runs and two volumes used only by passing selections. Retained from this run:
five exited containers of failed or uncertain attempts, the exploratory volume
`…2cd69f68db34`, the registered volume `…2aca347ad746`, and one temporary
directory of a worker's failed attempt. Not touched: the four running
containers, both earlier R8 volumes and their containers, every earlier exited
container, `deepseek-harness`, master volumes, the three R7 worktrees and the
untracked hand-over prompt.

## Final checks and what is not verified

At the registered source: fmt, boundary guard (674 files), host and Linux
Clippy with warnings denied, host suite (275 binaries, 1,213 passed), Linux
suite (275 binaries, 1,292 passed), tooling tests. `check_fuser_integrity.py`
still refuses the `r7-passthrough` manifest (OWNER-3, unchanged). There is no
CI.

Not verified:

- Any timing, cold-cache, storage-size or resource-bound claim; any `P` or `N`
  arm; Durable.
- The 27 PARTIAL rows' open scopes and gates G01 to G06, G08, G09.
- The reply the product would have sent to the sibling Unmount in 042.
- The early-refill arm that fails for a reason other than contention, and the
  low-water call site after an install failure.
- Any registered row of the ported content/storage harness, and four of its
  diagnostic test binaries.
- Maintenance stopped on an engine that is otherwise usable.

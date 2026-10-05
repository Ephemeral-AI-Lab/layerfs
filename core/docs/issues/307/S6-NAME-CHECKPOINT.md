# S6 name and non-file checkpoint

> **Status:** Implemented checkpoint after `cae3d43ed`, local main, 2026-10-06.
> S6 remains IN PROGRESS. Physical reservation/headroom and full resource/debt/
> device evidence remain required; this is not release or native qualification.

[Selected contract](S6-NAME-CUSTODY-CONTRACT.md) preceded code. Schema v12 adds
nonrecycled lookup owners and relative lower-binding name facts. Removed directory
and symlink state joins the independent orphan domain only after a semantically
checked non-root removal; canonical zero-count root metadata is not removal.
Lookup/read processing owners are independent and exact. Existing consumers survive
lookup release and logical close; last release wakes automatic cleanup.

Readlink now follows effective local layers and the retained original base root,
including captured input after install/failure. Captured-reader inode/dentry pages
are independently available through fixed keysets while that reader owns its sealed
generation. Name rewrites preserve the active inheritance bit; failure composition
advances it to the lower row's bit, dropping only redundant whiteouts and preserving
necessary ones. Known install requires no whole-name rewrite.

[Architecture](../../architecture/34-name-and-lookup-custody.md),
[identity](checks/s6-names/identity.json) and
[failure ledger](checks/s6-names/FAILURES.md) distinguish the implemented scope.
Actual public proofs cover 20 linked-symlink failure rounds, live/sealed target
reads, directory/symlink/root lookup owners, consumers beyond lookup release,
logical close, redundant/necessary whiteouts and captured metadata after install.
The real daemon proof uses a content-constructed canonical root and prepared actor
installs: six successes and six failures preserve removed directory attributes and
an inherited target; terminal cleanup follows the last request release.

macOS scoped tests pass: overlay37, Workspace32, daemon9 and SDK6 (84 total).
Linux ARM64 selected overlay/Workspace/daemon/SDK build and 78 tests pass. SDK global
provider cases execute on macOS and only compile on Linux. Scoped four-package
all-target Clippy with `-D warnings`, fmt, boundary559 and current-guide relative
links pass. Unchanged tool tests reuse the prior custody checkpoint's 26 passes.
Every test has an explicit <=120s ceiling after a `--no-run` build; none reached it.
Exact compile/fixture/lint/profile failures remain raw and append-only. Source review
and failure arithmetic are in the ledger; a stored fact's one owner round is a count
observation, not a numeric speedup. Linux compilation overlapped Mac daemon/SDK
proof and is declared interference; no wall/latency claim is made.

Actual compound/name/custody templates have indexed EXPLAIN and correlated runtime
profiles. Complete jobs retain equal work beside 128/1024/4096 unrelated rows. On
macOS the compound diagnostic has 34 statements/1298 VM steps/eight changed rows;
the 64-name window family has 870 VM steps (complete window 10 statements/1016 VM).
New hint projection/checking is included in those raw counts. Per-name composition
visits one name and indexed point state. No namespace mirror, foreground payload
fold, total file/name cap or failed-operation replay is introduced. Work and phase/
physical resource qualification remain distinct; MEMORY/OFF/page-quota proofs do
not satisfy actual device headroom. Native request/lookup/kernel mapping remains S8.

Production LOC: **150019 ->150533 (delta +514)**. Core (including excluded
predecessors) 84602 ->85116; root reference 65417 unchanged. No relocation or legacy
retirement is claimed. Counter: unchanged `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, exact
first-parent/staged archives of `core/crates` and `crates`; same nonblank/noncomment
product-src/shipped-SQL scope with tests/inline tests/docs/tools/harnesses/manifests/
builds excluded. Receipt: `core/target/cluster2-307/loc/s6-names-staged.json`, recomputed
after final staging and confirmed against the committed tree. Growth supplies exact
ownership/facts and their validation; it is not a performance or simplification claim.

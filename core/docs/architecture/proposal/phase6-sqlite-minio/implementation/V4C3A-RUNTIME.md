# V4c3a live catalog/source runtime checkpoint

> Status: Research; informative and not a product contract.
> PARTIAL: owning SQL/filesystem checks pass, live five-root proof unrun.

Parent ca2ee13ab4bd45608b2695736073c1e23dc989a6. [Frozen contract](V4C3A-SPEC.md),
[external evidence](v4c3a-native-checks/). Experimental runtime only; unchanged
current512population/serial/operation and256handles, one producer/single Branch.

Live name rows now have independent monotonic cookies allocated in the same SQL
transaction. Names(parent,cookie) index replaces full directory/name vectors and
ordinal replay. Dot/dotdot1/2, rows>=3, deleted positions never reused; rename
preserves source cookie. Canonical construction still consumes canonical name
order. Actual handle/ino/type/range checks precede64row/16KiB owned payload pages.
FUSE fills real kernel replies from those windows, at most8pages/512rows percallback,
then resumes at the last accepted cookie. No snapshot promise or lifetime cookie
registry. SQL allocation failure/cookie exhaustion refuses before node/name/serial
consumption. Reply/provider allocation and physical qualification remain separate.

Exact extent INSERT/DELETE triggers update source refs and an indexed retirement
queue in the same real SQLite transaction. Revived split source cancels its zero-ref
queue entry. Extent source changes refuse; source foreign keys now enforce actual
ownership. After known mutation success, at most64zero-ref files retire per callback.
Known immutable install clears captured extents/edits then drains bounded windows.
Reads/construction hold the same engine owner; unlinked open victims preserve their
live references. Unknown publication skips installation and retains captured source
references. No historical/source-population scan or durability expansion. New fixed
observations describe actual pages/rows/capacities and source retirements/pending
existence; they are not fake hooks or physical peak memory evidence.

Retirement file/SQL failure returns explicit error, refuses later writes/construction
before effects, preserves readable live bytes and returns postpublication install
uncertainty with pending state retained. Accepted logical writes
remain retained and source owner quarantined after retirement failure; no resend/fallback or
cleanup PASS. External missing zero-ref file proof shows the failure explicitly
and preserved live bytes. Full recovery/partial-source-creation/orphan-inode/lookup
retirement capabilities remain unqualified; current source creation rollback custody
is not promoted to a stronger crash/Unknown guarantee.

Owning commands from root:

- cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked
  --test directory --test source_retirement --test engine --test rename
  --test construction:18initial PASS; subsequently3source and7construction checks
  PASS after adding real failure quarantine,19unique owning checks in total. Actual SQL/files/immutable C1 fixture; exact pages,
  seeks/deleted cookie/rename/type/handle/highwater refusal, real query plan, long
  names, overwrite split/truncate/regrow/open victim,64retirement batch and explicit
  missing-provider failure; preserved large immediate edit/old root/legacy namespace.
- clippy with the same locked manifest --all-targets -- -D warnings:PASS.
- fmt --check:PASS; native build --release and zigbuild --release --target
  aarch64-unknown-linux-musl:PASS. Root ARMv8AEAD flags retained.
- Python generic case preparation compile PASS; closed original input preserved.

Initial compile failed because a new external test used unwrap_err on Node without
Debug. Corrected the external assertion, leaving runtime unchanged for that fix;
initial failures retained. No test body passed in the compile-failed attempt, no
unchanged passing suite/sample replay. Unchanged C1/storage/history/session/index
proofs retained by scope. Review added a real source-failure quarantine after identifying an otherwise
possible later retirement attempt; covering source/construction tests and Linux
postpublication custody build retained. Full unchanged Core suites/examples/boundary/LinuxClippy
unrun for experimental-only source. No CI/preflight or physical admission claim.

Generic case phase6-live-catalog128-v1 preserves original full128first3steps and
appends64ordinary overwrites/real find enumeration, then no-local-source-file and
complete128name observation before clean UpToDate Commit. Five independent full
expected manifests, actual C1/C2/MinIO/C5/known installation and separate readonly
five-root/history proof. Case SHA25665a6b47f683244fd18003cefd2f0109ba95ad4badb05dc7b0bfb5d6bc55b971b,
source acquisition pinned. No new family runner/daemon recognizer or public Init
substitute. Next one frozen source15s performance/9.5s proof, cacheINELIGIBLE.

Full goal remains active. Next V4c3b paged reservations, explicit population/operation
admission and inherited import/mount with short bounded SQL transactions; avoid
MEMORYjournal growth from population-sized transactions before removing limits.
Complete orphan/source lookup retirement, remaining syscalls/seven families/complete
DeepSeek/Unknown/concurrency/canonical/physical resource proofs remain open.

# #273 ordered follow-up: token diagnostics and lost checked release

> **Status:** functional-only checkpoint at clean source
> `e68904ce47f26de69674229e2d8c22a4eee24762`. Product source is still
> `f00644479a9b7dfe0b74e02438eed6d60e30dc0a`. This docs-only report
> follows [the deadline repair](PREMERGE-DEADLINE-FIX-20260929.md); it neither
> re-dates the earlier receipts nor approves a PR, numeric comparison or
> release. The user's ordered request continues at the **known C1 / local C5
> failure with a held SDK lease** gate; later gates have not been promoted.

## 1. Capture the previously unprinted token refusal — DONE, old cause unknown

Test-only commit `233deef7095ff70ad4a5d36e7905c5a9662d40cf` preserves the
exact well-formed-unregistered-token `Denied` assertion. It now records the
actual typed result and includes that result in a future assertion failure,
without printing the token. The clean-source live route emitted
`VIEW_LEASE_TOKEN_DIAGNOSTIC case=unregistered expected=Denied
actual=Err(Failure(Failure { code: Denied, unknown: false, cleanup: None,
history: None }))` and passed with the owned immutable daemon image
`sha256:65122998e92b125d0ec0b10f9826be5c74f064800f57199dd02aa4e290cdc22d`.
Local-only `core/target/issue273-next-sdk-token-233-01/result.json` SHA256
`d8ff0a113478ae6f4fc106163a0211a0d6420dc85c8329fc3fa8e6705cf65f15`.
The older `c0ab3e5c8` FAIL did not print its unexpected code: **its exact
cause remains unknown**, not reconstructed from this later success.

## 2. Withhold a live SDK checked-release reply — PASS, precise custody

At `30b32c9e8` the original test-only relay **FAILED** before release: it
had not propagated client EOF to the daemon, leaving a stale single control
session and causing mount `Io`. The FAIL receipt remains local-only at
`core/target/issue273-next-sdk-uncertain-release-30b-01/result.json` SHA256
`9c212e99f8fc92857da44bc027414b4668d0c176a257b1fca35b417ec855f528`.
A labelled, dirty-source diagnostic after adding EOF propagation passed, but
is **not** promoted to clean-source evidence:
`core/target/issue273-next-sdk-release-diagnostic-31b-02/result.json` SHA256
`e28f1a10ed11e8f6d2b0a18b6e0524d898a99a8193d2b17e20f44e3f8a9b960a`.
The intermediate diagnostic FAIL is retained too.

The prospective repaired test-only route is committed at
`e68904ce47f26de69674229e2d8c22a4eee24762`. Only the Rust test
process's PATH resolves the external Docker-port wrapper; it checks that the
container label is its own `view-release-loss-*`. All other Docker CLI calls
exec the real Docker binary. A transparent loopback TCP relay forwards the
unchanged authenticated production session and records record counts only,
not secrets or plaintext. After the public SDK test has pinned G1, proven G1
bytes, committed, produced live G2, and checked one held lease, it arms the
current session and calls `WorkspaceApi::release_view` **once**. The relay
forwards the session's checked Hello, receives the daemon's subsequent
terminal ciphertext, and withholds exactly that one reply. It does not fake
an outcome. No test-only product hook, retry, timeout, quota or worker change.

At the **clean** `e68904ce4` source this route **PASSed**. The actual SDK
release result is `Failure(Unknown, unknown=true)`, *not* `Completed`; a
separate read-only `view_status` finds the remote token `Denied` (the daemon
had in fact retired it), exact G2 remains live, and the owned sandbox unmounts
and deletes normally. There was **one** withheld terminal record (139
ciphertext bytes), one SDK release attempt and no resend. This establishes
**outcome uncertainty at the client despite known remote retirement**, not
remote retained pin custody. If a different fault occurs *before* remote
retirement, its custody still requires its own observation. Raw local-only
`core/target/issue273-next-sdk-uncertain-release-e689-01/result.json` SHA256
`40f4342626926ca415165b4b0aa08dca107b6b9fc29f25afc5571a4aadf26ae9`;
`relay.jsonl` is included in that directory's new `SHA256SUMS`. These
artifacts are not published GitHub evidence links.

Locked `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p
layerfs-sdk` on the clean source, warning-denying workspace/all-targets Clippy,
fmt check, product boundary (359 source files), its nine self-tests, Python
syntax checks and diff whitespace checks **PASS**. The ordinary macOS SDK
package test does not itself execute the live Docker route; the route above
was run separately with the immutable image and its registered external
driver. No all-12-package or release-profile suite was repeated at this
*test-only* source; see the earlier product-source checks in the deadline
handoff. The frozen #248 public functional gate at `31b49905` stays PASS at
that pinned source, numeric INELIGIBLE; no unchanged performance arm was
resampled just to re-date a test-only commit.

## 3. Known C1 / local C5 failure with a held SDK lease — NOT_PROVED

Two **non-product** host capability probes passed in owned, disposable
resources: a separate LAN-IP listener can bind the **same port** as the host
Service's 127.0.0.1 listener and is reachable from an owned Linux container,
and a static aarch64 helper can apply/restore a 2 KiB `RLIMIT_FSIZE` to PID 1
of an owned privileged disposable container. Their local-only results are
`core/target/issue273-next-c5-host-relay-capability-01/result.json` SHA256
`66c8de80be09db35a22ab5a7562e1517d395f26ef3fa2416d351b9eb766ab602`
and `core/target/issue273-next-c5-limit-capability-01/result.json` SHA256
`c8355d7e2f9cd3ee96e7286e25c5cfcbdf11610f1aebc79c83935d74b1f14b36`.
These are **not** an SDK Commit, held lease, physical C5 fault or proof that
the daemon survives an XFSZ signal. No LayerFS product limit was changed.

The remaining deterministic route must place an external pause **after the
host Service publishes the canonical Commit and before the daemon consumes
its reply**. Only then may it apply an owned-container file-size fault to the
local C5 reconciliation; it must restore that fault, inspect the exact typed
`KnownCommitLocalFailure`/`Reconcile`/known canonical outcome and the held
lease's actual old bytes. Authenticated service traffic is opaque, so a
pre-Commit quota fault or simple timed pause is not a substitute. No such
route was implemented or run here; **do not mark gate 3 PASS**.

## Still remaining in the requested order

4. Exhaust actual response Budget, do not change the 16 MiB sandbox limit or
   treat 33rd-lease count refusal as response Budget: **NOT_RUN**.
5. Bounded lowering without rewriting almost the entire 64 MiB fixture under
   unchanged quota and unchanged oracle: **FAIL retained / NOT_REPAIRED**.
6. Stage headroom completion escrow or explicit owner disposition preserving
   the 2 MiB quota: **FAIL retained / ruling PENDING** in
   [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5881598410).
7. Matched private/VM/backend/device/host cache state and phase-local resource
   observer or explicit numeric-profile ruling: **NOT_PROVED**. Docker Desktop's
   ineffective peak-reset observation is deferred only by #283. Frozen control
   NOT_RUN; historical nine numbers remain INELIGIBLE.

No PR was merged, no older receipt rewritten and no approval invented.
**Recommendation: NO MERGE / NO RELEASE ADMISSION.**

Per-commit production LOC (first parent vs exact staged tree, same
`python3 tools/production_loc.py --json --root <snapshot>` method over
`crates/` and `core/crates/`, excluding tests, harness tools and docs):

| Commit | Reference | Core | Combined | Delta |
| --- | ---: | ---: | ---: | ---: |
| `233deef70` token test | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | 0 |
| `30b32c9e8` release driver | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | 0 |
| `e68904ce4` EOF/diagnostic repair | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | 0 |
| This docs-only commit | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | 0 |

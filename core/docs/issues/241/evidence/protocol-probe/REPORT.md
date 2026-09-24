# #241 exact-size STATE/EDIT Linux kernel probe — 2026-09-24

## Decision

**The selected two-command carrier shape worked on the tested Linux/FUSE
kernel with unmodified `fuser` 0.18.0.** STATE
`_IOWR(0xf5,0x40,88)=0xc058f540` reached one callback with exactly 88
input bytes and `out_size=88`. The caller received all 88 response bytes,
including exact inode, 32-byte incarnation, generation, revision, length,
mtime seconds/nanoseconds and reserved zero. EDIT
`_IOW(0xf5,0x41,4192)=0x5060f541` reached the callback with exactly 4,192
input bytes and `out_size=0`; success returned with zero output bytes and the
caller's input buffer unchanged. The full 4 KiB inline replacement was
delivered byte for byte.

An EDIT from the initial stamp published once and, with real inode
invalidation, same-FD `fstat`, boundary bytes, EOF and a second STATE agreed
on revision 4, size 12,288 and newer mtime. A stale revision returned
`ESTALE` before publication and left STATE, size and mtime unchanged. These
are **PASS** for test-only carrier delivery and the measured mounted
semantics. No product Workspace/Store/SDK/Commit path was exercised.

The two induced uncertainty cases behaved conservatively. With notification
deliberately suppressed, EDIT returned zero and STATE reported the new stamp,
but same-FD `fstat` remained stale; the caller recorded **UNKNOWN** and did
not retry. With deliberate daemon death after a test-only accepted-state
write but before reply, the caller received `ECONNABORTED` (errno 103) and
recorded **UNKNOWN** without retry. These are fault probes, not spontaneous
provider failures. Real live-callback notifier failure and product uncertain
custody remain **INCOMPLETE**. No hard carrier blocker was found.

The earlier frozen 64/4,160-byte STATE/EDIT/ACK protocol was superseded
*before sampling* and all three of its cases remain explicitly
[NOT_RUN](EXPLORATORY_NOT_RUN.md). Its ACK and nonce are not selected product
ABI. The [exact final-candidate contract](FINAL_CANDIDATE_CONTRACT.md) was
frozen before these five attempts.

| Exact case | Gate result | Caller outcome | Publication count | Callback sequence |
| --- | --- | --- | ---: | --- |
| `state_exact` | PASS | Exact initial 88-byte STATE | 0 | STATE |
| `edit_state` | PASS | EDIT zero output; `fstat` 12,288/mtime+1, exact boundary/EOF; second STATE revision 4 | 1 | STATE, EDIT, STATE |
| `stale_refusal` | PASS | EDIT `ESTALE` 116; second STATE and `fstat` unchanged | 0 | STATE, EDIT, STATE |
| `suppressed_unknown` | PASS for induced stale detection | EDIT zero output; second STATE revised, same-FD `fstat` old 8,192/mtime; UNKNOWN | 1 | STATE, EDIT, STATE |
| `lost_reply_unknown` | PASS for test-only uncertain custody | EDIT `ECONNABORTED` 103, accepted artifact exact; UNKNOWN | 1 | STATE, EDIT |

## Custody and raw observations

Worktree `/Users/yifanxu/.codex/worktrees/issue241-protocol-probe/layerfs`,
branch `codex/issue241-protocol-probe`, base
`241096083f29fba74835581b4f3297c015924e8c`.
Exact contract commit `7c2a52770`; test-source commit
`e508f8d138ce1cb63a221a1b2b5435081f897978`.
Test source SHA-256 `ad2b809213453fff91f38d345b2515112384ca51b8c8e74a42d62f2e8ae1eab2`;
release binary SHA-256
`24469b9ba5ece62139ecb6fcb20d1517d5811eb11ddc6fa90ddcadd977d78da8`.
`core/Cargo.lock` SHA-256
`791a62b55b22d18ce827d49e500326d68a163384629e9c2865b893d8c5914efe`;
repository `.cargo/config.toml` SHA-256
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
Rustc 1.85.1, locked offline release `aarch64-unknown-linux-musl`,
privileged Alpine image
`sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`
and Linux `6.12.76-linuxkit` aarch64. [environment.json](environment.json)
records container, image, mount and limits.

Each [raw attempt](attempts/) has one fresh mounted virtual fixture, exact
Docker argv, source/binary hashes, stdout/stderr, mountinfo, caller log and
daemon event log. The event log preserves **full request hex** and the
daemon's intended reply bytes/errno. The caller log preserves ioctl rc/errno,
all 88 bytes actually received for STATE, and same-FD readback. No raw
`/dev/fuse` reply frame was captured; a daemon reply-method call is not a
delivery acknowledgement. [verify.py](verify.py) independently checks every
request byte, output byte, callback count, publication count and caller
decision from these raw files; [derived.json](derived.json) is its output.
[SHA256SUMS](SHA256SUMS) hashes retained evidence files.

| Case | Docker exit | Complete `docker exec` wall ns | STATE/EDIT callbacks | Observed error |
| --- | ---: | ---: | ---: | --- |
| `state_exact` | 0 | 95,473,417 | 1 / 0 | none |
| `edit_state` | 0 | 85,806,125 | 2 / 1 | none |
| `stale_refusal` | 0 | 89,681,792 | 2 / 1 | `ESTALE` 116 |
| `suppressed_unknown` | 0 | 81,549,167 | 2 / 1 | none; stale `fstat` |
| `lost_reply_unknown` | 0 | 68,959,958 | 1 / 1 | `ECONNABORTED` 103 |

Every case had zero automatic retry calls. The lost-reply artifact contains
all 12,288 expected virtual bytes, SHA-256
`361df1df6bf7d82ce7e5cb543b7da175c8d5920d371d1086c4c771d167425d71`.
It is a test-only file outside FUSE, not a durability or product publication
receipt. All mounts were absent after their cases; the owned container and
volume were removed after raw copy. See [cleanup.txt](cleanup.txt) and the
empty [post_cleanup_mountinfo](post_cleanup_mountinfo).

## Execution commands

From this worktree root, the locked release build was:

```sh
cargo +1.85.1 zigbuild --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-fuse --test range_ioctl_final_protocol_probe --release
```

The unique mount container was created once:

```sh
docker run -d --name layerfs-241-final-protocol-e508f8d1 --privileged --entrypoint sh --mount type=bind,src=/Users/yifanxu/.codex/worktrees/issue241-protocol-probe/layerfs/core/target/aarch64-unknown-linux-musl/release/deps,dst=/runner,readonly --mount type=volume,src=layerfs-241-final-protocol-e508f8d1,dst=/probe 5291449c3df7 -c 'sleep infinity'
```

Each case's `receipt.json` retains exact executed argv. The shape was:

```sh
docker exec -e PROBE_CASE=<case> -e PROBE_ROOT=/probe/<case> layerfs-241-final-protocol-e508f8d1 /runner/range_ioctl_final_protocol_probe-5e5983577f430f80 --ignored --exact linux::probe --nocapture
```

Python `time.monotonic_ns()` covered each complete Docker-exec command.
`docker cp` of `/probe/<case>/.` followed each command outside that wall and
its status is recorded. The wall figures are accounting only: cache state was
uncontrolled, and there is **no cold-cache or Edit→Commit timing claim**.

## Remaining proof

At final test-source identity, `python3 core/tools/check_product_boundary.py`
passed (290 production files), `python3 -m unittest discover -s core/tools
-p 'test_*.py'` passed (9 tests), `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all -- --check` passed, `cargo +1.85.1 test
--manifest-path core/Cargo.toml --locked` passed, and `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` passed
on macOS. The additional Linux-target command `cargo +1.85.1 clippy
--manifest-path core/Cargo.toml --locked --target
aarch64-unknown-linux-musl -p layerfs-fuse --test
range_ioctl_final_protocol_probe --release -- -D warnings` stopped before
this test at the pre-existing `clippy::useless_conversion` in
`core/crates/layerfs-workspace/src/backing/segments.rs:192`. The locked
Linux release build and mounted cases did execute. `python3
core/docs/issues/241/evidence/protocol-probe/verify.py` passed against all
five exact-size raw attempts.

The selected product path still needs the portable Workspace splice,
projection admission/handle checks, typed uncertain receipt, real mounted
alias/repeated-edit behavior, and independent Commit verification. A second
STATE can show a changed stamp but does not repair stale `fstat`; both checks
are needed before the caller claims coherent success. Without a nonce,
post-publication errors cannot be unambiguously attributed under concurrent
edits; the simple rule is UNKNOWN and no automatic replay. None of these
kernel-only rows is release admission.

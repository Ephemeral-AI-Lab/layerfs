# #241 test-only STATE/EDIT/ACK protocol probe — frozen before execution

Source base `241096083f29fba74835581b4f3297c015924e8c`; published `fuser`
0.18.0, actual privileged Linux Docker/FUSE, locked release build. This is a
new **provisional v2 test protocol**, not a promotion of `LFR1` and not a
Workspace, Store, SDK, Commit, durability, or performance claim. One attempt
per case, in order, each with a fresh mount and virtual 8,192-byte file. Raw
failures stay retained; no repeat-to-pass. Cache state is uncontrolled.

## Wire format

All integers are little-endian. Three Linux `_IOWR` commands use type `0xf5`:

| Command | Number | Encoded size | Input | Reply |
| --- | --- | ---: | ---: | ---: |
| STATE | `0xc040f542` | 64 | 64 | exactly 48 bytes |
| EDIT | `0xd040f543` | 4,160 | 4,160 | exactly 48 bytes on success |
| ACK | `0xc040f544` | 64 | 64 | exactly 48 bytes |

The 64-byte request header is `0..4="LFR2"`, `4..6=version 2`, `6..8=op`
(STATE 1, EDIT 2, ACK 3), `8..16=nonce`, `16..24=expected incarnation`,
`24..32=expected generation`, `32..40=expected revision`, `40..48=offset`,
`48..56=delete length`, `56..60=insert length`, `60..64=flags=0`.
STATE uses only nonce; all other fields are zero. ACK uses nonce and the
expected stamp; its other fields are zero. EDIT appends exactly 4,096 inline
payload bytes at `64..4160`, byte `i = i % 239`. It inserts those bytes at
offset 4,093, deletes zero bytes, and starts from exact stamp
`(incarnation=0x2410000000000001, generation=7, revision=3)`.

The 48-byte reply is `0..4="LFA2"`, `4..6=version 2`, `6..8=status`
(STATE 0, ACCEPTED 1, CONFIRMED 2), `8..16=nonce`, `16..24=incarnation`,
`24..32=generation`, `32..40=revision`, `40..48=size`. Initially size is
8,192 and revision 3; one edit yields size 12,288 and revision 4. The
test-only mtime advances from `1700000000` to `1700000001`. Unknown nonce
returns `ENOENT`; duplicate EDIT nonce returns `EALREADY` before mutation;
fresh nonce with stale expected stamp returns `ESTALE` before mutation.
STATE with nonce zero returns current stamp; STATE with a known nonce returns
that nonce's accepted/confirmed status. ACK may mark only an accepted nonce
with matching current stamp confirmed. One nonce is published at most once.

All commands are issued on the same real open writable descriptor. The
caller must check `fstat` new size/mtime, 16 bytes at offset 4090, and EOF
at offset 12288 before sending ACK. The daemon cannot observe that readback
directly; the test caller enforces the order. Normal EDIT calls real
`inval_inode(2,0,0)` before replying. A test-only post-publication error
returns `EIO` *after* accepted state and invalidation while the daemon stays
live. This is not a real notifier failure. The caller never retries EDIT.

## Prospective cases

| Case | Exact sequence and nonce | Required result |
| --- | --- | --- |
| `state` | STATE nonce 0 | One callback with exact 64 input bytes, `out_size=64`, caller receives exact 48-byte STATE reply and initial stamp/size; zero publications. |
| `edit_ack` | STATE 0; EDIT nonce `0x91`; caller same-FD readback; ACK `0x91`; STATE query `0x91`; duplicate EDIT `0x91`; stale EDIT `0x93` with original revision 3 | EDIT callback receives exact 4,160 input bytes and `out_size=4160`; caller receives exact 48-byte ACCEPTED reply, then ACK/STATE show CONFIRMED. Duplicate returns `EALREADY`, stale returns `ESTALE`; one publication and no replay. |
| `post_error_query` | STATE 0; EDIT nonce `0x92` with test-only post-publication `EIO`; STATE query `0x92`; caller readback; ACK `0x92`; STATE query `0x92` | Caller EDIT error is uncertain; query returns ACCEPTED with revision 4/size 12,288 without resending EDIT; after readback ACK and query show CONFIRMED. One publication. |

Each case records exact raw callback input hex, intended reply payload hex or
errno, caller-observed return/bytes, callback and publication counts,
mountinfo, stdout/stderr, source/binary/image/lock identities, and full
Docker-exec wall for accounting only. No raw `/dev/fuse` frame capture is
claimed. If Linux or unmodified `fuser` rejects a 48-byte reply for the
encoded `_IOWR` buffer, or ACK cannot be returned on the live FD, record
`FAIL`/`INCOMPLETE` with its exact errno; do not lengthen/retry the same case.

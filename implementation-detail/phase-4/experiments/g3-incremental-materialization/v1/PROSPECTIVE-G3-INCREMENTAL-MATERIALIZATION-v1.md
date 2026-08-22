# Prospective G3-v1 incremental materialization contract

Status: **frozen before build, dry-run, or measurement**

## One variable

Add one benchmark-private native materialization mechanism to the accepted
Canonical-v2 benchmark binary. Do not change canonical bytes/IDs, FastCDC,
mapping/delta formats, SQLite schema/durability, writer policy, public engine or
OS APIs, or existing reconstruction/range semantics.

## Trust model and authority

Attempt A is the separately recorded static NO-GO. Attempt B uses a protected
same-process verified-seed capability:

1. Fully authenticate and stream a file root to a unique same-volume seed.
2. Sync it, close every writable handle, reopen it read-only with `O_NOFOLLOW`,
   reread it completely, and require exact length/digest equality.
3. Unlink its name and retain only the read-only file descriptor. Process death,
   capability loss, reopen, or any binding mismatch rejects the fast path.
4. A single-use keyed-BLAKE3 permit binds: store instance, validation authority,
   profile, integrity epoch, exact current generation/head receipt/transition,
   parent and target namespace/file roots, destination directory device/inode and
   basename, observed destination invalidation identity, store open/authority/
   mutation serials, operation/range, OS-random nonce, and publication serial.
5. Permit authentication/consumption and seed binding precede clone or patch.
   Replay, wrong store/profile/epoch/generation/root/destination/operation, a
   process/open change, mutation-continuity gap, or missing seed takes the full
   authenticated fallback. No fast-path miss mints authority.

The destination identity is an invalidation gate only. Destination bytes are
never trusted: the candidate always comes from the protected seed or from full
authenticated reconstruction. Same-UID compromise of the private process and its
open descriptors is outside this prototype trust boundary; stronger hostile-
same-UID custody requires a different-UID/root service, immutable flag authority,
or an entitled snapshot provider and is not claimed.

## Native path and durability

- One fixed ASCII canonical basename is preflighted; the one-file prototype has
  no case/Unicode sibling collision. Slash, NUL, dot components, symlink, and
  wrong-kind destination substitutions are rejected before payload work.
- Open destination directories and files descriptor-relative with no-follow.
- `fclonefileat` creates a unique OS-random temp in the destination directory.
  Clone miss/unsupported/cross-volume uses the complete fallback.
- Patch only bytes returned by the existing authenticated range path for the
  permit-bound same-size replacement.
- Apply exact mode, sync data, sync metadata, publish with descriptor-relative
  atomic rename, then sync the containing directory.
- Before-publication failure removes the temp and leaves the old destination.
- Lost acknowledgement after rename triggers a fresh no-follow observation and
  complete authenticated comparison against requested, then prior, root. Only
  requested/prior is accepted; otherwise return ambiguous durability.
- Every in-process error removes its exact unique temp. Crash cleanup is by
  future private-root scan and is not represented as completed in-process work.

## Route and precedence

Prospective precedence:

1. destination name/directory/symlink/wrong-kind preflight;
2. current store head/receipt and permit authentication;
3. destination invalidation and seed capability validation;
4. same-size/count/operation qualification;
5. clone/patch or complete authenticated fallback;
6. data sync, metadata sync, rename, directory sync;
7. fresh reconciliation after ambiguous/lost acknowledgement;
8. exact temp cleanup, preserving the first error unless reconciliation yields a
   different head or ambiguity.

Count-changing edits always use the complete fallback. Invalid authority,
external destination invalidation, seed loss/corruption, clone failure, and
unsupported platform also use the complete fallback. Symlink/wrong-kind is the
prospectively declared typed rejection because neither route may follow it.

## Direct counters and equations

Each JSON row records Observed values unless explicitly marked below:

- route: operation, size, generation, parent/target roots, outcome and reason;
- authority: reads/bytes/validations/successes/failures and permit consumption;
- payload: combined mapping/object SQL queries/rows, canonical BLOB reads/bytes,
  authenticated objects/bytes, source bytes reconstructed;
- destination: bytes read, clone calls/success/failure, clone source logical
  bytes, copy calls/bytes, patch calls/bytes, fallback calls/write bytes;
- ranges/metadata: changed ranges/bytes and metadata operations;
- publication: temps created/removed, data sync, metadata sync, rename,
  directory sync, reconciliation calls/outcome;
- resources: Q high-water/terminal, external user/system CPU, RSS/footprint,
  temp/seed logical/apparent/allocated bytes;
- exactness: output bytes, length, mode, old-or-new result, and residue.

`clone_source_logical_bytes` is Derived from the seed length and is not copied,
allocated, physical-I/O, or write evidence. Physical I/O, OS cache warmth,
clone sharing, and stable-media effects are Unavailable from the selected APIs.

Equations and gates:

```text
changed_bytes = sum(end - start) over sorted disjoint permit ranges

qualified no-op:
  fallback_calls = payload_sql_queries = canonical_blob_reads = 0
  canonical_bytes_authenticated = source_bytes_reconstructed = 0
  patch_bytes = fallback_write_bytes = copied_payload_bytes = 0

qualified patch:
  fallback_calls = source_bytes_reconstructed = copied_payload_bytes = 0
  patch_bytes = changed_bytes
  authenticated payload work is bounded by selected mapping/chunks, not S

complete fallback:
  fallback_calls = 1
  source_bytes_reconstructed = fallback_write_bytes = target_length

attributed_wall = preflight + qualification + payload_prepare + data_sync
                + metadata + metadata_sync + rename + directory_sync
                + reconciliation + cleanup
unattributed_wall = operation_total - attributed_wall >= 0
```

A clone reports logical source length only. It does not report payload copy or
physical sharing. A no-op may clone/atomically republish the verified seed, but
must perform zero payload SQL/BLOB/authentication/reconstruction/patch/write work.

## Frozen v1 screen

One release build, then one-shot rows with no selective rerun:

| Sequence | Row | Size | Required result |
|---:|---|---:|---|
| 1 | `qualified-noop` | 10 MiB | seed clone; zero payload auth/patch/write |
| 2 | `qualified-one-byte` | 100 MiB | exactly 1 byte patched; selected auth only |
| 3 | `qualified-one-mib` | 10 MiB | exactly 1 MiB patched; selected auth only |
| 4 | `invalid-authority` | 1 MiB | complete authenticated fallback |
| 5 | `external-mutation` | 1 MiB | invalidation then complete fallback |
| 6 | `symlink-substitution` | 1 MiB | typed no-follow rejection, old link target untouched |
| 7 | `count-change` | 1 MiB | exact complete fallback, no locality claim |
| 8 | `before-publication-fault` | 1 MiB | old destination, zero temp residue |
| 9 | `lost-ack` | 1 MiB | fresh reconciliation proves old or new |

Each operation timer is below 5 seconds; the sum of the nine operation timers
must be strictly below 20 seconds. Preparation/build is excluded from operation
timers but included in a campaign global ceiling of 59 seconds. Fresh versioned
operands/results are mandatory. Zero row may be removed or rerun.

Wall time is descriptive. Direct avoided work controls G3. This screen cannot
claim G4 acceptance, cold physical I/O, cross-process authority, or broad native
workspace integration.

## Acceptance

Retain only if every row satisfies authority, byte/mode exactness, old-or-new,
cleanup, counter equations, bounded Q/RSS/storage, existing identities/errors,
and fallback parity; primary and independent analyzers agree; final focused and
workspace static closure passes once. Otherwise preserve v1 append-only and use
a fresh version for the smallest root-cause repair.

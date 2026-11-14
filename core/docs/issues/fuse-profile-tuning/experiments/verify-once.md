# Experiment `verify-once`: the cost of authentication on the read path

> **Status:** Diagnostic research for #684 on branch `exp/verify-once`, not a
> product contract. One sample per arm, one binary, one batch per profile, each
> batch led by its control. Times are diagnostic; counts are exact.

## Hypothesis

A cold read hashes every byte three times: pack SHA-256 (Store persistence
port), BLAKE3 in the Store resolver, and BLAKE3 again in the Workspace
`CanonicalClient`. Round 2 measured about 13.5 µs per BLAKE3 call over
objects of about 19 KiB (1.4 GB/s) and assumed NEON BLAKE3 should do several
GB/s. So either the hash crates miss their fast aarch64 paths, or the checks
themselves cost the time.

## Step 1: are the fast paths built? Yes, both

- **blake3 1.8.5** (`default-features = false` in every manifest). Its
  `build.rs` turns NEON on for every little-endian aarch64 target unless
  `pure` or `no_neon` is set; the `neon` feature only matters on 32-bit ARM.
  The warm build's script output has `cargo:rustc-cfg=blake3_neon`, and the
  probe binary links `blake3_hash_many_neon` (from `blake3_neon.c`).
- **sha2 0.11.0** (default features). On aarch64 it uses the ARMv8 SHA-2
  instructions when `sha2` is available. `.cargo/config.toml` sets
  `-C target-feature=+aes,+sha2`, so the check is static. The binary has 16
  `sha256h` instructions, and the guest CPU reports `sha2`.
- **No feature change is available.** So the second binary under tag
  `verify-once-fast` was **NOT_RUN**: there was nothing to enable.

The probe's new in-cache bench (`LAYERFS_PROBE_HASHBENCH`, 19,265-byte
objects and 235,000-byte packs, both the cold read's means) shows the
hardware ceiling on this VM:

| Hash, in cache | µs / call | GB/s |
| --- | ---: | ---: |
| BLAKE3, as `ObjectId::for_bytes` (`Hasher`: 18-byte domain, then bytes) | 13.56 | 1.42 |
| BLAKE3, one `blake3::hash` over a copy of domain + bytes (same identity) | 11.72 | 1.64 |
| BLAKE3 of the bytes alone (other identity, reference only) | 11.18 | 1.72 |
| Copy only (alloc + 19 KiB memcpy) | 0.30 | 63 |
| Pack SHA-256 (ARMv8 SHA-2 instructions) | 90.7 | 2.59 |

(Control arm's bench. Two arms' benches were disturbed, at 19.7 and 21.0 µs,
while their in-path BLAKE3 stayed at 13.6 and 12.2 µs. Use the in-path µs
per call below as the CPU indicator.)

**What this shows:**

- The in-path 13.3–13.7 µs per call equals the in-cache bench, so the time
  is the hash itself, not memory.
- 4-way NEON BLAKE3 tops out at about 1.7 GB/s here. It is slower than
  hardware SHA-256. The "several GB/s" expectation does not hold on this
  CPU.
- The one remaining hash-speed lever is in code, not in the build. After
  the 18-byte domain, the incremental `Hasher` must use alignment-limited
  subtrees, so only 12 of an object's 19 chunks reach the 4-way NEON
  kernel. One `blake3::hash` over a contiguous copy sends 16 of 19 there.
  The digest is identical; the bench asserts it, and every `fasthash` arm
  passed its checksum.

## Change (commit `a087d0adbf`)

New tokens in `LAYERFS_PROBE_READ`, all off when unset, each marked
`Prototype (...)`:

- **`nopacksha`** (`port/read_pack.rs`, `port/read_selection.rs`): skips the
  comparison of a pack read with its descriptor's SHA-256 key, for whole and
  ranged reads. The length and framing checks stay, and publication still
  computes the key.
- **`noresolve`** (`encoding/delta/read.rs`, `encoding/pool/read.rs`,
  `encoding/pool/branch.rs`): skips the resolver's BLAKE3 comparison of
  every reconstructed object against its locator identity, for delta-chain
  objects and bases, pooled leaves and program pages. The switch is
  process-wide.
- **`fasthash`** (`layerfs-content` `object/id.rs`): computes the same
  `ObjectId` with one `blake3::hash` over a contiguous copy.
- The existing **`noverify`** skips the client's BLAKE3.

Binary SHA-256 `d6ef25a8c6d1c5af1c72590dda562407140a5c52c69065f7e12f47b111cf6288`
(release, `--locked --offline -j 6`, `rust:1.85.1-bookworm`). Every arm ran
it.

## Results

Fixture: fresh `prepare`, Store 532,873,216 bytes. Each arm checked:

- `store_resident_before` 0
- 27,214 objects demanded
- the 500 MiB read back against the source fold
- exit 0

Guest 1-minute load at arm start was 2.4–4.3. Columns:

- **BLAKE3 µs** is in-path time per call, the CPU indicator.
- **Kept** names the checks still on: P = pack SHA-256, R = resolver BLAKE3,
  C = client BLAKE3.

**128 KiB / 1 in flight, `async` on** (batch control `a128-control`):

| Arm (`LAYERFS_PROBE_READ`) | Kept | MB/s | × | Wall ms | Store demand ms | Pack SHA n / ms | BLAKE3 n / ms | BLAKE3 µs | VmHWM KB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `async` (control) | P R C | 296.9 | 1.00 | 1,766 | 1,316 | 2,267 / 218 | 54,429 / 725 | 13.3 | 104,536 |
| `async,fasthash` | P R C | 318.3 | 1.07 | 1,647 | 1,245 | 2,267 / 218 | 54,429 / 662 | 12.2 | 104,392 |
| `async,noverify` | P R | 372.7 | 1.26 | 1,407 | 1,316 | 2,267 / 218 | 27,215 / 365 | 13.4 | 110,548 |
| `async,noresolve` | P C | 375.5 | 1.26 | 1,396 | 952 | 2,267 / 218 | 27,214 / 362 | 13.3 | 187,004 |
| `async,noverify,fasthash` | P R | 401.7 | 1.35 | 1,305 | 1,224 | 2,267 / 216 | 27,215 / 332 | 12.2 | 104,576 |
| `async,noresolve,nopacksha` | C | 428.3 | 1.44 | 1,224 | 758 | 0 | 27,214 / 371 | 13.6 | 104,716 |
| `async,noverify,nopacksha` | R | 442.2 | 1.49 | 1,186 | 1,094 | 0 | 27,215 / 367 | 13.5 | 103,892 |
| `async,noresolve,nopacksha,fasthash` | C | 463.6 | 1.56 | 1,131 | 712 | 0 | 27,214 / 332 | 12.2 | 104,688 |
| `async,noverify,nopacksha,fasthash` | R | 479.2 | 1.61 | 1,094 | 1,008 | 0 | 27,215 / 339 | 12.4 | 103,924 |
| `async,noverify,noresolve` | P | 531.2 | 1.79 | 987 | 902 | 2,267 / 218 | 0 | — | 104,628 |
| `async,noverify,noresolve,nopacksha` | none | 683.8 | 2.30 | 767 | 688 | 0 | 0 | — | 104,244 |

**512 KiB / 4 in flight, `single` on** (batch control `b512-control`):

| Arm (`LAYERFS_PROBE_READ`) | Kept | MB/s | × | Wall ms | Store demand ms | Pack SHA n / ms | BLAKE3 n / ms | BLAKE3 µs | VmHWM KB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `single` (control) | P R C | 377.8 | 1.00 | 1,388 | 1,699 | 2,390 / 219 | 54,429 / 746 | 13.7 | 98,204 |
| `single,noverify` | P R | 382.7 | 1.01 | 1,370 | 2,031 | 2,390 / 221 | 27,215 / 371 | 13.6 | 98,416 |
| `single,fasthash` | P R C | 385.8 | 1.02 | 1,359 | 1,665 | 2,390 / 219 | 54,429 / 683 | 12.5 | 98,120 |
| `single,noresolve,fasthash` | P C | 457.9 | 1.21 | 1,145 | 1,216 | 2,389 / 221 | 27,214 / 347 | 12.7 | 99,928 |
| `single,noverify,nopacksha` | R | 465.7 | 1.23 | 1,126 | 1,578 | 0 | 27,215 / 363 | 13.4 | 98,456 |
| `single,noresolve,nopacksha` | C | 472.7 | 1.25 | 1,109 | 1,077 | 0 | 27,214 / 381 | 14.0 | 105,888 |
| `single,noresolve` | P C | 485.7 | 1.29 | 1,079 | 1,111 | 2,390 / 217 | 27,214 / 377 | 13.9 | 100,260 |
| `single,noverify,noresolve` | P | 558.5 | 1.48 | 939 | 1,259 | 2,390 / 216 | 0 | — | 97,964 |
| `single,noverify,noresolve,nopacksha` | none | 594.1 | 1.57 | 882 | 1,043 | 0 | 0 | — | 122,248 |

At 512 KiB, `noresolve,fasthash` (457.9) came out below `noresolve`
(485.7), though it cannot be slower in work. Treat about ±5% as single-sample
noise at this profile. The `nopacksha` arms also drop pack SHA-256 calls on
ranged reads, which the `pack_hash` stage does not time.

## What worked, what did not

- **Where the one BLAKE3 runs matters more than whether there are two.**
  - At 4 in flight, `noverify` (resolver check kept) gains 1%. The client
    checks it removes already ran on the four READ threads, off the
    critical path.
  - `noresolve` (client check kept) gains 29%. It removes the hash from
    inside the Workspace's one Store read session (store demand
    2,031 → 1,111 ms), where it was serialised.
  - At 1 in flight both gain the same 26%.
- **Pack SHA-256** costs 218 ms per 500 MiB (2.6 GB/s). Dropping it on top
  of one BLAKE3 gives:
  - +19% at 128 KiB: 372.7 → 442.2, resolver check kept.
  - +22% at 512 KiB: 382.7 → 465.7, resolver check kept.
  - −3% at 512 KiB with the client check kept: 485.7 → 472.7, within noise.
    That check is already off the session, and the SHA is a small part of
    what remains there.
- **One-shot BLAKE3** (`fasthash`) cuts 13.4 → 12.2 µs per call (−9%) with
  an identical digest:
  - +7% with all checks kept: 296.9 → 318.3.
  - +8% on `noverify`: 372.7 → 401.7.
  - +8% on `noverify,nopacksha`: 442.2 → 479.2.
  - No measurable gain at 4 in flight.
- **A faster hash build:** none exists, as Step 1 shows.
- **Zero authentication** reaches 684 MB/s (2.3×) at 128 KiB. That is the
  ceiling these checks impose on this path, not a recommendation.
- **Memory:** `a128-noresolve` peaked at 187 MB and `b512-noauth` at 122 MB.
  Every other arm stayed at 98–111 MB. These are single samples and were not
  diagnosed; round 2 saw a similar 185 MB outlier.

## Integrity guarantee kept by each arm (the owner's decision input)

Facts that hold in every arm:

- The pack key and the object locators both live in the one SQLite Store,
  written at publication. Neither check stops a writer who can rewrite the
  Store. Both detect corruption (media, torn or misplaced pages, wrong rows)
  and software bugs.
- BLAKE3 `ObjectId`s are content addresses named by their parent objects,
  up to the Commit root. A BLAKE3 check on every object gives Merkle
  authentication from the root: SHA-256 alone does not.
- The content provider contract (`layerfs-content` `object/access.rs`
  `AuthenticatedObjects`) says returned bytes are *already authenticated
  against the requested identity*. Today the resolver fulfils it for every
  consumer: the Workspace client, content reads and compares, the save
  provider, SDK Store calls.

| Kept | Who authenticates which bytes against what | What is lost against the control |
| --- | --- | --- |
| **P R C** (control, `fasthash`) | The Store checks each whole pack body against its descriptor's SHA-256 before parsing. The resolver checks every reconstructed object, chain bases included, against its locator's `ObjectId`. The client checks each returned object against the id it asked for at that batch position. | Nothing; `fasthash` computes the same digest. |
| **P R** (`noverify`) | Pack bodies against their pack key. Every reconstructed object against its locator `ObjectId`, so the root-to-byte Merkle chain is intact and every provider consumer is covered. | The client no longer re-checks the Store→client hand-off in the same process. A bug that pairs bytes with the wrong id (batch order, cache keying) gets past it. Cardinality and length are still checked. |
| **P C** (`noresolve`) | Pack bodies against their pack key. Only the client checks the final objects it requested against their ids, so the Merkle chain still holds for Workspace reads. Chain bases and pooled values are checked only through the final object they produce. | The provider contract is broken for every other consumer: content reads, compare, save-provider base reads and SDK Store calls get unauthenticated objects unless each adds its own check. As a product change this needs a contract change and an audit of every consumer. |
| **R** (`noverify,nopacksha`) | Every reconstructed object against its locator `ObjectId`, in the Store, for every consumer. Corruption is still caught if it changes any byte of a returned object. | No check before parsing or decompression. Pack headers, directories, zstd frames and delta programs are decoded before anything is authenticated, so those decoders must reject arbitrary corruption safely. They are bounded and fail closed today, but that becomes the only guard. Records not read, and bytes that do not affect output, are no longer checked. The product already accepts this for scoped unit reads (`port/read_scoped.rs`: *no whole-pack or unread-record integrity claim*). |
| **C** (`noresolve,nopacksha`) | Only the client checks the final objects against their requested ids. | Both losses above: no check before parsing, and no Store-side authentication for any other consumer. |
| **P** (`noverify,noresolve`) | Pack bodies against their pack key only. | Content addressing is not checked on read. A wrong locator row (object id → pack, offset), a decode bug or a wrong delta base returns wrong bytes silently, because a valid pack authenticates nothing about which record was asked for. The Merkle chain is gone. |
| **none** | Nothing; only length, cardinality and structure checks. | Any corruption or bug is returned as file content. |

## Recommendation

1. **Adopt one-shot BLAKE3 (`fasthash`)**: same identity, no change to
   integrity, about 9% less time per hash and +7–8% cold read at 1 in
   flight. A product version would reuse one per-thread buffer instead of
   allocating per call.
2. **Authenticate once with BLAKE3; owner decision on where.**
   - **Resolver** (`noverify`, P R): keeps the provider contract for all
     consumers. At 128 KiB / 1 it is as fast as the client choice. At 4 in
     flight it gains nothing, because the hash runs inside the serialised
     Store session.
   - **Client** (`noresolve`, P C): +29% at 4 in flight, but it breaks the
     provider contract, as the table says.
   - **Middle option, not measured:** keep the check in the Store layer but
     run it after the batch leaves the read-session lock. It should give the
     client's parallelism and keep the contract.
3. **Pack SHA-256 on read: owner decision.**
   - What dropping it buys: +19–22% when the BLAKE3 is in the resolver, and
     nothing measurable when it is in the client at 4 in flight.
   - What it costs: the check before parsing and the audit of unread
     records. With a BLAKE3 kept, every returned byte is still authenticated
     from the Commit root.
   - Do not drop it unless a BLAKE3 check on every object stays.
4. Do not drop both BLAKE3 checks. The P-only arm (+79%) loses content
   addressing on read, as its row says.

## Checks and gaps

- **Built:** Linux release build of `-p layerfs-daemon --example
  fuse_profile_probe`, `--locked --offline`; rustfmt 1.85.1 on changed
  files.
- **Clippy:** `NOT_RUN`; the image has no Clippy.
- **Not run:**
  - unit and mount tests;
  - the second build tag, since no feature change exists;
  - corruption-injection tests showing which arm catches which fault: the
    integrity table is from source reading, not a fault test;
  - the resolver-check-outside-the-lock variant.
- **Switch scope:** `noresolve` and `nopacksha` are process-wide. The probe's
  write phases (sequential, random 4 KiB, creates) also ran under them; they
  passed, but they do not check the bytes they wrote.
- **Bench noise:** two arms' in-cache benches were disturbed (19.7 and
  21.0 µs) while their in-path hashes were normal.

Receipts: [`receipts/exp-verify-once/`](../receipts/exp-verify-once/), one
log per arm with every phase's `PROBE` line and the `hash_bench` line.

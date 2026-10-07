# Combined Save reservations with counted refills

> **Status:** Implemented allocation windows and Disposable host/Linux proofs
> after `656575d7a`; integrated daemon Commit remains pending.
> Owner approved counted block refills in SAVE-RESERVATION-DECISION-20261007.md.

- Add Storage store/reservations.rs: checked startup block settings, defaults
 4096 pack ids and16384 pooled ordinals, within the existing per-call limits.
 Add a constructor accepting these settings; ordinary Storage::new uses defaults.
 These are allocation windows, not total Save limits.
- State::new obtains one combined acknowledged reservation after owning producer
 state; initialize both ranges from it. Remove cross-Save pack-tail transfer.
 Keep encoding, object/pack formats, candidate indexes and publication bounds.
- Reuse reserve_packs and reserve_ordinals for explicit new blocks when required,
 sized to the configured window or current bounded demand, whichever is larger.
 Remove the superseded first-four-leaf lookahead/threshold treatment. Preserve
 exact typed Busy/unknown failure and acknowledged unused ordinal release.
- Expose initial/refill reservation counts in existing Storage diagnostics.
 All Store write equations count every attempt and publication batch; no
 automatic retry or silently enlarged total workload cap.
- Wire configurable blocks into the daemon's provider-neutral Store producer
 configuration. Init and Commit share this implementation, with independent
 Storage/Save state; the accepted earlier Init baseline retains its source pin.
- Add/port external Disposable reservation tests: one combined default block,
 local values and packs cross small configured windows, saved bytes reconstruct,
 scope separation across Saves, contention before effect and no changed format.
 Port the existing ordinal/tail assertions to the explicit per-Save contract;
 preserve old receipts. No Durable execution or new timing sample.
- Update source architecture/handbook and owner-ruling links. Targeted builds
 precede bounded runs; scoped Clippy/fmt/boundary/self-tests, exact LOC commit.

## Outcomes

[Receipts01–14](checks/pre-s8-save-blocks-20261007/) retain all outcomes.
Build01 failed on one same-Save provider call still using the old two-argument
reserve_packs signature; source review found and updated that last call before
build02. No failed test body occurred. Receipts03/08 pass four allocation bodies,
04/09 pass ten existing bounded transaction/pack ownership bodies,05/10 pass
three installed-Store bodies on host/Linux respectively. Receipt06 pins changed
source/test bytes and refreshes the container view;07 checks equality before build.

The configured1-pack/3-ordinal case saves ten distinct values in five leaves,
crosses both ranges, and reconstructs every byte. Exact counts are one initial
reservation, six refills (four ordinal and two pack), one publication: eight
successful Store write transactions. Ordinal-reservation count is five, including
the initial combined attempt. Default blocks cover the existing small/queued
cases with one new combined call per Save. Independent Saves obtain separate
ranges; abandoned consumed ids are not recycled. Deduplicated values retain
correct bytes and the exact unused ordinal-tail release keeps the single-writer
allocator dense at successful finish. Foreign acknowledged gaps remain.

With another process holding the writer, begin_save returns StorageError::Busy
and completes no write transaction. A later explicit Save succeeds; its first
pack and value ordinal are both1, proving no allocation effect on Busy. The
child is released and joined. Installed root/length/cache/multi-Workspace and
writer-overlap proofs remain successful under the new initialization path.
The count equation uses the owner's approved refill revision; a fixed block is
not a total file, Save, Commit or Workspace cap.

Host binaries (SHA256): ordinals
`9cabdf163a1e732876f85bb970b3f3cdad01e54ba97e638e4b1e1a81b90c8e48`,
transactions `e7d19d835f7465224010fbcf4e2294272de7c5bc36023ed983c2514f5abfc8bb`,
installed Store `a506c282469e8bfbda82603e851478377599723c9fcdc4c0d2844f936c3cd43e`.
Linux: ordinals `8d4faa0e0b6d35caf0f324321fe0fa83247f31bb6556e50c7d645d7d8516b5c4`,
transactions `804db8810c59e7dea8d21ee8fe2d658d3cd89c7dd710356b0ab19901293d3a10`,
installed Store `3f68fe12e25e53f90417c2f2c79c3f3c91c16605863f663dbaede21c8495c2cf`.
Pinned image `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`;
locked Cargo1.85.1/test-optimized/root ARM64 profile and
`LAYERFS_CONSTRUCTION_WORKERS=1`. Tests run one binary at a time under100s stops;
each observed body group is below1s, diagnostic only. Linux Store placement is
container-local `/tmp`. Successful fixture cleanup and process joins complete;
no new terminal unknown. Original earlier failed fixtures/receipts remain.

Profile: Disposable WAL/OFF only. Durable is compiled, execution NOT_RUN —
deferred by owner. Cache state is controlled only for named immutable-cache
count assertions, not an OS-cold performance claim. No timing sample is added;
the accepted earlier host Init baseline retains its exact earlier source and
is not relabelled as a measurement of this allocation treatment.

Final scoped all-target Clippy11, formatting12,751-file boundary13 and43 guard
self-tests14 pass. Ordinary same-Save provider demand was covered explicitly by
the carried-queue dependency proof. F8 local capture/Save/history/install
composition and F9 multiprocess Saves still need their own evidence.

Production LOC:170965 ->170969 (delta+4). Core105548 ->105552,
active62383 ->62387; reference65417, excluded predecessors36325 and excluded
integration6840 unchanged. Receipt15 counts exact staged/parent product trees.
This is an allocation-policy change, not transport retirement or a measured
algorithmic speed improvement.

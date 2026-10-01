# Mapping cache metadata correction source freeze

Root's owning System observation finds actual HashMap metadata peak17136/largest
11400 after CanonicalBuffer widened its real entry. The original16KiB allowance
is unchanged. Incremental reserve temporarily retains old+new tables and is the
cause; no read/cache/output threshold or provider fallback is changed.

NEW file/mapping/cache_table.rs owns a separate fixed16KiB metadata authority and
one exclusive last-owner table grant. Its controller's compiled Arc allocation
is counted as fixed24 on64bit. PageCache.pages precedes its table grant so map
allocation/data are destroyed before credit return. The32MiB CanonicalBudget
continues to count returned/copied data capacities only.

Before first provider/copy/cache-decode effect, prepare reserves the declared
retained count once. Pinned Rust1.85 HashMap/hashbrown layout uses actual
Layout<((ObjectId,bool),CanonicalBuffer)> entry width/alignment, power-of-two
buckets for7/8 load (small4/8), control offset aligned to max(entryalign,groupwidth),
then buckets+groupwidth control bytes. SSE2group16, other supported currentgroups
usize width (aarch64group8). Rust requested allocation is the computed Layout size;
external System proof must match actual largest/live requests. Aftertry_reserve,
actual reported HashMap capacity must match that computed class; a mismatch is an
explicit unsupported layout error before provider effects, never a larger grant.
Wholesale clear preserves allocation/capacity; insertion does not grow the table.
Default64 pages stay64. Explicit custom count shapes outside16KiB refuse before
allocation/provider effects rather than silently shrinking their count policy.

Root serializes Cargo/owning test checks. This corrects one bounded cache metadata
owner; C2 native/provider caches/pack/decoders and global memory fit remain open.
Primary pinned source contracts: Rust1.85.1 std/collections/hash/map.rs and
hashbrown RawTable TableLayout/capacity_to_buckets. No dependency changes/patches.

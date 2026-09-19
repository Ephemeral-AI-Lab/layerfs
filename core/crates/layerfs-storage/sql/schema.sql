-- Candidate C2 schema: five tables, twenty-five declared columns.
--
-- This identifier is deliberately not the reference schema 10 application id and
-- user_version: the candidate stores a role column and a persisted storage
-- policy, so a file that carries this header must not be
-- mistaken for a schema-10 Store. Version 3 widened the persisted policy CHECK
-- ranges to the supported configurable profile. Version 4 added
-- `store_policy.metadata_delta_max_depth`, the pooled-metadata dependency bound.
-- Version 5 removes `objects.base_object_id`; see the note below that table.
-- Version 6 adds `content_signatures`, the persisted cross-save content index;
-- see the W2 section at the end of this file. Older Stores are rejected rather
-- than migrated.
PRAGMA application_id = 1279677261;
PRAGMA user_version = 6;

CREATE TABLE store_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_profile INTEGER NOT NULL CHECK (format_profile = 1),
    -- Supported cutoffs are powers of two from 128 KiB to 1 MiB; the range is a
    -- necessary condition and the exact power-of-two check runs in the policy.
    small_file_threshold_bytes INTEGER NOT NULL
        CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
    whole_file_delta_max_depth INTEGER NOT NULL
        CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
    chunk_delta_max_depth INTEGER NOT NULL
        CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
    -- Pooled-metadata dependency bound: separate from both payload depths.
    metadata_delta_max_depth INTEGER NOT NULL
        CHECK (metadata_delta_max_depth BETWEEN 0 AND 50),
    -- Publication watermark: the highest pack id belonging to a COMPLETED save.
    -- It advances only inside a save's final transaction, so a pack is visible to
    -- an ordinary reader exactly when the save that created it was acknowledged.
    retained_pack_ceiling INTEGER NOT NULL
        CHECK (retained_pack_ceiling >= 0)
) STRICT;

CREATE TABLE object_packs (
    pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),
    data BLOB NOT NULL CHECK (length(data) >= 32)
) STRICT;

CREATE TABLE metadata_value_groups (
    first_ordinal INTEGER PRIMARY KEY CHECK (first_ordinal BETWEEN 1 AND 4294967295),
    count INTEGER NOT NULL CHECK (count BETWEEN 1 AND 165),
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id) ON DELETE CASCADE,
    group_number INTEGER NOT NULL CHECK (group_number BETWEEN 0 AND 255),
    digest BLOB NOT NULL CHECK (length(digest) = 32),
    CHECK (first_ordinal + count <= 4294967296),
    UNIQUE (pack_id, group_number)
) STRICT;

CREATE TABLE objects (
    object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32),
    -- Role codes 1-6 are payload and file-mapping roles; 7-13 are the
    -- filesystem-tree roles (directory leaf/branch, inode branch, filesystem root,
    -- attribute leaf/branch, symlink target). A pooled inode leaf keeps role 6.
    object_role INTEGER NOT NULL CHECK (object_role BETWEEN 1 AND 13),
    canonical_length INTEGER NOT NULL
        CHECK (canonical_length > 0 AND canonical_length <= 16777216),
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),
    group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),
    record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)
) STRICT, WITHOUT ROWID;

-- **R2, T1 (#188), owner ruling C.** Two secondary indexes were removed here.
--
-- `objects_bases` was `ON objects(base_object_id) WHERE base_object_id IS NOT
-- NULL`. **Nothing in the product ever queried it**: the only statement that
-- reads a base id filters on `object_id`, the primary key. It existed to make a
-- lookup by base fast, and no such lookup was ever written. Measured cost on the
-- retained-history lane: **2,822,144 B**, 4.5 % of a 63 MB Store.
--
-- `objects_locations` was `UNIQUE ON objects(pack_id, group_number, record_number)`
-- and its one consumer is the failed-save cleanup's page query
-- (`sqlite/cleanup.rs`), which seeks `pack_id > baseline` and orders by the same
-- three columns. Measured cost: **2,551,808 B**. Removing it costs that query a
-- scan, and the scan is bounded by `CLEANUP_PAGE_ROWS` per iteration; it buys
-- 4.0 % of the Store back on a path that runs only after a failed save.
--
-- What is **not** lost: the base identity is already inside the packed record
-- (`encoding/delta/record.rs` appends the 32-byte base id), and it is now read
-- from there. The uniqueness the second index enforced is a writer invariant —
-- locators are assigned by placement and an `object_id` duplicate is refused by
-- the primary key — so no reader depends on the constraint either.
--
-- **R2, T1 (#188), owner ruling D: `objects.base_object_id` was removed here.**
--
-- The column held the 32-byte identity of this row's direct base, which the
-- packed record already carries: the delta and pooled readers now read it from
-- the record (`encoding/delta/record.rs::stored_base`) and the walk is otherwise
-- unchanged, because each edge was already resolved by a primary-key seek on the
-- base identity. Measured on the retained-history lane: **1,335,296 B** of the
-- `objects` btree, 1,012 pages down to 686.
--
-- What it costs: the walk must now read the record of each element before it can
-- name that element's base, so a chain walk touches the packs its decode pass
-- touches anyway. Both walks share the operation's pack cache, so the bodies are
-- fetched once. A reader never guesses: an element whose record cannot be parsed
-- is an integrity failure, not a shorter chain.

-- ===========================================================================
-- **W2, #188d — the persisted cross-save content-signature index.**
--
-- This table is added by the W2 squad and is the only schema change that squad
-- makes. It is a clearly separated section on purpose: the `objects` table and
-- its indexes are **not** touched here — W3 owns them — and no column, index or
-- constraint of any pre-existing table moves.
--
-- **What it is.** One row per ring slot of `encoding/delta/candidates.rs`. The
-- index proposes a delta base by what an object contains rather than by the name
-- it was stored under, so it reaches a predecessor the caller never declared.
-- Before this table the index was owned by one save and dropped with it
-- (`cas/lifecycle.rs`, `cas/store.rs`), which made a cross-save match impossible
-- in the product at any index size; the harness had to model it itself.
--
-- **Why a separate table and not a column on `objects`.** A signature column
-- would put 64 bytes of cold payload on every row of the hot membership btree,
-- which every `lookup::location` seek reads, and it would pay that cost for every
-- role rather than for the whole-file lane alone. Keeping the signatures in their
-- own btree leaves the `objects` page shape exactly as it was: the bytes per read
-- of a location lookup do not move.
--
-- **Why the key is the slot.** A row *is* a ring slot, so `INSERT OR REPLACE`
-- maintains the ring and the declared bound is enforced by the primary key. No
-- eviction statement ever runs, and the table can never hold more rows than the
-- index has slots. `stamp` is the one-based insertion number, kept so that a
-- reopened Store can rebuild the ring position (`(stamp - 1) % SLOTS`), the
-- insertion cursor and the flush cursor without storing an order anywhere else.
--
-- **No `UNIQUE (object_id)`.** A duplicate would cost a second btree of roughly
-- 40 bytes per row, and it cannot arise: an object reaches the index only when
-- the Store did not already hold it, so the same identity is admitted at most
-- once. A row whose object the Store does not hold is harmless either way — the
-- selector probes every proposed candidate against `objects` and refuses an
-- absent one, so a stale proposal is a policy outcome and never an error.
--
-- **The bound is the table size.** At most `candidates::SLOTS` = 8,192 rows of
-- 32-byte identity plus 32-byte folded signature. Measured on the retained-
-- history lane the index settles at 7,614 rows; `Store::open` reads at most that
-- many and refuses a table that holds more rather than truncating it.
-- ===========================================================================
CREATE TABLE content_signatures (
    -- Ring slot. Bounded by `candidates::SLOTS` in Rust, which is where the
    -- declared index bound lives; the CHECK here only refuses a negative slot.
    slot INTEGER PRIMARY KEY CHECK (slot >= 0),
    -- One-based insertion number. Positive, so 0 always means "no row".
    stamp INTEGER NOT NULL CHECK (stamp > 0),
    object_id BLOB NOT NULL CHECK (length(object_id) = 32),
    -- Eight folded hashes, little-endian, 4 bytes each.
    signature BLOB NOT NULL CHECK (length(signature) = 32)
) STRICT;

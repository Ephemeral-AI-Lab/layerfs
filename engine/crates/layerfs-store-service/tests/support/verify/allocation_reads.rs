//! External #768 allocation reads on either backend, straight from the
//! test's private database: the Store's two id counters and what the stored
//! rows use of them, and the duplicate-allocation checks
//! (`verify768::duplicate_allocations` on PostgreSQL, the same checks in
//! SQL on SQLite) plus amcheck on PostgreSQL.
use super::adversary::{Kind, TestStore};
use super::verify768;
use layerfs_persistence::StoreSelection;

/// One reading of the allocators and the rows that use them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Alloc {
    /// `store_policy.next_ordinal`.
    pub next_ordinal: i64,
    /// Ordinals held by stored value blocks.
    pub used_ordinals: i64,
    /// The last stored ordinal (0 when none).
    pub last_ordinal: i64,
    /// `store_policy.next_pack_id`.
    pub next_pack: i64,
    /// Stored packs.
    pub packs: i64,
    /// The largest stored pack id (0 when none).
    pub last_pack: i64,
}

impl Alloc {
    /// Ordinals below the counter that no stored value uses.
    pub fn unused_ordinals(&self) -> i64 {
        self.next_ordinal - 1 - self.used_ordinals
    }
    /// Pack ids below the counter that no stored pack uses.
    pub fn unused_packs(&self) -> i64 {
        self.next_pack - 1 - self.packs
    }
}

/// The allocator reading; `{c}` is the cast PostgreSQL needs (its `sum`
/// of bigint is numeric), empty on SQLite.
const ALLOC: &str = "SELECT \
    (SELECT next_ordinal FROM store_policy WHERE id = 1), \
    (SELECT coalesce(sum(last_ordinal - first_ordinal + 1), 0){c} FROM inode_value_block), \
    (SELECT coalesce(max(last_ordinal), 0) FROM inode_value_block), \
    (SELECT next_pack_id FROM store_policy WHERE id = 1), \
    (SELECT count(*) FROM pack), \
    (SELECT coalesce(max(pack_id), 0) FROM pack)";

pub fn sqlite(store: &TestStore) -> rusqlite::Connection {
    let StoreSelection::Sqlite(config) = &store.selection else {
        panic!("a SQLite Store")
    };
    let connection = rusqlite::Connection::open(&config.path).unwrap();
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    connection
}

pub fn alloc(store: &TestStore) -> Alloc {
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .query_row(&ALLOC.replace("{c}", ""), [], |row| {
                Ok(Alloc {
                    next_ordinal: row.get(0)?,
                    used_ordinals: row.get(1)?,
                    last_ordinal: row.get(2)?,
                    next_pack: row.get(3)?,
                    packs: row.get(4)?,
                    last_pack: row.get(5)?,
                })
            })
            .unwrap(),
        Kind::Postgres => {
            let row = store
                .admin()
                .query_one(&ALLOC.replace("{c}", "::bigint"), &[])
                .unwrap();
            Alloc {
                next_ordinal: row.get(0),
                used_ordinals: row.get(1),
                last_ordinal: row.get(2),
                next_pack: row.get(3),
                packs: row.get(4),
                last_pack: row.get(5),
            }
        }
    }
}

/// Duplicate or out-of-range allocations, and on PostgreSQL amcheck
/// failures: each as text.
pub fn allocation_failures(store: &TestStore) -> Vec<String> {
    match store.kind {
        Kind::Postgres => {
            let mut out = verify768::duplicate_allocations(store);
            out.extend(verify768::amcheck(store));
            out
        }
        Kind::Sqlite => {
            let sql = sqlite(store);
            let checks = [
                (
                    "duplicate object locators",
                    "SELECT count(*) FROM (SELECT object_id FROM object_location \
                     GROUP BY object_id HAVING count(*) > 1)",
                ),
                (
                    "overlapping inode value blocks",
                    "SELECT count(*) FROM inode_value_block a JOIN inode_value_block b \
                     ON a.first_ordinal < b.first_ordinal AND b.first_ordinal <= a.last_ordinal",
                ),
                (
                    "locators into a missing pack",
                    "SELECT count(*) FROM object_location o WHERE NOT EXISTS \
                     (SELECT 1 FROM pack p WHERE p.pack_id = o.pack_id)",
                ),
                (
                    "inode values past the allocator",
                    "SELECT count(*) FROM inode_value_block, store_policy p \
                     WHERE p.id = 1 AND last_ordinal >= p.next_ordinal",
                ),
                (
                    "packs past the allocator",
                    "SELECT count(*) FROM pack, store_policy p WHERE p.id = 1 AND pack_id >= p.next_pack_id",
                ),
            ];
            let mut out = Vec::new();
            for (what, query) in checks {
                let n: i64 = sql.query_row(query, [], |row| row.get(0)).unwrap();
                if n != 0 {
                    out.push(format!("{what}: {n}"));
                }
            }
            let integrity: String = sql
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .unwrap();
            if integrity != "ok" {
                out.push(format!("integrity_check: {integrity}"));
            }
            out
        }
    }
}

/// Every stored value block `(first, last)`, in order.
pub fn value_blocks(store: &TestStore) -> Vec<(i64, i64)> {
    const BLOCKS: &str =
        "SELECT first_ordinal, last_ordinal FROM inode_value_block ORDER BY first_ordinal";
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .prepare(BLOCKS)
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        Kind::Postgres => store
            .admin()
            .query(BLOCKS, &[])
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect(),
    }
}

/// Every stored pack id, in order.
pub fn pack_ids(store: &TestStore) -> Vec<i64> {
    const PACKS: &str = "SELECT pack_id FROM pack ORDER BY pack_id";
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .prepare(PACKS)
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        Kind::Postgres => store
            .admin()
            .query(PACKS, &[])
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect(),
    }
}

/// A small deterministic generator (xorshift64*), so a failing round is
/// reproduced by its seed.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

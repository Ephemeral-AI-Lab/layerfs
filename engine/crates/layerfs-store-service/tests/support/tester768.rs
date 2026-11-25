//! Independent #768 mount/unmount tester helpers (not the author's): the
//! three session advisory keys a Branch mount takes, who holds them read
//! from `pg_locks` on a plain admin connection, the live intent rows of a
//! Branch, a blocker connection that holds a key or a table lock, and the
//! Store's generation. Nothing here is product code.
use super::adversary::{wait_until, TestStore};
use layerfs_history::{BranchId, CatalogId};
use std::time::Duration;

/// The session advisory key of `kind` and `id` (spec 5.2), derived as the
/// provider derives it: 1 the Store (catalog), 2 a project, 3 a Branch.
pub fn advisory_key(binding: &[u8], kind: u8, id: &[u8]) -> i64 {
    let catalog = CatalogId::derive(binding).unwrap();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(catalog.as_slice());
    hasher.update(&[kind]);
    hasher.update(id);
    let mut head = [0u8; 8];
    head.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    i64::from_be_bytes(head)
}

/// The Store, project and Branch keys of a mount of `branch`.
pub fn mount_keys(store: &TestStore, branch: BranchId) -> [i64; 3] {
    let catalog = CatalogId::derive(&store.binding).unwrap();
    [
        advisory_key(&store.binding, 1, catalog.as_slice()),
        advisory_key(&store.binding, 2, store.stack.as_slice()),
        advisory_key(&store.binding, 3, branch.as_slice()),
    ]
}

/// The Store key alone.
pub fn store_key(store: &TestStore) -> i64 {
    let catalog = CatalogId::derive(&store.binding).unwrap();
    advisory_key(&store.binding, 1, catalog.as_slice())
}

/// Every granted holder of `key` as (pid, mode), except `exclude`.
pub fn holders(admin: &mut postgres::Client, key: i64, exclude: &[i32]) -> Vec<(i32, String)> {
    let bits = key as u64;
    let class = (bits >> 32) as i64;
    let object = (bits & 0xffff_ffff) as i64;
    admin
        .query(
            "SELECT pid, mode FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND classid::bigint = $1 AND objid::bigint = $2 ORDER BY pid",
            &[&class, &object],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get::<_, i32>(0), row.get::<_, String>(1)))
        .filter(|(pid, _)| !exclude.contains(pid))
        .collect()
}

/// Holders of any of `keys`, except `exclude`, as (key index, pid, mode).
pub fn key_holders(
    admin: &mut postgres::Client,
    keys: &[i64],
    exclude: &[i32],
) -> Vec<(usize, i32, String)> {
    let mut found = Vec::new();
    for (n, key) in keys.iter().enumerate() {
        for (pid, mode) in holders(admin, *key, exclude) {
            found.push((n, pid, mode));
        }
    }
    found
}

/// Live (not `done`) mount intent rows of `branch`.
pub fn live_mounts(admin: &mut postgres::Client, branch: BranchId) -> i64 {
    admin
        .query_one(
            "SELECT count(*) FROM gc_intent WHERE kind = 'mount' AND scope_kind = 'branch' \
             AND scope_id = $1 AND state <> 'done'",
            &[&branch.as_slice()],
        )
        .unwrap()
        .get(0)
}

/// Every live (not `done`) intent row: (kind, scope_kind, state).
pub fn live_rows(admin: &mut postgres::Client) -> Vec<(String, String, String)> {
    admin
        .query(
            "SELECT kind, scope_kind, state FROM gc_intent WHERE state <> 'done' ORDER BY 1, 2, 3",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect()
}

pub fn pid_of(client: &mut postgres::Client) -> i32 {
    client
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0)
}

pub fn alive(admin: &mut postgres::Client, pid: i32) -> bool {
    admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
            &[&pid],
        )
        .unwrap()
        .get::<_, i64>(0)
        > 0
}

/// Terminates backend `pid` and waits (bounded) until it is gone.
pub fn terminate(admin: &mut postgres::Client, pid: i32) -> bool {
    let _: bool = admin
        .query_one("SELECT pg_terminate_backend($1)", &[&pid])
        .unwrap()
        .get(0);
    wait_until(Duration::from_secs(10), || !alive(admin, pid))
}

/// The Store's generation (`history_meta.catalog_incarnation`).
pub fn generation(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one(
            "SELECT catalog_incarnation FROM history_meta WHERE id = 1",
            &[],
        )
        .unwrap()
        .get(0)
}

/// Moves the Store's generation on, as a compaction's install does.
pub fn bump_generation(admin: &mut postgres::Client) {
    admin
        .execute(
            "UPDATE history_meta SET catalog_incarnation = catalog_incarnation + 1 WHERE id = 1",
            &[],
        )
        .unwrap();
}

/// The backend pids of the server's connections to this database, except
/// the asking one.
pub fn backends(admin: &mut postgres::Client) -> Vec<i32> {
    admin
        .query(
            "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
             AND pid <> pg_backend_pid() AND backend_type = 'client backend' ORDER BY pid",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| row.get(0))
        .collect()
}

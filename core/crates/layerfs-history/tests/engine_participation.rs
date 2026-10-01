//! Generic C5 callback/profile boundaries; this test does not forge an engine guard.
mod support;
use layerfs_history::{
    sqlite::{self, EngineParticipation},
    HistoryCatalog, HistoryCatalogConfig, HistoryError, HistoryResult, ReserveRequest,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct Boundary {
    calls: AtomicUsize,
    fail_at: AtomicUsize,
}
impl Boundary {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(usize::MAX),
        }
    }
}
impl EngineParticipation for Boundary {
    fn validate(&self) -> HistoryResult<()> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call >= self.fail_at.load(Ordering::SeqCst) {
            Err(HistoryError::Unsupported(
                "external callback required boundary",
            ))
        } else {
            Ok(())
        }
    }
}
fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        cursor_key: [71; 32],
        binding_key: support::BINDING.to_vec(),
        incarnation: 7,
    }
}
#[test]
fn required_factory_callback_refuses_before_sqlite_path_creation() {
    let temp = support::Temp::new("engine-callback-refusal");
    let path = temp.join("catalog.sqlite");
    let boundary = Arc::new(Boundary::new());
    boundary.fail_at.store(1, Ordering::SeqCst);
    assert!(matches!(
        sqlite::create_participating(&path, &config(), boundary.clone()),
        Err(HistoryError::Unsupported(
            "external callback required boundary"
        ))
    ));
    assert_eq!(boundary.calls.load(Ordering::SeqCst), 1);
    assert!(!path.exists());
}
#[test]
fn known_prebegin_refusal_and_midtransaction_unknown_retain_exact_callback_cause() {
    let temp = support::Temp::new("engine-callback-transactions");
    for mid in [false, true] {
        let path = temp.join(if mid { "mid.sqlite" } else { "pre.sqlite" });
        let boundary = Arc::new(Boundary::new());
        let catalog = sqlite::create_participating(&path, &config(), boundary.clone()).unwrap();
        assert_eq!(
            boundary.calls.load(Ordering::SeqCst),
            5,
            "before/after main open, metadata entry, before/after actual schema-reference open"
        );
        let scope = support::root(61);
        let known = catalog
            .reserve_inodes(&ReserveRequest { scope, count: 3 })
            .unwrap();
        assert_eq!(known.start, 1);
        boundary.fail_at.store(
            boundary.calls.load(Ordering::SeqCst) + if mid { 2 } else { 1 },
            Ordering::SeqCst,
        );
        let failure = catalog
            .reserve_inodes(&ReserveRequest { scope, count: 2 })
            .unwrap_err();
        assert_eq!(
            failure,
            if mid {
                HistoryError::UnknownOutcome
            } else {
                HistoryError::Unsupported("external callback required boundary")
            }
        );
        assert_eq!(
            catalog.participation_failure().unwrap(),
            Some(HistoryError::Unsupported(
                "external callback required boundary"
            ))
        );
        assert_eq!(
            catalog.reserve_inodes(&ReserveRequest { scope, count: 1 }),
            Err(HistoryError::UnknownOutcome)
        );
        // The quarantined provider continues to own its native connection. An
        // independent reader verifies the last known reservation without repair.
        let independent = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let highwater: i64 = independent
            .query_row(
                "SELECT highwater FROM scope_allocator WHERE scope_id=?1",
                [scope.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            highwater, 3,
            "no failed callback transition acknowledged or leaked"
        );
        drop(independent);
        let readonly = sqlite::open_read_only(&path, support::BINDING, [71; 32]).unwrap();
        assert!(matches!(
            readonly.reserve_inodes(&ReserveRequest { scope, count: 1 }),
            Err(HistoryError::ContinuityUnavailable)
        ));
        assert_eq!(readonly.participation_failure().unwrap(), None);
        drop(readonly);
        drop(catalog);
    }
}

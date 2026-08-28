use super::*;

#[test]
fn verified_open_accounts_schema_profile_authority_and_admission_transaction() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-admission-accounting-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let fresh = Engine::open(&path).unwrap();
    let counters = fresh.counters().unwrap();
    assert_eq!(counters.admission_transactions_started, 1);
    assert_eq!(counters.admission_transactions_committed, 1);
    assert_eq!(counters.admission_transactions_rolled_back, 0);
    assert_eq!(counters.admission_statements, 98);
    assert_eq!(counters.store_id_queries, 1);
    assert_eq!(counters.transactions_started, 0);
    assert_eq!(counters.publication_transactions_started, 0);
    assert_eq!(counters.publication_commits, 0);
    drop(fresh);

    let reopened = Engine::open(&path).unwrap();
    let counters = reopened.counters().unwrap();
    assert_eq!(counters.admission_transactions_started, 1);
    assert_eq!(counters.admission_transactions_committed, 1);
    assert_eq!(counters.admission_transactions_rolled_back, 0);
    assert_eq!(counters.admission_statements, 96);
    assert_eq!(counters.store_id_queries, 1);
    assert_eq!(counters.transactions_started, 0);
    assert_eq!(counters.publication_transactions_started, 0);
    assert_eq!(counters.publication_commits, 0);
    drop(reopened);

    fs::remove_file(path).unwrap();
}

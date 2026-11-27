//! #768: a write transaction the server aborted is never reported as
//! committed.
//!
//! PostgreSQL answers the `COMMIT` of an aborted transaction by rolling it
//! back, with the command tag `ROLLBACK` and no error, so a client that sends
//! a bare `COMMIT` and reads only "no error" reports success for a
//! transaction that wrote nothing. Here a body runs a write, then a
//! statement the server refuses (`22012`), ignores the refusal and returns
//! success. The session must answer the refusal, the write must be absent,
//! and the session must stay usable (a definite outcome, not a quarantine).
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::support;

use layerfs_persistence::internals::{postgresql_write_ignoring, BackendError};
use support::pg::{url, Schema};

const BINDING: &[u8] = b"postgresql-aborted-commit";
const CURSOR: [u8; 32] = [77; 32];

#[test]
fn a_commit_of_an_aborted_transaction_is_not_reported_committed() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url.clone(), "abortcommit");
    let handles = schema.create(BINDING, CURSOR);
    // The probe table lives outside the Store's schema, which the open checks.
    let probe = format!("{}_t", schema.name);
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {probe} CASCADE; CREATE SCHEMA {probe}; \
             CREATE TABLE {probe}.t (k int8 PRIMARY KEY)"
        ))
        .unwrap();
    let insert = format!("INSERT INTO {probe}.t VALUES (1)");
    let outcome = postgresql_write_ignoring(&handles, &[&insert, "SELECT 1/0"]);
    let rows: i64 = admin
        .query_one(&format!("SELECT count(*) FROM {probe}.t"), &[])
        .unwrap()
        .get(0);
    assert_eq!(rows, 0, "the aborted transaction wrote nothing");
    assert_eq!(
        outcome,
        Err(BackendError::Integrity),
        "the refused statement is reported, not a committed transaction"
    );
    // A definite refusal: the session still serves, and a clean body commits.
    let insert_two = format!("INSERT INTO {probe}.t VALUES (2)");
    assert_eq!(postgresql_write_ignoring(&handles, &[&insert_two]), Ok(()));
    let rows: i64 = admin
        .query_one(&format!("SELECT count(*) FROM {probe}.t"), &[])
        .unwrap()
        .get(0);
    assert_eq!(rows, 1, "a clean body commits on the same session");
    drop(handles);
    admin
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {probe} CASCADE"))
        .unwrap();
    schema.drop_schema();
}

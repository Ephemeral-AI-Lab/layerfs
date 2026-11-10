//! Live schema objects, independently of the saved source/column fingerprint.
use super::{emit, Observation, Options, Outcome};
use crate::backend::{
    postgresql::session::transaction::Transaction,
    records::{BackendError, Param},
};

use super::postgresql_contract_expected::EXPECTED;
const OBJECTS: &str = include_str!("../../sql/postgresql/diagnostic_objects.sql");

pub(super) fn check(
    tx: &Transaction<'_>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let rows = tx.query(
        "SELECT (to_regclass(current_schema()||'.init_operation') IS NOT NULL)::int::bigint,current_setting('server_version_num')::bigint",
        vec![],
    )?;
    let acquisition = rows.first().ok_or(BackendError::Integrity)?.get::<i64>(0)? != 0;
    let major = rows.first().ok_or(BackendError::Integrity)?.get::<i64>(1)? / 10000;
    let mut checked = 0u64;
    let mut incomplete = false;
    let mut failed = 0u64;
    // The expected manifest is a bounded schema-format input. Actual definition
    // text stays on the server, including maliciously oversized function bodies.
    let sql = format!(
        "SELECT count(*)::bigint,COALESCE(bool_and(healthy<>0),false)::int::bigint,COALESCE(bool_and(btrim(regexp_replace(replace(definition,current_schema()||'.',''), '[[:space:]]+', ' ', 'g'))=$4),false)::int::bigint FROM ({}) actual WHERE kind=$1 AND relation=$2 AND name=$3",
        OBJECTS.trim().trim_end_matches(';')
    );
    for &(kind, table, name, definition) in EXPECTED {
        options.check()?;
        if !acquisition && table.starts_with("init_") {
            continue;
        }
        let rows = tx.query(
            &sql,
            vec![kind, table, name, definition]
                .into_iter()
                .map(|value| Param::Text(value.to_owned()))
                .collect(),
        )?;
        let row = rows.first().ok_or(BackendError::Integrity)?;
        checked += 1;
        if row.get::<i64>(0)? == 1
            && row.get::<i64>(1)? == 1
            && row.get::<i64>(2)? != 1
            && major != 17
        {
            incomplete = true;
            emit(observer,"store.schema.objects",Outcome::Unknown,format!("live schema object {kind} {table}/{name} definition differs from the PostgreSQL 17 schema-2 signature; server-major {major} deparse compatibility is unverified"));
        } else if row.get::<i64>(0)? != 1 || row.get::<i64>(1)? != 1 || row.get::<i64>(2)? != 1 {
            failed += 1;
            emit(observer,"store.schema.objects",Outcome::Fail,format!("live schema object {kind} {table}/{name} is absent, disabled, invalid or differs from the schema-2 contract; this is a schema/authority failure, not a content-corruption claim"));
        }
    }
    emit(observer,"store.schema.objects.summary",if failed>0{Outcome::Fail}else if incomplete{Outcome::Unknown}else{Outcome::Pass},format!("{checked} live constraints, indexes, triggers and immutable-pack trigger functions compared independently with the shipped schema-2 contract; {failed} differ"));
    Ok(())
}

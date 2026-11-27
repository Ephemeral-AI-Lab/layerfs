//! E: the upgrade chain runner (#804 §3 rules 1–5) with test-only steps on
//! a private SQLite file: steps run in order, a gap or a newer Store is
//! refused before anything runs, the chain is one transaction, a failing
//! step leaves nothing behind, and an unrecognized source is refused.
use crate::support;

use layerfs_persistence::internals::{upgrade_check_chain, upgrade_plan, upgrade_run, UpgradeStep};
use layerfs_persistence::upgrade::UpgradeError;
use rusqlite::Connection;
use support::Temp;

/// The engine's open transaction, as a test step sees it.
struct Tx {
    connection: Connection,
    applied: Vec<i64>,
    fail_at: Option<i64>,
    source: &'static str,
}

fn version(tx: &Tx) -> i64 {
    tx.connection
        .query_row("SELECT v FROM meta", [], |r| r.get(0))
        .unwrap()
}

/// Recognizes the source recorded for the version the step accepts.
fn recognize(tx: &mut Tx) -> Result<Option<&'static str>, UpgradeError> {
    let recorded: String = tx
        .connection
        .query_row("SELECT source FROM meta", [], |r| r.get(0))
        .unwrap();
    Ok((recorded == format!("{}:{}", tx.source, version(tx))).then_some("a"))
}

/// Adds a table named for the version and records the next version.
fn apply(tx: &mut Tx) -> Result<(), UpgradeError> {
    let from = version(tx);
    if tx.fail_at == Some(from) {
        return Err(UpgradeError::Failed(format!("step {from} failed")));
    }
    tx.connection
        .execute_batch(&format!(
            "CREATE TABLE t{from}(x INTEGER); UPDATE meta SET v={next}, source='{source}:{next}'",
            next = from + 1,
            source = tx.source
        ))
        .unwrap();
    tx.applied.push(from);
    Ok(())
}

fn step(from: i64) -> UpgradeStep<Tx> {
    UpgradeStep {
        from,
        to: from + 1,
        raw: &[(0, 0)],
        variants: &["a"],
        recognize,
        apply,
    }
}

fn store(temp: &Temp, at: i64) -> (std::path::PathBuf, Tx) {
    let path = temp.join("chain.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(&format!(
            "PRAGMA journal_mode=WAL; CREATE TABLE meta(v INTEGER, source TEXT); \
             INSERT INTO meta VALUES({at}, 'src:{at}')"
        ))
        .unwrap();
    (
        path,
        Tx {
            connection,
            applied: Vec::new(),
            fail_at: None,
            source: "src",
        },
    )
}

fn tables(path: &std::path::Path) -> Vec<String> {
    let c = Connection::open(path).unwrap();
    let mut s = c
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap();
    let names = s
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    names
}

#[test]
fn steps_run_in_order_in_one_transaction() {
    let temp = Temp::new("chain-order");
    let (path, mut tx) = store(&temp, 1);
    // Given out of order: the runner orders them by version.
    let steps = [step(3), step(1), step(2)];
    tx.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let names = upgrade_run(&mut tx, &steps, 1, 4).unwrap();
    assert_eq!(names, ["1->2", "2->3", "3->4"]);
    assert_eq!(tx.applied, [1, 2, 3]);
    // Nothing is visible to another connection before the one COMMIT.
    assert_eq!(tables(&path), ["meta"]);
    tx.connection.execute_batch("COMMIT").unwrap();
    assert_eq!(tables(&path), ["meta", "t1", "t2", "t3"]);
    assert_eq!(version(&tx), 4);
}

#[test]
fn a_store_at_the_latest_runs_nothing() {
    let temp = Temp::new("chain-current");
    let (_, mut tx) = store(&temp, 4);
    let steps = [step(1), step(2), step(3)];
    assert!(upgrade_run(&mut tx, &steps, 4, 4).unwrap().is_empty());
    assert!(tx.applied.is_empty());
}

#[test]
fn a_gap_is_refused_before_any_step_runs() {
    let temp = Temp::new("chain-gap");
    let (_, mut tx) = store(&temp, 1);
    let steps = [step(1), step(3)];
    let error = upgrade_run(&mut tx, &steps, 1, 4).unwrap_err();
    let UpgradeError::Unsupported(text) = error else {
        panic!("{error:?}")
    };
    assert!(text.contains("format 2 has no upgrade step"), "{text}");
    assert!(tx.applied.is_empty());
    // A Store already past the gap still reaches the latest.
    assert_eq!(upgrade_plan(&steps, 3, 4).unwrap().len(), 1);
}

#[test]
fn a_newer_store_is_refused_as_newer() {
    let temp = Temp::new("chain-newer");
    let (_, mut tx) = store(&temp, 5);
    let error = upgrade_run(&mut tx, &[step(1)], 5, 4).unwrap_err();
    let UpgradeError::Unsupported(text) = error else {
        panic!("{error:?}")
    };
    assert!(
        text.contains("newer than this build") && !text.contains("outdated"),
        "{text}"
    );
}

#[test]
fn an_unrecognized_source_is_refused_unchanged() {
    let temp = Temp::new("chain-source");
    let (path, mut tx) = store(&temp, 1);
    tx.source = "other";
    tx.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = upgrade_run(&mut tx, &[step(1), step(2), step(3)], 1, 4).unwrap_err();
    assert!(matches!(error, UpgradeError::Unsupported(_)), "{error:?}");
    assert!(tx.applied.is_empty());
    tx.connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(tables(&path), ["meta"]);
}

#[test]
fn a_failing_step_never_stops_the_store_in_the_middle() {
    let temp = Temp::new("chain-middle");
    let (path, mut tx) = store(&temp, 1);
    tx.fail_at = Some(3);
    tx.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = upgrade_run(&mut tx, &[step(1), step(2), step(3)], 1, 4).unwrap_err();
    assert_eq!(error, UpgradeError::Failed("step 3 failed".to_owned()));
    assert_eq!(tx.applied, [1, 2]);
    // The engine's failure path: one ROLLBACK undoes every step.
    tx.connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(tables(&path), ["meta"]);
    assert_eq!(version(&tx), 1);
}

#[test]
fn a_later_step_recognizes_the_previous_target_exactly() {
    let temp = Temp::new("chain-target");
    let (_, mut tx) = store(&temp, 1);
    // Step 2's statements leave a source step 3 does not accept.
    fn skewed(tx: &mut Tx) -> Result<(), UpgradeError> {
        tx.connection
            .execute_batch("CREATE TABLE t2(x INTEGER); UPDATE meta SET v=3, source='drift:3'")
            .unwrap();
        tx.applied.push(2);
        Ok(())
    }
    let mut bad = step(2);
    bad.apply = skewed;
    tx.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = upgrade_run(&mut tx, &[step(1), bad, step(3)], 1, 4).unwrap_err();
    assert!(matches!(error, UpgradeError::Unsupported(_)), "{error:?}");
    assert_eq!(tx.applied, [1, 2]);
    tx.connection.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn the_chain_guard_names_each_broken_rule() {
    let keep = |v: i64| Some((v / 10, v % 10));
    let mapped = |from: i64, raw: &'static [(i64, i64)]| UpgradeStep::<Tx> {
        from,
        to: from + 1,
        raw,
        variants: &["a"],
        recognize,
        apply,
    };
    let good = [
        mapped(1, &[(10, 20), (11, 21)]),
        mapped(2, &[(20, 30), (21, 31)]),
    ];
    assert_eq!(upgrade_check_chain(&good, 1, 3, keep), Ok(()));
    let gap = [mapped(1, &[(10, 20)])];
    assert!(upgrade_check_chain(&gap, 1, 3, keep)
        .unwrap_err()
        .contains("gap"));
    let layout = [mapped(1, &[(10, 21)]), mapped(2, &[(20, 30)])];
    assert!(upgrade_check_chain(&layout, 1, 3, keep)
        .unwrap_err()
        .contains("changes acquisition or layout"));
    let skip = [mapped(1, &[(10, 30)]), mapped(2, &[(20, 30)])];
    assert!(upgrade_check_chain(&skip, 1, 3, keep).is_err());
}

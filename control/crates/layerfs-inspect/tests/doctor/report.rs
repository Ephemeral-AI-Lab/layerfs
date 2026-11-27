//! The observations a worker sends are bounded: retained rows, evidence
//! and the budget a probe may use.
use layerfs_inspect::doctor::budget::{Budget, Checks};
use layerfs_protocol::sdk::SdkValue;
use std::time::Duration;

#[test]
fn report_retention_is_bounded() {
    let mut checks = Checks::default();
    for _ in 0..1100 {
        checks.pass("object.read", "checked");
    }
    let report = checks.to_wire();
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks missing")
    };
    assert_eq!(rows.len(), 1024);
    assert_eq!(report.get("truncated"), Some(&SdkValue::Bool(true)));
}

#[test]
fn dropped_repeated_check_cannot_replace_retained_object_evidence() {
    let mut checks = Checks::default();
    for index in 0..1024 {
        checks.pass("object.read", "checked");
        checks.evidence("object.read", SdkValue::Int(index));
    }
    checks.fail(
        "object.read",
        "different object failed after retention limit",
    );
    checks.evidence("object.read", SdkValue::text("unretained-object"));
    checks.fix("object.read", "unretained fix");
    let report = checks.to_wire();
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks missing")
    };
    let last = rows.last().unwrap();
    assert_eq!(last.get("evidence"), Some(&SdkValue::Int(1023)));
    assert_eq!(last.get_text("fix"), Some(""));
    assert_eq!(last.get_text("outcome"), Some("pass"));
}

#[test]
fn oversized_evidence_does_not_expand_the_report() {
    let mut checks = Checks::default();
    checks.pass("inventory", "read");
    checks.evidence("inventory", SdkValue::text("x".repeat(20_000)));
    let report = checks.to_wire();
    assert_eq!(report.get("truncated"), Some(&SdkValue::Bool(true)));
    assert!(layerfs_home::json::print(&report).len() < 4096);
}

#[test]
fn probe_limit_never_extends_the_overall_budget() {
    let budget = Budget::new(Some(Duration::from_millis(50)));
    assert!(budget.limit(Duration::from_secs(10)) <= Duration::from_millis(50));
    assert!(Budget::new(Some(Duration::ZERO)).expired());
}

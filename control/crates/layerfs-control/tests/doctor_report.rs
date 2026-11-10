//! Reports must distinguish incomplete evidence from established failure.
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::target::{Budget, Checks};
use std::time::Duration;

#[test]
fn unknown_and_blocked_are_incomplete_without_inventing_an_error() {
    let mut checks = Checks::new("sandbox", "example");
    checks.unknown("worker.reach", "worker did not answer");
    checks.blocked("daemon.ready", "worker required", "worker.reach");
    let report = checks.to_sdk();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    assert_eq!(report.get("complete"), Some(&SdkValue::Bool(false)));
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks missing")
    };
    assert_eq!(rows[0].get_text("outcome"), Some("unknown"));
    assert_eq!(
        rows[1].get("blocked_by"),
        Some(&SdkValue::List(vec![SdkValue::text("worker.reach")]))
    );
}

#[test]
fn confirmed_corruption_survives_later_deadline() {
    let mut checks = Checks::new("store", "example");
    checks.fail("object.identity", "content identity mismatch");
    checks.unknown("diagnostic.deadline", "not all objects inspected");
    let report = checks.to_sdk();
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(report.get("complete"), Some(&SdkValue::Bool(false)));
    let Some(SdkValue::List(findings)) = report.get("findings") else {
        panic!("findings missing")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].get_text("check_id"), Some("object.identity"));
}

#[test]
fn report_retention_is_bounded_and_totals_are_preserved() {
    let mut checks = Checks::new("store", "example");
    for _ in 0..1100 {
        checks.pass("object.read", "checked");
    }
    let report = checks.to_sdk();
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks missing")
    };
    assert_eq!(rows.len(), 1024);
    assert_eq!(report.get("truncated"), Some(&SdkValue::Bool(true)));
    assert_eq!(report.get("complete"), Some(&SdkValue::Bool(false)));
    assert_eq!(report.get("counts").unwrap().get_int("pass"), Some(1100));
}

#[test]
fn dropped_repeated_check_cannot_replace_retained_object_evidence() {
    let mut checks = Checks::new("store", "example");
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
    checks.duration("object.read", Duration::from_secs(1));
    let report = checks.to_sdk();
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks missing")
    };
    let last = rows.last().unwrap();
    assert_eq!(last.get("evidence"), Some(&SdkValue::Int(1023)));
    assert_eq!(last.get_text("fix"), Some(""));
    assert_eq!(last.get("duration_ms"), Some(&SdkValue::Null));
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(report.get("counts").unwrap().get_int("fail"), Some(1));
}

#[test]
fn oversized_evidence_does_not_expand_the_report() {
    let mut checks = Checks::new("store", "example");
    checks.pass("inventory", "read");
    checks.evidence("inventory", SdkValue::text("x".repeat(20_000)));
    let report = checks.to_sdk();
    assert_eq!(report.get("truncated"), Some(&SdkValue::Bool(true)));
    assert!(layerfs_home::json::print(&report).len() < 4096);
}

#[test]
fn probe_limit_never_extends_the_overall_budget() {
    let budget = Budget::new(Some(Duration::from_millis(50)));
    assert!(budget.limit(Duration::from_secs(10)) <= Duration::from_millis(50));
    assert!(Budget::new(Some(Duration::ZERO)).expired());
}

//! The readable doctor form prints findings worst first (frozen
//! `doctor.worst_first`: error, warning, notice), report order within one
//! severity; the count line is unchanged.
use layerfs_bridge::control::SdkValue;
use layerfs_cli::output::doctor_text;

fn finding(severity: &str, check: &str) -> SdkValue {
    SdkValue::map([
        ("home", SdkValue::text("h")),
        ("check", SdkValue::text(check)),
        ("severity", SdkValue::text(severity)),
        ("message", SdkValue::text(format!("{check} message"))),
    ])
}

#[test]
fn findings_print_worst_first_and_stable_within_a_severity() {
    let report = SdkValue::map([(
        "findings",
        SdkValue::List(vec![
            finding("notice", "a.notice"),
            finding("error", "b.error1"),
            finding("warning", "c.warning"),
            finding("error", "d.error2"),
        ]),
    )]);
    let text = doctor_text(&report);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "2 error(s), 1 warning(s), 1 notice(s), 0 not checked"
    );
    let order: Vec<&str> = lines[1..]
        .iter()
        .map(|l| l.split(' ').find(|w| w.ends_with(':')).unwrap())
        .collect();
    assert_eq!(
        order,
        ["b.error1:", "d.error2:", "c.warning:", "a.notice:"],
        "{text}"
    );
}

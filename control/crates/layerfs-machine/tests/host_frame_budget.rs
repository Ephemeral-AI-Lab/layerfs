//! The CLI's value-based budget check (#819 round 5) agrees with the
//! worker's check of the same value's printed JSON, at and past each
//! bound, so a list the CLI streams is never refused for size.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json;
use layerfs_machine::worker::{value_within_budget, within_budget};

fn agrees(value: &SdkValue) -> bool {
    let printed = within_budget(json::print(value).as_bytes());
    assert_eq!(value_within_budget(value), printed, "{value:?}");
    printed
}

fn nested(levels: usize) -> SdkValue {
    (0..levels).fold(SdkValue::Null, |inner, _| SdkValue::List(vec![inner]))
}

fn object(fields: usize) -> SdkValue {
    SdkValue::Map(
        (0..fields)
            .map(|n| (format!("f{n}"), SdkValue::Int(n as i64)))
            .collect(),
    )
}

#[test]
fn the_value_check_matches_the_printed_check_at_every_bound() {
    let small = SdkValue::map([
        ("op", SdkValue::text("edit")),
        (
            "oldText",
            SdkValue::text("a \"quoted\"\\ line\n\twith\r\u{1} controls é"),
        ),
        ("newText", SdkValue::Bytes(vec![0xff, 0, 1, 2])),
        ("replaceAll", SdkValue::Bool(false)),
        ("mode", SdkValue::Null),
        ("n", SdkValue::Int(-12345)),
        (
            "list",
            SdkValue::List(vec![SdkValue::Bool(true), SdkValue::text("")]),
        ),
    ]);
    assert!(agrees(&small));
    // depth: 32 levels pass, 33 do not
    assert!(agrees(&nested(32)));
    assert!(!agrees(&nested(33)));
    // width: 128 fields pass, 129 do not
    assert!(agrees(&object(128)));
    assert!(!agrees(&object(129)));
    // size: around the 4 MiB decoded estimate, with escapes counted
    let around =
        |len: usize, fill: &str| SdkValue::map([("oldText", SdkValue::text(fill.repeat(len)))]);
    let mut fits = 0;
    for len in (1_397_850..1_398_000).step_by(7) {
        fits += usize::from(agrees(&around(len, "x")));
    }
    assert!(fits > 0 && fits < 22, "the bound was not crossed");
    for len in (699_000..699_100).step_by(3) {
        agrees(&around(len, "\n"));
    }
    for len in (232_990..233_060).step_by(2) {
        agrees(&around(len, "\u{2}"));
    }
    let bytes = |len: usize| SdkValue::map([("bytes", SdkValue::Bytes(vec![0xff; len]))]);
    for len in (1_048_000..1_048_700).step_by(31) {
        agrees(&bytes(len));
    }
}

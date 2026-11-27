//! The CLI's value-based budget check (#819 rounds 5 and 6) agrees with
//! the worker's check of the change frame it decodes, `{"change": value}`
//! printed, at and past each bound, so a list the CLI streams is never
//! refused for size.
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::json;
use layerfs_machine::worker::{value_within_budget, within_budget};

/// The change frame's JSON text the worker checks.
fn framed(value: &SdkValue) -> String {
    json::print(&SdkValue::map([("change", value.clone())]))
}

fn agrees(value: &SdkValue) -> bool {
    let printed = within_budget(framed(value).as_bytes());
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
    // depth: the frame is one of the 32 levels, so 31 pass, 32 do not
    assert!(agrees(&nested(31)));
    assert!(!agrees(&nested(32)));
    // width: 128 fields pass, 129 do not
    assert!(agrees(&object(128)));
    assert!(!agrees(&object(129)));
    // size: around the 4 MiB decoded estimate, with escapes counted
    let around =
        |len: usize, fill: &str| SdkValue::map([("oldText", SdkValue::text(fill.repeat(len)))]);
    let mut fits = 0;
    // `{"change":{"oldText":"x…"}}`: 3 × (len + 25) + 4 × 256 <= 4 MiB,
    // so len <= 1,397,735
    for len in (1_397_650..1_397_800).step_by(7) {
        fits += usize::from(agrees(&around(len, "x")));
    }
    assert_eq!(fits, 13, "the bound was not crossed where computed");
    // the frame's own cost counts: the same change unframed would fit
    let past = around(1_397_736, "x");
    assert!(!value_within_budget(&past));
    assert!(within_budget(json::print(&past).as_bytes()));
    assert!(!within_budget(framed(&past).as_bytes()));
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

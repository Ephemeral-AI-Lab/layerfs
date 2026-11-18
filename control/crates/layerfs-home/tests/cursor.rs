//! The one cursor of the call surface (2.0): it carries its call, its place
//! and the filters and pins of the call that made it, and is refused for
//! another call, for a filter given differently and when it is no cursor.
use layerfs_bridge::control::SdkValue;
use layerfs_home::cursor::{limit, make, resume, DEFAULT_LIMIT, MAX_LIMIT};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

#[test]
fn a_cursor_carries_its_place_filters_and_pins() {
    let made = make(
        "commit.list",
        t("c0ffee"),
        &[
            ("branch", t("agent")),
            ("given_at", SdkValue::Null),
            ("pin", t("aa11")),
        ],
    );
    assert!(!made.ends_with('='), "unpadded: {made}");
    // the same filters, or none, resume it
    for given in [
        vec![("branch", t("agent"))],
        vec![],
        vec![("given_at", SdkValue::Null)],
    ] {
        let carried = resume("commit.list", &made, &given).unwrap();
        assert_eq!(carried.place(), &t("c0ffee"));
        assert_eq!(carried.text("branch"), Some("agent"));
        assert_eq!(carried.text("pin"), Some("aa11"));
        // a field the call was not given is not carried
        assert_eq!(carried.get("given_at"), SdkValue::Null);
    }
}

#[test]
fn another_call_another_filter_or_no_cursor_is_refused() {
    let made = make("branch.list", t("dsh.b"), &[("prefix", t("dsh."))]);
    let refused = |call: &str, text: &str, given: &[(&str, SdkValue)]| {
        let refusal = resume(call, text, given).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
        refusal.message
    };
    let other = refused("commit.list", &made, &[]);
    assert!(
        other.contains("not the `next_cursor` of a commit.list page"),
        "{other}"
    );
    let filter = refused("branch.list", &made, &[("prefix", t("other."))]);
    assert!(filter.contains("give the same filters or none"), "{filter}");
    // a filter the cursor does not carry is a different filter too
    refused("branch.list", &made, &[("exclude_prefix", t("x"))]);
    for text in ["", "not base64!", "bm90IGpzb24", &format!("!{made}")] {
        refused("branch.list", text, &[]);
    }
}

#[test]
fn a_limit_is_one_to_a_thousand_with_its_default() {
    assert_eq!(limit("x.y", None, DEFAULT_LIMIT).unwrap(), 100);
    assert_eq!(limit("x.y", None, 200).unwrap(), 200);
    assert_eq!(limit("x.y", Some(MAX_LIMIT), DEFAULT_LIMIT).unwrap(), 1000);
    for bad in [0, -1, MAX_LIMIT + 1] {
        let refusal = limit("x.y", Some(bad), DEFAULT_LIMIT).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert_eq!(refusal.message, "x.y: `limit` is 1 to 1000");
    }
}

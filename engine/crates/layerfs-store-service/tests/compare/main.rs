mod diff_entries;
mod diff_pages;
mod diff_replay;
mod diff_resume;
mod diff_size;
mod file_compare_budget;
mod history_diff_oracle;
#[allow(dead_code)]
#[path = "../support/installed_store.rs"]
mod installed;
mod layer_diff;
#[allow(dead_code)]
#[path = "../support/sdk.rs"]
mod support;
mod wide_namespaces;

use layerfs_protocol::sdk::SdkValue;

/// One page of `commit.diff` from `from` to `to` (each a Branch, main or a
/// Commit id), after the path `after` (`Null` for the first page), at most
/// `limit` items, with line hunks when `hunks`.
fn commit_diff(
    fixture: &support::Fixture,
    from: &str,
    to: &str,
    limit: i64,
    after: &SdkValue,
    hunks: bool,
) -> SdkValue {
    let mut fields = vec![
        ("ref", SdkValue::text(to)),
        ("from", SdkValue::text(from)),
        ("limit", SdkValue::Int(limit)),
        ("hunks", SdkValue::Bool(hunks)),
    ];
    if !matches!(after, SdkValue::Null) {
        fields.push(("after", after.clone()));
    }
    fixture.call("commit.diff", fields)
}

/// Whether a `commit.diff` page has a page after it.
fn more(page: &SdkValue) -> bool {
    !matches!(page.get("next"), None | Some(SdkValue::Null))
}

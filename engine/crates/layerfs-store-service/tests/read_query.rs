//! Numbered page admission does not turn bounded replay into missing results.
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::read::query::{result_page, Work, MAXIMUM_QUERY_WORK};

fn args(fields: &[(&str, i64)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), SdkValue::Int(*v)))
            .collect(),
    )
}

#[test]
fn numbered_pages_count_result_offsets_and_validate_conflicting_legacy_options() {
    let page = result_page(&args(&[("page", 3), ("pageSize", 7)])).unwrap();
    assert_eq!((page.number, page.limit, page.skip), (3, 7, 14));
    for fields in [
        vec![("page", 0)],
        vec![("page", -1)],
        vec![("pageSize", 1001)],
        vec![("page", 1), ("pagePos", 0)],
    ] {
        assert_eq!(
            result_page(&args(&fields)).unwrap_err().code,
            "InvalidInput"
        );
    }
    let cursor = SdkValue::map([("cursor", SdkValue::text("/a"))]);
    assert_eq!(result_page(&cursor).unwrap_err().code, "InvalidInput");
    assert_eq!(
        result_page(&args(&[("page", 1001), ("pageSize", 10)]))
            .unwrap_err()
            .code,
        "Capacity"
    );
}

#[test]
fn exhausted_query_refuses_instead_of_pretending_a_partial_page_is_complete() {
    let mut work = Work::default();
    for _ in 0..MAXIMUM_QUERY_WORK {
        work.step().unwrap();
    }
    assert_eq!(work.examined(), MAXIMUM_QUERY_WORK);
    assert_eq!(work.step().unwrap_err().code, "Capacity");
    assert_eq!(work.examined(), MAXIMUM_QUERY_WORK);
}

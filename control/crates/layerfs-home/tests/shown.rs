//! An answer's list and its `<list>_count` companion (call surface 2.0
//! SCHEMA, conformance R1 F2-F4): the list keeps its first 20, and the
//! count is set only when the list was cut.
use layerfs_home::helpers::{shown, SHOWN};
use layerfs_protocol::sdk::SdkValue;

fn names(n: usize) -> Vec<SdkValue> {
    (0..n).map(|i| SdkValue::text(format!("n{i}"))).collect()
}

fn len(list: &SdkValue) -> usize {
    match list {
        SdkValue::List(items) => items.len(),
        other => panic!("not a list: {other:?}"),
    }
}

#[test]
fn the_count_is_set_only_beside_a_cut_list() {
    // none, one and exactly 20: whole lists, no count
    for n in [0, 1, SHOWN] {
        let (list, count) = shown(names(n), 0, false);
        assert_eq!(len(&list), n);
        assert_eq!(count, SdkValue::Null, "{n}");
    }
    // 21: the first 20 and the count of all
    let (list, count) = shown(names(SHOWN + 1), 0, false);
    assert_eq!(len(&list), SHOWN);
    assert_eq!(list, SdkValue::List(names(SHOWN)));
    assert_eq!(count, SdkValue::Int(21));
    // the source knew more than it sent: its total
    let (list, count) = shown(names(3), 57, false);
    assert_eq!(len(&list), 3);
    assert_eq!(count, SdkValue::Int(57));
    // the source cut without a total: what is known
    let (_, count) = shown(names(2), 0, true);
    assert_eq!(count, SdkValue::Int(2));
}

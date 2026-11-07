//! A list's page (layerfs-project#108): `page_size` defaults to 50 and is
//! held to 1..=1000, `page_pos` to 0 and up, and the next position is
//! given only while items remain, as the untyped page gave them.
use layerfs_sandbox::family::{page, Page};

#[test]
fn a_page_is_cut_and_its_next_position_given_as_before() {
    let items: Vec<u32> = (0..120).collect();
    let (first, next) = page(items.clone(), Page::default());
    assert_eq!((first.len(), first[0], next), (50, 0, Some(50)));
    let (last, next) = page(
        items.clone(),
        Page {
            size: Some(50),
            pos: Some(100),
        },
    );
    assert_eq!((last.len(), next), (20, None));
    let (one, next) = page(
        items.clone(),
        Page {
            size: Some(0),
            pos: Some(-3),
        },
    );
    assert_eq!((one, next), (vec![0], Some(1)));
    let (all, next) = page(
        items,
        Page {
            size: Some(5000),
            pos: None,
        },
    );
    assert_eq!((all.len(), next), (120, None));
}

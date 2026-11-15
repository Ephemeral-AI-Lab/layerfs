//! project.delete on a Store: the marker hides the project from name
//! resolution, and the retry classifies retired/live/absent by stack id.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_history::{HistoryName, LayerStackId, LayerStackRecord};
use layerfs_store_service::history::deletion::{delete_stack, Deletion};
use support::Fixture;

fn fixture() -> Fixture {
    Fixture::new("project-delete", |source| {
        std::fs::write(
            source.join("a"),
            "base
",
        )
        .unwrap();
        1
    })
}

fn installed(f: &Fixture) -> LayerStackRecord {
    f.service
        .store
        .history()
        .layer_stack_named(&HistoryName::new("installed").unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn delete_marks_once_and_hides_the_project() {
    let f = fixture();
    let shop = installed(&f);

    // A live stack with no marker is marked with the call's deleted_ms.
    assert_eq!(
        delete_stack(&f.service.store, shop.id, 1234).unwrap(),
        Deletion::Marked { deleted_ms: 1234 }
    );
    assert_eq!(
        f.service.store.history().stack_deleted_ms(shop.id).unwrap(),
        Some(1234)
    );

    // A hidden project resolves to NoSuchProject on every Store call.
    for method in ["project.list_branches", "project.stats"] {
        let refused = f.try_call(method, vec![]).unwrap_err();
        assert_eq!(refused.code, "NoSuchProject", "{method}: {refused:?}");
    }
    assert!(f
        .service
        .store
        .history()
        .layer_stacks(&layerfs_history::Page {
            cursor: None,
            limit: 10,
        })
        .unwrap()
        .records
        .is_empty());

    // A retry by the same stack id reuses the marker's deleted_ms.
    assert_eq!(
        delete_stack(&f.service.store, shop.id, 999).unwrap(),
        Deletion::Retired { deleted_ms: 1234 }
    );
    assert_eq!(Deletion::Retired { deleted_ms: 1234 }.deleted_ms(999), 1234);

    // An absent stack lets the caller finish its stale entry.
    let absent = LayerStackId::from_authority([9; 16]);
    assert_eq!(
        delete_stack(&f.service.store, absent, 7).unwrap(),
        Deletion::Absent
    );
    assert_eq!(Deletion::Absent.deleted_ms(7), 7);
    f.cleanup();
}

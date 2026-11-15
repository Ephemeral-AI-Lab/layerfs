//! project.delete: the layer_stack_retired marker, the visibility filter every
//! project lookup carries, and the retry classification the Store side needs.
mod support;

use layerfs_history::error::Missing;
use layerfs_history::{
    HistoryCatalog, HistoryError, LayerStackId, LayerStackRecord, Page, StackInitialization,
};
use support::{name, root, stack, Temp};

fn project(history: &dyn HistoryCatalog, n: u8, text: &str) -> LayerStackRecord {
    history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(n),
            name: name(text),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap()
}

fn projects(history: &dyn HistoryCatalog) -> Vec<String> {
    let mut out = Vec::new();
    let mut page = Page {
        cursor: None,
        limit: 1,
    };
    loop {
        let found = history.layer_stacks(&page).unwrap();
        out.extend(
            found
                .records
                .into_iter()
                .map(|record| record.name.as_str().to_owned()),
        );
        match found.continuation {
            Some(cursor) => page.cursor = Some(cursor),
            None => return out,
        }
    }
}

fn epoch(path: &std::path::Path, id: LayerStackId) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            "SELECT epoch FROM project_epoch WHERE layer_stack_id = ?1",
            [id.as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn deleting_a_project_hides_it_and_marks_once() {
    let temp = Temp::new("project-delete");
    let path = temp.join("store");
    let history = support::create(&path);
    let shop = project(&history, 1, "shop");
    let api = project(&history, 2, "api");
    assert_eq!(projects(&history), ["api", "shop"]);

    history.mark_stack_deleted(shop.id, 1234).unwrap();
    assert_eq!(history.stack_deleted_ms(shop.id).unwrap(), Some(1234));

    // Hidden from every project lookup, and only from the deleted one.
    assert_eq!(history.layer_stack(shop.id).unwrap(), None);
    assert_eq!(history.layer_stack_named(&name("shop")).unwrap(), None);
    assert_eq!(projects(&history), ["api"]);
    assert!(history.layer_stack(api.id).unwrap().is_some());
    assert!(history.layer_stack_named(&name("api")).unwrap().is_some());

    // A repeat writes nothing: the marker and its deleted_ms stay, and an
    // absent stack is the same typed absence.
    assert_eq!(
        history.mark_stack_deleted(shop.id, 999),
        Err(HistoryError::Missing(Missing::LayerStack))
    );
    assert_eq!(history.stack_deleted_ms(shop.id).unwrap(), Some(1234));
    assert_eq!(
        history.mark_stack_deleted(stack(9), 1),
        Err(HistoryError::Missing(Missing::LayerStack))
    );
    drop(history);

    // The marker transaction bumped the project epoch exactly once.
    assert_eq!(epoch(&path, shop.id), 1);
    assert_eq!(epoch(&path, api.id), 0);
}

#[test]
fn retry_classifies_retired_live_and_absent() {
    let temp = Temp::new("project-delete-retry");
    let history = support::create(&temp.join("store"));
    let shop = project(&history, 1, "shop");

    // Live with no marker: the retry redoes the marker with its own deleted_ms.
    assert_eq!(history.stack_deleted_ms(shop.id).unwrap(), None);
    assert!(history.layer_stack(shop.id).unwrap().is_some());

    // Retired: the retry reuses the recorded deleted_ms.
    history.mark_stack_deleted(shop.id, 77).unwrap();
    assert_eq!(history.stack_deleted_ms(shop.id).unwrap(), Some(77));
    assert_eq!(history.layer_stack(shop.id).unwrap(), None);

    // Absent: neither a live row nor a marker, so the caller finishes its entry.
    assert_eq!(history.stack_deleted_ms(stack(9)).unwrap(), None);
    assert_eq!(history.layer_stack(stack(9)).unwrap(), None);
}

#[test]
fn paging_skips_a_hidden_project() {
    let temp = Temp::new("project-delete-paging");
    let history = support::create(&temp.join("store"));
    for (n, text) in [(1, "a"), (2, "b"), (3, "c")] {
        project(&history, n, text);
    }
    history.mark_stack_deleted(stack(2), 5).unwrap();
    // Two windows of one record skip the hidden middle project.
    let mut page = Page {
        cursor: None,
        limit: 1,
    };
    let mut names = Vec::new();
    loop {
        let found = history.layer_stacks(&page).unwrap();
        names.extend(
            found
                .records
                .into_iter()
                .map(|record| record.name.as_str().to_owned()),
        );
        match found.continuation {
            Some(cursor) => page.cursor = Some(cursor),
            None => break,
        }
    }
    assert_eq!(names, ["a", "c"]);
}

#[test]
fn every_layer_stack_lookup_carries_the_marker_filter() {
    for (file, sql) in [
        (
            "layerstack_stack_by_id.sql",
            include_str!("../sql/sqlite/queries/history/layerstack_stack_by_id.sql"),
        ),
        (
            "layerstack_stack_by_name.sql",
            include_str!("../sql/sqlite/queries/history/layerstack_stack_by_name.sql"),
        ),
        (
            "layerstack_stack_first_page.sql",
            include_str!("../sql/sqlite/queries/history/layerstack_stack_first_page.sql"),
        ),
        (
            "layerstack_stack_next_page.sql",
            include_str!("../sql/sqlite/queries/history/layerstack_stack_next_page.sql"),
        ),
    ] {
        assert!(sql.contains("layer_stack_retired"), "{file}: {sql}");
        assert!(sql.contains("NOT EXISTS"), "{file}: {sql}");
    }
    // PostgreSQL overrides the by-name statement because it names the schema
    // in one statement; it carries the same filter.
    let postgresql = include_str!("../sql/postgresql/queries/history/layerstack_stack_by_name.sql");
    assert!(postgresql.contains("layer_stack_retired"), "{postgresql}");
    assert!(postgresql.contains("NOT EXISTS"), "{postgresql}");
    // The name-collision check is allow-listed: it must see a retired holder
    // so project.init/fork can answer NameTaken with its state.
    let taken = include_str!("../sql/sqlite/queries/history/layerstack_stack_name_taken.sql");
    assert!(taken.contains("layer_stack_retired"), "{taken}");
    assert!(
        taken.contains("CASE WHEN"),
        "the holder state is returned: {taken}"
    );
}

#[test]
fn doctor_accepts_a_retired_stack() {
    use layerfs_persistence::diagnostic::Outcome;
    let fixture = support::doctor::Fixture::new();
    let shop = project(&fixture.handles.history, 1, "shop");
    fixture
        .handles
        .history
        .mark_stack_deleted(shop.id, 1234)
        .unwrap();
    let observations = fixture.audit(false);
    assert!(
        !observations
            .iter()
            .any(|observation| observation.outcome == Outcome::Fail),
        "{observations:?}"
    );
    // The marker table joins the audited schema and the history is still whole.
    for id in [
        "store.schema",
        "store.database.foreign_keys",
        "store.history.summary",
    ] {
        assert!(
            observations
                .iter()
                .any(|observation| observation.id == id && observation.outcome == Outcome::Pass),
            "missing {id}: {observations:?}"
        );
    }
}

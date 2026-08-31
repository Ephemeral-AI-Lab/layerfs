use layerfs_sdk::{
    Client, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource, Query,
    QueryItem, QueryKind,
};
use std::sync::Arc;

#[test]
fn typed_keyset_queries_page_every_local_record() {
    let root = temp();
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite")).unwrap());
    let client = Client::connect(store.clone()).unwrap();
    for name in ["alpha", "beta"] {
        let initialized = client
            .initialize_layerstack(
                EntityName::new(name).unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap();
        client
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer {
                    layer_id: initialized.genesis_layer_id,
                },
            )
            .unwrap();
    }
    let query = Query::new(QueryKind::LayerStacks).limit(1);
    let first = client.query(query.clone()).unwrap();
    assert_eq!(first.items.len(), 1);
    let second = client
        .query(first.into_next_query(&query).expect("continuation"))
        .unwrap();
    assert_eq!(second.items.len(), 1);
    let branches = client.query(Query::new(QueryKind::Branches)).unwrap();
    assert_eq!(branches.items.len(), 2);
    assert!(branches
        .items
        .iter()
        .all(|item| matches!(item, QueryItem::Branch(_))));

    drop(client);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn temp() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-v4-query-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

use layerfs_sdk::{
    BranchStore, Client, ConnectionContext, EntityName, LayerStackEndpoint,
    LayerStackInitialization, LayerStackStore, LocalForkSource, Query, QueryItem, QueryKind,
    RemotePlacement,
};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn named_query_pages_expose_every_record_and_validate_cursors() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-v3-query-pages-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })
    .unwrap();

    let first = client
        .initialize_layerstack(name("project-000"), LayerStackInitialization::Empty)
        .unwrap();
    for index in 1..520 {
        client
            .initialize_layerstack(
                name(&format!("project-{index:03}")),
                LayerStackInitialization::Empty,
            )
            .unwrap();
    }

    let projects = collect(
        &client,
        Query::new(QueryKind::AuthorityLayerStacks).limit(73),
    );
    let project_ids = projects
        .into_iter()
        .map(|item| match item {
            QueryItem::LayerStack(record) => record.id,
            _ => panic!("LayerStack query item"),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(project_ids.len(), 520);

    client
        .pull_layer(first.genesis_layer_id, RemotePlacement::Reference)
        .unwrap();
    for index in 0..520 {
        client
            .fork_branch(
                name(&format!("branch-{index:03}")),
                LocalForkSource::Layer {
                    layer_id: first.genesis_layer_id,
                },
            )
            .unwrap();
    }
    let branches = collect(
        &client,
        Query::new(QueryKind::Branches)
            .in_layer_stack(first.layer_stack_id)
            .limit(91),
    );
    let branch_ids = branches
        .into_iter()
        .map(|item| match item {
            QueryItem::BranchScope(record, _) => record.id,
            _ => panic!("Branch query item"),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(branch_ids.len(), 520);

    assert!(client
        .query(Query::new(QueryKind::Branches).limit(0))
        .is_err());
    assert!(client
        .query(Query::new(QueryKind::Branches).limit(513))
        .is_err());
    assert!(client
        .query(Query::new(QueryKind::Branches).after(vec![0]))
        .is_err());
    assert!(client
        .query(Query::new(QueryKind::Layers).in_layer_stack(first.layer_stack_id),)
        .is_err());

    drop(client);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

fn collect(client: &Client, mut query: Query) -> Vec<QueryItem> {
    let mut items = Vec::new();
    loop {
        let page = client.query(query.clone()).unwrap();
        items.extend(page.items);
        let Some(continuation) = page.continuation else {
            return items;
        };
        query = query.after(continuation);
    }
}

fn name(value: &str) -> EntityName {
    EntityName::new(value).unwrap()
}

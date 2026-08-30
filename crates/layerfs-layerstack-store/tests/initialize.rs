use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{EntityName, LayerStackInitialization, StorageError};

#[test]
fn typed_constructors_and_genesis_are_v2_only() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v2-layerstack-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = store
        .initialize_layerstack(
            EntityName::new("api-server").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let layer = store.layer(initialized.genesis_layer_id).unwrap().unwrap();
    let stack = store.layer_stack(layer.layer_stack_id).unwrap().unwrap();
    assert_eq!(stack.id, initialized.layer_stack_id);
    assert_eq!(stack.name.as_str(), "api-server");
    assert_eq!(stack.head_layer_id, initialized.genesis_layer_id);
    assert!(!store.inventory_page(None, 512).unwrap().entries.is_empty());
    assert!(matches!(
        LayerStackStore::create(&path),
        Err(StorageError::StoreAlreadyExists)
    ));
    drop(store);
    let connected = LayerStackStore::connect(&path).unwrap();
    assert_eq!(connected.store_id().to_string().len(), 64);
    drop(connected);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn names_are_unique_per_authority_and_multiple_layerstacks_are_isolated() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v2-layerstack-names-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = LayerStackStore::create(&path).unwrap();
    let api = store
        .initialize_layerstack(
            EntityName::new("api-server").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let web = store
        .initialize_layerstack(
            EntityName::new("web-client").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    assert_ne!(api.layer_stack_id, web.layer_stack_id);
    assert_ne!(api.genesis_layer_id, web.genesis_layer_id);
    assert_eq!(
        store
            .layer_stack(api.layer_stack_id)
            .unwrap()
            .unwrap()
            .name
            .as_str(),
        "api-server"
    );
    assert_eq!(
        store
            .layer_stack(web.layer_stack_id)
            .unwrap()
            .unwrap()
            .name
            .as_str(),
        "web-client"
    );

    let conflict = store.initialize_layerstack(
        EntityName::new("api-server").unwrap(),
        LayerStackInitialization::Empty,
    );
    assert!(matches!(
        conflict,
        Err(StorageError::LayerStackNameConflict {
            name,
            existing_id,
            incoming_id,
        }) if name.as_str() == "api-server"
            && existing_id == api.layer_stack_id
            && incoming_id != existing_id
    ));

    drop(store);
    std::fs::remove_file(path).unwrap();
}

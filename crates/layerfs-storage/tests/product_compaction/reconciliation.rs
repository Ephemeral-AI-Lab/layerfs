use super::*;

#[test]
fn non_request_product_commit_reconciles_a_lost_acknowledgement() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-product-lost-ack-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let path = base.join("store.sqlite");
    let mut engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    engine.inject_lost_commit_acknowledgement();
    let stack_id = LayerStackId::from_bytes([0x51; 32]);
    let layer_id = LayerId::from_bytes([0x52; 32]);
    let head = engine
        .product_create_layer_stack(stack_id, layer_id, "lost-ack", root)
        .unwrap();
    assert_eq!(engine.product_layer_stack_head(stack_id), Ok(Some(head)));
    drop(engine);
    let reopened = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(reopened.product_layer_stack_head(stack_id), Ok(Some(head)));
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

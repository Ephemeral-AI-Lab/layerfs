//! Independent physical FULL for the previously sealed real-producer dual ID.
mod support;
#[test]
#[ignore = "archived unchanged C2 only; consumes presealed canonical bytes"]
fn seal_dual_chunk_full() {
    let source = std::path::PathBuf::from(std::env::var("SP1_DUAL_CANONICAL").unwrap());
    let out = std::path::PathBuf::from(std::env::var("SP1_DUAL_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let bytes = std::fs::read(source).unwrap();
    let object =
        layerfs_content::FinalizedObject::new(layerfs_content::ObjectRole::Chunk, bytes).unwrap();
    assert_eq!(
        format!("{:?}", object.id()),
        "ObjectId(\"a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399\")"
    );
    let store = support::create_store(&out.join("dual-chunk.sqlite"));
    let result = support::save_one(&store, object).unwrap();
    assert_eq!((result.full_records, result.prefix_records), (1, 0));
    std::fs::write(out.join("outcome.txt"), format!("{result:?}\n")).unwrap();
}

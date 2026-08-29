use layerfs_content::filesystem::{self, LogicalCounters};
use layerfs_content::CanonicalPath;
use layerfs_layer_store::LayerStore;
use layerfs_storage::{CoreReader, LayerInitialization};
use std::os::unix::fs::PermissionsExt;

#[test]
fn directory_initialization_streams_files_and_preserves_links() {
    let root = run_dir("directory");
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file"), b"streamed bytes").unwrap();
    std::fs::set_permissions(source.join("file"), std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::hard_link(source.join("file"), source.join("alias")).unwrap();
    std::os::unix::fs::symlink("file", source.join("link")).unwrap();

    let store = LayerStore::create(root.join("layer.sqlite")).unwrap();
    let (_, layer) = store
        .initialize(LayerInitialization::Directory(source))
        .unwrap();
    let reader = CoreReader(&store);
    let file = CanonicalPath::new("file").unwrap();
    let alias = CanonicalPath::new("alias").unwrap();
    let mut bytes = Vec::new();
    filesystem::read_range(&reader, layer.root_id, &file, 0..14, &mut bytes).unwrap();
    assert_eq!(bytes, b"streamed bytes");
    assert_eq!(
        filesystem::resolve(
            &reader,
            layer.root_id,
            &file,
            &mut LogicalCounters::default()
        )
        .unwrap()
        .inode,
        filesystem::resolve(
            &reader,
            layer.root_id,
            &alias,
            &mut LogicalCounters::default()
        )
        .unwrap()
        .inode
    );
    assert_eq!(
        filesystem::readlink(&reader, layer.root_id, &CanonicalPath::new("link").unwrap())
            .unwrap()
            .0,
        b"file"
    );

    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-layer-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

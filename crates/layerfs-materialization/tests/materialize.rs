use layerfs_materialization::{
    materialize, Attr, Entry, Kind, MaterializationError, MaterializationSource, NodeId, Result,
};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct Fixture;

impl MaterializationSource for Fixture {
    fn root(&self) -> Attr {
        attr(1, Kind::Directory, 0o755)
    }

    fn entries(&self, node: NodeId) -> Result<Vec<Entry>> {
        Ok(match node.0 {
            1 => vec![
                entry("dir", attr(2, Kind::Directory, 0o755)),
                entry("hard", attr(3, Kind::File, 0o640)),
                entry("link", attr(4, Kind::Symlink, 0o777)),
            ],
            2 => vec![entry("file", attr(3, Kind::File, 0o640))],
            _ => return Err(MaterializationError::Port("directory")),
        })
    }

    fn read(&self, node: NodeId, sink: &mut dyn std::io::Write) -> Result<()> {
        if node == NodeId(3) {
            sink.write_all(b"materialized")?;
            Ok(())
        } else {
            Err(MaterializationError::Port("file"))
        }
    }

    fn readlink(&self, node: NodeId) -> Result<Vec<u8>> {
        (node == NodeId(4))
            .then(|| b"dir/file".to_vec())
            .ok_or(MaterializationError::Port("symlink"))
    }
}

#[test]
fn port_materializes_files_metadata_hardlinks_and_symlinks() {
    let run = run_dir();
    let output = run.join("output");
    materialize(&Fixture, &output).unwrap();
    assert_eq!(
        std::fs::read(output.join("dir/file")).unwrap(),
        b"materialized"
    );
    assert_eq!(
        std::fs::metadata(output.join("dir/file")).unwrap().ino(),
        std::fs::metadata(output.join("hard")).unwrap().ino()
    );
    assert_eq!(
        std::fs::metadata(output.join("dir/file"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        std::fs::read_link(output.join("link")).unwrap(),
        std::path::Path::new("dir/file")
    );
    std::fs::remove_dir_all(run).unwrap();
}

fn attr(node: u64, kind: Kind, mode: u32) -> Attr {
    Attr {
        node: NodeId(node),
        kind,
        mode,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    }
}

fn entry(name: &str, attr: Attr) -> Entry {
    Entry {
        name: name.as_bytes().to_vec(),
        attr,
    }
}

fn run_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-materialize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

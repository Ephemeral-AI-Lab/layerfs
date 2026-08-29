use layerfs_fuse::{
    Attr, FilesystemPort, Kind, NodeId, PortError, PortResult, ProxyClient, ProxyHost,
};
use std::sync::{Arc, Mutex};

struct Fixture {
    bytes: Mutex<Vec<u8>>,
    created: Mutex<Vec<Vec<u8>>>,
    links: Mutex<Vec<Vec<u8>>>,
    unlinks: Mutex<Vec<Vec<u8>>>,
    fsyncs: std::sync::atomic::AtomicUsize,
    renamed: std::sync::atomic::AtomicBool,
    reject_mtimes: std::sync::atomic::AtomicBool,
    reject_writes: std::sync::atomic::AtomicBool,
}

impl FilesystemPort for Fixture {
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        match (parent, name) {
            (_, b"missing") => Err(PortError::NotFound),
            (NodeId(1), b"existing-dir") => self.attr(NodeId(3)),
            (NodeId(1), b"destination-dir") => self.attr(NodeId(5)),
            (NodeId(3), b"existing-child") => self.attr(NodeId(4)),
            (NodeId(5), b"target") if self.renamed.load(std::sync::atomic::Ordering::Acquire) => {
                self.attr(NodeId(4))
            }
            (NodeId(5), b"target") => self.attr(NodeId(6)),
            _ => self.attr(NodeId(2)),
        }
    }
    fn attr(&self, node: NodeId) -> PortResult<Attr> {
        Ok(Attr {
            node,
            size: self.bytes.lock().unwrap().len() as u64,
            kind: if node == NodeId(3) || node == NodeId(5) {
                Kind::Directory
            } else {
                Kind::File
            },
            mode: 0o600,
            links: 1,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        })
    }
    fn readlink(&self, _: NodeId) -> PortResult<Vec<u8>> {
        Err(PortError::Invalid)
    }
    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        if node == NodeId(5) {
            Ok(vec![(NodeId(6), Kind::File, b"target".to_vec())])
        } else {
            Ok(vec![
                (NodeId(2), Kind::File, b"file".to_vec()),
                (NodeId(3), Kind::Directory, b"existing-dir".to_vec()),
                (NodeId(5), Kind::Directory, b"destination-dir".to_vec()),
            ])
        }
    }
    fn create_file(&self, _: NodeId, _: &[u8], _: u32) -> PortResult<Attr> {
        Err(PortError::Invalid)
    }
    fn reserve_nodes(&self, _: u32) -> PortResult<NodeId> {
        Ok(NodeId(100))
    }
    fn create_file_open_reserved(
        &self,
        _: NodeId,
        name: &[u8],
        mode: u32,
        node: NodeId,
    ) -> PortResult<Attr> {
        self.bytes.lock().unwrap().clear();
        self.created.lock().unwrap().push(name.to_vec());
        Ok(Attr {
            node,
            size: 0,
            kind: Kind::File,
            mode,
            links: 1,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        })
    }
    fn create_files_closed_reserved(
        &self,
        entries: &[(
            NodeId,
            Vec<u8>,
            u32,
            NodeId,
            Vec<(u64, Vec<u8>)>,
            Option<(i64, u32)>,
        )],
    ) -> PortResult<()> {
        for (_, name, _, node, writes, mtime) in entries {
            self.created.lock().unwrap().push(name.clone());
            for (offset, bytes) in writes {
                self.write(*node, *offset, bytes)?;
            }
            if let Some((seconds, nanos)) = mtime {
                self.set_mtime(*node, *seconds, *nanos)?;
            }
        }
        Ok(())
    }
    fn mkdir(&self, _: NodeId, _: &[u8], _: u32) -> PortResult<Attr> {
        Err(PortError::Invalid)
    }
    fn symlink(&self, _: NodeId, _: &[u8], _: Vec<u8>) -> PortResult<Attr> {
        Err(PortError::Invalid)
    }
    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        if parent == NodeId(3) && name == b"existing-child" {
            return Err(PortError::Exists);
        }
        let mut links = self.links.lock().unwrap();
        if links.iter().any(|existing| existing == name) {
            return Err(PortError::Exists);
        }
        links.push(name.to_vec());
        drop(links);
        let mut attr = self.attr(node)?;
        attr.links = 2;
        Ok(attr)
    }
    fn unlink(&self, _: NodeId, name: &[u8], directory: bool) -> PortResult<()> {
        if directory {
            return Err(PortError::Invalid);
        }
        self.unlinks.lock().unwrap().push(name.to_vec());
        Ok(())
    }
    fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        new_parent: NodeId,
        new_name: &[u8],
        _: bool,
    ) -> PortResult<()> {
        if parent == NodeId(3)
            && name == b"existing-child"
            && new_parent == NodeId(5)
            && new_name == b"target"
        {
            self.renamed
                .store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        } else {
            Err(PortError::Invalid)
        }
    }
    fn pin(&self, _: NodeId, _: bool, _: bool) -> PortResult<()> {
        Ok(())
    }
    fn unpin(&self, _: NodeId, _: bool) -> PortResult<()> {
        Ok(())
    }
    fn read(&self, _: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>> {
        let bytes = self.bytes.lock().unwrap();
        let start = usize::try_from(offset).map_err(|_| PortError::Invalid)?;
        Ok(bytes[start.min(bytes.len())..(start + size).min(bytes.len())].to_vec())
    }
    fn write(&self, _: NodeId, offset: u64, value: &[u8]) -> PortResult<usize> {
        if self
            .reject_writes
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(PortError::NoSpace);
        }
        let mut bytes = self.bytes.lock().unwrap();
        let start = usize::try_from(offset).map_err(|_| PortError::Invalid)?;
        let len = bytes.len().max(start + value.len());
        bytes.resize(len, 0);
        bytes[start..start + value.len()].copy_from_slice(value);
        Ok(value.len())
    }
    fn truncate(&self, _: NodeId, size: u64) -> PortResult<()> {
        self.bytes
            .lock()
            .unwrap()
            .resize(size.try_into().map_err(|_| PortError::Invalid)?, 0);
        Ok(())
    }
    fn chmod(&self, _: NodeId, _: u32) -> PortResult<()> {
        Ok(())
    }
    fn set_mtime(&self, _: NodeId, _: i64, _: u32) -> PortResult<()> {
        if self
            .reject_mtimes
            .load(std::sync::atomic::Ordering::Acquire)
        {
            Err(PortError::NoSpace)
        } else {
            Ok(())
        }
    }
    fn fsync(&self, _: Option<NodeId>) -> PortResult<()> {
        self.fsyncs
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn capability_scopes_a_bounded_typed_proxy_session() {
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(Vec::new()),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(false),
    });
    let host = ProxyHost::start(fixture.clone()).unwrap();
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), [0; 32]).is_err());
    let client = ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap();
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).is_err());
    let directory = client.lookup(NodeId(1), b"existing-dir").unwrap();
    assert_eq!(directory.node, NodeId(3));
    assert_eq!(
        client
            .lookup(directory.node, b"existing-child")
            .unwrap()
            .node,
        NodeId(4),
    );
    assert!(matches!(
        client.link(NodeId(2), directory.node, b"existing-child"),
        Err(PortError::Exists)
    ));
    let destination = client.lookup(NodeId(1), b"destination-dir").unwrap();
    client.readdir(destination.node).unwrap();
    assert_eq!(
        client.lookup(destination.node, b"target").unwrap().node,
        NodeId(6),
    );
    client
        .rename(
            directory.node,
            b"existing-child",
            destination.node,
            b"target",
            false,
        )
        .unwrap();
    assert_eq!(
        client.lookup(destination.node, b"target").unwrap().node,
        NodeId(4),
    );
    client.write(NodeId(2), 0, b"proxy bytes").unwrap();
    assert_eq!(client.read(NodeId(2), 0, 64).unwrap(), b"proxy bytes");
    assert_eq!(client.lookup(NodeId(1), b"file").unwrap().node, NodeId(2));
    let linked = client.link(NodeId(2), NodeId(1), b"known-link").unwrap();
    assert_eq!(linked.links, 2);
    assert_eq!(
        client.lookup(NodeId(1), b"known-link").unwrap().node,
        NodeId(2)
    );
    assert!(matches!(
        client.link(NodeId(2), NodeId(1), b"known-link"),
        Err(PortError::Exists)
    ));
    client.barrier().unwrap();
    assert_eq!(*fixture.links.lock().unwrap(), vec![b"known-link".to_vec()]);
    client.unlink(NodeId(1), b"known-link", false).unwrap();
    assert_eq!(client.attr(NodeId(2)).unwrap().links, 1);
    assert!(matches!(
        client.lookup(NodeId(1), b"missing"),
        Err(PortError::NotFound)
    ));
    let created = client
        .create_file_open(NodeId(1), b"missing", 0o600)
        .unwrap();
    assert_eq!(created.node, NodeId(100));
    client.write(created.node, 0, b"discarded").unwrap();
    client.truncate(created.node, 0).unwrap();
    client.chmod(created.node, 0o640).unwrap();
    client.set_mtime(created.node, 7, 11).unwrap();
    client.write(created.node, 0, &[0; 4096]).unwrap();
    client.write(created.node, 5, b"abc").unwrap();
    client.write(created.node, 6, &[0]).unwrap();
    client.unpin(created.node, true).unwrap();
    client.barrier().unwrap();
    let bytes = fixture.bytes.lock().unwrap();
    assert_eq!(bytes.len(), 4096);
    assert_eq!(&bytes[5..8], b"a\0c");
    drop(bytes);

    let pending = client
        .create_file_open(NodeId(1), b"pending", 0o600)
        .unwrap();
    assert_eq!(client.pause(), Err(PortError::Busy));
    client.unpin(pending.node, true).unwrap();
    client.pause().unwrap();
    client.resume();

    let renamed = client
        .create_file_open(NodeId(1), b"rename-source", 0o600)
        .unwrap();
    client.write(renamed.node, 0, b"renamed").unwrap();
    client
        .rename(
            NodeId(1),
            b"rename-source",
            NodeId(1),
            b"rename-target",
            true,
        )
        .unwrap();
    client.unpin(renamed.node, true).unwrap();
    client.barrier().unwrap();
    assert!(fixture
        .created
        .lock()
        .unwrap()
        .iter()
        .any(|name| name == b"rename-target"));

    client.unlink(NodeId(1), b"file", false).unwrap();
    assert_eq!(
        *fixture.unlinks.lock().unwrap(),
        vec![b"known-link".to_vec()]
    );
    client.barrier().unwrap();
    assert_eq!(
        *fixture.unlinks.lock().unwrap(),
        vec![b"known-link".to_vec(), b"file".to_vec()]
    );
    assert_eq!(
        fixture.fsyncs.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "transport barriers must not invoke filesystem fsync",
    );

    fixture
        .reject_mtimes
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(client.set_mtime(NodeId(2), 9, 12), Err(PortError::NoSpace));
    assert!(host.healthy());
    drop(client);
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).is_err());
}

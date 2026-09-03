use layerfs_fuse::{
    serve_remote_control, Attr, FilesystemPort, Kind, NodeId, PortError, PortResult, ProxyClient,
    ProxyHost,
};
use std::sync::{Arc, Mutex};

struct Fixture {
    bytes: Mutex<Vec<u8>>,
    created: Mutex<Vec<Vec<u8>>>,
    links: Mutex<Vec<Vec<u8>>>,
    unlinks: Mutex<Vec<Vec<u8>>>,
    root_entries: usize,
    lookups: std::sync::atomic::AtomicUsize,
    readdirs: std::sync::atomic::AtomicUsize,
    reservations: std::sync::atomic::AtomicUsize,
    fsyncs: std::sync::atomic::AtomicUsize,
    writes: std::sync::atomic::AtomicUsize,
    renamed: std::sync::atomic::AtomicBool,
    reject_mtimes: std::sync::atomic::AtomicBool,
    reject_writes: std::sync::atomic::AtomicBool,
}

impl FilesystemPort for Fixture {
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.lookups
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
            kind: if node == NodeId(1) || node == NodeId(3) || node == NodeId(5) {
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
        self.readdirs
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if node == NodeId(5) {
            Ok(vec![(NodeId(6), Kind::File, b"target".to_vec())])
        } else if self.root_entries != 3 {
            Ok((0..self.root_entries)
                .map(|index| {
                    (
                        NodeId(index as u64 + 2),
                        Kind::File,
                        format!("file-{index}").into_bytes(),
                    )
                })
                .collect())
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
        self.reservations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    fn mkdir(&self, _: NodeId, _: &[u8], mode: u32) -> PortResult<Attr> {
        Ok(Attr {
            node: NodeId(900),
            size: 0,
            kind: Kind::Directory,
            mode,
            links: 2,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        })
    }
    fn mkdir_reserved(&self, _: NodeId, _: &[u8], mode: u32, node: NodeId) -> PortResult<Attr> {
        Ok(Attr {
            node,
            size: 0,
            kind: Kind::Directory,
            mode,
            links: 2,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        })
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
        } else if parent == NodeId(1)
            && name == b"reserved-directory"
            && new_parent == NodeId(5)
            && new_name == b"moved-directory"
        {
            Ok(())
        } else if parent == NodeId(1)
            && name == b"rename-source"
            && new_parent == NodeId(1)
            && new_name == b"rename-target"
        {
            let mut created = self.created.lock().unwrap();
            let name = created
                .iter_mut()
                .find(|created| created.as_slice() == b"rename-source")
                .ok_or(PortError::NotFound)?;
            *name = new_name.to_vec();
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
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
fn owner_edit_invalidates_warm_file_caches_without_remounting() {
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(vec![7; 128 * 1024]),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        root_entries: 3,
        lookups: std::sync::atomic::AtomicUsize::new(0),
        readdirs: std::sync::atomic::AtomicUsize::new(0),
        reservations: std::sync::atomic::AtomicUsize::new(0),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        writes: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(false),
    });
    let host = ProxyHost::start(fixture.clone()).unwrap();
    let client =
        Arc::new(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap());
    let control = serve_remote_control(
        ("127.0.0.1", host.port()),
        host.capability(),
        client.clone(),
    )
    .unwrap();
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    let mut mounted = if std::env::var_os("LAYERFS_LIVE_FUSE").is_some() {
        let root = std::env::temp_dir().join(format!("layerfs-invalidate-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let mount = layerfs_fuse::mount_host(client.clone(), &root, 0, 0).unwrap();
        client.set_notifier(mount.notifier().unwrap()).unwrap();
        Some((root, mount))
    } else {
        None
    };
    assert!(host.invalidate_file(NodeId(2)).is_err(), "must be paused");
    for (size, value) in [(256 * 1024, 9), (64 * 1024, 3), (64 * 1024, 5)] {
        let previous = fixture.bytes.lock().unwrap().clone();
        assert_eq!(client.attr(NodeId(2)).unwrap().size, previous.len() as u64);
        assert_eq!(client.read(NodeId(2), 0, 4096).unwrap(), previous[..4096]);
        assert_eq!(client.read(NodeId(2), 0, previous.len()).unwrap(), previous);
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        if let Some((root, _)) = &mounted {
            assert_eq!(std::fs::read(root.join("file")).unwrap(), previous);
        }
        host.control("pause").unwrap();
        assert!(host.invalidate_file(NodeId(0)).is_err());
        let next = vec![value; size];
        *fixture.bytes.lock().unwrap() = next.clone();
        host.invalidate_file(NodeId(2)).unwrap();
        host.control("resume").unwrap();
        assert_eq!(client.attr(NodeId(2)).unwrap().size, size as u64);
        assert_eq!(client.read(NodeId(2), 0, size).unwrap(), next);
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        if let Some((root, _)) = &mounted {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(root.join("file")).unwrap();
            assert_eq!(metadata.ino(), 2);
            assert_eq!(metadata.len(), size as u64);
            assert_eq!(std::fs::read(root.join("file")).unwrap(), next);
        }
    }
    std::thread::scope(|scope| {
        let shutdown = scope.spawn(|| host.control("shutdown"));
        control.wait_for_shutdown().unwrap();
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        if let Some((root, mount)) = mounted.as_mut() {
            mount.unmount().unwrap();
            std::fs::remove_dir(root).unwrap();
        }
        control.finish_shutdown(true).unwrap();
        shutdown.join().unwrap().unwrap();
    });
}

#[test]
fn capability_scopes_a_bounded_typed_proxy_session() {
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(Vec::new()),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        root_entries: 3,
        lookups: std::sync::atomic::AtomicUsize::new(0),
        readdirs: std::sync::atomic::AtomicUsize::new(0),
        reservations: std::sync::atomic::AtomicUsize::new(0),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        writes: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(false),
    });
    let host = ProxyHost::start(fixture.clone()).unwrap();
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), [0; 32]).is_err());
    let client =
        Arc::new(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap());
    client.note_fuse_max_write(1024 * 1024);
    client.note_fuse_read_config(1024 * 1024, 7);
    assert_eq!(
        fixture
            .reservations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "connect must not reserve node IDs",
    );
    let control = serve_remote_control(
        ("127.0.0.1", host.port()),
        host.capability(),
        client.clone(),
    )
    .unwrap();
    host.control("pause").unwrap();
    assert!(matches!(client.attr(NodeId(2)), Err(PortError::Busy)));
    host.control("resume").unwrap();
    assert!(client.attr(NodeId(2)).is_ok());
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).is_err());
    assert_eq!(client.readdirplus(NodeId(1)).unwrap().len(), 3);
    assert_eq!(
        fixture.readdirs.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "a complete cached directory must not cross the proxy again",
    );
    let directory = client.lookup(NodeId(1), b"existing-dir").unwrap();
    assert_eq!(directory.node, NodeId(3));
    assert_eq!(
        fixture.lookups.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "READDIRPLUS attributes must satisfy the following lookup locally",
    );
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
    let writes = fixture.writes.load(std::sync::atomic::Ordering::Relaxed);
    let block = vec![7; 64 * 1024];
    for index in 0..32 {
        client
            .write(NodeId(2), index * block.len() as u64, &block)
            .unwrap();
    }
    client.barrier().unwrap();
    assert_eq!(*fixture.bytes.lock().unwrap(), vec![7; 2 * 1024 * 1024]);
    assert_eq!(
        fixture.writes.load(std::sync::atomic::Ordering::Relaxed),
        writes + 2,
        "one global 1 MiB buffer must coalesce sequential writes"
    );
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
    assert_eq!(
        fixture
            .reservations
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "the first reservable create lazily reserves one batch",
    );
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
    client.pause().unwrap();
    client.unpin(pending.node, true).unwrap();
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
    let directory = client
        .mkdir(NodeId(1), b"reserved-directory", 0o700)
        .unwrap();
    assert_eq!(directory.node.0, renamed.node.0 + 1);
    assert_eq!(client.readdir(directory.node).unwrap().len(), 2);
    client
        .rename(
            NodeId(1),
            b"reserved-directory",
            destination.node,
            b"moved-directory",
            true,
        )
        .unwrap();
    assert_eq!(
        client
            .readdir(directory.node)
            .unwrap()
            .into_iter()
            .find(|(_, _, name)| name == b"..")
            .unwrap()
            .0,
        destination.node,
    );

    client.unlink(NodeId(1), b"file", false).unwrap();
    assert_eq!(
        *fixture.unlinks.lock().unwrap(),
        vec![b"known-link".to_vec()]
    );
    assert_eq!(
        fixture.fsyncs.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "transport barriers must not invoke filesystem fsync",
    );
    client.fsync(Some(NodeId(2))).unwrap();
    assert_eq!(
        *fixture.unlinks.lock().unwrap(),
        vec![b"known-link".to_vec(), b"file".to_vec()]
    );
    assert_eq!(fixture.fsyncs.load(std::sync::atomic::Ordering::Relaxed), 1,);

    fixture
        .reject_mtimes
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(client.set_mtime(NodeId(2), 9, 12), Err(PortError::NoSpace));
    assert!(host.healthy());
    client.pause().unwrap();
    let metrics = host.take_write_metrics().unwrap();
    assert_eq!(metrics.max_write_bytes, 1024 * 1024);
    assert!(metrics.kernel_write_requests >= 6);
    assert!(metrics.kernel_write_bytes >= 4096);
    assert_eq!(metrics.client_frame_bytes, metrics.host_frame_bytes);
    assert_eq!(
        metrics.frame_payload_copy_bytes,
        metrics.host_decode_copy_bytes
    );
    assert!(metrics.client_request_copy_bytes >= metrics.frame_payload_copy_bytes);
    let read = host.take_read_metrics().unwrap();
    assert_eq!(read.max_readahead_bytes, 1024 * 1024);
    assert_eq!(read.init_capabilities, 7);
    assert_eq!(read.kernel_read_requests, 1);
    assert_eq!(read.read_ahead_misses, 1);
    assert_eq!(read.read_ahead_fetches, 1);
    assert_eq!(read.read_ahead_fetched_bytes, b"proxy bytes".len() as u64);
    assert_eq!(read.host_response_frames, read.client_response_frames);
    assert_eq!(read.host_response_bytes, read.client_response_bytes);
    assert_eq!(read.host_response_copy_bytes, read.client_decode_copy_bytes);
    client.resume();
    std::thread::scope(|scope| {
        let shutdown = scope.spawn(|| host.control("shutdown"));
        control.wait_for_shutdown().unwrap();
        control.finish_shutdown(true).unwrap();
        shutdown.join().unwrap().unwrap();
    });
    drop(client);
    assert!(ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).is_err());
}

#[test]
fn connect_does_not_enumerate_or_reserve_a_hundred_thousand_entry_root() {
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(Vec::new()),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        root_entries: 100_000,
        lookups: std::sync::atomic::AtomicUsize::new(0),
        readdirs: std::sync::atomic::AtomicUsize::new(0),
        reservations: std::sync::atomic::AtomicUsize::new(0),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        writes: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(false),
    });
    let host = ProxyHost::start(fixture.clone()).unwrap();
    let _client = ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap();

    assert_eq!(
        fixture.readdirs.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "Workspace attach must not enumerate the root",
    );
    assert_eq!(
        fixture
            .reservations
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "Workspace attach must not reserve nodes",
    );
}

#[test]
fn read_ahead_never_returns_short_before_source_eof() {
    const WINDOW: usize = 16 * 1024 * 1024;
    let expected = (0..WINDOW + 128 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(expected.clone()),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        root_entries: 3,
        lookups: std::sync::atomic::AtomicUsize::new(0),
        readdirs: std::sync::atomic::AtomicUsize::new(0),
        reservations: std::sync::atomic::AtomicUsize::new(0),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        writes: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(false),
    });
    let host = ProxyHost::start(fixture).unwrap();
    let client = ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap();

    assert_eq!(
        client.read(NodeId(2), 0, 64 * 1024).unwrap(),
        expected[..64 * 1024]
    );
    let offset = WINDOW - 64 * 1024;
    assert_eq!(
        client.read(NodeId(2), offset as u64, 128 * 1024).unwrap(),
        expected[offset..offset + 128 * 1024]
    );
    for node in 3..=5 {
        assert_eq!(
            client.read(NodeId(node), 0, 64 * 1024).unwrap(),
            expected[..64 * 1024]
        );
    }
    assert_eq!(
        client
            .read(NodeId(2), (offset + 64 * 1024) as u64, 64 * 1024)
            .unwrap(),
        expected[offset + 64 * 1024..offset + 128 * 1024]
    );
    client.read(NodeId(6), 0, 64 * 1024).unwrap();
    client
        .read(NodeId(2), (offset + 64 * 1024) as u64, 64 * 1024)
        .unwrap();
    assert_eq!(
        client.read(NodeId(7), 0, 4 * 1024 * 1024).unwrap().len(),
        4 * 1024 * 1024
    );
    let metrics = client.take_read_metrics();
    assert_eq!(metrics.max_readahead_bytes, 8 * 1024 * 1024);
    assert_eq!(metrics.read_ahead_hits, 1);
    assert_eq!(metrics.read_ahead_misses, 8);
    assert_eq!(metrics.read_ahead_fetches, metrics.read_ahead_misses);
}

#[test]
fn deferred_mutation_errors_surface_at_the_next_synchronization_point() {
    let fixture = Arc::new(Fixture {
        bytes: Mutex::new(Vec::new()),
        created: Mutex::new(Vec::new()),
        links: Mutex::new(Vec::new()),
        unlinks: Mutex::new(Vec::new()),
        root_entries: 3,
        lookups: std::sync::atomic::AtomicUsize::new(0),
        readdirs: std::sync::atomic::AtomicUsize::new(0),
        reservations: std::sync::atomic::AtomicUsize::new(0),
        fsyncs: std::sync::atomic::AtomicUsize::new(0),
        writes: std::sync::atomic::AtomicUsize::new(0),
        renamed: std::sync::atomic::AtomicBool::new(false),
        reject_mtimes: std::sync::atomic::AtomicBool::new(false),
        reject_writes: std::sync::atomic::AtomicBool::new(true),
    });
    let host = ProxyHost::start(fixture.clone()).unwrap();
    let client = ProxyClient::connect(("127.0.0.1", host.port()), host.capability()).unwrap();

    assert_eq!(client.write(NodeId(2), 0, b"fails later"), Ok(11));
    assert_eq!(client.barrier(), Err(PortError::NoSpace));
    assert_eq!(
        client.barrier(),
        Ok(()),
        "the deferred error is acknowledged once"
    );

    assert_eq!(client.write(NodeId(2), 0, &[0; 4096]), Ok(4096));
    assert_eq!(client.fsync(Some(NodeId(2))), Err(PortError::NoSpace));
    assert_eq!(
        fixture.fsyncs.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "a failed deferred mutation must be reported before filesystem fsync",
    );
    client.fsync(Some(NodeId(2))).unwrap();
    assert_eq!(fixture.fsyncs.load(std::sync::atomic::Ordering::Relaxed), 1);

    client.readdir(NodeId(1)).unwrap();
    let pending = client
        .create_file_open(NodeId(1), b"batch-error", 0o600)
        .unwrap();
    assert_eq!(client.write(pending.node, 0, b"batch"), Ok(5));
    client.unpin(pending.node, true).unwrap();
    assert_eq!(client.pause(), Err(PortError::NoSpace));
    client.resume();
    assert_eq!(client.barrier(), Ok(()));

    assert!(!host.healthy());
    assert_eq!(host.failure(), Some(("Write", PortError::NoSpace)));
}

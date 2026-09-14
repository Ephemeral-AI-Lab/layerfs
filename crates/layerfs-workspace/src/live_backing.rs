//! Host-only physical backing and immutable input for a remote live owner.
use crate::cow_tree::{acquire_inode, acquire_inodes, WorkspaceSnapshot};
use crate::file_io::{spool_segment, HostSpool};
use crate::ResourcePolicy;
use layerfs_content::file::content::{read_range, FileContentRoot};
use layerfs_content::tree::directory::{
    DirectoryLookupCache, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::InodeTableRoot;
use layerfs_content::{CanonicalName, ObjectId};
use layerfs_fuse::live_wire::{self as wire, Input};
use layerfs_layerstack_store::{CoreReader, Result, StoreError};
use layerfs_workspace_core::backing::{BackingId, BackingRef};
use layerfs_workspace_core::{Node, NodeId};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

/// Host-side owner of the immutable-base service for one remote workspace:
/// SEED, grouped/metadata lookup, directory pages and canonical Store reads.
/// The mutable workspace state (namespace, pieces, replacement payload) is
/// owned by the sandbox; the host pulls frozen generations at Commit.
pub(crate) struct BackingOwner {
    pub(crate) snapshot: WorkspaceSnapshot,
    pub(crate) root: Node,
    pub(crate) directory: PathBuf,
    pub(crate) policy: ResourcePolicy,
    directory_lookup: DirectoryLookupCache,
    request_profile: [u64; 18],
    lookup_profile: [u64; 2],
    // Requested nodes, optional sibling nodes, and their exported content bytes.
    lookup_work: [u64; 4],
    exported_contents: HashSet<ObjectId>,
}

impl Drop for BackingOwner {
    fn drop(&mut self) {
        if std::env::var_os("LAYERFS_BACKING_PROFILE").is_some() {
            println!(
                "{{\"kind\":\"backing-profile\",\"counts_by_opcode\":{:?},\"lookup_negative\":{},\"lookup_positive\":{},\"lookup_requested_nodes\":{},\"lookup_sibling_nodes\":{},\"lookup_requested_content_bytes\":{},\"lookup_sibling_content_bytes\":{}}}",
                self.request_profile, self.lookup_profile[0], self.lookup_profile[1],
                self.lookup_work[0], self.lookup_work[1], self.lookup_work[2], self.lookup_work[3]
            );
        }
    }
}

impl BackingOwner {
    pub(crate) fn new(
        snapshot: WorkspaceSnapshot,
        root: Node,
        directory: PathBuf,
        policy: ResourcePolicy,
    ) -> Self {
        Self {
            snapshot,
            root,
            directory,
            policy,
            directory_lookup: DirectoryLookupCache::default(),
            request_profile: [0; 18],
            lookup_profile: [0; 2],
            lookup_work: [0; 4],
            exported_contents: HashSet::new(),
        }
    }

    fn acquired_out(
        &mut self,
        acquired: layerfs_workspace_core::namespace::AcquiredInode,
        content_budget: &mut usize,
    ) -> Result<Vec<u8>> {
        let node = Node {
            revision: 0,
            canonical: Some(acquired.inode),
            paths: BTreeSet::new(),
            mode: acquired.mode,
            links: acquired.links,
            pins: 0,
            mtime_seconds: acquired.mtime_seconds,
            mtime_nanoseconds: acquired.mtime_nanoseconds,
            data: acquired.data,
        };
        let mut out = Vec::new();
        wire::bytes_out(&mut out, &wire::node_out(NodeId(1), &node)?)?;
        let mut content = Vec::new();
        if let layerfs_workspace_core::Data::File(layerfs_workspace_core::FileData::Base {
            root,
            len,
        }) = &node.data
        {
            if *len <= wire::IMMUTABLE_PREFETCH_FILE_BYTES as u64
                && *len <= *content_budget as u64
                && !self.exported_contents.contains(&root.0)
            {
                // Optional acquisition never substitutes for a demanded read error.
                match read_range(
                    &CoreReader(&self.snapshot.reader),
                    *root,
                    0..*len,
                    &mut content,
                ) {
                    Ok(counters) => {
                        self.snapshot.reader.note_rope_read(counters)?;
                        if content.len() as u64 == *len {
                            *content_budget -= content.len();
                            // Hints only suppress optional exports. Eviction or a lost
                            // response always falls back to a normal demanded READ_BASE.
                            if self.exported_contents.len() == 8192 {
                                self.exported_contents.clear();
                            }
                            self.exported_contents.insert(root.0);
                        } else {
                            content.clear();
                        }
                    }
                    Err(_) => content.clear(),
                }
            }
        }
        wire::bytes_out(&mut out, &content)?;
        Ok(out)
    }

    pub(crate) fn request(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        if let Some(count) = bytes
            .first()
            .and_then(|opcode| self.request_profile.get_mut(*opcode as usize))
        {
            *count += 1;
        }
        let mut input = Input(bytes);
        let mut out = Vec::new();
        match input.byte()? {
            wire::SEED => {
                input.done()?;
                out.extend_from_slice(self.snapshot.root.as_bytes());
                wire::bytes_out(
                    &mut out,
                    self.snapshot
                        .expected_head
                        .as_ref()
                        .map_or(Vec::new(), |head| head.to_bytes().to_vec())
                        .as_slice(),
                )?;
                wire::u64_out(&mut out, self.policy.max_spool_bytes);
                wire::u64_out(&mut out, self.policy.max_final_delta_memory_bytes);
                out.extend(wire::node_out(layerfs_workspace_core::ROOT, &self.root)?);
            }
            opcode @ (wire::LOOKUP | wire::LOOKUP_METADATA) => {
                let grouped = opcode == wire::LOOKUP;
                let namespace = input.object()?;
                let directory = DirectoryStateRoot(input.object()?);
                let name = CanonicalName::from_bytes(input.bytes()?)?;
                input.done()?;
                if namespace != self.snapshot.root {
                    return Err(StoreError::Integrity("backing namespace"));
                }
                let core = CoreReader(&self.snapshot.reader);
                let namespace = layerfs_content::filesystem::namespace(&core, namespace)?;
                let inode = self.directory_lookup.lookup(
                    &core,
                    directory,
                    &name,
                    &mut NamespaceCounters::default(),
                )?;
                let mut content_budget = if grouped {
                    wire::IMMUTABLE_PREFETCH_PAGE_BYTES
                } else {
                    0
                };
                self.lookup_profile[usize::from(inode.is_some())] += 1;
                out.push(u8::from(inode.is_some()));
                if let Some(inode) = inode {
                    self.lookup_work[0] += 1;
                    let acquired = acquire_inode(
                        &self.snapshot.reader,
                        InodeTableRoot(namespace.inode_table_root),
                        inode,
                    )?;
                    let before = content_budget;
                    out.extend(self.acquired_out(acquired, &mut content_budget)?);
                    self.lookup_work[2] += (before - content_budget) as u64;
                }
                let mut siblings = Vec::new();
                // Reuse the entire already validated leaf, including earlier names.
                // Only a fully exported root leaf establishes directory completeness.
                let entries = if grouped {
                    self.directory_lookup
                        .leaf_entries(directory)
                        .iter()
                        .filter(|(candidate, _)| candidate != &name)
                        .take(127)
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let ids = entries.iter().map(|(_, inode)| *inode).collect::<Vec<_>>();
                self.lookup_work[1] += ids.len() as u64;
                if let Ok(acquired) = acquire_inodes(
                    &self.snapshot.reader,
                    InodeTableRoot(namespace.inode_table_root),
                    &ids,
                ) {
                    for ((name, _), acquired) in entries.into_iter().zip(acquired) {
                        let before = content_budget;
                        if let Ok(node) = self.acquired_out(acquired, &mut content_budget) {
                            siblings.push((name, node));
                        }
                        self.lookup_work[3] += (before - content_budget) as u64;
                    }
                }
                let complete = self.directory_lookup.leaf_complete(directory)
                    && siblings.len() + usize::from(inode.is_some())
                        == self.directory_lookup.leaf_entries(directory).len();
                out.extend_from_slice(&(siblings.len() as u32).to_be_bytes());
                for (name, node) in siblings {
                    wire::bytes_out(&mut out, name.as_bytes())?;
                    out.extend(node);
                }
                out.push(u8::from(complete));
            }
            wire::DIRECTORY_PAGE => {
                let namespace = input.object()?;
                let directory = DirectoryStateRoot(input.object()?);
                let after = input.bytes()?;
                let after = if after.is_empty() {
                    None
                } else {
                    Some(CanonicalName::from_bytes(after)?)
                };
                input.done()?;
                if namespace != self.snapshot.root {
                    return Err(StoreError::Integrity("backing namespace"));
                }
                let core = CoreReader(&self.snapshot.reader);
                let namespace = layerfs_content::filesystem::namespace(&core, namespace)?;
                let page = layerfs_content::tree::directory::directory_page_after(
                    &core,
                    directory,
                    after.as_ref(),
                    128,
                    256 * 1024,
                    &mut NamespaceCounters::default(),
                )?;
                out.push(u8::from(page.continuation.is_some()));
                out.extend_from_slice(&(page.entries.len() as u32).to_be_bytes());
                let ids = page
                    .entries
                    .iter()
                    .map(|(_, inode)| *inode)
                    .collect::<Vec<_>>();
                let acquired = acquire_inodes(
                    &self.snapshot.reader,
                    InodeTableRoot(namespace.inode_table_root),
                    &ids,
                )?;
                let mut content_budget = wire::IMMUTABLE_PREFETCH_PAGE_BYTES;
                for ((name, _), acquired) in page.entries.into_iter().zip(acquired) {
                    wire::bytes_out(&mut out, name.as_bytes())?;
                    out.extend(self.acquired_out(acquired, &mut content_budget)?);
                }
            }
            _ => return Err(StoreError::InvalidInput("backing request")),
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    fn with_empty_backing(test: impl FnOnce(&mut BackingOwner)) {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-backing-batch-{}",
            crate::WorkspaceId::new()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = crate::Workspace::open(store, branch, directory.join("spool")).unwrap();
        let mut owner = BackingOwner::new(
            WorkspaceSnapshot {
                store: workspace.store.clone(),
                workspace_id: workspace.workspace_id,
                branch_id: workspace.branch_id,
                expected_head: workspace.expected_head,
                expected_base: workspace.expected_base,
                root: workspace.base_root,
                reader: workspace.reader.clone(),
            },
            workspace.live.nodes[&crate::ROOT].clone(),
            workspace.spool.clone(),
            workspace.live.policy,
        );
        test(&mut owner);
        drop(owner);
        drop(workspace);
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn backing_words(opcode: u8, words: &[u64]) -> Vec<u8> {
        let mut bytes = vec![opcode];
        for word in words {
            wire::u64_out(&mut bytes, *word);
        }
        bytes
    }
    #[test]
    fn immutable_base_service_rejects_removed_payload_opcodes() {
        with_empty_backing(|owner| {
            for opcode in [
                wire::RESERVE,
                wire::APPEND,
                wire::CANCEL_RESERVATION,
                wire::READ_BACKING,
                wire::CHECK,
                wire::RELEASE,
                wire::FACTS_BEGIN,
                wire::FACTS_NODE,
                wire::FACTS_END,
            ] {
                let mut frame = vec![opcode];
                wire::u64_out(&mut frame, 1);
                assert!(
                    owner.request(&frame).is_err(),
                    "removed opcode {opcode} must be rejected"
                );
            }
            // The immutable-base surface still serves SEED.
            let seed = owner.request(&[wire::SEED]).unwrap();
            assert!(!seed.is_empty());
        });
    }


    #[test]
    fn metadata_lookup_preserves_requested_node_without_acquiring_siblings() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-point-lookup-{}",
            crate::WorkspaceId::new()
        ));
        let fixture = directory.join("fixture");
        std::fs::create_dir_all(&fixture).unwrap();
        for index in 0..100 {
            std::fs::write(
                fixture.join(format!("f{index:03}")),
                vec![index as u8; 4096],
            )
            .unwrap();
        }
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(fixture),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut results = Vec::new();
        for opcode in [wire::LOOKUP, wire::LOOKUP_METADATA] {
            let workspace = crate::Workspace::open(
                store.clone(),
                branch,
                directory.join(format!("spool-{opcode}")),
            )
            .unwrap();
            let mut owner = BackingOwner::new(
                WorkspaceSnapshot {
                    store: workspace.store.clone(),
                    workspace_id: workspace.workspace_id,
                    branch_id: workspace.branch_id,
                    expected_head: workspace.expected_head,
                    expected_base: workspace.expected_base,
                    root: workspace.base_root,
                    reader: workspace.reader.clone(),
                },
                workspace.live.nodes[&crate::ROOT].clone(),
                workspace.spool.clone(),
                workspace.live.policy,
            );
            let layerfs_workspace_core::Data::Directory(root) = &owner.root.data else {
                panic!("root directory")
            };
            let mut request = vec![opcode];
            request.extend_from_slice(owner.snapshot.root.as_bytes());
            request.extend_from_slice(root.base.unwrap().0.as_bytes());
            wire::bytes_out(&mut request, b"f000").unwrap();
            let before = owner.snapshot.reader.read_metrics_snapshot().unwrap();
            let response = owner.request(&request).unwrap();
            let after = owner.snapshot.reader.read_metrics_snapshot().unwrap();
            let mut input = Input(&response);
            assert_eq!(input.byte().unwrap(), 1);
            let requested = input.bytes().unwrap().to_vec();
            let content = input.bytes().unwrap().len();
            let siblings = input.u32().unwrap();
            for _ in 0..siblings {
                input.bytes().unwrap();
                input.bytes().unwrap();
                input.bytes().unwrap();
            }
            let complete = input.byte().unwrap();
            input.done().unwrap();
            let database_bytes = after.snapshot_database_bytes - before.snapshot_database_bytes;
            println!(
                "lookup opcode={opcode} work={:?} database_bytes={database_bytes}",
                owner.lookup_work
            );
            results.push((
                requested,
                content,
                siblings,
                complete,
                owner.lookup_work,
                database_bytes,
            ));
        }
        assert_eq!(
            results[0].0, results[1].0,
            "identical authenticated requested inode"
        );
        assert_eq!((results[0].1, results[0].2, results[0].3), (4096, 99, 1));
        assert_eq!((results[1].1, results[1].2, results[1].3), (0, 0, 0));
        assert_eq!(results[0].4, [1, 99, 4096, 99 * 4096]);
        assert_eq!(results[1].4, [1, 0, 0, 0]);
        assert!(results[1].5 < results[0].5);
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn sdk_k100_point_lookup_captures_only_changes_and_preserves_untouched_siblings() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-k100-point-lookup-{}",
            crate::WorkspaceId::new()
        ));
        let fixture = directory.join("fixture");
        for index in 0..100 {
            let path = fixture.join(format!("d{index:03}"));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("changed"), vec![index as u8; 512]).unwrap();
            std::fs::write(path.join("untouched"), vec![index as u8 ^ 0xff; 512]).unwrap();
        }
        let store_path = directory.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(fixture),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut workspace =
            crate::Workspace::open(store.clone(), branch, directory.join("spool")).unwrap();
        let remote = RemoteWorkspace::start_local(&workspace).unwrap();
        workspace.remote = Some(remote.clone());
        let workspace = std::sync::Mutex::new(workspace);
        for index in 0..100 {
            let path = format!("d{index:03}/changed");
            remote
                .edit(
                    &path,
                    vec![crate::WorkspaceFileRangeEdit {
                        workspace_id: crate::WorkspaceId::new(),
                        path: path.clone(),
                        start: 0,
                        delete_len: 1,
                        replacement: crate::WorkspaceFileReplacement::Inline(vec![0xa5]),
                    }],
                )
                .unwrap();
        }
        {
            let backing = remote.backing.lock().unwrap();
            assert_eq!(backing.lookup_work, [200, 0, 0, 0]);
            assert_eq!(backing.request_profile[wire::LOOKUP as usize], 0);
            assert_eq!(backing.request_profile[wire::LOOKUP_METADATA as usize], 200);
        }
        // The sandbox route captures the changed frontier locally; the
        // lookup counters above remain the point-mutation evidence.
        let summary = remote.capture().unwrap();
        assert!(summary.frontier_len >= 100);
        let input = remote.pull_frozen_input(&summary).unwrap();
        assert!(input.dirty.len() >= 100);
        let prepared = workspace
            .lock()
            .unwrap()
            .build_remote_candidate(&input, 1)
            .unwrap();
        let crate::changes::PreparedCommit {
            built,
            checkpoint,
            admission,
        } = prepared;
        let admission = admission.unwrap();
        let mut branch_record = store.branch(branch).unwrap().unwrap();
        branch_record.head_commit_id = None;
        branch_record.base_layer_id = layer;
        let root = built.root_id;
        let (workspace_id, base_root) = {
            let workspace = workspace.lock().unwrap();
            (workspace.workspace_id, workspace.base_root)
        };
        let outcome = store
            .commit_workspace_candidate(
                workspace_id,
                &branch_record,
                base_root,
                layer,
                built,
                admission,
            )
            .unwrap();
        let commit_id = match outcome {
            layerfs_layerstack_store::CommitOutcome::Committed { commit_id, .. } => commit_id,
            other => panic!("unexpected outcome {other:?}"),
        };
        let mut records = Vec::new();
        checkpoint
            .visit(|id, inode, content, attr| {
                let revision = input.nodes[&id].revision;
                records.push(crate::snapshot_input::CompletionRecord {
                    node: id,
                    revision,
                    inode: *inode.as_bytes(),
                    content: content.to_bytes(),
                    size: attr.size,
                    mode: attr.mode,
                    links: attr.links,
                    mtime_seconds: attr.mtime_seconds,
                    mtime_nanoseconds: attr.mtime_nanoseconds,
                    kind: match attr.kind {
                        layerfs_workspace_core::Kind::File => 1,
                        layerfs_workspace_core::Kind::Directory => 2,
                        layerfs_workspace_core::Kind::Symlink => 3,
                    },
                });
                Ok(())
            })
            .unwrap();
        remote
            .complete_generation(summary.token, root, Some(commit_id.to_bytes()), &records)
            .unwrap();
        remote.server.control("shutdown").unwrap();
        drop((remote, workspace, store));
        let reopened = LayerStackStore::connect(&store_path).unwrap();
        let pinned = reopened.pin_branch(branch).unwrap();
        for index in 0..100 {
            for (name, byte) in [("changed", index as u8), ("untouched", index as u8 ^ 0xff)] {
                let path =
                    layerfs_content::CanonicalPath::new(&format!("d{index:03}/{name}")).unwrap();
                let mut bytes = Vec::new();
                layerfs_content::filesystem::read_range(
                    &CoreReader(&pinned.reader),
                    pinned.root,
                    &path,
                    0..512,
                    &mut bytes,
                )
                .unwrap();
                let mut expected = vec![byte; 512];
                if name == "changed" {
                    expected[0] = 0xa5;
                }
                assert_eq!(bytes, expected, "{path:?}");
            }
        }
        println!("K100 point lookup:200 requestednodes,0 siblings,0 exportedbytes;100 capturedchanges;200 full-file reopen checks");
        drop((pinned, reopened));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cold_grouped_reads_preserve_alias_mutations_and_peer_isolation() {
        use layerfs_fuse::FilesystemPort;
        let directory = std::env::temp_dir().join(format!(
            "layerfs-live-cold-cache-{}",
            crate::WorkspaceId::new()
        ));
        let fixture = directory.join("fixture");
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::write(fixture.join("a"), b"original").unwrap();
        std::fs::hard_link(fixture.join("a"), fixture.join("b")).unwrap();
        std::fs::write(fixture.join("c"), b"untouched").unwrap();
        std::fs::write(fixture.join("d"), b"replace-me").unwrap();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(fixture),
            )
            .unwrap()
            .genesis_layer_id;
        let first_branch = store
            .fork_branch(
                EntityName::new("first").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let peer_branch = store
            .fork_branch(
                EntityName::new("peer").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let first_workspace =
            crate::Workspace::open(store.clone(), first_branch, directory.join("first-spool"))
                .unwrap();
        let peer_workspace =
            crate::Workspace::open(store.clone(), peer_branch, directory.join("peer-spool"))
                .unwrap();
        let original_root = store.pin_branch(first_branch).unwrap().root;
        // Both start_local calls use LiveRuntime::shared(): one cache/scheduler,
        // distinct owners, capabilities, mutable state and spool backing.
        let first_remote = RemoteWorkspace::start_local(&first_workspace).unwrap();
        let peer_remote = RemoteWorkspace::start_local(&peer_workspace).unwrap();
        let first = first_remote.server.local_owner().unwrap();
        let peer = peer_remote.server.local_owner().unwrap();
        first_remote.server.take_write_metrics().unwrap();
        let a = first.lookup(crate::ROOT, b"a").unwrap().node;
        assert_eq!(first.read(a, 0, 100).unwrap(), b"original");
        assert!(
            first_remote
                .server
                .take_write_metrics()
                .unwrap()
                .live_backing_calls
                > 0
        );
        let c = first.lookup(crate::ROOT, b"c").unwrap().node;
        assert_eq!(first.read(c, 0, 100).unwrap(), b"untouched");
        assert_eq!(
            first.lookup(crate::ROOT, b"absent"),
            Err(layerfs_fuse::PortError::NotFound)
        );
        assert_eq!(
            first_remote
                .server
                .take_write_metrics()
                .unwrap()
                .live_backing_calls,
            0
        );

        first.write(a, 0, b"modified").unwrap();
        first_remote.server.take_write_metrics().unwrap();
        let b = first.lookup(crate::ROOT, b"b").unwrap().node;
        assert_eq!(
            a, b,
            "prefetched alias must retain the already edited live inode"
        );
        assert_eq!(
            first_remote
                .server
                .take_write_metrics()
                .unwrap()
                .live_backing_calls,
            0
        );
        assert_eq!(first.read(b, 0, 100).unwrap(), b"modified");
        // d was prefetched but not installed: remove/recreate must beat cached facts.
        first.unlink(crate::ROOT, b"d", false).unwrap();
        assert!(first.lookup(crate::ROOT, b"d").is_err());
        let replacement = first.create_file(crate::ROOT, b"d", 0o600).unwrap().node;
        first.write(replacement, 0, b"new").unwrap();
        assert_eq!(first.lookup(crate::ROOT, b"d").unwrap().node, replacement);
        assert_eq!(first.read(replacement, 0, 100).unwrap(), b"new");

        peer_remote.server.take_write_metrics().unwrap();
        let peer_a = peer.lookup(crate::ROOT, b"a").unwrap().node;
        assert_eq!(peer.read(peer_a, 0, 100).unwrap(), b"original");
        assert!(
            peer_remote
                .server
                .take_write_metrics()
                .unwrap()
                .live_backing_calls
                > 0,
            "a distinct capability must authenticate its own cold acquisition"
        );
        assert_eq!(peer.lookup(crate::ROOT, b"b").unwrap().node, peer_a);
        let peer_d = peer.lookup(crate::ROOT, b"d").unwrap().node;
        assert_eq!(peer.read(peer_d, 0, 100).unwrap(), b"replace-me");
        assert_eq!(store.pin_branch(first_branch).unwrap().root, original_root);
        assert_eq!(store.pin_branch(peer_branch).unwrap().root, original_root);
        first_remote.server.control("shutdown").unwrap();
        peer_remote.server.control("shutdown").unwrap();
        drop((first, peer, first_remote, peer_remote));
        drop((first_workspace, peer_workspace, store));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn live_owner_builds_and_checkpoints_through_real_host_backing() {
        live_owner_check(false);
    }

    #[test]
    fn local_owner_builds_and_checkpoints_through_direct_host_backing() {
        live_owner_check(true);
    }

    fn live_owner_check(_local: bool) {
        use crate::snapshot_input::CompletionRecord;
        use layerfs_fuse::live_owner::LiveOwner;
        use layerfs_fuse::live_runtime::LiveRuntime;
        use layerfs_fuse::FilesystemPort;
        use std::sync::{Arc, Mutex};
        use crate::ROOT;
        let directory = std::env::temp_dir().join(format!(
            "layerfs-live-integrated-{}",
            crate::WorkspaceId::new()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut workspace =
            crate::Workspace::open(store.clone(), branch, directory.join("spool")).unwrap();
        let backing = Arc::new(Mutex::new(BackingOwner::new(
            WorkspaceSnapshot {
                store: workspace.store.clone(),
                workspace_id: workspace.workspace_id,
                branch_id: workspace.branch_id,
                expected_head: workspace.expected_head,
                expected_base: workspace.expected_base,
                root: workspace.base_root,
                reader: workspace.reader.clone(),
            },
            workspace.live.nodes[&crate::ROOT].clone(),
            workspace.spool.clone(),
            workspace.live.policy,
        )));
        let handler_backing = backing.clone();
        let handler: Arc<layerfs_fuse::live_transport::BackingHandler> =
            Arc::new(move |bytes| {
                handler_backing
                    .lock()
                    .map_err(|_| layerfs_fuse::PortError::Io)?
                    .request(bytes)
                    .map_err(crate::projection::storage_port_error)
            });
        let runtime = LiveRuntime::new().unwrap();
        let backing_dir = directory.join("owner-backing");
        let owner = runtime
            .block_on(LiveOwner::local(
                handler,
                runtime.scheduler(),
                backing_dir,
            ))
            .unwrap();
        let server = Arc::new(layerfs_fuse::live_transport::BackingServer::local(owner.clone()));
        let remote = RemoteWorkspace {
            backing: backing.clone(),
            server: server.clone(),
        };
        workspace.remote = Some(remote.clone());

        // Create and write files through the live owner (sandbox-local
        // payload; no host payload traffic).
        let file = owner.create_file(ROOT, b"file", 0o644).unwrap().node;
        runtime.block_on(owner.write_owned(file, 0, b"hello")).unwrap();
        runtime
            .block_on(owner.write_owned(file, 5, b" world"))
            .unwrap();
        let second = owner.create_file(ROOT, b"second", 0o644).unwrap().node;
        runtime
            .block_on(owner.write_owned(second, 0, &[7u8; 300]))
            .unwrap();
        owner.mkdir(ROOT, b"dir", 0o755).unwrap();

        // Capture and pull the frozen generation.
        let summary = remote.capture().unwrap();
        assert!(summary.frontier_len >= 4, "root + file + second + dir");
        // A second capture while unresolved is Busy.
        assert!(remote.capture().is_err());
        // Live writes continue after capture without any pause.
        runtime
            .block_on(owner.write_owned(second, 0, &[9u8; 300]))
            .unwrap();
        let input = remote.pull_frozen_input(&summary).unwrap();
        assert!(input.nodes.len() as u64 >= summary.frontier_len);

        // Build through the existing single-worker pipeline and publish.
        let prepared = workspace
            .build_remote_candidate(&input, 1)
            .unwrap();
        let crate::changes::PreparedCommit {
            built,
            checkpoint,
            admission,
        } = prepared;
        let admission = admission.unwrap();
        let mut branch_record = store.branch(workspace.branch_id).unwrap().unwrap();
        branch_record.head_commit_id = workspace.expected_head;
        branch_record.base_layer_id = workspace.expected_base;
        let root = built.root_id;
        let outcome = store
            .commit_workspace_candidate(
                workspace.workspace_id,
                &branch_record,
                workspace.base_root,
                workspace.expected_base,
                built,
                admission,
            )
            .unwrap();
        let commit_id = match outcome {
            layerfs_layerstack_store::CommitOutcome::Committed { commit_id, .. } => commit_id,
            other => panic!("unexpected outcome {other:?}"),
        };
        workspace.pending_checkpoint = Some(checkpoint);

        // Completion records apply with the captured revisions.
        let mut records: Vec<CompletionRecord> = Vec::new();
        workspace
            .pending_checkpoint
            .as_ref()
            .unwrap()
            .visit(|id, inode, content, attr| {
                let revision = input.nodes[&id].revision;
                records.push(CompletionRecord {
                    node: id,
                    revision,
                    inode: *inode.as_bytes(),
                    content: content.to_bytes(),
                    size: attr.size,
                    mode: attr.mode,
                    links: attr.links,
                    mtime_seconds: attr.mtime_seconds,
                    mtime_nanoseconds: attr.mtime_nanoseconds,
                    kind: match attr.kind {
                        layerfs_workspace_core::Kind::File => 1,
                        layerfs_workspace_core::Kind::Directory => 2,
                        layerfs_workspace_core::Kind::Symlink => 3,
                    },
                });
                Ok(())
            })
            .unwrap();
        let (applied, skipped) = remote
            .complete_generation(summary.token, root, Some(commit_id.to_bytes()), &records)
            .unwrap();
        assert!(applied + skipped >= 3);
        // The re-mutated file kept its live state (its revision advanced).
        assert_eq!(
            runtime.block_on(owner.read_owned(second, 0, 4)).unwrap(),
            vec![9u8; 4],
            "post-capture edit survives completion"
        );
        // Covered nodes read from their canonical base afterwards.
        let attr = owner.attr(file).unwrap();
        assert_eq!(attr.size, 11);

        // The published content is fully readable from the Store alone.
        let pinned = store.pin_branch(workspace.branch_id).unwrap();
        let found = records.iter().find(|record| record.node == file).unwrap();
        let content_root =
            layerfs_content::ObjectId::from_bytes(&found.content).unwrap();
        let mut published = Vec::new();
        layerfs_content::file::content::read_range(
            &layerfs_layerstack_store::CoreReader(&pinned.reader),
            layerfs_content::file::content::FileContentRoot(content_root),
            0..11,
            &mut published,
        )
        .unwrap();
        assert_eq!(published, b"hello world");

        // A second commit covers the post-capture change.
        let summary = remote.capture().unwrap();
        let input = remote.pull_frozen_input(&summary).unwrap();
        let prepared = workspace.build_remote_candidate(&input, 1).unwrap();
        let crate::changes::PreparedCommit {
            built,
            checkpoint,
            admission,
        } = prepared;
        let admission = admission.unwrap();
        let mut branch_record = store.branch(workspace.branch_id).unwrap().unwrap();
        branch_record.head_commit_id = Some(commit_id);
        branch_record.base_layer_id = workspace.expected_base;
        let outcome = store
            .commit_workspace_candidate(
                workspace.workspace_id,
                &branch_record,
                root,
                workspace.expected_base,
                built,
                admission,
            )
            .unwrap();
        assert!(matches!(
            outcome,
            layerfs_layerstack_store::CommitOutcome::Committed { .. }
        ));
        let _ = checkpoint;

        owner.prepare_shutdown().unwrap();
        drop(owner);
        drop(workspace);
        drop(remote);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[derive(Clone)]
pub(crate) struct RemoteWorkspace {
    pub(crate) backing: std::sync::Arc<std::sync::Mutex<BackingOwner>>,
    pub(crate) server: std::sync::Arc<layerfs_fuse::live_transport::BackingServer>,
}

impl RemoteWorkspace {
    pub(crate) fn start(workspace: &crate::Workspace) -> Result<Self> {
        Self::start_with_placement(workspace, false)
    }
    #[cfg(any(test, all(target_os = "linux", feature = "host-fuse")))]
    pub(crate) fn start_local(workspace: &crate::Workspace) -> Result<Self> {
        Self::start_with_placement(workspace, true)
    }

    fn start_with_placement(workspace: &crate::Workspace, local: bool) -> Result<Self> {
        use std::sync::{Arc, Mutex};
        let backing = Arc::new(Mutex::new(BackingOwner::new(
            WorkspaceSnapshot {
                store: workspace.store.clone(),
                workspace_id: workspace.workspace_id,
                branch_id: workspace.branch_id,
                expected_head: workspace.expected_head,
                expected_base: workspace.expected_base,
                root: workspace.base_root,
                reader: workspace.reader.clone(),
            },
            workspace.live.nodes[&crate::ROOT].clone(),
            workspace.spool.clone(),
            workspace.live.policy,
        )));
        let handler = backing.clone();
        let handler: Arc<layerfs_fuse::live_transport::BackingHandler> = Arc::new(move |bytes| {
            handler
                .lock()
                .map_err(|_| layerfs_fuse::PortError::Io)?
                .request(bytes)
                .map_err(crate::projection::storage_port_error)
        });
        let server = if local {
            let runtime = layerfs_fuse::live_runtime::LiveRuntime::shared()?;
            let backing_dir = workspace.spool.join("live-backing");
            if backing_dir.exists() {
                std::fs::remove_dir_all(&backing_dir)?;
            }
            std::fs::create_dir_all(&backing_dir)?;
            let owner = runtime
                .block_on(layerfs_fuse::live_owner::LiveOwner::local(
                    handler,
                    runtime.scheduler(),
                    backing_dir,
                ))
                .map_err(|_| StoreError::Integrity("local live owner"))?;
            layerfs_fuse::live_transport::BackingServer::local(owner)
        } else {
            layerfs_fuse::live_transport::BackingServer::start(move |bytes| handler(bytes))?
        };
        Ok(Self {
            backing,
            server: Arc::new(server),
        })
    }

    pub(crate) fn edit(
        &self,
        path: &str,
        edits: Vec<crate::WorkspaceFileRangeEdit>,
    ) -> crate::WorkspaceResult<()> {
        let nonce = match std::env::var("LAYERFS_EDIT_DIAGNOSTIC_NONCE") {
            Ok(nonce) => Some(nonce),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(crate::WorkspaceError::InvalidExecution),
        };
        if nonce
            .as_ref()
            .is_some_and(|nonce| !wire::valid_edit_diagnostic_nonce(nonce.as_bytes()))
        {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        if edits.is_empty() {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        if edits.iter().any(|edit| matches!(&edit.replacement, crate::WorkspaceFileReplacement::Inline(bytes) if bytes.len() > 1024 * 1024)) {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        let _charge = layerfs_fuse::live_runtime::LiveRuntime::shared()?
            .scheduler()
            .reserve_live(wire::MAX_FRAME)?;
        let mut begin = vec![wire::EDIT_BEGIN];
        wire::bytes_out(&mut begin, path.as_bytes())?;
        wire::u64_out(&mut begin, edits.len() as u64);
        if let Some(nonce) = &nonce {
            wire::bytes_out(&mut begin, nonce.as_bytes())?;
        }
        let members = edits.len();
        let parts = edits.into_iter().map(|edit| {
            let mut frame = vec![wire::EDIT_PART];
            wire::u64_out(&mut frame, edit.start);
            wire::u64_out(&mut frame, edit.delete_len);
            match edit.replacement {
                crate::WorkspaceFileReplacement::Inline(bytes) => {
                    frame.push(0);
                    wire::bytes_out(&mut frame, &bytes).expect("validated inline length");
                }
                crate::WorkspaceFileReplacement::Zero(len) => {
                    frame.push(1);
                    wire::u64_out(&mut frame, len);
                }
            }
            frame
        });
        let before = nonce
            .as_ref()
            .map(|_| self.server.backing_diagnostic_snapshot());
        let lookup_before = if nonce.is_some() {
            Some(
                self.backing
                    .lock()
                    .map_err(|_| crate::WorkspaceError::WorkspaceBusy)?
                    .lookup_work,
            )
        } else {
            None
        };
        let started = nonce.as_ref().map(|_| std::time::Instant::now());
        let response = self
            .server
            .request_group(
                std::iter::once(begin)
                    .chain(parts)
                    .chain(std::iter::once(vec![wire::EDIT_END])),
            )
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        if let (Some(nonce), Some(before), Some(started)) = (nonce, before, started) {
            let group_wall_ns = started.elapsed().as_nanos();
            let after = self.server.backing_diagnostic_snapshot();
            let values = wire::read_edit_diagnostic(&response, nonce.as_bytes())?;
            let host_dispatch_ns = after
                .0
                .checked_sub(before.0)
                .ok_or(crate::WorkspaceError::InvalidExecution)?;
            let host_queue_ns = after
                .1
                .checked_sub(before.1)
                .ok_or(crate::WorkspaceError::InvalidExecution)?;
            let lookup_after = self
                .backing
                .lock()
                .map_err(|_| crate::WorkspaceError::WorkspaceBusy)?
                .lookup_work;
            let mut lookup = [0u64; 4];
            for (index, value) in lookup.iter_mut().enumerate() {
                *value = lookup_after[index]
                    .checked_sub(lookup_before.unwrap()[index])
                    .ok_or(crate::WorkspaceError::InvalidExecution)?;
            }
            let fields = wire::EDIT_DIAGNOSTIC_FIELDS
                .iter()
                .zip(values)
                .map(|(name, value)| format!(",\"{name}\":{value}"))
                .collect::<String>();
            eprintln!("{{\"kind\":\"edit-diagnostic\",\"version\":1,\"nonce\":\"{nonce}\",\"members\":{members},\"group_wall_ns\":{group_wall_ns},\"host_backing_dispatch_ns\":{host_dispatch_ns},\"host_backing_queue_ns\":{host_queue_ns},\"host_lookup_requested_nodes\":{},\"host_lookup_sibling_nodes\":{},\"host_lookup_requested_content_bytes\":{},\"host_lookup_sibling_content_bytes\":{}{fields}}}", lookup[0], lookup[1], lookup[2], lookup[3]);
        } else if !response.is_empty() {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        Ok(())
    }

    /// (covered generation, live dirty count, charged spool bytes, physical
    /// spool bytes, head). Dirty-ness is the live dirty count: the sandbox
    /// owns the mutable state and the host never mirrors it.
    pub(crate) fn observe(
        &self,
    ) -> crate::WorkspaceResult<(u64, u64, u64, Option<layerfs_layerstack_store::CommitId>)> {
        let response = self
            .server
            .observe()
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        let mut input = Input(&response);
        let covered = input.u64()?;
        let dirty = input.u64()?;
        let charged = input.u64()?;
        let _physical = input.u64()?;
        let head = input
            .head()?
            .map(layerfs_layerstack_store::CommitId::from_bytes)
            .transpose()?;
        input.done()?;
        Ok((dirty, covered, charged, head))
    }
}

pub(crate) fn generation(worker: &crate::worker::WorkspaceWorker) -> crate::WorkspaceResult<u64> {
    let remote = worker
        .remote
        .lock()
        .map_err(|_| crate::WorkspaceError::WorkspaceBusy)?
        .clone();
    if let Some(remote) = remote {
        return remote.observe().map(|values| values.0);
    }
    Ok(worker
        .workspace
        .lock()
        .map_err(|_| crate::WorkspaceError::WorkspaceBusy)?
        .live
        .mutation_generation)
}

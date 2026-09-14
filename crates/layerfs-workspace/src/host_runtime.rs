//! Host ownership shared by a mounted client, SDK edits and Commit attempts.
//! Acquisition is supplied by the supported-surface adapter; an acknowledged
//! host root alone is not evidence that dirty writable mappings were captured.
use crate::changes::{PublishedCorrespondence, SnapshotCandidateInputs};
use crate::commit_attempt::{CommitCoordinator, Completion, PublishedContext};
use crate::cow_tree::WorkspaceSnapshot;
use crate::host_operations::HostOperations;
use crate::host_overlay::HostOverlay;
use crate::overlay_budget::{Budget, HostAdmission};
use crate::snapshot::Snapshot;
use layerfs_content::{filesystem, ObjectId};
use layerfs_fuse::live_transport::{BackingHandler, BackingServer};
use layerfs_layerstack_store::{CoreReader, LayerStackStore, Result, StoreError};
use layerfs_workspace_core::ResourcePolicy;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) struct HostRuntime {
    pub(crate) operations: Arc<HostOperations>,
    pub(crate) commits: CommitCoordinator,
    pub(crate) server: Arc<BackingServer>,
    sdk: crate::host_sdk::HostSdk,
    store: LayerStackStore,
    workspace: [u8; 16],
    scope: Option<ObjectId>,
    serials: Arc<Mutex<Option<std::ops::Range<u64>>>>,
    spool: PathBuf,
}

impl HostRuntime {
    pub(crate) fn start(
        snapshot: WorkspaceSnapshot,
        spool: &Path,
        policy: ResourcePolicy,
        local: bool,
    ) -> Result<Self> {
        Self::start_with_admission(snapshot, spool, policy, local, HostAdmission::shared()?)
    }

    fn start_with_admission(
        snapshot: WorkspaceSnapshot,
        spool: &Path,
        policy: ResourcePolicy,
        local: bool,
        admission: Arc<HostAdmission>,
    ) -> Result<Self> {
        let WorkspaceSnapshot {
            store,
            workspace_id,
            branch_id,
            expected_head,
            expected_base,
            root,
            reader,
        } = snapshot;
        let scope = filesystem::namespace(&CoreReader(&reader), root)?.scope;
        let mut branch = store
            .branch(branch_id)?
            .ok_or(StoreError::NotFound("branch"))?;
        // Branch metadata is stable identity; the earlier pinned input controls
        // conditional publication even if another Workspace moved the branch.
        branch.head_commit_id = expected_head;
        branch.base_layer_id = expected_base;
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(spool)?;
        let resources = Budget::open(spool, policy, admission)?;
        let correspondence = PublishedCorrespondence::empty(&resources.index, root, 0)?;
        let host = Arc::new(HostOverlay::new(
            reader,
            root,
            workspace_id,
            policy,
            resources,
        )?);
        let operations = Arc::new(HostOperations::new(host));
        let sdk = crate::host_sdk::HostSdk::new(operations.clone());
        let handler = operations.clone();
        let handler: Arc<BackingHandler> = Arc::new(move |bytes| handler.request(bytes));
        let server = if local {
            let runtime = layerfs_fuse::live_runtime::LiveRuntime::shared()?;
            let client = layerfs_fuse::host_client::HostClient::local(
                handler,
                workspace_id,
                runtime.scheduler(),
            )
            .map_err(|_| StoreError::Integrity("local host client"))?;
            BackingServer::local_host(client, workspace_id)
        } else {
            BackingServer::start_host(workspace_id, move |bytes| handler(bytes))?
        };
        let commits = CommitCoordinator::new(
            store.clone(),
            workspace_id,
            PublishedContext {
                branch,
                correspondence,
            },
            policy.overlay.max_requests,
        )?;
        Ok(Self {
            operations,
            sdk,
            commits,
            server: Arc::new(server),
            store,
            workspace: workspace_id,
            scope,
            serials: Default::default(),
            spool: spool.to_owned(),
        })
    }

    pub(crate) fn generation(&self) -> Result<u64> {
        Ok(self.operations.host.overlay.acquire()?.sequence)
    }

    pub(crate) fn is_dirty(&self) -> Result<bool> {
        let covered = self.commits.published()?.correspondence.covered_sequence;
        Ok(self.generation()? > covered || self.sdk.is_pending()? || self.commits.has_pending()?)
    }

    pub(crate) fn edit(&self, path: &str, edits: &[crate::WorkspaceFileRangeEdit]) -> Result<()> {
        if edits
            .iter()
            .any(|edit| edit.workspace_id.bytes() != self.workspace)
        {
            return Err(StoreError::InvalidInput("foreign SDK Workspace"));
        }
        self.sdk.edit(path, edits, |frame| self.control(frame))
    }

    /// The same owner, reached by the ordinary status and dirty routes.
    pub(crate) fn covered_sequence(&self) -> Result<u64> {
        Ok(self.commits.published()?.correspondence.covered_sequence)
    }

    /// Published head/base context, used for the public session's pinned head.
    pub(crate) fn published_head(&self) -> Result<Option<layerfs_layerstack_store::CommitId>> {
        Ok(self.commits.published()?.branch.head_commit_id)
    }

    /// Release the preopened mount descriptor before the verified unmount. The
    /// authority itself stays installed until End/Discard settles its owners.
    pub(crate) fn prepare_shutdown(&self) -> Result<()> {
        self.server
            .control("shutdown")
            .map_err(|_| StoreError::Integrity("host shutdown"))
    }

    /// Bounded explicit maintenance drain. A retained attempt or an active
    /// Commit makes every step report remaining work, so that condition stops
    /// the drain and the next lifecycle call continues it.
    pub(crate) fn maintain(&self) -> Result<()> {
        const MAX_STEPS: usize = 4096;
        if self.commits.has_pending()? {
            return Ok(());
        }
        for _ in 0..MAX_STEPS {
            if !self.maintenance_step()? {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn recover_sdk(&self) -> Result<crate::host_sdk::Recovery> {
        self.sdk.recover(|frame| self.control(frame))
    }

    /// Called only after the mounted consumer and its control service have
    /// verifiably retired. This is explicit End/Discard cleanup, never Commit.
    pub(crate) fn after_detach(&self) -> Result<()> {
        self.commits.abandon()?;
        self.sdk.after_detach()?;
        self.operations.host.detach_kernel()
    }

    fn control(&self, frame: &[u8]) -> std::io::Result<Vec<u8>> {
        self.server
            .request(frame)
            .map_err(|error| std::io::Error::other(format!("host control: {error:?}")))
    }

    pub(crate) fn maintenance_step(&self) -> Result<bool> {
        self.commits
            .maintenance_step(&self.operations.host, &self.spool)
    }

    pub(crate) fn commit(&self, capture: impl FnOnce() -> Result<Snapshot>) -> Result<Completion> {
        self.commits.commit(capture, |snapshot, previous| {
            #[cfg(test)]
            self.reach_build_latch();
            self.build_candidate(snapshot, previous)
        })
    }

    /// The exact production candidate construction. Separated from `commit` so
    /// a test can hold one attempt in flight while ordinary operations run.
    pub(crate) fn build_candidate(
        &self,
        snapshot: &Snapshot,
        previous: &PublishedContext,
    ) -> Result<crate::changes::PreparedSnapshotCommit> {
        let host = &self.operations.host;
        if !host.index.owns(&snapshot.root.index) {
            return Err(StoreError::InvalidInput("foreign Workspace snapshot"));
        }
        SnapshotCandidateInputs {
            snapshot,
            budget: &host.budget,
            ranges: &host.ranges,
            index: &host.index,
            origins: host.origins(),
            comparison: &previous.correspondence,
            store: &self.store,
            workspace_id: self.workspace,
            scope: self.scope,
            serials: self.serials.clone(),
            policy: host.policy,
            spool: &self.spool,
        }
        .build(1)
    }

    #[cfg(test)]
    fn reach_build_latch(&self) {
        let armed = BUILD_LATCH.lock().expect("host runtime build latch").take();
        if let Some((started, resume)) = armed {
            let _ = started.send(());
            let _ = resume.lock().expect("host runtime build latch").recv();
        }
    }
}

#[cfg(test)]
type BuildLatch = (
    std::sync::mpsc::Sender<()>,
    Mutex<std::sync::mpsc::Receiver<()>>,
);

#[cfg(test)]
static BUILD_LATCH: Mutex<Option<BuildLatch>> = Mutex::new(None);

/// Arm one build-phase pause for the next Commit construction on this process.
/// Tests run single-threaded; with no latch armed the production path is exact.
#[cfg(test)]
pub(crate) fn arm_build_latch() -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    *BUILD_LATCH.lock().expect("host runtime build latch") =
        Some((started_tx, Mutex::new(resume_rx)));
    (started_rx, resume_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_budget::HostLimits;
    use layerfs_content::CanonicalPath;
    use layerfs_fuse::host_client::HostClient;
    use layerfs_fuse::live_runtime::LiveRuntime;
    use layerfs_fuse::FilesystemPort;
    use layerfs_layerstack_store::{EntityName, LayerStackInitialization, LocalForkSource};
    use layerfs_workspace_core::ROOT;

    #[test]
    fn local_and_tcp_runtime_share_live_authority_and_published_coverage() {
        for local in [true, false] {
            let id = crate::WorkspaceId::new();
            let directory = std::env::temp_dir().join(format!("layerfs-host-runtime-{id}"));
            let source = directory.join("source");
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(source.join("file"), b"initial").unwrap();
            let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
            let layer = store
                .initialize_layerstack(
                    EntityName::new("runtime").unwrap(),
                    LayerStackInitialization::Directory(source),
                )
                .unwrap()
                .genesis_layer_id;
            let branch = store
                .fork_branch(
                    EntityName::new("main").unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap();
            let pinned = store.pin_branch(branch).unwrap();
            let workers = LiveRuntime::new().unwrap();
            let admission = HostAdmission::new(workers.scheduler(), HostLimits::default()).unwrap();
            let runtime = HostRuntime::start_with_admission(
                WorkspaceSnapshot {
                    store: store.clone(),
                    workspace_id: id.bytes(),
                    branch_id: branch,
                    expected_head: pinned.branch.head_commit_id,
                    expected_base: pinned.branch.base_layer_id,
                    root: pinned.root,
                    reader: pinned.reader,
                },
                &directory.join("spool"),
                ResourcePolicy::default(),
                local,
                admission.clone(),
            )
            .unwrap();
            let client = if local {
                runtime.server.host_owner().unwrap()
            } else {
                workers
                    .block_on(HostClient::connect(
                        format!("127.0.0.1:{}", runtime.server.port()),
                        runtime.server.capability(),
                        id.bytes(),
                        workers.scheduler(),
                    ))
                    .unwrap()
            };
            let node = client.lookup(ROOT, b"file").unwrap().node;
            assert!(!runtime.is_dirty().unwrap());
            client.write(node, 0, b"A").unwrap();
            let cut = runtime.operations.host.snapshot().unwrap();
            let covered = cut.root.sequence;
            let first = runtime
                .commit(|| {
                    client.write(node, 1, b"B").unwrap();
                    Ok(cut)
                })
                .unwrap();
            assert_eq!(
                runtime
                    .commits
                    .published()
                    .unwrap()
                    .correspondence
                    .covered_sequence,
                covered
            );
            assert!(runtime.is_dirty().unwrap());
            assert_eq!(client.read(node, 0, 7).unwrap(), b"ABitial");
            let first_root = first.receipt.attempt.candidate_root;
            let read = |store: &LayerStackStore, root: ObjectId| {
                let reader = store.snapshot_reader(root);
                let resolved = filesystem::resolve(
                    &CoreReader(&reader),
                    root,
                    &CanonicalPath::new("file").unwrap(),
                    &mut filesystem::LogicalCounters::default(),
                )
                .unwrap();
                let mut bytes = Vec::new();
                layerfs_content::file::content::read_range(
                    &CoreReader(&reader),
                    layerfs_content::file::content::FileContentRoot(resolved.record.content_root),
                    0..7,
                    &mut bytes,
                )
                .unwrap();
                bytes
            };
            assert_eq!(read(&store, first_root), b"Anitial");
            let second = runtime
                .commit(|| runtime.operations.host.snapshot())
                .unwrap();
            assert!(!runtime.is_dirty().unwrap());
            assert!(runtime.generation().unwrap() > 0);
            let second_root = second.receipt.attempt.candidate_root;
            assert_eq!(read(&store, second_root), b"ABitial");
            workers.block_on(client.detach()).unwrap();
            drop((client, runtime, store));
            // TCP handlers retire on the existing runtime after server stop.
            // Their remaining Arc must keep the budget owned until then.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            while admission.usage().workspaces != 0 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(admission.usage().workspaces, 0);
            let reopened = LayerStackStore::connect(directory.join("store.sqlite")).unwrap();
            assert_eq!(read(&reopened, first_root), b"Anitial");
            assert_eq!(read(&reopened, second_root), b"ABitial");
            drop(reopened);
            std::fs::remove_dir_all(directory).unwrap();
        }
        println!("Host runtime local/TCP PASS: exact owned input, concurrent acknowledged writes, C1/C2 Store reopen, monotonic generation vs published coverage, detach ownership; kernel acquisition remains outside this component check");
    }
    #[test]
    #[ignore = "requires an explicitly owned Linux Docker correctness container and sealed FUSE helper"]
    fn mounted_host_owner_preserves_sdk_mapping_handles_and_owned_commit_input() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        let container =
            std::env::var("LAYERFS_HOST_OVERLAY_CONTAINER").expect("owned correctness container");
        let id = crate::WorkspaceId::new();
        let directory = std::env::temp_dir().join(format!("layerfs-mounted-host-{id}"));
        let source = directory.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file"), vec![b'a'; 8192]).unwrap();
        let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("mounted").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let pinned = store.pin_branch(branch).unwrap();
        let runtime = HostRuntime::start(
            WorkspaceSnapshot {
                store: store.clone(),
                workspace_id: id.bytes(),
                branch_id: branch,
                expected_head: pinned.branch.head_commit_id,
                expected_base: pinned.branch.base_layer_id,
                root: pinned.root,
                reader: pinned.reader,
            },
            &directory,
            ResourcePolicy::default(),
            false,
        )
        .unwrap();
        let mountpoint = PathBuf::from(format!("/var/tmp/layerfs-host-{id}"));
        let mut projection = crate::docker::DockerProjection::attach(
            id,
            crate::ContainerId(container.clone()),
            mountpoint.clone(),
            runtime.server.clone(),
            &directory,
            None,
        )
        .unwrap();
        let script = r#"import os,sys,mmap
p=sys.argv[1]
f=os.open(p+'/file',os.O_RDWR)
s=os.fstat(f)
m=mmap.mmap(f,8192)
m[4096]=ord('u')
print('MAPPED',s.st_ino,flush=True)
assert sys.stdin.readline().strip()=='SDK'
assert m[0]==ord('S') and m[4096]==ord('u')
assert os.fstat(f).st_ino==s.st_ino
os.link(p+'/file',p+'/alias')
os.rename(p+'/file',p+'/renamed')
assert os.stat(p+'/alias').st_ino==s.st_ino
print('SDK_VISIBLE',flush=True)
assert sys.stdin.readline().strip()=='WRITE'
os.pwrite(f,b'X',1)
assert os.pread(f,2,0)==b'SX'
print('WROTE',flush=True)
assert sys.stdin.readline().strip()=='END'
os.unlink(p+'/renamed');os.unlink(p+'/alias')
assert os.pread(f,2,0)==b'SX'
m.close();os.close(f)
print('CLOSED',flush=True)
"#;
        let mut child = Command::new("docker")
            .args(["exec", "-i", &container, "python3", "-u", "-c", script])
            .arg(&mountpoint)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert!(line.starts_with("MAPPED "), "{line}");
        println!("{line}");
        runtime
            .edit(
                "file",
                &[crate::WorkspaceFileRangeEdit {
                    workspace_id: id,
                    path: "file".into(),
                    start: 0,
                    delete_len: 1,
                    replacement: crate::WorkspaceFileReplacement::Inline(vec![b'S']),
                }],
            )
            .unwrap();
        writeln!(input, "SDK").unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "SDK_VISIBLE");
        let snapshot = runtime.operations.host.snapshot().unwrap();
        let first = runtime
            .commit(|| {
                writeln!(input, "WRITE").unwrap();
                line.clear();
                output.read_line(&mut line).unwrap();
                assert_eq!(line.trim(), "WROTE");
                Ok(snapshot)
            })
            .unwrap();
        let read = |root: ObjectId| {
            let reader = store.snapshot_reader(root);
            let f = filesystem::resolve(
                &CoreReader(&reader),
                root,
                &CanonicalPath::new("alias").unwrap(),
                &mut filesystem::LogicalCounters::default(),
            )
            .unwrap();
            let mut out = Vec::new();
            layerfs_content::file::content::read_range(
                &CoreReader(&reader),
                layerfs_content::file::content::FileContentRoot(f.record.content_root),
                0..8192,
                &mut out,
            )
            .unwrap();
            out
        };
        let c1 = read(first.receipt.attempt.candidate_root);
        assert_eq!(&c1[..2], b"Sa");
        assert_eq!(c1[4096], b'u');
        let second = runtime
            .commit(|| runtime.operations.host.snapshot())
            .unwrap();
        assert_eq!(&read(second.receipt.attempt.candidate_root)[..2], b"SX");
        writeln!(input, "END").unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "CLOSED");
        assert!(child.wait().unwrap().success());
        projection.end().unwrap();
        assert!(projection.healthy());
        runtime.after_detach().unwrap();
        drop((projection, runtime, store));
        println!("MOUNTED HOST COMPONENT PASS: SDK visible through retained mapping/descriptor, unrelated dirty mmap byte preserved, stable inode/hardlink/rename/open-unlinked, owned C1/C2 and actual helper unmount; generic Commit dirty-mmap acquisition remains unproven");
        std::fs::remove_dir_all(directory).unwrap();
    }
}

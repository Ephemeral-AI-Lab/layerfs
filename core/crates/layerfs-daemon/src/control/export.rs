//! NFS kernel serving: the native route's mount entry, beside FUSE.
//!
//! Each Attach serves the Workspace's engine mount as one loopback NFSv3
//! export (`layerfs-nfs`) on a fresh port, then has the host's own NFS client
//! mount it on the Workspace's mount directory with the configured helper
//! (`mount_nfs` on macOS). The registry keeps the export as the entry's
//! kernel half; the Workspace API, its control records and every engine and
//! Store step are the ones the FUSE route uses:
//!
//! - Ready carries the same receipt record; an NFS receipt is recognized by
//!   `abi_major == 0`, and its `mount_id` is the export's TCP port.
//! - Commit first has the client write back what it buffered (the flush
//!   helper, `sync(2)` by default) and closes the export's implicit file
//!   descriptors, then runs the unchanged Commit. A Commit that installed
//!   another root closes the export's descriptors again; the client
//!   revalidates attributes on its own schedule (close-to-open, `actimeo`).
//! - Unmount asks the helper to unmount (a busy answer is reversible and the
//!   export keeps serving), then stops the export (every connection joined,
//!   every reference and descriptor given back), revokes the engine mount
//!   and runs the one logical Close. A step that is not established keeps
//!   the export and its engine mount in the entry, as FUSE custody is kept.
//! - ForceUnmount runs the forced helper once, then the same teardown.
use super::{
    kernel::{bounded, Kernel},
    registry::{bound_mut, idle},
    Failure, Service, Success,
};
use crate::store::BoundWorkspace;
use layerfs_bridge::control::{
    AbortDisposition, Activity, ControlCode, DetachDisposition, ForceUnmounted, ForcedCleanup,
    ForcedFacts, ForcedOutcome, MountPhase, MountReceipt, MountStatus, MountWork, ReadyMount,
    Reply, TeardownCustody, TeardownStage, WorkspaceToken, MOUNT_DIRECTORY_LIMIT,
};
use layerfs_nfs::{ExportConfig, Released, Server};
use layerfs_overlay::{CleanupState, KernelMount};
use std::{
    fmt,
    io::Read,
    net::{IpAddr, Ipv4Addr, TcpListener},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

/// Largest write and read the export negotiates (FSINFO wtmax/rtmax).
const WINDOW: u32 = 128 * 1024;
/// How long one helper run may take before it is killed and reported.
pub const HELPER_WAIT: Duration = Duration::from_secs(60);
/// Bytes of a helper's output kept for a refusal.
const OUTPUT: usize = 2048;

/// Stale inodes one install refreshes through the mount point at most.
pub const REFRESHED: usize = 4096;
/// `0` turns the post-install refresh off (see [`Service::installed_export`]).
pub const REFRESH: &str = "LAYERFS_NFS_REFRESH";

/// One mode-preserving chmod of each path below `root`, through the host's
/// NFS client; a symbolic link (chmod would follow it) and a path that is
/// gone are skipped. The export answers each without a change.
fn refresh(root: &std::path::Path, paths: &[PathBuf]) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for path in paths {
        let path = root.join(path);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            let mode = std::fs::Permissions::from_mode(metadata.mode() & 0o7777);
            let _ = std::fs::set_permissions(&path, mode);
        }
    }
}
/// One helper command; `{port}` and `{dir}` in its words are replaced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Helper(pub Vec<String>);
impl Helper {
    /// Splits a configured command line on whitespace.
    pub fn words(line: &str) -> Option<Self> {
        let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        (!words.is_empty()).then_some(Self(words))
    }
    fn command(&self, port: u16, directory: &str) -> Option<Command> {
        let mut words = self.0.iter().map(|word| {
            word.replace("{port}", &port.to_string())
                .replace("{dir}", directory)
        });
        let mut command = Command::new(words.next()?);
        command.args(words);
        Some(command)
    }
}
/// How the host's NFS client mounts, flushes and unmounts an export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mounter {
    pub mount: Helper,
    pub unmount: Helper,
    pub force: Helper,
    /// Writes back what the client buffered before a Commit; `sync(2)` of
    /// the daemon itself when absent.
    pub flush: Option<Helper>,
}
impl Mounter {
    /// The host's own client: `mount_nfs` on macOS (no root needed), the
    /// Linux NFS client elsewhere (root, as the daemon is in its container).
    /// The macOS mount is soft (an export that ended fails calls after the
    /// retries instead of hanging them) and hidden from Finder (`nobrowse`).
    pub fn host() -> Self {
        let helper = |words: &[&str]| Helper(words.iter().map(|w| (*w).to_owned()).collect());
        if cfg!(target_os = "macos") {
            Self {
                mount: helper(&[
                    "/sbin/mount_nfs",
                    "-o",
                    "vers=3,tcp,port={port},mountport={port},locallocks,nfc,actimeo=60,rsize=131072,wsize=131072,soft,intr,timeo=20,retrans=6,nobrowse",
                    "127.0.0.1:/",
                    "{dir}",
                ]),
                unmount: helper(&["/sbin/umount", "{dir}"]),
                force: helper(&["/sbin/umount", "-f", "{dir}"]),
                flush: None,
            }
        } else {
            Self {
                mount: helper(&[
                    "mount",
                    "-t",
                    "nfs",
                    "-o",
                    "vers=3,proto=tcp,port={port},mountport={port},mountproto=tcp,nolock,actimeo=60",
                    "127.0.0.1:/",
                    "{dir}",
                ]),
                unmount: helper(&["umount", "{dir}"]),
                force: helper(&["umount", "-f", "-l", "{dir}"]),
                flush: None,
            }
        }
    }
}
/// Declared NFS serving inputs, assembled once by the application.
#[derive(Clone, Debug)]
pub struct ExportServingConfig {
    /// Existing daemon-owned directory; each mount is a new child of it.
    pub mounts: PathBuf,
    /// Runtime command identity presented as every inode's owner.
    pub command_uid: u32,
    pub command_gid: u32,
    /// Address each export listens on, port chosen by the system.
    pub listen: IpAddr,
    pub mounter: Mounter,
    /// Refuse creating macOS AppleDouble `._*` names.
    pub refuse_apple_double: bool,
    /// Lookup references one export keeps before collecting.
    pub lookups_held: usize,
}
impl ExportServingConfig {
    pub fn new(mounts: PathBuf, command_uid: u32, command_gid: u32, mounter: Mounter) -> Self {
        Self {
            mounts,
            command_uid,
            command_gid,
            listen: IpAddr::V4(Ipv4Addr::LOCALHOST),
            mounter,
            refuse_apple_double: true,
            lookups_held: layerfs_nfs::kernel::LOOKUPS_HELD,
        }
    }
}
/// Process-wide NFS serving: configuration only; each Attach starts its own
/// export and each terminal operation stops it.
#[derive(Debug)]
pub struct ExportServing {
    config: ExportServingConfig,
}
impl ExportServing {
    pub fn start(config: ExportServingConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &ExportServingConfig {
        &self.config
    }
    fn directory(&self, token: WorkspaceToken) -> PathBuf {
        self.config.mounts.join(token.namespace.to_string())
    }
    /// At startup, before any Attach: every mount the host still has at or
    /// under the mount home is one a daemon that ended without unmounting
    /// left (its export is gone, so the client would hang on it). Each is
    /// forced off once with the force helper; one line per mount says what
    /// happened. Nothing is retried.
    pub fn reclaim(&self) -> Vec<String> {
        let points = match mounted_under(&self.config.mounts) {
            Ok(points) => points,
            Err(error) => return vec![format!("mount table unreadable: {error}")],
        };
        (points.into_iter())
            .map(|point| {
                let text = point.display().to_string();
                match run(&self.config.mounter.force, 0, &text) {
                    Ran::Done => format!("forced off {text}"),
                    Ran::Busy(detail) | Ran::Failed(detail) => {
                        format!("left {text}: {detail}")
                    }
                }
            })
            .collect()
    }
}
/// Mount points at or under `home`, deepest first, from the host's table:
/// `mount`'s `<what> on <point> (<options>)` lines on macOS,
/// `/proc/self/mountinfo` (field 5, octal escapes) on Linux.
fn mounted_under(home: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    let points: Vec<PathBuf> = {
        let out = Command::new("/sbin/mount")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()?;
        (String::from_utf8_lossy(&out.stdout).lines())
            .filter_map(|line| {
                Some(PathBuf::from(
                    line.split_once(" on ")?.1.rsplit_once(" (")?.0,
                ))
            })
            .collect()
    };
    #[cfg(not(target_os = "macos"))]
    let points: Vec<PathBuf> = (std::fs::read_to_string("/proc/self/mountinfo")?.lines())
        .filter_map(|line| line.split(' ').nth(4))
        .map(|point| PathBuf::from(point.replace("\\040", " ").replace("\\011", "\t")))
        .collect();
    let mut found: Vec<PathBuf> = (points.into_iter())
        .filter(|point| point.starts_with(home))
        .collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}
/// A serving export and the mount the host client holds of it.
pub(super) struct Exported {
    pub server: Arc<Server>,
    pub mount: KernelMount,
    pub ready: ReadyMount,
}
/// A terminal operation owns the export; status still observes it.
pub(super) struct ExportLeaving {
    pub ready: ReadyMount,
    pub phase: MountPhase,
    pub work: MountWork,
}
/// Export custody a terminal or attach step could not finish disposing:
/// kept exactly, never retried or settled by a later observation.
pub(super) struct ExportKept {
    pub ready: Option<ReadyMount>,
    pub stage: TeardownStage,
    pub detail: String,
    pub work: Option<MountWork>,
    pub evidence: Box<dyn fmt::Debug + Send>,
    pub forced: Option<ForcedFacts>,
}
impl ExportKept {
    pub(super) fn custody(&self, token: WorkspaceToken) -> TeardownCustody {
        TeardownCustody {
            token,
            stage: self.stage,
            detached: matches!(self.stage, TeardownStage::Revoke | TeardownStage::Close),
            work: self.work,
            detail: bounded(&self.detail),
            forced: self.forced.clone(),
        }
    }
}
impl fmt::Debug for ExportKept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExportKept")
            .field("ready", &self.ready)
            .field("stage", &self.stage)
            .field("detail", &self.detail)
            .field("work", &self.work)
            .field("evidence", &self.evidence)
            .field("forced", &self.forced)
            .finish()
    }
}
impl fmt::Debug for Exported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exported")
            .field("address", &self.server.local_addr())
            .field("mount", &self.mount)
            .field("held", &self.server.held())
            .finish()
    }
}
/// The maintained counters of a serving export.
pub(super) fn work(server: &Server) -> MountWork {
    let held = server.held();
    let served = server.counters().iter().map(|count| count.calls).sum();
    MountWork {
        loops_configured: 1,
        loops_entered: 1,
        loops_exited: 0,
        loops_joined: 0,
        active: 0,
        retained: u32::try_from(held.retained).unwrap_or(u32::MAX),
        completed: served,
        served,
        inline: 0,
        refused: 0,
        terminal: 0,
        forget_units: held.collected,
    }
}
/// The status block of an export-owning entry.
pub(super) fn block(kernel: &Kernel) -> Option<MountStatus> {
    Some(match kernel {
        Kernel::Exported(exported) => MountStatus {
            phase: MountPhase::Ready,
            ready: Some(exported.ready.clone()),
            detached: false,
            work: Some(work(&exported.server)),
        },
        Kernel::ExportLeaving(leaving) => MountStatus {
            phase: leaving.phase,
            ready: Some(leaving.ready.clone()),
            detached: leaving.phase == MountPhase::Draining,
            work: Some(leaving.work),
        },
        Kernel::ExportKept(kept) => MountStatus {
            phase: MountPhase::Retained,
            ready: kept.ready.clone(),
            detached: matches!(kept.stage, TeardownStage::Revoke | TeardownStage::Close),
            work: kept.work,
        },
        _ => return None,
    })
}
/// What one helper run answered.
enum Ran {
    Done,
    /// The client reported the mount busy; nothing changed.
    Busy(String),
    Failed(String),
}
fn run(helper: &Helper, port: u16, directory: &str) -> Ran {
    let Some(mut command) = helper.command(port, directory) else {
        return Ran::Failed("empty helper command".into());
    };
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return Ran::Failed(format!("{:?}: {error}", helper.0.first())),
    };
    let deadline = Instant::now() + HELPER_WAIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                drop(child.kill());
                drop(child.wait());
                break Err(format!("helper did not finish in {HELPER_WAIT:?}; killed"));
            }
            Err(error) => break Err(format!("helper wait: {error}")),
        }
    };
    let mut output = String::new();
    for pipe in [
        child.stderr.take().map(|p| Box::new(p) as Box<dyn Read>),
        child.stdout.take().map(|p| Box::new(p) as Box<dyn Read>),
    ]
    .into_iter()
    .flatten()
    {
        let mut bytes = Vec::new();
        drop(pipe.take(OUTPUT as u64).read_to_end(&mut bytes));
        output.push_str(&String::from_utf8_lossy(&bytes));
    }
    match status {
        Ok(status) if status.success() => Ran::Done,
        Ok(status) => {
            let detail = format!("{status}: {}", output.trim());
            if output.to_ascii_lowercase().contains("busy") {
                Ran::Busy(detail)
            } else {
                Ran::Failed(detail)
            }
        }
        Err(detail) => Ran::Failed(format!("{detail}: {}", output.trim())),
    }
}
fn receipt(token: WorkspaceToken, directory: String, mount: KernelMount, port: u16) -> ReadyMount {
    ReadyMount {
        token,
        directory,
        receipt: MountReceipt {
            mount: mount.owner_id(),
            root: mount.root_serial(),
            mount_id: u64::from(port),
            device_major: 0,
            device_minor: 0,
            abi_major: 0,
            abi_minor: 0,
            offered: 0,
            selected: 0,
            max_write: WINDOW,
            max_readahead: WINDOW,
            max_background: 0,
            congestion_threshold: 0,
            page_size: 4096,
            loops: 1,
            abort_bound: true,
        },
    }
}
fn kernel_failure(code: ControlCode, phase: &'static str, detail: impl Into<String>) -> Failure {
    Failure::Kernel(Box::new(super::types::KernelFailure {
        code,
        phase,
        detail: detail.into(),
        evidence: None,
    }))
}
/// Where a failed export Attach leaves the entry.
enum Stop {
    Unattached(Failure),
    Unknown(Failure),
    Retained(Box<ExportKept>),
}
impl Service {
    /// Attach over NFS: Ready is acknowledged only after the export serves
    /// and the host client's mount helper reported the mount.
    pub(super) fn attach_export(
        &self,
        serving: &ExportServing,
        token: WorkspaceToken,
    ) -> Result<Success, Failure> {
        let workspace = self.admit_export(token)?;
        match self.export_admitted(serving, token, &workspace) {
            Ok(exported) => {
                let reply = Reply::Ready(Box::new(exported.ready.clone()));
                self.settle(token, Kernel::Exported(Box::new(exported)), Activity::Idle);
                Ok(Success::reply(reply))
            }
            Err(Stop::Unattached(failure)) => {
                self.settle(token, Kernel::Unattached, Activity::Idle);
                Err(failure)
            }
            Err(Stop::Unknown(failure)) => {
                self.settle(token, Kernel::Attaching, Activity::Uncertain);
                Err(failure)
            }
            Err(Stop::Retained(kept)) => {
                let custody = kept.custody(token);
                self.settle(token, Kernel::ExportKept(kept), Activity::Uncertain);
                Err(Failure::Retained(Box::new(custody)))
            }
        }
    }
    fn admit_export(&self, token: WorkspaceToken) -> Result<Arc<BoundWorkspace>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        if let Kernel::ExportKept(kept) = &value.kernel {
            return Err(Failure::Retained(Box::new(kept.custody(token))));
        }
        idle(value)?;
        if !matches!(value.kernel, Kernel::Unattached) {
            return Err(kernel_failure(
                ControlCode::Invalid,
                "attach:admission",
                "Workspace already owns a kernel connection",
            ));
        }
        value.kernel = Kernel::Attaching;
        value.activity = Activity::Attaching;
        value.epoch = value.epoch.saturating_add(1);
        Ok(value.workspace.clone())
    }
    fn export_admitted(
        &self,
        serving: &ExportServing,
        token: WorkspaceToken,
        workspace: &Arc<BoundWorkspace>,
    ) -> Result<Exported, Stop> {
        let config = &serving.config;
        let directory = serving.directory(token);
        let Some(text) = directory
            .to_str()
            .filter(|text| text.len() <= MOUNT_DIRECTORY_LIMIT)
            .map(str::to_owned)
        else {
            return Err(Stop::Unattached(kernel_failure(
                ControlCode::Invalid,
                "attach:directory",
                "mount directory exceeds the control record bound",
            )));
        };
        let unknown = |failure: Failure| {
            if failure.uncertain() {
                Stop::Unknown(failure)
            } else {
                Stop::Unattached(failure)
            }
        };
        // The same preparation as the FUSE Attach: the root serial, then the
        // initial authoritative serial range before any request can create.
        let operation = workspace
            .operation()
            .map_err(|error| Stop::Unattached(Failure::Workspace(error)))?;
        let root = operation
            .workspace()
            .base()
            .map(|base| base.root().root_inode().serial())
            .map_err(|error| Stop::Unattached(Failure::Workspace(error)))?;
        if let Err(error) = operation.workspace().prepare_serials(operation.ports()) {
            let failure = match operation.ports().failure() {
                Ok(Some(cause)) => match cause.as_ref() {
                    crate::store::PortError::History(error) => Failure::History(error.clone()),
                    _ => Failure::Workspace(error),
                },
                Ok(None) | Err(_) => Failure::Workspace(error),
            };
            return Err(unknown(failure));
        }
        drop(operation);
        if let Err(error) = std::fs::create_dir(&directory) {
            return Err(Stop::Unattached(kernel_failure(
                ControlCode::Failed,
                "attach:directory",
                format!("mount directory {text}: {error}"),
            )));
        }
        let mount = match self.job(workspace.route(), |client: &crate::OwnerClient, route| {
            client.kernel_mount(route, root)
        }) {
            Ok(mount) => mount,
            Err(failure) => {
                drop(std::fs::remove_dir(&directory));
                return Err(unknown(failure));
            }
        };
        let export = ExportConfig {
            path: "/".into(),
            uid: config.command_uid,
            gid: config.command_gid,
            read_only: false,
            refuse_apple_double: config.refuse_apple_double,
            lookups_held: config.lookups_held,
        };
        let started = TcpListener::bind((config.listen, 0)).and_then(|listener| {
            Server::start(
                listener,
                Arc::new(crate::service::Mounted(workspace.clone())),
                mount,
                export,
            )
        });
        let server = match started {
            Ok(server) => server,
            Err(error) => {
                drop(std::fs::remove_dir(&directory));
                return Err(self.unwind_export(
                    workspace,
                    mount,
                    None,
                    kernel_failure(ControlCode::Failed, "attach:export", error.to_string()),
                ));
            }
        };
        let port = server.local_addr().port();
        match run(&config.mounter.mount, port, &text) {
            Ran::Done => {}
            Ran::Busy(detail) | Ran::Failed(detail) => {
                let released = server.stop();
                drop(std::fs::remove_dir(&directory));
                return Err(self.unwind_export(
                    workspace,
                    mount,
                    Some(released),
                    kernel_failure(ControlCode::Failed, "attach:mount", detail),
                ));
            }
        }
        Ok(Exported {
            server: Arc::new(server),
            mount,
            ready: receipt(token, text, mount, port),
        })
    }
    /// No client mount and no export of this attempt remains: revoke its
    /// engine mount so the entry returns to Unattached.
    fn unwind_export(
        &self,
        workspace: &BoundWorkspace,
        mount: KernelMount,
        released: Option<Released>,
        failure: Failure,
    ) -> Stop {
        match self.job(workspace.route(), |client: &crate::OwnerClient, route| {
            client.kernel_revoke(route, mount)
        }) {
            Ok(()) => Stop::Unattached(failure),
            Err(revoke) => Stop::Retained(Box::new(ExportKept {
                ready: None,
                stage: TeardownStage::Revoke,
                detail: revoke.wire().detail,
                work: None,
                evidence: Box::new((failure, mount, released, revoke)),
                forced: None,
            })),
        }
    }
    /// Before a Commit: the client writes back what it buffered, and the
    /// export closes its implicit file descriptors. Runs outside the
    /// registry lock; a Workspace without an export has nothing to flush.
    pub(super) fn flush_export(&self, token: WorkspaceToken) -> Result<(), Failure> {
        let (server, directory) = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            match &super::registry::bound(&entries, token)?.kernel {
                Kernel::Exported(exported) => {
                    (exported.server.clone(), exported.ready.directory.clone())
                }
                _ => return Ok(()),
            }
        };
        let Some(serving) = self.export.as_ref() else {
            return Ok(());
        };
        match &serving.config.mounter.flush {
            None => nix::unistd::sync(),
            Some(helper) => match run(helper, server.local_addr().port(), &directory) {
                Ran::Done => {}
                Ran::Busy(detail) | Ran::Failed(detail) => {
                    return Err(kernel_failure(ControlCode::Failed, "commit:flush", detail))
                }
            },
        }
        server.close_files();
        Ok(())
    }
    /// After a Commit installed another root: the export's descriptors of the
    /// replaced tree are closed, and the inodes the install made stale that
    /// the client may cache are refreshed through the mount point (see
    /// `layerfs_nfs::refresh`): a mode-preserving chmod of each, at most
    /// [`REFRESHED`], on a thread of its own after this returns, so the
    /// Commit reply never waits on the client. False without an export.
    pub(super) fn installed_export(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        shown: layerfs_content::ObjectId,
        installed: layerfs_content::ObjectId,
    ) -> Result<bool, Failure> {
        let (server, directory) = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            match &super::registry::bound(&entries, token)?.kernel {
                Kernel::Exported(exported) => {
                    (exported.server.clone(), exported.ready.directory.clone())
                }
                _ => return Ok(false),
            }
        };
        server.installed();
        // `LAYERFS_NFS_REFRESH=0` leaves the client to its own revalidation
        // (`actimeo`, close-to-open): the comparison without the refresh.
        if std::env::var(REFRESH).is_ok_and(|value| value == "0") {
            return Ok(true);
        }
        let mut stale = std::collections::BTreeSet::new();
        // A walk that stops (at the bound, or failing) refreshes what it found;
        // the rest is revalidated on the client's own schedule.
        let _ = workspace.stale_after_install(shown, installed, |found| {
            stale.insert(match found {
                crate::store::Stale::Inode(serial) => serial,
                crate::store::Stale::Entry { parent, .. } => parent,
            });
            if stale.len() >= REFRESHED {
                return Err(std::io::Error::other("refresh bound"));
            }
            Ok(())
        });
        let paths = server.refresh(stale);
        if !paths.is_empty() {
            let root = PathBuf::from(directory);
            let _ = std::thread::Builder::new()
                .name("layerfs-nfs-refresh".into())
                .spawn(move || refresh(&root, &paths));
        }
        Ok(true)
    }
    /// Normal terminal unmount of an export-owning entry, admitted by
    /// [`take`]. A busy client mount is the reversible answer.
    pub(super) fn unmount_export(
        &self,
        token: WorkspaceToken,
        workspace: Arc<BoundWorkspace>,
        exported: Exported,
    ) -> Result<Success, Failure> {
        let helper = self
            .export
            .as_ref()
            .map(|serving| serving.config.mounter.unmount.clone());
        let port = exported.server.local_addr().port();
        let ran = match &helper {
            Some(helper) => run(helper, port, &exported.ready.directory),
            None => Ran::Failed("export serving not assembled".into()),
        };
        match ran {
            Ran::Done => {}
            Ran::Busy(detail) => {
                self.settle(token, Kernel::Exported(Box::new(exported)), Activity::Idle);
                return Err(kernel_failure(
                    ControlCode::Busy,
                    "unmount:kernel",
                    format!("client reported the mount busy; no terminal effect: {detail}"),
                ));
            }
            Ran::Failed(detail) => {
                return Err(self.keep_export(
                    token,
                    TeardownStage::Detach,
                    format!("client unmount: {detail}"),
                    exported,
                    None,
                ))
            }
        }
        self.teardown_export(token, &workspace, exported, None)?;
        self.close(token, &workspace, None)
    }
    /// After the client mount is gone: stop the export, revoke the engine
    /// mount, remove the directory and leave the entry Unattached for Close.
    fn teardown_export(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        exported: Exported,
        forced: Option<ForcedFacts>,
    ) -> Result<(), Failure> {
        self.export_phase(token, MountPhase::Draining, Some(work(&exported.server)));
        let Exported {
            server,
            mount,
            ready,
        } = exported;
        let released = match Arc::try_unwrap(server) {
            Ok(server) => server.stop(),
            Err(server) => {
                return Err(self.keep_export(
                    token,
                    TeardownStage::Requests,
                    "export still shared by a running control step".into(),
                    Exported {
                        server,
                        mount,
                        ready,
                    },
                    forced,
                ))
            }
        };
        if let Err(failure) = self.job(workspace.route(), |client: &crate::OwnerClient, route| {
            client.kernel_revoke(route, mount)
        }) {
            let custody = ExportKept {
                ready: Some(ready),
                stage: TeardownStage::Revoke,
                detail: failure.wire().detail,
                work: None,
                evidence: Box::new((failure, mount, released)),
                forced,
            };
            let reply = custody.custody(token);
            self.settle(
                token,
                Kernel::ExportKept(Box::new(custody)),
                Activity::Uncertain,
            );
            return Err(Failure::Retained(Box::new(reply)));
        }
        drop(std::fs::remove_dir(&ready.directory));
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(binding) = bound_mut(&mut entries, token) {
                binding.kernel = Kernel::Unattached;
            }
        }
        self.released
            .lock()
            .map(|mut last| *last = Some((token, released)))
            .ok();
        Ok(())
    }
    fn keep_export(
        &self,
        token: WorkspaceToken,
        stage: TeardownStage,
        detail: String,
        exported: Exported,
        forced: Option<ForcedFacts>,
    ) -> Failure {
        let kept = ExportKept {
            ready: Some(exported.ready.clone()),
            stage,
            detail,
            work: Some(work(&exported.server)),
            evidence: Box::new(exported),
            forced,
        };
        let custody = kept.custody(token);
        self.settle(
            token,
            Kernel::ExportKept(Box::new(kept)),
            Activity::Uncertain,
        );
        Failure::Retained(Box::new(custody))
    }
    fn export_phase(&self, token: WorkspaceToken, phase: MountPhase, work: Option<MountWork>) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Ok(binding) = bound_mut(&mut entries, token) {
                if let Kernel::ExportLeaving(leaving) = &mut binding.kernel {
                    leaving.phase = phase;
                    if let Some(work) = work {
                        leaving.work = work;
                    }
                }
            }
        }
    }
    /// Describes retained export custody for an operator. Observation only.
    pub fn retained_export(&self, token: WorkspaceToken) -> Result<Option<String>, Failure> {
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        Ok(match &bound_mut(&mut entries, token)?.kernel {
            Kernel::ExportKept(kept) => Some(format!("{kept:?}")),
            _ => None,
        })
    }
    /// What the last stopped export of `token` gave back (diagnostics).
    pub fn export_released(&self, token: WorkspaceToken) -> Option<Released> {
        let last = self.released.lock().ok()?;
        last.filter(|(seen, _)| *seen == token)
            .map(|(_, released)| released)
    }
    /// The current holdings of a serving export (diagnostics).
    pub fn export_held(&self, token: WorkspaceToken) -> Option<Released> {
        let entries = self.entries.lock().ok()?;
        match &super::registry::bound(&entries, token).ok()?.kernel {
            Kernel::Exported(exported) => Some(exported.server.held()),
            _ => None,
        }
    }
    /// Forced terminal unmount of an export-owning entry: the same
    /// before-effect guard as FUSE, one forced client unmount, then the
    /// normal teardown and Close.
    pub(super) fn force_export(
        &self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<Success, Failure> {
        let (workspace, exported, commit) = {
            let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound_mut(&mut entries, token)?;
            let commit = super::force::guard(value, token, relinquish_unknown)?;
            let exported = take(&mut value.kernel, MountPhase::Stopping)?;
            value.activity = Activity::Closing;
            value.epoch = value.epoch.saturating_add(1);
            (value.workspace.clone(), exported, commit)
        };
        let mut facts = ForcedFacts {
            abort: AbortDisposition::Written,
            detach: DetachDisposition::NotAttempted,
            commit,
            fenced: 0,
        };
        let helper = self
            .export
            .as_ref()
            .map(|serving| serving.config.mounter.force.clone());
        let port = exported.server.local_addr().port();
        let ran = match &helper {
            Some(helper) => run(helper, port, &exported.ready.directory),
            None => Ran::Failed("export serving not assembled".into()),
        };
        match ran {
            Ran::Done => facts.detach = DetachDisposition::Detached,
            Ran::Busy(detail) | Ran::Failed(detail) => {
                facts.detach = DetachDisposition::Failed;
                return Err(self.keep_export(
                    token,
                    TeardownStage::Detach,
                    format!("forced client unmount: {detail}"),
                    exported,
                    Some(facts),
                ));
            }
        }
        let work = work(&exported.server);
        self.teardown_export(token, &workspace, exported, Some(facts.clone()))?;
        let kernel = self.closed(token, &workspace, None, Some(facts.clone()))?;
        let (cleanup, observation_failure) = match self
            .owner
            .observe_cleanup(None, token.namespace, token.workspace.to_bytes())
            .map_err(Failure::Owner)
        {
            Ok(CleanupState::Held) => (ForcedCleanup::Held, None),
            Ok(CleanupState::Queued) => (ForcedCleanup::Queued, None),
            Ok(CleanupState::Gone) => (ForcedCleanup::Gone, None),
            Ok(CleanupState::Live) => (
                ForcedCleanup::Unobserved,
                Some(Box::new(Failure::Rejected(
                    ControlCode::Unknown,
                    "closed Workspace remains live",
                ))),
            ),
            Err(error) => (ForcedCleanup::Unobserved, Some(Box::new(error))),
        };
        self.depart(
            token,
            Success {
                reply: Reply::ForceUnmounted(Box::new(ForceUnmounted {
                    token,
                    outcome: ForcedOutcome {
                        facts,
                        work,
                        cleanup,
                    },
                })),
                commit: None,
                observation_failure,
                kernel,
            },
        )
    }
}
/// Terminal admission over an export-owning entry; the entry observes the
/// export as leaving until the operation settles it.
pub(super) fn take(kernel: &mut Kernel, phase: MountPhase) -> Result<Exported, Failure> {
    match kernel {
        Kernel::Exported(_) => {}
        Kernel::ExportLeaving(_) => {
            return Err(kernel_failure(
                ControlCode::Busy,
                "unmount:admission",
                "terminal unmount already admitted",
            ))
        }
        _ => {
            return Err(kernel_failure(
                ControlCode::Invalid,
                "unmount:admission",
                "Workspace owns no export",
            ))
        }
    }
    let Kernel::Exported(exported) = std::mem::replace(kernel, Kernel::Unattached) else {
        unreachable!("checked export")
    };
    *kernel = Kernel::ExportLeaving(Box::new(ExportLeaving {
        ready: exported.ready.clone(),
        phase,
        work: work(&exported.server),
    }));
    Ok(*exported)
}

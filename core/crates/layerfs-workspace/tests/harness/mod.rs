//! Direct single-owner Workspace harness over the shared content-built root.
//! Each helper is one bounded source window, like one native request.
#![allow(dead_code)]
use crate::common::{fixture, name, Fixture};
use layerfs_content::filesystem::PathName;
use layerfs_content::ContentError;
use layerfs_overlay::{Overlay, ProfileConfig, Route};
use layerfs_workspace::{
    CanonicalClient, InodeSerials, JobOutcome, NamespaceJob, Operation, Outcome, OverlayJobs,
    OverlayRead, Refusal, SourceView, Time, ViewStat, Workspace, WorkspaceError, WorkspaceResult,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

pub const T1: Time = Time {
    seconds: 1_700_000_000,
    nanoseconds: 5,
};
pub const T2: Time = Time {
    seconds: -2,
    nanoseconds: 800_000_000,
};
/// Another complete operation run between two owner rounds of one operation.
pub type Interleaved = Box<dyn FnOnce(&Bench)>;
/// The scope allocator of the test: unique ascending ranges, counted calls.
pub struct Allocator {
    pub next: AtomicU64,
    pub calls: AtomicU64,
}
impl InodeSerials for Allocator {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok((self.next.fetch_add(count, Ordering::Relaxed), count))
    }
}
pub struct Bench {
    pub fixture: Fixture,
    pub overlay: Overlay,
    pub workspace: Workspace,
    pub allocator: Allocator,
    pub rounds: Cell<u64>,
    /// Runs once, after the first owner round of the next operation.
    pub between: RefCell<Option<Interleaved>>,
    owners: Cell<u64>,
    path: PathBuf,
}
impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
/// Counts owner rounds and lets a test interleave another complete operation
/// between two rounds of one operation, through the public job port only.
pub struct Port<'a>(pub &'a Bench);
impl OverlayRead for Port<'_> {
    fn inode(
        &self,
        source: layerfs_overlay::BaseSource,
        serial: u64,
    ) -> WorkspaceResult<Option<layerfs_overlay::Inode>> {
        OverlayRead::inode(&self.0.overlay, source, serial)
    }
    fn directory_entry(
        &self,
        source: layerfs_overlay::BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<layerfs_overlay::DirectoryEntry>> {
        OverlayRead::directory_entry(&self.0.overlay, source, parent, name)
    }
    fn directory_entries(
        &self,
        source: layerfs_overlay::BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<layerfs_overlay::DirectoryEntryWindow> {
        OverlayRead::directory_entries(&self.0.overlay, source, parent, after)
    }
    fn read(
        &self,
        source: layerfs_overlay::BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<layerfs_overlay::LocalRead>> {
        OverlayRead::read(self.local(), source, serial, offset, length)
    }
    fn cell(
        &self,
        source: layerfs_overlay::BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> WorkspaceResult<Option<layerfs_overlay::Cell>> {
        OverlayRead::cell(&self.0.overlay, source, serial, generation, offset)
    }
}
impl Port<'_> {
    fn local(&self) -> &Overlay {
        &self.0.overlay
    }
}
impl OverlayJobs for Port<'_> {
    fn namespace(&self, job: &NamespaceJob) -> WorkspaceResult<JobOutcome> {
        self.0.rounds.set(self.0.rounds.get() + 1);
        let outcome = job.perform(&self.0.overlay);
        let interleaved = self.0.between.borrow_mut().take();
        if let Some(interleaved) = interleaved {
            interleaved(self.0);
        }
        outcome
    }
}
impl Bench {
    pub fn new(tag: &str) -> Self {
        Self::with_cache(tag, 4 * 1024 * 1024)
    }
    pub fn with_cache(tag: &str, cache: usize) -> Self {
        let fixture = fixture();
        let client = Arc::new(CanonicalClient::with_lengths(
            Arc::new(fixture.store.clone()),
            Arc::new(fixture.store.clone()),
            cache,
        ));
        let path = std::env::temp_dir().join(format!(
            "layerfs-namespace-{tag}-{}.sqlite",
            std::process::id()
        ));
        let overlay = Overlay::create(&path, ProfileConfig::default()).unwrap();
        let workspace =
            Workspace::open(&overlay, client, fixture.root, fixture.scope, [42; 32]).unwrap();
        Self {
            fixture,
            overlay,
            workspace,
            allocator: Allocator {
                next: AtomicU64::new(1000),
                calls: AtomicU64::new(0),
            },
            rounds: Cell::new(0),
            between: RefCell::new(None),
            owners: Cell::new(0),
            path,
        }
    }
    pub fn route(&self) -> Route {
        self.workspace.route()
    }
    pub fn demand(&self) -> u64 {
        self.fixture.store.demand.load(Ordering::Relaxed)
    }
    /// One owned source window around `work`, released after it completes.
    pub fn window<T>(&self, work: impl FnOnce(&SourceView) -> T) -> T {
        self.owners.set(self.owners.get() + 1);
        let source = self
            .overlay
            .acquire_base_source(self.route(), self.owners.get())
            .unwrap();
        let view = self.workspace.view_for_source(source).unwrap();
        let result = work(&view);
        self.overlay.release_base_source(source).unwrap();
        result
    }
    /// One complete operation including its reply-send attempt.
    pub fn run(&self, operation: Operation, now: Time) -> WorkspaceResult<Outcome> {
        let outcome = self.window(|view| {
            self.workspace
                .mutate(&Port(self), &self.allocator, view, operation, now)
        });
        if let Ok(Outcome::Applied { publication, .. }) = &outcome {
            self.overlay.reply_attempted(*publication).unwrap();
        }
        outcome
    }
    pub fn applied(&self, operation: Operation, now: Time) -> Option<ViewStat> {
        match self.run(operation, now).unwrap() {
            Outcome::Applied { stat, .. } => stat,
            other => panic!("expected a publication, got {other:?}"),
        }
    }
    pub fn refused(&self, operation: Operation) -> Refusal {
        let before = self.overlay.state(self.route()).unwrap();
        let refusal = match self.run(operation, T1) {
            Err(WorkspaceError::Refused(refusal)) => refusal,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(
            self.overlay.state(self.route()).unwrap(),
            before,
            "a refusal publishes nothing"
        );
        refusal
    }
    pub fn stat(&self, serial: u64) -> WorkspaceResult<ViewStat> {
        self.window(|view| view.stat(&self.overlay, serial))
    }
    pub fn lookup(&self, parent: u64, child: &str) -> Option<ViewStat> {
        match self.window(|view| view.lookup(&self.overlay, parent, &name(child))) {
            Ok(stat) => Some(stat),
            Err(WorkspaceError::Content(ContentError::PathNotFound)) => None,
            Err(error) => panic!("lookup {child}: {error:?}"),
        }
    }
    /// Complete listing through bounded windows, with the largest visit count.
    pub fn list(&self, parent: u64) -> (Vec<(String, u64)>, usize) {
        let mut all = Vec::new();
        let mut after: Option<Vec<u8>> = None;
        let mut widest = 0;
        loop {
            let page = self
                .window(|view| view.list(&self.overlay, parent, after.as_deref()))
                .unwrap();
            widest = widest.max(page.visited);
            all.extend(
                page.entries
                    .into_iter()
                    .map(|(name, serial)| (String::from_utf8(name).unwrap(), serial)),
            );
            match page.continuation {
                Some(next) => after = Some(next),
                None => return (all, widest),
            }
        }
    }
    pub fn names(&self, parent: u64) -> Vec<String> {
        self.list(parent)
            .0
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }
}
pub fn create(parent: u64, child: &str) -> Operation {
    Operation::Create {
        parent,
        name: name(child),
        mode: 0o640,
    }
}
pub fn mkdir(parent: u64, child: &str) -> Operation {
    Operation::Mkdir {
        parent,
        name: name(child),
        mode: 0o755,
    }
}
pub fn unlink(parent: u64, child: &str) -> Operation {
    Operation::Unlink {
        parent,
        name: name(child),
    }
}
pub fn rmdir(parent: u64, child: &str) -> Operation {
    Operation::Rmdir {
        parent,
        name: name(child),
    }
}
pub fn link(serial: u64, parent: u64, child: &str) -> Operation {
    Operation::Link {
        serial,
        parent,
        name: name(child),
    }
}
pub fn path(steps: &[&str]) -> Option<Vec<PathName>> {
    Some(steps.iter().map(|step| name(step)).collect())
}
pub fn rename(
    from: (u64, &str),
    to: (u64, &str),
    replace: bool,
    destination_path: Option<Vec<PathName>>,
) -> Operation {
    Operation::Rename {
        parent: from.0,
        name: name(from.1),
        new_parent: to.0,
        new_name: name(to.1),
        replace,
        destination_path,
    }
}
pub fn write(serial: u64, offset: u64, data: &[u8]) -> Operation {
    Operation::Write {
        serial,
        position: layerfs_workspace::Position::At(offset),
        data: data.into(),
    }
}
pub fn append(serial: u64, data: &[u8]) -> Operation {
    Operation::Write {
        serial,
        position: layerfs_workspace::Position::End,
        data: data.into(),
    }
}
pub fn resize(serial: u64, size: u64) -> Operation {
    Operation::SetAttributes {
        serial,
        mode: None,
        mtime: None,
        size: Some(size),
    }
}
impl Bench {
    /// The whole file through bounded read windows.
    pub fn content(&self, serial: u64) -> Vec<u8> {
        let mut all = Vec::new();
        loop {
            let read = self
                .window(|view| {
                    view.read(
                        &self.overlay,
                        serial,
                        all.len() as u64,
                        layerfs_overlay::READ_WINDOW as u32,
                        &mut all,
                    )
                })
                .unwrap();
            if read < layerfs_overlay::READ_WINDOW as u64 {
                return all;
            }
        }
    }
    pub fn read_at(&self, serial: u64, offset: u64, length: u32) -> Vec<u8> {
        let mut out = Vec::new();
        self.window(|view| view.read(&self.overlay, serial, offset, length, &mut out))
            .unwrap();
        out
    }
}

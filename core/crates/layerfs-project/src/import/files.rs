//! Constructs each native regular-file identity once into the single Save owner.
//!
//! The owning thread alone reads the backing and accepts objects. It feeds a
//! fixed admission window from the backed identity stream; constructors return
//! bounded ordered batches, and canonical inode consumption releases each slot.
//! Roots never enter a Store acquisition row or grow with the acquired root.
mod completion;
use crate::backing::{Backing, Stamp, WINDOW_ROWS};
use crate::error::content;
use crate::error::ProjectError as Failure;
use crate::{
    batch::{BatchProducer, ImportBatch, QUEUE_SLOTS},
    namespace::ImportProgress,
};
pub(crate) use completion::FileCompletions;
use layerfs_content::{construct_stream, ObjectId};
use layerfs_storage::port::acquisition;
use layerfs_storage::{Save, Storage};
use layerfs_telemetry::timer::Timing;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::Instant,
};

const INIT_WORKERS: usize = 4;

/// The first path of one native identity with the evidence it must keep matching.
struct Job {
    /// Canonical position, shared by every later path of the identity.
    id: u64,
    path: PathBuf,
    stamp: Stamp,
}
impl Job {
    fn of(job: acquisition::Job) -> Self {
        Self {
            id: job.position,
            stamp: Stamp::from_identity(&job.native),
            path: PathBuf::from(OsString::from_vec(job.native_path)),
        }
    }
}

/// Constructs identities while the closure consumes their roots in canonical
/// order into the same unfinished Save. The final filesystem root is emitted
/// only after this scope succeeds and every constructor has stopped.
pub(crate) fn with_completions<'b, 'p, 's, T>(
    backing: &'b Backing<'p>,
    aliases: usize,
    store: &Storage,
    save: &'b Save<'s>,
    progress: &mut ImportProgress,
    consume: impl FnOnce(&mut FileCompletions<'b, 'p, 's>, &mut ImportProgress) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let deadline = progress.deadline();
    let (feed, jobs) = mpsc::sync_channel::<Job>(WINDOW_ROWS);
    let jobs = Mutex::new(jobs);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|workers| -> Result<T, Failure> {
        let (sender, receiver) = mpsc::sync_channel::<ImportBatch>(QUEUE_SLOTS);
        let mut handles = Vec::with_capacity(INIT_WORKERS);
        for _ in 0..INIT_WORKERS {
            let sender = sender.clone();
            let jobs = &jobs;
            let cancelled = &cancelled;
            handles.push(workers.spawn(move || {
                let mut producer = BatchProducer::new(&sender);
                while let Some(job) = next_job(jobs, &mut producer, cancelled) {
                    let result = construct_file(
                        &job,
                        policy,
                        &capacities,
                        &mut producer,
                        deadline,
                        cancelled,
                    );
                    let failed = result.is_err();
                    if producer.done(job.id, result).is_err() || failed {
                        return;
                    }
                }
                let _ = producer.flush();
            }));
        }
        drop(sender);
        let mut completed = FileCompletions::new(backing, save, deadline, feed, receiver);
        let mut result = (|| {
            completed.admit()?;
            let result = consume(&mut completed, progress)?;
            completed.finish()?;
            if aliases > 0 {
                check_aliases(backing, progress)?;
            }
            Ok(result)
        })();
        completed.counters(&mut progress.work);
        cancelled.store(true, Ordering::Release);
        completed.close();
        for handle in handles {
            if handle.join().is_err() && result.is_ok() {
                result = Err(Failure::WorkerUnavailable);
            }
        }
        result
    })
}

/// Takes one job. A constructor hands over its pending batch before it waits
/// on the queue or its lock, so the owner always receives the completions
/// that let it refill the queue.
fn next_job(
    jobs: &Mutex<mpsc::Receiver<Job>>,
    producer: &mut BatchProducer<'_>,
    cancelled: &AtomicBool,
) -> Option<Job> {
    if cancelled.load(Ordering::Acquire) {
        return None;
    }
    if let Ok(queue) = jobs.try_lock() {
        match queue.try_recv() {
            Ok(job) => return Some(job),
            Err(mpsc::TryRecvError::Disconnected) => return None,
            Err(mpsc::TryRecvError::Empty) => (),
        }
    }
    producer.flush().ok()?;
    let queue = jobs.lock().ok()?;
    if cancelled.load(Ordering::Acquire) {
        None
    } else {
        queue.recv().ok()
    }
}

/// Every later path of a constructed identity must still name that same file.
///
/// Entries stream in acquisition order, so later paths of one directory share
/// one path read and adjacent later paths of one identity share one evidence read.
fn check_aliases(backing: &Backing<'_>, progress: &mut ImportProgress) -> Result<(), Failure> {
    let mut entries = backing.entries();
    let mut directory: Option<(u64, PathBuf)> = None;
    let mut first: Option<(u64, Stamp)> = None;
    while let Some(entry) = entries.next()? {
        progress.tick()?;
        let Some(canonical) = entry.canonical.filter(|first| *first != entry.position) else {
            continue;
        };
        let parent = entry.key.parent.ok_or(Failure::InvalidInput)?;
        if directory.as_ref().is_none_or(|(held, _)| *held != parent) {
            let path = OsString::from_vec(backing.directory_path(parent)?);
            directory = Some((parent, PathBuf::from(path)));
        }
        if first.is_none_or(|(held, _)| held != canonical) {
            let job = backing.job(canonical)?;
            first = Some((canonical, Stamp::from_identity(&job.native)));
        }
        let (Some((_, directory)), Some((_, stamp))) = (&directory, &first) else {
            return Err(Failure::InvalidInput);
        };
        let path = directory.join(OsStr::from_bytes(&entry.key.name));
        if !stamp.matches_file(&fs::symlink_metadata(&path)?) {
            return Err(Failure::InvalidInput);
        }
    }
    Ok(())
}

fn construct_file(
    job: &Job,
    policy: layerfs_content::ConstructionPolicy,
    capacities: &layerfs_content::ConstructionCapacities,
    producer: &mut BatchProducer<'_>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ObjectId, Failure> {
    if Instant::now() >= deadline {
        return Err(Failure::Deadline);
    }
    let file = File::open(&job.path)?;
    if !job.stamp.matches_file(&file.metadata()?) {
        return Err(Failure::InvalidInput);
    }
    let mut source = Source {
        file,
        cancelled,
        deadline,
        deadline_expired: false,
    };
    let (result, _) = Timing::disabled("history.import_file", |scope| {
        construct_stream(
            policy,
            capacities,
            &mut source,
            producer,
            scope.child("content.construct"),
        )
    });
    let built = result.map_err(|error| {
        if source.deadline_expired {
            Failure::Deadline
        } else {
            content(error)
        }
    })?;
    if built.logical_len != job.stamp.len
        || !job.stamp.matches_file(&source.file.metadata()?)
        || !job.stamp.matches_file(&fs::symlink_metadata(&job.path)?)
    {
        return Err(Failure::InvalidInput);
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline);
    }
    Ok(built.root)
}

/// Cancellation is checked between real reads, so a failed consumer does not
/// require another worker's entire file to finish before the scoped join.
struct Source<'a> {
    file: File,
    cancelled: &'a AtomicBool,
    deadline: Instant,
    deadline_expired: bool,
}
impl Read for Source<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::other("namespace file construction stopped"));
        }
        if Instant::now() >= self.deadline {
            self.deadline_expired = true;
            return Err(io::Error::other("namespace file construction deadline"));
        }
        self.file.read(buffer)
    }
}

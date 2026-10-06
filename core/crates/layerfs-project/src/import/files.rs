//! Constructs each native regular-file identity once into the single Save owner.
//!
//! The owning thread alone reads the backing and accepts objects. It feeds a
//! fixed job queue from the backed identity stream; constructors return bounded
//! ordered batches, and each constructed root is recorded against its identity
//! in bounded write windows. Neither queue grows with the acquired root.
use crate::backing::{Backing, Stamp, WINDOW_ROWS};
use crate::error::ProjectError as Failure;
use crate::error::{content, storage};
use crate::{
    batch::{BatchProducer, Event, ImportBatch, QUEUE_SLOTS},
    namespace::ImportProgress,
};
use layerfs_content::{construct_stream, ObjectId};
use layerfs_storage::port::acquisition::{self, WRITE_WINDOW_ROWS};
use layerfs_storage::{Save, Storage};
use layerfs_telemetry::timer::Timing;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
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

/// Constructs every native identity once and records its root in backing.
pub(crate) fn save_files(
    backing: &Backing<'_>,
    aliases: usize,
    store: &Storage,
    save: &Save<'_>,
    progress: &mut ImportProgress,
) -> Result<(), Failure> {
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let deadline = progress.deadline();
    let (feed, jobs) = mpsc::sync_channel::<Job>(WINDOW_ROWS);
    let jobs = Mutex::new(jobs);
    let mut natives = backing.jobs();
    let mut roots: Vec<(u64, ObjectId)> = Vec::new();
    std::thread::scope(|workers| -> Result<(), Failure> {
        let mut feed = Some(feed);
        let (sender, receiver) = mpsc::sync_channel::<ImportBatch>(QUEUE_SLOTS);
        for _ in 0..INIT_WORKERS {
            let sender = sender.clone();
            let jobs = &jobs;
            workers.spawn(move || {
                let mut producer = BatchProducer::new(&sender);
                while let Some(job) = next_job(jobs, &mut producer) {
                    let result = construct_file(&job, policy, &capacities, &mut producer, deadline);
                    if producer.done(job.id as usize, result).is_err() {
                        return;
                    }
                }
                let _ = producer.flush();
            });
        }
        drop(sender);
        let (mut sent, mut finished) = (0usize, 0usize);
        // The one job the full queue handed back, offered again before the next.
        let mut held: Option<Job> = None;
        loop {
            progress.tick()?;
            while let Some(sender) = &feed {
                let job = match held.take() {
                    Some(job) => job,
                    None => match natives.next()? {
                        Some(job) => Job::of(job),
                        None => {
                            feed = None;
                            break;
                        }
                    },
                };
                match sender.try_send(job) {
                    Ok(()) => sent += 1,
                    Err(mpsc::TrySendError::Full(job)) => {
                        held = Some(job);
                        break;
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        return Err(Failure::WorkerUnavailable)
                    }
                }
            }
            if feed.is_none() && finished == sent {
                return Ok(());
            }
            let batch = match receiver.recv_timeout(Duration::from_millis(500)) {
                Ok(batch) => batch,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Failure::WorkerUnavailable)
                }
            };
            for event in batch.events {
                match event {
                    Event::Object(object) => save.accept(object).map_err(storage)?,
                    Event::Done(id, result) => {
                        roots.push((id as u64, result?));
                        finished += 1;
                        if roots.len() == WRITE_WINDOW_ROWS {
                            backing.complete_files(&mut roots)?;
                        }
                    }
                }
            }
        }
    })?;
    backing.complete_files(&mut roots)?;
    if aliases > 0 {
        check_aliases(backing, progress)?;
    }
    Ok(())
}

/// Takes one job. A constructor hands over its pending batch before it waits
/// on the queue or its lock, so the owner always receives the completions
/// that let it refill the queue.
fn next_job(jobs: &Mutex<mpsc::Receiver<Job>>, producer: &mut BatchProducer<'_>) -> Option<Job> {
    if let Ok(queue) = jobs.try_lock() {
        match queue.try_recv() {
            Ok(job) => return Some(job),
            Err(mpsc::TryRecvError::Disconnected) => return None,
            Err(mpsc::TryRecvError::Empty) => (),
        }
    }
    producer.flush().ok()?;
    jobs.lock().ok()?.recv().ok()
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
) -> Result<ObjectId, Failure> {
    if Instant::now() >= deadline {
        return Err(Failure::Deadline);
    }
    let mut file = File::open(&job.path)?;
    if !job.stamp.matches_file(&file.metadata()?) {
        return Err(Failure::InvalidInput);
    }
    let (result, _) = Timing::disabled("history.import_file", |scope| {
        construct_stream(
            policy,
            capacities,
            &mut file,
            producer,
            scope.child("content.construct"),
        )
    });
    let built = result.map_err(content)?;
    if built.logical_len != job.stamp.len
        || !job.stamp.matches_file(&file.metadata()?)
        || !job.stamp.matches_file(&fs::symlink_metadata(&job.path)?)
    {
        return Err(Failure::InvalidInput);
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline);
    }
    Ok(built.root)
}

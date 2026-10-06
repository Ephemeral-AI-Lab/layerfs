//! Constructs each native regular-file identity once into the single Save owner.
//!
//! The owning thread alone reads the scratch runs and accepts objects. It feeds
//! a fixed job queue from the backed identity stream; constructors return
//! bounded ordered batches, and each constructed root is ordered by position in
//! backing. Neither queue grows with the acquired root.
use crate::error::ProjectError as Failure;
use crate::error::{content, storage};
use crate::{
    batch::{BatchProducer, Event, ImportBatch, QUEUE_SLOTS},
    namespace::ImportProgress,
    runs::{Cursor, Run, Sorter},
    scan::Identities,
    scratch::{by_position, Job, Rooted, Scratch, WINDOW_ROWS},
};
use layerfs_content::{construct_stream, ObjectId};
use layerfs_storage::{Save, Storage};
use layerfs_telemetry::timer::Timing;
use std::{
    fs::{self, File},
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

const INIT_WORKERS: usize = 4;

/// Returns every constructed regular-file root in position order.
pub(crate) fn save_files(
    identities: &Identities,
    scratch: &mut Scratch,
    store: &Storage,
    save: &Save<'_>,
    progress: &mut ImportProgress,
) -> Result<Run, Failure> {
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let deadline = progress.deadline();
    let (feed, jobs) = mpsc::sync_channel::<Job>(WINDOW_ROWS);
    let jobs = Mutex::new(jobs);
    let mut natives = Cursor::new(&identities.jobs, Job::native);
    let mut roots = Sorter::new(by_position, deadline);
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
        loop {
            progress.tick()?;
            while let Some(sender) = &feed {
                if natives.peek()?.is_none() {
                    feed = None;
                    break;
                }
                let job = natives.take().ok_or(Failure::InvalidInput)?;
                match sender.try_send(job) {
                    Ok(()) => sent += 1,
                    Err(mpsc::TrySendError::Full(job)) => {
                        natives.restore(job);
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
                        let root = Rooted::record(id as u64, result?);
                        roots.push(scratch.backing(), &root)?;
                        finished += 1;
                    }
                }
            }
        }
    })?;
    let work = &mut progress.work;
    work.job_capacity_bytes = work
        .job_capacity_bytes
        .max(natives.resident_bytes() + WINDOW_ROWS * std::mem::size_of::<Job>());
    let contents = roots.finish(scratch.backing())?;
    work.sort_capacity_bytes = work.sort_capacity_bytes.max(roots.resident_bytes());
    check_aliases(&identities.aliases, progress)?;
    Ok(contents)
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
fn check_aliases(aliases: &Run, progress: &mut ImportProgress) -> Result<(), Failure> {
    let mut aliases = Cursor::new(aliases, Job::alias);
    while let Some(alias) = aliases.next()? {
        progress.tick()?;
        if !alias
            .stamp
            .matches_file(&fs::symlink_metadata(&alias.path)?)
        {
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

//! One operator-bound directory import into a single C2 save.
use crate::error::ProjectError as Failure;
use crate::error::{content, storage};
use crate::{
    batch::{BatchProducer, Event, ImportBatch, QUEUE_SLOTS},
    namespace::{ImportProgress, PreparedEntry},
};
use layerfs_content::{construct_stream, ObjectId, PathName};
use layerfs_history::RecordKind;
use layerfs_storage::{Save, Storage};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::{
    collections::VecDeque,
    fs::{self, File, Metadata},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

const INIT_WORKERS: usize = 4;

struct Job {
    index: usize,
    path: PathBuf,
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
    mode: u32,
}

pub(crate) fn scan_and_save(
    source: &Path,
    store: &Storage,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> Result<Vec<PreparedEntry>, Failure> {
    let root_metadata = fs::symlink_metadata(source)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(Failure::InvalidInput);
    }
    let save = store.begin_save().map_err(storage)?;

    let scanned = timer.child("history.import_scan").run(|_| {
        let mut entries = vec![entry(
            0,
            Vec::new(),
            RecordKind::Directory,
            &root_metadata,
            None,
        )?];
        let mut jobs = Vec::new();
        let mut pending = VecDeque::from([(source.to_path_buf(), 0usize)]);
        while let Some((directory, parent)) = pending.pop_front() {
            progress.work.frontier = progress.work.frontier.max(pending.len() + 1);
            progress.work.frontier_capacity_bytes = progress.work.frontier_capacity_bytes.max(
                pending.capacity() * std::mem::size_of::<(PathBuf, usize)>()
                    + pending.iter().map(|(p, _)| p.capacity()).sum::<usize>()
                    + directory.capacity(),
            );
            progress.tick()?;
            let mut children = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
            progress.work.directory_children = progress.work.directory_children.max(children.len());
            progress.work.child_vector_bytes = progress
                .work
                .child_vector_bytes
                .max(children.capacity() * std::mem::size_of::<fs::DirEntry>());
            children.sort_by(|a, b| a.file_name().as_bytes().cmp(b.file_name().as_bytes()));
            for child in children {
                progress.tick()?;
                let path = child.path();
                let name = child.file_name().as_bytes().to_vec();
                PathName::from_bytes(&name).map_err(content)?;
                let metadata = fs::symlink_metadata(&path)?;
                let kind = if metadata.file_type().is_symlink() {
                    RecordKind::Symlink
                } else if metadata.is_dir() {
                    RecordKind::Directory
                } else if metadata.is_file() {
                    RecordKind::RegularFile
                } else {
                    return Err(Failure::Unsupported);
                };
                if kind == RecordKind::RegularFile {
                    jobs.push(Job {
                        index: entries.len(),
                        path: path.clone(),
                        dev: metadata.dev(),
                        ino: metadata.ino(),
                        len: metadata.len(),
                        mtime: metadata.mtime(),
                        mtime_nsec: metadata.mtime_nsec(),
                        ctime: metadata.ctime(),
                        ctime_nsec: metadata.ctime_nsec(),
                        mode: metadata.mode(),
                    });
                }
                let index = entries.len();
                let mut prepared = entry(parent, name, kind, &metadata, None)?;
                if kind == RecordKind::Symlink {
                    prepared.target = Some(super::source::read_link(&path, &metadata)?);
                }
                entries.push(prepared);
                if kind == RecordKind::Directory {
                    pending.push_back((path, index));
                }
            }
        }
        let groups = prepare_aliases(&mut entries, &mut jobs)?;
        progress.work.unique_files = groups;
        progress.work.regular_aliases = jobs.len() - groups;
        progress.work.entries = entries.len();
        progress.work.entry_capacity_bytes = entries.capacity()
            * std::mem::size_of::<PreparedEntry>()
            + entries
                .iter()
                .map(|e| e.name.capacity() + e.target.as_ref().map_or(0, Vec::capacity))
                .sum::<usize>();
        progress.work.jobs = jobs.len();
        progress.work.job_capacity_bytes = jobs.capacity() * std::mem::size_of::<Job>()
            + jobs.iter().map(|j| j.path.capacity()).sum::<usize>();
        Ok((entries, jobs))
    });
    let scanned = scanned.and_then(|(mut entries, jobs)| {
        timer
            .child("history.import_files")
            .run(|_| save_files(&mut entries, jobs, store, &save, progress))?;
        Ok(entries)
    });
    let scanned = scanned.and_then(|entries| {
        progress.tick()?;
        Ok(entries)
    });
    let entries = scanned?;
    timer
        .child("history.import_finish_save")
        .run(|_| save.finish().map_err(storage))?;
    Ok(entries)
}

fn save_files(
    entries: &mut [PreparedEntry],
    jobs: Vec<Job>,
    store: &Storage,
    save: &Save<'_>,
    progress: &mut ImportProgress,
) -> Result<(), Failure> {
    let remaining = progress.work.unique_files;
    // The already retained job vector is sorted by native identity. Borrowed
    // groups avoid an additional input-sized alias map or group collection.
    let queue = Mutex::new(0usize);
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let deadline = progress.deadline();
    std::thread::scope(|workers| -> Result<(), Failure> {
        let (sender, receiver) = mpsc::sync_channel::<ImportBatch>(QUEUE_SLOTS);
        for _ in 0..INIT_WORKERS {
            let sender = sender.clone();
            let queue = &queue;
            let jobs = &jobs;
            workers.spawn(move || {
                let mut producer = BatchProducer::new(&sender);
                loop {
                    let (start, end) = {
                        let mut next = queue.lock().expect("import queue poisoned");
                        if *next == jobs.len() {
                            break;
                        }
                        let start = *next;
                        let first = &jobs[start];
                        let end = start
                            + jobs[start..].partition_point(|job| {
                                job.dev == first.dev && job.ino == first.ino
                            });
                        *next = end;
                        (start, end)
                    };
                    let job = &jobs[start];
                    let index = job.index;
                    let result = construct_file(job, policy, &capacities, &mut producer, deadline)
                        .and_then(|root| {
                            for alias in &jobs[start..end] {
                                if !matches_metadata(alias, &fs::symlink_metadata(&alias.path)?) {
                                    return Err(Failure::InvalidInput);
                                }
                            }
                            Ok(root)
                        });
                    if producer.done(index, result).is_err() {
                        return;
                    }
                }
                let _ = producer.flush();
            });
        }
        drop(sender);
        let mut finished = 0;
        while finished < remaining {
            progress.tick()?;
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
                    Event::Done(index, result) => {
                        entries.get_mut(index).ok_or(Failure::InvalidInput)?.content =
                            Some(result?);
                        finished += 1;
                    }
                }
            }
        }
        Ok(())
    })
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
    let opened = file.metadata()?;
    if !matches_metadata(job, &opened) {
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
    let after = file.metadata()?;
    if built.logical_len != job.len || !matches_metadata(job, &after) {
        return Err(Failure::InvalidInput);
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline);
    }
    Ok(built.root)
}

fn entry(
    parent: usize,
    name: Vec<u8>,
    kind: RecordKind,
    metadata: &Metadata,
    content: Option<ObjectId>,
) -> Result<PreparedEntry, Failure> {
    // The existing portable symlink grammar fixes mode0777, including on hosts
    // whose lstat exposes umask-dependent link bits. Regular/directory bits stay
    // exact; read_link still compares the native link's original metadata.
    let mode = if kind == RecordKind::Symlink {
        0o777
    } else {
        metadata.mode() & 0o7777
    };
    let mask = if kind == RecordKind::Directory {
        0o1777
    } else {
        0o777
    };
    if mode & !mask != 0 || !(0..1_000_000_000).contains(&metadata.mtime_nsec()) {
        return Err(Failure::InvalidInput);
    }
    Ok(PreparedEntry {
        alias: None,
        parent,
        name,
        kind,
        mode,
        mtime_seconds: metadata.mtime(),
        mtime_nanoseconds: metadata.mtime_nsec() as u32,
        content,
        target: None,
    })
}
fn matches_metadata(job: &Job, metadata: &Metadata) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.dev() == job.dev
        && metadata.ino() == job.ino
        && metadata.len() == job.len
        && metadata.mode() == job.mode
        && metadata.mtime() == job.mtime
        && metadata.mtime_nsec() == job.mtime_nsec
        && metadata.ctime() == job.ctime
        && metadata.ctime_nsec() == job.ctime_nsec
}
fn prepare_aliases(entries: &mut [PreparedEntry], jobs: &mut [Job]) -> Result<usize, Failure> {
    jobs.sort_unstable_by_key(|job| (job.dev, job.ino, job.index));
    let mut first: Option<&Job> = None;
    let mut groups = 0;
    for job in jobs.iter() {
        match first {
            Some(original) if original.dev == job.dev && original.ino == job.ino => {
                if original.len != job.len
                    || original.mode != job.mode
                    || original.mtime != job.mtime
                    || original.mtime_nsec != job.mtime_nsec
                    || original.ctime != job.ctime
                    || original.ctime_nsec != job.ctime_nsec
                {
                    return Err(Failure::InvalidInput);
                }
                entries[job.index].alias = Some(original.index);
            }
            _ => {
                first = Some(job);
                groups += 1;
            }
        }
    }
    Ok(groups)
}

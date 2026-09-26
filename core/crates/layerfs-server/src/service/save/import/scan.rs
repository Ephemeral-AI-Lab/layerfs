//! One operator-bound directory import into a single C2 save.
use super::{
    batch::{BatchProducer, Event, ImportBatch, QUEUE_SLOTS},
    namespace::{ImportProgress, PreparedEntry},
};
use crate::service::error::{content, storage};
use layerfs_bridge::contract::{Code, Failure, MAX_FILE};
use layerfs_content::{construct_stream, FinalizedConsumer, ObjectId, PathName};
use layerfs_history::RecordKind;
use layerfs_storage::{SaveHandoff, Store};
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
}

pub(crate) fn scan_and_save(
    source: &Path,
    store: &Store,
    progress: &mut ImportProgress<'_>,
    timer: &TimingScope<'_, Active>,
) -> Result<Vec<PreparedEntry>, Failure> {
    let root_metadata = fs::symlink_metadata(source)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(Code::InvalidInput.into());
    }
    let mut save = store
        .begin_save(timer.child("history.import_begin_save"))
        .map_err(storage)?;
    let mut handoff = SaveHandoff::new(&mut save);
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
            progress.tick()?;
            let mut children = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
            children.sort_by(|a, b| a.file_name().as_bytes().cmp(b.file_name().as_bytes()));
            for child in children {
                progress.tick()?;
                let path = child.path();
                let name = child.file_name().as_bytes().to_vec();
                PathName::from_bytes(&name).map_err(content)?;
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink() {
                    return Err(Code::Unsupported.into());
                }
                let kind = if metadata.is_dir() {
                    RecordKind::Directory
                } else if metadata.is_file() {
                    RecordKind::RegularFile
                } else {
                    return Err(Code::Unsupported.into());
                };
                if kind == RecordKind::RegularFile {
                    if metadata.len() > MAX_FILE {
                        return Err(Code::Capacity.into());
                    }
                    jobs.push(Job {
                        index: entries.len(),
                        path: path.clone(),
                        dev: metadata.dev(),
                        ino: metadata.ino(),
                        len: metadata.len(),
                        mtime: metadata.mtime(),
                        mtime_nsec: metadata.mtime_nsec(),
                    });
                }
                let index = entries.len();
                entries.push(entry(parent, name, kind, &metadata, None)?);
                if kind == RecordKind::Directory {
                    pending.push_back((path, index));
                }
            }
        }
        Ok((entries, jobs))
    });
    let scanned = scanned.and_then(|(mut entries, jobs)| {
        timer
            .child("history.import_files")
            .run(|_| save_files(&mut entries, jobs, store, &mut handoff, progress))?;
        Ok(entries)
    });
    let retained = handoff.take_failure();
    drop(handoff);
    let scanned = match retained {
        Some(error) => Err(storage(error)),
        None => scanned,
    };
    let scanned = scanned.and_then(|entries| {
        progress.tick()?;
        Ok(entries)
    });
    match scanned {
        Ok(entries) => {
            save.finish(timer.child("history.import_finish_save"))
                .map_err(storage)?;
            Ok(entries)
        }
        Err(mut error) => {
            if !error.unknown {
                if let Err(cleanup) = save.abort(timer.child("history.import_abort_save")) {
                    let cleanup = storage(cleanup);
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
            }
            Err(error)
        }
    }
}

fn save_files(
    entries: &mut [PreparedEntry],
    jobs: Vec<Job>,
    store: &Store,
    handoff: &mut SaveHandoff<'_>,
    progress: &mut ImportProgress<'_>,
) -> Result<(), Failure> {
    let remaining = jobs.len();
    let queue = Mutex::new(VecDeque::from(jobs));
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let deadline = progress.deadline();
    std::thread::scope(|workers| -> Result<(), Failure> {
        let (sender, receiver) = mpsc::sync_channel::<ImportBatch>(QUEUE_SLOTS);
        for _ in 0..INIT_WORKERS {
            let sender = sender.clone();
            let queue = &queue;
            workers.spawn(move || {
                let mut producer = BatchProducer::new(&sender);
                loop {
                    let job = queue.lock().expect("import queue poisoned").pop_front();
                    let Some(job) = job else { break };
                    let index = job.index;
                    let result = construct_file(job, policy, &capacities, &mut producer, deadline);
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
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Code::Io.into()),
            };
            for event in batch.events {
                match event {
                    Event::Object(object) => handoff.accept(object).map_err(content)?,
                    Event::Done(index, result) => {
                        entries.get_mut(index).ok_or(Code::InvalidInput)?.content = Some(result?);
                        finished += 1;
                    }
                }
            }
        }
        Ok(())
    })
}

fn construct_file(
    job: Job,
    policy: layerfs_content::ConstructionPolicy,
    capacities: &layerfs_content::ConstructionCapacities,
    producer: &mut BatchProducer<'_>,
    deadline: Instant,
) -> Result<ObjectId, Failure> {
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let mut file = File::open(&job.path)?;
    let opened = file.metadata()?;
    if opened.dev() != job.dev || opened.ino() != job.ino {
        return Err(Code::InvalidInput.into());
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
    if built.logical_len != job.len
        || after.len() != job.len
        || after.mtime() != job.mtime
        || after.mtime_nsec() != job.mtime_nsec
    {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
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
    let mode = metadata.mode() & 0o7777;
    let mask = if kind == RecordKind::Directory {
        0o1777
    } else {
        0o777
    };
    if mode & !mask != 0 || !(0..1_000_000_000).contains(&metadata.mtime_nsec()) {
        return Err(Code::InvalidInput.into());
    }
    Ok(PreparedEntry {
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

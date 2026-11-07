//! `project.export(project, destination, { branch, on_progress })`: the
//! call, its three Store routes (`native` SQLite and PostgreSQL in this
//! process, SQLite behind a machine's worker) and the worker's side.
use crate::lease;
use crate::open_args::{OpenArgs, PgSelection};
use crate::place::{self, Executor, Worker};
use crate::project::discovery::home_of;
use crate::project::frame::ExportFrame;
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::resolve::{self, Target};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::unhex;
use layerfs_history::HistoryName;
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::{refused, Outcome, View};
use layerfs_store_service::history::MAIN;
use std::path::Path;

use super::destination::check_destination;
use super::walk::{pin_snapshot, stream_snapshot, Counts};
use super::writer::LocalWriter;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One `project.export(project, destination, { branch, on_progress })`.
pub struct ProjectExport {
    /// The project, `<store>/<project>`.
    pub project: ProjectRef,
    /// The local folder it is exported to: absolute, absent or empty.
    pub destination: String,
    /// The Branch whose snapshot is exported; `main` when absent.
    pub branch: Option<String>,
}

/// Summary of the pinned snapshot and exported tree counts.
struct SnapshotSummary {
    snapshot: String,
    node: String,
    root: String,
    counts: Counts,
}

/// Runs `project.export(project, destination, { branch, on_progress })`.
pub fn run(home: &Path, context: &mut Events<'_>, request: &ProjectExport) -> Outcome {
    let home = home.to_path_buf();
    let project = request.project.to_string();
    let project = project.as_str();
    let destination_text = request.destination.as_str();
    let branch = request.branch.as_deref().unwrap_or(MAIN);
    validate_branch_arg(branch)?;
    let destination = check_destination(&home, destination_text)?;
    let target = resolve::target(&home, project)?;
    let project_record = ProjectRecord::read(&home, &target.store, &target.project)?
        .ok_or_else(|| refused("NoSuchProject", format!("no project {project}")))?;
    target.record.unblocked()?;
    let (lease, record) = lease::claim(&home, &target.store)?;
    let store_ref = record.store_ref()?.clone();
    let summary = match &store_ref {
        StoreRef::Postgres { .. } => export_postgres(
            context,
            &home,
            &target,
            &record,
            branch,
            &destination,
            lease,
        )?,
        StoreRef::Sqlite { .. } => match place::plan(&home, &store_ref)? {
            Executor::Local => export_local_sqlite(
                context,
                &home,
                &target,
                &record,
                branch,
                &destination,
                lease,
            )?,
            Executor::Worker(worker) => export_on_worker(
                context,
                worker,
                &target,
                &record,
                branch,
                &destination,
                lease,
            )?,
        },
    };
    Ok(build_result(
        project_record,
        &record,
        branch,
        destination_text,
        summary,
    ))
}

/// Rejects invalid branch names and 66-char hex Commit/Layer identities before
/// any effect.
fn validate_branch_arg(branch: &str) -> Result<()> {
    if branch == MAIN {
        return Ok(());
    }
    if branch.len() == 66 && unhex(branch).is_some() {
        return Err(refused(
            "InvalidInput",
            format!("`{branch}` is a Commit or Layer identity; `branch` must name a Branch"),
        ));
    }
    HistoryName::new(branch).map_err(|_| {
        refused(
            "InvalidInput",
            format!("`{branch}` is not a valid Branch name"),
        )
    })?;
    Ok(())
}

fn export_local_sqlite(
    context: &mut Events<'_>,
    home: &Path,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let store_file = paths::store_file(home, &target.store);
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(target.store.clone()),
        project: Some(target.project.clone()),
        store_path: Some(store_file.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    export_in_process(context, &open, &target.project, branch, dest)
}

fn export_postgres(
    context: &mut Events<'_>,
    home: &Path,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let url = crate::record::url(record)?;
    let key = record.postgresql_key(home, &target.store);
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(target.store.clone()),
        project: Some(target.project.clone()),
        store_path: Some(key.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: Some(PgSelection {
            url: Some(url),
            ..PgSelection::default()
        }),
    };
    export_in_process(context, &open, &target.project, branch, dest)
}

fn export_in_process(
    context: &mut Events<'_>,
    open_args: &OpenArgs,
    project: &str,
    branch: &str,
    dest: &Path,
) -> Result<SnapshotSummary> {
    crate::service::with_service(open_args, |service| {
        let pinned = pin_snapshot(service, project, branch)?;
        let mut writer = LocalWriter::start(context, dest)?;
        let counts = stream_snapshot(&pinned, &mut |part| writer.write_part(context, part))?;
        let counts = writer.finish(context, dest, counts)?;
        Ok(SnapshotSummary {
            snapshot: pinned.label,
            node: pinned.node,
            root: pinned.root,
            counts,
        })
    })
}

fn export_on_worker(
    context: &mut Events<'_>,
    mut worker: Worker,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let request = ExportFrame {
        store: layerfs_home::names::StoreName::parse(&target.store)?,
        project: target.project.clone(),
        branch: Some(branch.to_owned()),
        store_record: Some(record.to_record().clone()),
    }
    .encode();
    let mut writer: Option<LocalWriter> = None;
    // a local failure refuses the event: the connection closes and the
    // worker stops streaming at its next part
    let value = worker.call_until("project.export", request, &mut |event| {
        let part = match event.get("part") {
            Some(SdkValue::Bytes(part)) => Some(part),
            _ if event.get_text("phase") == Some("prepare") => None,
            _ => return Ok(()),
        };
        let w = match writer.as_mut() {
            Some(w) => w,
            None => writer.insert(LocalWriter::start(context, dest)?),
        };
        match part {
            Some(part) => w.write_part(context, part),
            None => Ok(()),
        }
    })?;
    let expected = worker_counts(&value)?;
    let w = match writer {
        Some(w) => w,
        None => LocalWriter::start(context, dest)?,
    };
    let counts = w.finish(context, dest, expected)?;
    Ok(SnapshotSummary {
        snapshot: value.get_text("snapshot").unwrap_or("").to_owned(),
        node: value.get_text("node").unwrap_or("").to_owned(),
        root: value.get_text("root").unwrap_or("").to_owned(),
        counts,
    })
}

/// Worker side of `project.export`: opens the SQLite Store under `root`, pins
/// the Branch snapshot once and streams bounded `tree::Record` parts back to
/// the caller through `events`.
pub fn in_worker(
    root: &Path,
    request: &ExportFrame,
    events: &mut dyn FnMut(SdkValue) -> Result<()>,
) -> Outcome {
    let root = home_of(Some(root))?;
    let store = request.store.as_str();
    let project = request.project.as_str();
    let branch = request.branch.as_deref().unwrap_or(MAIN);
    validate_branch_arg(branch)?;
    let _lease = lease::hold_locked(&root, store)?;
    let open = OpenArgs {
        home: Some(root.clone()),
        store: Some(store.to_owned()),
        project: Some(project.to_owned()),
        store_path: Some(paths::store_file(&root, store).display().to_string()),
        store_record: request.store_record.clone(),
        postgresql: None,
    };
    crate::service::with_service(&open, |service| {
        let pinned = pin_snapshot(service, project, branch)?;
        events(SdkValue::map([("phase", SdkValue::text("prepare"))]))?;
        let counts = stream_snapshot(&pinned, &mut |part| {
            events(SdkValue::map([("part", SdkValue::Bytes(part.to_vec()))]))
        })?;
        Ok(SdkValue::map([
            ("snapshot", SdkValue::text(&pinned.label)),
            ("node", SdkValue::text(&pinned.node)),
            ("root", SdkValue::text(&pinned.root)),
            ("files", SdkValue::Int(counts.files as i64)),
            ("directories", SdkValue::Int(counts.directories as i64)),
            ("symlinks", SdkValue::Int(counts.symlinks as i64)),
            ("bytes", SdkValue::Int(counts.bytes as i64)),
        ]))
    })
}

fn build_result(
    project: ProjectRecord,
    record: &StoreRecord,
    branch: &str,
    destination: &str,
    summary: SnapshotSummary,
) -> SdkValue {
    let mut out = project.to_sdk(View::Full, record);
    for (k, v) in [
        ("branch", SdkValue::text(branch)),
        ("snapshot", SdkValue::Text(summary.snapshot)),
        ("node", SdkValue::Text(summary.node)),
        ("root", SdkValue::Text(summary.root)),
        ("destination", SdkValue::text(destination)),
        ("files", SdkValue::Int(summary.counts.files as i64)),
        (
            "directories",
            SdkValue::Int(summary.counts.directories as i64),
        ),
        ("symlinks", SdkValue::Int(summary.counts.symlinks as i64)),
        ("bytes", SdkValue::Int(summary.counts.bytes as i64)),
    ] {
        out = layerfs_home::home::with_field(out, k, v);
    }
    out
}

fn worker_counts(value: &SdkValue) -> Result<Counts> {
    let take = |field: &str| -> Result<u64> {
        value
            .get_int(field)
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| {
                refused(
                    "Failed",
                    format!("worker project.export reply is missing valid `{field}` count"),
                )
            })
    };
    Ok(Counts {
        files: take("files")?,
        directories: take("directories")?,
        symlinks: take("symlinks")?,
        bytes: take("bytes")?,
    })
}

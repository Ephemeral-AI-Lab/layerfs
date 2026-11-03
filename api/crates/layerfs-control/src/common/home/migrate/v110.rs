//! The one conversion of a 1.1.0 home into the 2.0 layout (#533 step 4),
//! under the home lock, run by the first 2.0 call on that home.
//!
//! Records move into `store/`, `backup/` and `record.json` once:
//! `.stores/<s>/` (a Store's record and its projects), `<home>/<p>/` (a
//! project with a Store of its own becomes Store `<p>` holding project
//! `<p>`), `stores.json` and `stores.v1.json` (folded into each Store's
//! record), the volume `.runtime/projects/<p>.json` names, each sqlite
//! Store's file where this host can read it (`volume.rs`),
//! `remote-state/<s>` (the Store's backup link), `remotes` (one
//! `backup/<bucket>/record.json` per remote), `.runtime/` (Init failures to
//! `log/`) and `traces/` (to `trace/`). Sandbox folders stay inside their
//! project's folder until step 12; machine folders come in step 6.
//!
//! Every old file that was read is moved, unchanged, under `log/1.1.0/`;
//! nothing is deleted. An unknown file is left where it is and reported.
//! `layerfs.json` is written last, so a conversion stopped part way is
//! continued by the next call, and a second run changes nothing.
use crate::common::home::layout::OLD;
use crate::common::home::paths;
use crate::common::home::records::{now_ms, private_dir, read_record, with_field, write_record};
use crate::common::outcome::{failed, refused};
use crate::store::record::{kind_of, StoreKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup::{link as state, Provider, Remotes};
use std::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Names the 2.0 layout owns at the top of the home.
const OWNED: [&str; 7] = [
    "layerfs.json",
    "layerfs.lock",
    "machine",
    "store",
    "backup",
    "trace",
    "log",
];

/// What one conversion did.
#[derive(Default)]
struct Report {
    stores: Vec<String>,
    placed: Vec<String>,
    projects: Vec<String>,
    buckets: Vec<String>,
    moved: Vec<String>,
    unknown: Vec<String>,
    notes: Vec<String>,
}

impl Report {
    fn value(self) -> SdkValue {
        let list =
            |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
        SdkValue::map([
            ("from", SdkValue::text("1.1.0")),
            ("at", SdkValue::Int(now_ms())),
            ("stores", list(self.stores)),
            ("placed", list(self.placed)),
            ("projects", list(self.projects)),
            ("buckets", list(self.buckets)),
            ("moved_to_log", list(self.moved)),
            ("unknown", list(self.unknown)),
            ("notes", list(self.notes)),
        ])
    }
}

pub(super) fn convert(home: &Path, volumes: &Path) -> Result<SdkValue> {
    let mut report = Report::default();
    let old = paths::log_dir(home).join("1.1.0");
    // 1. Stores with records of their own, and their projects
    for store in paths::names(&home.join(".stores")).map_err(failed)? {
        shared_store(home, &store, &mut report)?;
    }
    // 2. projects with a Store of their own: <home>/<p>/project.record
    for name in paths::names(home).map_err(failed)? {
        if OWNED.contains(&name.as_str()) || OLD.contains(&name.as_str()) {
            continue;
        }
        let dir = home.join(&name);
        if dir.join("project.record").is_file() {
            own_store(home, &name, &dir, &mut report)?;
        }
    }
    // 3. stores.json (and its version 1 copy) folded into the records
    for file in ["stores.json", "stores.v1.json"] {
        fold_registry(home, &home.join(file), &mut report)?;
    }
    // 3b. each sqlite Store's file, from its 1.1.0 Docker volume
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let Some(record) = read_record(&paths::store_record(home, &store))? else {
            continue;
        };
        if kind_of(&record)? == StoreKind::Postgresql {
            continue;
        }
        let was_native = record.get_text("machine") == Some("native");
        let (record, note) = super::volume::place(home, volumes, &store, record)?;
        match note {
            Some(note) => report.notes.push(note),
            None if !was_native && record.get_text("machine") == Some("native") => {
                report.placed.push(store)
            }
            None => {}
        }
    }
    // 4. each Store's last transfer becomes its backup link
    for store in names_of_files(&home.join("remote-state"))? {
        if let Some(last) = state::load(&home.join("remote-state").join(&store))
            .map_err(|e| refused("Failed", e))?
        {
            link(home, &store, &last, &mut report)?;
        }
    }
    // 5. remotes become buckets
    buckets(home, &mut report)?;
    // 6. Init failures to log/, traces/ to trace/
    let failures = home.join(".runtime").join("init-failures.jsonl");
    if failures.is_file() {
        private_dir(&paths::log_dir(home))?;
        fs::copy(&failures, paths::log_dir(home).join("init-failures.jsonl")).map_err(failed)?;
    }
    if home.join("traces").is_dir() && !paths::trace_dir(home).exists() {
        fs::rename(home.join("traces"), paths::trace_dir(home)).map_err(failed)?;
        report.moved.push("traces -> trace".to_owned());
    }
    // 7. every old file read moves, unchanged, under log/1.1.0/
    for name in OLD.into_iter().filter(|name| *name != "traces") {
        let from = home.join(name);
        if from.exists() {
            private_dir(&old)?;
            fs::rename(&from, old.join(name)).map_err(failed)?;
            report.moved.push(name.to_owned());
        }
    }
    // 8. anything else is left where it is and reported
    for entry in fs::read_dir(home).map_err(failed)? {
        let name = entry
            .map_err(failed)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !OWNED.contains(&name.as_str()) && !name.starts_with(".layerfs.json") {
            report.unknown.push(name);
        }
    }
    Ok(report.value())
}

fn names_of_files(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    match fs::read_dir(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(failed(e)),
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(failed)?;
                if entry.file_type().map_err(failed)?.is_file() {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

/// A 1.1.0 Store record (`.stores/<s>/store.record`) and its projects.
fn shared_store(home: &Path, store: &str, report: &mut Report) -> Result<()> {
    let dir = home.join(".stores").join(store);
    let old = dir.join("store.record");
    if old.is_file() && read_record(&paths::store_record(home, store))?.is_none() {
        let value = crate::sandbox::files::load(&old)?;
        let kind = kind_of(&value)?;
        let mut record = SdkValue::map([
            ("name", SdkValue::text(store)),
            ("kind", SdkValue::text(kind.as_str())),
            ("binding", value.get("binding").cloned().unwrap_or_default()),
            ("cursor", value.get("cursor").cloned().unwrap_or_default()),
        ]);
        if kind == StoreKind::Postgresql {
            for (from, to) in [
                ("schema", "schema"),
                ("synchronousCommit", "synchronous_commit"),
            ] {
                record = with_field(record, to, value.get(from).cloned().unwrap_or_default());
            }
            record = with_field(record, "url", SdkValue::Null);
            report.notes.push(format!(
                "Store {store} is PostgreSQL: 1.1.0 kept no URL; run store connect again"
            ));
        } else {
            record = sqlite_defaults(record);
        }
        write_record(&paths::store_record(home, store), &record)?;
        report.stores.push(store.to_owned());
    }
    for project in paths::names(&dir.join("projects")).map_err(failed)? {
        move_project(
            home,
            store,
            &project,
            &dir.join("projects").join(&project),
            report,
        )?;
    }
    Ok(())
}

/// A sqlite Store record's 2.0 fields beside its identity. Its machine is
/// not known until `stores.json` or `layerfs import-env` names it.
fn sqlite_defaults(record: SdkValue) -> SdkValue {
    let record = with_field(record, "profile", SdkValue::text("disposable"));
    let record = with_field(record, "machine", SdkValue::Null);
    with_field(record, "backups", SdkValue::Map(Vec::new()))
}

/// A project with its own 1.1.0 Store: Store `<p>` holding project `<p>`.
fn own_store(home: &Path, name: &str, dir: &Path, report: &mut Report) -> Result<()> {
    if read_record(&paths::store_record(home, name))?.is_none() {
        let value = crate::sandbox::files::load(&dir.join("project.record"))?;
        let manifest = match value.get("manifest") {
            Some(SdkValue::Bytes(bytes)) => {
                layerfs_bridge::provision::StoreManifest::decode(bytes).map_err(failed)?
            }
            _ => {
                return Err(refused(
                    "Failed",
                    format!("{name}/project.record has no manifest"),
                ))
            }
        };
        let mut record = sqlite_defaults(SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("sqlite")),
            ("binding", SdkValue::Bytes(manifest.binding.clone())),
            ("cursor", SdkValue::Bytes(manifest.cursor_key.to_vec())),
            ("locator", SdkValue::text(&manifest.locator)),
        ]));
        // the 1.1.0 runtime's record of the project names its Store's volume
        let runtime = home.join(".runtime/projects").join(format!("{name}.json"));
        if let Ok(text) = fs::read_to_string(&runtime) {
            let value = crate::common::json::parse(&text)
                .map_err(|e| refused("Failed", format!("{}: {}", runtime.display(), e.0)))?;
            if let Some(volume) = value.get_text("volume") {
                record = with_field(record, "volume", SdkValue::text(volume));
            }
        }
        write_record(&paths::store_record(home, name), &record)?;
        report.stores.push(name.to_owned());
    }
    move_project(home, name, name, dir, report)
}

/// Moves a project folder to `store/<s>/project/<p>/` and writes its
/// `record.json` from its `project.record`.
fn move_project(
    home: &Path,
    store: &str,
    project: &str,
    from: &Path,
    report: &mut Report,
) -> Result<()> {
    let to = paths::project_dir(home, store, project);
    if to.exists() {
        report.notes.push(format!(
            "{store}/{project} exists; {} left in place",
            from.display()
        ));
        return Ok(());
    }
    private_dir(to.parent().expect("project folder has a parent"))?;
    fs::rename(from, &to).map_err(failed)?;
    let old = to.join("project.record");
    if old.is_file() {
        let value = crate::sandbox::files::load(&old)?;
        let record = SdkValue::map([
            ("name", SdkValue::text(project)),
            ("store", SdkValue::text(store)),
            (
                "manifest",
                value.get("manifest").cloned().unwrap_or_default(),
            ),
            (
                "installed",
                value.get("installed").cloned().unwrap_or_default(),
            ),
        ]);
        write_record(&to.join("record.json"), &record)?;
        fs::remove_file(&old).map_err(failed)?;
    }
    report.projects.push(format!("{store}/{project}"));
    Ok(())
}

/// Folds the runtime's `stores.json` into each Store's record: when it was
/// made, its volume (so its machine is `docker`), its PostgreSQL connection
/// name and its link.
fn fold_registry(home: &Path, file: &Path, report: &mut Report) -> Result<()> {
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(());
    };
    let doc = crate::common::json::parse(&text)
        .map_err(|e| refused("Failed", format!("{}: {}", file.display(), e.0)))?;
    let Some(SdkValue::List(stores)) = doc.get("stores") else {
        return Ok(());
    };
    for entry in stores {
        let Some(name) = entry.get_text("name") else {
            continue;
        };
        let path = paths::store_record(home, name);
        let Some(mut record) = read_record(&path)? else {
            report.notes.push(format!(
                "stores.json names Store {name}, which has no record"
            ));
            continue;
        };
        if let Some(created) = entry.get("created") {
            record = with_field(record, "created", created.clone());
        }
        if let Some(volume) = entry.get_text("volume") {
            record = with_field(record, "machine", SdkValue::text("docker"));
            record = with_field(record, "volume", SdkValue::text(volume));
        }
        if let Some(connection) = entry.get_text("connection") {
            record = with_field(record, "connection_1_1_0", SdkValue::text(connection));
        }
        write_record(&path, &record)?;
    }
    Ok(())
}

/// A Store's 1.1.0 link (its last transfer) as its backup link in the
/// bucket of the same name (O23: one link per Store per bucket).
fn link(home: &Path, store: &str, last: &state::Transfer, report: &mut Report) -> Result<()> {
    let path = paths::store_record(home, store);
    let Some(record) = read_record(&path)? else {
        report.notes.push(format!(
            "remote-state names Store {store}, which has no record"
        ));
        return Ok(());
    };
    let op = match last.direction {
        state::Direction::Push => "create",
        state::Direction::Pull => "restore",
    };
    let linked = SdkValue::map([
        ("file", SdkValue::text(&last.file)),
        (
            "heads",
            last.heads
                .map(|h| SdkValue::Bytes(h.to_vec()))
                .unwrap_or_default(),
        ),
        (
            "last",
            SdkValue::map([
                ("op", SdkValue::text(op)),
                ("at", SdkValue::Int(last.at as i64)),
                ("bytes", SdkValue::Int(last.bytes as i64)),
                ("sha256", SdkValue::Bytes(last.sha256.to_vec())),
            ]),
        ),
    ]);
    let backups = match record.get("backups") {
        Some(SdkValue::Map(fields)) => SdkValue::Map(fields.clone()),
        _ => SdkValue::Map(Vec::new()),
    };
    let record = with_field(record, "backups", with_field(backups, &last.remote, linked));
    write_record(&path, &record)
}

/// Each `remotes` entry as `backup/<bucket>/record.json`. `local_folder` is
/// not in 2.0; it is reported and kept only in `log/1.1.0/remotes`.
fn buckets(home: &Path, report: &mut Report) -> Result<()> {
    let remotes = Remotes::load(&Remotes::home_path(home)).map_err(|e| refused("Failed", e))?;
    for name in remotes.names().map(str::to_owned).collect::<Vec<_>>() {
        let record = match remotes.get(&name).expect("listed") {
            Provider::Minio(s) => SdkValue::map([
                ("name", SdkValue::text(&name)),
                ("provider", SdkValue::text("minio")),
                ("endpoint", SdkValue::text(&s.endpoint)),
                ("bucket", SdkValue::text(&s.bucket)),
                ("prefix", SdkValue::text(&s.prefix)),
                ("region", SdkValue::text(&s.region)),
                ("key", SdkValue::text(s.credentials.text())),
                ("part_size", SdkValue::Int(s.part_size as i64)),
            ]),
            Provider::R2(s) => SdkValue::map([
                ("name", SdkValue::text(&name)),
                ("provider", SdkValue::text("r2")),
                ("account", SdkValue::text(&s.account)),
                ("jurisdiction", SdkValue::text(&s.jurisdiction)),
                ("bucket", SdkValue::text(&s.bucket)),
                ("prefix", SdkValue::text(&s.prefix)),
                ("region", SdkValue::text("auto")),
                ("key", SdkValue::text(s.credentials.text())),
                ("part_size", SdkValue::Int(s.part_size as i64)),
            ]),
            Provider::LocalFolder(path) => {
                report.notes.push(format!(
                    "remote {name} is a local_folder ({}), which 2.0 does not have; kept in log/1.1.0/remotes",
                    path.display()
                ));
                continue;
            }
        };
        let path: PathBuf = paths::bucket_record(home, &name);
        if read_record(&path)?.is_none() {
            write_record(&path, &record)?;
            report.buckets.push(name);
        }
    }
    Ok(())
}

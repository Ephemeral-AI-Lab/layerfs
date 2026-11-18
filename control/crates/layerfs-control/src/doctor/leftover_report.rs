//! The leftovers in `layerfs doctor`'s report (layerfs-project#180): the
//! home's own walk on `native`, and each running machine's, asked of its
//! worker's `doctor.sandbox` operation through `place::on_machine` as the
//! Store checks ask for a partial copy. A machine that is stopped, held by
//! an operation or does not answer is passed over: nothing is started.
//!
//! Each entry is a warning on its machine that also carries `leftover: {
//! machine, kind, path, bytes, age, id, fixable }` (and `not_before` for a
//! folder copy), for a page that shows it with its size, age and fix
//! command. The report's `leftovers` lists each machine walked:
//! `{ machine, complete, image_locks, unmount_markers }`.
//!
//! A folder copy is `fixable` only when a line of the home's
//! `log/uncertain.jsonl` names it with its Init's deadline
//! ([`named_copies`]); any other copy may belong to an Init that is still
//! running, and is reported only.
use super::leftovers::{self, Kind, BOUNDS};
use super::records::Findings;
use super::scan::Scanned;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use layerfs_home::op::{Op, Standing};
use layerfs_store::place::{self, Executor};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How long after its Init's recorded deadline a folder copy is removable.
pub const INIT_GRACE_MS: i64 = 60 * 60 * 1000;

/// The most of `log/uncertain.jsonl` that is read, from its end: the log
/// is never rotated, and a copy named before that is reported only.
const UNCERTAIN_TAIL: u64 = 4 << 20;

/// A folder copy a line of `log/uncertain.jsonl` names.
#[derive(Clone, Debug)]
pub struct NamedCopy {
    /// The machine whose worker root holds it.
    pub machine: String,
    /// Its id: `incoming/init-<copy>`.
    pub copy: String,
    /// The Store and project of its Init.
    pub store: String,
    /// The project's name.
    pub project: String,
    /// The Init's deadline, in ms since the epoch; `None` when the line
    /// records none.
    pub deadline: Option<i64>,
}

/// The folder copies `home`'s `log/uncertain.jsonl` names (a lost
/// `project.init` or `project.stage` answer, `layerfs-store`
/// `project/send.rs` `log_left`), the newest line of each copy last.
pub fn named_copies(home: &Path) -> Vec<NamedCopy> {
    let mut text = Vec::new();
    let read =
        std::fs::File::open(paths::log_dir(home).join("uncertain.jsonl")).and_then(|mut file| {
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(length.saturating_sub(UNCERTAIN_TAIL)))?;
            file.take(UNCERTAIN_TAIL).read_to_end(&mut text)
        });
    if read.is_err() {
        return Vec::new();
    }
    let field = |line: &SdkValue, name: &str| line.get_text(name).map(str::to_owned);
    (String::from_utf8_lossy(&text).lines())
        .filter_map(|line| layerfs_home::json::parse(line).ok())
        .filter(|line| line.get_text("call") == Some("project.init"))
        .filter_map(|line| {
            Some(NamedCopy {
                machine: field(&line, "machine")?,
                copy: field(&line, "copy")?,
                store: field(&line, "store")?,
                project: field(&line, "project")?,
                deadline: line.get_int("deadline"),
            })
        })
        .collect()
}

/// The line naming copy `copy` on `machine`, the newest one.
pub fn named<'a>(copies: &'a [NamedCopy], machine: &str, copy: &str) -> Option<&'a NamedCopy> {
    (copies.iter()).rfind(|named| named.machine == machine && named.copy == copy)
}

/// The Sandboxes `scanned` records on `machine`.
fn known(scanned: &Scanned, machine: &str) -> Vec<String> {
    (scanned.sandboxes.iter())
        .filter(|sandbox| sandbox.machine() == machine)
        .map(|sandbox| sandbox.name().to_owned())
        .collect()
}

/// Adds every side's leftovers to `findings`.
pub(super) fn check(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    let copies = named_copies(home);
    if layerfs_machine::runs_here(home, "native").unwrap_or(false) {
        let walked = leftovers::walk(home, &[], BOUNDS);
        report("native", &walked, (scanned, &copies), findings);
    }
    for machine in paths::names(&home.join("machine")).unwrap_or_default() {
        // only a machine whose worker is recorded is asked: nothing starts
        if machine == "native" || !paths::worker_record(home, &machine).exists() {
            continue;
        }
        let Ok(Executor::Worker(mut worker)) = place::on_machine(home, &machine) else {
            continue;
        };
        let names = known(scanned, &machine);
        let args = SdkValue::map([
            ("leftovers", SdkValue::Bool(true)),
            (
                "known",
                SdkValue::List(names.into_iter().map(SdkValue::Text).collect()),
            ),
            (
                "timeout_ms",
                SdkValue::Int(BOUNDS.within.as_millis() as i64),
            ),
        ]);
        if let Ok(Ok(walked)) = worker.attempt("doctor.sandbox", args, &mut |_| {}) {
            report(&machine, &walked, (scanned, &copies), findings);
        }
    }
}

/// One side's walk as findings on `machine` and its row of `leftovers`.
fn report(
    machine: &str,
    walked: &SdkValue,
    (scanned, copies): (&Scanned, &[NamedCopy]),
    findings: &mut Findings,
) {
    let entity = ("machine", machine);
    let complete = walked.get_bool("complete");
    findings.walked(SdkValue::map([
        ("machine", SdkValue::text(machine)),
        ("complete", SdkValue::Bool(complete)),
        (
            "image_locks",
            SdkValue::Int(walked.get_int("image_locks").unwrap_or(0)),
        ),
        (
            "unmount_markers",
            SdkValue::Int(walked.get_int("unmount_markers").unwrap_or(0)),
        ),
    ]));
    if !complete {
        findings.warning(
            entity,
            format!(
                "the walk for leftovers on machine {machine} stopped at its bound ({} entries or {} s); there may be more than it lists",
                BOUNDS.entries,
                BOUNDS.within.as_secs()
            ),
            "nothing to do: remove what is listed, then run `layerfs doctor` again".to_owned(),
        );
    }
    let Some(SdkValue::List(rows)) = walked.get("leftovers") else {
        return;
    };
    for row in rows {
        // a worker's answer is read as data: a kind or name the walk does
        // not give is dropped
        let (Some(kind), Some(name), Some(path)) = (
            row.get_text("kind").and_then(Kind::named),
            row.get_text("name"),
            row.get_text("path"),
        ) else {
            continue;
        };
        if kind.report_only() || kind.path(Path::new("/"), name).is_none() {
            continue;
        }
        if kind == Kind::BackupStaging && backup_runs(scanned, name) {
            continue;
        }
        let id = leftovers::id(machine, kind, name);
        let (bytes, age) = (row.get_int("bytes"), row.get_int("age").unwrap_or(0));
        let size = bytes.map_or("size not measured".to_owned(), |b| format!("{b} bytes"));
        let named = (kind == Kind::InitCopy)
            .then(|| named(copies, machine, name))
            .flatten();
        let not_before = named
            .and_then(|named| named.deadline)
            .map(|deadline| deadline.saturating_add(INIT_GRACE_MS));
        let fixable = kind != Kind::InitCopy || not_before.is_some();
        let (what, fix) = describe(kind, &id, name, named);
        let mut detail = vec![
            ("machine", SdkValue::text(machine)),
            ("kind", SdkValue::text(kind.as_str())),
            ("path", SdkValue::text(path)),
            ("bytes", bytes.map_or(SdkValue::Null, SdkValue::Int)),
            ("age", SdkValue::Int(age)),
            ("id", SdkValue::text(&id)),
            ("fixable", SdkValue::Bool(fixable)),
        ];
        detail.extend(not_before.map(|at| ("not_before", SdkValue::Int(at))));
        findings.leftover(
            entity,
            format!("{path} on machine {machine} is {what} ({size}, last changed {age} s ago)"),
            fix,
            SdkValue::Map(
                (detail.into_iter())
                    .map(|(field, value)| (field.to_owned(), value))
                    .collect(),
            ),
        );
    }
}

/// Whether a live process holds an operation on the Store a staged
/// snapshot `<store>/<file>` belongs to: its own `backup.create`, for one.
fn backup_runs(scanned: &Scanned, name: &str) -> bool {
    let store = name.split_once('/').map_or(name, |(store, _)| store);
    (scanned.stores.iter())
        .find(|record| record.name() == store)
        .and_then(|record| record.op())
        .is_some_and(|op: Op| op.standing() == Standing::Running)
}

/// What an entry is, and what removes it.
fn describe(kind: Kind, id: &str, name: &str, named: Option<&NamedCopy>) -> (String, String) {
    let fix = format!("`layerfs doctor fix {id}`");
    match kind {
        Kind::RunFolder => (
            "a Sandbox run folder no Sandbox record names".to_owned(),
            format!("{fix} removes it when it has its `owner` marker, no daemon of it runs and nothing is mounted under it"),
        ),
        Kind::EmptyRoot => (
            "the empty private root of a Sandbox whose run folder is gone".to_owned(),
            format!("{fix} removes it"),
        ),
        Kind::BackupStaging => (
            format!(
                "a snapshot a `backup create` of Store {} staged and did not remove",
                name.split_once('/').map_or(name, |(store, _)| store)
            ),
            format!("{fix} removes it unless a `backup create` of that Store is running"),
        ),
        Kind::ImageTmp => (
            "a temporary directory an interrupted image unpack left".to_owned(),
            format!("{fix} removes it unless an unpack of that image is running; that image's next unpack removes it too"),
        ),
        Kind::ImagePart => (
            "a partly received Sandbox image, which the next load of that image resumes".to_owned(),
            format!("{fix} removes it once it has not changed for an hour"),
        ),
        Kind::InitCopy => match named {
            Some(NamedCopy { store, project, deadline: Some(_), .. }) => (
                format!("the folder copy of a `project init` of {store}/{project} whose answer was lost"),
                format!("{fix} removes it an hour after that Init's deadline"),
            ),
            _ => (
                "a folder copy of a `project init`; no line of log/uncertain.jsonl names it with a deadline, so it may belong to an Init that is still running".to_owned(),
                "report-only: `layerfs doctor fix` refuses it; once no `project init` into that machine runs, remove the folder there by hand".to_owned(),
            ),
        },
        Kind::ImageLock | Kind::UnmountMarker => (String::new(), String::new()),
    }
}

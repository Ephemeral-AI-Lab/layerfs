//! The host worker's file writes: each one `fs.*` method on a host-machine
//! sqlite Store, or on a PostgreSQL Store whose record holds a login
//! revision (#819 P-B writes), its content or changes streamed in bounded
//! frames.
//!
//! The CLI defers a `file write` or `file apply` input it would read whole
//! (stdin or `--from`) when the Store looks eligible ([`likely`]). At the
//! call boundary the route is chosen before anything is sent: an eligible
//! call streams the deferred input to the worker, any other call reads it
//! whole and runs directly as before. Single-change writes (`edit`, `mkdir`,
//! `rm`, `cp`, `mv`) go as one request frame when it fits, else directly.
//!
//! A PostgreSQL write's request carries the Store's saved password and the
//! record's login revision, as a PostgreSQL read's does
//! ([`crate::host_postgresql`]): every copy of the request the CLI makes is
//! overwritten once sent or dropped, and the worker dials with it and reads
//! no secret itself. A PostgreSQL Store whose record holds no login
//! revision, or whose host machine is stopped, keeps its writes direct.
use crate::resolve::Target;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use std::cell::RefCell;
use std::io::Read;
use std::path::Path;

/// One native worker call runs one Store write method.
pub const OP: &str = "store.write";

const METHODS: [&str; 7] = [
    "fs.write", "fs.edit", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "fs.apply",
];

/// The fields a write request's header may carry; content and changes
/// never travel in it.
const FIELDS: [&str; 12] = [
    "branch",
    "path",
    "from",
    "to",
    "mode",
    "message",
    "expected",
    "oldText",
    "newText",
    "replaceAll",
    "recursive",
    "streamed",
];

/// What a deferred CLI input holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// `file write`'s content.
    Content,
    /// `file apply`'s JSON list of changes.
    Changes,
}

/// A CLI input not yet read: stdin or an opened `--from` file.
pub struct Input {
    /// What it holds.
    pub kind: InputKind,
    /// Where it is read from, once.
    pub source: Box<dyn Read + Send>,
    /// The same open file when the input is a regular file, sharing its
    /// offset: a `file apply` list is checked against the worker's budget
    /// through it before anything is sent, and the offset restored. Any
    /// other input gets one when the route is chosen (its spool file).
    pub file: Option<std::fs::File>,
}

thread_local! {
    static INPUT: RefCell<Option<Input>> = const { RefCell::new(None) };
}

/// Runs `call` with the CLI's deferred `input`, taken by the first write
/// whose route is chosen; an input nothing took is dropped unread.
pub fn with_input<T>(input: Option<Input>, call: impl FnOnce() -> T) -> T {
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            INPUT.with(|held| held.borrow_mut().take());
        }
    }
    INPUT.with(|held| *held.borrow_mut() = input);
    let _clear = Clear;
    call()
}

/// Whether this thread's call has a deferred input not yet taken.
pub fn deferred() -> bool {
    INPUT.with(|held| held.borrow().is_some())
}

/// Reads a deferred input whole, as the direct route takes it: content as
/// text when UTF-8 else bytes; changes as the Store's list.
fn materialize(input: Input, args: &mut Vec<(String, SdkValue)>) -> Result<(), SdkRefusal> {
    let mut bytes = Vec::new();
    let Input {
        kind,
        mut source,
        file,
    } = input;
    // a spool file is freed once read, before the call runs
    drop(file);
    let read = source.read_to_end(&mut bytes).map_err(input_error);
    drop(source);
    read?;
    match kind {
        InputKind::Content => {
            args.retain(|(name, _)| name != "content" && name != "bytes");
            args.push(match String::from_utf8(bytes) {
                Ok(text) => ("content".to_owned(), SdkValue::Text(text)),
                Err(raw) => ("bytes".to_owned(), SdkValue::Bytes(raw.into_bytes())),
            });
        }
        InputKind::Changes => {
            let text = String::from_utf8(bytes).map_err(|_| not_json("it is not UTF-8"))?;
            let list = match layerfs_home::json::parse(&text) {
                Ok(SdkValue::List(list)) => list,
                Ok(_) => {
                    return Err(refused(
                        "InvalidInput",
                        "`changes` must be a list of changes",
                    ))
                }
                Err(error) => return Err(not_json(&error.0)),
            };
            drop(text);
            let changes = list
                .into_iter()
                .map(store_change)
                .collect::<Result<Vec<_>, _>>()?;
            args.retain(|(name, _)| name != "changes");
            args.push(("changes".to_owned(), SdkValue::List(changes)));
        }
    }
    Ok(())
}

fn input_error(error: std::io::Error) -> SdkRefusal {
    refused("InvalidInput", format!("cannot read the input: {error}"))
}

fn not_json(why: &str) -> SdkRefusal {
    refused("InvalidInput", format!("the input is not JSON: {why}"))
}

/// One public change record as the Store takes it.
pub(crate) fn store_change(change: SdkValue) -> Result<SdkValue, SdkRefusal> {
    match change {
        SdkValue::Map(fields) => crate::calls::file::owned_change(fields),
        _ => Err(refused("InvalidInput", "each change is a record")),
    }
}

/// Whether the CLI should defer reading the input of a write to `project`
/// (`<store>/<project>`): its Store is a sqlite Store on the in-process host
/// machine whose worker endpoint fits. A wrong guess is still correct: the
/// call reads the input whole and runs directly.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn likely(home: &Path, project: &str) -> bool {
    let Ok(target) = crate::resolve::target(home, project) else {
        return false;
    };
    (host_sqlite(&target) || host_postgresql(home, &target))
        && layerfs_machine::worker::HostPaths::new(home).is_ok_and(|paths| paths.fits())
}

/// No host worker on this platform.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn likely(_home: &Path, _project: &str) -> bool {
    false
}

fn host_sqlite(target: &Target) -> bool {
    matches!(
        &target.store_ref,
        crate::record::StoreRef::Sqlite { machine, .. }
            if layerfs_machine::in_process() == Some(machine.as_str())
    ) && target.record.unblocked().is_ok()
}

/// Whether `target` is a PostgreSQL Store whose writes go to the host
/// worker: its record holds a login revision (a record connected before
/// login revisions stays direct), it is unblocked, and the host machine is
/// not stopped (§8.1).
fn host_postgresql(home: &Path, target: &Target) -> bool {
    matches!(&target.store_ref, crate::record::StoreRef::Postgres { .. })
        && layerfs_machine::in_process().is_some()
        && target.record.login_revision().is_some()
        && target.record.unblocked().is_ok()
        && layerfs_machine::record::MachineRecord::admit_host(
            home,
            layerfs_home::names::HOST_MACHINE,
        )
        .is_ok()
}

/// The route of one CLI write, chosen before anything is sent: the host
/// worker's outcome, or None for the direct route with `args` complete.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn routed(
    home: &Path,
    target: &Target,
    method: &str,
    args: &mut Vec<(String, SdkValue)>,
) -> Option<Outcome> {
    use layerfs_machine::worker::HostPaths;

    let mut input = take(method);
    let direct = |args: &mut Vec<(String, SdkValue)>, input: Option<Input>| {
        input.and_then(|input| materialize(input, args).err().map(Err))
    };
    let postgresql = crate::host_read::cli() && host_postgresql(home, target);
    if !crate::host_read::cli() || !METHODS.contains(&method) {
        return direct(args, input);
    }
    if !postgresql && !host_sqlite(target) {
        return direct(args, input);
    }
    let body = match (method, input.as_ref().map(|input| input.kind)) {
        ("fs.write", Some(InputKind::Content)) => "content",
        ("fs.apply", Some(InputKind::Changes)) => "changes",
        // content already read (a scoped apply): the direct route takes it
        ("fs.write" | "fs.apply", _) => return direct(args, input),
        (_, None) => "none",
        (_, Some(_)) => return direct(args, input),
    };
    let mut fields: Vec<_> = args
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "content" | "bytes" | "changes"))
        .cloned()
        .collect();
    if body == "content" {
        fields.push(("streamed".to_owned(), SdkValue::Bool(true)));
    }
    let mut header = vec![
        ("method".to_owned(), SdkValue::text(method)),
        ("store".to_owned(), SdkValue::text(&target.store)),
        ("project".to_owned(), SdkValue::text(&target.project)),
        ("body".to_owned(), SdkValue::text(body)),
        ("fields".to_owned(), SdkValue::Map(fields)),
    ];
    if let (true, Some(revision)) = (postgresql, target.record.login_revision()) {
        let saved = match crate::postgresql::login::saved(&target.record) {
            Ok(saved) => saved,
            Err(refusal) => return Some(Err(refusal)),
        };
        header.push(("login_revision".to_owned(), SdkValue::text(revision)));
        header.push(("password".to_owned(), SdkValue::text(saved.password())));
    }
    // a header that goes direct takes its copy of the password with it
    let header = layerfs_home::wipe::Guard::new(SdkValue::Map(header), postgresql);
    let paths = match HostPaths::new(home) {
        Ok(paths) => paths,
        Err(error) => return Some(Err(error)),
    };
    if !paths.request_fits(OP, header.get()) {
        return direct(args, input);
    }
    // An input that is not a regular file (a pipe, FIFO or terminal) is
    // first copied into a private unnamed file: the worker then reads it at
    // disk speed, never at its producer's, and a list is checked before
    // anything is sent. When that copy cannot be made the call runs direct,
    // and so does a write whose worker is not ready, before any copy (#833).
    if input.as_ref().is_some_and(|held| held.file.is_none()) {
        match paths.direct_unless_published(OP) {
            Ok(true) => return direct(args, input),
            Ok(false) => {}
            Err(refusal) => return Some(Err(refusal)),
        }
    }
    if let Some(held) = input.as_mut().filter(|held| held.file.is_none()) {
        let source = std::mem::replace(&mut held.source, Box::new(std::io::empty()));
        let (source, file) = crate::host_stream::spool(home, source);
        held.source = source;
        held.file = file;
        if held.file.is_none() {
            return direct(args, input);
        }
    }
    if body == "changes" {
        // a change past the worker's budget runs direct: a route, not a
        // refusal (a list that cannot be checked runs direct too)
        let file = input.as_ref().and_then(|held| held.file.as_ref());
        let fits = match file.map(std::fs::File::try_clone) {
            Some(Ok(mut file)) => crate::host_stream::fits_worker(&mut file),
            _ => false,
        };
        if !fits {
            return direct(args, input);
        }
    }
    let header = header.take();
    // None from the worker: none was ready, nothing was sent, and the
    // write goes direct with its input unread (#833)
    match input {
        None => paths.call_validated(OP, header, valid_answer),
        Some(Input { kind, source, file }) => {
            // the source alone reads the input from here: the spool's
            // other descriptor goes now, and the file with the source, once
            // its last byte is read, never after the answer
            drop(file);
            let mut source = Some(source);
            let mut send = |sender: &mut layerfs_machine::worker::StreamSender<'_>| {
                crate::host_stream::send(kind, &mut source, sender)
            };
            let routed = paths.call_streamed(OP, header, &mut send, valid_answer);
            match (routed, source) {
                (None, Some(source)) => {
                    let input = Input {
                        kind,
                        source,
                        file: None,
                    };
                    direct(args, Some(input))
                }
                (routed, _) => routed,
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn routed(
    _home: &Path,
    _target: &Target,
    method: &str,
    args: &mut Vec<(String, SdkValue)>,
) -> Option<Outcome> {
    let input = take(method);
    input.and_then(|input| materialize(input, args).err().map(Err))
}

/// The deferred input, for the one method that reads it.
fn take(method: &str) -> Option<Input> {
    matches!(method, "fs.write" | "fs.apply")
        .then(|| INPUT.with(|held| held.borrow_mut().take()))
        .flatten()
}

/// A write's success as the Store answers it.
fn valid_answer(value: &SdkValue) -> bool {
    matches!(value.get_text("kind"), Some("Committed" | "UpToDate"))
        && value.get_text("branch").is_some()
        && matches!(value.get("head"), Some(SdkValue::Null | SdkValue::Text(_)))
}

/// Checks one write request's header on the worker: its method, body,
/// field names, and that every field value is a scalar (null, a boolean, a
/// number or text), so a held header stays within its decoded bound. The
/// engine checks every value as for a direct call.
///
/// A PostgreSQL Store's write also carries its non-empty `login_revision`
/// and `password` (#819 P-B writes), and nothing else.
pub fn header(request: &SdkValue) -> Result<(&str, &str, &str, &str, &SdkValue), SdkRefusal> {
    let invalid = || refused("InvalidInput", "invalid native Store write request");
    let login = ["login_revision", "password"]
        .map(|name| request.get_text(name).is_some_and(|text| !text.is_empty()));
    match request {
        SdkValue::Map(envelope) if envelope.len() == 5 => {}
        SdkValue::Map(envelope) if envelope.len() == 7 && login == [true, true] => {}
        _ => return Err(invalid()),
    }
    let text = |name| request.get_text(name).ok_or_else(invalid);
    let (method, store, project, body) = (
        text("method")?,
        text("store")?,
        text("project")?,
        text("body")?,
    );
    let fields = request.get("fields").ok_or_else(invalid)?;
    let SdkValue::Map(listed) = fields else {
        return Err(invalid());
    };
    let expected = match method {
        "fs.write" => "content",
        "fs.apply" => "changes",
        _ if METHODS.contains(&method) => "none",
        _ => {
            return Err(refused(
                "Unsupported",
                "the host worker serves only file reads and file writes",
            ))
        }
    };
    let scalar = |value: &SdkValue| {
        matches!(
            value,
            SdkValue::Null | SdkValue::Bool(_) | SdkValue::Int(_) | SdkValue::Text(_)
        )
    };
    if body != expected
        || !listed
            .iter()
            .all(|(name, value)| FIELDS.contains(&name.as_str()) && scalar(value))
    {
        return Err(invalid());
    }
    Ok((method, store, project, body, fields))
}

/// Where the host worker keeps write outcomes its caller may not know:
/// `machine/<host>/retained/<request id>.json`, one file each, never
/// removed by the worker's idle exit, stop, replacement or crash. The user
/// removes a checked record with `rm`; at [`RETAINED_CAP`] the worker
/// removes the oldest `undelivered` ones, never an `uncertain` one.
pub fn retained_dir(home: &Path) -> std::path::PathBuf {
    layerfs_home::home::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE).join("retained")
}

/// The most kept outcomes. A new one at the cap first removes the oldest
/// `undelivered` records, which only inform; `uncertain` ones are never
/// removed, so when they alone fill the cap a new record is refused
/// `Capacity` (the worker logs that outcome and reports it instead).
pub const RETAINED_CAP: usize = 256;

/// Keeps one write's outcome before its answer is sent (`uncertain`: the
/// engine could not tell whether it committed, with the engine's evidence),
/// or after the answer of a Commit could not be delivered (`undelivered`),
/// keyed by the caller's request id. The record names the call, never its
/// content. It is written whole under a temporary name and linked into
/// place, so a reader never sees part of one and a record is never
/// replaced; like every Normal-profile write it is not synced.
pub fn keep(
    home: &Path,
    request_id: &str,
    kind: &str,
    request: &SdkValue,
    outcome: &Outcome,
) -> Result<(), SdkRefusal> {
    use std::io::Write;
    if request_id.is_empty()
        || request_id.len() > 128
        || !request_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(refused("InvalidInput", "an unusable request id"));
    }
    let dir = retained_dir(home);
    layerfs_home::home::private_dir(&dir)?;
    let path = dir.join(format!("{request_id}.json"));
    if path.exists() {
        return Ok(());
    }
    let kept = records(&dir)?;
    if kept.len() >= RETAINED_CAP {
        make_room(&dir, &kept)?;
    }
    let fields = request.get("fields");
    let field = |name: &str| SdkValue::opt_text(fields.and_then(|f| f.get_text(name)));
    let outcome = match outcome {
        Ok(value) => SdkValue::map([
            ("kind", value.get("kind").cloned().unwrap_or_default()),
            ("head", value.get("head").cloned().unwrap_or_default()),
        ]),
        Err(refusal) => SdkValue::map([
            ("code", SdkValue::text(&refusal.code)),
            ("message", SdkValue::text(&refusal.message)),
            ("detail", refusal.detail.clone()),
        ]),
    };
    let record = SdkValue::map([
        ("request_id", SdkValue::text(request_id)),
        ("kind", SdkValue::text(kind)),
        ("at", SdkValue::Int(layerfs_home::home::now_ms())),
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("method", SdkValue::opt_text(request.get_text("method"))),
        ("store", SdkValue::opt_text(request.get_text("store"))),
        ("project", SdkValue::opt_text(request.get_text("project"))),
        ("branch", field("branch")),
        ("path", field("path")),
        ("outcome", outcome),
    ]);
    let part = dir.join(format!(".{request_id}.part"));
    let written = layerfs_home::home::host_socket::private_file(&part, false)
        .and_then(|mut file| {
            file.set_len(0)?;
            file.write_all(layerfs_home::json::print(&record).as_bytes())
        })
        .and_then(|()| match std::fs::hard_link(&part, &path) {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            linked => linked,
        });
    let _ = std::fs::remove_file(&part);
    written.map_err(|error| {
        refused(
            "Failed",
            format!("cannot keep write outcome {}: {error}", path.display()),
        )
    })
}

/// Removes the oldest `undelivered` records so one more fits the cap.
fn make_room(dir: &Path, kept: &[std::path::PathBuf]) -> Result<(), SdkRefusal> {
    let mut undelivered: Vec<(i64, &std::path::PathBuf)> = kept
        .iter()
        .filter_map(|path| {
            let record = layerfs_home::json::parse(&std::fs::read_to_string(path).ok()?).ok()?;
            (record.get_text("kind") == Some("undelivered"))
                .then(|| (record.get_int("at").unwrap_or(0), path))
        })
        .collect();
    let excess = kept.len() + 1 - RETAINED_CAP;
    if undelivered.len() < excess {
        return Err(refused(
            "Capacity",
            format!(
                "{} write outcomes are kept in {}, {} of them uncertain; check their Branches, then remove those records with rm",
                kept.len(),
                dir.display(),
                kept.len() - undelivered.len()
            ),
        ));
    }
    undelivered.sort();
    for (_, path) in undelivered.into_iter().take(excess) {
        std::fs::remove_file(path)
            .map_err(|error| refused("Failed", format!("{}: {error}", path.display())))?;
    }
    Ok(())
}

fn records(dir: &Path) -> Result<Vec<std::path::PathBuf>, SdkRefusal> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(refused("Failed", format!("{}: {error}", dir.display()))),
    };
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    names.sort();
    Ok(names)
}

/// The kept write outcomes: how many, and the first `shown` by request id.
pub fn retained(home: &Path, shown: usize) -> Result<(usize, Vec<SdkValue>), SdkRefusal> {
    let names = records(&retained_dir(home))?;
    let records = names
        .iter()
        .take(shown)
        .map(|path| {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|text| layerfs_home::json::parse(&text).ok())
                .unwrap_or_else(|| {
                    SdkValue::map([("file", SdkValue::text(path.display().to_string()))])
                })
        })
        .collect();
    Ok((names.len(), records))
}

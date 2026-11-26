//! The host worker's file writes: each one `fs.*` method on a host-machine
//! sqlite Store, its content or changes streamed in bounded frames.
//!
//! The CLI defers a `file write` or `file apply` input it would read whole
//! (stdin or `--from`) when the Store looks eligible ([`likely`]). At the
//! call boundary the route is chosen before anything is sent: an eligible
//! call streams the deferred input to the worker, any other call reads it
//! whole and runs directly as before. Single-change writes (`edit`, `mkdir`,
//! `rm`, `cp`, `mv`) go as one request frame when it fits, else directly.
use crate::calls::Change;
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
    let mut source = input.source;
    source.read_to_end(&mut bytes).map_err(input_error)?;
    match input.kind {
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
            let changes = list
                .iter()
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
pub(crate) fn store_change(change: &SdkValue) -> Result<SdkValue, SdkRefusal> {
    match change {
        SdkValue::Map(fields) => crate::calls::file::store_change(&Change {
            fields: fields.clone(),
        }),
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
    host_sqlite(&target)
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

    let input = take(method);
    let direct = |args: &mut Vec<(String, SdkValue)>, input: Option<Input>| {
        input.and_then(|input| materialize(input, args).err().map(Err))
    };
    if !crate::host_read::cli() || !METHODS.contains(&method) || !host_sqlite(target) {
        return direct(args, input);
    }
    let body = match (method, &input) {
        (
            "fs.write",
            Some(Input {
                kind: InputKind::Content,
                ..
            }),
        ) => "content",
        (
            "fs.apply",
            Some(Input {
                kind: InputKind::Changes,
                ..
            }),
        ) => "changes",
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
    let header = SdkValue::map([
        ("method", SdkValue::text(method)),
        ("store", SdkValue::text(&target.store)),
        ("project", SdkValue::text(&target.project)),
        ("body", SdkValue::text(body)),
        ("fields", SdkValue::Map(fields)),
    ]);
    let paths = match HostPaths::new(home) {
        Ok(paths) => paths,
        Err(error) => return Some(Err(error)),
    };
    if !paths.request_fits(OP, &header) {
        return direct(args, input);
    }
    Some(match input {
        None => paths.call_validated(OP, header, valid_answer),
        Some(Input { kind, source }) => {
            let mut source = Some(source);
            let mut send = |sender: &mut layerfs_machine::worker::StreamSender<'_>| {
                crate::host_stream::send(kind, &mut source, sender)
            };
            paths.call_streamed(OP, header, &mut send, valid_answer)
        }
    })
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

/// Checks one write request's header on the worker: its method, body and
/// field names. The engine checks every value as for a direct call.
pub fn header(request: &SdkValue) -> Result<(&str, &str, &str, &str, &SdkValue), SdkRefusal> {
    let invalid = || refused("InvalidInput", "invalid native Store write request");
    match request {
        SdkValue::Map(envelope) if envelope.len() == 5 => {}
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
    if body != expected
        || !listed
            .iter()
            .all(|(name, _)| FIELDS.contains(&name.as_str()))
    {
        return Err(invalid());
    }
    Ok((method, store, project, body, fields))
}

/// Where the host worker keeps write outcomes its caller may not know:
/// `machine/<host>/retained/<request id>.json`, one file each, never
/// removed by the worker's idle exit, stop, replacement or crash.
pub fn retained_dir(home: &Path) -> std::path::PathBuf {
    layerfs_home::home::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE).join("retained")
}

/// Keeps one write's outcome before its answer is sent (`uncertain`: the
/// engine could not tell whether it committed), or after its answer could
/// not be delivered (`undelivered`), keyed by the caller's request id.
/// The record names the call, never its content.
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
    let path = dir.join(format!("{request_id}.json"));
    let written =
        layerfs_home::home::host_socket::private_file(&path, false).and_then(|mut file| {
            file.set_len(0)?;
            file.write_all(layerfs_home::json::print(&record).as_bytes())
        });
    written.map_err(|error| {
        refused(
            "Failed",
            format!("cannot keep write outcome {}: {error}", path.display()),
        )
    })
}

/// The kept write outcomes: how many, and the first `shown` by request id.
pub fn retained(home: &Path, shown: usize) -> Result<(usize, Vec<SdkValue>), SdkRefusal> {
    let dir = retained_dir(home);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, Vec::new())),
        Err(error) => return Err(refused("Failed", format!("{}: {error}", dir.display()))),
    };
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    names.sort();
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

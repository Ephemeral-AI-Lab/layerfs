//! The host worker's retained write outcomes: the on-disk store of write
//! outcomes its caller may not know (`machine/<host>/retained/`). The
//! worker writes them ([`keep`]); the doctor reads them ([`retained`]).
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// Where the host worker keeps write outcomes its caller may not know:
/// `machine/<host>/retained/<request id>.json`, one file each, never
/// removed by the worker's idle exit, stop, replacement or crash. The user
/// removes a checked record with `rm`; at [`RETAINED_CAP`] the worker
/// removes the oldest `undelivered` ones, never an `uncertain` one.
pub fn retained_dir(home: &Path) -> std::path::PathBuf {
    layerfs_home::disk::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE).join("retained")
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
    layerfs_home::disk::private_dir(&dir)?;
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
        ("at", SdkValue::Int(layerfs_home::disk::now_ms())),
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("method", SdkValue::opt_text(request.get_text("method"))),
        ("store", SdkValue::opt_text(request.get_text("store"))),
        ("project", SdkValue::opt_text(request.get_text("project"))),
        ("branch", field("branch")),
        ("path", field("path")),
        ("outcome", outcome),
    ]);
    let part = dir.join(format!(".{request_id}.part"));
    let written = layerfs_home::socket::private_file(&part, false)
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

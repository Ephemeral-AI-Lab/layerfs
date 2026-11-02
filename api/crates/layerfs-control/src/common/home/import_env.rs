//! `layerfs import-env` (#533 step 17, G14, O4): writes records from a 1.1.0
//! environment once, so the home then works with `LAYERFS_HOME` alone.
//!
//! It fills only what is not recorded yet: the built-in machines' settings
//! (`native`'s daemon and image archive, `docker`'s socket and image), a
//! bucket `remote` from `LAYERFS_REMOTE_*`, and the machine of each Store a
//! 1.1.0 home left unknown (from `LAYERFS_SANDBOX`/`LAYERFS_PLACEMENT`). It
//! never makes a VM or connects an ssh host (those have effects of their
//! own) and never writes a secret: for those it answers the command to run.
//! Every name it read and what became of it is in the answer and in
//! `log/import-env.jsonl`.
use super::{lock, log_line, now_ms, paths, read_record, with_field, write_record};
use crate::common::outcome::{failed, refused, Outcome};
use crate::store::record::{kind_of, StoreKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What one run did, by setting name.
#[derive(Default)]
struct Report {
    written: Vec<SdkValue>,
    kept: Vec<SdkValue>,
    commands: Vec<SdkValue>,
    ignored: Vec<SdkValue>,
}

impl Report {
    fn line(list: &mut Vec<SdkValue>, setting: &str, what: String) {
        list.push(SdkValue::map([
            ("setting", SdkValue::text(setting)),
            ("what", SdkValue::Text(what)),
        ]));
    }
}

/// The value of `name` in `env`, when set and not empty.
fn get<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter()
        .find(|(n, v)| n == name && !v.is_empty())
        .map(|(_, v)| v.as_str())
}

/// Sets `field` of the record at `path` to `value` unless it is recorded.
fn fill(
    path: &Path,
    field: &str,
    value: SdkValue,
    setting: &str,
    report: &mut Report,
) -> Result<()> {
    let record = read_record(path)?.unwrap_or_default();
    let what = format!("{} {field}", path.display());
    match record.get(field) {
        Some(old) if *old != SdkValue::Null => Report::line(&mut report.kept, setting, what),
        _ => {
            write_record(path, &with_field(record, field, value))?;
            Report::line(&mut report.written, setting, what);
        }
    }
    Ok(())
}

/// The machine a 1.1.0 `LAYERFS_SANDBOX` (or `LAYERFS_PLACEMENT`) value meant.
fn machine_of(sandbox: &str) -> Option<&'static str> {
    match sandbox {
        "docker" | "local" => Some("docker"),
        "native" | "mac" | "linux" | "namespaces" => Some("native"),
        _ => None,
    }
}

/// A built-in machine's record, made as the first call makes it.
fn built_in(home: &Path, name: &str) -> Result<std::path::PathBuf> {
    let path = paths::machine_record(home, name);
    if read_record(&path)?.is_none() {
        write_record(
            &path,
            &SdkValue::map([
                ("name", SdkValue::text(name)),
                ("platform", SdkValue::text(name)),
                ("created", SdkValue::Int(now_ms())),
            ]),
        )?;
    }
    Ok(path)
}

fn machines(home: &Path, env: &[(String, String)], report: &mut Report) -> Result<()> {
    let native = built_in(home, "native")?;
    let docker = built_in(home, "docker")?;
    if let Some(daemon) = get(env, "LAYERFS_DAEMON") {
        fill(
            &native,
            "daemon",
            SdkValue::text(daemon),
            "LAYERFS_DAEMON",
            report,
        )?;
    }
    if let Some(archive) = get(env, "LAYERFS_IMAGE_ARCHIVE") {
        let field = "image_archive";
        fill(
            &native,
            field,
            SdkValue::text(archive),
            "LAYERFS_IMAGE_ARCHIVE",
            report,
        )?;
    }
    if let Some(socket) = get(env, "LAYERFS_DOCKER") {
        let socket = socket.strip_prefix("unix://").unwrap_or(socket);
        fill(
            &docker,
            "docker",
            SdkValue::text(socket),
            "LAYERFS_DOCKER",
            report,
        )?;
    }
    if let Some(image) = get(env, "LAYERFS_IMAGE") {
        fill(
            &docker,
            "image",
            SdkValue::text(image),
            "LAYERFS_IMAGE",
            report,
        )?;
    }
    if let Some(host) = get(env, "LAYERFS_SSH_HOST") {
        let mut command = format!("layerfs machine connect ssh {host}");
        for (setting, option) in [
            ("LAYERFS_SSH_USER", "--user"),
            ("LAYERFS_SSH_PORT", "--port"),
            ("LAYERFS_SSH_IDENTITY", "--identity"),
            ("LAYERFS_SSH_KEY", "--identity"),
            ("LAYERFS_SSH_KNOWN_HOSTS", "--known-hosts"),
            ("LAYERFS_SSH_STORE", "--root"),
        ] {
            if let Some(value) = get(env, setting) {
                command.push_str(&format!(" {option} {value}"));
            }
        }
        Report::line(&mut report.commands, "LAYERFS_SSH_HOST", command);
    }
    for setting in ["LAYERFS_SANDBOX", "LAYERFS_PLACEMENT"] {
        if let Some(kind) = get(env, setting).and_then(|v| v.strip_prefix("vm:")) {
            let mut command = format!("layerfs machine create {kind} {kind}");
            for (size, option) in [
                ("LAYERFS_VM_CPUS", "--cpus"),
                ("LAYERFS_VM_MEMORY", "--memory"),
                ("LAYERFS_VM_STORE_SIZE", "--disk"),
                ("LAYERFS_VM_IMAGE", "--image"),
            ] {
                if let Some(value) = get(env, size) {
                    command.push_str(&format!(" {option} {value}"));
                }
            }
            Report::line(&mut report.commands, setting, command);
        }
    }
    Ok(())
}

/// The minio bucket `remote` from `LAYERFS_REMOTE_*`, its key a reference.
fn bucket(home: &Path, env: &[(String, String)], report: &mut Report) -> Result<()> {
    let (Some(endpoint), Some(bucket)) = (
        get(env, "LAYERFS_REMOTE_ENDPOINT"),
        get(env, "LAYERFS_REMOTE_BUCKET"),
    ) else {
        return Ok(());
    };
    let path = paths::bucket_record(home, "remote");
    if read_record(&path)?.is_some() {
        Report::line(
            &mut report.kept,
            "LAYERFS_REMOTE_ENDPOINT",
            path.display().to_string(),
        );
        return Ok(());
    }
    let part_size = match get(env, "LAYERFS_REMOTE_PART_SIZE") {
        Some(text) => text.parse::<i64>().map_err(|_| {
            refused(
                "InvalidInput",
                "LAYERFS_REMOTE_PART_SIZE is not a whole number",
            )
        })?,
        None => 8 << 20,
    };
    let key = get(env, "LAYERFS_REMOTE_CREDENTIALS").unwrap_or("env:LAYERFS_REMOTE");
    write_record(
        &path,
        &SdkValue::map([
            ("name", SdkValue::text("remote")),
            ("provider", SdkValue::text("minio")),
            ("endpoint", SdkValue::text(endpoint)),
            ("bucket", SdkValue::text(bucket)),
            (
                "prefix",
                SdkValue::text(get(env, "LAYERFS_REMOTE_PREFIX").unwrap_or("")),
            ),
            (
                "region",
                SdkValue::text(get(env, "LAYERFS_REMOTE_REGION").unwrap_or("us-east-1")),
            ),
            ("key", SdkValue::text(key)),
            ("part_size", SdkValue::Int(part_size)),
        ]),
    )?;
    Report::line(
        &mut report.written,
        "LAYERFS_REMOTE_ENDPOINT",
        format!("{} (key {key}, a reference)", path.display()),
    );
    Ok(())
}

/// Each Store whose machine 1.1.0 did not record gets the one its
/// environment placed Sandboxes on.
fn stores(home: &Path, env: &[(String, String)], report: &mut Report) -> Result<()> {
    let setting = ["LAYERFS_SANDBOX", "LAYERFS_PLACEMENT"]
        .into_iter()
        .find(|s| get(env, s).is_some());
    let machine = setting.and_then(|s| get(env, s)).and_then(machine_of);
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let path = paths::store_record(home, &store);
        let Some(record) = read_record(&path)? else {
            continue;
        };
        if kind_of(&record)? == StoreKind::Postgresql {
            continue;
        }
        match (record.get("machine"), machine) {
            (Some(SdkValue::Null) | None, Some(machine)) => {
                fill(
                    &path,
                    "machine",
                    SdkValue::text(machine),
                    setting.unwrap_or(""),
                    report,
                )?;
                if machine == "docker" {
                    if let Some(volume) = get(env, "LAYERFS_VOLUME") {
                        fill(
                            &path,
                            "volume",
                            SdkValue::text(volume),
                            "LAYERFS_VOLUME",
                            report,
                        )?;
                    }
                }
            }
            (Some(SdkValue::Null) | None, None) => Report::line(
                &mut report.commands,
                "LAYERFS_SANDBOX",
                format!(
                    "Store {store} has no machine recorded and the environment names none; \
                     set LAYERFS_SANDBOX=docker or native and run import-env again"
                ),
            ),
            _ => {}
        }
    }
    Ok(())
}

/// Settings 2.0 reads no more and records nowhere, said once.
const SECRET_OR_GONE: [(&str, &str); 4] = [
    (
        "LAYERFS_STORE_POSTGRESQL",
        "a secret: `layerfs store connect <name> <url> --password env:NAME`",
    ),
    ("LAYERFS_STORE_POLICY", "Init uses the default policy"),
    (
        "LAYERFS_LOCAL_DIR",
        "native Sandboxes live under machine/native/",
    ),
    ("LAYERFS_PROJECT", "every call names its project"),
];

/// Runs import-env on `home` with `env` (the process environment).
pub fn import_env(home: &Path, env: &[(String, String)]) -> Outcome {
    super::open(home)?;
    let _lock = lock::take(home)?;
    let mut report = Report::default();
    machines(home, env, &mut report)?;
    bucket(home, env, &mut report)?;
    stores(home, env, &mut report)?;
    for (setting, why) in SECRET_OR_GONE {
        if get(env, setting).is_some() {
            Report::line(&mut report.ignored, setting, why.to_owned());
        }
    }
    let answer = SdkValue::map([
        ("written", SdkValue::List(report.written)),
        ("kept", SdkValue::List(report.kept)),
        ("commands", SdkValue::List(report.commands)),
        ("ignored", SdkValue::List(report.ignored)),
    ]);
    log_line(
        home,
        "import-env.jsonl",
        &with_field(answer.clone(), "at", SdkValue::Int(now_ms())),
    )?;
    Ok(answer)
}

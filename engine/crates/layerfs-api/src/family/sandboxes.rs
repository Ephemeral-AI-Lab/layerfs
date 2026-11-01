//! The `sandbox` and `workspace` families (#533 step 12): Sandboxes named
//! across the home (unique under `layerfs.lock`), recorded in
//! `machine/<machine>/sandbox/<name>/record.json`, each running one project's
//! daemon on the machine that holds its Store. A Workspace is
//! `<sandbox>:<path>`. A Sandbox on `native` runs in this process's host
//! Sandbox code: host processes over loopback NFS on macOS, namespaces on
//! Linux. On any other machine the same call runs in that machine's worker,
//! which keeps the Sandbox's records under its own root; this home keeps
//! the name, the project and the machine.
use crate::calls::Context;
use crate::home::{self, paths, read_record, write_record};
use crate::host::outcome::{failed, refused, Outcome};
use crate::store::route;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Every Sandbox record in the home: `(machine, name, record)`.
fn all(home: &Path) -> Result<Vec<(String, String, SdkValue)>> {
    let mut found = Vec::new();
    for machine in paths::names(&home.join("machine")).map_err(failed)? {
        let dir = paths::machine_dir(home, &machine).join("sandbox");
        for name in paths::names(&dir).map_err(failed)? {
            if let Some(record) = read_record(&dir.join(&name).join("record.json"))? {
                found.push((machine.clone(), name, record));
            }
        }
    }
    Ok(found)
}

/// The Sandbox named `name`: its machine and record, or `NoSuchSandbox`.
fn sandbox_of(home: &Path, name: &str) -> Result<(String, SdkValue)> {
    crate::names::name("sandbox", name).map_err(|why| refused("InvalidInput", why))?;
    all(home)?
        .into_iter()
        .find(|(_, n, _)| n == name)
        .map(|(machine, _, record)| (machine, record))
        .ok_or_else(|| {
            refused(
                "NoSuchSandbox",
                format!(
                    "no Sandbox named {name}; `sandbox.create(project, name, machine)` starts one"
                ),
            )
        })
}

/// Where a `native` Sandbox's daemon executable is: the native machine
/// record's `daemon`, else the internal `LAYERFS_DAEMON`, else the bundle's.
fn daemon(home: &Path) -> Result<String> {
    let record = read_record(&paths::machine_record(home, "native"))?.unwrap_or_default();
    if let Some(path) = record.get_text("daemon") {
        return Ok(path.to_owned());
    }
    if let Some(path) = std::env::var_os("LAYERFS_DAEMON").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(path).display().to_string());
    }
    let cache = std::env::var_os("HOME")
        .map(|h| {
            PathBuf::from(h)
                .join(".cache/layerfs/bundle")
                .join(env!("CARGO_PKG_VERSION"))
        })
        .ok_or_else(|| refused("InvalidInput", "HOME is unset, so the bundle is unknown"))?;
    let path = cache
        .join(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
        .join("layerfs-daemon");
    if !path.is_file() {
        return Err(refused(
            "Unavailable",
            format!(
                "no LayerFS daemon at {}; install the bundle",
                path.display()
            ),
        ));
    }
    Ok(path.display().to_string())
}

/// The host Sandbox operations' arguments for project `<store>/<project>`
/// on `native`, plus `more`.
fn host_args(
    home: &Path,
    store: &str,
    project: &str,
    record: &SdkValue,
    more: Vec<(&str, SdkValue)>,
) -> Result<SdkValue> {
    let backend = if cfg!(target_os = "macos") {
        "native"
    } else {
        "namespaces"
    };
    // the local runtime's root: this user's, traversable (0755), as the
    // runtime requires
    let local = paths::machine_dir(home, "native").join("sandbox-root");
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(&local)
            .map_err(failed)?;
        std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).map_err(failed)?;
    }
    let mut fields = vec![
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(store)),
        ("project", SdkValue::text(project)),
        ("store_record", record.clone()),
        (
            "storePath",
            SdkValue::text(paths::store_file(home, store).display().to_string()),
        ),
        (
            "deploy",
            SdkValue::map([
                ("sandbox", SdkValue::text(backend)),
                ("localDir", SdkValue::text(local.display().to_string())),
                ("daemon", SdkValue::text(daemon(home)?)),
                (
                    "containerPrefix",
                    SdkValue::text(format!("lfs-{}-", &home::open(home)?.id[..8])),
                ),
            ]),
        ),
    ];
    fields.extend(more);
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// Runs one `sandbox` call.
pub(crate) fn sandbox(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    match call {
        "create" => create(context, args),
        "list" => {
            let only = args.get_text("project");
            let items: Vec<SdkValue> = all(&home)?
                .into_iter()
                .filter(|(_, _, r)| only.is_none() || r.get_text("project") == only)
                .map(|(_, _, record)| record)
                .collect();
            Ok(SdkValue::map([("items", SdkValue::List(items))]))
        }
        "get" => {
            let name = args.get_text("name").unwrap_or("");
            let (machine, record) = sandbox_of(&home, name)?;
            let workspaces = on_machine(
                context,
                &machine,
                &record,
                "sandbox.list_workspaces",
                vec![],
            )?;
            Ok(home::with_field(
                record,
                "workspaces",
                workspaces.get("workspaces").cloned().unwrap_or_default(),
            ))
        }
        "close" => {
            let name = args.get_text("name").unwrap_or("");
            let (machine, record) = sandbox_of(&home, name)?;
            let force = matches!(args.get("force"), Some(SdkValue::Bool(true)));
            let closed = on_machine(
                context,
                &machine,
                &record,
                "sandbox.close",
                vec![("force", SdkValue::Bool(force))],
            );
            match closed {
                Ok(value) => {
                    let _lock = home::lock::take(&home)?;
                    std::fs::remove_dir_all(paths::sandbox_dir(&home, &machine, name))
                        .map_err(failed)?;
                    Ok(home::with_field(value, "closed", SdkValue::text(name)))
                }
                // a Sandbox already gone loses its record too
                Err(refusal) if refusal.code == "NoSuchSandbox" => {
                    let _lock = home::lock::take(&home)?;
                    std::fs::remove_dir_all(paths::sandbox_dir(&home, &machine, name))
                        .map_err(failed)?;
                    Ok(SdkValue::map([
                        ("closed", SdkValue::text(name)),
                        ("was_gone", SdkValue::Bool(true)),
                    ]))
                }
                Err(refusal) => Err(refusal),
            }
        }
        _ => Err(refused("InvalidInput", format!("no call sandbox.{call}"))),
    }
}

/// `sandbox.create(project, name, machine, { image })`.
fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let project = args.get_text("project").unwrap_or("");
    let name = args.get_text("name").unwrap_or("");
    let machine = args.get_text("machine").unwrap_or("");
    crate::names::name("sandbox", name).map_err(|why| refused("InvalidInput", why))?;
    let target = route::target(&home, project)?;
    route::moving(&target.record, &target.store)?;
    if read_record(&paths::machine_record(&home, machine))?.is_none() {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    if target.record.get_text("kind") == Some("postgresql") {
        return Err(refused(
            "Unsupported",
            "a Sandbox of a PostgreSQL Store is not built in 2.0 yet",
        ));
    }
    if machine == "docker" {
        return Err(refused(
            "Unsupported",
            "Sandboxes on the docker machine are not built in 2.0 yet; use native or a VM machine",
        ));
    }
    let store_machine = route::machine(&target.record, &target.store)?;
    if store_machine != machine {
        return Err(refused(
            "Unsupported",
            format!("Store {} is on machine {store_machine}; a Sandbox of {project} runs there (`store move` moves the Store)", target.store),
        ));
    }
    // claim the name across the home
    {
        let _lock = home::lock::take(&home)?;
        if all(&home)?.iter().any(|(_, n, _)| n == name) {
            return Err(refused("Exists", format!("a Sandbox named {name} exists")));
        }
        write_record(
            &paths::sandbox_dir(&home, machine, name).join("record.json"),
            &SdkValue::map([
                ("name", SdkValue::text(name)),
                ("project", SdkValue::text(project)),
                ("machine", SdkValue::text(machine)),
                ("state", SdkValue::text("creating")),
                ("created", SdkValue::Int(home::now_ms())),
            ]),
        )?;
    }
    let mut more = vec![];
    if let Some(image) = args.get_text("image") {
        more.push(("image", SdkValue::text(image)));
    }
    let record = read_record(&paths::sandbox_dir(&home, machine, name).join("record.json"))?
        .unwrap_or_default();
    let started = on_machine(context, machine, &record, "sandbox.create", more);
    let _lock = home::lock::take(&home)?;
    let path = paths::sandbox_dir(&home, machine, name).join("record.json");
    match started {
        Ok(value) => {
            let mut record = record;
            for (field, value) in [
                ("id", value.get("sandbox").cloned().unwrap_or_default()),
                (
                    "platform",
                    value.get("platform").cloned().unwrap_or_default(),
                ),
                ("runtime", value.get("runtime").cloned().unwrap_or_default()),
                ("image", value.get("image").cloned().unwrap_or_default()),
                ("state", SdkValue::text("running")),
            ] {
                record = home::with_field(record, field, value);
            }
            write_record(&path, &record)?;
            Ok(record)
        }
        Err(refusal) if refusal.code == "Unknown" => {
            write_record(
                &path,
                &home::with_field(record, "state", SdkValue::text("unknown")),
            )?;
            Err(refusal)
        }
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::sandbox_dir(&home, machine, name));
            Err(refusal)
        }
    }
}

/// Runs host Sandbox operation `op` for Sandbox `record` on `machine`.
fn on_machine(
    context: &mut Context<'_>,
    machine: &str,
    record: &SdkValue,
    op: &str,
    more: Vec<(&str, SdkValue)>,
) -> Outcome {
    let home = context.home.clone();
    let project = record.get_text("project").unwrap_or("");
    let target = route::target(&home, project)?;
    let mut more = more;
    if let Some(id) = record.get_text("id") {
        more.push(("sandbox", SdkValue::text(id)));
    }
    match route::endpoint(&home, machine)? {
        None => {
            let args = host_args(&home, &target.store, &target.project, &target.record, more)?;
            crate::host::dispatch::call(op, args)
        }
        Some(endpoint) => {
            // the guest runs the same call on its own native machine, with
            // the records it needs sent along
            let project_record = read_record(&paths::project_record(
                &home,
                &target.store,
                &target.project,
            ))?
            .unwrap_or_default();
            let mut fields = vec![
                ("op", SdkValue::text(op)),
                ("store", SdkValue::text(&target.store)),
                ("project", SdkValue::text(&target.project)),
                ("store_record", target.record.clone()),
                ("project_record", project_record),
            ];
            fields.push((
                "more",
                SdkValue::Map(more.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
            ));
            let request =
                SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
            crate::worker::call(&endpoint, "sandbox.host", request, &mut |event| {
                context.event(event)
            })
        }
    }
}

/// Runs a Sandbox host operation in a worker for its caller (`sandbox.host`):
/// the Store's and project's records are written under the root first,
/// with the Store on this machine's own `native`.
pub(crate) fn in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let op = args
        .get_text("op")
        .ok_or_else(|| refused("InvalidInput", "operation name"))?;
    if !matches!(
        op,
        "sandbox.create"
            | "sandbox.close"
            | "sandbox.list_workspaces"
            | "workspace.mount"
            | "workspace.exec"
            | "workspace.commit"
            | "workspace.unmount"
            | "backend.daemon.workspace.status"
    ) {
        return Err(refused(
            "InvalidInput",
            format!("the worker runs no Sandbox operation {op}"),
        ));
    }
    let store = args.get_text("store").unwrap_or("");
    let project = args.get_text("project").unwrap_or("");
    crate::names::name("store", store).map_err(|why| refused("InvalidInput", why))?;
    crate::names::name("project", project).map_err(|why| refused("InvalidInput", why))?;
    let record = home::with_field(
        args.get("store_record").cloned().unwrap_or_default(),
        "machine",
        SdkValue::text("native"),
    );
    write_record(&paths::store_record(root, store), &record)?;
    let project_path = paths::project_record(root, store, project);
    if read_record(&project_path)?.is_none() {
        write_record(
            &project_path,
            &args.get("project_record").cloned().unwrap_or_default(),
        )?;
    }
    let more: Vec<(&str, SdkValue)> = match args.get("more") {
        Some(SdkValue::Map(fields)) => fields
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect(),
        _ => Vec::new(),
    };
    let host = host_args(root, store, project, &record, more)?;
    if op == "backend.daemon.workspace.status" {
        return crate::host::dispatch::backend(op.to_owned(), host);
    }
    crate::host::dispatch::call(op, host)
}

/// `(sandbox, path)` of a Workspace written `<sandbox>:<path>`.
fn ws(text: &str) -> Result<(&str, &str)> {
    text.split_once(':')
        .filter(|(s, p)| !s.is_empty() && p.starts_with('/'))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                format!("a Workspace is <sandbox>:<path>, not {text:?}"),
            )
        })
}

/// Runs one `workspace` call.
pub(crate) fn workspace(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    if call == "mount" {
        let name = args.get_text("sandbox").unwrap_or("");
        let (machine, record) = sandbox_of(&home, name)?;
        let branch = args.get("branch").cloned().unwrap_or_default();
        let mounted = on_machine(
            context,
            &machine,
            &record,
            "workspace.mount",
            vec![("branch", branch)],
        )?;
        let path = mounted.get_text("path").unwrap_or("").to_owned();
        return Ok(home::with_field(
            mounted,
            "ws",
            SdkValue::text(format!("{name}:{path}")),
        ));
    }
    let (name, path) = ws(args.get_text("ws").unwrap_or(""))?;
    let (machine, record) = sandbox_of(&home, name)?;
    let mut more = vec![("path", SdkValue::text(path))];
    let op = match call {
        "exec" => {
            more.push(("command", args.get("command").cloned().unwrap_or_default()));
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            "workspace.exec"
        }
        "commit" => {
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            "workspace.commit"
        }
        "unmount" => {
            for flag in ["discard", "force"] {
                if let Some(value) = args.get(flag) {
                    more.push((flag, value.clone()));
                }
            }
            "workspace.unmount"
        }
        "status" => "backend.daemon.workspace.status",
        _ => return Err(refused("InvalidInput", format!("no call workspace.{call}"))),
    };
    let out =
        if op == "backend.daemon.workspace.status" && route::endpoint(&home, &machine)?.is_none() {
            let target = route::target(&home, record.get_text("project").unwrap_or(""))?;
            if let Some(id) = record.get_text("id") {
                more.push(("sandbox", SdkValue::text(id)));
            }
            crate::host::dispatch::backend(
                op.to_owned(),
                host_args(&home, &target.store, &target.project, &target.record, more)?,
            )?
        } else {
            on_machine(context, &machine, &record, op, more)?
        };
    if call == "exec" {
        // the command's output as events too, for front ends that stream
        for stream in ["stdout", "stderr"] {
            match out.get(stream) {
                Some(SdkValue::Text(text)) if !text.is_empty() => {
                    context.output(stream, text.as_bytes())
                }
                Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => context.output(stream, bytes),
                _ => {}
            }
        }
    }
    Ok(out)
}

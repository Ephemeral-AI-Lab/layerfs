//! Named Sandboxes on the machine that runs them (#536 §6): a local daemon
//! process (native on macOS, namespaces on Linux, the docker machine's
//! worker container included, D-8) whose directory is `<prefix><name>`,
//! running on its project's Store and recorded in its folder
//! `<root>/sandbox/<name>/`. A Sandbox starts only when an agent asks for
//! one ([`create`], through `launch::local`); its records go when an agent
//! closes it ([`close`]), after a stopped runtime is deleted ([`reclaim`])
//! when it ended by itself. A record of the retired docker backend is
//! forgotten by its close. Nothing is retried on the caller's behalf.
use crate::daemon::values;
use crate::host::{Deploy, Host, Started};
use crate::record::DaemonRecord;
use crate::registry::{SandboxState, SANDBOXES};
use crate::OperationFailure;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use layerfs_bridge::provision::StoreManifest;
use layerfs_bridge::{DaemonStore, KernelRoute};
use layerfs_home::helpers::lock;
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Wait for the daemon's listener after Start.
pub const STARTUP: Duration = Duration::from_secs(30);
/// A failed create's report keeps its daemon's last log lines, up to this many bytes.
pub const LOG_LINES: u16 = 20;
pub const LOG_BYTES: usize = 2048;
/// How long an explicit close waits for the daemon's Workspace status before
/// it returns the exact admission refusal. The close's `op` intent must not
/// be held for the whole admission wait (#540).
const CLOSE_STATUS_WAIT: Duration = Duration::from_secs(2);

pub fn operation(failure: Box<OperationFailure>) -> SdkRefusal {
    values::refusal(&failure).unwrap_or_else(|| failed(failure))
}

/// Workspaces one Sandbox holds at once; a mount into a full one is refused
/// `Capacity`.
pub const WORKSPACES: u32 = 16;

/// Control connections a Sandbox's daemon admits: [`crate::daemon::pool::CONNECTIONS`]
/// for this process and the rest for others (#352 F-017).
pub const DAEMON_CONNECTIONS: u16 = 8;

/// A control connection parked in `receive()` this long is reclaimable when
/// admission is refused (#547 slice 4): long enough that an idle pool
/// connection is likely abandoned, short enough that recovery stays bounded.
pub const IDLE_TTL_MS: u32 = 60_000;

/// A decoded request still in flight this long is expired (#547 slice 5):
/// long enough that a 64 MiB Commit finishes normally, short enough that a
/// fence/Store/diagnostic wait cannot hold a pool slot forever.
pub const REQUEST_DEADLINE_MS: u32 = 300_000;

pub fn setup(
    private: [u8; 32],
    peer: [u8; 32],
    store: DaemonStore,
    route: KernelRoute,
) -> DaemonSetup {
    DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: private,
        control_peer: peer,
        route,
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: DAEMON_CONNECTIONS,
            handshake_ms: 2000,
            idle_ttl_ms: IDLE_TTL_MS,
            request_deadline_ms: REQUEST_DEADLINE_MS,
            // The daemon's shared immutable-object cache: a fixed bound, the
            // daemon's own default. 64 KiB held too few directory and inode
            // pages for lookups in a large directory to stay resident
            // (#356 F-032); 8 MiB was overrun by one `find`, vitest or
            // `ls -l` of 10,000 entries, so a later Workspace re-read it all.
            // 64 MiB by owner decision (#359 warm daemon, record 031).
            cache_bytes: 64 * 1024 * 1024,
            namespaces: WORKSPACES,
            // 4 MiB (1,024 pages), not 1 MiB: one 256 KiB write window's
            // transaction dirties a few hundred pages, and a 1 MiB pager
            // spilled it while it was still writing.
            pager_kib: 4096,
            serial_low_water: 256,
        },
        store,
    }
}

/// A Sandbox name prefix when the deployment sets none.
pub const PREFIX: &str = "layerfs-";

/// Sandbox `id`'s runtime name (its id is its name in the home), on every
/// backend: `<prefix><id>`. The prefix is the deployment's (`lfs-<home id>-`
/// from the home; `layerfs-` by default), so a Sandbox's directory says
/// whose it is.
pub fn sandbox_name(deploy: Option<&Deploy>, id: &str) -> Result<String, SdkRefusal> {
    let prefix = deploy
        .and_then(|d| d.container_prefix.as_deref())
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(PREFIX);
    layerfs_home::names::SandboxName::parse(id)?;
    Ok(format!("{prefix}{id}"))
}

/// The labels of Sandbox `id` of `project`: what it is, whose, and which. Saved in its record, so a reopen expects the same.
/// A project of a shared Store (#506) also carries `layerfs.store`.
pub fn labels(project: &str, store: Option<&str>, id: &str) -> Vec<(String, String)> {
    [
        ("layerfs.role", "sandbox"),
        ("layerfs.project", project),
        ("layerfs.sandbox", id),
    ]
    .into_iter()
    .chain(store.map(|store| ("layerfs.store", store)))
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect()
}

/// The Store a call's project selected for its Sandbox's daemon: the
/// project's installed Store file, or a PostgreSQL Store (#506), whose
/// Sandbox binds no Store folder.
pub enum OpenedStore {
    File(Box<(layerfs_store::project::discovery::Project, StoreManifest)>),
    Postgres(Box<DaemonStore>),
}

/// The installed Store file of the project a call names.
fn installed(host: &Host) -> Result<OpenedStore, SdkRefusal> {
    let name = &host.project;
    let identity = layerfs_store::identity::identity(&host.open_args())?;
    layerfs_store::identity::sqlite_only(&identity, "a Sandbox of a Store file")?;
    let project = layerfs_store::project::discovery::project(
        Some(&host.home),
        Some(&host.store),
        Some(&host.project),
    )?;
    let Some(installed) = project.installed.clone() else {
        return Err(refused(
            "NoStore",
            format!("project {name} has no installed Store: its Import did not finish, so import it again"),
        ));
    };
    Ok(OpenedStore::File(Box::new((project, installed))))
}

/// Starts the Sandbox the call names (`sandbox`) as its deployment says
/// (`deploy.sandbox`: native or namespaces), its daemon on `store`. Called
/// under the guard of its folder `directory`, so no other call on it runs
/// meanwhile.
pub fn start(
    (host, platform): (&Host, &str),
    directory: &Path,
    store: OpenedStore,
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let deploy = host
        .deploy()
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    // Inputs are read first, so a refused request leaves no creation intent.
    let backend = crate::runtime::backend(deploy)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        crate::launch::local::start((host, platform), directory, store, backend)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        drop((directory, store, backend, platform));
        Err(refused(
            "Unsupported",
            "a Sandbox runs on Linux (namespaces) or macOS (native)",
        ))
    }
}

/// `sandbox.create(sandbox, { image, platform })`: under its folder's guard,
/// starts the Sandbox the call names on the call's image (`deploy.image`)
/// and waits for its daemon: `{ sandbox, name, image, platform, runtime,
/// route, runtime_id }`. A folder that already records a started Sandbox
/// is `Exists`; one the retired docker backend recorded is forgotten first
/// (D-8). `deploy.sandbox` names its backend, `native` or
/// `namespaces`; a native one's `image` is `local`. `platform` (#529) is
/// checked against that backend and recorded.
/// The only call that starts a Sandbox. Failures are `Failed` or
/// `Unknown`, never retried.
pub fn create(host: &Host, named: Option<&str>) -> Outcome {
    // the platform is checked against the deployment before any effect (#529)
    let deploy = host
        .deploy()
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    let platform = crate::runtime::platform(named, deploy)?;
    // a PostgreSQL Store's URL is the call's
    let store = match layerfs_store::postgresql::open::sandbox_store(&host.open_args())? {
        Some(selected) => OpenedStore::Postgres(Box::new(selected)),
        None => installed(host)?,
    };
    let directory = crate::files::sandbox_dir(host)?;
    layerfs_home::home::private_dir(&directory)?;
    let made = (|| {
        let _guard = layerfs_store::project::discovery::guard(&directory)?;
        let recorded = crate::files::started(&directory)?;
        // a record of the retired docker backend is forgotten, as a close
        // forgets it, so its name is free again (D-8): nothing it named runs
        match recorded {
            Some(retired) if retired.retired() => {
                discard(&directory, &retired)?;
            }
            Some(_) => {
                return Err(refused(
                    "Exists",
                    format!(
                        "a Sandbox is recorded at {} on this machine; `sandbox.close` closes it",
                        directory.display()
                    ),
                ));
            }
            None => {}
        }
        start((host, &platform), &directory, store)
    })();
    let state = match made {
        Ok(state) => state,
        Err(refusal) => {
            // a worker's folder held only this call's turn; on `native` the
            // folder keeps the caller's record, which the caller removes
            if refusal.code == "Failed" || refusal.code == "InvalidInput" {
                let _ =
                    std::fs::remove_file(directory.join(layerfs_store::project::discovery::TURN));
                let _ = std::fs::remove_dir(&directory);
            }
            return Err(refusal);
        }
    };
    let runtime = state.runtime()?;
    let started = Started {
        sandbox: Some(state.id.clone()),
        name: Some(state.name.clone()),
        image: Some(crate::runtime::image(runtime).to_owned()),
        platform: Some(platform.to_owned()),
        runtime: Some(crate::runtime::kind(runtime).to_owned()),
        route: Some(crate::files::route(runtime).to_owned()),
        runtime_id: Some(runtime.identity().to_string()),
        control: None,
    };
    Ok(started.encode())
}

/// `sandbox.close(sandbox, { force })`: stops Sandbox `sandbox` now, under
/// its folder's guard. Refused `NoSuchSandbox` when nothing is recorded,
/// and, unless `force`, `MountsActive` while a Workspace is mounted in it.
/// Forced (O31), it stops the Sandbox through its backend anyway and
/// answers each Workspace it could not drain as `Unknown` (`unknown`: their
/// paths): their uncommitted changes and processes end with the Sandbox.
/// `held` names Workspaces the caller records (a Sandbox it reaches
/// directly, #535 G4), counted as mounted.
/// Stop, then delete, each attempted once: a failed stop is not followed by
/// a delete and keeps its cause; the folder goes after the delete. A
/// Sandbox whose runtime ended by itself is reclaimed ([`reclaim`]) and
/// answered `NoSuchSandbox`, as gone.
pub fn close(host: &Host, force: bool, held: &[String]) -> Outcome {
    let id = host.sandbox()?.to_owned();
    let directory = crate::files::sandbox_dir(host)?;
    let Some(recorded) = crate::files::started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    if recorded.retired() {
        return forget(&id, &directory, &recorded);
    }
    let state = match crate::registry::sandbox_live(host) {
        Ok(state) => state,
        Err(gone) if gone.detail.get_bool("containerGone") => {
            let container = gone.detail.get_text("container").unwrap_or("").to_owned();
            reclaim(host, &container, gone.detail.get_bool("containerStopped"))?;
            return Err(gone);
        }
        Err(refusal) => return Err(refusal),
    };
    let _guard = layerfs_store::project::discovery::guard(&state.directory)?;
    // its running commands (D-10): `Busy`, or with `force` cancelled first
    let unreached = &mut crate::exec::cancel::unreached;
    crate::exec::cancel::settle(&state.directory, &id, None, force, unreached)?;
    let recoverable = crate::unmount_guard::check_all(&state)?;
    if !force {
        // A daemon that refuses all control connections must refuse this close
        // with its exact cause in bounded time, not hold the `op` intent while
        // the SDK's ordinary 60 s admission wait runs out. One shared deadline
        // bounds all Workspace probes; N Workspaces cannot multiply it.
        crate::files::refresh_except_within(&state, &recoverable, CLOSE_STATUS_WAIT)?;
    }
    // `force` is the recovery path: it skips the daemon status probe and stops
    // the runtime anyway, reporting every Workspace it could not drain as
    // `unknown` below. It is still never blocked by the daemon it stops.
    // Explicit close can take over an orphaned/retained unmount, but must
    // still refuse every unrelated live Workspace. Stop detaches before
    // killing the daemon; Delete independently checks physical detach.
    let mut mounted = lock(&state.mounts)?
        .keys()
        .filter(|path| !recoverable.contains(path))
        .cloned()
        .collect::<Vec<_>>();
    // a caller that reaches the daemon directly keeps the Workspace records
    // and sends the paths it holds (#535 G4)
    for path in held {
        if !mounted.iter().any(|known| known == path) {
            mounted.push(path.to_owned());
        }
    }
    if !mounted.is_empty() && !force {
        return Err(SdkRefusal {
            code: "MountsActive".to_owned(),
            message: format!(
                "Sandbox {id} still holds {} Workspace(s); commit or unmount them first",
                mounted.len()
            ),
            detail: SdkValue::map([(
                "workspaces",
                SdkValue::List(mounted.into_iter().map(SdkValue::Text).collect()),
            )]),
        });
    }
    let value = crate::files::started(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("Sandbox {id} was closed")))?;
    let mut owner = crate::reopen::recorded(host, &id, &value)?
        .reopen()
        .map_err(failed)?;
    owner.stop(STOP_S).map_err(failed)?;
    owner.delete().map_err(failed)?;
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Closed")),
        ("sandbox", SdkValue::text(&id)),
        (
            "unknown",
            SdkValue::List(mounted.into_iter().map(SdkValue::Text).collect()),
        ),
    ]))
}

/// `sandbox.destroy`: delete only Sandbox `id`'s data plane, for an
/// invisible holder (a copied mount namespace) that exec records cannot see
/// and no unmount can name. It asks the daemon nothing and settles no
/// command: its runtime is stopped and deleted, and the folder goes with it,
/// whatever still holds the mount. The shared Store is outside the folder and
/// never touched.
pub fn destroy(host: &Host) -> Outcome {
    let id = host.sandbox()?.to_owned();
    let directory = crate::files::sandbox_dir(host)?;
    let Some(recorded) = crate::files::started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    if recorded.retired() {
        return Err(crate::reopen::retired(&id, recorded.name().unwrap_or("")));
    }
    let _guard = layerfs_store::project::discovery::guard(&directory)?;
    let Some(value) = crate::files::started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("Sandbox {id} was destroyed"),
        ));
    };
    let mut owner = crate::reopen::recorded(host, &id, &value)?
        .reopen()
        .map_err(failed)?;
    owner.stop(STOP_S).map_err(failed)?;
    owner.delete().map_err(failed)?;
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Destroyed")),
        ("sandbox", SdkValue::text(&id)),
    ]))
}

/// Forgets Sandbox `id`, recorded in `directory` (its daemon record
/// `recorded`) under the retired docker backend (D-8, layerfs-project#111):
/// its folder goes and the answer names its container, which is left for
/// the user to remove (`docker rm -f`); nothing else is touched.
fn forget(id: &str, directory: &Path, recorded: &DaemonRecord) -> Outcome {
    let _guard = layerfs_store::project::discovery::guard(directory)?;
    let container = discard(directory, recorded)?;
    std::fs::remove_dir_all(directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Forgotten")),
        ("sandbox", SdkValue::text(id)),
        ("container", SdkValue::text(&container)),
        (
            "remove",
            SdkValue::text(format!("docker rm -f {container}")),
        ),
    ]))
}

/// Removes what folder `directory` holds of a Sandbox the retired docker
/// backend recorded (`recorded`, its daemon record), under the folder's
/// guard, whose turn file stays; answers the container's name.
fn discard(directory: &Path, recorded: &DaemonRecord) -> Result<String, SdkRefusal> {
    lock(&SANDBOXES)?.remove(directory);
    for entry in std::fs::read_dir(directory).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        if entry.file_name() == layerfs_store::project::discovery::TURN {
            continue;
        }
        let path = entry.path();
        match entry.file_type().map_err(failed)?.is_dir() {
            true => std::fs::remove_dir_all(&path),
            false => std::fs::remove_file(&path),
        }
        .map_err(failed)?;
    }
    Ok(recorded.name().unwrap_or("").to_owned())
}

/// Seconds a closing Sandbox's daemon has to exit before it is killed.
const STOP_S: u16 = 5;

/// Reclaims the Sandbox a call names, whose daemon a reopen found gone or
/// stopped (`container`, its runtime id; `stopped`): one Delete of a
/// stopped runtime, then its folder. Under its folder's guard the record is
/// read again: a Sandbox started again meanwhile has another runtime id
/// and is left alone (`Busy`).
pub fn reclaim(host: &Host, container: &str, stopped: bool) -> Outcome {
    let id = host.sandbox()?;
    let directory = crate::files::sandbox_dir(host)?;
    let _guard = layerfs_store::project::discovery::guard(&directory)?;
    let Some(value) = crate::files::started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    if layerfs_home::helpers::required(value.container(), "container")? != container {
        return Err(refused(
            "Busy",
            format!("Sandbox {id} was started again while it was reclaimed; nothing was removed"),
        ));
    }
    if stopped {
        crate::reopen::recorded(host, id, &value)?
            .stopped()
            .delete()
            .map_err(failed)?;
    }
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Reclaimed")),
        ("id", SdkValue::text(id)),
    ]))
}

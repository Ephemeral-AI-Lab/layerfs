//! A home's remotes and the whole-Store transfers over them (#506): push,
//! pull and load of every project of a Store at once, on the remote drivers
//! and transfers of `layerfs-store-service::remote`.
//!
//! #509: a SQLite Store links to at most one remote file. Every transfer
//! names its remote file (`file`, the object `<prefix>/<file>/store.sqlite`),
//! which the runtime takes from the Store's link: push and pull use the
//! link, publish makes it (a new file), load makes a new Store linked to a
//! file, send uploads a copy to a new file and records nothing. The record
//! `remote-state/<store>` is the link's native twin: remote, file and the
//! last transfer. Which Store links to which file is checked one to one by
//! the runtime, under its registry lock; this module checks the remote.
//!
//! Remotes live in the home's settings file, which holds credential
//! references, never secrets. A call may instead carry `credentials:
//! { accessKey, secretKey }`, used for that call only and never written. The
//! last push or pull of each Store is recorded in the home, outside the
//! Store, since a pull replaces the Store file. Each transfer is one
//! attempt: an uncertain outcome is reported, never resent.
//!
//! #506 E2/E4: each transfer records the moved file's history heads
//! digest (`heads`), and a call naming its runtime job (`job`) reports its
//! phases to that job's progress cell (`progress.rs`). Pull and load hold
//! their Store's gate (`local.rs`), so this process's heads reads wait for
//! neither and never open the Store under them.
use crate::discovery;
use crate::job::{failed, refused, Outcome};
use crate::registry::{random, text};
use crate::stores::{self, Identity, Kind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::HistoryName;
use layerfs_store_service::remote::storage::{
    cloudflare_r2::R2Settings,
    minio::{self, MinioSettings, DEFAULT_PART_SIZE},
    s3_api::{S3Client, S3Config},
    CredentialRef, Credentials, Driver, RemoteError,
};
use layerfs_store_service::remote::{self, state, Direction, Provider, Remotes, Transfer};
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

fn settings_path(args: &SdkValue) -> Result<PathBuf> {
    Ok(Remotes::home_path(&discovery::home(args)?))
}

fn load(args: &SdkValue) -> Result<Remotes> {
    Remotes::load(&settings_path(args)?).map_err(|cause| refused("Failed", cause))
}

/// One remote as `runtime.remote.list` shows it: no secret, only its reference.
fn shown(name: &str, provider: &Provider) -> SdkValue {
    match provider {
        Provider::Minio(s) => SdkValue::map([
            ("name", SdkValue::text(name)),
            ("provider", SdkValue::text("minio")),
            ("endpoint", SdkValue::text(&s.endpoint)),
            ("bucket", SdkValue::text(&s.bucket)),
            ("prefix", SdkValue::text(&s.prefix)),
            ("region", SdkValue::text(&s.region)),
            ("credentials", SdkValue::text(s.credentials.text())),
            ("partSize", SdkValue::Int(s.part_size as i64)),
        ]),
        Provider::R2(s) => SdkValue::map([
            ("name", SdkValue::text(name)),
            ("provider", SdkValue::text("r2")),
            ("account", SdkValue::text(&s.account)),
            ("jurisdiction", SdkValue::text(&s.jurisdiction)),
            ("endpoint", SdkValue::text(s.endpoint())),
            ("bucket", SdkValue::text(&s.bucket)),
            ("prefix", SdkValue::text(&s.prefix)),
            ("region", SdkValue::text("auto")),
            ("credentials", SdkValue::text(s.credentials.text())),
            ("partSize", SdkValue::Int(s.part_size as i64)),
        ]),
        Provider::LocalFolder(path) => SdkValue::map([
            ("name", SdkValue::text(name)),
            ("provider", SdkValue::text("local_folder")),
            ("path", SdkValue::text(path.display().to_string())),
        ]),
    }
}

/// `runtime.remote.list { home }`.
pub(crate) fn list(args: &SdkValue) -> Outcome {
    let remotes = load(args)?;
    Ok(SdkValue::List(
        remotes
            .names()
            .map(|name| shown(name, remotes.get(name).expect("listed")))
            .collect(),
    ))
}

/// `runtime.remote.add { home, remote: { name, provider: minio, endpoint,
/// bucket, prefix?, region?, credentials?, partSize? } | { name, provider:
/// r2, account, jurisdiction?, bucket, prefix?, credentials?, partSize? } |
/// { name, provider: local_folder, path } }`. `credentials` is a reference (`env:LAYERFS_…` or
/// `file:/path`; default `env:LAYERFS_REMOTE`), never a secret. A name in
/// use is refused `Exists`.
pub(crate) fn add(args: &SdkValue) -> Outcome {
    let given = args
        .get("remote")
        .ok_or_else(|| refused("InvalidInput", "remote { name, provider, … } is required"))?;
    let name = text(given, "name")?;
    let credentials = CredentialRef::parse(
        given
            .get_text("credentials")
            .unwrap_or("env:LAYERFS_REMOTE"),
    )
    .map_err(|error| refused("InvalidInput", error.to_string()));
    let part_size = match given.get("partSize") {
        None | Some(SdkValue::Null) => Ok(DEFAULT_PART_SIZE),
        Some(SdkValue::Int(n)) if *n > 0 => Ok(*n as u64),
        Some(_) => Err(refused(
            "InvalidInput",
            "partSize is a whole number of bytes",
        )),
    };
    let provider = match text(given, "provider")? {
        "minio" => Provider::Minio(MinioSettings {
            endpoint: text(given, "endpoint")?.to_owned(),
            bucket: text(given, "bucket")?.to_owned(),
            prefix: given.get_text("prefix").unwrap_or_default().to_owned(),
            region: given.get_text("region").unwrap_or("us-east-1").to_owned(),
            credentials: credentials?,
            part_size: part_size?,
        }),
        "r2" => Provider::R2(R2Settings {
            account: text(given, "account")?.to_owned(),
            jurisdiction: given
                .get_text("jurisdiction")
                .unwrap_or_default()
                .to_owned(),
            bucket: text(given, "bucket")?.to_owned(),
            prefix: given.get_text("prefix").unwrap_or_default().to_owned(),
            credentials: credentials?,
            part_size: part_size?,
        }),
        "local_folder" => {
            let path = PathBuf::from(text(given, "path")?);
            if !path.is_absolute() {
                return Err(refused(
                    "InvalidInput",
                    "a local_folder remote's path is absolute",
                ));
            }
            Provider::LocalFolder(path)
        }
        other => {
            return Err(refused(
                "Unsupported",
                format!("remote provider {other}: minio, r2 or local_folder"),
            ))
        }
    };
    // checked as a connection would be, without keys; an https:// endpoint
    // in a build without TLS is refused here, before it is saved
    if let Some(config) = s3_config(&provider, placeholder())? {
        config.validate().map_err(|error| match error {
            RemoteError::Unsupported(why) => refused("Unsupported", why),
            other => refused("InvalidInput", other.to_string()),
        })?;
    }
    let home = discovery::home(args)?;
    let _guard = discovery::guard(&home)?;
    let mut remotes = load(args)?;
    if remotes.get(name).is_some() {
        return Err(refused("Exists", format!("a remote named {name} exists")));
    }
    remotes
        .set(name, provider.clone())
        .map_err(|cause| refused("InvalidInput", cause))?;
    remotes
        .save(&settings_path(args)?)
        .map_err(|cause| refused("Failed", cause))?;
    Ok(shown(name, &provider))
}

/// `runtime.remote.remove { home, name }`: forgets the remote; its Store
/// files stay on it. Refused `InUse`, naming them, while a Store's link
/// record names the remote (#509).
pub(crate) fn remove(args: &SdkValue) -> Outcome {
    let name = text(args, "name")?;
    let home = discovery::home(args)?;
    let _guard = discovery::guard(&home)?;
    let mut remotes = load(args)?;
    if remotes.remove(name).is_none() {
        return Err(refused("NoSuchRemote", format!("no remote named {name}")));
    }
    let linked = linked_to(&home, name)?;
    if !linked.is_empty() {
        return Err(refused(
            "InUse",
            format!(
                "Stores link to remote {name}: {}; unlink them first",
                linked.join(", ")
            ),
        ));
    }
    remotes
        .save(&settings_path(args)?)
        .map_err(|cause| refused("Failed", cause))?;
    Ok(SdkValue::map([("removed", SdkValue::text(name))]))
}

/// The Stores whose link record names the remote `name`, in name order.
fn linked_to(home: &Path, name: &str) -> Result<Vec<String>> {
    let directory = home.join(state::STATE_DIRECTORY);
    let items = match std::fs::read_dir(&directory) {
        Ok(items) => items,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(refused(
                "Failed",
                format!("{}: {error}", directory.display()),
            ))
        }
    };
    let mut stores = Vec::new();
    for item in items {
        let item = item.map_err(|error| refused("Failed", error.to_string()))?;
        let store = item.file_name().to_string_lossy().into_owned();
        if remote::store_object(&store).is_err() {
            continue; // a staging file, or not a record
        }
        let record = state::load(&item.path()).map_err(|cause| refused("Failed", cause))?;
        if record.is_some_and(|record| record.remote == name) {
            stores.push(store);
        }
    }
    stores.sort();
    Ok(stores)
}

fn placeholder() -> Credentials {
    Credentials {
        access_key: "-".into(),
        secret_key: "-".into(),
    }
}

/// The S3 client inputs of an S3 provider (MinIO or R2), as its `connect`
/// makes them, with `credentials`; none for a local folder.
fn s3_config(provider: &Provider, credentials: Credentials) -> Result<Option<S3Config>> {
    Ok(match provider {
        Provider::Minio(settings) => Some(minio_config(settings, credentials)),
        Provider::R2(settings) => Some(
            settings
                .config(credentials)
                .map_err(|error| refused("InvalidInput", error.to_string()))?,
        ),
        Provider::LocalFolder(_) => None,
    })
}

/// The MinIO client of `settings`, as `minio::connect` makes it, with `credentials`.
fn minio_config(settings: &MinioSettings, credentials: Credentials) -> S3Config {
    S3Config {
        endpoint: settings.endpoint.clone(),
        bucket: settings.bucket.clone(),
        prefix: settings.prefix.clone(),
        region: settings.region.clone(),
        credentials,
        connect_timeout: Duration::from_secs(5),
        io_timeout: Duration::from_secs(60),
        part_size: settings.part_size,
        limits: minio::limits(),
    }
}

/// The driver of the call's `remote`, with its per-call `credentials` when given.
pub(crate) fn driver(args: &SdkValue) -> Result<(String, Box<dyn Driver>)> {
    let name = text(args, "remote")?;
    let remotes = load(args)?;
    let provider = remotes.get(name).ok_or_else(|| {
        refused(
            "NoSuchRemote",
            format!("no remote named {name} in this home"),
        )
    })?;
    let given = args
        .get("credentials")
        .filter(|v| !matches!(v, SdkValue::Null));
    let keys = match given {
        Some(given) => Some(Credentials {
            access_key: text(given, "accessKey")?.to_owned(),
            secret_key: text(given, "secretKey")?.to_owned(),
        }),
        None => None,
    };
    let driver: Box<dyn Driver> = match keys
        .map(|keys| s3_config(provider, keys))
        .transpose()?
        .flatten()
    {
        Some(config) => Box::new(
            S3Client::new(config).map_err(|error| refused("InvalidInput", error.to_string()))?,
        ),
        None => provider
            .driver()
            .map_err(|error| refused("InvalidInput", error.to_string()))?,
    };
    Ok((name.to_owned(), driver))
}

/// Records a completed transfer of `store`; a record that fails is reported
/// beside the transfer, which completed.
fn record(home: &Path, store: &str, transfer: &Transfer) -> Option<SdkValue> {
    crate::progress::begin("record");
    let saved = state::state_path(home, store).and_then(|path| state::save(&path, transfer));
    saved.err().map(SdkValue::text)
}

fn transfer_value(transfer: &Transfer) -> SdkValue {
    SdkValue::map([
        ("remote", SdkValue::text(&transfer.remote)),
        ("file", SdkValue::text(&transfer.file)),
        (
            "direction",
            SdkValue::text(match transfer.direction {
                Direction::Push => "push",
                Direction::Pull => "pull",
            }),
        ),
        ("at", SdkValue::Int(transfer.at as i64)),
        ("sha256", SdkValue::text(hex(&transfer.sha256))),
        ("bytes", SdkValue::Int(transfer.bytes as i64)),
        (
            "heads",
            transfer
                .heads
                .map_or(SdkValue::Null, |heads| SdkValue::text(hex(&heads))),
        ),
    ])
}

/// The call's remote file, `file` (`[A-Za-z0-9._-]`, the object `<file>/store.sqlite`).
fn file(args: &SdkValue) -> Result<&str> {
    let file = text(args, "file")?;
    remote::store_object(file).map_err(|cause| refused("InvalidInput", cause))?;
    Ok(file)
}

/// `runtime.store.get_link { home, store }`: the Store's native link record
/// (remote, file and last transfer), or null.
pub(crate) fn last(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let path = state::state_path(&home, text(args, "store")?)
        .map_err(|cause| refused("InvalidInput", cause))?;
    match state::load(&path).map_err(|cause| refused("Failed", cause))? {
        Some(transfer) => Ok(transfer_value(&transfer)),
        None => Ok(SdkValue::Null),
    }
}

/// `runtime.store.unlink { home, store }`: drops the Store's native link
/// record; the remote file stays. `{ removed }` says whether there was one.
pub(crate) fn unlink(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let path = state::state_path(&home, text(args, "store")?)
        .map_err(|cause| refused("InvalidInput", cause))?;
    let removed = state::remove(&path).map_err(|cause| refused("Failed", cause))?;
    Ok(SdkValue::map([("removed", SdkValue::Bool(removed))]))
}

fn answer(kind: &str, store: &str, transfer: &Transfer, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![
        ("kind".to_owned(), SdkValue::text(kind)),
        ("store".to_owned(), SdkValue::text(store)),
        ("transfer".to_owned(), transfer_value(transfer)),
    ];
    fields.extend(more.into_iter().map(|(k, v)| (k.to_owned(), v)));
    SdkValue::Map(fields)
}

/// What an upload does with the Store's link.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Upload {
    /// To the Store's link; the file must be what the link's record says.
    Push,
    /// To a new file, which becomes the Store's link.
    Publish,
    /// To a new file; the Store's record does not change.
    Send,
}

/// `runtime.store.push { home, store, storePath, remote, file, credentials?,
/// force? }`: one consistent snapshot of the live Store, uploaded whole to
/// its linked file `<prefix>/<file>/store.sqlite`, if that file is what the
/// link's record says (else `Conflict`), or whatever is there with `force`.
pub(crate) fn push(args: &SdkValue) -> Outcome {
    crate::progress::observed(args, || upload(args, Upload::Push))
}

/// `runtime.store.publish { home, store, storePath, remote, file,
/// credentials? }`: as push, to a new file (`Exists` when an object is
/// there, checked by a HEAD before the snapshot and again before the
/// upload), whose record links the Store to it.
pub(crate) fn publish(args: &SdkValue) -> Outcome {
    crate::progress::observed(args, || upload(args, Upload::Publish))
}

/// `runtime.store.send { home, store, storePath, remote, file, credentials? }`:
/// as publish, but the Store's record is not touched: the new file is linked
/// to no Store.
pub(crate) fn send(args: &SdkValue) -> Outcome {
    crate::progress::observed(args, || upload(args, Upload::Send))
}

fn upload(args: &SdkValue, kind: Upload) -> Outcome {
    let store = text(args, "store")?;
    let file = file(args)?;
    let identity = stores::identity(args)?;
    stores::sqlite_only(&identity, "push")?;
    let path = stores::store_path(args)?;
    let (remote, driver) = driver(args)?;
    let expected = match kind {
        Upload::Push => expected(args, store, &remote, file)?,
        Upload::Publish | Upload::Send => remote::Expected::Absent,
    };
    let pushed = crate::direct::with_service(args, |service| {
        remote::push(
            &service.opened,
            &path,
            driver.as_ref(),
            file,
            &identity.binding,
            expected,
        )
        .map_err(|failure| refused(push_code(&failure, kind), failure_text(&failure, kind)))
    })?;
    let transfer = Transfer::now(&remote, file, Direction::Push, pushed.sha256, pushed.bytes)
        .with_heads(pushed.heads);
    let mut more = vec![
        ("object", SdkValue::text(&pushed.object)),
        ("pieces", SdkValue::Int(pushed.pieces as i64)),
    ];
    if let Some(cleanup) = pushed.cleanup {
        more.push(("cleanup", SdkValue::text(cleanup.to_string())));
    }
    if kind != Upload::Send {
        if let Some(error) = record(&discovery::home(args)?, store, &transfer) {
            more.push(("recordFailed", error));
        }
    }
    let name = match kind {
        Upload::Push => "Pushed",
        Upload::Publish => "Published",
        Upload::Send => "Sent",
    };
    Ok(answer(name, store, &transfer, more))
}

/// What the linked file must be for a push: what the link's record says
/// (`state::expected`); `force: true` skips the check and overwrites.
fn expected(args: &SdkValue, store: &str, remote: &str, file: &str) -> Result<remote::Expected> {
    if args.get_bool("force") {
        return Ok(remote::Expected::Any);
    }
    let home = discovery::home(args)?;
    let path = state::state_path(&home, store).map_err(|cause| refused("InvalidInput", cause))?;
    state::expected(&path, remote, file).map_err(|cause| refused("Failed", cause))
}

fn push_code(failure: &remote::PushFailure, kind: Upload) -> &'static str {
    use layerfs_store_service::remote::storage::RemoteError;
    match failure {
        remote::PushFailure::Invalid(_) => "InvalidInput",
        remote::PushFailure::Space { .. } => "Capacity",
        remote::PushFailure::RemoteMoved { .. } if kind == Upload::Push => "Conflict",
        remote::PushFailure::RemoteMoved { .. } => "Exists",
        remote::PushFailure::Remote(RemoteError::Uncertain { .. }) => "Unknown",
        _ => "Failed",
    }
}

fn failure_text(failure: &remote::PushFailure, kind: Upload) -> String {
    match (failure, kind) {
        (remote::PushFailure::RemoteMoved { .. }, Upload::Publish | Upload::Send) => {
            "Store upload: the remote already holds a file of that name; a publish or a \
             send makes a new file and never overwrites one: name another file"
                .to_owned()
        }
        _ => failure.to_string(),
    }
}

/// `runtime.store.pull { home, store, storePath, remote, file, credentials? }`:
/// replaces the local Store, with every project, by its linked file. Refused
/// while anything holds the Store open, this process included. Projects new
/// in the pulled Store get entries; an entry whose project the pulled Store
/// lacks is listed in `missing` and kept.
pub(crate) fn pull(args: &SdkValue) -> Outcome {
    let path = stores::store_path(args)?;
    crate::local::exclusive(&path, || {
        crate::progress::observed(args, || pull_once(args))
    })
}

fn pull_once(args: &SdkValue) -> Outcome {
    let store = text(args, "store")?;
    let file = file(args)?;
    let identity = stores::identity(args)?;
    stores::sqlite_only(&identity, "pull")?;
    let path = stores::store_path(args)?;
    if crate::direct::holds(&path)? {
        return Err(refused(
            "Busy",
            "this process holds the Store open; close its clients and pull again",
        ));
    }
    let (remote, driver) = driver(args)?;
    let pulled = remote::pull(
        driver.as_ref(),
        file,
        &path,
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(|failure| refused("Failed", failure.to_string()))?;
    let transfer = Transfer::now(&remote, file, Direction::Pull, pulled.sha256, pulled.bytes)
        .with_heads(pulled.heads);
    let (added, missing) = register(args, &identity)?;
    let mut more = vec![
        ("replaced", SdkValue::Bool(pulled.replaced)),
        ("added", added),
        ("missing", missing),
    ];
    if let Some(error) = record(&discovery::home(args)?, store, &transfer) {
        more.push(("recordFailed", error));
    }
    Ok(answer("Pulled", store, &transfer, more))
}

/// `runtime.store.load { home, store, remote, file, storePath, credentials? }`:
/// a new local Store `store` (at `storePath`, which must not exist) from the
/// remote file `file`, with an entry for each project, linked to the file.
pub(crate) fn load_store(args: &SdkValue) -> Outcome {
    let path = stores::store_path(args)?;
    crate::local::exclusive(&path, || {
        crate::progress::observed(args, || load_once(args))
    })
}

fn load_once(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let store = text(args, "store")?;
    let file = file(args)?;
    let path = stores::store_path(args)?;
    let (remote, driver) = driver(args)?;
    let directory = stores::claim(&home, store)?;
    let _guard = discovery::guard(&directory)?;
    let cursor_key = random()?;
    let loaded = match remote::load(driver.as_ref(), file, &path, cursor_key) {
        Ok(loaded) => loaded,
        Err(failure) => {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refused("Failed", failure.to_string()));
        }
    };
    let identity = Identity {
        kind: Kind::Sqlite,
        binding: loaded.binding,
        cursor_key,
    };
    stores::save_record(&home, store, &identity)?;
    let transfer = Transfer::now(&remote, file, Direction::Pull, loaded.sha256, loaded.bytes)
        .with_heads(loaded.heads);
    let (added, _) = register(args, &identity)?;
    let mut more = vec![("projects", added)];
    if let Some(error) = record(&home, store, &transfer) {
        more.push(("recordFailed", error));
    }
    Ok(answer("Loaded", store, &transfer, more))
}

/// Gives every project of the Store the call names an entry (its first
/// Branch `main`, as Init names it): `(added, missing)`, where `missing`
/// lists entries whose project the Store no longer has.
fn register(args: &SdkValue, identity: &Identity) -> Result<(SdkValue, SdkValue)> {
    let home = discovery::home(args)?;
    let store = text(args, "store")?;
    let path = stores::store_path(args)?;
    let main = HistoryName::new("main").map_err(failed)?;
    let found = crate::direct::with_service(args, |service| {
        let sqlite = service.opened.sqlite_version.clone();
        service
            .store
            .published(|history| {
                let mut found = Vec::new();
                for stack in layerfs_project::store::projects(history)? {
                    let snapshot = match history.branch_named(stack.id, &main)? {
                        Some(branch) => history.branch_snapshot(branch.id)?,
                        None => None,
                    };
                    found.push((stack, snapshot));
                }
                Ok((found, sqlite))
            })
            .map_err(failed)
    })?;
    let (found, sqlite) = found;
    let projects = stores::store_dir(&home, store)?.join("projects");
    let mut added = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for (stack, snapshot) in found {
        let name = stack.name.as_str().to_owned();
        names.insert(name.clone());
        let own = name == store && stores::legacy(&home, store)?;
        if own || projects.join(&name).exists() {
            continue;
        }
        let Some(snapshot) = snapshot else { continue };
        let entry = stores::make_entry(&home, store, &name)?;
        stores::save_entry(
            &entry,
            stores::manifest(
                identity,
                &path,
                stack.id,
                snapshot.branch.id,
                snapshot.effective_root.to_bytes(),
                &sqlite,
            )?,
        )?;
        added.push(SdkValue::text(name));
    }
    let mut missing = Vec::new();
    if projects.exists() {
        for item in std::fs::read_dir(&projects).map_err(failed)? {
            let name = item
                .map_err(failed)?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if !names.contains(&name) {
                missing.push(SdkValue::text(name));
            }
        }
    }
    Ok((SdkValue::List(added), SdkValue::List(missing)))
}

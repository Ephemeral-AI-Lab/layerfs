//! In-process StoreService opening, closing, and call dispatch.
use crate::discovery::project_manifest;
use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::{OpenRefusal, StoreService};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

struct Opened {
    service: Arc<StoreService>,
    clients: usize,
    directory: PathBuf,
}

static STORES: LazyLock<Mutex<BTreeMap<PathBuf, Opened>>> = LazyLock::new(Default::default);

fn refused(code: &str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_string(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

fn failed<E: std::fmt::Display>(e: E) -> SdkRefusal {
    SdkRefusal {
        code: "Failed".to_string(),
        message: e.to_string(),
        detail: SdkValue::Null,
    }
}

fn store_path(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    match args.get_text("storePath") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(refused(
            "InvalidInput",
            "Store calls need the client's storePath",
        )),
    }
}

fn open(path: &Path, args: &SdkValue) -> Result<Opened, SdkRefusal> {
    let manifest = project_manifest(args)?;
    let random_hex = layerfs_bridge::sdk_values::hex(
        &layerfs_bridge::duplex::generate_keypair()
            .map(|p| p.public)
            .map_err(failed)?[..12],
    );
    let directory = std::env::temp_dir().join(format!(
        "layerfs-store-{}-{}",
        std::process::id(),
        random_hex
    ));
    std::fs::create_dir_all(&directory).map_err(failed)?;
    let template = directory.join("overlay.sqlite");
    let opened = StoreService::open_direct(path, &manifest.binding, manifest.cursor_key, &template);
    match opened {
        Ok(service) => Ok(Opened {
            service: Arc::new(service),
            clients: 0,
            directory,
        }),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            Err(match refusal {
                OpenRefusal::Unsupported(reason) => refused("Unsupported", reason.0),
                OpenRefusal::Missing(reason) => refused("NoStore", reason),
                OpenRefusal::Failed(reason) => refused("Failed", reason),
            })
        }
    }
}

pub fn store_open(args: &SdkValue) -> Result<SdkValue, SdkRefusal> {
    let path = store_path(args)?;
    let mut stores = STORES
        .lock()
        .map_err(|_| refused("Failed", "stores lock poisoned"))?;
    if !stores.contains_key(&path) {
        let opened = open(&path, args)?;
        stores.insert(path.clone(), opened);
    }
    let opened = stores.get_mut(&path).expect("store present");
    opened.clients += 1;
    Ok(SdkValue::Null)
}

pub fn store_close(args: &SdkValue) -> Result<SdkValue, SdkRefusal> {
    let path = store_path(args)?;
    let mut stores = STORES
        .lock()
        .map_err(|_| refused("Failed", "stores lock poisoned"))?;
    let Some(opened) = stores.get_mut(&path) else {
        return Err(refused("InvalidInput", "this process holds no such Store"));
    };
    opened.clients = opened.clients.saturating_sub(1);
    if opened.clients > 0 {
        return Ok(SdkValue::Null);
    }
    let opened = stores.remove(&path).expect("store present");
    drop(stores);
    if let Ok(service) = Arc::try_unwrap(opened.service) {
        let _ = service.close();
    }
    let _ = std::fs::remove_dir_all(&opened.directory);
    Ok(SdkValue::Null)
}

fn service(args: &SdkValue) -> Result<Arc<StoreService>, SdkRefusal> {
    let path = store_path(args)?;
    STORES
        .lock()
        .map_err(|_| refused("Failed", "stores lock poisoned"))?
        .get(&path)
        .map(|opened| opened.service.clone())
        .ok_or_else(|| refused("InvalidInput", "the client has not opened its Store"))
}

pub fn call(op: &str, args: SdkValue) -> Result<SdkValue, SdkRefusal> {
    if op == "project.init" {
        return crate::project::init(&args);
    }
    let s = service(&args)?;
    let stripped = strip_meta_args(args);
    let call = SdkCall {
        method: op.to_string(),
        args: stripped,
    };
    layerfs_api::call(&s, &call)
}

pub fn backend_call(op: &str, args: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let s = service(&args)?;
    let stripped = strip_meta_args(args);
    let call = SdkCall {
        method: op.to_string(),
        args: stripped,
    };
    layerfs_store_service::operator::dispatch(&s, &call)
}

fn strip_meta_args(args: SdkValue) -> SdkValue {
    match args {
        SdkValue::Map(fields) => SdkValue::Map(
            fields
                .into_iter()
                .filter(|(k, _)| !matches!(k.as_str(), "storePath" | "sandbox" | "deploy" | "home"))
                .collect(),
        ),
        other => other,
    }
}

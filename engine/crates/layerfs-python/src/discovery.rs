//! Project discovery and Store manifest persistence.
use layerfs_bridge::control::{Answer, Reply, SdkRefusal, SdkValue};
use layerfs_bridge::provision::{StoreManifest, StoreProfile};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn failed<E: std::fmt::Display>(e: E) -> SdkRefusal {
    SdkRefusal {
        code: "Failed".to_string(),
        message: e.to_string(),
        detail: SdkValue::Null,
    }
}

fn refused(code: &str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_string(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

pub fn home(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    let home_path = args
        .get_text("home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LAYERFS_HOME").map(PathBuf::from))
        .ok_or_else(|| refused("InvalidInput", "LAYERFS_HOME is required"))?;
    if !home_path.is_absolute() {
        return Err(refused("InvalidInput", "LAYERFS_HOME must be absolute"));
    }
    ensure_directory(&home_path)?;
    Ok(home_path)
}

pub fn ensure_directory(path: &Path) -> Result<(), SdkRefusal> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(failed(e)),
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(failed)?;
    }
    Ok(())
}

pub fn project_manifest(args: &SdkValue) -> Result<StoreManifest, SdkRefusal> {
    let home_dir = home(args)?;
    let name = args
        .get_text("project")
        .or_else(|| args.get_text("name"))
        .ok_or_else(|| refused("InvalidInput", "missing project name"))?;
    let path = home_dir.join(name);
    let record_file = path.join("project.record");
    if !record_file.is_file() {
        return Err(refused(
            "NoSuchProject",
            format!("no project.record for project {name}"),
        ));
    }
    let value = load_record(&record_file)?;
    let SdkValue::Bytes(manifest_bytes) = value.get("manifest").cloned().unwrap_or(SdkValue::Null)
    else {
        return Err(refused("NoSuchProject", "no complete project manifest"));
    };
    let manifest = StoreManifest::decode(&manifest_bytes)
        .map_err(|e| refused("InvalidInput", format!("manifest decode: {e}")))?;
    if manifest.profile != StoreProfile::Disposable {
        return Err(refused("InvalidInput", "Disposable profile required"));
    }
    Ok(manifest)
}

pub fn load_record(path: &Path) -> Result<SdkValue, SdkRefusal> {
    let mut file = OpenOptions::new().read(true).open(path).map_err(failed)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(failed)?;
    let answer = Answer::decode(&bytes)
        .map_err(|e| refused("InvalidInput", format!("record decode: {e}")))?;
    match answer.reply {
        Reply::Sdk(value) => Ok(value),
        _ => Err(refused("InvalidInput", "discovery record not SDK value")),
    }
}

pub fn save_record(path: &Path, value: SdkValue) -> Result<(), SdkRefusal> {
    let answer = Answer {
        id: 1,
        reply: Reply::Sdk(value),
    };
    let bytes = answer
        .encode()
        .map_err(|e| refused("Failed", format!("record encode: {e}")))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(failed)?;
    file.write_all(&bytes).map_err(failed)?;
    Ok(())
}

pub fn save_project(
    dir: &Path,
    manifest: &StoreManifest,
    installed: Option<&StoreManifest>,
) -> Result<(), SdkRefusal> {
    let manifest_bytes = manifest
        .encode()
        .map_err(|e| refused("Failed", format!("manifest encode: {e}")))?;
    let mut fields = vec![("manifest".to_string(), SdkValue::Bytes(manifest_bytes))];
    if let Some(inst) = installed {
        let inst_bytes = inst
            .encode()
            .map_err(|e| refused("Failed", format!("installed encode: {e}")))?;
        fields.push(("installed".to_string(), SdkValue::Bytes(inst_bytes)));
    }
    save_record(&dir.join("project.record"), SdkValue::Map(fields))
}

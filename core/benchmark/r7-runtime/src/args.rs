use crate::Result;
use std::{collections::BTreeMap, path::PathBuf};

pub const IMAGE: &str = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6";

pub struct Args {
    pub mode: String,
    pub socket: String,
    pub image: String,
    pub volume: String,
    pub executable: PathBuf,
    pub receipt: PathBuf,
    pub manifest: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub source: Option<PathBuf>,
    pub sealed: Option<PathBuf>,
    pub init_seconds: u64,
}
impl Args {
    pub fn parse() -> Result<Self> {
        let mut input = std::env::args().skip(1);
        let mode = input.next().ok_or("provision or serve required")?;
        let mut values = BTreeMap::new();
        while let Some(key) = input.next() {
            if !key.starts_with("--") {
                return Err(format!("expected option, got {key}").into());
            }
            let value = input.next().ok_or("missing option value")?;
            if values.insert(key, value).is_some() {
                return Err("duplicate option".into());
            }
        }
        let mut required = |name| values.remove(name).ok_or_else(|| format!("missing {name}"));
        let socket = required("--socket")?;
        let volume = required("--volume")?;
        let executable = required("--daemon")?.into();
        let receipt = required("--receipt")?.into();
        let manifest = required("--manifest")?.into();
        let uid: u32 = required("--uid")?.parse()?;
        let gid = required("--gid")?.parse()?;
        if uid == 0 {
            return Err("ordinary command identity must be nonroot".into());
        }
        let image = values.remove("--image").unwrap_or_else(|| IMAGE.into());
        if image != IMAGE {
            return Err("R7 pinned image required".into());
        }
        let source = values.remove("--source-copy").map(PathBuf::from);
        let sealed = values.remove("--sealed").map(PathBuf::from);
        let init_seconds = values.remove("--init-seconds").map(|v| v.parse()).transpose()?.unwrap_or(120);
        if init_seconds == 0 {
            return Err("explicit positive setup wall stop required".into());
        }
        if !values.is_empty() {
            return Err(format!("unknown options: {:?}", values.keys()).into());
        }
        Ok(Self { mode, socket, image, volume, executable, receipt, manifest, uid, gid, source, sealed, init_seconds })
    }
}

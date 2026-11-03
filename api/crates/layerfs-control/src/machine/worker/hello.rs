//! The worker's answer to the caller's secret (#536 §9 `worker.hello`):
//! `{ ready: true, protocol, bundle }`, the protocol it speaks and the hash
//! of the sealed bundle it runs from. The caller compares both with its own
//! before it sends anything: a worker of another build is refused, and its
//! machine is stopped and started again to run this build's worker.
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The worker protocol this build speaks. The first worker answered its
/// secret with `{ ready: true }` alone, which names no protocol.
pub const PROTOCOL: i64 = 2;

/// What a worker says of itself when it answers its secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// The protocol it speaks; `None` for a worker of the first hello.
    pub protocol: Option<i64>,
    /// The hash of the sealed bundle it runs from; `None` for a worker run
    /// from no sealed bundle (a test's or a development build's).
    pub bundle: Option<String>,
}

impl Hello {
    /// This process as a worker: [`PROTOCOL`] and the bundle beside its
    /// executable (`bundle.json` next to the guest's `layerfs`).
    pub fn own() -> Self {
        let exe = std::env::current_exe().ok();
        let dir = exe.as_deref().and_then(Path::parent);
        Self {
            protocol: Some(PROTOCOL),
            bundle: dir.and_then(|dir| sealed_hash(dir).ok().flatten()),
        }
    }

    /// A worker's answer frame, read as a hello.
    pub fn of(frame: &SdkValue) -> Self {
        Self {
            protocol: frame.get_int("protocol"),
            bundle: frame.get_text("bundle").map(str::to_owned),
        }
    }

    /// The answer frame: `{ ready: true, protocol, bundle }`.
    pub fn to_frame(&self) -> SdkValue {
        SdkValue::map([
            ("ready", SdkValue::Bool(true)),
            (
                "protocol",
                self.protocol.map_or(SdkValue::Null, SdkValue::Int),
            ),
            ("bundle", SdkValue::opt_text(self.bundle.clone())),
        ])
    }

    /// `{ protocol, bundle }`.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            (
                "protocol",
                self.protocol.map_or(SdkValue::Null, SdkValue::Int),
            ),
            ("bundle", SdkValue::opt_text(self.bundle.clone())),
        ])
    }

    /// Why this worker is not one this build may send calls to, if it is
    /// not: another protocol (or none), or a bundle other than the one this
    /// build deploys. A worker run from no sealed bundle is checked by its
    /// protocol alone. `Unavailable`, its `detail` naming both sides
    /// (`worker`, `expected`); nothing was sent.
    pub fn mismatch(&self, address: &str) -> Option<SdkRefusal> {
        let deployed = if self.bundle.is_some() {
            deployed()
        } else {
            None
        };
        let expected = Self {
            protocol: Some(PROTOCOL),
            bundle: deployed.clone(),
        };
        let same_protocol = self.protocol == Some(PROTOCOL);
        let same_bundle = self.bundle.is_none() || self.bundle == deployed;
        if same_protocol && same_bundle {
            return None;
        }
        let show = |hello: &Self| {
            let protocol = hello.protocol.map_or("none".to_owned(), |p| p.to_string());
            let bundle = hello.bundle.as_deref().unwrap_or("none");
            format!("protocol {protocol}, bundle {bundle}")
        };
        let mut refusal = refused(
            "Unavailable",
            format!(
                "the worker at {address} is of another build ({}; this build: {}); nothing was sent",
                show(self),
                show(&expected)
            ),
        );
        refusal.detail =
            SdkValue::map([("worker", self.to_sdk()), ("expected", expected.to_sdk())]);
        Some(refusal)
    }
}

/// Whether `refusal` is a [`Hello::mismatch`].
pub fn is_mismatch(refusal: &SdkRefusal) -> bool {
    refusal.detail.get("expected").is_some() && refusal.detail.get("worker").is_some()
}

/// The calls that replace machine `machine`'s worker with this build's:
/// a start installs the bundle this build deploys (#535 D11: by hand).
pub fn replace_with(machine: &str) -> String {
    format!("`layerfs machine stop {machine}`, then `layerfs machine start {machine}`")
}

/// The hash `bundle.json` in `dir` names, if there is one.
pub(crate) fn sealed_hash(dir: &Path) -> Result<Option<String>, SdkRefusal> {
    let manifest = crate::common::home::read_record(&dir.join("bundle.json"))?;
    Ok(manifest.and_then(|m| m.get_text("hash").map(str::to_owned)))
}

/// The hash of the bundle this build deploys to a machine, if it is there.
fn deployed() -> Option<String> {
    let dir = crate::sandbox::deploy::bundle_dir("linux").ok()?;
    sealed_hash(&dir).ok().flatten()
}

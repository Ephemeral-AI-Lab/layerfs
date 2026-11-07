//! A command's exec id (compute-plane 2.0 §5.1, O-5): the caller's name for
//! it, unique among its Workspace's running commands (D-10), or one control
//! makes when the caller names none. It names the command's record
//! (`workspace/<id>/exec/<exec id>.json`) and its cgroup, so it is one plain
//! path component.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;

/// The longest exec id.
pub const MAX: usize = 64;

/// A checked exec id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecId(String);

impl ExecId {
    /// `text` as an exec id: 1 to 64 ASCII letters, digits, `.`, `_` or
    /// `-`, not starting with `.` or `-`; refused `InvalidInput` otherwise.
    pub fn parse(text: &str) -> Result<Self, SdkRefusal> {
        if text.len() > MAX || !layerfs_sandbox_backend::stop::component(text) {
            return Err(refused(
                "InvalidInput",
                format!(
                    "exec_id is 1 to {MAX} letters, digits, '.', '_' or '-', not starting with \
                     '.' or '-'; not {text:?}"
                ),
            ));
        }
        Ok(Self(text.to_owned()))
    }

    /// A new id made by control: `exec-<16 hex>`.
    pub fn mint() -> Result<Self, SdkRefusal> {
        let bytes = layerfs_home::helpers::random()?;
        let id = u64::from_be_bytes(bytes[..8].try_into().expect("eight bytes"));
        Ok(Self(format!("exec-{id:016x}")))
    }

    /// Its text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExecId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

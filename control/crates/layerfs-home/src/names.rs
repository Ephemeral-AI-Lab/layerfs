//! Names of machines, Stores, buckets, projects, Sandboxes and Workspaces
//! (S §2.3, #536 C3): 1 to
//! 63 lowercase letters, digits, `_`, `.` and `-`, starting and ending with a
//! letter or digit, never a Windows device name (each name is a folder in
//! the home). A project is always `<store>/<project>`. Each kind of name
//! is a type checked once by its `parse` (#536 §9.2).
use crate::outcome::refused;
use layerfs_bridge::control::SdkRefusal;

/// Windows device names, refused everywhere.
const DEVICES: [&str; 22] = [
    "con", "nul", "aux", "prn", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The built-in machines' names, which `machine create` and `connect` and
/// `sandbox create` refuse as new names. `docker` names the docker machine
/// (its worker container), not a Sandbox backend: since D-8
/// (layerfs-project#111) its Sandboxes run in namespaces, and the name stays
/// so existing homes keep their machine record.
pub const BUILT_IN: [&str; 2] = ["native", "docker"];

/// The prefix of every name LayerFS gives outside its records, for the
/// home with id `home_id`: `layerfs-<home id16>-` before a machine's VM,
/// container and volume names and a Sandbox's run folder, so two homes on
/// one host never share one (owner, 2026-11-15, layerfs-project#180).
/// Since layerfs-project#752 (c2) it uses 16 hex digits, not 8: an 8-hex
/// prefix leaves only 32 bits, so two homes can collide.
pub fn provider_prefix(home_id: &str) -> String {
    let cut = home_id.get(..16).unwrap_or(home_id);
    format!("layerfs-{cut}-")
}

/// The prefix this build wrote before c2 (layerfs-project#752): the same
/// `layerfs-` form with the home id's first 8 hex digits. A recorded name
/// made then still reads; new names use [`provider_prefix`].
pub fn legacy_prefix(home_id: &str) -> String {
    let cut = home_id.get(..8).unwrap_or(home_id);
    format!("layerfs-{cut}-")
}

/// The home id prefix a provider name keeps: the hex run in its
/// `layerfs-<hex>-` or `lfs-<hex>-` form, 16 digits for a name this build
/// gives and 8 for one made before c2. `None` when the name is not one of
/// those forms. The inverse of [`provider_prefix`] and [`legacy_prefix`],
/// for a record whose home id is what a repair ([`super::layout`] `adopt`)
/// reads: a record keeps only the prefix, never the whole id
/// (layerfs-project#752, c6).
pub fn provider_prefix_hex(recorded: &str) -> Option<&str> {
    if let Some(rest) = recorded.strip_prefix("layerfs-") {
        if let Some(hex) = rest.get(..16) {
            if hex.bytes().all(|b| b.is_ascii_hexdigit()) && rest.as_bytes().get(16) == Some(&b'-')
            {
                return Some(hex);
            }
        }
        let hex = rest.get(..8)?;
        (hex.bytes().all(|b| b.is_ascii_hexdigit()) && rest.as_bytes().get(8) == Some(&b'-'))
            .then_some(hex)
    } else {
        let rest = recorded.strip_prefix("lfs-")?;
        let hex = rest.get(..8)?;
        (hex.bytes().all(|b| b.is_ascii_hexdigit()) && rest.as_bytes().get(8) == Some(&b'-'))
            .then_some(hex)
    }
}

/// Whether `recorded` is a name this home's providers give: its current
/// `layerfs-<id16>-` form, the earlier `layerfs-<id8>-` form, or the oldest
/// `lfs-<id8>-` form. Only the prefix is compared; its tail is the machine's
/// or Sandbox's name, which the caller knows. The eight-hex forms pin only
/// 32 bits (layerfs-project#752's documented cap), so a name made from an id
/// that shares the first eight hex digits matches too. A 16-hex name from
/// another id does not.
pub fn matches_provider_name(home_id: &str, recorded: &str) -> bool {
    let id8 = home_id.get(..8).unwrap_or(home_id);
    recorded.starts_with(&provider_prefix(home_id))
        || recorded.starts_with(&legacy_prefix(home_id))
        || recorded.starts_with(&format!("lfs-{id8}-"))
}

/// `name` as one machine, Store, bucket, project or Sandbox name, or why not.
pub fn name(what: &str, name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let valid = (1..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| edge(b) || matches!(b, b'_' | b'.' | b'-'))
        && bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge);
    if !valid {
        return Err(format!(
            "{what} name `{name}` must be 1-63 lowercase letters, digits, _ . -, starting and ending with a letter or digit"
        ));
    }
    if DEVICES.contains(&name) {
        return Err(format!("{what} name `{name}` is reserved"));
    }
    Ok(())
}

/// `<store>/<project>` split into its two names, or why not. A bare project
/// name is refused even when one Store holds it (H §7.5).
pub fn project(text: &str) -> Result<(&str, &str), String> {
    let Some((store, project)) = text.split_once('/') else {
        return Err(format!(
            "project `{text}` must be written with its Store, as <store>/<project>"
        ));
    };
    name("Store", store)?;
    name("project", project)?;
    Ok((store, project))
}

/// One kind of checked name: `parse` refuses `InvalidInput` with [`name`]'s
/// reason; `listed` takes a folder name the home already holds as it is
/// (each was checked by the call that made it), so a list shows every
/// folder as before.
macro_rules! checked {
    ($($(#[$doc:meta])* $type:ident = $what:literal;)*) => {$(
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type(String);

        impl $type {
            /// `text` as this name, checked once.
            pub fn parse(text: &str) -> Result<Self, SdkRefusal> {
                name($what, text).map_err(|why| refused("InvalidInput", why))?;
                Ok(Self(text.to_owned()))
            }

            /// A folder name the home holds, listed as it is.
            pub fn listed(text: String) -> Self {
                Self(text)
            }

            /// The name as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::ops::Deref for $type {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}

checked! {
    /// A machine's name (`machine/<name>/`).
    MachineName = "machine";
    /// A Store's name (`store/<name>/`).
    StoreName = "store";
    /// A project's name within its Store.
    ProjectName = "project";
    /// A Sandbox's name, unique across the home.
    SandboxName = "sandbox";
    /// A connected bucket's name (`backup/<name>/`).
    BucketName = "bucket";
    /// A Workspace's name, unique within its Sandbox (#536 C3).
    WorkspaceName = "workspace";
}

/// A project as every call names it, `<store>/<project>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectRef {
    /// The Store that holds it.
    pub store: StoreName,
    /// Its name in that Store.
    pub project: ProjectName,
}

impl ProjectRef {
    /// `text` as `<store>/<project>`, checked once ([`project`]'s refusals).
    pub fn parse(text: &str) -> Result<Self, SdkRefusal> {
        let (store, name) = project(text).map_err(|why| refused("InvalidInput", why))?;
        Ok(Self {
            store: StoreName(store.to_owned()),
            project: ProjectName(name.to_owned()),
        })
    }

    /// The project `project` of Store `store`.
    pub fn new(store: StoreName, project: ProjectName) -> Self {
        Self { store, project }
    }
}

impl std::fmt::Display for ProjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.store, self.project)
    }
}

/// A Workspace as the `workspace` calls name it: `<sandbox>:<path>` (its
/// absolute path in that Sandbox) or `<sandbox>:<name>` (the name its
/// mount gave it, #536 C3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRef {
    /// The Sandbox it is mounted in.
    pub sandbox: String,
    /// Its path or its name there.
    pub at: WorkspaceAt,
}

/// Where a [`WorkspaceRef`] points in its Sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceAt {
    /// Its absolute path.
    Path(String),
    /// Its name.
    Name(WorkspaceName),
}

impl WorkspaceAt {
    /// The path or the name as text: what the Sandbox's side resolves.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Name(name) => name,
        }
    }
}

impl WorkspaceRef {
    /// `text` as `<sandbox>:<path>` or `<sandbox>:<name>` (the name
    /// checked); the Sandbox's name is checked where the Sandbox is looked
    /// up, as before.
    pub fn parse(text: &str) -> Result<Self, SdkRefusal> {
        let wrong = || {
            refused(
                "InvalidInput",
                format!("a Workspace is <sandbox>:<name>, not {text:?}"),
            )
        };
        let (sandbox, rest) = text
            .split_once(':')
            .filter(|(s, p)| !s.is_empty() && !p.is_empty())
            .ok_or_else(wrong)?;
        let at = match rest.starts_with('/') {
            true => WorkspaceAt::Path(rest.to_owned()),
            false => WorkspaceAt::Name(WorkspaceName::parse(rest)?),
        };
        Ok(Self {
            sandbox: sandbox.to_owned(),
            at,
        })
    }
}

impl std::fmt::Display for WorkspaceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.sandbox, self.at.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_prefix_uses_sixteen_hex_not_eight() {
        let a = "0123456789abcdef0123456789abcdef";
        let b = "01234567fedcba980123456789abcdef";
        assert_eq!(provider_prefix(a), "layerfs-0123456789abcdef-");
        assert_eq!(legacy_prefix(a), "layerfs-01234567-");
        assert_ne!(provider_prefix(a), provider_prefix(b));
        // the earlier 8-hex names collided for these two homes
        assert_eq!(legacy_prefix(a), legacy_prefix(b));
    }

    #[test]
    fn a_short_home_id_does_not_panic() {
        assert_eq!(provider_prefix("abc"), "layerfs-abc-");
        assert_eq!(legacy_prefix("abc"), "layerfs-abc-");
    }

    #[test]
    fn matches_provider_name_accepts_the_three_eras() {
        let id = "0123456789abcdef0123456789abcdef";
        assert!(matches_provider_name(id, "layerfs-0123456789abcdef-docker"));
        assert!(matches_provider_name(id, "layerfs-01234567-docker"));
        assert!(matches_provider_name(id, "lfs-01234567-live"));
        assert!(!matches_provider_name(
            id,
            "layerfs-fedcba9876543210-docker"
        ));
        assert!(!matches_provider_name(id, "layerfs-fedcba98-docker"));
        assert!(!matches_provider_name(id, "lfs-fedcba98-live"));
        assert!(!matches_provider_name(id, "docker"));
        assert!(!matches_provider_name(id, "layerfs-01234567"));
    }

    #[test]
    fn matches_provider_name_pins_only_32_bits_for_eight_hex_names() {
        let home = "0123456789abcdef0123456789abcdef";
        let collides = "01234567fedcba980123456789abcdef";
        assert!(matches_provider_name(home, "layerfs-01234567-live"));
        assert!(matches_provider_name(collides, "layerfs-01234567-live"));
        assert!(matches_provider_name(collides, "lfs-01234567-live"));
        assert!(!matches_provider_name(
            collides,
            "layerfs-0123456789abcdef-live"
        ));
    }

    #[test]
    fn provider_prefix_hex_reads_the_three_eras_and_nothing_else() {
        assert_eq!(
            provider_prefix_hex("layerfs-0123456789abcdef-docker"),
            Some("0123456789abcdef")
        );
        assert_eq!(
            provider_prefix_hex("layerfs-01234567-docker"),
            Some("01234567")
        );
        assert_eq!(provider_prefix_hex("lfs-01234567-live"), Some("01234567"));
        // the 16-hex form is the longest, and a name needs its tail after a `-`
        assert_eq!(provider_prefix_hex("layerfs-0123456789abcdef"), None);
        assert_eq!(provider_prefix_hex("layerfs-01234567"), None);
        assert_eq!(
            provider_prefix_hex("layerfs-fedcba9876543210-live"),
            Some("fedcba9876543210")
        );
        assert_eq!(provider_prefix_hex("docker"), None);
        assert_eq!(provider_prefix_hex("layerfs-nothex-docker"), None);
        assert_eq!(provider_prefix_hex("lfs-0123456-live"), None);
    }
}

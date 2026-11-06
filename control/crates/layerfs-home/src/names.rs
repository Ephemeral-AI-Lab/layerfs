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
                format!("a Workspace is <sandbox>:<path> or <sandbox>:<name>, not {text:?}"),
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

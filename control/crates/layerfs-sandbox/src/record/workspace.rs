//! A Workspace's record in its Sandbox's folder, `workspace/<id>.json`
//! (#536 C3; typed by layerfs-project#108): `{ v, id, name?, branch, path,
//! namespace, created }`, written once after its mount succeeded and
//! removed by its unmount. As the Sandbox record does, it keeps unknown,
//! null and mistyped fields as read, in file order.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::fields::{self, Fields};
use layerfs_home::home::{read_record, write_entity};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The fields a Workspace record knows.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    V(i64),
    Id(String),
    Name(String),
    Branch(String),
    Path(String),
    Namespace(i64),
    Created(i64),
}

impl fields::Field for Field {
    fn decode(name: &str, value: &SdkValue) -> Option<Self> {
        let text = || fields::text(value);
        let int = || fields::int(value);
        Some(match name {
            "v" => Self::V(int()?),
            "id" => Self::Id(text()?),
            "name" => Self::Name(text()?),
            "branch" => Self::Branch(text()?),
            "path" => Self::Path(text()?),
            "namespace" => Self::Namespace(int()?),
            "created" => Self::Created(int()?),
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::V(_) => "v",
            Self::Id(_) => "id",
            Self::Name(_) => "name",
            Self::Branch(_) => "branch",
            Self::Path(_) => "path",
            Self::Namespace(_) => "namespace",
            Self::Created(_) => "created",
        }
    }

    fn encode(&self) -> SdkValue {
        match self {
            Self::V(n) | Self::Namespace(n) | Self::Created(n) => SdkValue::Int(*n),
            Self::Id(text) | Self::Name(text) | Self::Branch(text) | Self::Path(text) => {
                SdkValue::text(text)
            }
        }
    }
}

/// One Workspace's record.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceRecord {
    fields: Fields<Field>,
}

impl WorkspaceRecord {
    /// The record at `path`, if there is one.
    pub fn read(path: &Path) -> Result<Option<Self>> {
        Ok(read_record(path)?.map(|value| Self {
            fields: Fields::decode(&value),
        }))
    }

    /// The record of a Workspace just mounted: its id (64 hex digits), its
    /// `name` if it has one, its Branch, kernel path, namespace and now.
    pub fn mounted(
        id: String,
        name: Option<&str>,
        branch: &str,
        path: &str,
        namespace: i64,
    ) -> Self {
        let mut known = vec![Field::Id(id)];
        known.extend(name.map(|name| Field::Name(name.to_owned())));
        known.extend([
            Field::Branch(branch.to_owned()),
            Field::Path(path.to_owned()),
            Field::Namespace(namespace),
            Field::Created(layerfs_home::home::now_ms()),
        ]);
        Self {
            fields: Fields::of(known),
        }
    }

    /// Writes the record at `path`, with its version.
    pub fn write(&self, path: &Path) -> Result<()> {
        write_entity(path, &self.fields.encode())
    }

    fn text(&self, pick: fn(&Field) -> Option<&String>) -> Option<&str> {
        self.fields.get(pick).map(String::as_str)
    }

    /// Its id, 64 hex digits.
    pub fn id(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Id(text) => Some(text),
            _ => None,
        })
    }

    /// The name its mount gave it, if any.
    pub fn name(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Name(text) => Some(text),
            _ => None,
        })
    }

    /// Its Branch.
    pub fn branch(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Branch(text) => Some(text),
            _ => None,
        })
    }

    /// Its kernel path.
    pub fn path(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Path(text) => Some(text),
            _ => None,
        })
    }

    /// Its daemon namespace.
    pub fn namespace(&self) -> Option<i64> {
        self.fields.get(|f| match f {
            Field::Namespace(n) => Some(*n),
            _ => None,
        })
    }
}

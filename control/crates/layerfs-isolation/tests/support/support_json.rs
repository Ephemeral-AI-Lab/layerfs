//! `control/SUPPORT.json` read back as rows, for the conformance kits: this
//! crate's `conformance` test and `layerfs-platform`'s, which includes this
//! file. It reads the committed file with the crate's own JSON reader, so a
//! kit checks what callers and docs read, not only `support.rs`.
#![allow(dead_code)]
use layerfs_isolation::json::Json;
use layerfs_isolation::RuntimeError;
use std::io::Read;
use std::path::PathBuf;

/// One JSON value; objects keep their key order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The field `key` of an object; panics naming it when absent.
    pub fn get(&self, key: &str) -> &Value {
        let Value::Object(fields) = self else {
            panic!("{key}: not in a JSON object: {self:?}");
        };
        (fields.iter().find(|(name, _)| name == key))
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("no field {key} in {self:?}"))
    }

    pub fn text(&self) -> &str {
        match self {
            Value::Text(text) => text,
            other => panic!("not a JSON string: {other:?}"),
        }
    }

    /// A string or `null`.
    pub fn maybe(&self) -> Option<&str> {
        match self {
            Value::Null => None,
            other => Some(other.text()),
        }
    }

    pub fn list(&self) -> &[Value] {
        match self {
            Value::List(items) => items,
            other => panic!("not a JSON array: {other:?}"),
        }
    }

    pub fn boolean(&self) -> bool {
        match self {
            Value::Bool(value) => *value,
            other => panic!("not a JSON boolean: {other:?}"),
        }
    }
}

/// One platform on one host, as `SUPPORT.json` writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub host: String,
    pub platform: String,
    pub how: String,
    pub backend: Option<String>,
    pub kernel: Option<String>,
    pub sqlite: Option<String>,
    pub postgresql_store_calls: String,
    pub postgresql_sandbox: bool,
    pub postgresql_reason: Option<String>,
    /// `(name, kind)` in order.
    pub options: Vec<(String, String)>,
    pub description: String,
}

/// The committed file: `control/SUPPORT.json`, two folders above either
/// control crate.
pub fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../SUPPORT.json")
}

/// Every row of the committed file, host by host in file order.
pub fn rows() -> Vec<Row> {
    let text = std::fs::read_to_string(path()).unwrap();
    let mut json = Json::new(text.as_bytes());
    let document = value(&mut json, 0).unwrap();
    json.finish().unwrap();
    let mut rows = Vec::new();
    for host in document.get("hosts").list() {
        let name = host.get("host").text();
        for platform in host.get("platforms").list() {
            let postgresql = platform.get("postgresql");
            rows.push(Row {
                host: name.to_owned(),
                platform: platform.get("platform").text().to_owned(),
                how: platform.get("how").text().to_owned(),
                backend: platform.get("backend").maybe().map(str::to_owned),
                kernel: platform.get("kernel").maybe().map(str::to_owned),
                sqlite: platform.get("sqlite").maybe().map(str::to_owned),
                postgresql_store_calls: postgresql.get("store_calls").text().to_owned(),
                postgresql_sandbox: postgresql.get("sandbox").boolean(),
                postgresql_reason: postgresql.get("reason").maybe().map(str::to_owned),
                options: (platform.get("options").list().iter())
                    .map(|o| {
                        (
                            o.get("name").text().to_owned(),
                            o.get("kind").text().to_owned(),
                        )
                    })
                    .collect(),
                description: platform.get("description").text().to_owned(),
            });
        }
    }
    rows
}

fn value<R: Read>(json: &mut Json<R>, depth: u8) -> Result<Value, RuntimeError> {
    if depth > 16 {
        return Err(RuntimeError::Protocol("SUPPORT.json depth"));
    }
    json.whitespace()?;
    match json.peek()? {
        Some(b'{') => {
            let mut fields = Vec::new();
            json.object(|json, key| {
                assert!(!key.overflow, "a SUPPORT.json key over 128 bytes");
                let key = key.text().to_owned();
                fields.push((key, value(json, depth + 1)?));
                Ok(())
            })?;
            Ok(Value::Object(fields))
        }
        Some(b'[') => {
            let mut items = Vec::new();
            json.array(|json| {
                items.push(value(json, depth + 1)?);
                Ok(())
            })?;
            Ok(Value::List(items))
        }
        Some(b'"') => {
            let mut out = Vec::new();
            json.text_into(&mut out, 4096)?;
            Ok(Value::Text(String::from_utf8(out).expect("UTF-8")))
        }
        Some(b't' | b'f') => json.boolean().map(Value::Bool),
        Some(b'n') => json.null().map(|_| Value::Null),
        _ => json.integer().map(Value::Int),
    }
}

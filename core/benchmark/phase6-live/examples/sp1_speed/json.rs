//! Small benchmark receipt writer; no new dependency or product serialization.
use std::collections::BTreeMap;
#[derive(Clone)]
pub enum Json {
    Null,
    Bool(bool),
    Num(u64),
    Str(String),
    Array(Vec<Json>),
    Map(BTreeMap<String, Json>),
}
impl From<u64> for Json {
    fn from(v: u64) -> Self {
        Self::Num(v)
    }
}
impl From<usize> for Json {
    fn from(v: usize) -> Self {
        Self::Num(v as u64)
    }
}
impl From<&str> for Json {
    fn from(v: &str) -> Self {
        Self::Str(v.into())
    }
}
impl From<String> for Json {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}
impl From<bool> for Json {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
pub fn object(items: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                use std::fmt::Write;
                write!(&mut out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
impl Json {
    pub fn encode(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(v) => v.to_string(),
            Self::Num(v) => v.to_string(),
            Self::Str(v) => quote(v),
            Self::Array(v) => format!(
                "[{}]",
                v.iter().map(Self::encode).collect::<Vec<_>>().join(",")
            ),
            Self::Map(v) => format!(
                "{{{}}}",
                v.iter()
                    .map(|(k, v)| format!("{}:{}", quote(k), v.encode()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
    pub fn put(&mut self, key: &str, value: impl Into<Json>) {
        if let Self::Map(m) = self {
            m.insert(key.into(), value.into());
        }
    }
}

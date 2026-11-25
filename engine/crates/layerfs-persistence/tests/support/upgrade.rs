//! Shared pieces of the `store upgrade` tests (#804): the frozen fixtures
//! under `fixtures/store-formats/`, their manifests, file hashes and a small
//! JSON reader (the workspace carries no JSON crate).
#![allow(dead_code)]
use sha2::Digest;
use std::path::{Path, PathBuf};

/// `fixtures/store-formats/` at the repository root.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/store-formats")
}

/// Lowercase hex sha256 of a file's bytes.
pub fn sha256_file(path: &Path) -> String {
    hex(&sha2::Sha256::digest(
        std::fs::read(path).expect("read file"),
    ))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

pub fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Copies the SQLite v2 fixture into `dir` as `store.sqlite`.
pub fn sqlite_v2_copy(dir: &Path) -> PathBuf {
    let path = dir.join("store.sqlite");
    std::fs::copy(fixtures().join("sqlite/v2/store.sqlite"), &path).expect("copy fixture");
    path
}

/// The sha256 of the Store file and its sidecars, if any.
pub fn store_state(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let file = PathBuf::from(name);
        if file.exists() {
            out.push((suffix.to_owned(), sha256_file(&file)));
        }
    }
    out
}

/// One parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Json {
        let bytes = text.as_bytes();
        let mut at = 0;
        let value = value(bytes, &mut at);
        skip(bytes, &mut at);
        assert_eq!(at, bytes.len(), "trailing JSON text");
        value
    }
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("not a string: {other:?}"),
        }
    }
    pub fn items(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(items) => items,
            other => panic!("not an object: {other:?}"),
        }
    }
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            other => panic!("not an array: {other:?}"),
        }
    }
}

fn skip(b: &[u8], at: &mut usize) {
    while *at < b.len() && b[*at].is_ascii_whitespace() {
        *at += 1;
    }
}

fn value(b: &[u8], at: &mut usize) -> Json {
    skip(b, at);
    match b[*at] {
        b'{' => {
            *at += 1;
            let mut items = Vec::new();
            loop {
                skip(b, at);
                if b[*at] == b'}' {
                    *at += 1;
                    return Json::Obj(items);
                }
                let Json::Str(key) = value(b, at) else {
                    panic!("object key")
                };
                skip(b, at);
                assert_eq!(b[*at], b':');
                *at += 1;
                items.push((key, value(b, at)));
                skip(b, at);
                if b[*at] == b',' {
                    *at += 1;
                }
            }
        }
        b'[' => {
            *at += 1;
            let mut items = Vec::new();
            loop {
                skip(b, at);
                if b[*at] == b']' {
                    *at += 1;
                    return Json::Arr(items);
                }
                items.push(value(b, at));
                skip(b, at);
                if b[*at] == b',' {
                    *at += 1;
                }
            }
        }
        b'"' => {
            *at += 1;
            let mut out = String::new();
            loop {
                match b[*at] {
                    b'"' => {
                        *at += 1;
                        return Json::Str(out);
                    }
                    b'\\' => {
                        *at += 1;
                        let c = match b[*at] {
                            b'n' => '\n',
                            b't' => '\t',
                            b'r' => '\r',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'u' => {
                                let code = u32::from_str_radix(
                                    std::str::from_utf8(&b[*at + 1..*at + 5]).unwrap(),
                                    16,
                                )
                                .unwrap();
                                *at += 4;
                                char::from_u32(code).unwrap_or('\u{fffd}')
                            }
                            other => other as char,
                        };
                        out.push(c);
                        *at += 1;
                    }
                    _ => {
                        let start = *at;
                        while b[*at] != b'"' && b[*at] != b'\\' {
                            *at += 1;
                        }
                        out.push_str(std::str::from_utf8(&b[start..*at]).unwrap());
                    }
                }
            }
        }
        b't' => {
            *at += 4;
            Json::Bool(true)
        }
        b'f' => {
            *at += 5;
            Json::Bool(false)
        }
        b'n' => {
            *at += 4;
            Json::Null
        }
        _ => {
            let start = *at;
            while *at < b.len() && matches!(b[*at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
            {
                *at += 1;
            }
            Json::Num(
                std::str::from_utf8(&b[start..*at])
                    .unwrap()
                    .parse()
                    .unwrap(),
            )
        }
    }
}

/// One fixture manifest's history: `(project, branch, head, commits)`.
pub fn manifest_branches(manifest: &Path) -> Vec<(String, String, String, Vec<String>)> {
    let json = Json::parse(&std::fs::read_to_string(manifest).expect("manifest"));
    let mut out = Vec::new();
    for (project, body) in json.get("projects").expect("projects").items() {
        for (branch, data) in body.get("branches").expect("branches").items() {
            let commits = data
                .get("commits")
                .expect("commits")
                .arr()
                .iter()
                .map(|c| c.str().to_owned())
                .collect();
            out.push((
                project.clone(),
                branch.clone(),
                data.get("head").expect("head").str().to_owned(),
                commits,
            ));
        }
    }
    out
}

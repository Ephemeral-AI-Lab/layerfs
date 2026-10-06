//! Fresh append-only streams, bounded record windows and streaming hashes.
use super::{
    digest::{hex, Sha256},
    json::{Json, WINDOW},
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub struct Stream {
    file: File,
    pub name: &'static str,
    hash: Sha256,
    pub bytes: u64,
    pub records: u64,
}
impl Stream {
    pub fn open(root: &Path, name: &'static str) -> io::Result<Self> {
        Ok(Self {
            file: OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(name))?,
            name,
            hash: Sha256::new(),
            bytes: 0,
            records: 0,
        })
    }
    pub fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > WINDOW {
            return Err(io::Error::other("E2 append window exceeded"));
        }
        // Positive writes update the acknowledged prefix before any later error.
        let mut offset = 0;
        while offset < bytes.len() {
            let written = self.file.write(&bytes[offset..])?;
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "E2 original append",
                ));
            }
            self.hash.update(&bytes[offset..offset + written]);
            self.bytes += written as u64;
            offset += written;
        }
        self.records += 1;
        Ok(())
    }
    pub fn descriptor(&self, out: &mut Json, status: &str) -> io::Result<()> {
        out.raw("{")?;
        out.text("path", self.name)?;
        out.raw(",")?;
        out.field("bytes", self.bytes)?;
        out.raw(",")?;
        out.field("records", self.records)?;
        out.raw(",")?;
        out.text("sha256", &hex(&self.hash.clone().finish()))?;
        out.raw(",")?;
        out.text("status", status)?;
        out.raw("}")
    }
}
pub struct Recorder {
    pub root: PathBuf,
    pub identity: String,
    pub identity_sha256: String,
    pub identity_bytes: usize,
    /// External identities allocated before the original public startup call.
    pub startup_record_id: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub binary_sha256: String,
    /// Canonical process-local inputs frozen before the original startup call.
    pub invocation: [PathBuf; 4],
    pub identity_source_sha256: String,
    pub identity_source_bytes: usize,
    pub startup: Stream,
    pub jobs: Stream,
    pub probes: Stream,
    pub stdout: Stream,
    pub stderr: Stream,
}
/// Remove only JSON whitespace outside strings, so pretty input cannot split
/// an original JSONL record. Full schema/duplicate-key validation is independent.
pub fn compact_identity(input: &str) -> io::Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut quoted = false;
    let mut escaped = false;
    for value in input.chars() {
        if quoted {
            if value < ' ' {
                return Err(io::Error::other("raw control in supplied JSON string"));
            }
            output.push(value);
            if escaped {
                escaped = false;
            } else if value == '\\' {
                escaped = true;
            } else if value == '"' {
                quoted = false;
            }
        } else if value == '"' {
            quoted = true;
            output.push(value);
        } else if !matches!(value, ' ' | '\t' | '\r' | '\n') {
            output.push(value);
        }
    }
    if quoted || escaped {
        return Err(io::Error::other("unterminated supplied JSON string"));
    }
    Ok(output)
}
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0; 8192];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        hash.update(&block[..n]);
    }
    Ok(hex(&hash.finish()))
}
impl Recorder {
    pub fn create(db: &Path, root: &Path, identity_path: &Path) -> io::Result<Self> {
        fn future_path(path: &Path) -> io::Result<PathBuf> {
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("owned input parent absent"))?
                .canonicalize()?;
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("owned input filename absent"))?;
            Ok(parent.join(name))
        }
        let invocation = [
            std::env::current_exe()?.canonicalize()?,
            future_path(db)?,
            future_path(root)?,
            identity_path.canonicalize()?,
        ];
        let mut bytes = Vec::new();
        File::open(identity_path)?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err(io::Error::other("E2 identity byte window"));
        }
        let identity_source_sha256 = hex(&super::digest::sha256(&bytes));
        let identity_source_bytes = bytes.len();
        let original_identity = std::str::from_utf8(&bytes)
            .map_err(io::Error::other)?
            .trim()
            .to_owned();
        let identity = compact_identity(&original_identity)?;
        if !identity.starts_with('{') || !identity.ends_with('}') {
            return Err(io::Error::other("E2 identity JSON object required"));
        }
        let identity_sha256 = hex(&super::digest::sha256(original_identity.as_bytes()));
        let identity_bytes = original_identity.len();
        let mint = |role: &str| {
            hex(&super::digest::sha256(
                format!("{identity_sha256}:{role}:0").as_bytes(),
            ))
        };
        let startup_record_id = mint("startup");
        let attempt_id = mint("attempt");
        let owner_id = mint("owner");
        let binary_sha256 = hash_file(&invocation[0])?;
        std::fs::create_dir(root)?;
        let mut identity_copy = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("identity.json"))?;
        identity_copy.write_all(original_identity.as_bytes())?;
        let startup = Stream::open(root, "startup.jsonl")?;
        let jobs = Stream::open(root, "jobs.jsonl")?;
        let probes = Stream::open(root, "probes.jsonl")?;
        let stdout = Stream::open(root, "stdout.txt")?;
        let stderr = Stream::open(root, "stderr.txt")?;
        Ok(Self {
            root: root.to_owned(),
            identity,
            identity_sha256,
            identity_bytes,
            startup_record_id,
            attempt_id,
            owner_id,
            binary_sha256,
            invocation,
            identity_source_sha256,
            identity_source_bytes,
            startup,
            jobs,
            probes,
            stdout,
            stderr,
        })
    }
    pub fn header(&self, out: &mut Json, kind: &str, index: u64) -> io::Result<()> {
        out.raw("{\"schema\":\"cluster-two-job-receipts-v1\",\"case\":\"E01-startup\",\"mode\":\"diagnostic\",\"sample_count\":0,\"admission_eligible\":false,\"qualification_status\":\"NOT_EVALUATED\",\"global_persistence\":\"NOT_IN_SCOPE\",")?;
        out.text("kind", kind)?;
        out.raw(",")?;
        out.field("record_index", index)?;
        out.raw(",")?;
        let record_id = if kind == "startup" && index == 0 {
            self.startup_record_id.clone()
        } else {
            hex(&super::digest::sha256(
                format!("{}:{kind}:{index}", self.identity_sha256).as_bytes(),
            ))
        };
        out.text("record_id", &record_id)?;
        out.raw(",")?;
        out.text("attempt_id", &self.attempt_id)?;
        out.raw(",")?;
        out.text("owner_id", &self.owner_id)?;
        out.raw(",\"identity\":")?;
        out.raw(&self.identity)?;
        out.raw(",")?;
        out.text("identity_sha256", &self.identity_sha256)?;
        out.raw(",")?;
        out.text("actual_binary_sha256", &self.binary_sha256)?;
        out.raw(",\"source_binding\":\"base-commit-plus-current-inventory\",")?;
        out.text("build_target_os", std::env::consts::OS)?;
        out.raw(",")?;
        out.text("build_target_architecture", std::env::consts::ARCH)?;
        out.raw(",\"invocation\":{\"schema\":\"cluster-two-e2-invocation-v1\",\"path_scope\":\"process-local-canonical-inputs\",\"bound_before_startup\":true,\"argv\":[")?;
        for (index, path) in self.invocation.iter().enumerate() {
            if index != 0 {
                out.raw(",")?;
            }
            out.string(
                path.to_str()
                    .ok_or_else(|| io::Error::other("invocation input UTF-8"))?,
            )?;
        }
        out.raw("],")?;
        out.text("identity_source_sha256", &self.identity_source_sha256)?;
        out.raw(",")?;
        out.field("identity_source_bytes", self.identity_source_bytes)?;
        out.raw(",")?;
        out.text("identity_copy_sha256", &self.identity_sha256)?;
        out.raw(",\"docker_path_mapping\":{\"status\":\"UNAVAILABLE\",\"reason\":\"no-verified-host-container-path-mapping\"}}")
    }
    pub fn write_once(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(name))?;
        file.write_all(bytes)
    }
}

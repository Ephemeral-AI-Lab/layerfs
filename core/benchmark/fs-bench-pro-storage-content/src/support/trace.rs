//! `layerfs-trace-v1`: a flat JSONL record writer.
//!
//! **Why the resource axes live here and not in the product's tree.** The frozen
//! product telemetry writer cannot carry a sibling `resources` key
//! (`timer/json.rs:29-89`): it emits `name`, `elapsed_ns`, optional `outcome`,
//! optional `incomplete` and `children`, and nothing else. So the time axis is
//! emitted by the product, unmodified, into `timing.json`, and every resource
//! reading is written here as a **flat** record keyed to the product receipt by
//! monotonic clock rather than by tree nesting.
//!
//! Flat means flat: one JSON object per line, every value a scalar, no nested
//! object anywhere. A reader can therefore parse this file with a line splitter,
//! which is what makes the Python side able to re-derive every published figure
//! independently of the process that produced it.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Schema tag of every record this writer emits.
pub const SCHEMA: &str = "layerfs-trace-v1";

/// What one record is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Run identity: source commit, seals, case, lane, worker count.
    Run,
    /// A timed window: label, parent, open and close stamps.
    Window,
    /// A work counter reported by the product or by the oracle.
    Counter,
    /// A resource reading from one named instrument.
    Resource,
    /// A gate outcome.
    Gate,
    /// An oracle outcome.
    Oracle,
    /// A free-form receipt field the runner publishes.
    Receipt,
}

impl Kind {
    fn token(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Window => "window",
            Self::Counter => "counter",
            Self::Resource => "resource",
            Self::Gate => "gate",
            Self::Oracle => "oracle",
            Self::Receipt => "receipt",
        }
    }
}

/// Append-only JSONL writer.
pub struct TraceWriter {
    path: PathBuf,
    handle: std::fs::File,
    sequence: u64,
    bytes: u64,
}

impl TraceWriter {
    /// Creates a trace file. It refuses to overwrite one: a rerun that overwrites
    /// the evidence destroys the only witness there is.
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} already exists; traces are append-only", path.display()),
            ));
        }
        let handle = std::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            handle,
            sequence: 0,
            bytes: 0,
        })
    }

    /// Opens an existing trace and continues its sequence.
    ///
    /// A phase-split row writes its measured phase and its verification phase from
    /// two invocations. `benchmark_rules.md` section 6 requires them to have
    /// separate timing scopes; it does not require them to have separate files, and
    /// one append-only trace per row is what lets the verifier re-derive the row's
    /// status from the same record set the runner published. The sequence counter
    /// continues rather than restarting, because a reader that sees `seq` go
    /// backwards records a structural defect.
    pub fn append(path: &Path) -> std::io::Result<Self> {
        let mut sequence = 0_u64;
        let mut bytes = 0_u64;
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            bytes = text.len() as u64;
            let prefix = format!("{{\"schema\":\"{SCHEMA}\",\"seq\":");
            for line in text.lines() {
                let Some(rest) = line.strip_prefix(prefix.as_str()) else {
                    continue;
                };
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                if let Ok(value) = digits.parse::<u64>() {
                    sequence = sequence.max(value.saturating_add(1));
                }
            }
        }
        let handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            handle,
            sequence,
            bytes,
        })
    }

    /// Path this writer owns.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes written so far.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Writes one flat record.
    pub fn write(
        &mut self,
        kind: Kind,
        key: &str,
        value: &str,
        unit: &str,
        basis: &str,
    ) -> std::io::Result<()> {
        let record = format!(
            "{{\"schema\":\"{SCHEMA}\",\"seq\":{},\"kind\":\"{}\",\"key\":\"{}\",\"value\":\"{}\",\"unit\":\"{}\",\"basis\":\"{}\"}}\n",
            self.sequence,
            kind.token(),
            escape(key),
            escape(value),
            escape(unit),
            escape(basis),
        );
        self.handle.write_all(record.as_bytes())?;
        self.sequence += 1;
        self.bytes += record.len() as u64;
        Ok(())
    }

    /// Writes one record whose value is a number, so a reader needs no string
    /// parsing to re-derive a figure.
    pub fn write_number(
        &mut self,
        kind: Kind,
        key: &str,
        value: i128,
        unit: &str,
        basis: &str,
    ) -> std::io::Result<()> {
        let record = format!(
            "{{\"schema\":\"{SCHEMA}\",\"seq\":{},\"kind\":\"{}\",\"key\":\"{}\",\"value\":{},\"numeric\":true,\"unit\":\"{}\",\"basis\":\"{}\"}}\n",
            self.sequence,
            kind.token(),
            escape(key),
            value,
            escape(unit),
            escape(basis),
        );
        self.handle.write_all(record.as_bytes())?;
        self.sequence += 1;
        self.bytes += record.len() as u64;
        Ok(())
    }

    /// Writes one window record from a bracket.
    pub fn write_window(
        &mut self,
        label: &str,
        parent: i64,
        open_ns: u64,
        close_ns: u64,
    ) -> std::io::Result<()> {
        let record = format!(
            "{{\"schema\":\"{SCHEMA}\",\"seq\":{},\"kind\":\"window\",\"key\":\"{}\",\"value\":{},\"parent\":{},\"open_ns\":{},\"close_ns\":{},\"elapsed_ns\":{},\"unit\":\"ns\",\"basis\":\"clock-monotonic-raw-4\"}}\n",
            self.sequence,
            escape(label),
            close_ns.saturating_sub(open_ns),
            parent,
            open_ns,
            close_ns,
            close_ns.saturating_sub(open_ns),
        );
        self.handle.write_all(record.as_bytes())?;
        self.sequence += 1;
        self.bytes += record.len() as u64;
        Ok(())
    }

    /// Flushes without syncing. There is no `fsync` anywhere in this harness.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.handle.flush()
    }
}

/// One record read back off disk, as the gate layer needs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlatRecord {
    /// Record kind: `counter`, `oracle`, `gate`, ...
    pub kind: String,
    /// Record key.
    pub key: String,
    /// Record value, as text.
    pub value: String,
    /// Whether the writer emitted it as a JSON number.
    pub numeric: bool,
}

/// Reads a flat trace back, so a gate is decided by the published evidence.
///
/// The pinned-constant gates are applied by `main` **after** the driver has
/// published its counters, and they read what was actually written rather than an
/// in-memory side channel: a figure the trace does not carry is a figure no reader
/// can re-derive, and a gate that accepted one would be deciding on a number the
/// evidence does not contain.
pub fn read_flat(path: &Path) -> Result<Vec<FlatRecord>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut records = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let kind = field(line, "kind").ok_or_else(|| format!("line {}: no kind", number + 1))?;
        let key = field(line, "key").ok_or_else(|| format!("line {}: no key", number + 1))?;
        let raw = raw_field(line, "value")
            .ok_or_else(|| format!("line {}: no value", number + 1))?;
        let numeric = !raw.starts_with('"');
        let value = if numeric {
            raw.to_string()
        } else {
            unescape(raw.trim_matches('"'))
        };
        records.push(FlatRecord {
            kind,
            key,
            value,
            numeric,
        });
    }
    Ok(records)
}

/// Extracts one quoted string field from a flat record.
fn field(line: &str, name: &str) -> Option<String> {
    let raw = raw_field(line, name)?;
    raw.strip_prefix('"').map(|rest| {
        unescape(rest.trim_end_matches('"'))
    })
}

/// Extracts one field's raw text, quoted or not.
fn raw_field(line: &str, name: &str) -> Option<String> {
    let needle = format!("\"{name}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    if let Some(body) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut escaped = false;
        for character in body.chars() {
            if escaped {
                out.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => return Some(format!("\"{out}\"")),
                other => out.push(other),
            }
        }
        return None;
    }
    let end = rest.find(',').unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches('}').to_string())
}

/// Reverses [`escape`].
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Escapes the two characters that would break a flat JSON line.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
